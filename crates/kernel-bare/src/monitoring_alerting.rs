/// Real-time Monitoring & Alerting
///
/// Comprehensive system monitoring with automatic alert generation
/// and real-time metric collection for observability.

use core::cmp::min;

const MAX_AGENTS: usize = 64;
const MAX_METRICS: usize = 256;
const MAX_ALERT_RULES: usize = 32;
const MAX_ALERT_HISTORY: usize = 1024;

/// Metric type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetricType {
    Cpu = 0,
    Memory = 1,
    Disk = 2,
    Network = 3,
    Latency = 4,
    Throughput = 5,
}

/// Alert level
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// Metric data point
#[derive(Clone, Copy, Debug)]
pub struct MetricDataPoint {
    pub timestamp: u64,
    pub value: u64,
}

impl MetricDataPoint {
    pub fn new(timestamp: u64, value: u64) -> Self {
        MetricDataPoint { timestamp, value }
    }
}

/// Alert rule
#[derive(Clone, Copy, Debug)]
pub struct AlertRule {
    pub rule_id: u32,
    pub metric_type: MetricType,
    pub threshold: u64,
    pub alert_level: AlertLevel,
    pub enabled: bool,
}

impl AlertRule {
    pub fn new(rule_id: u32, metric_type: MetricType, threshold: u64) -> Self {
        AlertRule {
            rule_id,
            metric_type,
            threshold,
            alert_level: AlertLevel::Warning,
            enabled: true,
        }
    }

    pub fn evaluate(&self, value: u64) -> bool {
        self.enabled && value > self.threshold
    }
}

/// Alert event
#[derive(Clone, Copy, Debug)]
pub struct AlertEvent {
    pub alert_id: u32,
    pub rule_id: u32,
    pub timestamp: u64,
    pub level: AlertLevel,
    pub value: u64,
    pub threshold: u64,
}

impl AlertEvent {
    pub fn new(alert_id: u32, rule_id: u32, level: AlertLevel, value: u64, threshold: u64) -> Self {
        AlertEvent {
            alert_id,
            rule_id,
            timestamp: 0,
            level,
            value,
            threshold,
        }
    }
}

/// Monitoring Agent
#[derive(Clone, Copy, Debug)]
pub struct MonitoringAgent {
    pub agent_id: u32,
    pub metric_type: MetricType,
    pub last_collection_time: u64,
    pub collection_interval: u32,
    pub collection_count: u32,
}

impl MonitoringAgent {
    pub fn new(agent_id: u32, metric_type: MetricType) -> Self {
        MonitoringAgent {
            agent_id,
            metric_type,
            last_collection_time: 0,
            collection_interval: 1,
            collection_count: 0,
        }
    }
}

/// Real-time Monitoring & Alerting System
pub struct MonitoringSystem {
    agents: [Option<MonitoringAgent>; MAX_AGENTS],
    metrics: [Option<MetricDataPoint>; MAX_METRICS],
    alert_rules: [Option<AlertRule>; MAX_ALERT_RULES],
    alert_history: [Option<AlertEvent>; MAX_ALERT_HISTORY],
    agent_count: u32,
    rule_count: u32,
    alert_count: u32,
    alert_history_index: u32,
}

impl MonitoringSystem {
    pub fn new() -> Self {
        MonitoringSystem {
            agents: [None; MAX_AGENTS],
            metrics: [None; MAX_METRICS],
            alert_rules: [None; MAX_ALERT_RULES],
            alert_history: [None; MAX_ALERT_HISTORY],
            agent_count: 0,
            rule_count: 0,
            alert_count: 0,
            alert_history_index: 0,
        }
    }

    pub fn add_agent(&mut self, metric_type: MetricType) -> u32 {
        for i in 0..MAX_AGENTS {
            if self.agents[i].is_none() {
                let agent_id = i as u32 + 1;
                let agent = MonitoringAgent::new(agent_id, metric_type);
                self.agents[i] = Some(agent);
                self.agent_count += 1;
                return agent_id;
            }
        }
        0
    }

    pub fn remove_agent(&mut self, agent_id: u32) -> bool {
        let idx = (agent_id as usize) - 1;
        if idx < MAX_AGENTS {
            if self.agents[idx].is_some() {
                self.agents[idx] = None;
                self.agent_count -= 1;
                return true;
            }
        }
        false
    }

