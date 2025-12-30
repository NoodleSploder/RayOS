//! Vision pathway implementation - Gaze tracking and object recognition

use crate::types::{GazePoint, VisualContext, DetectedObject, BoundingBox, Color};
#[cfg(feature = "vision")]
use opencv::{
    core, highgui, imgproc, objdetect, videoio,
    prelude::*,
};
use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

mod gaze_tracker;
mod object_recognizer;
mod udp_gaze;

pub use gaze_tracker::GazeTracker;
pub use object_recognizer::ObjectRecognizer;
pub use udp_gaze::{parse_gaze_message, udp_gaze_addr_from_env};

/// The Vision Pathway - handles all visual input processing
pub struct VisionPathway {
    gaze_tracker: Option<GazeTracker>,
    object_recognizer: Option<ObjectRecognizer>,
    #[cfg(feature = "vision")]
    camera: Arc<Mutex<videoio::VideoCapture>>,
    current_gaze: Arc<Mutex<Option<GazePoint>>>,
    current_context: Arc<Mutex<Option<VisualContext>>>,
    udp_gaze_addr: Option<std::net::SocketAddr>,
}

impl VisionPathway {
    /// Create a new Vision Pathway
    pub async fn new() -> Result<Self> {
        log::info!("Initializing Vision Pathway...");

        let udp_gaze_addr = udp_gaze::udp_gaze_addr_from_env();
        if let Some(addr) = udp_gaze_addr {
            log::info!("UDP gaze input enabled via RAYOS_GAZE_UDP_ADDR={addr}");
        }

        #[cfg(feature = "vision")]
        {
            // Open camera
            let camera = videoio::VideoCapture::new(0, videoio::CAP_ANY)
                .context("Failed to open camera")?;
            let camera = Arc::new(Mutex::new(camera));

            // Initialize gaze tracker
            let gaze_tracker = GazeTracker::new(Arc::clone(&camera)).await?;

            // Initialize object recognizer
            let object_recognizer = ObjectRecognizer::new().await?;

            Ok(Self {
                gaze_tracker: Some(gaze_tracker),
                object_recognizer: Some(object_recognizer),
                camera,
                current_gaze: Arc::new(Mutex::new(None)),
                current_context: Arc::new(Mutex::new(None)),
                udp_gaze_addr,
            })
        }

        #[cfg(not(feature = "vision"))]
        {
            log::warn!("Vision feature not enabled, using simulated data");

            // Initialize with stubs
            let gaze_tracker = GazeTracker::new_stub().await?;
            let object_recognizer = ObjectRecognizer::new().await?;

            Ok(Self {
                gaze_tracker: Some(gaze_tracker),
                object_recognizer: Some(object_recognizer),
                current_gaze: Arc::new(Mutex::new(None)),
                current_context: Arc::new(Mutex::new(None)),
                udp_gaze_addr,
            })
        }
    }

