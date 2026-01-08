//! RayOS Recovery Policy - Detailed Implementation
//!
//! Extended recovery policy for multi-level fallback and observability.
//! Coordinates with BootMarkers, Watchdog, and PersistentLog for comprehensive reliability.

/// Recovery event type for logging
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryEventType {
    /// Failure detected at boot stage
    FailureDetected = 0,
    /// Attempting recovery
    AttemptingRecovery = 1,
    /// Loading golden snapshot
    LoadingSnapshot = 2,
    /// Recovery boot in progress
    RecoveryBoot = 3,
    /// Recovery succeeded
    RecoverySucceeded = 4,
    /// Recovery failed
    RecoveryFailed = 5,
}

/// Recovery event for logging
#[derive(Clone, Copy)]
pub struct RecoveryEvent {
    /// Event type
    pub event_type: RecoveryEventType,
    /// Timestamp (boot-relative milliseconds)
    pub timestamp: u64,
    /// Boot stage involved
    pub boot_stage: u32,
    /// Additional context code
    pub context: u32,
}

impl RecoveryEvent {
    pub fn new(event_type: RecoveryEventType, timestamp: u64, boot_stage: u32) -> Self {
        RecoveryEvent {
            event_type,
            timestamp,
            boot_stage,
            context: 0,
        }
    }
}

/// Maximum recovery events to track
const MAX_RECOVERY_EVENTS: usize = 64;

/// Recovery policy with event tracking
pub struct RecoveryPolicyWithEvents {
    /// Recovery events for observability
    events: [Option<RecoveryEvent>; MAX_RECOVERY_EVENTS],
    /// Number of events
    event_count: u32,
    /// Consecutive recovery attempts
    recovery_attempts: u32,
    /// Maximum consecutive recovery attempts before halt
    max_recovery_attempts: u32,
}

impl RecoveryPolicyWithEvents {
    pub fn new() -> Self {
        RecoveryPolicyWithEvents {
            events: [None; MAX_RECOVERY_EVENTS],
            event_count: 0,
            recovery_attempts: 0,
            max_recovery_attempts: 3,
        }
    }

    /// Record a recovery event
    pub fn record_event(&mut self, event: RecoveryEvent) -> Result<u32, &'static str> {
        if self.event_count >= (MAX_RECOVERY_EVENTS as u32) {
            // Rotate: remove oldest event
            for i in 0..(MAX_RECOVERY_EVENTS - 1) {
                self.events[i] = self.events[i + 1];
            }
            self.event_count = (MAX_RECOVERY_EVENTS - 1) as u32;
        }

        self.events[self.event_count as usize] = Some(event);
        self.event_count += 1;

        Ok(self.event_count - 1)
    }

    /// Get recovery event by index
    pub fn get_event(&self, index: u32) -> Option<RecoveryEvent> {
        if (index as usize) < (self.event_count as usize) {
            self.events[index as usize]
        } else {
            None
        }
    }

    /// Start recovery attempt
    pub fn start_recovery(&mut self) -> Result<(), &'static str> {
        self.recovery_attempts = self.recovery_attempts.saturating_add(1);

        if self.recovery_attempts > self.max_recovery_attempts {
            return Err("Max recovery attempts exceeded");
        }

        Ok(())
    }

    /// Mark recovery success (reset attempts)
    pub fn recovery_success(&mut self) {
        self.recovery_attempts = 0;
    }

    /// Get recovery attempt count
    pub fn recovery_attempts(&self) -> u32 {
        self.recovery_attempts
    }

    /// Get total events
    pub fn event_count(&self) -> u32 {
        self.event_count
    }

    /// Clear all events
    pub fn clear_events(&mut self) {
        self.event_count = 0;
    }
}

/// Integrated recovery coordinator
pub struct RecoveryCoordinator {
    /// Policy with events
    policy: RecoveryPolicyWithEvents,
    /// Last kernel panic address (for debugging)
    last_panic_addr: u64,
    /// Last watchdog timeout time
    last_watchdog_timeout: u64,
    /// Number of times golden state was loaded
    golden_loads: u32,
    /// System is in critical recovery state
    critical: bool,
}

