//! RayOS Intent - CLI Interface
//!
//! Command-line interface for testing and interacting with the Intent system.

use rayos_intent::*;
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("RayOS Intent - Phase 5: Natural Language Understanding");
    println!("========================================================\n");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "parse" => {
                if args.len() < 3 {
                    eprintln!("Usage: {} parse \"<command>\"", args[0]);
                    return Ok(());
                }
                parse_command(&args[2..])?;
            }
            "repl" => {
                run_repl()?;
            }
            "test" => {
                run_tests()?;
            }
            "info" => {
                show_info()?;
            }
            "--help" | "-h" => {
                show_help();
            }
            _ => {
                eprintln!("Unknown command: {}", args[1]);
                show_help();
            }
        }
    } else {
        show_help();
    }

    Ok(())
}

/// Parse a single command
fn parse_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let input = args.join(" ");

    println!("Input: {}\n", input);

    // Create engine
    let config = IntentConfig::default();
    let engine = IntentEngine::new(config);

    // Parse
    let result = engine.parse(&input);

    // Display results
    println!("Intent ID: {:?}", result.intent.id);
    println!("Command: {:?}", result.intent.command);
    println!("Confidence: {:.2}", result.intent.confidence);
    println!("Needs Clarification: {}", result.needs_clarification);

    if !result.alternatives.is_empty() {
        println!("\nAlternatives:");
        for alt in &result.alternatives {
            println!("  - {:?}", alt.command);
        }
    }

    Ok(())
}

/// Run interactive REPL
fn run_repl() -> Result<(), Box<dyn std::error::Error>> {
    println!("Interactive Intent Parser (type 'exit' to quit)\n");

    // Create engine
    let config = IntentConfigBuilder::new()
        .enable_llm(false)  // Simulated mode
        .confidence_threshold(0.7)
        .build();

    let engine = IntentEngine::new(config);

    println!("{}\n", engine.info());

    loop {
        // Prompt
        print!("> ");
        io::stdout().flush()?;

        // Read input
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        // Check for exit
        if input == "exit" || input == "quit" {
            break;
        }

        if input.is_empty() {
            continue;
        }

        // Handle special commands
        match input {
            "info" => {
                println!("{}\n", engine.info());
                continue;
            }
            "gaze" => {
                // Simulate gaze update
                engine.update_gaze((100.0, 200.0), Some("test.rs".to_string()));
                println!("Gaze updated to (100, 200) on test.rs\n");
                continue;
            }
            "load" => {
                println!("System Load: {:.2}\n", engine.get_load_factor());
                continue;
            }
            "help" => {
                show_repl_help();
                continue;
            }
            _ => {}
        }

        // Parse intent
        let result = engine.parse(input);

        // Display
        println!("  Command: {:?}", result.intent.command);
        println!("  Confidence: {:.2}", result.intent.confidence);

        if result.needs_clarification {
            println!("  ⚠ Needs clarification");
        } else {
            println!("  ✓ Ready to execute");
        }

        println!();
    }

    println!("Goodbye!");
    Ok(())
}

/// Run test suite
fn run_tests() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running Intent Parser Tests\n");

    let config = IntentConfig::default();
    let engine = IntentEngine::new(config);

    let test_cases = vec![
        "find all rust files",
        "create file named test.rs",
        "delete that file",
        "rename this to new_name.rs",
        "go to home directory",
        "run cargo build",
        "show me recent files",
    ];

    let mut passed = 0;
    let total = test_cases.len();

    for (i, test) in test_cases.iter().enumerate() {
        print!("Test {}: \"{}\" ... ", i + 1, test);

        let result = engine.parse(test);

        if result.intent.confidence > 0.5 {
            println!("✓ PASS (confidence: {:.2})", result.intent.confidence);
            passed += 1;
        } else {
            println!("✗ FAIL (confidence: {:.2})", result.intent.confidence);
        }
    }

    println!("\nResults: {}/{} passed ({:.1}%)",
        passed, total, (passed as f32 / total as f32) * 100.0);

    Ok(())
}

/// Show engine info
fn show_info() -> Result<(), Box<dyn std::error::Error>> {
    let config = IntentConfig::default();
    let engine = IntentEngine::new(config.clone());

    println!("{}", engine.info());
    println!();
    println!("Configuration:");
    println!("  Confidence Threshold: {:.2}", config.confidence_threshold);
    println!("  Fusion Enabled: {}", config.enable_fusion);
    println!("  Policy Enforcement: {}", config.enforce_policy);

    Ok(())
}

/// Show help message
fn show_help() {
    println!("RayOS Intent - Natural Language Understanding\n");
    println!("USAGE:");
    println!("  rayos-intent <COMMAND>\n");
    println!("COMMANDS:");
    println!("  parse <text>   Parse a single command");
    println!("  repl           Start interactive REPL");
    println!("  test           Run test suite");
    println!("  info           Show engine information");
    println!("  help           Show this help message\n");
    println!("EXAMPLES:");
    println!("  rayos-intent parse \"find all rust files\"");
    println!("  rayos-intent repl");
    println!("  rayos-intent test");
}

/// Show REPL help
fn show_repl_help() {
    println!("REPL Commands:");
    println!("  info    - Show engine information");
    println!("  gaze    - Simulate gaze update");
    println!("  load    - Show system load");
    println!("  help    - Show this help");
    println!("  exit    - Exit REPL");
    println!();
}
