//! RayOS Watchdog Timer
//!
//! Hardware/software watchdog that detects kernel hangs and triggers auto-reboot.
//! Default timeout is 30 seconds; application must periodically "kick" the watchdog.
//!
//! **Design**: Timer-based hang detection with exponential backoff on repeated failures.
//! After N consecutive resets, system enters recovery mode instead of infinite reboot loop.

/// Watchdog policy for behavior on timeout
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchdogPolicy {
    /// Auto-reboot on timeout
    AutoReboot = 0,
    /// Trigger kernel dump then reboot
    DumpThenReboot = 1,
    /// Attempt recovery boot
    RecoveryBoot = 2,
    /// Halt and drop to debugger (if available)
    Halt = 3,
}

/// Watchdog status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchdogStatus {
    /// Watchdog is disarmed
    Disarmed = 0,
    /// Watchdog is armed and ticking
    Armed = 1,
    /// Watchdog timeout expired, reboot pending
    Expired = 2,
    /// Watchdog attempted recovery
    Recovering = 3,
    /// Watchdog has given up (too many consecutive timeouts)
    Failed = 4,
}

/// Watchdog configuration
#[derive(Clone, Copy)]
pub struct WatchdogConfig {
    /// Initial timeout in milliseconds
    pub timeout_ms: u32,
    /// Current timeout (may increase after failures)
    pub current_timeout_ms: u32,
    /// Maximum timeout (cap on exponential backoff)
    pub max_timeout_ms: u32,
    /// Backoff factor for repeated timeouts (2 = double each time)
    pub backoff_factor: u32,
    /// Number of consecutive failures before giving up
    pub max_consecutive_failures: u32,
}

impl WatchdogConfig {
    pub fn new(timeout_ms: u32) -> Self {
        WatchdogConfig {
            timeout_ms,
            current_timeout_ms: timeout_ms,
            max_timeout_ms: timeout_ms * 4, // Max 4x initial
            backoff_factor: 2,
            max_consecutive_failures: 3,
        }
    }

    /// Apply exponential backoff
    pub fn apply_backoff(&mut self) {
        self.current_timeout_ms = core::cmp::min(
            self.current_timeout_ms.saturating_mul(self.backoff_factor),
            self.max_timeout_ms,
        );
    }

    /// Reset to initial timeout
    pub fn reset(&mut self) {
        self.current_timeout_ms = self.timeout_ms;
    }
}

/// Watchdog timer
pub struct Watchdog {
    /// Watchdog configuration
    config: WatchdogConfig,
    /// Current status
    status: WatchdogStatus,
    /// Time of last successful kick (milliseconds since boot)
    last_kick_time: u64,
    /// Number of consecutive timeouts
    consecutive_failures: u32,
    /// Policy on timeout
    policy: WatchdogPolicy,
    /// Boot time (milliseconds since system boot)
    boot_time_ms: u64,
}

impl Watchdog {
    pub fn new(timeout_ms: u32) -> Self {
        Watchdog {
            config: WatchdogConfig::new(timeout_ms),
            status: WatchdogStatus::Disarmed,
            last_kick_time: 0,
            consecutive_failures: 0,
            policy: WatchdogPolicy::AutoReboot,
            boot_time_ms: 0,
        }
    }

    /// Initialize with current time
    pub fn init(&mut self, boot_time_ms: u64) {
        self.boot_time_ms = boot_time_ms;
    }

    /// Arm the watchdog (start monitoring)
    pub fn arm(&mut self) {
        self.status = WatchdogStatus::Armed;
        self.last_kick_time = self.boot_time_ms;
        self.consecutive_failures = 0;
        self.config.reset();
    }

    /// Disarm the watchdog (stop monitoring)
    pub fn disarm(&mut self) {
        self.status = WatchdogStatus::Disarmed;
    }

    /// Kick the watchdog (reset timeout)
    pub fn kick(&mut self) {
        if self.status == WatchdogStatus::Armed || self.status == WatchdogStatus::Expired {
            self.last_kick_time = self.boot_time_ms;
            self.consecutive_failures = 0;
            self.config.reset();
            self.status = WatchdogStatus::Armed;
        }
    }

    /// Check if watchdog has timed out
    pub fn check(&mut self, current_time_ms: u64) -> Option<WatchdogPolicy> {
        if self.status != WatchdogStatus::Armed {
            return None;
        }

        let elapsed = current_time_ms.saturating_sub(self.last_kick_time);

        if elapsed > (self.config.current_timeout_ms as u64) {
            self.status = WatchdogStatus::Expired;
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);

            if self.consecutive_failures >= self.config.max_consecutive_failures {
                self.status = WatchdogStatus::Failed;
            } else {
                self.config.apply_backoff();
            }

            return Some(self.policy);
        }

        None
    }

    /// Handle timeout and return next action
    pub fn handle_timeout(&mut self) -> WatchdogPolicy {
        self.policy
    }

    /// Attempt recovery boot
    pub fn attempt_recovery(&mut self) {
        self.status = WatchdogStatus::Recovering;
    }

    /// Set watchdog policy
    pub fn set_policy(&mut self, policy: WatchdogPolicy) {
        self.policy = policy;
    }

    /// Get current status
    pub fn status(&self) -> WatchdogStatus {
        self.status
    }

    /// Get time until timeout (milliseconds, 0 if expired)
    pub fn time_remaining(&self, current_time_ms: u64) -> u32 {
        if self.status != WatchdogStatus::Armed {
            return 0;
        }

        let elapsed = current_time_ms.saturating_sub(self.last_kick_time);
        let timeout = self.config.current_timeout_ms as u64;

        if elapsed >= timeout {
            0
        } else {
            (timeout - elapsed) as u32
        }
    }

    /// Get number of consecutive failures
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Check if watchdog has given up
    pub fn has_failed(&self) -> bool {
        self.status == WatchdogStatus::Failed
    }
}

#[cfg(test)]
mod watchdog_tests {
    use super::*;

    #[test]
    fn test_watchdog_creation() {
        let dog = Watchdog::new(30000); // 30 seconds
        assert_eq!(dog.status(), WatchdogStatus::Disarmed);
    }

    #[test]
    fn test_arm_disarm() {
        let mut dog = Watchdog::new(30000);
        dog.init(0);

        dog.arm();
        assert_eq!(dog.status(), WatchdogStatus::Armed);

        dog.disarm();
        assert_eq!(dog.status(), WatchdogStatus::Disarmed);
    }

    #[test]
    fn test_kick() {
        let mut dog = Watchdog::new(30000);
        dog.init(0);
        dog.arm();

        let remaining = dog.time_remaining(1000);
        assert!(remaining > 0);

        dog.kick();
        let remaining_after = dog.time_remaining(1000);
        assert!(remaining_after > remaining);
    }

    #[test]
    fn test_timeout() {
        let mut dog = Watchdog::new(1000); // 1 second
        dog.init(0);
        dog.arm();

        // Not expired yet
        assert!(dog.check(500).is_none());

        // Expired
        let timeout = dog.check(2000);
        assert!(timeout.is_some());
        assert_eq!(dog.status(), WatchdogStatus::Expired);
    }

    #[test]
    fn test_exponential_backoff() {
        let mut dog = Watchdog::new(1000);
        dog.init(0);
        dog.arm();

        // First timeout
        dog.check(2000);
        let first_timeout = dog.config.current_timeout_ms;

        // Reset but don't kick (simulate hang continuing)
        dog.status = WatchdogStatus::Armed;
        dog.check(first_timeout as u64 + 3000);

        // Timeout should have backed off
        assert!(dog.config.current_timeout_ms > first_timeout);
    }
}