impl RecoveryCoordinator {
    pub fn new() -> Self {
        RecoveryCoordinator {
            policy: RecoveryPolicyWithEvents::new(),
            last_panic_addr: 0,
            last_watchdog_timeout: 0,
            golden_loads: 0,
            critical: false,
        }
    }

    /// Handle kernel panic
    pub fn handle_panic(&mut self, panic_addr: u64, timestamp: u64) -> Result<(), &'static str> {
        self.last_panic_addr = panic_addr;

        let event = RecoveryEvent::new(RecoveryEventType::FailureDetected, timestamp, 0);
        self.policy.record_event(event)?;

        self.policy.start_recovery()?;
        self.critical = true;

        Ok(())
    }

    /// Handle watchdog timeout
    pub fn handle_watchdog_timeout(&mut self, timestamp: u64, stage: u32) -> Result<(), &'static str> {
        self.last_watchdog_timeout = timestamp;

        let event = RecoveryEvent::new(RecoveryEventType::FailureDetected, timestamp, stage);
        self.policy.record_event(event)?;

        self.policy.start_recovery()?;
        self.critical = true;

        Ok(())
    }

    /// Load golden state
    pub fn load_golden(&mut self, timestamp: u64) -> Result<(), &'static str> {
        let event = RecoveryEvent::new(RecoveryEventType::LoadingSnapshot, timestamp, 0);
        self.policy.record_event(event)?;

        self.golden_loads = self.golden_loads.saturating_add(1);

        Ok(())
    }

    /// Mark recovery as successful
    pub fn mark_recovered(&mut self, timestamp: u64) -> Result<(), &'static str> {
        let event = RecoveryEvent::new(RecoveryEventType::RecoverySucceeded, timestamp, 0);
        self.policy.record_event(event)?;

        self.policy.recovery_success();
        self.critical = false;

        Ok(())
    }

    /// Check if system is in critical state
    pub fn is_critical(&self) -> bool {
        self.critical
    }

    /// Get golden load count
    pub fn golden_loads(&self) -> u32 {
        self.golden_loads
    }

    /// Get recovery events
    pub fn recovery_events(&self) -> u32 {
        self.policy.event_count()
    }

    /// Get recovery attempts
    pub fn recovery_attempts(&self) -> u32 {
        self.policy.recovery_attempts()
    }

    /// Check if recovery is exhausted
    pub fn is_recovery_exhausted(&self) -> bool {
        self.policy.recovery_attempts() > self.policy.max_recovery_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_event() {
        let event = RecoveryEvent::new(RecoveryEventType::FailureDetected, 1000, 2);
        assert_eq!(event.event_type, RecoveryEventType::FailureDetected);
        assert_eq!(event.timestamp, 1000);
    }

    #[test]
    fn test_recovery_policy_events() {
        let mut policy = RecoveryPolicyWithEvents::new();

        let event = RecoveryEvent::new(RecoveryEventType::FailureDetected, 1000, 1);
        policy.record_event(event).unwrap();

        assert_eq!(policy.event_count(), 1);
    }

    #[test]
    fn test_recovery_attempts() {
        let mut policy = RecoveryPolicyWithEvents::new();

        policy.start_recovery().unwrap();
        policy.start_recovery().unwrap();

        assert_eq!(policy.recovery_attempts(), 2);

        policy.recovery_success();
        assert_eq!(policy.recovery_attempts(), 0);
    }

    #[test]
    fn test_coordinator() {
        let mut coordinator = RecoveryCoordinator::new();

        coordinator.handle_panic(0x12345678, 1000).unwrap();
        assert!(coordinator.is_critical());

        coordinator.load_golden(1100).unwrap();
        assert!(coordinator.golden_loads() > 0);

        coordinator.mark_recovered(1200).unwrap();
        assert!(!coordinator.is_critical());
    }

    #[test]
    fn test_exhaustion() {
        let mut policy = RecoveryPolicyWithEvents::new();
        policy.max_recovery_attempts = 2;

        policy.start_recovery().ok();
        policy.start_recovery().ok();
        policy.start_recovery().ok();

        assert!(policy.recovery_attempts() > policy.max_recovery_attempts);
    }
}