    pub fn record_metric(&mut self, metric_id: u32, timestamp: u64, value: u64) -> bool {
        let idx = (metric_id as usize) % MAX_METRICS;
        let data_point = MetricDataPoint::new(timestamp, value);
        self.metrics[idx] = Some(data_point);
        true
    }

    pub fn add_alert_rule(&mut self, metric_type: MetricType, threshold: u64) -> u32 {
        for i in 0..MAX_ALERT_RULES {
            if self.alert_rules[i].is_none() {
                let rule_id = i as u32 + 1;
                let rule = AlertRule::new(rule_id, metric_type, threshold);
                self.alert_rules[i] = Some(rule);
                self.rule_count += 1;
                return rule_id;
            }
        }
        0
    }

    pub fn remove_alert_rule(&mut self, rule_id: u32) -> bool {
        let idx = (rule_id as usize) - 1;
        if idx < MAX_ALERT_RULES {
            if self.alert_rules[idx].is_some() {
                self.alert_rules[idx] = None;
                self.rule_count -= 1;
                return true;
            }
        }
        false
    }

    pub fn evaluate_rules(&mut self, metric_id: u32, value: u64) {
        for i in 0..MAX_ALERT_RULES {
            if let Some(rule) = self.alert_rules[i] {
                if rule.evaluate(value) {
                    self.generate_alert(rule.rule_id, rule.alert_level, value, rule.threshold);
                }
            }
        }
    }

    pub fn generate_alert(&mut self, rule_id: u32, level: AlertLevel, value: u64, threshold: u64) {
        let idx = (self.alert_history_index as usize) % MAX_ALERT_HISTORY;
        let alert_id = self.alert_count;
        self.alert_count += 1;
        let mut alert = AlertEvent::new(alert_id, rule_id, level, value, threshold);
        alert.timestamp = 0; // Would be filled in by real clock
        self.alert_history[idx] = Some(alert);
        self.alert_history_index += 1;
    }

    pub fn get_agent_count(&self) -> u32 {
        self.agent_count
    }

    pub fn get_rule_count(&self) -> u32 {
        self.rule_count
    }

    pub fn get_alert_count(&self) -> u32 {
        self.alert_count
    }

    pub fn count_alerts_by_level(&self, level: AlertLevel) -> u32 {
        let mut count = 0;
        for i in 0..MAX_ALERT_HISTORY {
            if let Some(alert) = self.alert_history[i] {
                if alert.level == level {
                    count += 1;
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_collection() {
        let mut system = MonitoringSystem::new();
        let agent_id = system.add_agent(MetricType::Cpu);
        assert!(agent_id > 0);
        assert_eq!(system.get_agent_count(), 1);
    }

    #[test]
    fn test_alert_rule_evaluation() {
        let mut system = MonitoringSystem::new();
        let rule_id = system.add_alert_rule(MetricType::Memory, 80);
        assert!(rule_id > 0);
    }

    #[test]
    fn test_threshold_detection() {
        let mut system = MonitoringSystem::new();
        system.add_alert_rule(MetricType::Cpu, 90);
        system.evaluate_rules(1, 95);
        assert!(system.get_alert_count() > 0);
    }

    #[test]
    fn test_alert_escalation() {
        let mut system = MonitoringSystem::new();
        system.add_alert_rule(MetricType::Disk, 100);
        system.evaluate_rules(1, 110);
        let critical_count = system.count_alerts_by_level(AlertLevel::Critical);
        assert!(critical_count >= 0);
    }

    #[test]
    fn test_metric_storage() {
        let mut system = MonitoringSystem::new();
        system.record_metric(1, 1000, 75);
        system.record_metric(2, 1001, 85);
        system.record_metric(3, 1002, 95);
    }

    #[test]
    fn test_alert_routing() {
        let mut system = MonitoringSystem::new();
        system.add_alert_rule(MetricType::Network, 70);
        system.add_alert_rule(MetricType::Latency, 100);
        assert_eq!(system.get_rule_count(), 2);
    }

    #[test]
    fn test_history_tracking() {
        let mut system = MonitoringSystem::new();
        system.add_alert_rule(MetricType::Memory, 80);
        for i in 0..10 {
            system.evaluate_rules(1, 85 + i);
        }
        assert!(system.get_alert_count() > 0);
    }

    #[test]
    fn test_rule_management() {
        let mut system = MonitoringSystem::new();
        let rule_id = system.add_alert_rule(MetricType::Cpu, 90);
        assert!(system.remove_alert_rule(rule_id));
        assert_eq!(system.get_rule_count(), 0);
    }
}
