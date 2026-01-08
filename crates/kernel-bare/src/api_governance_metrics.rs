//! Rate Limit Observability & Metrics
//!
//! Comprehensive metrics collection for governance monitoring.

#![no_std]

use core::cmp;

/// Governance metric type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GovernanceMetricType {
    RateLimitEvent,
    QuotaViolation,
    PolicyEnforcement,
    CostTracking,
}

/// Rate limit metric event
#[derive(Clone, Copy)]
pub struct RateLimitMetric {
    pub service_id: u32,
    pub timestamp: u64,
    pub allowed: bool,
    pub denied: bool,
    pub tokens_used: u32,
    pub wait_time_ms: u32,
}

/// Quota metric event
#[derive(Clone, Copy)]
pub struct QuotaMetricEvent {
    pub tenant_id: u32,
    pub quota_type: u8,
    pub usage: u64,
    pub limit: u64,
    pub percent_utilized: u8,
}

/// Policy metric event
#[derive(Clone, Copy)]
pub struct PolicyMetric {
    pub policy_id: u32,
    pub matches: u32,
    pub denials: u16,
    pub alerts: u16,
    pub enforcement_time_us: u32,
}

/// Cost metric event
#[derive(Clone, Copy)]
pub struct CostMetricEvent {
    pub service_id: u32,
    pub tenant_id: u32,
    pub cost_amount: u64,
    pub metric_type: u8,
}

/// Governance metrics collector
pub struct GovernanceCollector {
    rate_limit_metrics: [RateLimitMetric; 256],
    rate_limit_count: u16,

    quota_metrics: [QuotaMetricEvent; 256],
    quota_count: u16,

    policy_metrics: [PolicyMetric; 128],
    policy_count: u8,

    cost_metrics: [CostMetricEvent; 512],
    cost_count: u16,

    alert_thresholds: [u32; 16],
    alert_count: u8,

    active_alerts: u16,

    total_metrics_recorded: u32,
}

impl GovernanceCollector {
    /// Create new metrics collector
    pub fn new() -> Self {
        GovernanceCollector {
            rate_limit_metrics: [RateLimitMetric {
                service_id: 0,
                timestamp: 0,
                allowed: false,
                denied: false,
                tokens_used: 0,
                wait_time_ms: 0,
            }; 256],
            rate_limit_count: 0,

            quota_metrics: [QuotaMetricEvent {
                tenant_id: 0,
                quota_type: 0,
                usage: 0,
                limit: 0,
                percent_utilized: 0,
            }; 256],
            quota_count: 0,

            policy_metrics: [PolicyMetric {
                policy_id: 0,
                matches: 0,
                denials: 0,
                alerts: 0,
                enforcement_time_us: 0,
            }; 128],
            policy_count: 0,

            cost_metrics: [CostMetricEvent {
                service_id: 0,
                tenant_id: 0,
                cost_amount: 0,
                metric_type: 0,
            }; 512],
            cost_count: 0,

            alert_thresholds: [0; 16],
            alert_count: 0,

            active_alerts: 0,
            total_metrics_recorded: 0,
        }
    }

    /// Record a rate limit decision
    pub fn record_rate_limit(&mut self, service_id: u32, allowed: bool, tokens_used: u32) -> bool {
        if (self.rate_limit_count as usize) >= 256 {
            return false;
        }

        self.rate_limit_metrics[self.rate_limit_count as usize] = RateLimitMetric {
            service_id,
            timestamp: 0,
            allowed,
            denied: !allowed,
            tokens_used,
            wait_time_ms: 0,
        };

        self.rate_limit_count += 1;
        self.total_metrics_recorded += 1;
        true
    }

    /// Record a quota event
    pub fn record_quota_event(&mut self, tenant_id: u32, quota_type: u8, usage: u64, limit: u64) -> bool {
        if (self.quota_count as usize) >= 256 {
            return false;
        }

        let percent_utilized = cmp::min(((usage * 100) / cmp::max(limit, 1)) as u8, 100);

        self.quota_metrics[self.quota_count as usize] = QuotaMetricEvent {
            tenant_id,
            quota_type,
            usage,
            limit,
            percent_utilized,
        };

        self.quota_count += 1;
        self.total_metrics_recorded += 1;
        true
    }

