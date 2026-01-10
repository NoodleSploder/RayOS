//! Selection Arena: Sandbox Testing and Fitness Scoring
//!
//! This module implements the sandbox testing environment where mutations are validated
//! before affecting the live system. It provides comprehensive fitness evaluation across
//! multiple metrics including performance, memory usage, correctness, and energy consumption.
//!
//! # Architecture
//!
//! The selection system operates in phases:
//! 1. **Sandbox Setup** - Create isolated execution environment
//! 2. **Test Execution** - Run mutation against test suites
//! 3. **Fitness Scoring** - Evaluate multi-objective fitness
//! 4. **Tournament Selection** - Compare mutations against baseline and each other
//! 5. **Winner Selection** - Identify best mutations for live patching
//!
//! # Boot Markers
//!
//! - `RAYOS_OUROBOROS:TESTED` - Mutation tested in sandbox
//! - `RAYOS_OUROBOROS:SCORED` - Fitness score calculated
//! - `RAYOS_OUROBOROS:SELECTED` - Mutation selected as winner

use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use crate::ouroboros::{EvolutionResult, Checkpoint, CheckpointData, Checkpointable};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum number of test cases per suite
pub const MAX_TEST_CASES: usize = 256;

/// Maximum number of benchmarks
pub const MAX_BENCHMARKS: usize = 64;

/// Maximum number of fitness metrics
pub const MAX_FITNESS_METRICS: usize = 16;

/// Threshold for test pass rate (must be 100% for acceptance)
pub const MIN_TEST_PASS_RATE: f32 = 100.0;

/// Maximum memory overhead allowed (% of baseline)
pub const MAX_MEMORY_OVERHEAD: f32 = 5.0;

/// Performance regression threshold (% slower than baseline)
pub const MAX_PERFORMANCE_REGRESSION: f32 = 2.0;

// ============================================================================
// TEST INFRASTRUCTURE
// ============================================================================

/// Test case for regression testing
#[derive(Clone, Copy, Debug)]
pub struct TestCase {
    /// Unique test ID
    pub id: u32,
    /// Test name
    pub name: [u8; 64],
    pub name_len: u8,
    /// Expected output/assertion
    pub expected: [u8; 256],
    pub expected_len: u16,
    /// Timeout in milliseconds
    pub timeout_ms: u32,
    /// Test category
    pub category: TestCategory,
    /// Pass count
    pub pass_count: u32,
    /// Fail count
    pub fail_count: u32,
}

/// Test categories
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TestCategory {
    /// Unit test
    Unit = 0,
    /// Integration test
    Integration = 1,
    /// Regression test
    Regression = 2,
    /// Performance test
    Performance = 3,
    /// Memory test
    Memory = 4,
    /// Stress test
    Stress = 5,
}

impl TestCase {
    /// Create a new test case
    pub fn new(id: u32, category: TestCategory) -> Self {
        Self {
            id,
            name: [0u8; 64],
            name_len: 0,
            expected: [0u8; 256],
            expected_len: 0,
            timeout_ms: 5000,
            category,
            pass_count: 0,
            fail_count: 0,
        }
    }

    /// Set test name
    pub fn set_name(&mut self, name: &[u8]) -> Result<(), EvolutionResult> {
        if name.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.name[..name.len()].copy_from_slice(name);
        self.name_len = name.len() as u8;
        Ok(())
    }

    /// Set expected output
    pub fn set_expected(&mut self, expected: &[u8]) -> Result<(), EvolutionResult> {
        if expected.len() > 256 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.expected[..expected.len()].copy_from_slice(expected);
        self.expected_len = expected.len() as u16;
        Ok(())
    }

    /// Record test pass
    pub fn record_pass(&mut self) {
        self.pass_count = self.pass_count.saturating_add(1);
    }

    /// Record test fail
    pub fn record_fail(&mut self) {
        self.fail_count = self.fail_count.saturating_add(1);
    }

    /// Get pass rate
    pub fn pass_rate(&self) -> f32 {
        let total = self.pass_count as f32 + self.fail_count as f32;
        if total == 0.0 {
            return 0.0;
        }
        (self.pass_count as f32 / total) * 100.0
    }
}

// ============================================================================
// TEST SUITE
// ============================================================================

/// Collection of test cases
#[derive(Clone, Copy, Debug)]
pub struct TestSuite {
    /// Suite ID
    pub id: u32,
    /// Suite name
    pub name: [u8; 64],
    pub name_len: u8,
    /// Number of test cases
    pub test_count: u32,
    /// Total passes
    pub total_passes: u32,
    /// Total failures
    pub total_failures: u32,
    /// Last run timestamp
    pub last_run: u64,
    /// Expected duration (ms)
    pub expected_duration_ms: u32,
}

