use anyhow::Result;
use glam::Vec3;
/// RayOS Kernel - Main Entry Point
///
/// This boots the bicameral kernel and enters the infinite megakernel loop.
/// Phase 1: The Skeleton - Proving CPU-GPU unified memory and persistent compute.
use rayos_kernel::{types::Priority, RayKernelBuilder};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// Global shutdown signal
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("╔═══════════════════════════════════════╗");
    log::info!("║        RayOS Kernel v0.1.0           ║");
    log::info!("║   GPU-Native AI-Centric OS Kernel    ║");
    log::info!("║    Phase 1: The Skeleton (Proof)     ║");
    log::info!("║  Bicameral GPU + LLM Architecture    ║");
    log::info!("╚═══════════════════════════════════════╝");
    log::info!("");

    // Build and initialize the kernel
    log::info!("[BOOT] Building kernel systems...");
    let kernel = RayKernelBuilder::new()
        .with_dream_mode(true)
        .with_dream_timeout(300)
        .with_target_fps(60)
        .build()
        .await?;

    // Start the kernel (begin megakernel loop)
    log::info!("[BOOT] Starting megakernel...");
    kernel.start().await?;

    // Wait for startup
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Demo: Submit some test tasks
    log::info!("");
    log::info!("[DEMO] Submitting test tasks to kernel...");
    demo_submit_tasks(&kernel).await?;

    log::info!("");
    log::info!("[MONITOR] Autonomous system running...");
    log::info!("  System 1: GPU Reflex Engine (persistent shader kernel)");
    log::info!("  System 2: LLM Cognitive Engine (intent parsing + policy)");
    log::info!("  Conductor: Task orchestration & entropy management");
    log::info!("");

    // Monitor for a bit
    for i in 0..30 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let metrics = kernel.metrics();
        log::info!(
            "[METRICS] T+{:.1}s: Queue={}, Entropy={:.2}, User={}, Rays={}",
            (i + 1) as f32 * 0.5,
            metrics.queue_depth,
            metrics.entropy,
            metrics.user_present,
            metrics.active_rays
        );

        // Check for shutdown signal
        if SHUTDOWN.load(Ordering::Relaxed) {
            break;
        }
    }

    // Check if we should dream
    if kernel.should_dream() {
        log::info!("");
        log::info!("[AUTONOMOUS] System 1 entropy high - entering Dream Mode...");
        kernel.enter_dream_mode().await?;
        log::info!("[AUTONOMOUS] Dream Mode completed");
    }

    // Graceful shutdown
    log::info!("");
    log::info!("[SHUTDOWN] Stopping kernel systems...");
    kernel.shutdown();

    log::info!("✓ RayOS Kernel terminated successfully");
    log::info!("");

    Ok(())
}

/// Demo function to submit test tasks
async fn demo_submit_tasks(kernel: &rayos_kernel::RayKernel) -> Result<()> {
    // Test 1: Natural language intent
    log::info!("  [1] Processing NL task: 'optimize rendering pipeline'");
    kernel
        .process_input(Some("optimize the rendering pipeline"), None)
        .await?;

    // Test 2: Gaze-based task
    log::info!("  [2] Processing gaze task at (512, 384)");
    kernel.process_input(None, Some((512.0, 384.0))).await?;

    // Test 3: Combined multimodal
    log::info!("  [3] Processing multimodal: 'delete that file' + gaze at (100, 200)");
    kernel
        .process_input(Some("delete that file"), Some((100.0, 200.0)))
        .await?;

    // Test 4: Direct GPU ray submission
    log::info!("  [4] Submitting high-priority compute ray");
    use rayos_kernel::types::LogicRay;
    let _test_ray = LogicRay::new(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        12345,
        Priority::High,
        0,
        0,
    );
    log::info!("  ✓ Ray 12345 created (High priority)");

    Ok(())
}
