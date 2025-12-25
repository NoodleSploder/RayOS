/// Policy Arbiter - Dynamic resource allocation
///
/// Makes decisions about GPU/CPU resource allocation based on
/// system load and user priority.

use crate::types::SystemMetrics;

/// Resource allocation policy
#[derive(Debug, Clone)]
pub struct ResourcePolicy {
    /// VRAM allocation for System 2 (LLM) in GB
    pub system2_vram_gb: f32,
    /// VRAM allocation for System 1 (Compute) in GB
    pub system1_vram_gb: f32,
    /// Number of worker threads
    pub worker_threads: usize,
    /// Enable aggressive work stealing?
    pub aggressive_stealing: bool,
}

impl ResourcePolicy {
    /// Default balanced policy
    pub fn balanced() -> Self {
        Self {
            system2_vram_gb: 2.0,
            system1_vram_gb: 6.0,
            worker_threads: 4,
            aggressive_stealing: false,
        }
    }

    /// High performance policy (prioritize compute)
    pub fn high_performance() -> Self {
        Self {
            system2_vram_gb: 1.0,
            system1_vram_gb: 10.0,
            worker_threads: 8,
            aggressive_stealing: true,
        }
    }

    /// Power saving policy
    pub fn power_saving() -> Self {
        Self {
            system2_vram_gb: 1.0,
            system1_vram_gb: 2.0,
            worker_threads: 2,
            aggressive_stealing: false,
        }
    }
}

/// Policy decision engine
pub struct PolicyEngine {
    current_policy: ResourcePolicy,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            current_policy: ResourcePolicy::balanced(),
        }
    }

    /// Update policy based on system metrics
    pub fn update_policy(&mut self, metrics: &SystemMetrics) {
        // Decide policy based on entropy and queue depth

        if metrics.entropy > 0.7 {
            // System is struggling, go high performance
            self.current_policy = ResourcePolicy::high_performance();
            log::info!("Switching to high performance policy");
        } else if metrics.entropy < 0.2 && metrics.queue_depth < 100 {
            // System is idle, save power
            self.current_policy = ResourcePolicy::power_saving();
            log::info!("Switching to power saving policy");
        } else {
            // Normal operation
            self.current_policy = ResourcePolicy::balanced();
        }
    }

    /// Get current policy
    pub fn current_policy(&self) -> &ResourcePolicy {
        &self.current_policy
    }

    /// Calculate optimal worker distribution
    pub fn calculate_worker_distribution(&self, total_workers: usize) -> WorkerDistribution {
        // Distribute workers based on current policy
        let compute_workers = (total_workers as f32 * 0.8) as usize;
        let io_workers = total_workers - compute_workers;

        WorkerDistribution {
            compute_workers,
            io_workers,
            total: total_workers,
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Worker distribution result
#[derive(Debug, Clone)]
pub struct WorkerDistribution {
    pub compute_workers: usize,
    pub io_workers: usize,
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SystemMetrics;

    #[test]
    fn test_policy_switching() {
        let mut engine = PolicyEngine::new();

        // High entropy should trigger high performance
        let high_load_metrics = SystemMetrics {
            active_rays: 10000,
            queue_depth: 50000,
            user_present: true,
            entropy: 0.8,
            avg_latency_us: 30000,
        };

        engine.update_policy(&high_load_metrics);
        assert!(engine.current_policy().aggressive_stealing);

        // Low entropy should trigger power saving
        let low_load_metrics = SystemMetrics {
            active_rays: 10,
            queue_depth: 5,
            user_present: false,
            entropy: 0.1,
            avg_latency_us: 100,
        };

        engine.update_policy(&low_load_metrics);
        assert_eq!(engine.current_policy().worker_threads, 2);
    }

    #[test]
    fn test_worker_distribution() {
        let engine = PolicyEngine::new();
        let dist = engine.calculate_worker_distribution(10);

        assert_eq!(dist.total, 10);
        assert!(dist.compute_workers > dist.io_workers);
    }
}
