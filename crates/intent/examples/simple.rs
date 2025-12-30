//! Example: Simple command parsing with Intent Engine
//!
//! Demonstrates basic usage of the Intent system without LLM.

use rayos_intent::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Simple Intent Parsing Example ===\n");

    // Create engine with default config (heuristic mode)
    let config = IntentConfig::default();
    let engine = IntentEngine::new(config);

    // Example commands
    let commands = vec![
        "find all rust files",
        "create file named example.rs",
        "delete test.txt",
        "go to home directory",
    ];

    for cmd in commands {
        println!("Input: \"{}\"", cmd);

        // Parse command
        let result = engine.parse(cmd);

        // Display result
        println!("  Command Type: {:?}",
            match result.intent.command {
                Command::Create { .. } => "Create",
                Command::Modify { .. } => "Modify",
                Command::Delete { .. } => "Delete",
                Command::Query { .. } => "Query",
                Command::Navigate { .. } => "Navigate",
                Command::Execute { .. } => "Execute",
                Command::Configure { .. } => "Configure",
                Command::Sequence { .. } => "Sequence",
                Command::Ambiguous { .. } => "Ambiguous",
            }
        );
        println!("  Confidence: {:.2}", result.intent.confidence);
        println!("  Needs Clarification: {}", result.needs_clarification);
        println!();
    }

    Ok(())
}
