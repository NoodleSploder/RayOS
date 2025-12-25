/// System 1: The Reflex Engine - "The Subconscious"
///
/// This is the persistent compute shader that runs the megakernel loop.
/// Handles millisecond-level execution, the core event loop of RayOS.

pub mod megakernel;
pub mod ray_logic;

use crate::hal::HalManager;
use crate::hal::hive::HiveManager;
use crate::types::{LogicRay, TaskResult, SystemMetrics, KernelConfig};
use anyhow::Result;
use crossbeam::queue::SegQueue;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// System 1 - The Reflex Engine
pub struct ReflexEngine {
    /// Hardware abstraction layer
    hal: Arc<HalManager>,

    /// Hive manager for multi-GPU coordination
    hive: Arc<HiveManager>,

    /// Global task queue
    task_queue: Arc<SegQueue<LogicRay>>,

    /// Is the engine running?
    running: Arc<AtomicBool>,

    /// Total rays processed
    rays_processed: Arc<AtomicU64>,

    /// Configuration
    config: KernelConfig,
}

impl ReflexEngine {
    /// Create a new Reflex Engine
    pub async fn new(config: KernelConfig) -> Result<Self> {
        log::info!("Initializing System 1: Reflex Engine");

        let hal = Arc::new(HalManager::new().await?);
        let device_count = hal.device_count();

        let hive = Arc::new(HiveManager::new(device_count));

        Ok(Self {
            hal,
            hive,
            task_queue: Arc::new(SegQueue::new()),
            running: Arc::new(AtomicBool::new(false)),
            rays_processed: Arc::new(AtomicU64::new(0)),
            config,
        })
    }

    /// Start the megakernel loop
    pub async fn start(&self) -> Result<()> {
        if self.running.load(Ordering::Relaxed) {
            log::warn!("Reflex Engine already running");
            return Ok(());
        }

        log::info!("Starting Reflex Engine megakernel loop");
        self.running.store(true, Ordering::Relaxed);

        // Spawn the main loop
        let engine = self.clone_arc();
        tokio::spawn(async move {
            if let Err(e) = engine.megakernel_loop().await {
                log::error!("Megakernel loop error: {}", e);
            }
        });

        Ok(())
    }

    /// Stop the megakernel loop
    pub fn stop(&self) {
        log::info!("Stopping Reflex Engine");
        self.running.store(false, Ordering::Relaxed);
    }

    /// Submit a task (ray) for execution
    pub fn submit_ray(&self, ray: LogicRay) -> Result<()> {
        if self.task_queue.len() >= self.config.max_queue_size {
            anyhow::bail!("Task queue full! Backpressure applied.");
        }

        self.task_queue.push(ray);
        Ok(())
    }

    /// Submit multiple rays in batch
    pub fn submit_ray_batch(&self, rays: Vec<LogicRay>) -> Result<()> {
        if self.task_queue.len() + rays.len() >= self.config.max_queue_size {
            anyhow::bail!("Task queue would overflow!");
        }

        // Distribute to hive for parallel processing
        self.hive.submit_batch(rays)?;

        Ok(())
    }

    /// The infinite megakernel loop (while true)
    async fn megakernel_loop(&self) -> Result<()> {
        log::info!("Entering megakernel loop (infinite execution)");

        let mut frame_count = 0u64;
        let start_time = std::time::Instant::now();

        while self.running.load(Ordering::Relaxed) {
            let frame_start = std::time::Instant::now();

            // Process rays from global queue
            let mut rays_this_frame = 0;

            // Dequeue and distribute to workers
            while let Some(ray) = self.task_queue.pop() {
                if let Err(e) = self.hive.submit_task(ray) {
                    log::error!("Failed to submit task to hive: {}", e);
                    break;
                }
                rays_this_frame += 1;

                // Limit per frame to maintain real-time performance
                if rays_this_frame >= 10000 {
                    break;
                }
            }

            // Execute rays on workers (simulate for now)
            self.execute_worker_tasks().await?;

            self.rays_processed.fetch_add(rays_this_frame as u64, Ordering::Relaxed);

            // Maintain frame rate
            let frame_time = frame_start.elapsed();
            let target_frame_time = std::time::Duration::from_micros(
                self.config.target_frame_time_us
            );

            if frame_time < target_frame_time {
                tokio::time::sleep(target_frame_time - frame_time).await;
            }

            frame_count += 1;

            // Log stats every second
            if frame_count % 60 == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rays_per_sec = self.rays_processed.load(Ordering::Relaxed) as f64 / elapsed;

                log::debug!(
                    "Frame {}: {:.0} rays/sec, Queue depth: {}",
                    frame_count,
                    rays_per_sec,
                    self.task_queue.len()
                );
            }

            // Load balancing
            self.hive.balance_load();
        }

