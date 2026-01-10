//! Boot Markers & Telemetry System for Ouroboros Engine
//!
//! Emits RAYOS_OUROBOROS prefixed boot markers to track evolution cycles.
//! Collects and aggregates metrics from all evolution phases.
//!
//! Phase 32, Task 1


/// Evolution marker types emitted during self-optimization
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EvolutionMarker {
    /// Cycle starts - cycle_id available
    CycleStart = 1,
    /// New mutation generated - mutation_id, severity available
    MutationGenerated = 2,
    /// Sandbox test begins - test_id, mutation_id available
    TestStarted = 3,
    /// Test completed - test_id, pass/fail status
    TestCompleted = 4,
    /// Fitness evaluated - fitness_score, improvement_delta
    FitnessEvaluated = 5,
    /// Mutation approved by selector - selection_method
    SelectionApproved = 6,
    /// Live patch deployed - version_id, patch_count
    PatchApplied = 7,
    /// Cycle finished - total_duration_ms, mutations_generated
    CycleComplete = 8,
    /// Dream session activated - idle_duration_ms
    DreamSessionStart = 9,
    /// Dream session concluded - cycles_executed, improvements_found
    DreamSessionEnd = 10,
    /// Performance regression detected - regression_percent
    RegressionDetected = 11,
    /// Mutation rolled back - rollback_reason_id
    RollbackExecuted = 12,
}

impl EvolutionMarker {
    /// Convert to boot marker string (e.g., "RAYOS_OUROBOROS_CYCLE_START")
    pub const fn to_marker_name(self) -> &'static str {
        match self {
            EvolutionMarker::CycleStart => "RAYOS_OUROBOROS_CYCLE_START",
            EvolutionMarker::MutationGenerated => "RAYOS_OUROBOROS_MUTATION_GENERATED",
            EvolutionMarker::TestStarted => "RAYOS_OUROBOROS_TEST_STARTED",
            EvolutionMarker::TestCompleted => "RAYOS_OUROBOROS_TEST_COMPLETED",
            EvolutionMarker::FitnessEvaluated => "RAYOS_OUROBOROS_FITNESS_EVALUATED",
            EvolutionMarker::SelectionApproved => "RAYOS_OUROBOROS_SELECTION_APPROVED",
            EvolutionMarker::PatchApplied => "RAYOS_OUROBOROS_PATCH_APPLIED",
            EvolutionMarker::CycleComplete => "RAYOS_OUROBOROS_CYCLE_COMPLETE",
            EvolutionMarker::DreamSessionStart => "RAYOS_OUROBOROS_DREAM_SESSION_START",
            EvolutionMarker::DreamSessionEnd => "RAYOS_OUROBOROS_DREAM_SESSION_END",
            EvolutionMarker::RegressionDetected => "RAYOS_OUROBOROS_REGRESSION_DETECTED",
            EvolutionMarker::RollbackExecuted => "RAYOS_OUROBOROS_ROLLBACK_EXECUTED",
        }
    }
}

/// Marker data with encoded values
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkerData {
    /// Marker type
    pub marker: EvolutionMarker,
    /// Cycle ID (32-bit)
    pub cycle_id: u32,
    /// First 32-bit value (ID, count, or percentage)
    pub value1: u32,
    /// Second 32-bit value (status, reason, or delta)
    pub value2: u32,
}

impl MarkerData {
    /// Create new marker data
    pub const fn new(
        marker: EvolutionMarker,
        cycle_id: u32,
        value1: u32,
        value2: u32,
    ) -> Self {
        MarkerData {
            marker,
            cycle_id,
            value1,
            value2,
        }
    }

    /// Encode to binary format (12 bytes: 1 marker + 1 pad + 4*cycle_id + 4*value1 + 4*value2)
    pub fn encode(&self) -> [u8; 13] {
        let mut buffer = [0u8; 13];
        buffer[0] = self.marker as u8;
        buffer[1] = 0; // padding
        buffer[2..6].copy_from_slice(&self.cycle_id.to_le_bytes());
        buffer[6..10].copy_from_slice(&self.value1.to_le_bytes());
        buffer[10..13].copy_from_slice(&self.value2.to_le_bytes()[0..3]); // 3 bytes of value2
        buffer
    }

