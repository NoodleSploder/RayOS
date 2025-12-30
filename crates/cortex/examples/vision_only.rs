//! Example: Using individual Cortex components

use rayos_cortex::vision::VisionPathway;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Initialize just the vision pathway
    let mut vision = VisionPathway::new().await?;

    println!("Starting vision processing...");
    vision.start().await?;

    // Poll for gaze data
    for _ in 0..100 {
        if let Some(gaze) = vision.get_gaze_data().await? {
            println!(
                "Gaze: ({:.3}, {:.3}) confidence: {:.2}",
                gaze.screen_x, gaze.screen_y, gaze.confidence
            );
        }

        if let Some(context) = vision.get_visual_context().await? {
            println!("Objects: {:?}", context.objects.len());
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    vision.stop().await?;
    println!("Vision processing stopped.");

    Ok(())
}
