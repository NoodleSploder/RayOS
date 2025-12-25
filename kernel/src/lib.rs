/// RayOS Kernel Library
///
/// A GPU-native, AI-centric operating system kernel implementing
/// the Bicameral Architecture: System 1 (Reflex) + System 2 (Cognitive)

pub mod types;
pub mod hal;
pub mod system1;
pub mod system2;

use anyhow::Result;
use std::sync::Arc;
use types::{KernelConfig, Watcher, SystemMetrics};
use system1::ReflexEngine;
use system2::CognitiveEngine;
use hal::allocator::ZeroCopyAllocator;

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
        self.reflex.metrics()
    }

    /// Check if we should enter dream mode
    pub fn should_dream(&self) -> bool {
        self.watcher.should_dream(&self.config)
    }

    /// Enter dream mode (self-optimization)
    pub async fn enter_dream_mode(&self) -> Result<()> {
        log::info!("Entering Dream Mode (Default Mode Network)...");
        // Implement Ouroboros engine - the self-modification loop
        // This analyzes system behavior and generates optimizations

        let metrics = self.metrics();

        log::info!("Dream Mode Analysis:");
        log::info!("  Active Rays: {}", metrics.active_rays);
        log::info!("  Queue Depth: {}", metrics.queue_depth);
        log::info!("  Entropy: {:.3}", metrics.entropy);
        log::info!("  Avg Latency: {} μs", metrics.avg_latency_us);

        // Identify optimization opportunities
        let mut optimizations = Vec::new();

        if metrics.entropy > 0.5 {
            optimizations.push("High entropy detected - consider queue rebalancing");
        }

        if metrics.avg_latency_us > 200 {
            optimizations.push("High latency - consider scaling workers");
        }

        if metrics.queue_depth > 100 {
            optimizations.push("Large queue depth - consider batch processing");
        }

        if optimizations.is_empty() {
            log::info!("System running optimally, no changes needed");
        } else {
            log::info!("Optimization opportunities identified:");
            for opt in &optimizations {
                log::info!("  - {}", opt);
            }

            // In a full implementation, this would:
            // 1. Generate new kernel code based on optimizations
            // 2. Compile it using JIT or runtime compilation
            // 3. Hot-swap the running code (self-modification)
            // 4. Monitor performance impact

            // For now, just log the analysis
            log::info!("Dream mode analysis complete (self-modification deferred)");
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