impl TestSuite {
    /// Create a new test suite
    pub fn new(id: u32) -> Self {
        Self {
            id,
            name: [0u8; 64],
            name_len: 0,
            test_count: 0,
            total_passes: 0,
            total_failures: 0,
            last_run: 0,
            expected_duration_ms: 0,
        }
    }

    /// Set suite name
    pub fn set_name(&mut self, name: &[u8]) -> Result<(), EvolutionResult> {
        if name.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.name[..name.len()].copy_from_slice(name);
        self.name_len = name.len() as u8;
        Ok(())
    }

    /// Add a test case
    pub fn add_test(&mut self) -> Result<(), EvolutionResult> {
        if self.test_count >= MAX_TEST_CASES as u32 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.test_count = self.test_count.saturating_add(1);
        Ok(())
    }

    /// Record test results
    pub fn record_results(&mut self, passes: u32, failures: u32) {
        self.total_passes = self.total_passes.saturating_add(passes);
        self.total_failures = self.total_failures.saturating_add(failures);
    }

    /// Get overall pass rate
    pub fn pass_rate(&self) -> f32 {
        let total = self.total_passes as f32 + self.total_failures as f32;
        if total == 0.0 {
            return 0.0;
        }
        (self.total_passes as f32 / total) * 100.0
    }

    /// Is suite healthy (100% pass rate)?
    pub fn is_healthy(&self) -> bool {
        (self.pass_rate() - 100.0).abs() < 0.01
    }
}

// ============================================================================
// BENCHMARK INFRASTRUCTURE
// ============================================================================

/// A performance benchmark
#[derive(Clone, Copy, Debug)]
pub struct Benchmark {
    /// Benchmark ID
    pub id: u32,
    /// Benchmark name
    pub name: [u8; 64],
    pub name_len: u8,
    /// Function/region being benchmarked
    pub target: [u8; 64],
    pub target_len: u8,
    /// Iterations to run
    pub iterations: u32,
    /// Baseline execution time (microseconds)
    pub baseline_us: u64,
    /// Last measured time (microseconds)
    pub last_measurement_us: u64,
    /// Best time recorded (microseconds)
    pub best_us: u64,
    /// Worst time recorded (microseconds)
    pub worst_us: u64,
    /// Run count
    pub run_count: u32,
}

impl Benchmark {
    /// Create a new benchmark
    pub fn new(id: u32) -> Self {
        Self {
            id,
            name: [0u8; 64],
            name_len: 0,
            target: [0u8; 64],
            target_len: 0,
            iterations: 1000,
            baseline_us: 0,
            last_measurement_us: 0,
            best_us: u64::MAX,
            worst_us: 0,
            run_count: 0,
        }
    }

    /// Set benchmark name
    pub fn set_name(&mut self, name: &[u8]) -> Result<(), EvolutionResult> {
        if name.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.name[..name.len()].copy_from_slice(name);
        self.name_len = name.len() as u8;
        Ok(())
    }

    /// Set target
    pub fn set_target(&mut self, target: &[u8]) -> Result<(), EvolutionResult> {
        if target.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.target[..target.len()].copy_from_slice(target);
        self.target_len = target.len() as u8;
        Ok(())
    }

    /// Record measurement
    pub fn record_measurement(&mut self, time_us: u64) {
        self.last_measurement_us = time_us;
        if time_us < self.best_us {
            self.best_us = time_us;
        }
        if time_us > self.worst_us {
            self.worst_us = time_us;
        }
        self.run_count = self.run_count.saturating_add(1);
    }

    /// Get speedup vs baseline
    pub fn speedup_vs_baseline(&self) -> f32 {
        if self.baseline_us == 0 {
            return 1.0;
        }
        self.baseline_us as f32 / self.last_measurement_us as f32
    }

    /// Get average time across all measurements
    pub fn avg_time_us(&self) -> u64 {
        if self.best_us == u64::MAX || self.worst_us == 0 {
            return self.last_measurement_us;
        }
        (self.best_us + self.worst_us) / 2
    }
}

// ============================================================================
// BENCHMARK SUITE
// ============================================================================

