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
use anyhow::Result;
use log::LevelFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(LevelFilter::Info)
        .init();

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

