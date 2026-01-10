//! Real-time Alerting System: Threshold-Based Alert Management and Notification
//!
//! Comprehensive alerting infrastructure with configurable thresholds, alert types,
//! severity levels, and notification queuing for real-time monitoring.
//!
//! Phase 35, Task 5

/// Alert severity level
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum AlertSeverity {
    Info = 0,
    Warning = 1,
    Critical = 2,
    Emergency = 3,
}

impl AlertSeverity {
    /// Get severity name
    pub const fn name(&self) -> &'static str {
        match self {
            AlertSeverity::Info => "Info",
            AlertSeverity::Warning => "Warning",
            AlertSeverity::Critical => "Critical",
            AlertSeverity::Emergency => "Emergency",
        }
    }

    /// Get severity score (0-3)
    pub const fn score(&self) -> u8 {
        *self as u8
    }
}

/// Alert type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AlertType {
    HighLatency = 0,
    LowThroughput = 1,
    HighMemory = 2,
    HighCpu = 3,
    LowEfficiency = 4,
    MutationFailure = 5,
    ModuleError = 6,
    ThresholdViolation = 7,
}

impl AlertType {
    /// Get alert type name
    pub const fn name(&self) -> &'static str {
        match self {
            AlertType::HighLatency => "High Latency",
            AlertType::LowThroughput => "Low Throughput",
            AlertType::HighMemory => "High Memory",
            AlertType::HighCpu => "High CPU",
            AlertType::LowEfficiency => "Low Efficiency",
            AlertType::MutationFailure => "Mutation Failure",
            AlertType::ModuleError => "Module Error",
            AlertType::ThresholdViolation => "Threshold Violation",
        }
    }
}

/// Alert threshold configuration
#[derive(Clone, Copy, Debug)]
pub struct AlertThreshold {
    /// Threshold ID
    pub id: u32,
    /// Alert type
    pub alert_type: AlertType,
    /// Upper threshold value
    pub upper_bound: u32,
    /// Lower threshold value
    pub lower_bound: u32,
    /// Is enabled
    pub enabled: bool,
    /// Severity when triggered
    pub severity: AlertSeverity,
}

impl AlertThreshold {
    /// Create new alert threshold
    pub const fn new(id: u32, alert_type: AlertType, upper: u32) -> Self {
        AlertThreshold {
            id,
            alert_type,
            upper_bound: upper,
            lower_bound: 0,
            enabled: true,
            severity: AlertSeverity::Warning,
        }
    }

    /// Set lower bound
    pub fn with_lower_bound(mut self, lower: u32) -> Self {
        self.lower_bound = lower;
        self
    }

    /// Set severity
    pub fn with_severity(mut self, severity: AlertSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Check if value violates threshold
    pub fn is_violated(&self, value: u32) -> bool {
        if !self.enabled {
            return false;
        }
        value > self.upper_bound || value < self.lower_bound
    }

    /// Check if value exceeds upper bound
    pub fn exceeds_upper(&self, value: u32) -> bool {
        self.enabled && value > self.upper_bound
    }

    /// Check if value falls below lower bound
    pub fn below_lower(&self, value: u32) -> bool {
        self.enabled && value < self.lower_bound
    }
}

/// Alert event
#[derive(Clone, Copy, Debug)]
pub struct AlertEvent {
    /// Event ID
    pub id: u32,
    /// Alert type
    pub alert_type: AlertType,
    /// Severity
    pub severity: AlertSeverity,
    /// Timestamp (ms)
    pub timestamp_ms: u64,
    /// Measured value
    pub measured_value: u32,
    /// Threshold that was violated
    pub threshold_value: u32,
    /// Is active
    pub is_active: bool,
}

impl AlertEvent {
    /// Create new alert event
    pub const fn new(
        id: u32,
        alert_type: AlertType,
        severity: AlertSeverity,
        timestamp_ms: u64,
        measured: u32,
        threshold: u32,
    ) -> Self {
        AlertEvent {
            id,
            alert_type,
            severity,
            timestamp_ms,
            measured_value: measured,
            threshold_value: threshold,
            is_active: true,
        }
    }

