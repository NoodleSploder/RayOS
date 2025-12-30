//! Example: Deictic reference resolution with gaze context
//!
//! Shows how Intent resolves "that", "this", "it" using visual context.

use rayos_intent::*;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Deictic Resolution Example ===\n");

    // Create engine
    let config = IntentConfig::default();
    let engine = IntentEngine::new(config);

    // Simulate visual objects on screen
    let objects = vec![
        VisualObject {
            id: "file_123".to_string(),
            object_type: "file".to_string(),
            bounds: (100.0, 100.0, 200.0, 50.0),
            properties: {
                let mut props = HashMap::new();
                props.insert("name".to_string(), "important.rs".to_string());
                props
            },
        },
        VisualObject {
            id: "folder_456".to_string(),
            object_type: "folder".to_string(),
            bounds: (100.0, 200.0, 200.0, 50.0),
            properties: {
                let mut props = HashMap::new();
                props.insert("name".to_string(), "src".to_string());
                props
            },
        },
    ];

    engine.update_visual_objects(objects);

    // User looks at important.rs
    println!("👁️  User gazes at (150, 125) - 'important.rs'");
    engine.update_gaze((150.0, 125.0), Some("file_123".to_string()));

    // User says "delete that"
    println!("🎤 User says: \"delete that\"\n");
    let result = engine.parse("delete that");

    println!("Parsed Intent:");
    if let Command::Delete { target } = result.intent.command {
        match target {
            Target::Deictic { gaze_position, object_id } => {
                println!("  ✓ Resolved deictic reference:");
                if let Some(pos) = gaze_position {
                    println!("    Gaze position: ({:.1}, {:.1})", pos.0, pos.1);
                }
                if let Some(id) = object_id {
                    println!("    Object ID: {}", id);
                }
            }
            _ => println!("  ✗ Failed to resolve deictic reference"),
        }
    }

    println!("  Confidence: {:.2}", result.intent.confidence);
    println!();

    // Now user looks at folder
    println!("👁️  User gazes at (150, 225) - 'src' folder");
    engine.update_gaze((150.0, 225.0), Some("folder_456".to_string()));

    // User says "open this"
    println!("🎤 User says: \"go to this\"\n");
    let result2 = engine.parse("go to this");

    println!("Parsed Intent:");
    println!("  Command Type: {:?}",
        match result2.intent.command {
            Command::Navigate { .. } => "Navigate",
            _ => "Other",
        }
    );
    println!("  Confidence: {:.2}", result2.intent.confidence);

    Ok(())
}
