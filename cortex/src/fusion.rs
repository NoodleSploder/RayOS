//! Audio-Visual Fusion module
//!
//! Combines gaze, visual context, and audio transcription to create
//! a unified understanding of user intent.
//!
//! Example: "Delete that" (audio) + looking at file icon (vision) = Delete file

use crate::types::{GazePoint, VisualContext, FusedContext, DetectedObject};
use std::collections::VecDeque;

/// Manages multimodal context fusion
pub struct AudioVisualFusion {
    /// Recent gaze history for temporal context
    gaze_history: VecDeque<GazePoint>,
    /// Recent audio transcripts
    audio_buffer: VecDeque<String>,
    /// Maximum history length
    max_history: usize,
}

impl AudioVisualFusion {
    pub fn new() -> Self {
        Self {
            gaze_history: VecDeque::with_capacity(60), // 1 second at 60Hz
            audio_buffer: VecDeque::with_capacity(10),
            max_history: 60,
        }
    }

    /// Fuse gaze and visual context
    pub fn fuse(&mut self, gaze: GazePoint, visual: VisualContext) -> FusedContext {
        // Add to history
        self.gaze_history.push_back(gaze);
        if self.gaze_history.len() > self.max_history {
            self.gaze_history.pop_front();
        }

        // Get most recent audio (if any)
        let audio_transcript = self.audio_buffer.back().cloned();

        FusedContext {
            gaze,
            visual,
            audio_transcript,
        }
    }

    /// Add audio transcript to buffer
    pub fn add_audio(&mut self, transcript: String) {
        self.audio_buffer.push_back(transcript);
        if self.audio_buffer.len() > 10 {
            self.audio_buffer.pop_front();
        }
    }

    /// Resolve deictic references (e.g., "that", "it", "this")
    /// by correlating gaze position with detected objects
    pub fn resolve_reference(&self, pronoun: &str, context: &FusedContext) -> Option<String> {
        if !["that", "it", "this", "these", "those"].contains(&pronoun) {
            return None;
        }

        // Find object closest to gaze point
        let gaze_x = context.gaze.screen_x;
        let gaze_y = context.gaze.screen_y;

        let closest = context.visual.objects.iter()
            .min_by(|a, b| {
                let dist_a = self.distance_to_gaze(a, gaze_x, gaze_y);
                let dist_b = self.distance_to_gaze(b, gaze_x, gaze_y);
                dist_a.partial_cmp(&dist_b).unwrap()
            });

        closest.map(|obj| obj.label.clone())
    }

    /// Calculate distance from object center to gaze point
    fn distance_to_gaze(&self, obj: &DetectedObject, gaze_x: f32, gaze_y: f32) -> f32 {
        let obj_center_x = obj.bbox.x + obj.bbox.width / 2.0;
        let obj_center_y = obj.bbox.y + obj.bbox.height / 2.0;

        let dx = obj_center_x - gaze_x;
        let dy = obj_center_y - gaze_y;

        (dx * dx + dy * dy).sqrt()
    }

    /// Get gaze fixation point (where user has been looking)
    pub fn get_fixation_point(&self) -> Option<(f32, f32)> {
        if self.gaze_history.is_empty() {
            return None;
        }

        // Calculate average gaze position over recent history
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let count = self.gaze_history.len() as f32;

        for gaze in &self.gaze_history {
            sum_x += gaze.screen_x;
            sum_y += gaze.screen_y;
        }

        Some((sum_x / count, sum_y / count))
    }

    /// Detect if user is fixated on a particular area
    pub fn is_fixated(&self, threshold: f32) -> bool {
        if self.gaze_history.len() < 30 { // Need at least 0.5s of data
            return false;
        }

        // Calculate variance of recent gaze positions
        let (mean_x, mean_y) = match self.get_fixation_point() {
            Some(point) => point,
            None => return false,
        };

        let mut variance = 0.0;
        for gaze in &self.gaze_history {
            let dx = gaze.screen_x - mean_x;
            let dy = gaze.screen_y - mean_y;
            variance += dx * dx + dy * dy;
        }

        variance /= self.gaze_history.len() as f32;

        // Low variance means fixation
        variance < threshold
    }
}

impl Default for AudioVisualFusion {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BoundingBox;

    #[test]
    fn test_resolve_reference() {
        let fusion = AudioVisualFusion::new();

        let gaze = GazePoint {
            screen_x: 0.5,
            screen_y: 0.5,
            confidence: 0.9,
            timestamp: 0,
        };

        let visual = VisualContext {
            objects: vec![
                DetectedObject {
                    label: "cup".to_string(),
                    confidence: 0.9,
                    bbox: BoundingBox {
                        x: 0.45,
                        y: 0.45,
                        width: 0.1,
                        height: 0.1,
                    },
                },
            ],
            colors: vec![],
            text: None,
            timestamp: 0,
        };

        let context = FusedContext {
            gaze,
            visual,
            audio_transcript: Some("Delete that".to_string()),
        };

        let resolved = fusion.resolve_reference("that", &context);
        assert_eq!(resolved, Some("cup".to_string()));
    }
}
