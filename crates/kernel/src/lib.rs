/// RayOS Kernel Library
///
/// A GPU-native, AI-centric operating system kernel implementing
/// the Bicameral Architecture: System 1 (Reflex) + System 2 (Cognitive)

pub mod types;
pub mod hal;
pub mod system1;
pub mod system2;
pub mod task_queue;

use anyhow::Result;
use std::sync::Arc;
use types::{KernelConfig, Watcher, SystemMetrics};
use system1::ReflexEngine;
use system2::CognitiveEngine;
use hal::allocator::ZeroCopyAllocator;
use task_queue::TaskCompletion;

/// The RayOS Kernel - "The Brain"
pub struct RayKernel {
    /// System 1: The Reflex Engine (Subconscious)
    reflex: Arc<ReflexEngine>,

    /// System 2: The Cognitive Engine (Conscious)
    cognitive: Arc<CognitiveEngine>,

    /// Zero-copy allocator for unified memory
    allocator: Arc<ZeroCopyAllocator>,

    /// Autonomy watcher (The Metabolism)
    watcher: Arc<Watcher>,

    /// Configuration
    config: KernelConfig,
}

impl RayKernel {
    /// Initialize the RayOS kernel
    pub async fn initialize(config: KernelConfig) -> Result<Self> {
        log::info!("=== Initializing RayOS Kernel ===");
        log::info!("Version: 0.1.0-alpha (Phase 1: The Skeleton)");

        // Initialize allocator
        let allocator = Arc::new(ZeroCopyAllocator::default());
        log::info!("✓ Zero-Copy Allocator initialized");

        // Initialize System 1 (Reflex Engine)
        let reflex = Arc::new(ReflexEngine::new(config.clone()).await?);
        log::info!("✓ System 1 (Reflex Engine) initialized");

        // Initialize System 2 (Cognitive Engine)
        let cognitive = Arc::new(CognitiveEngine::new(config.clone()));
        log::info!("✓ System 2 (Cognitive Engine) initialized");

        // Initialize watcher
        let watcher = Arc::new(Watcher::new());
        log::info!("✓ Autonomy Watcher initialized");

        log::info!("=== RayOS Kernel Ready ===");

        Ok(Self {
            reflex,
            cognitive,
            allocator,
            watcher,
            config,
        })
    }

    /// Start the kernel (begin the infinite loop)
    pub async fn start(&self) -> Result<()> {
        log::info!("Starting RayOS Kernel...");

        // Start the megakernel loop
        self.reflex.start().await?;

        log::info!("RayOS Kernel is now running");
        Ok(())
    }

    /// Process user input (multimodal)
    pub async fn process_input(&self, text: Option<&str>, gaze: Option<(f32, f32)>) -> Result<()> {
        if text.is_some() || gaze.is_some() {
            self.watcher.record_interaction();
        }

        // Parse intent using System 2
        let rays = self.cognitive.process_multimodal(text, gaze).await?;

        // Submit to System 1 for execution
        for ray in rays {
            self.reflex.submit_ray(ray)?;
        }

        Ok(())
    }

    /// Get system metrics
    pub fn metrics(&self) -> SystemMetrics {
        let metrics = self.reflex.metrics();
        self.watcher.update_entropy(&metrics);
        metrics
    }

    /// Drain completed task results (best-effort, non-blocking).
    pub fn drain_completions(&self, max: usize) -> Vec<TaskCompletion> {
        self.reflex.drain_completions(max)
    }

    /// Check if we should enter dream mode
    pub fn should_dream(&self) -> bool {
        let metrics = self.metrics();
        self.watcher.should_dream(&self.config, &metrics)
    }

    /// Enter dream mode (self-optimization)
    pub async fn enter_dream_mode(&self) -> Result<()> {
        log::info!("Entering Dream Mode (Default Mode Network)...");
        // Implement a real (but bounded) optimization loop that tunes System 1 execution
        // parameters based on real workload metrics.

        for pass in 0..3u32 {
            let metrics = self.metrics();

            log::info!("Dream Mode Pass {pass}:");
            log::info!("  Active Rays: {}", metrics.active_rays);
            log::info!("  Queue Depth: {}", metrics.queue_depth);
            log::info!("  Entropy: {:.3}", metrics.entropy);
            log::info!("  Avg Latency: {} μs", metrics.avg_latency_us);

            // Nothing to optimize without measurements.
            if metrics.avg_latency_us == 0 {
                log::info!("  No completed tasks yet; skipping tuning");
                break;
            }

            let target = self.config.target_frame_time_us.max(1);
            let ratio = metrics.avg_latency_us as f32 / target as f32;

            let (cur_budget, cur_dispatches) = self.reflex.gpu_tuning();
            let mut new_budget = cur_budget;
            let mut new_dispatches = cur_dispatches;

            // Heuristic tuning rules:
            // - If latency is too high, do more work per GPU submission and allow more retries.
            // - If latency is well below target, scale down to reduce wasted GPU churn.
            if ratio > 1.2 {
                new_budget = (cur_budget.saturating_mul(2)).clamp(64, 2048);
                new_dispatches = (cur_dispatches + 16).clamp(16, 256);
            } else if ratio < 0.5 {
                new_budget = (cur_budget / 2).clamp(32, 1024);
                new_dispatches = (cur_dispatches.saturating_sub(16)).clamp(8, 128);
            }

            if new_budget == cur_budget && new_dispatches == cur_dispatches {
                log::info!("  No tuning changes needed");
                break;
            }

            self.reflex.set_gpu_tuning(new_budget, new_dispatches);
            log::info!(
                "  Applied tuning: iteration_budget {}→{}, max_dispatches {}→{}",
                cur_budget,
                new_budget,
                cur_dispatches,
                new_dispatches
            );

            // Let the system settle briefly.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        Ok(())
    }

    /// Shutdown the kernel gracefully
    pub fn shutdown(&self) {
        log::info!("Shutting down RayOS Kernel...");
        self.reflex.stop();
        log::info!("Kernel shutdown complete");
    }

    /// Get allocator reference
    pub fn allocator(&self) -> &Arc<ZeroCopyAllocator> {
        &self.allocator
    }
}

/// Builder for RayKernel with custom configuration
pub struct RayKernelBuilder {
    config: KernelConfig,
}

impl RayKernelBuilder {
    pub fn new() -> Self {
        Self {
            config: KernelConfig::default(),
        }
    }

    pub fn with_dream_mode(mut self, enabled: bool) -> Self {
        self.config.enable_dream_mode = enabled;
        self
    }

    pub fn with_dream_timeout(mut self, seconds: u64) -> Self {
        self.config.dream_timeout_secs = seconds;
        self
    }

    pub fn with_max_queue_size(mut self, size: usize) -> Self {
        self.config.max_queue_size = size;
        self
    }

    pub fn with_target_fps(mut self, fps: u32) -> Self {
        self.config.target_frame_time_us = 1_000_000 / fps as u64;
        self
    }

    pub async fn build(self) -> Result<RayKernel> {
        RayKernel::initialize(self.config).await
    }
}

impl Default for RayKernelBuilder {
    fn default() -> Self {
        Self::new()
    }
}
