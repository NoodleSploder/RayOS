//! RayOS Conductor - Phase 4: The Life
//!
//! Core data structures for task orchestration, monitoring, and self-optimization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Unique identifier for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a worker thread/GPU
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerId(pub usize);

/// Priority levels for tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Critical = 0,  // User-facing, must run now
    High = 1,      // Important background work
    Normal = 2,    // Regular tasks
    Low = 3,       // Deferred optimizations
    Dream = 4,     // Self-optimization during idle
}

/// Task execution status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running { worker_id: WorkerId, started_at: Instant },
    Completed { duration: Duration },
    Failed { error: String },
    Cancelled,
}

/// A unit of work in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub priority: Priority,
    pub payload: TaskPayload,

    #[serde(skip, default = "default_task_status")]
    pub status: TaskStatus,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(default)]
    pub dependencies: Vec<TaskId>,
}

fn default_task_status() -> TaskStatus {
    TaskStatus::Pending
}

impl Task {
    pub fn new(priority: Priority, payload: TaskPayload) -> Self {
        Self {
            id: TaskId::new(),
            priority,
            payload,
            status: TaskStatus::Pending,
            created_at: Instant::now(),
            dependencies: Vec::new(),
        }
    }

    pub fn with_dependencies(mut self, deps: Vec<TaskId>) -> Self {
        self.dependencies = deps;
        self
    }
}

/// Different types of work the system can perform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskPayload {
    /// Compute task (generic work)
    Compute {
        name: String,
        estimated_duration: Duration,
    },

    /// Index a file in the semantic file system
    IndexFile {
        path: PathBuf,
    },

    /// Search query
    Search {
        query: String,
        limit: usize,
    },

    /// Self-optimization mutation
    Optimize {
        target: OptimizationTarget,
    },

    /// Background maintenance
    Maintenance {
        task_type: MaintenanceType,
    },
}

/// What to optimize
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptimizationTarget {
    Function { name: String, binary: Vec<u8> },
    Module { path: PathBuf },
    System,
}

/// Types of maintenance tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaintenanceType {
    GarbageCollection,
    IndexRebuild,
    CacheFlush,
    MetricsExport,
}

/// System performance metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// Total tasks executed
    pub total_tasks: u64,
    /// Tasks currently running
    pub active_tasks: u64,
    /// Tasks waiting in queue
    pub pending_tasks: u64,
    /// Average task latency (ms)
    pub avg_latency_ms: f64,
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Memory usage (MB)
    pub memory_mb: f64,
    /// GPU usage percentage (if available)
    pub gpu_usage: Option<f64>,
    /// Time since last user interaction
    pub idle_duration: Duration,
}

/// Worker (thread/GPU) status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatus {
    pub id: WorkerId,
    pub worker_type: WorkerType,
    pub current_task: Option<TaskId>,
    pub tasks_completed: u64,
    pub total_work_time: Duration,
    pub load_factor: f64,  // 0.0 = idle, 1.0 = saturated
}

/// Type of computational resource
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerType {
    CpuThread,
    ApuCompute,
    DGpu { index: usize },
}

/// System load snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemLoad {
    #[serde(skip, default = "Instant::now")]
    pub timestamp: Instant,
    pub metrics: SystemMetrics,
    pub workers: Vec<WorkerStatus>,
    pub bottleneck: Option<Bottleneck>,
}

/// Detected performance bottleneck
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Bottleneck {
    CpuSaturated,
    MemoryPressure,
    GpuStarved,
    IoWait,
    TaskQueueOverflow,
}

/// Result of a code mutation attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationResult {
    pub mutation_id: Uuid,
    pub original_duration: Duration,
    pub mutated_duration: Duration,
    pub improvement_factor: f64,  // >1.0 = faster, <1.0 = slower
    pub passed_tests: bool,
    pub binary_diff: Vec<u8>,
}

impl MutationResult {
    pub fn is_improvement(&self) -> bool {
        self.passed_tests && self.improvement_factor > 1.05  // 5% threshold
    }
}

/// Configuration for the conductor system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConductorConfig {
    /// Number of worker threads
    pub worker_threads: usize,
    /// Enable GPU workers if available
    pub enable_gpu: bool,
    /// Idle time before entering Dream Mode (seconds)
    pub dream_threshold_secs: u64,
    /// Maximum task queue size
    pub max_queue_size: usize,
    /// Latency threshold for watchdog (ms)
    pub latency_threshold_ms: u64,
    /// Enable self-optimization
    pub enable_ouroboros: bool,
    /// Metrics export interval (seconds)
    pub metrics_interval_secs: u64,
}

impl Default for ConductorConfig {
    fn default() -> Self {
        Self {
            worker_threads: num_cpus::get(),
            enable_gpu: true,
            dream_threshold_secs: 300,  // 5 minutes
            max_queue_size: 10000,
            latency_threshold_ms: 16,  // 60fps target
            enable_ouroboros: false,   // Off by default for safety
            metrics_interval_secs: 60,
        }
    }
}

/// Dream Mode state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DreamState {
    Awake,
    Drowsy,    // Approaching idle threshold
    Dreaming,  // Active self-optimization
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new(
            Priority::Normal,
            TaskPayload::Compute {
                name: "test".to_string(),
                estimated_duration: Duration::from_millis(100),
            },
        );

        assert_eq!(task.priority, Priority::Normal);
        assert!(matches!(task.status, TaskStatus::Pending));
    }

    #[test]
    fn test_mutation_improvement() {
        let result = MutationResult {
            mutation_id: Uuid::new_v4(),
            original_duration: Duration::from_millis(100),
            mutated_duration: Duration::from_millis(80),
            improvement_factor: 1.25,
            passed_tests: true,
            binary_diff: vec![],
        };

        assert!(result.is_improvement());
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Normal);
        assert!(Priority::Normal < Priority::Low);
        assert!(Priority::Low < Priority::Dream);
    }
}