    /// Resolve alert (make inactive)
    pub fn resolve(&mut self) {
        self.is_active = false;
    }
}

/// Alert notification
#[derive(Clone, Copy, Debug)]
pub struct AlertNotification {
    /// Notification ID
    pub id: u32,
    /// Alert event ID
    pub event_id: u32,
    /// Message hash
    pub message_hash: u32,
    /// Timestamp (ms)
    pub timestamp_ms: u64,
    /// Delivery attempts
    pub delivery_attempts: u8,
    /// Max delivery attempts
    pub max_attempts: u8,
    /// Is delivered
    pub is_delivered: bool,
}

impl AlertNotification {
    /// Create new alert notification
    pub const fn new(id: u32, event_id: u32, timestamp_ms: u64) -> Self {
        AlertNotification {
            id,
            event_id,
            message_hash: 0,
            timestamp_ms,
            delivery_attempts: 0,
            max_attempts: 3,
            is_delivered: false,
        }
    }

    /// Mark as delivered
    pub fn delivered(&mut self) {
        self.is_delivered = true;
        self.delivery_attempts = self.max_attempts;
    }

    /// Increment delivery attempts
    pub fn increment_attempt(&mut self) -> bool {
        if self.delivery_attempts < self.max_attempts {
            self.delivery_attempts += 1;
            true
        } else {
            false
        }
    }

    /// Check if can retry
    pub fn can_retry(&self) -> bool {
        !self.is_delivered && self.delivery_attempts < self.max_attempts
    }
}

/// Alert statistic
#[derive(Clone, Copy, Debug)]
pub struct AlertStatistic {
    /// Statistic ID
    pub id: u32,
    /// Alert type
    pub alert_type: AlertType,
    /// Count of alerts triggered
    pub trigger_count: u32,
    /// Count of active alerts
    pub active_count: u16,
    /// Count of resolved alerts
    pub resolved_count: u32,
    /// Last triggered (ms)
    pub last_triggered_ms: u64,
}

impl AlertStatistic {
    /// Create new alert statistic
    pub const fn new(id: u32, alert_type: AlertType) -> Self {
        AlertStatistic {
            id,
            alert_type,
            trigger_count: 0,
            active_count: 0,
            resolved_count: 0,
            last_triggered_ms: 0,
        }
    }
}

/// Alert Manager System
pub struct AlertManager {
    /// Thresholds (max 32)
    thresholds: [Option<AlertThreshold>; 32],
    /// Alert events (max 100)
    events: [Option<AlertEvent>; 100],
    /// Notifications (max 100)
    notifications: [Option<AlertNotification>; 100],
    /// Statistics (max 8 alert types)
    statistics: [Option<AlertStatistic>; 8],
    /// Alert suppression flag
    suppress_alerts: bool,
    /// Total alerts triggered
    total_alerts: u32,
}

impl AlertManager {
    /// Create new alert manager
    pub const fn new() -> Self {
        AlertManager {
            thresholds: [None; 32],
            events: [None; 100],
            notifications: [None; 100],
            statistics: [None; 8],
            suppress_alerts: false,
            total_alerts: 0,
        }
    }

    /// Configure threshold
    pub fn configure_threshold(&mut self, threshold: AlertThreshold) -> bool {
        for slot in &mut self.thresholds {
            if slot.is_none() {
                *slot = Some(threshold);
                return true;
            }
        }
        false
    }

    /// Update threshold
    pub fn update_threshold(&mut self, id: u32, enabled: bool, upper: u32) -> bool {
        for threshold in &mut self.thresholds {
            if let Some(t) = threshold {
                if t.id == id {
                    t.enabled = enabled;
                    t.upper_bound = upper;
                    return true;
                }
            }
        }
        false
    }

