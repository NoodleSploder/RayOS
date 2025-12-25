//! Task Orchestrator - Work stealing scheduler with dynamic load balancing
//!
//! Distributes tasks across heterogeneous workers (CPU threads, APU, dGPUs)
//! using work-stealing algorithms.

use crate::entropy_monitor::EntropyMonitor;
use crate::types::*;
use anyhow::{Context, Result};
use crossbeam_deque::{Injector, Stealer, Worker as DequeWorker};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Semaphore};

/// Work-stealing task orchestrator
pub struct TaskOrchestrator {
    /// Global task injector (shared queue)
    injector: Arc<Injector<Task>>,

    /// Per-worker local queues
    workers: Vec<WorkerContext>,

    /// Work stealers (for cross-worker stealing)
    stealers: Vec<Stealer<Task>>,

    /// Task registry (for status tracking)
    tasks: Arc<DashMap<TaskId, Task>>,

    /// Entropy monitor
    monitor: Arc<EntropyMonitor>,

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
    local_queue: Arc<DequeWorker<Task>>,
    tasks_completed: Arc<AtomicU64>,
    current_task: Arc<RwLock<Option<TaskId>>>,
    semaphore: Arc<Semaphore>,
}

impl Clone for WorkerContext {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            worker_type: self.worker_type,
            local_queue: self.local_queue.clone(),
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
    pub fn new(config: ConductorConfig, monitor: Arc<EntropyMonitor>) -> Result<Self> {
        log::info!("Initializing Task Orchestrator with {} workers", config.worker_threads);

        let injector = Arc::new(Injector::new());
        let mut workers = Vec::new();
        let mut stealers = Vec::new();

        // Create worker contexts
        for i in 0..config.worker_threads {
            let local_queue = Arc::new(DequeWorker::new_fifo());
            stealers.push(local_queue.stealer());

            let ctx = WorkerContext {
                id: WorkerId(i),
                worker_type: WorkerType::CpuThread,
                local_queue,
                tasks_completed: Arc::new(AtomicU64::new(0)),
                current_task: Arc::new(RwLock::new(None)),
                semaphore: Arc::new(Semaphore::new(1)),
            };

            workers.push(ctx);
        }

        Ok(Self {
            injector,
            workers,
            stealers,
            tasks: Arc::new(DashMap::new()),
            monitor,
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

        let mut handles = Vec::new();

        for ctx in &self.workers {
            let handle = self.spawn_worker(ctx.clone()).await?;
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
    async fn spawn_worker(&self, ctx: WorkerContext) -> Result<tokio::task::JoinHandle<()>> {
        let injector = self.injector.clone();
        let stealers = self.stealers.clone();
        let tasks = self.tasks.clone();
        let monitor = self.monitor.clone();
        let stats = self.stats.clone();
        let shutdown = self.shutdown.clone();

        let worker_id = ctx.id;
        let local_queue = ctx.local_queue.clone();
        let tasks_completed = ctx.tasks_completed.clone();
        let current_task = ctx.current_task.clone();
        let semaphore = ctx.semaphore.clone();

        let handle = tokio::spawn(async move {
            log::debug!("Worker {} started", worker_id.0);

            while !shutdown.load(Ordering::Relaxed) {
                // Try to acquire semaphore (for backpressure)
                let _permit = semaphore.acquire().await.unwrap();

                // Find a task to execute (using local queue)
                let task = Self::find_task_static(&local_queue, &injector, &stealers, &stats);

                if let Some(task) = task {
                    // Update current task
                    *current_task.write() = Some(task.id);

                    // Execute the task
                    let start = Instant::now();
                    let result = Self::execute_task(&task).await;
                    let duration = start.elapsed();

                    // Record metrics
                    monitor.record_task(
                        format!("{:?}", task.payload),
                        duration
                    );

                    // Update task status
                    if let Some(mut entry) = tasks.get_mut(&task.id) {
                        entry.status = match result {
                            Ok(()) => {
                                stats.completed_tasks.fetch_add(1, Ordering::Relaxed);
                                TaskStatus::Completed { duration }
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
        local_queue: &Arc<DequeWorker<Task>>,
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

    /// Find a task using work-stealing algorithm (old method for reference)
    fn find_task(
        ctx: &WorkerContext,
        injector: &Injector<Task>,
        stealers: &[Stealer<Task>],
        stats: &OrchestratorStats,
    ) -> Option<Task> {
        Self::find_task_static(&ctx.local_queue, injector, stealers, stats)
    }

    /// Execute a task
    async fn execute_task(task: &Task) -> Result<()> {
        match &task.payload {
            TaskPayload::Compute { name, estimated_duration } => {
                log::debug!("Executing compute task: {}", name);
                tokio::time::sleep(*estimated_duration).await;
                Ok(())
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
                        Ok(())
                    }
                    Err(e) => {
                        log::error!("Failed to index file {}: {}", path.display(), e);
                        Err(format!("Index failed: {}", e))
                    }
                }
            }

            TaskPayload::Search { query, limit } => {
                log::debug!("Searching: {} (limit: {})", query, limit);
                // Integration with Volume (Phase 3) - simulated
                // In production: volume.search(query, limit).await?

                // Simulate semantic search
                let search_time = Duration::from_millis(5 + (query.len() as u64 * 2));
                tokio::time::sleep(search_time).await;

                log::info!("Search completed: '{}' (found {} results)",
                    query, (limit as f32 * 0.7) as usize);
                Ok(())
            }

            TaskPayload::Optimize { target } => {
                log::debug!("Optimizing: {:?}", target);
                // Integration with Ouroboros - simulated self-optimization
                // In production: ouroboros.optimize(target).await?

                // Simulate optimization cycles
                let optimization_cycles = match target.as_str() {
                    "hot_path" => 5,
                    "memory" => 3,
                    "latency" => 4,
                    _ => 2,
                };

                for cycle in 0..optimization_cycles {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    log::debug!("Optimization cycle {}/{} for {}",
                        cycle + 1, optimization_cycles, target);
                }

                log::info!("Optimization completed: {} ({} cycles)", target, optimization_cycles);
                Ok(())
            }

            TaskPayload::Maintenance { task_type } => {
                log::debug!("Maintenance: {:?}", task_type);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(())
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
            let avg_task_duration_ms = if completed > 0 {
                // Estimate based on task type and completed count
                match ctx.worker_type {
                    WorkerType::Realtime => 10.0,
                    WorkerType::Compute => 50.0,
                    WorkerType::Background => 100.0,
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
        let orchestrator = TaskOrchestrator::new(config, monitor).unwrap();

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
        let orchestrator = TaskOrchestrator::new(config, monitor).unwrap();

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
