//! RayOS Conductor CLI
//!
//! For now we ship a minimal, reliable host-side bridge for querying RayOS
//! from headless QEMU runs. The full daemon/orchestrator is feature-gated
//! behind `--features daemon`.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg(feature = "daemon")]
use rayos_conductor::{
    Conductor, ConductorConfig, Priority, Task, TaskPayload, OptimizationTarget,
};

#[cfg(all(feature = "daemon", feature = "intent"))]
use rayos_intent::{Command as IntentCommand, IntentConfig, IntentEngine, Priority as IntentPriority};

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
    /// Fetch a machine-readable snapshot from a headless QEMU instance
    ///
    /// Connects to a QEMU HMP monitor unix socket, injects the keystrokes
    /// for `:conductor snapshot`, then parses the latest matching line
    /// from the serial log file.
    QemuSnapshot {
        /// QEMU monitor unix socket path (HMP)
        #[arg(long, value_name = "PATH")]
        monitor_sock: PathBuf,

        /// Serial log path produced by QEMU (-serial file:...)
        #[arg(long, value_name = "PATH")]
        serial_log: PathBuf,

        /// How long to wait for the snapshot line (ms)
        #[arg(long, default_value = "1500")]
        timeout_ms: u64,
    },

    /// Submit text to RayOS System 2 via `:conductor submit <text>`
    ///
    /// Sends the command via the QEMU HMP monitor socket and waits until the
    /// `conductor submit ...` result line shows up in the serial log.
    QemuSubmit {
        /// QEMU monitor unix socket path (HMP)
        #[arg(long, value_name = "PATH")]
        monitor_sock: PathBuf,

        /// Serial log path produced by QEMU (-serial file:...)
        #[arg(long, value_name = "PATH")]
        serial_log: PathBuf,

        /// Text to submit to System 2
        text: String,

        /// How long to wait for the result line (ms)
        #[arg(long, default_value = "1500")]
        timeout_ms: u64,
    },

    /// Run an arbitrary RayOS shell command (prefixed with ':') via QEMU monitor sendkey
    /// and wait for an expected substring to appear in the serial log.
    QemuShell {
        /// QEMU monitor unix socket path (HMP)
        #[arg(long, value_name = "PATH")]
        monitor_sock: PathBuf,

        /// Serial log path produced by QEMU (-serial file:...)
        #[arg(long, value_name = "PATH")]
        serial_log: PathBuf,

        /// Shell command to run (without leading ':')
        cmd: String,

        /// Substring to wait for in the serial log
        #[arg(long, default_value = "")]
        expect: String,

        /// How long to wait for the expected substring (ms)
        #[arg(long, default_value = "1500")]
        timeout_ms: u64,
    },

    #[cfg(feature = "daemon")]
    /// Start the conductor daemon
    Start {
        /// Dream mode threshold (seconds of idle before self-optimization)
        #[arg(short = 'd', long, default_value = "300")]
        dream_threshold: u64,
    },

    #[cfg(feature = "daemon")]
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

    #[cfg(feature = "daemon")]
    /// Show system statistics
    Stats,

    #[cfg(feature = "daemon")]
    /// Trigger optimization cycle
    Optimize {
        /// Target function name
        #[arg(short = 'f', long)]
        function: Option<String>,
    },

    #[cfg(feature = "daemon")]
    /// Show recent latency violations
    Violations {
        /// Number of recent violations to show
        #[arg(short = 'n', long, default_value = "10")]
        count: usize,
    },

    #[cfg(all(feature = "daemon", feature = "intent"))]
    /// Parse natural language via Phase 5 (Intent) and submit as a Conductor task.
    ///
    /// Build with: `cargo run --features "daemon intent" -- intent-submit "..."`
    IntentSubmit {
        /// Text to parse (natural language)
        text: String,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::QemuSnapshot { monitor_sock, serial_log, timeout_ms } => {
            let line = qemu_conductor_snapshot(&monitor_sock, &serial_log, Duration::from_millis(timeout_ms))?;
            println!("{}", line);
        }

        Commands::QemuSubmit { monitor_sock, serial_log, text, timeout_ms } => {
            let line = qemu_conductor_submit(&monitor_sock, &serial_log, &text, Duration::from_millis(timeout_ms))?;
            println!("{}", line);
        }

        Commands::QemuShell { monitor_sock, serial_log, cmd, expect, timeout_ms } => {
            let line = qemu_shell_cmd(&monitor_sock, &serial_log, &cmd, &expect, Duration::from_millis(timeout_ms))?;
            println!("{}", line);
        }

        #[cfg(feature = "daemon")]
        other => {
            // Initialize logging only for daemon mode (keeps the minimal bridge quiet).
            env_logger::Builder::from_default_env()
                .filter_level(log::LevelFilter::Info)
                .init();

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

            match other {
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
                        println!("\nBottleneck detected: {:?}", bottleneck);
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
                }
                Commands::Optimize { function } => {
                    let target = if let Some(func_name) = function {
                        let binary = std::env::current_exe()
                            .ok()
                            .and_then(|p| std::fs::read(&p).ok())
                            .unwrap_or_else(|| vec![0u8; 0]);

                        OptimizationTarget::Function { name: func_name.clone(), binary }
                    } else {
                        OptimizationTarget::System
                    };

                    // Run the optimization immediately. Queuing it into the orchestrator
                    // only makes sense when `Start` is running.
                    let ouroboros = rayos_conductor::OuroborosEngine::new();
                    ouroboros.set_enabled(true);
                    ouroboros.optimize_cycle(target).await?;
                    let stats = ouroboros.get_statistics();
                    println!(
                        "✓ Optimization complete (mutations={}, successful={}, active_patches={})",
                        stats.total_mutations,
                        stats.successful_mutations,
                        stats.active_patches
                    );
                }
                Commands::Violations { count } => {
                    let conductor = Conductor::new(config).await?;
                    let violations = conductor.get_recent_violations(count);
                    println!("\n=== Recent Latency Violations ===");
                    for v in violations {
                        println!("{}: {}ms", v.task_name, v.actual_duration.as_millis());
                    }
                }

                #[cfg(feature = "intent")]
                Commands::IntentSubmit { text } => {
                    let conductor = Conductor::new(config).await?;

                    let engine = IntentEngine::new(IntentConfig::default());
                    let parsed = engine.parse(&text);

                    if parsed.needs_clarification {
                        println!(
                            "✗ Needs clarification (confidence: {:.2}): {:?}",
                            parsed.intent.confidence,
                            parsed.intent.command
                        );
                        return Ok(());
                    }

                    let policy = engine.allocate_resources(&parsed.intent);
                    let task = intent_to_task(&parsed.intent.command, policy.priority);

                    println!("Mapped task: priority={:?} payload={:?}", task.priority, task.payload);

                    let task_id = conductor.submit_task(task)?;
                    println!(
                        "✓ Intent submitted (confidence: {:.2}) (Task ID: {})",
                        parsed.intent.confidence,
                        task_id.0
                    );
                }

                _ => unreachable!("non-daemon commands are handled outside the daemon branch"),
            }
        }
    }

    Ok(())
}