    /// Check value and trigger alert if needed
    pub fn check_and_alert(&mut self, value: u32, timestamp_ms: u64) -> bool {
        if self.suppress_alerts {
            return false;
        }

        let mut triggered = false;
        let mut violations = [(AlertType::HighLatency, AlertSeverity::Info, 0u32); 16];
        let mut violation_count = 0usize;

        // Collect violations first
        for threshold in &self.thresholds {
            if let Some(t) = threshold {
                if t.is_violated(value) && violation_count < 16 {
                    violations[violation_count] = (t.alert_type, t.severity, t.upper_bound);
                    violation_count += 1;
                }
            }
        }

        // Process violations
        for i in 0..violation_count {
            let (alert_type, severity, threshold_val) = violations[i];
            let event = AlertEvent::new(
                self.total_alerts,
                alert_type,
                severity,
                timestamp_ms,
                value,
                threshold_val,
            );

            if self.queue_event(event) {
                self.total_alerts += 1;
                triggered = true;

                // Update statistics
                self.update_statistic(alert_type, timestamp_ms);
            }
        }

        triggered
    }

    /// Queue alert event
    pub fn queue_event(&mut self, event: AlertEvent) -> bool {
        for slot in &mut self.events {
            if slot.is_none() {
                *slot = Some(event);
                return true;
            }
        }
        false
    }

    /// Create notification from event
    pub fn notify_event(&mut self, event_id: u32, timestamp_ms: u64) -> bool {
        let notification = AlertNotification::new(self.total_alerts, event_id, timestamp_ms);

        for slot in &mut self.notifications {
            if slot.is_none() {
                *slot = Some(notification);
                return true;
            }
        }
        false
    }

    /// Resolve alert
    pub fn resolve_alert(&mut self, event_id: u32) -> bool {
        let mut alert_type = None;

        for event in &mut self.events {
            if let Some(e) = event {
                if e.id == event_id {
                    e.resolve();
                    alert_type = Some(e.alert_type);
                    break;
                }
            }
        }

        // Update statistics after loop
        if let Some(atype) = alert_type {
            if let Some(stat) = self.find_statistic_mut(atype) {
                stat.resolved_count = stat.resolved_count.saturating_add(1);
                if stat.active_count > 0 {
                    stat.active_count -= 1;
                }
            }
            return true;
        }

        false
    }

    /// Get active alerts count
    pub fn active_alerts_count(&self) -> u32 {
        self.events
            .iter()
            .filter(|e| e.map(|event| event.is_active).unwrap_or(false))
            .count() as u32
    }

    /// Get pending notifications count
    pub fn pending_notifications_count(&self) -> u32 {
        self.notifications
            .iter()
            .filter(|n| n.map(|notif| !notif.is_delivered).unwrap_or(false))
            .count() as u32
    }

    /// Get alerts by severity
    pub fn alerts_by_severity(&self, severity: AlertSeverity) -> u32 {
        self.events
            .iter()
            .filter(|e| e.map(|event| event.severity == severity && event.is_active).unwrap_or(false))
            .count() as u32
    }

    /// Suppress all alerts
    pub fn suppress(&mut self) {
        self.suppress_alerts = true;
    }

    /// Resume alerts
    pub fn resume(&mut self) {
        self.suppress_alerts = false;
    }

    /// Mark notification as delivered
    pub fn mark_delivered(&mut self, notif_id: u32) -> bool {
        for notif in &mut self.notifications {
            if let Some(n) = notif {
                if n.id == notif_id {
                    n.delivered();
                    return true;
                }
            }
        }
        false
    }

    /// Retry notification delivery
    pub fn retry_notification(&mut self, notif_id: u32) -> bool {
        for notif in &mut self.notifications {
            if let Some(n) = notif {
                if n.id == notif_id {
                    return n.increment_attempt();
                }
            }
        }
        false
    }

    /// Get notification for retry
    pub fn get_retry_notification(&self) -> Option<u32> {
        for notif in &self.notifications {
            if let Some(n) = notif {
                if n.can_retry() {
                    return Some(n.id);
                }
            }
        }
        None
    }

