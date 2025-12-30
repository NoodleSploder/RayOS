//! Task Orchestrator - Work stealing scheduler with dynamic load balancing
//!
//! Distributes tasks across heterogeneous workers (CPU threads, APU, dGPUs)
//! using work-stealing algorithms.

use crate::entropy_monitor::EntropyMonitor;
use crate::ouroboros::OuroborosEngine;
use crate::types::*;
use anyhow::Result;
use crossbeam_deque::{Injector, Stealer, Worker as DequeWorker};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Work-stealing task orchestrator
pub struct TaskOrchestrator {
    /// Global task injector (shared queue)
    injector: Arc<Injector<Task>>,

    /// Per-worker metadata
    workers: Vec<WorkerContext>,

    /// Task registry (for status tracking)
    tasks: Arc<DashMap<TaskId, Task>>,

    /// Entropy monitor
    monitor: Arc<EntropyMonitor>,

    /// Ouroboros engine (self-optimization)
    ouroboros: Arc<OuroborosEngine>,

    /// Configuration
    config: ConductorConfig,

    /// Statistics
    stats: Arc<OrchestratorStats>,

    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
}

struct WorkerContext {
    id: WorkerId,
    worker_type: WorkerType,
    tasks_completed: Arc<AtomicU64>,
    current_task: Arc<RwLock<Option<TaskId>>>,
    semaphore: Arc<Semaphore>,
}

impl Clone for WorkerContext {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            worker_type: self.worker_type,
            tasks_completed: self.tasks_completed.clone(),
            current_task: self.current_task.clone(),
            semaphore: self.semaphore.clone(),
        }
    }
}

#[derive(Default)]
struct OrchestratorStats {
    total_tasks: AtomicU64,
    completed_tasks: AtomicU64,
    failed_tasks: AtomicU64,
    stolen_tasks: AtomicU64,
}