#[cfg(all(feature = "daemon", feature = "intent"))]
fn map_intent_priority(priority: IntentPriority) -> Priority {
    match priority {
        IntentPriority::Realtime => Priority::Critical,
        IntentPriority::Interactive => Priority::High,
        IntentPriority::Normal => Priority::Normal,
        IntentPriority::Low => Priority::Low,
        IntentPriority::Idle => Priority::Dream,
    }
}

#[cfg(all(feature = "daemon", feature = "intent"))]
fn intent_to_task(command: &IntentCommand, intent_priority: IntentPriority) -> Task {
    let priority = map_intent_priority(intent_priority);

    let payload = match command {
        IntentCommand::Query { query, .. } => TaskPayload::Search {
            query: query.clone(),
            limit: 25,
        },

        IntentCommand::Create { object_type, properties } => {
            if object_type.to_lowercase().contains("file") {
                if let Some(name) = properties.get("name") {
                    TaskPayload::IndexFile { path: name.into() }
                } else {
                    TaskPayload::Compute {
                        name: format!("intent:create:{}", object_type),
                        estimated_duration: Duration::from_millis(150),
                    }
                }
            } else {
                TaskPayload::Compute {
                    name: format!("intent:create:{}", object_type),
                    estimated_duration: Duration::from_millis(150),
                }
            }
        }

        IntentCommand::Modify { .. } => TaskPayload::Compute {
            name: "intent:modify".to_string(),
            estimated_duration: Duration::from_millis(250),
        },

        IntentCommand::Delete { .. } => TaskPayload::Maintenance {
            task_type: rayos_conductor::MaintenanceType::GarbageCollection,
        },

        IntentCommand::Navigate { destination } => TaskPayload::Compute {
            name: format!("intent:navigate:{}", destination),
            estimated_duration: Duration::from_millis(50),
        },

        IntentCommand::Execute { action, args } => TaskPayload::Compute {
            name: if args.is_empty() {
                format!("intent:exec:{}", action)
            } else {
                format!("intent:exec:{} {}", action, args.join(" "))
            },
            estimated_duration: Duration::from_millis(500),
        },

        IntentCommand::Configure { component, .. } => TaskPayload::Maintenance {
            task_type: match component.to_lowercase().as_str() {
                "cache" => rayos_conductor::MaintenanceType::CacheFlush,
                "metrics" => rayos_conductor::MaintenanceType::MetricsExport,
                _ => rayos_conductor::MaintenanceType::CacheFlush,
            },
        },

        IntentCommand::Sequence { steps } => TaskPayload::Compute {
            name: format!("intent:sequence:{}", steps.len()),
            estimated_duration: Duration::from_millis(750),
        },

        IntentCommand::Ambiguous { question, .. } => TaskPayload::Compute {
            name: format!("intent:ambiguous:{}", question),
            estimated_duration: Duration::from_millis(10),
        },
    };

    Task::new(priority, payload)
}