    /// Clear resolved alerts
    pub fn clear_resolved(&mut self) -> u32 {
        let mut cleared = 0u32;

        for slot in &mut self.events {
            if let Some(e) = slot {
                if !e.is_active {
                    cleared += 1;
                    *slot = None;
                }
            }
        }

        cleared
    }

    /// Get statistics for alert type
    pub fn get_statistics(&self, alert_type: AlertType) -> Option<AlertStatistic> {
        for stat in &self.statistics {
            if let Some(s) = stat {
                if s.alert_type == alert_type {
                    return Some(*s);
                }
            }
        }
        None
    }

    /// Get total alerts triggered
    pub fn total_alerts_triggered(&self) -> u32 {
        self.total_alerts
    }

    /// Get highest severity alert
    pub fn highest_severity(&self) -> Option<AlertSeverity> {
        self.events
            .iter()
            .filter_map(|e| e.map(|event| if event.is_active { Some(event.severity) } else { None }).flatten())
            .max()
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.thresholds = [None; 32];
        self.events = [None; 100];
        self.notifications = [None; 100];
        self.statistics = [None; 8];
        self.suppress_alerts = false;
        self.total_alerts = 0;
    }

    /// Statistics summary
    pub fn statistics_summary(&self) -> (u32, u32, u32, u32) {
        (
            self.total_alerts,
            self.active_alerts_count(),
            self.pending_notifications_count(),
            self.alerts_by_severity(AlertSeverity::Emergency),
        )
    }

    // Helper methods

    fn update_statistic(&mut self, alert_type: AlertType, timestamp_ms: u64) {
        let mut found = false;

        for stat in &mut self.statistics {
            if let Some(s) = stat {
                if s.alert_type == alert_type {
                    s.trigger_count += 1;
                    s.active_count += 1;
                    s.last_triggered_ms = timestamp_ms;
                    found = true;
                    break;
                }
            }
        }

        if !found {
            // Create new statistic
            let mut new_stat = AlertStatistic::new(self.statistics.len() as u32, alert_type);
            new_stat.trigger_count = 1;
            new_stat.active_count = 1;
            new_stat.last_triggered_ms = timestamp_ms;

            for slot in &mut self.statistics {
                if slot.is_none() {
                    *slot = Some(new_stat);
                    break;
                }
            }
        }
    }

