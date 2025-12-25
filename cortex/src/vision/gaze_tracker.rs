//! Gaze tracking implementation
//!
//! This module handles eye tracking and gaze point estimation.
//! In production, this would integrate with hardware like Tobii or use
//! MediaPipe for software-based eye tracking.

use crate::types::GazePoint;
#[cfg(feature = "vision")]
use opencv::{core, objdetect, prelude::*, imgproc};
use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};

pub struct GazeTracker {
    #[cfg(feature = "vision")]
    camera: Arc<Mutex<opencv::videoio::VideoCapture>>,
    #[cfg(feature = "vision")]
    face_cascade: objdetect::CascadeClassifier,
    #[cfg(feature = "vision")]
    eye_cascade: objdetect::CascadeClassifier,
}

impl GazeTracker {
    #[cfg(feature = "vision")]
    pub async fn new(camera: Arc<Mutex<opencv::videoio::VideoCapture>>) -> Result<Self> {
        log::info!("Initializing Gaze Tracker...");

        // Load Haar Cascades for face and eye detection
        // In a real deployment, these would be downloaded or bundled with the app
        let face_cascade = objdetect::CascadeClassifier::new(
            "/usr/share/opencv4/haarcascades/haarcascade_frontalface_default.xml"
        ).unwrap_or_else(|_| {
            log::warn!("Could not load face cascade, using stub");
            objdetect::CascadeClassifier::default()
        });

        let eye_cascade = objdetect::CascadeClassifier::new(
            "/usr/share/opencv4/haarcascades/haarcascade_eye.xml"
        ).unwrap_or_else(|_| {
            log::warn!("Could not load eye cascade, using stub");
            objdetect::CascadeClassifier::default()
        });

        Ok(Self {
            camera,
            face_cascade,
            eye_cascade,
        })
    }

    #[cfg(not(feature = "vision"))]
    pub async fn new_stub() -> Result<Self> {
        log::info!("Initializing Gaze Tracker (stub mode)...");
        Ok(Self {})
    }

    #[cfg(feature = "vision")]
    /// Track gaze from a video frame
    pub fn track(&mut self, frame: &core::Mat) -> Result<Option<GazePoint>> {
        // Convert to grayscale
        let mut gray = core::Mat::default();
        imgproc::cvt_color(frame, &mut gray, imgproc::COLOR_BGR2GRAY, 0)
            .context("Failed to convert to grayscale")?;

        // Detect faces
        let mut faces = core::Vector::<core::Rect>::new();
        if self.face_cascade.detect_multi_scale(
            &gray,
            &mut faces,
            1.1,
            3,
            0,
            core::Size::new(30, 30),
            core::Size::new(0, 0),
        ).is_err() {
            return Ok(None);
        }

        if faces.is_empty() {
            return Ok(None);
        }

        // Use the first detected face
        let face = faces.get(0)?;

        // Extract face region
        let face_roi = core::Mat::roi(&gray, face)?;

        // Detect eyes in face region
        let mut eyes = core::Vector::<core::Rect>::new();
        if self.eye_cascade.detect_multi_scale(
            &face_roi,
            &mut eyes,
            1.1,
            3,
            0,
            core::Size::new(20, 20),
            core::Size::new(0, 0),
        ).is_err() || eyes.is_empty() {
            // No eyes detected, estimate from face center
            return Ok(Some(self.estimate_gaze_from_face(&face, frame)));
        }

        // Calculate gaze point from eye positions
        Ok(Some(self.calculate_gaze_point(&face, &eyes, frame)))
    }

    #[cfg(feature = "vision")]
    fn estimate_gaze_from_face(&self, face: &core::Rect, frame: &core::Mat) -> GazePoint {
        let frame_width = frame.cols() as f32;
        let frame_height = frame.rows() as f32;

        // Estimate gaze from face center
        let gaze_x = (face.x + face.width / 2) as f32 / frame_width;
        let gaze_y = (face.y + face.height / 2) as f32 / frame_height;

        GazePoint {
            screen_x: gaze_x.clamp(0.0, 1.0),
            screen_y: gaze_y.clamp(0.0, 1.0),
            confidence: 0.5, // Low confidence without eyes
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
        #[cfg(feature = "vision")]    fn calculate_gaze_point(
        &self,
        face: &core::Rect,
        eyes: &core::Vector<core::Rect>,
        frame: &core::Mat,
    ) -> GazePoint {
        let frame_width = frame.cols() as f32;
        let frame_height = frame.rows() as f32;

        // Calculate average eye position
        let mut eye_x = face.x;
        let mut eye_y = face.y;
        let eye_count = eyes.len();

        for i in 0..eye_count {
            if let Ok(eye) = eyes.get(i) {
                eye_x += eye.x + eye.width / 2;
                eye_y += eye.y + eye.height / 2;
            }
        }

        eye_x /= eye_count as i32;
        eye_y /= eye_count as i32;

        // Normalize to screen coordinates
        let gaze_x = eye_x as f32 / frame_width;
        let gaze_y = eye_y as f32 / frame_height;

        GazePoint {
            screen_x: gaze_x.clamp(0.0, 1.0),
            screen_y: gaze_y.clamp(0.0, 1.0),
            confidence: 0.8, // Higher confidence with eyes detected
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}