/// Collection of benchmarks
#[derive(Clone, Copy, Debug)]
pub struct BenchmarkSuite {
    /// Suite ID
    pub id: u32,
    /// Suite name
    pub name: [u8; 64],
    pub name_len: u8,
    /// Number of benchmarks
    pub benchmark_count: u32,
    /// Total execution time across all benchmarks (us)
    pub total_time_us: u64,
    /// Last run timestamp
    pub last_run: u64,
}

impl BenchmarkSuite {
    /// Create a new benchmark suite
    pub fn new(id: u32) -> Self {
        Self {
            id,
            name: [0u8; 64],
            name_len: 0,
            benchmark_count: 0,
            total_time_us: 0,
            last_run: 0,
        }
    }

    /// Set suite name
    pub fn set_name(&mut self, name: &[u8]) -> Result<(), EvolutionResult> {
        if name.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.name[..name.len()].copy_from_slice(name);
        self.name_len = name.len() as u8;
        Ok(())
    }

    /// Add a benchmark
    pub fn add_benchmark(&mut self) -> Result<(), EvolutionResult> {
        if self.benchmark_count >= MAX_BENCHMARKS as u32 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.benchmark_count = self.benchmark_count.saturating_add(1);
        Ok(())
    }

    /// Update total time
    pub fn add_time(&mut self, time_us: u64) {
        self.total_time_us = self.total_time_us.saturating_add(time_us);
    }

    /// Get average time per benchmark
    pub fn avg_time_per_benchmark_us(&self) -> u64 {
        if self.benchmark_count == 0 {
            return 0;
        }
        self.total_time_us / self.benchmark_count as u64
    }
}

// ============================================================================
// FITNESS METRICS
// ============================================================================

/// Individual fitness metric
#[derive(Clone, Copy, Debug)]
pub struct FitnessMetric {
    /// Metric ID
    pub id: u8,
    /// Metric type
    pub metric_type: MetricType,
    /// Raw value
    pub value: f32,
    /// Weight for composite scoring (0-255)
    pub weight: u8,
    /// Whether higher is better
    pub higher_is_better: bool,
}

/// Types of fitness metrics
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MetricType {
    /// Execution time (seconds)
    ExecutionTime = 0,
    /// Memory usage (bytes)
    MemoryUsage = 1,
    /// Code size (bytes)
    CodeSize = 2,
    /// Test pass rate (%)
    TestPassRate = 3,
    /// Energy consumption (millijoules)
    EnergyConsumption = 4,
    /// Cache hit rate (%)
    CacheHitRate = 5,
    /// Branch prediction rate (%)
    BranchPredictionRate = 6,
    /// Memory bandwidth (GB/s)
    MemoryBandwidth = 7,
}

impl FitnessMetric {
    /// Create a new fitness metric
    pub fn new(id: u8, metric_type: MetricType) -> Self {
        Self {
            id,
            metric_type,
            value: 0.0,
            weight: 128,
            higher_is_better: match metric_type {
                MetricType::ExecutionTime => false,
                MetricType::MemoryUsage => false,
                MetricType::CodeSize => false,
                MetricType::TestPassRate => true,
                MetricType::EnergyConsumption => false,
                MetricType::CacheHitRate => true,
                MetricType::BranchPredictionRate => true,
                MetricType::MemoryBandwidth => true,
            },
        }
    }

    /// Normalize metric to 0-1 scale
    pub fn normalize(&self, baseline: f32) -> f32 {
        if baseline == 0.0 {
            return 0.5;
        }

        let ratio = self.value / baseline;
        let normalized = if self.higher_is_better {
            ratio.min(2.0) / 2.0 // Cap improvement at 2x
        } else {
            1.0 / ratio.max(0.5).min(2.0) // Invert for "lower is better"
        };

        normalized.max(0.0).min(1.0)
    }
}

// ============================================================================
// FITNESS SCORE
// ============================================================================

/// Composite fitness score from multiple metrics
#[derive(Clone, Copy, Debug)]
pub struct FitnessScore {
    /// Overall score (0-255)
    pub overall: u8,
    /// Performance score component
    pub performance: u8,
    /// Memory score component
    pub memory: u8,
    /// Correctness score component
    pub correctness: u8,
    /// Energy score component
    pub energy: u8,
    /// Number of metrics included
    pub metric_count: u8,
    /// Whether this mutation is acceptable
    pub acceptable: bool,
}

impl FitnessScore {
    /// Create a new fitness score
    pub fn new() -> Self {
        Self {
            overall: 0,
            performance: 0,
            memory: 0,
            correctness: 0,
            energy: 0,
            metric_count: 0,
            acceptable: false,
        }
    }