    fn find_statistic_mut(&mut self, alert_type: AlertType) -> Option<&mut AlertStatistic> {
        for stat in &mut self.statistics {
            if let Some(s) = stat {
                if s.alert_type == alert_type {
                    return Some(s);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_severity_enum() {
        assert_eq!(AlertSeverity::Info as u8, 0);
        assert_eq!(AlertSeverity::Emergency as u8, 3);
    }

    #[test]
    fn test_alert_severity_score() {
        assert_eq!(AlertSeverity::Warning.score(), 1);
        assert_eq!(AlertSeverity::Critical.score(), 2);
    }

    #[test]
    fn test_alert_severity_name() {
        assert_eq!(AlertSeverity::Info.name(), "Info");
        assert_eq!(AlertSeverity::Emergency.name(), "Emergency");
    }

    #[test]
    fn test_alert_type_enum() {
        assert_eq!(AlertType::HighLatency as u8, 0);
        assert_eq!(AlertType::ThresholdViolation as u8, 7);
    }

    #[test]
    fn test_alert_type_name() {
        assert_eq!(AlertType::HighLatency.name(), "High Latency");
        assert_eq!(AlertType::ModuleError.name(), "Module Error");
    }

    #[test]
    fn test_alert_threshold_creation() {
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        assert_eq!(threshold.upper_bound, 100);
        assert!(threshold.enabled);
    }

    #[test]
    fn test_alert_threshold_with_lower_bound() {
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100)
            .with_lower_bound(10);
        assert_eq!(threshold.lower_bound, 10);
    }

    #[test]
    fn test_alert_threshold_with_severity() {
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100)
            .with_severity(AlertSeverity::Critical);
        assert_eq!(threshold.severity, AlertSeverity::Critical);
    }

    #[test]
    fn test_alert_threshold_is_violated_upper() {
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        assert!(threshold.is_violated(150));
        assert!(!threshold.is_violated(50));
    }

    #[test]
    fn test_alert_threshold_is_violated_lower() {
        let threshold = AlertThreshold::new(1, AlertType::LowThroughput, 100)
            .with_lower_bound(50);
        assert!(threshold.is_violated(30));
        assert!(!threshold.is_violated(70));
    }

    #[test]
    fn test_alert_threshold_exceeds_upper() {
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        assert!(threshold.exceeds_upper(150));
        assert!(!threshold.exceeds_upper(50));
    }

    #[test]
    fn test_alert_threshold_below_lower() {
        let threshold = AlertThreshold::new(1, AlertType::LowThroughput, 100)
            .with_lower_bound(50);
        assert!(threshold.below_lower(30));
        assert!(!threshold.below_lower(70));
    }

    #[test]
    fn test_alert_threshold_disabled() {
        let mut threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        threshold.enabled = false;
        assert!(!threshold.is_violated(150));
    }

    #[test]
    fn test_alert_event_creation() {
        let event = AlertEvent::new(1, AlertType::HighLatency, AlertSeverity::Warning, 1000, 150, 100);
        assert!(event.is_active);
        assert_eq!(event.measured_value, 150);
    }

    #[test]
    fn test_alert_event_resolve() {
        let mut event = AlertEvent::new(1, AlertType::HighLatency, AlertSeverity::Warning, 1000, 150, 100);
        event.resolve();
        assert!(!event.is_active);
    }

    #[test]
    fn test_alert_notification_creation() {
        let notif = AlertNotification::new(1, 1, 1000);
        assert!(!notif.is_delivered);
        assert_eq!(notif.delivery_attempts, 0);
    }

    #[test]
    fn test_alert_notification_delivered() {
        let mut notif = AlertNotification::new(1, 1, 1000);
        notif.delivered();
        assert!(notif.is_delivered);
        assert_eq!(notif.delivery_attempts, notif.max_attempts);
    }

    #[test]
    fn test_alert_notification_increment_attempt() {
        let mut notif = AlertNotification::new(1, 1, 1000);
        assert!(notif.increment_attempt());
        assert_eq!(notif.delivery_attempts, 1);
    }

    #[test]
    fn test_alert_notification_max_attempts() {
        let mut notif = AlertNotification::new(1, 1, 1000);
        notif.max_attempts = 2;
        assert!(notif.increment_attempt());
        assert!(notif.increment_attempt());
        assert!(!notif.increment_attempt());
    }

    #[test]
    fn test_alert_notification_can_retry() {
        let notif = AlertNotification::new(1, 1, 1000);
        assert!(notif.can_retry());

        let mut notif2 = AlertNotification::new(2, 2, 1000);
        notif2.delivered();
        assert!(!notif2.can_retry());
    }

    #[test]
    fn test_alert_statistic_creation() {
        let stat = AlertStatistic::new(1, AlertType::HighLatency);
        assert_eq!(stat.trigger_count, 0);
        assert_eq!(stat.active_count, 0);
    }

    #[test]
    fn test_alert_manager_creation() {
        let manager = AlertManager::new();
        assert_eq!(manager.total_alerts, 0);
        assert!(!manager.suppress_alerts);
    }

    #[test]
    fn test_alert_manager_configure_threshold() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        assert!(manager.configure_threshold(threshold));
    }

    #[test]
    fn test_alert_manager_check_and_alert() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);

        assert!(manager.check_and_alert(150, 1000));
        assert!(!manager.check_and_alert(50, 1000));
    }

    #[test]
    fn test_alert_manager_queue_event() {
        let mut manager = AlertManager::new();
        let event = AlertEvent::new(1, AlertType::HighLatency, AlertSeverity::Warning, 1000, 150, 100);
        assert!(manager.queue_event(event));
    }

