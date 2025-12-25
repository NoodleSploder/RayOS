/// Hive Manager - "Work Stealing" across multiple GPUs
///
/// Implements distributed task execution across APU and dGPUs
/// using a work-stealing algorithm to balance load.

use crate::types::LogicRay;
use anyhow::Result;
use crossbeam::queue::SegQueue;
use parking_lot::RwLock;
use std::sync::Arc;

/// Statistics for a worker GPU
#[derive(Debug, Clone, Default)]
pub struct WorkerStats {
    /// Total tasks processed
    pub tasks_completed: u64,
    /// Current queue depth
    pub queue_depth: usize,
    /// Average task time in microseconds
    pub avg_task_time_us: u64,
}

/// A worker GPU in the hive
pub struct HiveWorker {
    /// Worker ID
    pub id: usize,
    /// Task queue for this worker
    queue: Arc<SegQueue<LogicRay>>,
    /// Statistics
    stats: Arc<RwLock<WorkerStats>>,
    /// Is this worker active?
    active: Arc<RwLock<bool>>,
}

impl HiveWorker {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            queue: Arc::new(SegQueue::new()),
            stats: Arc::new(RwLock::new(WorkerStats::default())),
            active: Arc::new(RwLock::new(true)),
        }
    }

    /// Push a task to this worker's queue
    pub fn push_task(&self, ray: LogicRay) {
        self.queue.push(ray);
        self.stats.write().queue_depth = self.queue.len();
    }

    /// Try to steal a task from this worker (called by other workers)
    pub fn try_steal(&self) -> Option<LogicRay> {
        self.queue.pop()
    }

    /// Pop a task for execution
    pub fn pop_task(&self) -> Option<LogicRay> {
        let ray = self.queue.pop();
        if ray.is_some() {
            self.stats.write().queue_depth = self.queue.len();
        }
        ray
    }

    /// Get current statistics
    pub fn stats(&self) -> WorkerStats {
        self.stats.read().clone()
    }

    /// Set worker active state
    pub fn set_active(&self, active: bool) {
        *self.active.write() = active;
    }

    /// Is this worker active?
    pub fn is_active(&self) -> bool {
        *self.active.read()
    }
}

/// Hive Manager - coordinates work across all GPUs
pub struct HiveManager {
    /// All worker GPUs
    workers: Vec<HiveWorker>,
    /// Primary worker (APU or first GPU)
    primary_worker_id: usize,
}

impl HiveManager {
    /// Create a new hive manager with specified number of workers
    pub fn new(num_workers: usize) -> Self {
        log::info!("Initializing Hive Manager with {} workers", num_workers);

        let workers = (0..num_workers)
            .map(|id| HiveWorker::new(id))
            .collect();

        Self {
            workers,
            primary_worker_id: 0,
        }
    }

    /// Submit a task to the hive (load balances automatically)
    pub fn submit_task(&self, ray: LogicRay) -> Result<()> {
        // Find worker with smallest queue
        let best_worker = self.workers
            .iter()
            .filter(|w| w.is_active())
            .min_by_key(|w| w.stats().queue_depth)
            .ok_or_else(|| anyhow::anyhow!("No active workers available"))?;

        best_worker.push_task(ray);
        Ok(())
    }

    /// Submit multiple tasks in batch
    pub fn submit_batch(&self, rays: Vec<LogicRay>) -> Result<()> {
        // Round-robin distribution
        let active_workers: Vec<_> = self.workers
            .iter()
            .filter(|w| w.is_active())
            .collect();

        if active_workers.is_empty() {
            anyhow::bail!("No active workers available");
        }

        for (idx, ray) in rays.into_iter().enumerate() {
            let worker_idx = idx % active_workers.len();
            active_workers[worker_idx].push_task(ray);
        }

        Ok(())
    }

    /// Get a reference to a specific worker
    pub fn worker(&self, id: usize) -> Option<&HiveWorker> {
        self.workers.get(id)
    }

    /// Get the primary worker
    pub fn primary_worker(&self) -> &HiveWorker {
        &self.workers[self.primary_worker_id]
    }

    /// Total tasks across all workers
    pub fn total_queue_depth(&self) -> usize {
        self.workers
            .iter()
            .map(|w| w.stats().queue_depth)
            .sum()
    }

    /// Get aggregate statistics
    pub fn aggregate_stats(&self) -> Vec<WorkerStats> {
        self.workers
            .iter()
            .map(|w| w.stats())
            .collect()
    }

    /// Wake up idle workers if needed
    pub fn balance_load(&self) {
        let total_depth = self.total_queue_depth();
        let worker_count = self.workers.len();
        let avg_depth = total_depth / worker_count.max(1);

        // Activate all workers if average queue depth is high
        if avg_depth > 1000 {
            for worker in &self.workers {
                worker.set_active(true);
            }
            log::debug!("High load detected, activating all workers");
        }
    }
}