impl TaskOrchestrator {
    /// Create a new task orchestrator
    pub fn new(
        config: ConductorConfig,
        monitor: Arc<EntropyMonitor>,
        ouroboros: Arc<OuroborosEngine>,
    ) -> Result<Self> {
        log::info!("Initializing Task Orchestrator with {} workers", config.worker_threads);

        let injector = Arc::new(Injector::new());
        let mut workers = Vec::new();

        // Create worker contexts
        for i in 0..config.worker_threads {
            let ctx = WorkerContext {
                id: WorkerId(i),
                worker_type: WorkerType::CpuThread,
                tasks_completed: Arc::new(AtomicU64::new(0)),
                current_task: Arc::new(RwLock::new(None)),
                semaphore: Arc::new(Semaphore::new(1)),
            };

            workers.push(ctx);
        }

        Ok(Self {
            injector,
            workers,
            tasks: Arc::new(DashMap::new()),
            monitor,
            ouroboros,
            config,
            stats: Arc::new(OrchestratorStats::default()),
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Submit a task to the global queue
    pub fn submit(&self, task: Task) -> Result<TaskId> {
        let task_id = task.id;

        // Check queue capacity
        let pending = self.stats.total_tasks.load(Ordering::Relaxed)
                    - self.stats.completed_tasks.load(Ordering::Relaxed)
                    - self.stats.failed_tasks.load(Ordering::Relaxed);

        if pending >= self.config.max_queue_size as u64 {
            anyhow::bail!("Task queue overflow: {} pending tasks", pending);
        }

        // Store in registry
        self.tasks.insert(task_id, task.clone());

        // Add to global injector
        let priority = task.priority;
        self.injector.push(task);
        self.stats.total_tasks.fetch_add(1, Ordering::Relaxed);

        log::debug!("Submitted task {} with priority {:?}", task_id.0, priority);

        Ok(task_id)
    }

    /// Submit multiple tasks
    pub fn submit_batch(&self, tasks: Vec<Task>) -> Result<Vec<TaskId>> {
        tasks.into_iter().map(|t| self.submit(t)).collect()
    }

    /// Get task status
    pub fn get_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        self.tasks.get(&task_id).map(|t| t.status.clone())
    }

    /// Start the orchestrator (spawns worker threads)
    pub async fn start(&self) -> Result<()> {
        log::info!("Starting {} worker threads", self.workers.len());

        // Crossbeam's `Worker<T>` is not `Sync`, which means it can't live inside an `Arc` and be
        // shared across threads. We instead create each local queue here, move it into the worker
        // task, and share only `Stealer<T>` handles.
        let mut local_queues = Vec::with_capacity(self.workers.len());
        let mut stealers = Vec::with_capacity(self.workers.len());
        for _ in 0..self.workers.len() {
            let worker = DequeWorker::new_fifo();
            stealers.push(worker.stealer());
            local_queues.push(worker);
        }
        let stealers = Arc::new(stealers);

        let mut handles = Vec::new();

        for (ctx, local_queue) in self.workers.iter().cloned().zip(local_queues.into_iter()) {
            let handle = self.spawn_worker(ctx, local_queue, stealers.clone()).await?;
            handles.push(handle);
        }

        // Wait for shutdown signal
        while !self.shutdown.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Wait for workers to finish
        for handle in handles {
            handle.await?;
        }

        Ok(())
    }

    /// Spawn a worker thread
    async fn spawn_worker(
        &self,
        ctx: WorkerContext,
        local_queue: DequeWorker<Task>,
        stealers: Arc<Vec<Stealer<Task>>>,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let injector = self.injector.clone();
        let tasks = self.tasks.clone();
        let monitor = self.monitor.clone();
        let ouroboros = self.ouroboros.clone();
        let stats = self.stats.clone();
        let shutdown = self.shutdown.clone();

        let worker_id = ctx.id;
        let tasks_completed = ctx.tasks_completed.clone();
        let current_task = ctx.current_task.clone();
        let semaphore = ctx.semaphore.clone();

        let mut local_queue = local_queue;

        let handle = tokio::spawn(async move {
            log::debug!("Worker {} started", worker_id.0);

            while !shutdown.load(Ordering::Relaxed) {
                // Try to acquire semaphore (for backpressure)
            let _permit = semaphore.clone().acquire_owned().await.unwrap();

                // Find a task to execute (using local queue)
                let task = Self::find_task_static(&local_queue, &injector, stealers.as_slice(), &stats);

                if let Some(task) = task {
                    // Update current task
                    *current_task.write() = Some(task.id);

                    // Mark as running in registry (so external status polling can see progress).
                    if let Some(mut entry) = tasks.get_mut(&task.id) {
                        entry.status = TaskStatus::Running {
                            worker_id,
                            started_at: Instant::now(),
                        };
                    }

                    // Execute the task
                    let start = Instant::now();
                    let result = Self::execute_task(&task, &ouroboros).await;
                    let duration = start.elapsed();

                    // Record metrics
                    monitor.record_task(
                        format!("{:?}", task.payload),
                        duration
                    );

                    // Update task status
                    if let Some(mut entry) = tasks.get_mut(&task.id) {
                        entry.status = match result {
                            Ok(result) => {
                                stats.completed_tasks.fetch_add(1, Ordering::Relaxed);
                                TaskStatus::Completed { duration, result }
                            }
                            Err(e) => {
                                stats.failed_tasks.fetch_add(1, Ordering::Relaxed);
                                TaskStatus::Failed { error: e.to_string() }
                            }
                        };
                    }

                    // Clear current task
                    *current_task.write() = None;
                    tasks_completed.fetch_add(1, Ordering::Relaxed);
                } else {
                    // No work available, sleep briefly
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }

            log::debug!("Worker {} shutting down", worker_id.0);
        });

        Ok(handle)
    }

    /// Find a task using work-stealing algorithm (static version for async context)
    fn find_task_static(
        local_queue: &DequeWorker<Task>,
        injector: &Injector<Task>,
        stealers: &[Stealer<Task>],
        stats: &OrchestratorStats,
    ) -> Option<Task> {
        // 1. Try local queue first
        if let Some(task) = local_queue.pop() {
            return Some(task);
        }

        // 2. Try global injector
        loop {
            match injector.steal_batch_and_pop(local_queue) {
                crossbeam_deque::Steal::Success(task) => return Some(task),
                crossbeam_deque::Steal::Empty => break,
                crossbeam_deque::Steal::Retry => continue,
            }
        }

        // 3. Try stealing from other workers (work stealing!)
        let mut rng = fastrand::Rng::new();
        let start = rng.usize(..stealers.len());

        for i in 0..stealers.len() {
            let idx = (start + i) % stealers.len();
            let stealer = &stealers[idx];

            loop {
                match stealer.steal_batch_and_pop(local_queue) {
                    crossbeam_deque::Steal::Success(task) => {
                        stats.stolen_tasks.fetch_add(1, Ordering::Relaxed);
                        log::trace!("Stole task from worker {}", idx);
                        return Some(task);
                    }
                    crossbeam_deque::Steal::Empty => break,
                    crossbeam_deque::Steal::Retry => continue,
                }
            }
        }

        None
    }

    /// Execute a task
    async fn execute_task(task: &Task, ouroboros: &Arc<OuroborosEngine>) -> Result<Option<String>> {
        match &task.payload {
            TaskPayload::Compute { name, estimated_duration } => {
                log::debug!("Executing compute task: {}", name);
                tokio::time::sleep(*estimated_duration).await;
                Ok(Some("OK".to_string()))
            }

            TaskPayload::IndexFile { path } => {
                log::debug!("Indexing file: {}", path.display());
                // Integration with Volume (Phase 3) - simulated
                // In production: volume.index_file(path).await?

                // Simulate file reading and indexing
                match std::fs::metadata(&path) {
                    Ok(metadata) => {
                        let size = metadata.len();
                        let duration = Duration::from_millis((size / 1000).max(10) as u64);
                        tokio::time::sleep(duration).await;
                        log::info!("Indexed file: {} ({} bytes)", path.display(), size);
                        Ok(Some(format!("Indexed {} bytes", size)))
                    }
                    Err(e) => {
                        log::error!("Failed to index file {}: {}", path.display(), e);
                        Err(anyhow::anyhow!("Index failed: {}", e))
                    }
                }
            }

            TaskPayload::Search { query, limit } => {
                log::debug!("Searching: {} (limit: {})", query, limit);
                let query_owned = query.clone();
                let limit = *limit;
                let search_root = determine_search_root();

                // Do the filesystem scan on the blocking pool so we don't stall the async runtime.
                let matches = tokio::task::spawn_blocking(move || host_path_search(&search_root, &query_owned, limit))
                    .await
                    .map_err(|e| anyhow::anyhow!("Search join failed: {}", e))?;

                let result = format_search_result(&query, &matches);
                log::info!("Search completed: '{}' ({} matches)", query, matches.len());
                Ok(Some(result))
            }

            TaskPayload::Optimize { target } => {
                log::debug!("Optimizing: {:?}", target);
                ouroboros.optimize_cycle(target.clone()).await?;
                let stats = ouroboros.get_statistics();
                log::info!(
                    "Optimization completed: {:?} (mutations={}, successful={}, active_patches={})",
                    target,
                    stats.total_mutations,
                    stats.successful_mutations,
                    stats.active_patches
                );
                Ok(Some(format!(
                    "Optimization complete (mutations={}, successful={}, active_patches={})",
                    stats.total_mutations, stats.successful_mutations, stats.active_patches
                )))
            }

            TaskPayload::Maintenance { task_type } => {
                log::debug!("Maintenance: {:?}", task_type);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(Some(format!("Maintenance {:?} complete", task_type)))
            }
        }
    }

    /// Get current system load snapshot
    pub fn get_system_load(&self) -> SystemLoad {
        let active_tasks = self.stats.total_tasks.load(Ordering::Relaxed)
            - self.stats.completed_tasks.load(Ordering::Relaxed)
            - self.stats.failed_tasks.load(Ordering::Relaxed);

        let pending_tasks = self.injector.len() as u64;

        let metrics = self.monitor.collect_metrics(active_tasks, pending_tasks);

        let workers: Vec<WorkerStatus> = self.workers.iter().map(|ctx| {
            let completed = ctx.tasks_completed.load(Ordering::Relaxed);
            let current = ctx.current_task.read().clone();

            // Calculate actual work time based on completed tasks and average duration
            let avg_task_duration_ms: f64 = if completed > 0 {
                match ctx.worker_type {
                    WorkerType::CpuThread => 50.0,
                    WorkerType::ApuCompute => 30.0,
                    WorkerType::DGpu { .. } => 15.0,
                }
            } else {
                0.0
            };

            let total_work_ms = completed as f64 * avg_task_duration_ms;
            let total_work_time = Duration::from_millis(total_work_ms as u64);

            WorkerStatus {
                id: ctx.id,
                worker_type: ctx.worker_type,
                current_task: current,
                tasks_completed: completed,
                total_work_time,
                load_factor: if current.is_some() { 1.0 } else { 0.0 },
            }
        }).collect();

        let bottleneck = self.monitor.detect_bottleneck(&SystemLoad {
            timestamp: Instant::now(),
            metrics: metrics.clone(),
            workers: workers.clone(),
            bottleneck: None,
        });

        SystemLoad {
            timestamp: Instant::now(),
            metrics,
            workers,
            bottleneck,
        }
    }

    /// Get orchestrator statistics
    pub fn get_stats(&self) -> OrchestratorStatistics {
        OrchestratorStatistics {
            total_tasks: self.stats.total_tasks.load(Ordering::Relaxed),
            completed_tasks: self.stats.completed_tasks.load(Ordering::Relaxed),
            failed_tasks: self.stats.failed_tasks.load(Ordering::Relaxed),
            stolen_tasks: self.stats.stolen_tasks.load(Ordering::Relaxed),
            pending_tasks: (self.stats.total_tasks.load(Ordering::Relaxed)
                          - self.stats.completed_tasks.load(Ordering::Relaxed)
                          - self.stats.failed_tasks.load(Ordering::Relaxed)),
            worker_count: self.workers.len(),
        }
    }

    /// Graceful shutdown
    pub fn shutdown(&self) {
        log::info!("Initiating orchestrator shutdown");
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

fn determine_search_root() -> std::path::PathBuf {
    if let Ok(root) = std::env::var("RAYOS_SEARCH_ROOT") {
        let p = std::path::PathBuf::from(root);
        if p.is_dir() {
            return p;
        }
    }

    // Default behavior (used by scripts like `test-boot-ai.sh`): we run from `.../RayOS/conductor`.
    // Prefer the workspace root so search results feel relevant.
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.file_name().is_some_and(|n| n == "conductor") {
            if let Some(parent) = cwd.parent() {
                return parent.to_path_buf();
            }
        }
        return cwd;
    }

    std::path::PathBuf::from(".")
}

#[derive(Debug, Clone)]
struct SearchMatch {
    path: String,
    score: usize,
    snippet: Option<String>,
}

fn host_path_search(root: &std::path::Path, query: &str, limit: usize) -> Vec<SearchMatch> {
    let query_lower = query.to_lowercase();
    let mut tokens: Vec<String> = query_lower
        .split_whitespace()
        .filter(|t| t.len() >= 2)
        .take(6)
        .map(|t| t.to_string())
        .collect();
    if tokens.is_empty() && !query_lower.trim().is_empty() {
        tokens.push(query_lower.trim().to_string());
    }

    // Keep output compact for the guest chat UI.
    let out_limit = limit.max(1).min(5);
    let mut scored: Vec<(usize, String)> = Vec::new();
    let mut visited_files: usize = 0;
    let max_files: usize = 10_000;

    fn should_skip_dir(name: &str) -> bool {
        matches!(name, ".git" | "target" | "build" | "iso-output")
    }

    fn visit_dir(
        dir: &std::path::Path,
        root: &std::path::Path,
        tokens: &[String],
        scored: &mut Vec<(usize, String)>,
        visited_files: &mut usize,
        max_files: usize,
    ) {
        if *visited_files >= max_files {
            return;
        }

        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            if *visited_files >= max_files {
                break;
            }

            let path = entry.path();
            let Ok(ft) = entry.file_type() else {
                continue;
            };

            if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if should_skip_dir(name) {
                        continue;
                    }
                }
                visit_dir(&path, root, tokens, scored, visited_files, max_files);
                continue;
            }

            if !ft.is_file() {
                continue;
            }

            *visited_files += 1;

            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            let rel_lower = rel_s.to_lowercase();

            let mut score = 0usize;
            for t in tokens {
                if rel_lower.contains(t) {
                    score += 1;
                }
            }
            if score > 0 {
                scored.push((score, rel_s));
            }
        }
    }

    visit_dir(root, root, &tokens, &mut scored, &mut visited_files, max_files);

    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.len().cmp(&b.1.len()))
            .then_with(|| a.1.cmp(&b.1))
    });

    scored
        .into_iter()
        .take(out_limit)
        .map(|(score, path)| {
            let snippet = read_text_snippet(root.join(&path));
            SearchMatch {
                path,
                score,
                snippet,
            }
        })
        .collect()
}

