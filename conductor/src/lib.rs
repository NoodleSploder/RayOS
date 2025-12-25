//! # RayOS Conductor - Phase 4: The Life
//!
//! Autonomous task orchestration and self-optimization system.
//!
//! ## Architecture
//!
//! - **Entropy Monitor**: Detects inefficiency and triggers Dream Mode
//! - **Task Orchestrator**: Work-stealing scheduler across heterogeneous workers
//! - **Ouroboros Engine**: Self-optimization through genetic mutations
//!
//! ## Example
//!
//! ```no_run
//! use rayos_conductor::{Conductor, ConductorConfig};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create conductor with default config
//!     let config = ConductorConfig::default();
//!     let mut conductor = Conductor::new(config).await?;
//!
//!     // Start the system
//!     conductor.start().await?;
//!
//!     Ok(())
//! }
//! ```

pub mod types;
pub mod entropy_monitor;
pub mod task_orchestrator;
pub mod ouroboros;

pub use types::*;
pub use entropy_monitor::EntropyMonitor;
pub use task_orchestrator::{TaskOrchestrator, OrchestratorStatistics};
pub use ouroboros::{OuroborosEngine, OuroborosStatistics};

use anyhow::Result;
use std::sync::Arc;
use tokio::signal;

/// Main conductor system - orchestrates all Phase 4 components
pub struct Conductor {
    config: ConductorConfig,
    monitor: Arc<EntropyMonitor>,
    orchestrator: Arc<TaskOrchestrator>,
    ouroboros: Arc<OuroborosEngine>,
}

impl Conductor {
    /// Create a new conductor system
    pub async fn new(config: ConductorConfig) -> Result<Self> {
        log::info!("═══════════════════════════════════════");
        log::info!("  RayOS Conductor - Phase 4: The Life");
        log::info!("═══════════════════════════════════════");
        log::info!("Worker threads: {}", config.worker_threads);
        log::info!("Dream threshold: {}s", config.dream_threshold_secs);
        log::info!("Ouroboros: {}", if config.enable_ouroboros { "ENABLED ⚠️" } else { "disabled" });

        let monitor = Arc::new(EntropyMonitor::new(
            config.latency_threshold_ms,
            config.dream_threshold_secs,
        ));

        let orchestrator = Arc::new(TaskOrchestrator::new(
            config.clone(),
            monitor.clone(),
        )?);

        let ouroboros = Arc::new(OuroborosEngine::new());
        ouroboros.set_enabled(config.enable_ouroboros);

        Ok(Self {
            config,
            monitor,
            orchestrator,
            ouroboros,
        })
    }

    /// Start the conductor system
    pub async fn start(&mut self) -> Result<()> {
        log::info!("Starting Conductor...");

        // Spawn orchestrator
        let orchestrator = self.orchestrator.clone();
        tokio::spawn(async move {
            if let Err(e) = orchestrator.start().await {
                log::error!("Orchestrator error: {}", e);
            }
        });

        // Spawn dream mode monitor
        let monitor = self.monitor.clone();
        let ouroboros = self.ouroboros.clone();
        let orchestrator_clone = self.orchestrator.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;

                let state = monitor.dream_state();
                match state {
                    DreamState::Dreaming => {
                        log::info!("😴 Entering Dream Mode - starting self-optimization");

                        // Trigger optimization tasks
                        let optimization_tasks = vec![
                            Task {
                                id: TaskId::new(),
                                priority: Priority::Dream,
                                payload: TaskPayload::Optimize {
                                    target: "hot_path".to_string(),
                                },
                            },
                            Task {
                                id: TaskId::new(),
                                priority: Priority::Dream,
                                payload: TaskPayload::Optimize {
                                    target: "memory".to_string(),
                                },
                            },
                            Task {
                                id: TaskId::new(),
                                priority: Priority::Dream,
                                payload: TaskPayload::Optimize {
                                    target: "latency".to_string(),
                                },
                            },
                        ];

                        for task in optimization_tasks {
                            if let Err(e) = orchestrator_clone.submit_task(task) {
                                log::error!("Failed to submit optimization task: {}", e);
                            }
                        }
                    }
                    DreamState::Drowsy => {
                        log::debug!("System approaching idle...");
                    }
                    DreamState::Awake => {}
                }
            }
        });

        // Wait for shutdown signal
        signal::ctrl_c().await?;
        log::info!("Shutdown signal received");

        self.orchestrator.shutdown();

        Ok(())
    }

    /// Submit a task for execution
    pub fn submit_task(&self, task: Task) -> Result<TaskId> {
        self.orchestrator.submit(task)
    }

    /// Submit multiple tasks
    pub fn submit_batch(&self, tasks: Vec<Task>) -> Result<Vec<TaskId>> {
        self.orchestrator.submit_batch(tasks)
    }

    /// Get current system load
    pub fn get_system_load(&self) -> SystemLoad {
        self.orchestrator.get_system_load()
    }

    /// Get orchestrator statistics
    pub fn get_orchestrator_stats(&self) -> OrchestratorStatistics {
        self.orchestrator.get_stats()
    }

    /// Get Ouroboros statistics
    pub fn get_ouroboros_stats(&self) -> OuroborosStatistics {
        self.ouroboros.get_statistics()
    }

    /// Signal user activity (resets idle timer)
    pub fn user_activity(&self) {
        self.monitor.user_activity();
    }
}