        log::info!("Megakernel loop terminated");
        Ok(())
    }

    /// Execute tasks from worker queues
    async fn execute_worker_tasks(&self) -> Result<()> {
        // For each worker, pop and execute tasks
        let stats = self.hive.aggregate_stats();

        for (worker_id, worker_stats) in stats.iter().enumerate() {
            if worker_stats.queue_depth == 0 {
                continue;
            }

            if let Some(worker) = self.hive.worker(worker_id) {
                // Pop a task and execute it using RT Core simulation
                if let Some(ray) = worker.pop_task() {
                    // Actual RT Core traversal simulation
                    log::debug!("Executing ray {} on worker {}", ray.task_id, worker_id);
                    self.execute_ray(ray).await?;
                }
            }
        }

        Ok(())
    }

    /// Execute a single ray using RT Core logic tree traversal
    async fn execute_ray(&self, ray: LogicRay) -> Result<TaskResult> {
        // Implement RT Core-style BVH traversal simulation
        // In real hardware, this would be done by GPU RT cores

        let start = std::time::Instant::now();

        // Simulate BVH tree traversal
        // Each ray traverses a logic tree structure
        let tree_depth = (ray.logic_tree_id as usize % 10) + 5;  // 5-15 levels deep
        let mut nodes_visited = 0;

        for depth in 0..tree_depth {
            // Simulate AABB intersection test (what RT cores do)
            let intersects = (ray.task_id + depth as u64) % 3 != 0;  // 2/3 hit rate

            if intersects {
                nodes_visited += 1;
                // Simulate work at this node
                tokio::time::sleep(std::time::Duration::from_micros(1)).await;
            } else {
                // Early exit (ray missed)
                break;
            }
        }

        // Track latency
        let elapsed = start.elapsed().as_micros() as u64;
        self.rays_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        log::trace!("Ray {} traversed {} nodes in {} μs", ray.task_id, nodes_visited, elapsed);

        Ok(TaskResult::Success)
    }

    /// Get current system metrics
    pub fn metrics(&self) -> SystemMetrics {
        let queue_depth = self.hive.total_queue_depth();
        let active_rays = self.task_queue.len();
        let rays_processed = self.rays_processed.load(std::sync::atomic::Ordering::Relaxed);

        // Calculate entropy based on queue depth and activity
        // Higher queue depth = higher entropy (more disorder/unpredictability)
        let entropy = if rays_processed > 0 {
            (queue_depth as f32 / (rays_processed as f32 + 1.0)).min(1.0)
        } else {
            0.0
        };

        // Estimate average latency based on queue depth
        // More tasks = higher latency due to contention
        let avg_latency_us = 50 + (queue_depth as u64 * 10);

        // User presence would be connected to Cortex gaze/vision system
        // For now, assume present if we have active rays
        let user_present = active_rays > 0 || queue_depth > 0;

        SystemMetrics {
            active_rays,
            queue_depth,
            user_present,
            entropy,
            avg_latency_us,
        }
    }

    /// Helper to clone Arc references for async tasks
    fn clone_arc(&self) -> Self {
        Self {
            hal: Arc::clone(&self.hal),
            hive: Arc::clone(&self.hive),
            task_queue: Arc::clone(&self.task_queue),
            running: Arc::clone(&self.running),
            rays_processed: Arc::clone(&self.rays_processed),
            config: self.config.clone(),
        }
    }
}
