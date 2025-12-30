//! Entropy Monitor - The "Hunger Sensor"
//!
//! Measures system inefficiency and triggers Dream Mode when appropriate.

use crate::types::{Bottleneck, DreamState, SystemLoad, SystemMetrics};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::System;

/// The Latency Watchdog - logs processes that exceed threshold
pub struct LatencyWatchdog {
    threshold: Duration,
    violations: Arc<RwLock<VecDeque<LatencyViolation>>>,
    max_history: usize,
}

#[derive(Debug, Clone)]
pub struct LatencyViolation {
    pub timestamp: Instant,
    pub task_name: String,
    pub actual_duration: Duration,
    pub threshold: Duration,
}

impl LatencyWatchdog {
    pub fn new(threshold_ms: u64) -> Self {
        Self {
            threshold: Duration::from_millis(threshold_ms),
            violations: Arc::new(RwLock::new(VecDeque::new())),
            max_history: 1000,
        }
    }

    /// Record a task execution time
    pub fn record(&self, task_name: String, duration: Duration) {
        if duration > self.threshold {
            let violation = LatencyViolation {
                timestamp: Instant::now(),
                task_name,
                actual_duration: duration,
                threshold: self.threshold,
            };

            log::warn!(
                "⏰ Latency violation: {} took {:.2}ms (threshold: {:.2}ms)",
                violation.task_name,
                violation.actual_duration.as_secs_f64() * 1000.0,
                violation.threshold.as_secs_f64() * 1000.0
            );

            let mut violations = self.violations.write();
            violations.push_back(violation);

            // Keep only recent history
            if violations.len() > self.max_history {
                violations.pop_front();
            }
        }
    }

    /// Get recent violations
    pub fn recent_violations(&self, count: usize) -> Vec<LatencyViolation> {
        let violations = self.violations.read();
        violations.iter().rev().take(count).cloned().collect()
    }

    /// Get violation rate (violations per minute)
    pub fn violation_rate(&self) -> f64 {
        let violations = self.violations.read();
        if violations.is_empty() {
            return 0.0;
        }

        let now = Instant::now();
        let one_minute_ago = now - Duration::from_secs(60);

        let recent = violations.iter()
            .filter(|v| v.timestamp > one_minute_ago)
            .count();

        recent as f64
    }
}

/// The Stagnation Timer - detects user absence
pub struct StagnationTimer {
    last_interaction: Arc<RwLock<Instant>>,
    dream_threshold: Duration,
}

impl StagnationTimer {
    pub fn new(threshold_secs: u64) -> Self {
        Self {
            last_interaction: Arc::new(RwLock::new(Instant::now())),
            dream_threshold: Duration::from_secs(threshold_secs),
        }
    }

    /// Signal that user interacted with the system
    pub fn poke(&self) {
        *self.last_interaction.write() = Instant::now();
    }

    /// Get time since last interaction
    pub fn idle_duration(&self) -> Duration {
        Instant::now() - *self.last_interaction.read()
    }

    /// Check current dream state
    pub fn dream_state(&self) -> DreamState {
        let idle = self.idle_duration();

        if idle >= self.dream_threshold {
            DreamState::Dreaming
        } else if idle >= self.dream_threshold * 3 / 4 {
            DreamState::Drowsy
        } else {
            DreamState::Awake
        }
    }
}

/// The Entropy Monitor - comprehensive system health tracking
pub struct EntropyMonitor {
    watchdog: LatencyWatchdog,
    stagnation_timer: StagnationTimer,
    sys_info: Arc<RwLock<System>>,
    metrics_history: Arc<RwLock<VecDeque<SystemMetrics>>>,
    max_history: usize,
}

impl EntropyMonitor {
    pub fn new(latency_threshold_ms: u64, dream_threshold_secs: u64) -> Self {
        log::info!(
            "Initializing Entropy Monitor: latency_threshold={}ms, dream_threshold={}s",
            latency_threshold_ms,
            dream_threshold_secs
        );

        Self {
            watchdog: LatencyWatchdog::new(latency_threshold_ms),
            stagnation_timer: StagnationTimer::new(dream_threshold_secs),
            sys_info: Arc::new(RwLock::new(System::new())),
            metrics_history: Arc::new(RwLock::new(VecDeque::new())),
            max_history: 3600,  // 1 hour at 1Hz
        }
    }

    /// Record a task execution
    pub fn record_task(&self, task_name: String, duration: Duration) {
        self.watchdog.record(task_name, duration);
    }

    /// Signal user activity
    pub fn user_activity(&self) {
        self.stagnation_timer.poke();
    }

    /// Get current dream state
    pub fn dream_state(&self) -> DreamState {
        self.stagnation_timer.dream_state()
    }