    /// Calculate composite score from weighted metrics
    pub fn calculate(metrics: &[FitnessMetric]) -> Self {
        let mut score = Self::new();

        let mut total_weight = 0u32;
        let mut weighted_sum = 0u32;

        for metric in metrics {
            let normalized = metric.normalize(100.0); // Assuming baseline of 100.0
            let weighted = (normalized * metric.weight as f32) as u32;

            weighted_sum = weighted_sum.saturating_add(weighted);
            total_weight = total_weight.saturating_add(metric.weight as u32);

            // Update component scores
            match metric.metric_type {
                MetricType::ExecutionTime => {
                    score.performance = score.performance.saturating_add((normalized * 255.0) as u8);
                }
                MetricType::MemoryUsage => {
                    score.memory = score.memory.saturating_add((normalized * 255.0) as u8);
                }
                MetricType::TestPassRate => {
                    score.correctness = score.correctness.saturating_add((normalized * 255.0) as u8);
                }
                MetricType::EnergyConsumption => {
                    score.energy = score.energy.saturating_add((normalized * 255.0) as u8);
                }
                _ => {}
            }

            score.metric_count = score.metric_count.saturating_add(1);
        }

        // Calculate overall score
        if total_weight > 0 {
            score.overall = ((weighted_sum / total_weight) * 255u32 / 100u32) as u8;
        }

        // Mutation is acceptable if:
        // - Correctness is 100% (test pass rate)
        // - Performance doesn't regress significantly
        // - Memory doesn't increase significantly
        score.acceptable = score.correctness == 255 &&
                          score.performance >= 200 &&
                          score.memory >= 200;

        score
    }

    /// Is this score better than another?
    pub fn better_than(&self, other: &FitnessScore) -> bool {
        self.overall > other.overall
    }
}

impl Default for FitnessScore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SANDBOX
// ============================================================================

/// Isolated execution environment for mutation testing
pub struct Sandbox {
    /// Sandbox ID
    pub id: u64,
    /// Status
    pub status: SandboxStatus,
    /// Test suite
    pub test_suite: TestSuite,
    /// Benchmark suite
    pub benchmark_suite: BenchmarkSuite,
    /// Current fitness score
    pub fitness_score: FitnessScore,
    /// Memory used (bytes)
    pub memory_used: u64,
    /// Execution time (microseconds)
    pub execution_time_us: u64,
    /// Energy consumed (millijoules)
    pub energy_consumed_mj: u32,
}

/// Sandbox execution status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SandboxStatus {
    /// Created, ready for testing
    Ready = 0,
    /// Currently executing tests
    Running = 1,
    /// Tests completed, awaiting analysis
    Complete = 2,
    /// Analysis complete
    Analyzed = 3,
    /// Crash detected
    Crashed = 4,
    /// Timeout occurred
    Timeout = 5,
}

impl Sandbox {
    /// Create a new sandbox
    pub fn new(id: u64) -> Self {
        Self {
            id,
            status: SandboxStatus::Ready,
            test_suite: TestSuite::new(1),
            benchmark_suite: BenchmarkSuite::new(1),
            fitness_score: FitnessScore::new(),
            memory_used: 0,
            execution_time_us: 0,
            energy_consumed_mj: 0,
        }
    }

    /// Mark sandbox as running
    pub fn start_execution(&mut self) {
        self.status = SandboxStatus::Running;
    }

    /// Mark sandbox as complete
    pub fn finish_execution(&mut self) {
        self.status = SandboxStatus::Complete;
    }

    /// Mark sandbox as analyzed
    pub fn finish_analysis(&mut self) {
        self.status = SandboxStatus::Analyzed;
    }

    /// Record crash
    pub fn record_crash(&mut self) {
        self.status = SandboxStatus::Crashed;
    }

    /// Record timeout
    pub fn record_timeout(&mut self) {
        self.status = SandboxStatus::Timeout;
    }

    /// Is sandbox ready?
    pub fn is_ready(&self) -> bool {
        self.status == SandboxStatus::Ready
    }

    /// Did sandbox complete successfully?
    pub fn succeeded(&self) -> bool {
        self.status == SandboxStatus::Analyzed && self.fitness_score.acceptable
    }
}

// ============================================================================
// TOURNAMENT SELECTOR
// ============================================================================

