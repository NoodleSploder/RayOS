//! # RayOS Cortex - Phase 2: The Eyes
//!
//! The sensory processing layer of RayOS that connects vision, audio, and LLM capabilities.
//! This module implements the "Vision Pathway" and "Cognitive Connection" from the RayOS architecture.

pub mod vision;
pub mod llm;
pub mod fusion;
pub mod types;
pub mod kernel_link;

pub use types::*;
pub use vision::VisionPathway;
pub use llm::LLMConnector;
pub use fusion::AudioVisualFusion;

use crossbeam_channel::Sender;
use std::path::PathBuf;

/// The main Cortex coordinator that orchestrates all sensory input processing
pub struct Cortex {
    vision: VisionPathway,
    llm: LLMConnector,
    fusion: AudioVisualFusion,
    system2_tx: Option<Sender<FusedContext>>,
    qemu_monitor: Option<kernel_link::QemuMonitorLink>,
    last_intent_sig: u64,
    last_intent_ts_ms: u64,
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
            qemu_monitor: None,
            last_intent_sig: 0,
            last_intent_ts_ms: 0,
        })
    }

    /// Connect to the kernel's System 2
    pub fn connect_to_system2(&mut self, tx: Sender<FusedContext>) {
        log::info!("Connecting Cortex to kernel's System 2");
        self.system2_tx = Some(tx);
    }

    /// Connect to a running QEMU guest via its monitor socket and inject `CORTEX:`
    /// messages by typing into the guest shell.
    pub fn connect_to_qemu_monitor(&mut self, sock_path: impl Into<PathBuf>) {
        let sock_path = sock_path.into();
        log::info!("Connecting Cortex to QEMU monitor at {}", sock_path.display());
        self.qemu_monitor = Some(kernel_link::QemuMonitorLink::new(sock_path));
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

                        // Forward into the guest (kernel-bare) if configured.
                        if let Some(ref link) = self.qemu_monitor {
                            if should_send_intent(&intent, &mut self.last_intent_sig, &mut self.last_intent_ts_ms) {
                                if let Err(e) = link.send_intent(&intent).await {
                                    log::warn!("Failed to send intent to guest via QEMU monitor: {e}");
                                }
                            }
                        }

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

fn should_send_intent(intent: &Intent, last_sig: &mut u64, last_ts_ms: &mut u64) -> bool {
    // Avoid spamming idle / duplicate intents.
    if matches!(intent, Intent::Idle) {
        return false;
    }

    let sig = intent_signature(intent);
    let now = now_ms();

    // If same intent within 500ms, drop.
    if sig == *last_sig && now.saturating_sub(*last_ts_ms) < 500 {
        return false;
    }

    *last_sig = sig;
    *last_ts_ms = now;
    true
}

fn intent_signature(intent: &Intent) -> u64 {
    // Very small stable hash for de-dupe.
    fn hash_bytes(mut h: u64, bytes: &[u8]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        if h == 0 {
            h = FNV_OFFSET;
        }
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        h
    }

    match intent {
        Intent::Select { target } => hash_bytes(hash_bytes(0, b"select"), target.as_bytes()),
        Intent::Move { source, destination } => {
            let h = hash_bytes(hash_bytes(0, b"move"), source.as_bytes());
            hash_bytes(h, destination.as_bytes())
        }
        Intent::Delete { target } => hash_bytes(hash_bytes(0, b"delete"), target.as_bytes()),
        Intent::Create { object_type } => hash_bytes(hash_bytes(0, b"create"), object_type.as_bytes()),
        Intent::Break => hash_bytes(0, b"break"),
        Intent::Idle => hash_bytes(0, b"idle"),
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