    #[test]
    fn test_alert_manager_active_alerts_count() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);
        manager.check_and_alert(150, 1000);

        assert_eq!(manager.active_alerts_count(), 1);
    }

    #[test]
    fn test_alert_manager_resolve_alert() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);
        manager.check_and_alert(150, 1000);

        assert!(manager.resolve_alert(0));
        assert_eq!(manager.active_alerts_count(), 0);
    }

    #[test]
    fn test_alert_manager_suppress_resume() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);

        manager.suppress();
        assert!(!manager.check_and_alert(150, 1000));

        manager.resume();
        assert!(manager.check_and_alert(150, 1000));
    }

    #[test]
    fn test_alert_manager_notify_event() {
        let mut manager = AlertManager::new();
        assert!(manager.notify_event(1, 1000));
    }

    #[test]
    fn test_alert_manager_pending_notifications() {
        let mut manager = AlertManager::new();
        manager.notify_event(1, 1000);
        assert_eq!(manager.pending_notifications_count(), 1);

        manager.mark_delivered(0);
        assert_eq!(manager.pending_notifications_count(), 0);
    }

    #[test]
    fn test_alert_manager_alerts_by_severity() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100)
            .with_severity(AlertSeverity::Critical);
        manager.configure_threshold(threshold);
        manager.check_and_alert(150, 1000);

        assert_eq!(manager.alerts_by_severity(AlertSeverity::Critical), 1);
        assert_eq!(manager.alerts_by_severity(AlertSeverity::Warning), 0);
    }

    #[test]
    fn test_alert_manager_highest_severity() {
        let mut manager = AlertManager::new();
        let threshold1 = AlertThreshold::new(1, AlertType::HighLatency, 100)
            .with_severity(AlertSeverity::Warning);
        let threshold2 = AlertThreshold::new(2, AlertType::HighCpu, 80)
            .with_severity(AlertSeverity::Critical);
        manager.configure_threshold(threshold1);
        manager.configure_threshold(threshold2);

        manager.check_and_alert(150, 1000);
        manager.check_and_alert(90, 1000);

        assert_eq!(manager.highest_severity(), Some(AlertSeverity::Critical));
    }

    #[test]
    fn test_alert_manager_clear_resolved() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);
        manager.check_and_alert(150, 1000);
        manager.resolve_alert(0);

        let cleared = manager.clear_resolved();
        assert!(cleared > 0);
        assert_eq!(manager.active_alerts_count(), 0);
    }

    #[test]
    fn test_alert_manager_get_statistics() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);
        manager.check_and_alert(150, 1000);

        let stat = manager.get_statistics(AlertType::HighLatency);
        assert!(stat.is_some());
        assert_eq!(stat.unwrap().trigger_count, 1);
    }

    #[test]
    fn test_alert_manager_statistics_summary() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);
        manager.check_and_alert(150, 1000);

        let (total, active, pending, emergency) = manager.statistics_summary();
        assert_eq!(total, 1);
        assert_eq!(active, 1);
    }

    #[test]
    fn test_alert_manager_update_threshold() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);

        assert!(manager.update_threshold(1, true, 200));
    }

    #[test]
    fn test_alert_manager_clear() {
        let mut manager = AlertManager::new();
        let threshold = AlertThreshold::new(1, AlertType::HighLatency, 100);
        manager.configure_threshold(threshold);
        manager.check_and_alert(150, 1000);

        manager.clear();
        assert_eq!(manager.total_alerts, 0);
        assert_eq!(manager.active_alerts_count(), 0);
    }

    #[test]
    fn test_alert_manager_retry_notification() {
        let mut manager = AlertManager::new();
        manager.notify_event(1, 1000);

        assert!(manager.retry_notification(0));
        assert!(!manager.retry_notification(999));  // Non-existent
    }

    #[test]
    fn test_alert_manager_get_retry_notification() {
        let mut manager = AlertManager::new();
        manager.notify_event(1, 1000);

        let notif_id = manager.get_retry_notification();
        assert!(notif_id.is_some());
    }
}
