//! Core data types for the Cortex system

use serde::{Deserialize, Serialize};

/// Represents a gaze point on the screen
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GazePoint {
    /// X coordinate on screen (normalized 0.0-1.0)
    pub screen_x: f32,
    /// Y coordinate on screen (normalized 0.0-1.0)
    pub screen_y: f32,
    /// Confidence of the gaze detection (0.0-1.0)
    pub confidence: f32,
    /// Timestamp in milliseconds
    pub timestamp: u64,
}

/// Visual context extracted from the screen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualContext {
    /// Detected objects in the view
    pub objects: Vec<DetectedObject>,
    /// Dominant colors in the scene
    pub colors: Vec<Color>,
    /// Text detected via OCR
    pub text: Option<String>,
    /// Timestamp of capture
    pub timestamp: u64,
}

/// A detected object in the visual field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedObject {
    /// Object class/label
    pub label: String,
    /// Detection confidence (0.0-1.0)
    pub confidence: f32,
    /// Bounding box (normalized coordinates)
    pub bbox: BoundingBox,
}

/// Bounding box for detected objects
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// RGB color
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Fused multimodal context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedContext {
    pub gaze: GazePoint,
    pub visual: VisualContext,
    pub audio_transcript: Option<String>,
}

/// User intent parsed from context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Intent {
    /// User wants to select something
    Select { target: String },
    /// User wants to move something
    Move { source: String, destination: String },
    /// User wants to delete something
    Delete { target: String },
    /// User wants to create something
    Create { object_type: String },
    /// User is in "break mode" (holding coffee, etc.)
    Break,
    /// No clear intent detected
    Idle,
}

/// Configuration for the Cortex system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CortexConfig {
    /// Camera device index
    pub camera_index: i32,
    /// Path to LLM model weights
    pub model_path: String,
    /// Target frames per second for vision processing
    pub target_fps: u32,
    /// Enable debug visualizations
    pub debug_mode: bool,
}

impl Default for CortexConfig {
    fn default() -> Self {
        Self {
            camera_index: 0,
            model_path: "models/llama-7b.gguf".to_string(),
            target_fps: 60,
            debug_mode: false,
        }
    }
}