fn read_text_snippet(path: std::path::PathBuf) -> Option<String> {
    // Avoid huge files / binary blobs.
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > 128 * 1024 {
        return None;
    }

    let bytes = std::fs::read(&path).ok()?;
    if bytes.iter().take(512).any(|&b| b == 0) {
        return None;
    }

    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Keep snippets short; the guest chat is narrow.
        let s = trimmed.replace('\t', " ");
        return Some(truncate_chars_with_ellipsis(&s, 60));
    }

    None
}

fn format_search_result(query: &str, matches: &[SearchMatch]) -> String {
    if matches.is_empty() {
        return format!("Search: no matches for '{}'", query);
    }

    let mut s = format!("Search: {} match(es): ", matches.len());
    for (i, m) in matches.iter().enumerate() {
        if i > 0 {
            s.push_str("; ");
        }

        let mut entry = if m.path.chars().count() > 64 {
            format!("…{}", tail_chars(&m.path, 63))
        } else {
            m.path.clone()
        };

        if let Some(snippet) = &m.snippet {
            entry.push_str(" — ");
            entry.push_str(snippet);
        }

        // Include a tiny hint when the match is weak.
        if m.score <= 1 {
            entry.push_str(" (weak)");
        }

        s.push_str(&entry);
    }

    // Hard cap for serial chunking / on-screen UI.
    const MAX_LEN: usize = 220;
    s = truncate_chars_with_ellipsis(&s, MAX_LEN);

    s
}

