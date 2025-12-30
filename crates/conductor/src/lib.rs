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

pub use types::*;

// The async daemon/orchestrator implementation is still under active construction.
// Keep it feature-gated so the crate can compile in a minimal mode while we build
// the host-side bridge and protocol.
#[cfg(feature = "daemon")]
pub mod entropy_monitor;
#[cfg(feature = "daemon")]
pub mod task_orchestrator;
#[cfg(feature = "daemon")]
pub mod ouroboros;

#[cfg(feature = "daemon")]
pub use entropy_monitor::EntropyMonitor;
#[cfg(feature = "daemon")]
pub use entropy_monitor::LatencyViolation;
#[cfg(feature = "daemon")]
pub use task_orchestrator::{TaskOrchestrator, OrchestratorStatistics};
#[cfg(feature = "daemon")]
pub use ouroboros::{OuroborosEngine, OuroborosStatistics};

#[cfg(feature = "daemon")]
use anyhow::Result;
#[cfg(feature = "daemon")]
use std::sync::Arc;
#[cfg(feature = "daemon")]
use tokio::signal;

/// Main conductor system - orchestrates all Phase 4 components
#[cfg(feature = "daemon")]
pub struct Conductor {
    config: ConductorConfig,
    monitor: Arc<EntropyMonitor>,
    orchestrator: Arc<TaskOrchestrator>,
    ouroboros: Arc<OuroborosEngine>,
}

#[cfg(feature = "daemon")]
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

        let ouroboros = Arc::new(OuroborosEngine::new());
        ouroboros.set_enabled(config.enable_ouroboros);

        let orchestrator = Arc::new(TaskOrchestrator::new(
            config.clone(),
            monitor.clone(),
            ouroboros.clone(),
        )?);

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
        let orchestrator_clone = self.orchestrator.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;

                let state = monitor.dream_state();
                if state == DreamState::Dreaming {
                    log::info!("Entering Dream Mode - starting self-optimization");

                    let optimization_tasks = vec![
                        Task::new(Priority::Dream, TaskPayload::Optimize { target: OptimizationTarget::System }),
                        Task::new(Priority::Dream, TaskPayload::Optimize { target: OptimizationTarget::System }),
                        Task::new(Priority::Dream, TaskPayload::Optimize { target: OptimizationTarget::System }),
                    ];

                    for task in optimization_tasks {
                        if let Err(e) = orchestrator_clone.submit(task) {
                            log::error!("Failed to submit optimization task: {}", e);
                        }
                    }
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

    /// Get recent latency violations from the watchdog.
    pub fn get_recent_violations(&self, count: usize) -> Vec<LatencyViolation> {
        self.monitor.get_violations(count)
    }
}
