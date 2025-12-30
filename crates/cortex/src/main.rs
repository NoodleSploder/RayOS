//! RayOS Cortex - Phase 2: The Eyes
//!
//! Main entry point for the Cortex sensory processing system.
//! This application:
//! - Captures video from camera
//! - Tracks user gaze
//! - Recognizes objects in view
//! - Connects to LLM for intent interpretation
//! - Sends intents to the kernel's System 2

use rayos_cortex::Cortex;
use rayos_cortex::kernel_link::QemuMonitorLink;
use anyhow::Result;
use log::LevelFilter;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(LevelFilter::Info)
        .init();

    // Test/automation mode: send a single `CORTEX:` line into a running QEMU guest and exit.
    // This avoids initializing the VisionPathway (camera/OpenCV).
    if let (Ok(sock), Ok(line)) = (env::var("RAYOS_QEMU_MONITOR_SOCK"), env::var("RAYOS_CORTEX_TEST_LINE")) {
        let link = QemuMonitorLink::new(sock);
        log::info!("Sending one CORTEX line via QEMU monitor: {line}");
        link.send_cortex_line(&line).await?;
        return Ok(());
    }

    // Optional: send a raw shell line (useful for debugging).
    if let (Ok(sock), Ok(shell_line)) = (
        env::var("RAYOS_QEMU_MONITOR_SOCK"),
        env::var("RAYOS_CORTEX_TEST_SHELL_LINE"),
    ) {
        let link = QemuMonitorLink::new(sock);
        log::info!("Sending one shell line via QEMU monitor: {shell_line}");
        link.send_shell_line(&shell_line).await?;
        return Ok(());
    }

    log::info!("═══════════════════════════════════════");
    log::info!("  RayOS Cortex - Phase 2: The Eyes");
    log::info!("═══════════════════════════════════════");

    // Initialize the Cortex system
    let mut cortex = match Cortex::new().await {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to initialize Cortex: {}", e);
            log::error!("Make sure you have:");
            log::error!("  - A working camera (webcam)");
            log::error!("  - OpenCV installed");
            log::error!("  - Sufficient GPU memory");
            return Err(e);
        }
    };

    // If provided, forward intents into a running guest via QEMU monitor.
    if let Ok(sock) = env::var("RAYOS_QEMU_MONITOR_SOCK") {
        cortex.connect_to_qemu_monitor(sock);
    }

    log::info!("Cortex initialized successfully!");
    log::info!("Starting main processing loop...");
    log::info!("Press Ctrl+C to exit");
    log::info!("───────────────────────────────────────");

    // Set up Ctrl+C handler
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
    ctrlc::set_handler(move || {
        log::info!("Received shutdown signal...");
        let _ = tx.blocking_send(());
    })?;

    // Run the main loop in a separate task
    let cortex_task = tokio::spawn(async move {
        if let Err(e) = cortex.run().await {
            log::error!("Cortex error: {}", e);
        }
        cortex
    });

    // Wait for shutdown signal
    rx.recv().await;

    // Shutdown gracefully
    log::info!("Shutting down Cortex...");
    let cortex = cortex_task.await?;
    cortex.shutdown().await?;

    log::info!("Cortex shutdown complete. Goodbye! 👁️");
    Ok(())
}