    /// Record a policy enforcement
    pub fn record_policy_event(&mut self, policy_id: u32) -> bool {
        if (self.policy_count as usize) >= 128 {
            return false;
        }

        self.policy_metrics[self.policy_count as usize] = PolicyMetric {
            policy_id,
            matches: 1,
            denials: 0,
            alerts: 0,
            enforcement_time_us: 0,
        };

        self.policy_count += 1;
        self.total_metrics_recorded += 1;
        true
    }

    /// Record a cost attribution
    pub fn record_cost_event(&mut self, service_id: u32, tenant_id: u32, cost: u64) -> bool {
        if (self.cost_count as usize) >= 512 {
            return false;
        }

        self.cost_metrics[self.cost_count as usize] = CostMetricEvent {
            service_id,
            tenant_id,
            cost_amount: cost,
            metric_type: 0,
        };

        self.cost_count += 1;
        self.total_metrics_recorded += 1;
        true
    }

    /// Get rate limit statistics
    pub fn get_rate_limit_stats(&self) -> (u16, u16, u16) {
        let mut allowed = 0u16;
        let mut denied = 0u16;

        for i in 0..(self.rate_limit_count as usize) {
            if self.rate_limit_metrics[i].allowed {
                allowed += 1;
            } else {
                denied += 1;
            }
        }

        (self.rate_limit_count, allowed, denied)
    }

    /// Get quota statistics
    pub fn get_quota_stats(&self) -> (u16, u16) {
        let mut violations = 0u16;

        for i in 0..(self.quota_count as usize) {
            if self.quota_metrics[i].percent_utilized >= 100 {
                violations += 1;
            }
        }

        (self.quota_count, violations)
    }

    /// Get policy statistics
    pub fn get_policy_stats(&self) -> (u8, u32, u16) {
        let mut total_matches = 0u32;
        let mut total_denials = 0u16;

        for i in 0..(self.policy_count as usize) {
            total_matches += self.policy_metrics[i].matches;
            total_denials += self.policy_metrics[i].denials;
        }

        (self.policy_count, total_matches, total_denials)
    }

    /// Get cost statistics
    pub fn get_cost_stats(&self) -> (u16, u64) {
        let mut total_cost = 0u64;

        for i in 0..(self.cost_count as usize) {
            total_cost += self.cost_metrics[i].cost_amount;
        }

        (self.cost_count, total_cost)
    }

    /// Set alert threshold
    pub fn set_alert_threshold(&mut self, threshold_type: u8, value: u32) -> bool {
        if (threshold_type as usize) < 16 {
            self.alert_thresholds[threshold_type as usize] = value;
            return true;
        }
        false
    }

    /// Get pending alerts
    pub fn get_pending_alerts(&self) -> u16 {
        self.active_alerts
    }

    /// Get metric percentiles
    pub fn get_metric_percentiles(&self) -> (u32, u32, u32) {
        // Simplified: just return current counts
        (self.rate_limit_count as u32, self.quota_count as u32, self.policy_count as u32)
    }

    /// Get total metrics recorded
    pub fn get_total_metrics(&self) -> u32 {
        self.total_metrics_recorded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_creation() {
        let gc = GovernanceCollector::new();
        assert_eq!(gc.total_metrics_recorded, 0);
    }

    #[test]
    fn test_metric_recording() {
        let mut gc = GovernanceCollector::new();
        let recorded = gc.record_rate_limit(1, true, 50);
        assert!(recorded);
    }

    #[test]
    fn test_stats_aggregation() {
        let mut gc = GovernanceCollector::new();
        gc.record_rate_limit(1, true, 50);
        gc.record_rate_limit(1, false, 0);

        let (total, allowed, denied) = gc.get_rate_limit_stats();
        assert_eq!(total, 2);
        assert_eq!(allowed, 1);
        assert_eq!(denied, 1);
    }
}
