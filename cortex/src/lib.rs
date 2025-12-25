//! # RayOS Cortex - Phase 2: The Eyes
//!
//! The sensory processing layer of RayOS that connects vision, audio, and LLM capabilities.
//! This module implements the "Vision Pathway" and "Cognitive Connection" from the RayOS architecture.

pub mod vision;
pub mod llm;
pub mod fusion;
pub mod types;

pub use types::*;
pub use vision::VisionPathway;
pub use llm::LLMConnector;
pub use fusion::AudioVisualFusion;

use crossbeam_channel::Sender;

/// The main Cortex coordinator that orchestrates all sensory input processing
pub struct Cortex {
    vision: VisionPathway,
    llm: LLMConnector,
    fusion: AudioVisualFusion,
    system2_tx: Option<Sender<FusedContext>>,
}

impl Cortex {
    /// Initialize the Cortex system with default configuration
    pub async fn new() -> anyhow::Result<Self> {
        log::info!("Initializing RayOS Cortex...");

        let vision = VisionPathway::new().await?;
        let llm = LLMConnector::new().await?;
        let fusion = AudioVisualFusion::new();

        log::info!("Cortex initialization complete");

        Ok(Self {
            vision,
            llm,
            fusion,
            system2_tx: None,
        })
    }

    /// Connect to the kernel's System 2
    pub fn connect_to_system2(&mut self, tx: Sender<FusedContext>) {
        log::info!("Connecting Cortex to kernel's System 2");
        self.system2_tx = Some(tx);
    }

    /// Start the main processing loop
    pub async fn run(&mut self) -> anyhow::Result<()> {
        log::info!("Starting Cortex main loop...");

        // Start vision processing
        self.vision.start().await?;

        loop {
            // Get gaze data
            if let Some(gaze) = self.vision.get_gaze_data().await? {
                log::debug!("Gaze: ({:.2}, {:.2})", gaze.screen_x, gaze.screen_y);

                // Get visual context
                if let Some(context) = self.vision.get_visual_context().await? {
                    // Fuse with any audio context
                    let fused = self.fusion.fuse(gaze, context);

                    // Send to LLM for interpretation
                    if let Some(intent) = self.llm.process_context(&fused).await? {
                        log::info!("Detected intent: {:?}", intent);

                        // Send intent to kernel's System 2
                        if let Some(ref tx) = self.system2_tx {
                            match tx.send(fused.clone()) {
                                Ok(_) => log::debug!("Intent sent to System 2"),
                                Err(e) => log::warn!("Failed to send intent to System 2: {}", e),
                            }
                        } else {
                            log::warn!("System 2 connection not established, intent not forwarded");
                        }
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(16)).await; // ~60Hz
        }
    }

    /// Shutdown the Cortex system gracefully
    pub async fn shutdown(self) -> anyhow::Result<()> {
        log::info!("Shutting down Cortex...");
        self.vision.stop().await?;
        Ok(())
    }
}
