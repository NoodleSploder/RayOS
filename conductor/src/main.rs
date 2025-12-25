//! RayOS Conductor - Phase 4: The Life
//!
//! Main entry point for the task orchestration and self-optimization daemon.

use rayos_conductor::{
    Conductor, ConductorConfig, Priority, Task, TaskPayload, OptimizationTarget,
};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "conductor")]
#[command(about = "RayOS Conductor - Task Orchestration & Self-Optimization", long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Enable Ouroboros self-optimization (use with caution)
    #[arg(long)]
    enable_ouroboros: bool,

    /// Number of worker threads
    #[arg(short = 'w', long, default_value = "0")]
    workers: usize,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the conductor daemon
    Start {
        /// Dream mode threshold (seconds of idle before self-optimization)
        #[arg(short = 'd', long, default_value = "300")]
        dream_threshold: u64,
    },

    /// Submit a compute task
    Task {
        /// Task name
        name: String,

        /// Priority level (critical, high, normal, low, dream)
        #[arg(short = 'p', long, default_value = "normal")]
        priority: String,

        /// Estimated duration (ms)
        #[arg(short = 'e', long, default_value = "100")]
        duration_ms: u64,
    },

    /// Show system statistics
    Stats,

    /// Trigger optimization cycle
    Optimize {
        /// Target function name
        #[arg(short = 'f', long)]
        function: Option<String>,
    },

    /// Show recent latency violations
    Violations {
        /// Number of recent violations to show
        #[arg(short = 'n', long, default_value = "10")]
        count: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let cli = Cli::parse();

    // Load configuration
    let mut config = if let Some(config_path) = cli.config {
        log::info!("Loading config from: {}", config_path.display());
        let content = std::fs::read_to_string(config_path)?;
        toml::from_str(&content)?
    } else {
        ConductorConfig::default()
    };

    // Override with CLI options
    if cli.workers > 0 {
        config.worker_threads = cli.workers;
    }
    if cli.enable_ouroboros {
        config.enable_ouroboros = true;
    }

    match cli.command {
        Commands::Start { dream_threshold } => {
            config.dream_threshold_secs = dream_threshold;

            let mut conductor = Conductor::new(config).await?;
            conductor.start().await?;
        }

        Commands::Task { name, priority, duration_ms } => {
            let priority = parse_priority(&priority)?;

            let conductor = Conductor::new(config).await?;

            let task = Task::new(
                priority,
                TaskPayload::Compute {
                    name: name.clone(),
                    estimated_duration: Duration::from_millis(duration_ms),
                },
            );

            let task_id = conductor.submit_task(task)?;
            println!("✓ Task submitted: {} (ID: {})", name, task_id.0);
        }

        Commands::Stats => {
            let conductor = Conductor::new(config).await?;

            let load = conductor.get_system_load();
            let orch_stats = conductor.get_orchestrator_stats();
            let ouro_stats = conductor.get_ouroboros_stats();

            println!("\n=== System Load ===");
            println!("Active tasks: {}", load.metrics.active_tasks);
            println!("Pending tasks: {}", load.metrics.pending_tasks);
            println!("CPU usage: {:.1}%", load.metrics.cpu_usage);
            println!("Memory: {:.1} MB", load.metrics.memory_mb);
            println!("Idle duration: {:.1}s", load.metrics.idle_duration.as_secs_f64());

            if let Some(bottleneck) = load.bottleneck {
                println!("\n⚠️ Bottleneck detected: {:?}", bottleneck);
            }

            println!("\n=== Orchestrator ===");
            println!("Total tasks: {}", orch_stats.total_tasks);
            println!("Completed: {}", orch_stats.completed_tasks);
            println!("Failed: {}", orch_stats.failed_tasks);
            println!("Stolen: {} (work stealing)", orch_stats.stolen_tasks);
            println!("Workers: {}", orch_stats.worker_count);

            println!("\n=== Ouroboros (Self-Optimization) ===");
            println!("Total mutations: {}", ouro_stats.total_mutations);
            println!("Successful: {}", ouro_stats.successful_mutations);
            println!("Active patches: {}", ouro_stats.active_patches);
            println!("Avg improvement: {:.2}x", ouro_stats.avg_improvement_factor);

            println!("\n=== Workers ===");
            for worker in &load.workers {
                println!(
                    "Worker {}: {:?}, load={:.2}, completed={}",
                    worker.id.0,
                    worker.worker_type,
                    worker.load_factor,
                    worker.tasks_completed
                );
            }
        }

        Commands::Optimize { function } => {
            let mut conductor = Conductor::new(config).await?;

            if !conductor.get_ouroboros_stats().total_mutations == 0 && config.enable_ouroboros {
                println!("⚠️ Ouroboros is disabled. Use --enable-ouroboros to enable self-optimization.");
                return Ok(());
            }

            let target = if let Some(func_name) = function {
                // Try to locate and read the function binary
                let binary = if let Ok(exe_path) = std::env::current_exe() {
                    // Read current executable
                    std::fs::read(&exe_path).unwrap_or_else(|_| {
                        log::warn!("Could not read executable, using placeholder binary");
                        vec![0u8; 100]
                    })
                } else {
                    log::warn!("Could not locate executable, using placeholder binary");
                    vec![0u8; 100]
                };

                OptimizationTarget::Function {
                    name: func_name.clone(),
                    binary,
                }
            } else {
                OptimizationTarget::System
            };

            println!("🔄 Starting optimization cycle...");

            let task = Task::new(
                Priority::Dream,
                TaskPayload::Optimize { target },
            );

            conductor.submit_task(task)?;
            println!("✓ Optimization task queued");
        }

        Commands::Violations { count } => {
            let conductor = Conductor::new(config).await?;
            let load = conductor.get_system_load();

            // Get violations from monitor (need to expose this in Conductor)
            println!("\n=== Recent Latency Violations ===");
            println!("(Tasks that exceeded {}ms threshold)", config.latency_threshold_ms);
            println!("\nNote: Violation tracking requires daemon to be running");
        }
    }

    Ok(())
}

fn parse_priority(s: &str) -> Result<Priority> {
    match s.to_lowercase().as_str() {
        "critical" => Ok(Priority::Critical),
        "high" => Ok(Priority::High),
        "normal" => Ok(Priority::Normal),
        "low" => Ok(Priority::Low),
        "dream" => Ok(Priority::Dream),
        _ => anyhow::bail!("Invalid priority: {} (must be critical/high/normal/low/dream)", s),
    }
}