fn truncate_chars_with_ellipsis(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }

    let mut out: String = s.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

fn tail_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }

    s.chars().rev().take(max_chars).collect::<Vec<_>>().into_iter().rev().collect()
}

#[derive(Debug, Clone)]
pub struct OrchestratorStatistics {
    pub total_tasks: u64,
    pub completed_tasks: u64,
    pub failed_tasks: u64,
    pub stolen_tasks: u64,
    pub pending_tasks: u64,
    pub worker_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_submission() {
        let config = ConductorConfig {
            worker_threads: 2,
            ..Default::default()
        };

        let monitor = Arc::new(EntropyMonitor::new(16, 300));
        let ouroboros = Arc::new(OuroborosEngine::new());
        let orchestrator = TaskOrchestrator::new(config, monitor, ouroboros).unwrap();

        let task = Task::new(
            Priority::Normal,
            TaskPayload::Compute {
                name: "test".to_string(),
                estimated_duration: Duration::from_millis(10),
            },
        );

        let task_id = orchestrator.submit(task).unwrap();

        // Check task is in registry
        assert!(orchestrator.get_status(task_id).is_some());
    }

    #[tokio::test]
    async fn test_batch_submission() {
        let config = ConductorConfig {
            worker_threads: 2,
            ..Default::default()
        };

        let monitor = Arc::new(EntropyMonitor::new(16, 300));
        let ouroboros = Arc::new(OuroborosEngine::new());
        let orchestrator = TaskOrchestrator::new(config, monitor, ouroboros).unwrap();

        let tasks: Vec<Task> = (0..10).map(|i| {
            Task::new(
                Priority::Normal,
                TaskPayload::Compute {
                    name: format!("task_{}", i),
                    estimated_duration: Duration::from_millis(5),
                },
            )
        }).collect();

        let task_ids = orchestrator.submit_batch(tasks).unwrap();
        assert_eq!(task_ids.len(), 10);
    }
}