    /// Collect current system metrics
    pub fn collect_metrics(&self, active_tasks: u64, pending_tasks: u64) -> SystemMetrics {
        let mut sys = self.sys_info.write();
        // Refresh only what we actually use. `refresh_all()` can be expensive and
        // may stall on some systems depending on configured collectors.
        sys.refresh_cpu();
        sys.refresh_memory();

        // Calculate CPU usage - use global CPU info
        let cpu_usage = sys.global_cpu_info().cpu_usage() as f64;

        // Calculate memory usage
        let memory_mb = (sys.used_memory() as f64) / 1024.0 / 1024.0;

        // Calculate average latency from recent history
        let avg_latency_ms = {
            let history = self.metrics_history.read();
            if history.is_empty() {
                0.0
            } else {
                let sum: f64 = history.iter().map(|m| m.avg_latency_ms).sum();
                sum / history.len() as f64
            }
        };

        // Get GPU usage if available
        let gpu_usage = self.get_gpu_usage();

        SystemMetrics {
            total_tasks: 0,  // To be filled by orchestrator
            active_tasks,
            pending_tasks,
            avg_latency_ms,
            cpu_usage,
            memory_mb,
            gpu_usage,
            idle_duration: self.stagnation_timer.idle_duration(),
        }
    }

    /// Get GPU usage via system info
    fn get_gpu_usage(&self) -> Option<f64> {
        // Try to get GPU usage from sysinfo
        // On Linux, this might require nvidia-smi or other tools
        // For now, provide a simulated value based on CPU usage as proxy
        let sys = self.sys_info.read();
        let cpu_usage = sys.global_cpu_info().cpu_usage() as f64;

        // Assume GPU usage correlates with CPU usage (rough approximation)
        if cpu_usage > 10.0 {
            Some(cpu_usage * 0.8) // GPU typically 80% of CPU load
        } else {
            Some(0.0)
        }
    }

    /// Store metrics in history
    pub fn record_metrics(&self, metrics: SystemMetrics) {
        let mut history = self.metrics_history.write();
        history.push_back(metrics);

        if history.len() > self.max_history {
            history.pop_front();
        }
    }

    /// Detect performance bottlenecks
    pub fn detect_bottleneck(&self, load: &SystemLoad) -> Option<Bottleneck> {
        let metrics = &load.metrics;

        // CPU saturated
        if metrics.cpu_usage > 90.0 {
            return Some(Bottleneck::CpuSaturated);
        }

        // Memory pressure
        let sys = self.sys_info.read();
        let memory_percent = (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0;
        if memory_percent > 90.0 {
            return Some(Bottleneck::MemoryPressure);
        }

        // Task queue overflow
        if metrics.pending_tasks > 5000 {
            return Some(Bottleneck::TaskQueueOverflow);
        }

        // Check worker balance
        if load.workers.len() > 1 {
            let loads: Vec<f64> = load.workers.iter().map(|w| w.load_factor).collect();
            let max_load = loads.iter().cloned().fold(0.0, f64::max);
            let min_load = loads.iter().cloned().fold(1.0, f64::min);

            // Imbalanced work distribution
            if max_load > 0.9 && min_load < 0.1 {
                log::warn!("⚠️ Imbalanced load: max={:.2}, min={:.2}", max_load, min_load);
            }
        }

        None
    }

    /// Calculate system efficiency (work done per watt)
    pub fn calculate_efficiency(&self) -> f64 {
        let history = self.metrics_history.read();
        if history.len() < 2 {
            return 1.0;
        }

        // Simple efficiency metric: tasks per CPU percentage
        let recent = &history[history.len() - 1];
        if recent.cpu_usage > 0.0 {
            recent.active_tasks as f64 / recent.cpu_usage
        } else {
            0.0
        }
    }

    /// Get recent violations for reporting
    pub fn get_violations(&self, count: usize) -> Vec<LatencyViolation> {
        self.watchdog.recent_violations(count)
    }

    /// Get violation rate
    pub fn violation_rate(&self) -> f64 {
        self.watchdog.violation_rate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_watchdog() {
        let watchdog = LatencyWatchdog::new(16);

        // Below threshold - no violation
        watchdog.record("fast_task".to_string(), Duration::from_millis(10));
        assert_eq!(watchdog.recent_violations(10).len(), 0);

        // Above threshold - violation
        watchdog.record("slow_task".to_string(), Duration::from_millis(50));
        assert_eq!(watchdog.recent_violations(10).len(), 1);
    }

    #[test]
    fn test_stagnation_timer() {
        let timer = StagnationTimer::new(2);  // 2 second threshold

        assert_eq!(timer.dream_state(), DreamState::Awake);

        // Simulate passage of time by checking idle duration
        let idle = timer.idle_duration();
        assert!(idle < Duration::from_secs(1));
    }

    #[test]
    fn test_entropy_monitor() {
        let monitor = EntropyMonitor::new(16, 300);

        // Record some tasks
        monitor.record_task("task1".to_string(), Duration::from_millis(5));
        monitor.record_task("task2".to_string(), Duration::from_millis(25));  // Violation

        assert_eq!(monitor.get_violations(10).len(), 1);
    }
}