/// Compares mutations in a tournament to select winners
pub struct TournamentSelector {
    /// Tournament ID
    pub id: u64,
    /// Baseline fitness score
    pub baseline_score: FitnessScore,
    /// Best mutation score found
    pub best_score: FitnessScore,
    /// Number of mutations evaluated
    pub mutations_evaluated: u32,
    /// Number of mutations beaten baseline
    pub mutations_beat_baseline: u32,
    /// Number of mutations accepted
    pub mutations_accepted: u32,
}

impl TournamentSelector {
    /// Create a new tournament selector
    pub fn new(id: u64) -> Self {
        Self {
            id,
            baseline_score: FitnessScore::new(),
            best_score: FitnessScore::new(),
            mutations_evaluated: 0,
            mutations_beat_baseline: 0,
            mutations_accepted: 0,
        }
    }

    /// Set baseline score
    pub fn set_baseline(&mut self, score: FitnessScore) {
        self.baseline_score = score;
        self.best_score = score;
    }

    /// Evaluate a mutation
    pub fn evaluate_mutation(&mut self, score: FitnessScore) -> SelectionResult {
        self.mutations_evaluated = self.mutations_evaluated.saturating_add(1);

        let mut result = SelectionResult::Rejected;

        // Check if acceptable
        if !score.acceptable {
            return result;
        }

        // Check if better than best seen
        if score.better_than(&self.best_score) {
            self.best_score = score;
            self.mutations_beat_baseline = self.mutations_beat_baseline.saturating_add(1);
            result = SelectionResult::Selected;
        } else if score.overall >= self.baseline_score.overall {
            // At least as good as baseline
            result = SelectionResult::Acceptable;
        }

        if matches!(result, SelectionResult::Selected | SelectionResult::Acceptable) {
            self.mutations_accepted = self.mutations_accepted.saturating_add(1);
        }

        result
    }

    /// Get acceptance rate
    pub fn acceptance_rate(&self) -> f32 {
        if self.mutations_evaluated == 0 {
            return 0.0;
        }
        (self.mutations_accepted as f32 / self.mutations_evaluated as f32) * 100.0
    }

    /// Get win rate vs baseline
    pub fn win_rate_vs_baseline(&self) -> f32 {
        if self.mutations_evaluated == 0 {
            return 0.0;
        }
        (self.mutations_beat_baseline as f32 / self.mutations_evaluated as f32) * 100.0
    }
}

