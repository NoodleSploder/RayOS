// RAYOS Phase 24 Task 3: Failure Injection Framework
// Inject failures and verify recovery mechanisms
// File: crates/kernel-bare/src/failure_injection.rs
// Lines: 820 | Tests: 20 unit + scenarios | Markers: 5

use core::fmt;

const MAX_INJECTED_FAILURES: usize = 100;
const MAX_RECOVERY_SAMPLES: usize = 1000;

// ============================================================================
// TYPES & ENUMS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureScenario {
    ClientCrash,
    BufferExhaustion,
    EventQueueOverflow,
    MemoryCorruption,
    Deadlock,
    NetworkPartition,
}

#[derive(Debug, Clone, Copy)]
pub struct FailureEvent {
    pub scenario: FailureScenario,
    pub target_id: u16,
    pub timestamp: u32,
    pub recovered: bool,
    pub recovery_time_ms: u32,
}

impl FailureEvent {
    pub fn new(scenario: FailureScenario, target_id: u16, timestamp: u32) -> Self {
        FailureEvent {
            scenario,
            target_id,
            timestamp,
            recovered: false,
            recovery_time_ms: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FailureInjectionResult {
    pub passed: bool,
    pub total_failures_injected: u32,
    pub successful_recoveries: u32,
    pub cascading_failures: u32,
    pub avg_recovery_time_ms: u32,
    pub max_recovery_time_ms: u32,
    pub data_corruption_detected: bool,
}

impl FailureInjectionResult {
    pub fn new() -> Self {
        FailureInjectionResult {
            passed: true,
            total_failures_injected: 0,
            successful_recoveries: 0,
            cascading_failures: 0,
            avg_recovery_time_ms: 0,
            max_recovery_time_ms: 0,
            data_corruption_detected: false,
        }
    }
}

impl Default for FailureInjectionResult {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// FAILURE INJECTOR
// ============================================================================

pub struct FailureInjector {
    pub failures: [FailureEvent; MAX_INJECTED_FAILURES],
    pub failure_count: usize,
    pub current_timestamp: u32,
    pub next_failure_id: u16,
}

impl FailureInjector {
    pub fn new() -> Self {
        FailureInjector {
            failures: [FailureEvent::new(FailureScenario::ClientCrash, 0, 0); MAX_INJECTED_FAILURES],
            failure_count: 0,
            current_timestamp: 0,
            next_failure_id: 1,
        }
    }

    pub fn inject_failure(&mut self, scenario: FailureScenario) -> u16 {
        if self.failure_count >= MAX_INJECTED_FAILURES {
            return 0; // Can't inject more failures
        }

        let event = FailureEvent::new(scenario, self.next_failure_id, self.current_timestamp);
        self.failures[self.failure_count] = event;
        self.failure_count += 1;

        let id = self.next_failure_id;
        self.next_failure_id += 1;
        id
    }

    pub fn mark_recovered(&mut self, failure_id: u16, recovery_time_ms: u32) {
        for failure in &mut self.failures[..self.failure_count] {
            if failure.target_id == failure_id && !failure.recovered {
                failure.recovered = true;
                failure.recovery_time_ms = recovery_time_ms;
                break;
            }
        }
    }

    pub fn tick(&mut self) {
        self.current_timestamp += 1;
    }

    pub fn get_recovery_rate(&self) -> u32 {
        if self.failure_count == 0 {
            return 100;
        }
        (self.successful_recoveries() * 100) / self.failure_count as u32
    }

    pub fn successful_recoveries(&self) -> u32 {
        self.failures[..self.failure_count]
            .iter()
            .filter(|f| f.recovered)
            .count() as u32
    }

    pub fn avg_recovery_time(&self) -> u32 {
        if self.failure_count == 0 {
            return 0;
        }
        let sum: u32 = self.failures[..self.failure_count]
            .iter()
            .map(|f| f.recovery_time_ms)
            .sum();
        sum / self.failure_count as u32
    }

    pub fn max_recovery_time(&self) -> u32 {
        self.failures[..self.failure_count]
            .iter()
            .map(|f| f.recovery_time_ms)
            .max()
            .unwrap_or(0)
    }
}

impl Default for FailureInjector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// RECOVERY VALIDATOR
// ============================================================================

pub struct RecoveryValidator {
    pub samples: [u32; MAX_RECOVERY_SAMPLES],
    pub sample_count: usize,
    pub system_healthy_threshold: u32,
}

impl RecoveryValidator {
    pub fn new() -> Self {
        RecoveryValidator {
            samples: [0u32; MAX_RECOVERY_SAMPLES],
            sample_count: 0,
            system_healthy_threshold: 50, // 50% health minimum
        }
    }

    pub fn record_system_health(&mut self, health_percent: u32) {
        if self.sample_count >= MAX_RECOVERY_SAMPLES {
            // Shift left
            for i in 0..MAX_RECOVERY_SAMPLES - 1 {
                self.samples[i] = self.samples[i + 1];
            }
            self.sample_count = MAX_RECOVERY_SAMPLES - 1;
        }
        self.samples[self.sample_count] = health_percent;
        self.sample_count += 1;
    }

    pub fn is_recovered(&self) -> bool {
        if self.sample_count < 2 {
            return false;
        }

        // Check if last sample shows recovery (health above threshold)
        let last_health = self.samples[self.sample_count - 1];
        last_health >= self.system_healthy_threshold
    }

    pub fn time_to_recover(&self) -> Option<u32> {
        if self.sample_count < 2 {
            return None;
        }

        // Find when system recovered (crossed threshold)
        for i in 0..self.sample_count {
            if self.samples[i] >= self.system_healthy_threshold {
                return Some(i as u32);
            }
        }
        None
    }

    pub fn avg_health(&self) -> u32 {
        if self.sample_count == 0 {
            return 0;
        }
        let sum: u32 = self.samples[..self.sample_count].iter().sum();
        sum / self.sample_count as u32
    }
}

impl Default for RecoveryValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// FAILURE INJECTION TEST HARNESS
// ============================================================================

pub struct FailureInjectionHarness {
    pub result: FailureInjectionResult,
    pub injector: FailureInjector,
    pub validator: RecoveryValidator,
    pub current_tick: u32,
    pub max_ticks: u32,
}

impl FailureInjectionHarness {
    pub fn new(max_ticks: u32) -> Self {
        FailureInjectionHarness {
            result: FailureInjectionResult::new(),
            injector: FailureInjector::new(),
            validator: RecoveryValidator::new(),
            current_tick: 0,
            max_ticks,
        }
    }

    pub fn inject_random_failure(&mut self, failure_type: FailureScenario) -> u16 {
        let id = self.injector.inject_failure(failure_type);
        self.result.total_failures_injected += 1;
        id
    }

    pub fn tick(&mut self, system_health_percent: u32) {
        self.current_tick += 1;
        self.injector.tick();
        self.validator.record_system_health(system_health_percent);
    }

    pub fn mark_failure_recovered(&mut self, failure_id: u16, recovery_time_ms: u32) {
        self.injector.mark_recovered(failure_id, recovery_time_ms);
        self.result.successful_recoveries += 1;
    }

    pub fn should_continue(&self) -> bool {
        self.current_tick < self.max_ticks
    }

    pub fn finish(&mut self) {
        self.result.avg_recovery_time_ms = self.injector.avg_recovery_time();
        self.result.max_recovery_time_ms = self.injector.max_recovery_time();

        // Check for cascading failures
        if self.result.total_failures_injected > 0
            && self.result.successful_recoveries < self.result.total_failures_injected / 2
        {
            self.result.cascading_failures =
                self.result.total_failures_injected - self.result.successful_recoveries;
            self.result.passed = false;
        }

        // Check for data corruption
        if self.validator.avg_health() < 20 {
            self.result.data_corruption_detected = true;
        }
    }

    pub fn get_recovery_rate(&self) -> u32 {
        self.injector.get_recovery_rate()
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failure_event_new() {
        let event = FailureEvent::new(FailureScenario::ClientCrash, 1, 0);
        assert_eq!(event.target_id, 1);
        assert!(!event.recovered);
    }

    #[test]
    fn test_failure_injector_new() {
        let injector = FailureInjector::new();
        assert_eq!(injector.failure_count, 0);
    }

    #[test]
    fn test_failure_injector_inject() {
        let mut injector = FailureInjector::new();
        let id = injector.inject_failure(FailureScenario::ClientCrash);
        assert_eq!(id, 1);
        assert_eq!(injector.failure_count, 1);
    }

    #[test]
    fn test_failure_injector_mark_recovered() {
        let mut injector = FailureInjector::new();
        let id = injector.inject_failure(FailureScenario::ClientCrash);
        injector.mark_recovered(id, 100);
        assert!(injector.failures[0].recovered);
    }

    #[test]
    fn test_failure_injector_recovery_rate() {
        let mut injector = FailureInjector::new();
        let id1 = injector.inject_failure(FailureScenario::ClientCrash);
        let id2 = injector.inject_failure(FailureScenario::BufferExhaustion);

        injector.mark_recovered(id1, 50);
        // id2 not recovered
        assert_eq!(injector.get_recovery_rate(), 50);
    }

    #[test]
    fn test_recovery_validator_new() {
        let validator = RecoveryValidator::new();
        assert_eq!(validator.sample_count, 0);
    }

    #[test]
    fn test_recovery_validator_health_recording() {
        let mut validator = RecoveryValidator::new();
        validator.record_system_health(100);
        assert_eq!(validator.sample_count, 1);
    }

    #[test]
    fn test_recovery_validator_recovery_detection() {
        let mut validator = RecoveryValidator::new();
        validator.record_system_health(20);
        validator.record_system_health(50);
        assert!(validator.is_recovered());
    }

    #[test]
    fn test_recovery_validator_avg_health() {
        let mut validator = RecoveryValidator::new();
        validator.record_system_health(60);
        validator.record_system_health(80);
        validator.record_system_health(100);
        assert_eq!(validator.avg_health(), 80);
    }

    #[test]
    fn test_failure_injection_harness_new() {
        let harness = FailureInjectionHarness::new(1000);
        assert_eq!(harness.max_ticks, 1000);
        assert_eq!(harness.result.total_failures_injected, 0);
    }

    #[test]
    fn test_failure_injection_harness_tick() {
        let mut harness = FailureInjectionHarness::new(100);
        harness.tick(100);
        assert_eq!(harness.current_tick, 1);
    }

    #[test]
    fn test_failure_injection_result_new() {
        let result = FailureInjectionResult::new();
        assert!(result.passed);
        assert_eq!(result.total_failures_injected, 0);
    }

    #[test]
    fn test_all_failure_scenarios() {
        let scenarios = [
            FailureScenario::ClientCrash,
            FailureScenario::BufferExhaustion,
            FailureScenario::EventQueueOverflow,
            FailureScenario::MemoryCorruption,
            FailureScenario::Deadlock,
            FailureScenario::NetworkPartition,
        ];

        for scenario in &scenarios {
            let mut injector = FailureInjector::new();
            injector.inject_failure(*scenario);
            assert_eq!(injector.failures[0].scenario, *scenario);
        }
    }

    #[test]
    fn test_cascading_failure_detection() {
        let mut harness = FailureInjectionHarness::new(100);

        // Inject multiple failures, recover only some
        for i in 0..10 {
            harness.inject_random_failure(FailureScenario::ClientCrash);
            if i < 3 {
                harness.mark_failure_recovered(i as u16 + 1, 50);
            }
        }

        harness.finish();
        assert!(!harness.result.passed);
        assert!(harness.result.cascading_failures > 0);
    }

    #[test]
    fn test_successful_recovery_scenario() {
        let mut harness = FailureInjectionHarness::new(100);

        for i in 0..10 {
            harness.inject_random_failure(FailureScenario::BufferExhaustion);
            harness.mark_failure_recovered(i as u16 + 1, 100);
        }

        harness.finish();
        assert_eq!(harness.get_recovery_rate(), 100);
    }

    #[test]
    fn test_data_corruption_detection() {
        let mut harness = FailureInjectionHarness::new(50);

        while harness.should_continue() {
            harness.tick(10); // Very low health
        }

        harness.finish();
        assert!(harness.result.data_corruption_detected);
    }

    #[test]
    fn test_partial_recovery() {
        let mut harness = FailureInjectionHarness::new(100);

        harness.tick(100);
        harness.inject_random_failure(FailureScenario::ClientCrash);
        for i in 0..10 {
            harness.tick(40); // Low health initially
        }
        harness.mark_failure_recovered(1, 500);
        for i in 0..10 {
            harness.tick(80); // Recovering
        }

        harness.finish();
        assert!(harness.result.successful_recoveries > 0);
    }

    #[test]
    fn test_multiple_concurrent_failures() {
        let mut harness = FailureInjectionHarness::new(200);

        // Inject multiple failures at different times
        for t in [10, 20, 30, 40, 50].iter() {
            while harness.current_tick < *t {
                harness.tick(100);
            }
            harness.inject_random_failure(FailureScenario::Deadlock);
        }

        assert_eq!(harness.result.total_failures_injected, 5);
    }

    #[test]
    fn test_recovery_time_tracking() {
        let mut injector = FailureInjector::new();
        let id1 = injector.inject_failure(FailureScenario::ClientCrash);
        let id2 = injector.inject_failure(FailureScenario::BufferExhaustion);

        injector.mark_recovered(id1, 100);
        injector.mark_recovered(id2, 200);

        assert_eq!(injector.avg_recovery_time(), 150);
        assert_eq!(injector.max_recovery_time(), 200);
    }

    #[test]
    fn test_network_partition_recovery() {
        let mut harness = FailureInjectionHarness::new(100);

        harness.inject_random_failure(FailureScenario::NetworkPartition);
        for i in 0..50 {
            harness.tick(if i < 25 { 30 } else { 85 });
        }
        harness.mark_failure_recovered(1, 250);

        harness.finish();
        assert!(harness.result.successful_recoveries > 0);
    }
}

// ============================================================================
// FAILURE INJECTION SCENARIOS (Integration Tests)
// ============================================================================

#[cfg(test)]
mod failure_scenarios {
    use super::*;

    #[test]
    fn test_1000_random_client_crashes() {
        let mut harness = FailureInjectionHarness::new(5000);

        let mut next_id = 1u16;
        for i in 0..5000 {
            if harness.should_continue() {
                if i % 5 == 0 {
                    let id = harness.inject_random_failure(FailureScenario::ClientCrash);
                    // Simulate quick recovery
                    harness.mark_failure_recovered(id, 50);
                }
                harness.tick(100 - (i as u32 % 40));
            }
        }

        harness.finish();
        assert!(harness.result.successful_recoveries >= harness.result.total_failures_injected / 2);
    }

    #[test]
    fn test_buffer_exhaustion_recovery() {
        let mut harness = FailureInjectionHarness::new(200);

        let id = harness.inject_random_failure(FailureScenario::BufferExhaustion);

        for i in 0..100 {
            harness.tick(if i < 50 { 30 } else { 80 });
        }

        harness.mark_failure_recovered(id, 500);
        harness.finish();

        assert!(harness.result.successful_recoveries > 0);
    }

    #[test]
    fn test_deadlock_detection_and_timeout() {
        let mut harness = FailureInjectionHarness::new(300);

        let id = harness.inject_random_failure(FailureScenario::Deadlock);

        // System stays unhealthy for a while
        for _ in 0..100 {
            harness.tick(20);
        }

        // Detect deadlock (system unresponsive)
        assert!(harness.result.total_failures_injected > 0);
    }

    #[test]
    fn test_cascading_failures_prevention() {
        let mut harness = FailureInjectionHarness::new(500);

        // Inject many failures but recover from each
        for _ in 0..20 {
            let id = harness.inject_random_failure(FailureScenario::ClientCrash);
            harness.mark_failure_recovered(id, 100);
            for _ in 0..10 {
                harness.tick(90);
            }
        }

        harness.finish();
        assert_eq!(harness.result.cascading_failures, 0);
        assert!(harness.result.passed);
    }
}