    /// Decode from binary format
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 13 {
            return None;
        }
        let marker = match bytes[0] {
            1 => EvolutionMarker::CycleStart,
            2 => EvolutionMarker::MutationGenerated,
            3 => EvolutionMarker::TestStarted,
            4 => EvolutionMarker::TestCompleted,
            5 => EvolutionMarker::FitnessEvaluated,
            6 => EvolutionMarker::SelectionApproved,
            7 => EvolutionMarker::PatchApplied,
            8 => EvolutionMarker::CycleComplete,
            9 => EvolutionMarker::DreamSessionStart,
            10 => EvolutionMarker::DreamSessionEnd,
            11 => EvolutionMarker::RegressionDetected,
            12 => EvolutionMarker::RollbackExecuted,
            _ => return None,
        };
        let mut cycle_bytes = [0u8; 4];
        cycle_bytes.copy_from_slice(&bytes[2..6]);
        let cycle_id = u32::from_le_bytes(cycle_bytes);

        let mut v1_bytes = [0u8; 4];
        v1_bytes.copy_from_slice(&bytes[6..10]);
        let value1 = u32::from_le_bytes(v1_bytes);

        let mut v2_bytes = [0u8; 4];
        v2_bytes[0..3].copy_from_slice(&bytes[10..13]);
        let value2 = u32::from_le_bytes(v2_bytes);

        Some(MarkerData {
            marker,
            cycle_id,
            value1,
            value2,
        })
    }
}

/// Marker entry with timestamp
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkerEntry {
    pub marker: MarkerData,
    /// Timestamp in milliseconds (approximate)
    pub timestamp_ms: u32,
}

/// Ring buffer for evolution cycle history (last 256 cycles)
pub struct CycleHistory {
    entries: [Option<MarkerEntry>; 256],
    write_pos: usize,
    count: usize,
}

impl CycleHistory {
    /// Create new empty history
    pub const fn new() -> Self {
        CycleHistory {
            entries: [None; 256],
            write_pos: 0,
            count: 0,
        }
    }

    /// Add marker entry to history
    pub fn add_entry(&mut self, entry: MarkerEntry) {
        self.entries[self.write_pos] = Some(entry);
        self.write_pos = (self.write_pos + 1) % 256;
        if self.count < 256 {
            self.count += 1;
        }
    }

    /// Get entry at index (0 = oldest, count-1 = newest)
    pub fn get(&self, index: usize) -> Option<MarkerEntry> {
        if index >= self.count {
            return None;
        }
        let actual_pos = (self.write_pos + 256 - self.count + index) % 256;
        self.entries[actual_pos]
    }

    /// Get total entries in history
    pub fn len(&self) -> usize {
        self.count
    }

    /// Get most recent entry of type
    pub fn last_of_type(&self, marker_type: EvolutionMarker) -> Option<MarkerEntry> {
        for i in (0..self.count).rev() {
            if let Some(entry) = self.get(i) {
                if entry.marker.marker == marker_type {
                    return Some(entry);
                }
            }
        }
        None
    }

    /// Count entries of specific type
    pub fn count_of_type(&self, marker_type: EvolutionMarker) -> usize {
        let mut count = 0;
        for i in 0..self.count {
            if let Some(entry) = self.get(i) {
                if entry.marker.marker == marker_type {
                    count += 1;
                }
            }
        }
        count
    }

    /// Clear history
    pub fn clear(&mut self) {
        self.entries = [None; 256];
        self.write_pos = 0;
        self.count = 0;
    }
}

/// Statistics aggregated from evolution cycles
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryStats {
    /// Total cycles completed
    pub total_cycles: u32,
    /// Total mutations generated
    pub total_mutations_generated: u32,
    /// Total tests executed
    pub total_tests_executed: u32,
    /// Total patches applied
    pub total_patches_applied: u32,
    /// Total regressions detected
    pub total_regressions: u32,
    /// Total rollbacks executed
    pub total_rollbacks: u32,
    /// Sum of all cycle durations (ms)
    pub total_cycle_time_ms: u32,
    /// Sum of all fitness improvements (scaled by 1000)
    pub total_fitness_improvement: u32,
    /// Average mutations per cycle (scaled by 1000)
    pub avg_mutations_per_cycle: u32,
}

