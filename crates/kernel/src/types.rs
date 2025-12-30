/// Core types for the RayOS kernel
///
/// This module defines the fundamental data structures that flow through
/// the bicameral kernel architecture.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use parking_lot::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

/// Priority levels for task execution
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Dream mode - background optimization
    Dream = 0,
    /// Low priority background tasks
    Low = 64,
    /// Normal user-initiated tasks
    Normal = 128,
    /// High priority tasks
    High = 192,
    /// Immediate user interaction
    Immediate = 255,
}

/// A Logic Ray - the fundamental unit of execution
///
/// Replaces traditional "threads" with rays that traverse spatial logic structures
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct LogicRay {
    /// Current State Vector (spatial origin for RT)
    pub origin: Vec3,

    /// Intent Vector (direction for RT traversal)
    pub direction: Vec3,

    /// Unique task identifier
    pub task_id: u64,

    /// Priority level (0 = Dream, 255 = Immediate)
    pub priority: u8,

    /// Reserved padding for alignment
    _padding: [u8; 7],

    /// Unified Memory Pointer to actual data payload
    pub data_ptr: u64,

    /// Which BVH logic tree to traverse
    pub logic_tree_id: u32,

    /// Reserved for future use
    _reserved: u32,
}

impl LogicRay {
    pub fn new(
        origin: Vec3,
        direction: Vec3,
        task_id: u64,
        priority: Priority,
        data_ptr: u64,
        logic_tree_id: u32,
    ) -> Self {
        Self {
            origin,
            direction,
            task_id,
            priority: priority as u8,
            _padding: [0; 7],
            data_ptr,
            logic_tree_id,
            _reserved: 0,
        }
    }
}

/// Task result from execution
#[derive(Debug, Clone)]
pub enum TaskResult {
    /// Task completed successfully
    Success,
    /// Task needs to be rescheduled
    Retry,
    /// Task failed with error
    Error(String),
}

/// System state for monitoring
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    /// Number of active rays in flight
    pub active_rays: usize,
    /// Queue depth
    pub queue_depth: usize,
    /// User presence detected
    pub user_present: bool,
    /// System entropy (0.0 = efficient, 1.0 = chaotic)
    pub entropy: f32,
    /// Average task latency in microseconds
    pub avg_latency_us: u64,
}

/// Configuration for the kernel
#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Enable dream mode during idle
    pub enable_dream_mode: bool,
    /// Idle timeout before entering dream mode (seconds)
    pub dream_timeout_secs: u64,
    /// Maximum queue size before backpressure
    pub max_queue_size: usize,
    /// Target frame time in microseconds (16666 = 60fps)
    pub target_frame_time_us: u64,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            enable_dream_mode: true,
            dream_timeout_secs: 300, // 5 minutes
            max_queue_size: 1_000_000,
            target_frame_time_us: 16_666, // 60 FPS
        }
    }
}

/// The Watcher - autonomy daemon for metabolism monitoring
pub struct Watcher {
    /// Is the user present?
    pub user_present: Arc<AtomicBool>,
    /// Current system entropy
    pub entropy: Mutex<f32>,
    /// Last user interaction timestamp
    pub last_interaction: Mutex<Instant>,
}

impl Watcher {
    pub fn new() -> Self {
        Self {
            user_present: Arc::new(AtomicBool::new(true)),
            entropy: Mutex::new(0.0),
            last_interaction: Mutex::new(Instant::now()),
        }
    }

    /// Update entropy based on system metrics
    pub fn update_entropy(&self, metrics: &SystemMetrics) {
        // Simple entropy calculation: ratio of latency to target
        let target_latency = 16_666; // 60 FPS target
        *self.entropy.lock() = (metrics.avg_latency_us as f32 / target_latency as f32).min(1.0);
    }

    pub fn entropy(&self) -> f32 {
        *self.entropy.lock()
    }

    pub fn record_interaction(&self) {
        self.user_present.store(true, std::sync::atomic::Ordering::Relaxed);
        *self.last_interaction.lock() = Instant::now();
    }

    /// Check if we should enter dream mode
    pub fn should_dream(&self, config: &KernelConfig, metrics: &SystemMetrics) -> bool {
        if !config.enable_dream_mode {
            return false;
        }

        // Only dream when idle and no user is present.
        if metrics.user_present {
            return false;
        }

        // If the system is under load (high entropy), don't enter dream mode.
        if metrics.entropy > 0.2 {
            return false;
        }

        let idle_time = self.last_interaction.lock().elapsed().as_secs();
        idle_time >= config.dream_timeout_secs
    }
}
