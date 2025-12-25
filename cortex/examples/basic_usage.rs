//! Example: Basic usage of the Cortex API

use rayos_cortex::Cortex;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Create and start Cortex
    let mut cortex = Cortex::new().await?;

    println!("Cortex is running!");
    println!("Look around and try saying commands like:");
    println!("  - 'Select this' (while looking at an object)");
    println!("  - 'Delete that' (while looking at a file)");
    println!("  - Hold a coffee cup to enter break mode");

    // Run the main loop
    cortex.run().await?;

    Ok(())
}