impl TelemetryStats {
    /// Create new empty stats
    pub const fn new() -> Self {
        TelemetryStats {
            total_cycles: 0,
            total_mutations_generated: 0,
            total_tests_executed: 0,
            total_patches_applied: 0,
            total_regressions: 0,
            total_rollbacks: 0,
            total_cycle_time_ms: 0,
            total_fitness_improvement: 0,
            avg_mutations_per_cycle: 0,
        }
    }

    /// Update average mutations per cycle
    fn update_avg_mutations(&mut self) {
        if self.total_cycles > 0 {
            self.avg_mutations_per_cycle =
                (self.total_mutations_generated as u64 * 1000 / self.total_cycles as u64) as u32;
        }
    }
}

/// Collects and aggregates telemetry from evolution cycles
pub struct TelemetryCollector {
    /// Ring buffer history of markers
    pub history: CycleHistory,
    /// Aggregated statistics
    pub stats: TelemetryStats,
    /// Current cycle ID
    current_cycle: u32,
}

impl TelemetryCollector {
    /// Create new telemetry collector
    pub const fn new() -> Self {
        TelemetryCollector {
            history: CycleHistory::new(),
            stats: TelemetryStats::new(),
            current_cycle: 0,
        }
    }

    /// Record cycle start
    pub fn record_cycle_start(&mut self, cycle_id: u32) {
        self.current_cycle = cycle_id;
        let marker = MarkerData::new(EvolutionMarker::CycleStart, cycle_id, 0, 0);
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0, // would be filled by caller
        };
        self.history.add_entry(entry);
    }

    /// Record mutation generated
    pub fn record_mutation_generated(&mut self, mutation_id: u32, severity: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::MutationGenerated,
            self.current_cycle,
            mutation_id,
            severity,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
        self.stats.total_mutations_generated += 1;
    }

    /// Record test started
    pub fn record_test_started(&mut self, test_id: u32, mutation_id: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::TestStarted,
            self.current_cycle,
            test_id,
            mutation_id,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
    }

    /// Record test completed
    pub fn record_test_completed(&mut self, test_id: u32, passed: bool) {
        let status = if passed { 1 } else { 0 };
        let marker = MarkerData::new(EvolutionMarker::TestCompleted, self.current_cycle, test_id, status);
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
        self.stats.total_tests_executed += 1;
    }

    /// Record fitness evaluated
    pub fn record_fitness_evaluated(&mut self, fitness_score: u32, improvement_delta: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::FitnessEvaluated,
            self.current_cycle,
            fitness_score,
            improvement_delta,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
        self.stats.total_fitness_improvement += improvement_delta;
    }

    /// Record selection approved
    pub fn record_selection_approved(&mut self, selection_method: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::SelectionApproved,
            self.current_cycle,
            selection_method,
            0,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
    }

    /// Record patch applied
    pub fn record_patch_applied(&mut self, version_id: u32, patch_count: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::PatchApplied,
            self.current_cycle,
            version_id,
            patch_count,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
        self.stats.total_patches_applied += 1;
    }

    /// Record cycle complete
    pub fn record_cycle_complete(&mut self, duration_ms: u32, mutations_generated: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::CycleComplete,
            self.current_cycle,
            duration_ms,
            mutations_generated,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
        self.stats.total_cycles = self.stats.total_cycles.saturating_add(1);
        self.stats.total_cycle_time_ms = self.stats.total_cycle_time_ms.saturating_add(duration_ms);
        self.stats.update_avg_mutations();
    }

    /// Record dream session start
    pub fn record_dream_session_start(&mut self, idle_duration_ms: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::DreamSessionStart,
            self.current_cycle,
            idle_duration_ms,
            0,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
    }

    /// Record dream session end
    pub fn record_dream_session_end(&mut self, cycles_executed: u32, improvements_found: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::DreamSessionEnd,
            self.current_cycle,
            cycles_executed,
            improvements_found,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
    }

    /// Record regression detected
    pub fn record_regression_detected(&mut self, regression_percent: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::RegressionDetected,
            self.current_cycle,
            regression_percent,
            0,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
        self.stats.total_regressions += 1;
    }

    /// Record rollback executed
    pub fn record_rollback_executed(&mut self, rollback_reason: u32) {
        let marker = MarkerData::new(
            EvolutionMarker::RollbackExecuted,
            self.current_cycle,
            rollback_reason,
            0,
        );
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 0,
        };
        self.history.add_entry(entry);
        self.stats.total_rollbacks += 1;
    }

    /// Get mutation acceptance rate (percent * 1000)
    pub fn mutation_acceptance_rate(&self) -> u32 {
        if self.stats.total_mutations_generated == 0 {
            return 0;
        }
        (self.stats.total_patches_applied as u64 * 1000
            / self.stats.total_mutations_generated as u64) as u32
    }

    /// Get average cycle duration (ms)
    pub fn avg_cycle_duration_ms(&self) -> u32 {
        if self.stats.total_cycles == 0 {
            return 0;
        }
        self.stats.total_cycle_time_ms / self.stats.total_cycles
    }

    /// Clear all telemetry data
    pub fn clear(&mut self) {
        self.history.clear();
        self.stats = TelemetryStats::new();
        self.current_cycle = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evolution_marker_names() {
        assert_eq!(
            EvolutionMarker::CycleStart.to_marker_name(),
            "RAYOS_OUROBOROS_CYCLE_START"
        );
        assert_eq!(
            EvolutionMarker::MutationGenerated.to_marker_name(),
            "RAYOS_OUROBOROS_MUTATION_GENERATED"
        );
        assert_eq!(
            EvolutionMarker::RollbackExecuted.to_marker_name(),
            "RAYOS_OUROBOROS_ROLLBACK_EXECUTED"
        );
    }

    #[test]
    fn test_marker_data_encode_decode() {
        let marker = MarkerData::new(EvolutionMarker::CycleStart, 42, 100, 200);
        let encoded = marker.encode();
        assert_eq!(encoded.len(), 13);
        assert_eq!(encoded[0], 1); // CycleStart

        let decoded = MarkerData::decode(&encoded).unwrap();
        assert_eq!(decoded.marker, EvolutionMarker::CycleStart);
        assert_eq!(decoded.cycle_id, 42);
        assert_eq!(decoded.value1, 100);
    }

    #[test]
    fn test_marker_data_invalid_decode() {
        let bytes = [0u8; 5];
        assert!(MarkerData::decode(&bytes).is_none());

        let mut bytes = [0u8; 13];
        bytes[0] = 99; // invalid marker
        assert!(MarkerData::decode(&bytes).is_none());
    }

    #[test]
    fn test_cycle_history_add_and_retrieve() {
        let mut history = CycleHistory::new();
        assert_eq!(history.len(), 0);

        let entry1 = MarkerEntry {
            marker: MarkerData::new(EvolutionMarker::CycleStart, 1, 0, 0),
            timestamp_ms: 100,
        };
        history.add_entry(entry1);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap(), entry1);
    }

    #[test]
    fn test_cycle_history_wraparound() {
        let mut history = CycleHistory::new();

        // Fill entire buffer
        for i in 0..256 {
            let entry = MarkerEntry {
                marker: MarkerData::new(EvolutionMarker::CycleStart, i as u32, 0, 0),
                timestamp_ms: i as u32,
            };
            history.add_entry(entry);
        }
        assert_eq!(history.len(), 256);

        // Add one more, should wraparound
        let new_entry = MarkerEntry {
            marker: MarkerData::new(EvolutionMarker::MutationGenerated, 256, 0, 0),
            timestamp_ms: 256,
        };
        history.add_entry(new_entry);
        assert_eq!(history.len(), 256); // still 256
        assert_eq!(history.get(255).unwrap().marker.cycle_id, 256); // newest
    }

    #[test]
    fn test_cycle_history_last_of_type() {
        let mut history = CycleHistory::new();

        history.add_entry(MarkerEntry {
            marker: MarkerData::new(EvolutionMarker::CycleStart, 1, 0, 0),
            timestamp_ms: 10,
        });
        history.add_entry(MarkerEntry {
            marker: MarkerData::new(EvolutionMarker::MutationGenerated, 1, 5, 0),
            timestamp_ms: 20,
        });
        history.add_entry(MarkerEntry {
            marker: MarkerData::new(EvolutionMarker::CycleStart, 2, 0, 0),
            timestamp_ms: 30,
        });

        let last = history.last_of_type(EvolutionMarker::CycleStart).unwrap();
        assert_eq!(last.marker.cycle_id, 2);

        let last_mut = history.last_of_type(EvolutionMarker::MutationGenerated).unwrap();
        assert_eq!(last_mut.marker.value1, 5);
    }

    #[test]
    fn test_cycle_history_count_of_type() {
        let mut history = CycleHistory::new();

        for _ in 0..3 {
            history.add_entry(MarkerEntry {
                marker: MarkerData::new(EvolutionMarker::CycleStart, 1, 0, 0),
                timestamp_ms: 0,
            });
        }
        history.add_entry(MarkerEntry {
            marker: MarkerData::new(EvolutionMarker::MutationGenerated, 1, 0, 0),
            timestamp_ms: 0,
        });

        assert_eq!(history.count_of_type(EvolutionMarker::CycleStart), 3);
        assert_eq!(history.count_of_type(EvolutionMarker::MutationGenerated), 1);
        assert_eq!(history.count_of_type(EvolutionMarker::TestStarted), 0);
    }

    #[test]
    fn test_telemetry_stats_creation() {
        let stats = TelemetryStats::new();
        assert_eq!(stats.total_cycles, 0);
        assert_eq!(stats.total_mutations_generated, 0);
        assert_eq!(stats.total_patches_applied, 0);
    }

    #[test]
    fn test_telemetry_collector_basic() {
        let mut collector = TelemetryCollector::new();
        collector.record_cycle_start(1);
        assert_eq!(collector.stats.total_cycles, 0); // not yet complete

        collector.record_mutation_generated(10, 1);
        assert_eq!(collector.stats.total_mutations_generated, 1);

        collector.record_test_started(20, 10);
        collector.record_test_completed(20, true);
        assert_eq!(collector.stats.total_tests_executed, 1);

        collector.record_patch_applied(1, 1);
        assert_eq!(collector.stats.total_patches_applied, 1);

        collector.record_cycle_complete(100, 1);
        assert_eq!(collector.stats.total_cycles, 1);
        assert_eq!(collector.stats.total_cycle_time_ms, 100);
    }

    #[test]
    fn test_telemetry_mutation_acceptance_rate() {
        let mut collector = TelemetryCollector::new();
        collector.stats.total_mutations_generated = 100;
        collector.stats.total_patches_applied = 50;

        let rate = collector.mutation_acceptance_rate();
        assert_eq!(rate, 500); // 50%
    }

    #[test]
    fn test_telemetry_mutation_acceptance_rate_zero_mutations() {
        let collector = TelemetryCollector::new();
        assert_eq!(collector.mutation_acceptance_rate(), 0);
    }

    #[test]
    fn test_telemetry_avg_cycle_duration() {
        let mut collector = TelemetryCollector::new();
        collector.stats.total_cycles = 5;
        collector.stats.total_cycle_time_ms = 500;

        let avg = collector.avg_cycle_duration_ms();
        assert_eq!(avg, 100);
    }

    #[test]
    fn test_telemetry_avg_mutations_per_cycle() {
        let mut collector = TelemetryCollector::new();
        collector.stats.total_cycles = 10;
        collector.stats.total_mutations_generated = 50;
        collector.stats.update_avg_mutations();

        assert_eq!(collector.stats.avg_mutations_per_cycle, 5000); // 5 * 1000
    }

    #[test]
    fn test_telemetry_record_dream_session() {
        let mut collector = TelemetryCollector::new();
        collector.record_cycle_start(1);
        collector.record_dream_session_start(300000); // 5 minutes
        collector.record_dream_session_end(3, 2);

        assert_eq!(collector.history.count_of_type(EvolutionMarker::DreamSessionStart), 1);
        assert_eq!(
            collector.history.count_of_type(EvolutionMarker::DreamSessionEnd),
            1
        );
    }

    #[test]
    fn test_telemetry_record_regression_and_rollback() {
        let mut collector = TelemetryCollector::new();
        collector.record_cycle_start(1);
        collector.record_regression_detected(5); // 5% regression
        assert_eq!(collector.stats.total_regressions, 1);

        collector.record_rollback_executed(1); // reason 1
        assert_eq!(collector.stats.total_rollbacks, 1);

        let last_rollback = collector
            .history
            .last_of_type(EvolutionMarker::RollbackExecuted);
        assert!(last_rollback.is_some());
    }

    #[test]
    fn test_telemetry_clear() {
        let mut collector = TelemetryCollector::new();
        collector.record_cycle_start(1);
        collector.record_mutation_generated(10, 1);
        assert_eq!(collector.stats.total_mutations_generated, 1);
        assert_eq!(collector.history.len(), 2);

        collector.clear();
        assert_eq!(collector.stats.total_mutations_generated, 0);
        assert_eq!(collector.history.len(), 0);
        assert_eq!(collector.current_cycle, 0);
    }

    #[test]
    fn test_telemetry_integration_full_cycle() {
        let mut collector = TelemetryCollector::new();

        // Simulate full evolution cycle
        collector.record_cycle_start(1);
        for i in 0..5 {
            collector.record_mutation_generated(i, 1);
        }

        for i in 0..5 {
            collector.record_test_started(i, i);
            collector.record_test_completed(i, i < 3); // first 3 pass
        }

        for i in 0..3 {
            collector.record_fitness_evaluated(100 + i * 10, 10);
            collector.record_selection_approved(1);
            collector.record_patch_applied(1, 1);
        }

        collector.record_cycle_complete(250, 5);

        // Verify stats
        assert_eq!(collector.stats.total_cycles, 1);
        assert_eq!(collector.stats.total_mutations_generated, 5);
        assert_eq!(collector.stats.total_tests_executed, 5);
        assert_eq!(collector.stats.total_patches_applied, 3);
        assert_eq!(collector.stats.total_cycle_time_ms, 250);

        // Verify history
        assert_eq!(collector.history.len(), 18); // 1 start + 5 mutations + 5 tests + 3*4 for evaluate/approve/patch + 1 complete
        assert!(collector
            .history
            .last_of_type(EvolutionMarker::CycleStart)
            .is_some());
    }

    #[test]
    fn test_telemetry_fitness_improvement_accumulation() {
        let mut collector = TelemetryCollector::new();
        collector.record_cycle_start(1);

        collector.record_fitness_evaluated(100, 10);
        collector.record_fitness_evaluated(110, 15);
        collector.record_fitness_evaluated(125, 20);

        assert_eq!(collector.stats.total_fitness_improvement, 45);
    }

    #[test]
    fn test_telemetry_saturation_safety() {
        let mut collector = TelemetryCollector::new();
        collector.stats.total_cycle_time_ms = u32::MAX - 50;

        collector.record_cycle_complete(100, 1);

        // Should saturate, not overflow
        assert_eq!(collector.stats.total_cycle_time_ms, u32::MAX);
    }

    #[test]
    fn test_marker_entry_creation() {
        let marker = MarkerData::new(EvolutionMarker::PatchApplied, 10, 42, 3);
        let entry = MarkerEntry {
            marker,
            timestamp_ms: 1000,
        };

        assert_eq!(entry.marker.cycle_id, 10);
        assert_eq!(entry.marker.value1, 42);
        assert_eq!(entry.marker.value2, 3);
        assert_eq!(entry.timestamp_ms, 1000);
    }
}