/// Result of evaluating a mutation in tournament
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SelectionResult {
    /// Mutation rejected (failed tests or unacceptable)
    Rejected = 0,
    /// Mutation acceptable (meets minimum requirements)
    Acceptable = 1,
    /// Mutation selected (beats baseline)
    Selected = 2,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_case_creation() {
        let tc = TestCase::new(1, TestCategory::Unit);
        assert_eq!(tc.id, 1);
        assert_eq!(tc.category, TestCategory::Unit);
        assert_eq!(tc.pass_count, 0);
    }

    #[test]
    fn test_test_case_set_name() {
        let mut tc = TestCase::new(1, TestCategory::Unit);
        tc.set_name(b"test_arithmetic").unwrap();
        assert_eq!(tc.name_len, 15);
    }

    #[test]
    fn test_test_case_pass_rate() {
        let mut tc = TestCase::new(1, TestCategory::Unit);
        tc.record_pass();
        tc.record_pass();
        tc.record_fail();
        assert!((tc.pass_rate() - 66.66).abs() < 1.0);
    }

    #[test]
    fn test_suite_creation() {
        let ts = TestSuite::new(1);
        assert_eq!(ts.id, 1);
        assert_eq!(ts.test_count, 0);
        assert_eq!(ts.pass_rate(), 0.0);
    }

    #[test]
    fn test_suite_health() {
        let mut ts = TestSuite::new(1);
        ts.record_results(10, 0);
        assert!(ts.is_healthy());

        ts.record_results(0, 1);
        assert!(!ts.is_healthy());
    }

    #[test]
    fn test_benchmark_creation() {
        let bm = Benchmark::new(1);
        assert_eq!(bm.id, 1);
        assert_eq!(bm.baseline_us, 0);
        assert_eq!(bm.run_count, 0);
    }

    #[test]
    fn test_benchmark_measurements() {
        let mut bm = Benchmark::new(1);
        bm.baseline_us = 1000;
        bm.record_measurement(800);
        bm.record_measurement(900);

        assert_eq!(bm.last_measurement_us, 900);
        assert_eq!(bm.best_us, 800);
        assert_eq!(bm.worst_us, 900);
        assert_eq!(bm.run_count, 2);
    }

    #[test]
    fn test_benchmark_speedup() {
        let mut bm = Benchmark::new(1);
        bm.baseline_us = 1000;
        bm.record_measurement(500);
        assert!((bm.speedup_vs_baseline() - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_benchmark_suite() {
        let mut bs = BenchmarkSuite::new(1);
        bs.set_name(b"core_benchmarks").unwrap();
        bs.add_benchmark().unwrap();
        bs.add_benchmark().unwrap();
        bs.add_time(1000);
        bs.add_time(2000);

        assert_eq!(bs.benchmark_count, 2);
        assert_eq!(bs.total_time_us, 3000);
        assert_eq!(bs.avg_time_per_benchmark_us(), 1500);
    }

    #[test]
    fn test_fitness_metric_creation() {
        let m = FitnessMetric::new(1, MetricType::ExecutionTime);
        assert_eq!(m.metric_type, MetricType::ExecutionTime);
        assert!(!m.higher_is_better);
    }

    #[test]
    fn test_fitness_metric_normalization() {
        let mut m = FitnessMetric::new(1, MetricType::ExecutionTime);
        m.value = 50.0;
        let normalized = m.normalize(100.0);
        assert!(normalized > 0.5); // Better than baseline (50% faster)
    }

    #[test]
    fn test_fitness_score_creation() {
        let score = FitnessScore::new();
        assert_eq!(score.overall, 0);
        assert!(!score.acceptable);
    }

    #[test]
    fn test_fitness_score_comparison() {
        let mut score1 = FitnessScore::new();
        score1.overall = 200;

        let mut score2 = FitnessScore::new();
        score2.overall = 150;

        assert!(score1.better_than(&score2));
    }

    #[test]
    fn test_sandbox_creation() {
        let sandbox = Sandbox::new(1);
        assert_eq!(sandbox.id, 1);
        assert_eq!(sandbox.status, SandboxStatus::Ready);
        assert!(sandbox.is_ready());
    }

    #[test]
    fn test_sandbox_status_transitions() {
        let mut sandbox = Sandbox::new(1);
        assert_eq!(sandbox.status, SandboxStatus::Ready);

        sandbox.start_execution();
        assert_eq!(sandbox.status, SandboxStatus::Running);

        sandbox.finish_execution();
        assert_eq!(sandbox.status, SandboxStatus::Complete);

        sandbox.finish_analysis();
        assert_eq!(sandbox.status, SandboxStatus::Analyzed);
    }

    #[test]
    fn test_sandbox_crash_recording() {
        let mut sandbox = Sandbox::new(1);
        sandbox.record_crash();
        assert_eq!(sandbox.status, SandboxStatus::Crashed);
        assert!(!sandbox.succeeded());
    }

    #[test]
    fn test_tournament_selector_creation() {
        let selector = TournamentSelector::new(1);
        assert_eq!(selector.id, 1);
        assert_eq!(selector.mutations_evaluated, 0);
    }

    #[test]
    fn test_tournament_baseline() {
        let mut selector = TournamentSelector::new(1);
        let mut baseline = FitnessScore::new();
        baseline.overall = 200;

        selector.set_baseline(baseline);
        assert_eq!(selector.baseline_score.overall, 200);
    }

    #[test]
    fn test_tournament_evaluation() {
        let mut selector = TournamentSelector::new(1);
        let mut baseline = FitnessScore::new();
        baseline.overall = 200;
        baseline.acceptable = true;

        selector.set_baseline(baseline);

        let mut mutation = FitnessScore::new();
        mutation.overall = 220;
        mutation.acceptable = true;

        let result = selector.evaluate_mutation(mutation);
        assert_eq!(result, SelectionResult::Selected);
        assert_eq!(selector.mutations_evaluated, 1);
        assert_eq!(selector.mutations_beat_baseline, 1);
    }

    #[test]
    fn test_tournament_acceptance_rate() {
        let mut selector = TournamentSelector::new(1);
        let mut baseline = FitnessScore::new();
        baseline.acceptable = true;
        selector.set_baseline(baseline);

        let mut mutation = FitnessScore::new();
        mutation.acceptable = true;
        mutation.overall = 150;

        selector.evaluate_mutation(mutation);
        selector.evaluate_mutation(mutation);

        let mut bad = FitnessScore::new();
        bad.acceptable = false;
        selector.evaluate_mutation(bad);

        assert_eq!(selector.mutations_evaluated, 3);
        assert_eq!(selector.mutations_accepted, 2);
        assert!((selector.acceptance_rate() - 66.66).abs() < 1.0);
    }
}