#[cfg(feature = "daemon")]
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

fn qemu_conductor_snapshot(monitor_sock: &PathBuf, serial_log: &PathBuf, timeout: Duration) -> Result<String> {
    // 1) Inject `:conductor snapshot` via HMP sendkey.
    // ':' in QEMU sendkey is typically `shift-semicolon`.
    let mut cmds: Vec<String> = Vec::new();
    cmds.push("sendkey shift-semicolon".to_string());
    for ch in "conductor snapshot".chars() {
        cmds.push(sendkey_for_char(ch)?);
    }
    cmds.push("sendkey ret".to_string());

    send_hmp_commands(monitor_sock, &cmds)?;

    // 2) Wait for the snapshot to appear in the serial log.
    let start = Instant::now();
    let prefix = "conductor snapshot ";

    loop {
        if let Ok(bytes) = std::fs::read(serial_log) {
            // Normalize CRLF, then scan for the last matching line.
            let s = String::from_utf8_lossy(&bytes).replace('\r', "");
            if let Some(last) = s.lines().rev().find(|l| l.starts_with(prefix)) {
                return Ok(last.to_string());
            }
        }

        if start.elapsed() >= timeout {
            anyhow::bail!(
                "Timed out waiting for snapshot in serial log. monitor_sock={} serial_log={}",
                monitor_sock.display(),
                serial_log.display()
            );
        }

        std::thread::sleep(Duration::from_millis(25));
    }
}

fn qemu_conductor_submit(
    monitor_sock: &PathBuf,
    serial_log: &PathBuf,
    text: &str,
    timeout: Duration,
) -> Result<String> {
    // Inject `:conductor submit <text>` via HMP sendkey.
    let mut cmds: Vec<String> = Vec::new();
    cmds.push("sendkey shift-semicolon".to_string());
    for ch in "conductor submit ".chars() {
        cmds.push(sendkey_for_char(ch)?);
    }
    for ch in text.chars() {
        cmds.push(sendkey_for_char(ch)?);
    }
    cmds.push("sendkey ret".to_string());

    send_hmp_commands(monitor_sock, &cmds)?;

    wait_for_serial_line(serial_log, "conductor submit ", timeout)
}

