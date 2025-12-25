//! Example: Testing the Audio-Visual Fusion

use rayos_cortex::fusion::AudioVisualFusion;
use rayos_cortex::types::{GazePoint, VisualContext, DetectedObject, BoundingBox, Color};

fn main() {
    let mut fusion = AudioVisualFusion::new();

    // Simulate looking at a file icon
    let gaze = GazePoint {
        screen_x: 0.25,
        screen_y: 0.30,
        confidence: 0.9,
        timestamp: 0,
    };

    // Simulate detected objects
    let visual = VisualContext {
        objects: vec![
            DetectedObject {
                label: "file_icon".to_string(),
                confidence: 0.95,
                bbox: BoundingBox {
                    x: 0.20,
                    y: 0.25,
                    width: 0.10,
                    height: 0.10,
                },
            },
        ],
        colors: vec![Color { r: 255, g: 255, b: 255 }],
        text: Some("document.txt".to_string()),
        timestamp: 0,
    };

    // Add audio command
    fusion.add_audio("Delete that".to_string());

    // Fuse the context
    let fused = fusion.fuse(gaze, visual);

    // Resolve the reference
    if let Some(target) = fusion.resolve_reference("that", &fused) {
        println!("User wants to delete: {}", target);
        println!("Resolved 'that' to object at gaze point!");
    }

    // Check for fixation
    println!("Is fixated: {}", fusion.is_fixated(0.01));
}