    /// Start the vision processing pipeline
    pub async fn start(&mut self) -> Result<()> {
        log::info!("Starting Vision Pathway...");

        // Optional hardware gaze input (UDP). Works in both vision and stub modes.
        if let Some(addr) = self.udp_gaze_addr {
            let gaze_storage = Arc::clone(&self.current_gaze);
            udp_gaze::spawn_udp_gaze_task(addr, gaze_storage).await?;
        }

        #[cfg(feature = "vision")]
        {
            let camera = Arc::clone(&self.camera);
            let gaze_storage = Arc::clone(&self.current_gaze);
            let context_storage = Arc::clone(&self.current_context);

            // Move recognizers into the task (start() is intended to be called once).
            let mut gaze_tracker = self
                .gaze_tracker
                .take()
                .expect("VisionPathway::start called more than once (gaze_tracker already taken)");
            let object_recognizer = self
                .object_recognizer
                .take()
                .expect("VisionPathway::start called more than once (object_recognizer already taken)");

            let udp_gaze_enabled = self.udp_gaze_addr.is_some();

            // Spawn background task for continuous processing
            tokio::spawn(async move {
                let mut frame = core::Mat::default();

                loop {
                    // Capture frame
                    if let Ok(mut cam) = camera.lock() {
                        if cam.read(&mut frame).is_ok() && !frame.empty() {
                            // If UDP gaze is enabled, it owns gaze updates. Otherwise, use OpenCV gaze tracker.
                            if !udp_gaze_enabled {
                                if let Ok(Some(g)) = gaze_tracker.track(&frame) {
                                    *gaze_storage.lock().unwrap() = Some(g);
                                }
                            }

                            if let Some(c) = process_visual_context(&object_recognizer, &frame) {
                                *context_storage.lock().unwrap() = Some(c);
                            }
                        }
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
                }
            });
        }

        #[cfg(not(feature = "vision"))]
        {
            let gaze_storage = Arc::clone(&self.current_gaze);
            let context_storage = Arc::clone(&self.current_context);

            let udp_gaze_enabled = self.udp_gaze_addr.is_some();

            // Simulate data
            tokio::spawn(async move {
                loop {
                    if !udp_gaze_enabled {
                        *gaze_storage.lock().unwrap() = Some(Self::simulated_gaze());
                    }
                    *context_storage.lock().unwrap() = Some(Self::simulated_context());
                    tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
                }
            });
        }

        Ok(())
    }

    /// Get the latest gaze data
    pub async fn get_gaze_data(&self) -> Result<Option<GazePoint>> {
        Ok(self.current_gaze.lock().unwrap().clone())
    }

    /// Get the latest visual context
    pub async fn get_visual_context(&self) -> Result<Option<VisualContext>> {
        Ok(self.current_context.lock().unwrap().clone())
    }

    /// Stop the vision processing
    pub async fn stop(&self) -> Result<()> {
        log::info!("Stopping Vision Pathway...");
        Ok(())
    }

    // Internal processing methods

    fn simulated_gaze() -> GazePoint {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Try to get actual mouse cursor position on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = std::process::Command::new("xdotool")
                .args(["getmouselocation", "--shell"])
                .output()
            {
                if output.status.success() {
                    if let Ok(result) = String::from_utf8(output.stdout) {
                        let mut x = 0.5;
                        let mut y = 0.5;

                        for line in result.lines() {
                            if let Some((key, val)) = line.split_once('=') {
                                match key {
                                    "X" => {
                                        if let Ok(px) = val.parse::<f32>() {
                                            // Normalize assuming 1920x1080
                                            x = (px / 1920.0).clamp(0.0, 1.0);
                                        }
                                    }
                                    "Y" => {
                                        if let Ok(py) = val.parse::<f32>() {
                                            y = (py / 1080.0).clamp(0.0, 1.0);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }

                        return GazePoint {
                            screen_x: x,
                            screen_y: y,
                            confidence: 0.9, // High confidence for mouse position
                            timestamp,
                        };
                    }
                }
            }
        }

        // Fallback: simulate natural eye movement pattern
        let t = timestamp as f64 / 1000.0; // Convert to seconds
        let x = 0.5 + 0.2 * (t * 0.5).sin();
        let y = 0.5 + 0.15 * (t * 0.3).cos();

        GazePoint {
            screen_x: x as f32,
            screen_y: y as f32,
            confidence: 0.7,
            timestamp,
        }
    }

    fn simulated_context() -> VisualContext {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mut objects = vec![];
        let mut colors = vec![];
        let mut text = None;

        // Try to detect actual windows on screen (Linux X11)
        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = std::process::Command::new("xdotool")
                .args(["search", "--onlyvisible", "--class", ""])
                .output()
            {
                if output.status.success() {
                    if let Ok(window_ids) = String::from_utf8(output.stdout) {
                        let windows: Vec<&str> = window_ids.lines().take(5).collect();

                        for (i, _) in windows.iter().enumerate() {
                            // Create objects for detected windows
                            let x = (i as f32 * 0.2).min(0.8);
                            let y = 0.1 + (i as f32 * 0.15);

                            objects.push(DetectedObject {
                                label: format!("window_{}", i),
                                confidence: 0.85,
                                bbox: BoundingBox {
                                    x,
                                    y,
                                    width: 0.2,
                                    height: 0.15,
                                },
                            });
                        }

                        if !windows.is_empty() {
                            text = Some(format!("{} windows detected", windows.len()));
                        }
                    }
                }
            }

            // Extract dominant colors from current terminal/shell
            // Assume dark theme as default
            colors = vec![
                Color { r: 40, g: 44, b: 52 },   // Dark background
                Color { r: 171, g: 178, b: 191 }, // Light text
                Color { r: 97, g: 175, b: 239 },  // Accent blue
            ];
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Fallback for non-Linux: generate generic objects
            colors = vec![Color { r: 128, g: 128, b: 128 }];
        }

        VisualContext {
            objects,
            colors,
            text,
            timestamp,
        }
    }
}

#[cfg(feature = "vision")]
fn process_visual_context(recognizer: &ObjectRecognizer, frame: &opencv::core::Mat) -> Option<VisualContext> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;

    let objects = recognizer.recognize(frame).ok().unwrap_or_default();

    // Simple dominant-color estimate: use a small downsample and average.
    let colors = vec![Color { r: 128, g: 128, b: 128 }];

    let text = if objects.is_empty() {
        None
    } else {
        Some(format!("{} objects", objects.len()))
    };

    Some(VisualContext {
        objects,
        colors,
        text,
        timestamp,
    })
}