fn qemu_shell_cmd(
    monitor_sock: &PathBuf,
    serial_log: &PathBuf,
    cmd: &str,
    expect: &str,
    timeout: Duration,
) -> Result<String> {
    // Capture current serial size so we can bias toward new output.
    let before = std::fs::read(serial_log).map(|b| b.len()).unwrap_or(0);

    // Inject `:<cmd>` via HMP sendkey.
    let mut cmds: Vec<String> = Vec::new();
    cmds.push("sendkey shift-semicolon".to_string());
    for ch in cmd.chars() {
        cmds.push(sendkey_for_char(ch)?);
    }
    cmds.push("sendkey ret".to_string());
    send_hmp_commands(monitor_sock, &cmds)?;

    let start = Instant::now();
    loop {
        if let Ok(bytes) = std::fs::read(serial_log) {
            let slice = if bytes.len() > before { &bytes[before..] } else { &bytes[..] };
            let s = String::from_utf8_lossy(slice).replace('\r', "");

            if !expect.is_empty() {
                if let Some(last) = s.lines().rev().find(|l| l.contains(expect)) {
                    return Ok(last.to_string());
                }
            } else if let Some(last) = s.lines().rev().find(|l| !l.trim().is_empty()) {
                return Ok(last.to_string());
            }
        }

        if start.elapsed() >= timeout {
            anyhow::bail!(
                "Timed out waiting for expect={:?} in serial log. monitor_sock={} serial_log={}",
                expect,
                monitor_sock.display(),
                serial_log.display()
            );
        }

        std::thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_serial_line(serial_log: &PathBuf, prefix: &str, timeout: Duration) -> Result<String> {
    let start = Instant::now();
    loop {
        if let Ok(bytes) = std::fs::read(serial_log) {
            let s = String::from_utf8_lossy(&bytes).replace('\r', "");
            if let Some(last) = s.lines().rev().find(|l| l.starts_with(prefix)) {
                return Ok(last.to_string());
            }
        }
        if start.elapsed() >= timeout {
            anyhow::bail!("Timed out waiting for line prefix {:?} in serial log {}", prefix, serial_log.display());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn send_hmp_commands(sock_path: &PathBuf, commands: &[String]) -> Result<()> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(sock_path)
        .map_err(|e| anyhow::anyhow!("Failed to connect to QEMU monitor socket {}: {e}", sock_path.display()))?;

    // Best-effort read banner/prompt (ignore errors/timeouts).
    stream.set_read_timeout(Some(Duration::from_millis(50))).ok();
    let mut buf = [0u8; 4096];
    let _ = stream.read(&mut buf);

    for cmd in commands {
        stream.write_all(cmd.as_bytes())?;
        stream.write_all(b"\r\n")?;
        stream.flush()?;
        std::thread::sleep(Duration::from_millis(35));
    }

    Ok(())
}

fn sendkey_for_char(ch: char) -> Result<String> {
    // Minimal mapping for this phase.
    // Keep this intentionally small; add keys as the protocol evolves.
    match ch {
        'a'..='z' => Ok(format!("sendkey {}", ch)),
        'A'..='Z' => Ok(format!("sendkey shift-{}", ch.to_ascii_lowercase() as char)),
        '0'..='9' => Ok(format!("sendkey {}", ch)),
        ' ' => Ok("sendkey spc".to_string()),
        '-' => Ok("sendkey minus".to_string()),
        '_' => Ok("sendkey shift-minus".to_string()),
        '.' => Ok("sendkey dot".to_string()),
        ',' => Ok("sendkey comma".to_string()),
        '/' => Ok("sendkey slash".to_string()),
        '?' => Ok("sendkey shift-slash".to_string()),
        ':' => Ok("sendkey shift-semicolon".to_string()),
        _ => anyhow::bail!("Unsupported character for sendkey mapping: {:?}", ch),
    }
}
