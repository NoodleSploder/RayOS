//! Regression Detection for Ouroboros Engine
//!
//! Detects and prevents performance regressions from mutations.
//! Maintains baselines, applies statistical significance tests, and triggers rollbacks.
//!
//! Phase 32, Task 5


/// Performance baseline for regression detection
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PerformanceBaseline {
    /// Baseline throughput (ops/sec)
    pub throughput: u32,
    /// Baseline latency (us)
    pub latency: u32,
    /// Baseline memory (KB)
    pub memory: u32,
    /// Standard deviation (scaled by 100)
    pub std_dev: u32,
}

impl PerformanceBaseline {
    /// Create new baseline
    pub const fn new(throughput: u32, latency: u32, memory: u32) -> Self {
        PerformanceBaseline {
            throughput,
            latency,
            memory,
            std_dev: 50, // default 0.5 std dev (scaled by 100)
        }
    }

    /// Update baseline with new measurements
    pub fn update(&mut self, throughput: u32, latency: u32, memory: u32) {
        // Exponential moving average (alpha = 0.3)
        self.throughput = ((self.throughput as u64 * 70 + throughput as u64 * 30) / 100) as u32;
        self.latency = ((self.latency as u64 * 70 + latency as u64 * 30) / 100) as u32;
        self.memory = ((self.memory as u64 * 70 + memory as u64 * 30) / 100) as u32;
    }
}

/// Regression detection result
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegressionResult {
    /// Whether regression detected
    pub detected: bool,
    /// Regression severity percent (scaled by 100)
    pub severity: u32,
    /// Z-score of deviation (scaled by 100)
    pub z_score: u32,
    /// Statistical significance (p-value scaled by 1000)
    pub p_value: u32,
}

impl RegressionResult {
    /// Create result indicating no regression
    pub const fn no_regression() -> Self {
        RegressionResult {
            detected: false,
            severity: 0,
            z_score: 0,
            p_value: 1000, // p = 1.0 (no significance)
        }
    }

    /// Create result indicating regression detected
    pub fn detected(severity: u32, z_score: u32, p_value: u32) -> Self {
        RegressionResult {
            detected: true,
            severity,
            z_score,
            p_value,
        }
    }

    /// Check if statistically significant (p < 0.05)
    pub fn is_significant(&self) -> bool {
        self.p_value < 50 // p_value scaled by 1000, so 50 = 0.05
    }
}

/// Regression detector with statistical analysis
pub struct RegressionDetector {
    /// Performance baseline
    baseline: PerformanceBaseline,
    /// Measurement history ring buffer
    history: [u32; 100],
    /// Write position
    write_pos: usize,
    /// Entry count
    entry_count: usize,
    /// Regression threshold percent
    threshold_percent: u32,
}

impl RegressionDetector {
    /// Create new detector
    pub const fn new(baseline: PerformanceBaseline) -> Self {
        RegressionDetector {
            baseline,
            history: [0u32; 100],
            write_pos: 0,
            entry_count: 0,
            threshold_percent: 200, // 2% default threshold (scaled by 100)
        }
    }

    /// Set regression threshold
    pub fn set_threshold(&mut self, percent: u32) {
        self.threshold_percent = percent;
    }

    /// Detect regression in throughput measurement
    pub fn detect_regression(&mut self, throughput: u32) -> RegressionResult {
        // Record measurement
        self.history[self.write_pos] = throughput;
        self.write_pos = (self.write_pos + 1) % 100;
        if self.entry_count < 100 {
            self.entry_count += 1;
        }

        // Calculate percent change from baseline
        if self.baseline.throughput == 0 {
            return RegressionResult::no_regression();
        }

        let percent_change = if throughput < self.baseline.throughput {
            let diff = self.baseline.throughput - throughput;
            ((diff as u64 * 10000) / self.baseline.throughput as u64) as u32
        } else {
            0 // improvement, not regression
        };

        // Check against threshold
        if percent_change > self.threshold_percent {
            // Calculate z-score
            let z_score = (percent_change as u64 * 100 / self.baseline.std_dev.max(1) as u64)
                as u32;

            // Estimate p-value (simplified)
            let p_value = Self::z_score_to_p_value(z_score);

            return RegressionResult::detected(percent_change, z_score, p_value);
        }

        RegressionResult::no_regression()
    }

    /// Convert z-score to p-value (simplified)
    fn z_score_to_p_value(z_score: u32) -> u32 {
        // Z-score scaled by 100, p-value scaled by 1000
        // Simplified conversion
        if z_score > 30000 {
            // z > 3.0 => p < 0.0027
            return 3;
        } else if z_score > 20000 {
            // z > 2.0 => p < 0.0455
            return 45;
        } else if z_score > 10000 {
            // z > 1.0 => p < 0.3173
            return 317;
        } else {
            return 1000; // p >= 1.0
        }
    }

    /// Detect trend regression (gradual degradation)
    pub fn detect_trend_regression(&self) -> bool {
        if self.entry_count < 10 {
            return false; // need enough history
        }

        // Check last 10 measurements vs baseline
        let mut degraded_count = 0;
        for i in 0..10 {
            let pos = (self.write_pos + 100 - 10 + i) % 100;
            if self.history[pos] < self.baseline.throughput {
                degraded_count += 1;
            }
        }

        // If 7+ of last 10 are below baseline, it's a trend
        degraded_count >= 7
    }

    /// Get average from history
    pub fn avg_from_history(&self) -> u32 {
        if self.entry_count == 0 {
            return 0;
        }
        let mut sum = 0u64;
        for i in 0..self.entry_count {
            sum += self.history[i] as u64;
        }
        (sum / self.entry_count as u64) as u32
    }

    /// Update baseline with current data
    pub fn update_baseline(&mut self) {
        if self.entry_count > 0 {
            let avg = self.avg_from_history();
            self.baseline.update(avg, self.baseline.latency, self.baseline.memory);
        }
    }

    /// Clear history
    pub fn reset_history(&mut self) {
        self.history = [0u32; 100];
        self.write_pos = 0;
        self.entry_count = 0;
    }
}

/// Adaptive threshold that adjusts based on system state
pub struct AdaptiveThreshold {
    /// Base threshold percent (scaled by 100)
    base_threshold: u32,
    /// Current multiplier (scaled by 100)
    multiplier: u32,
    /// System load level (0-100)
    load_level: u32,
    /// Recent variation in measurements
    recent_variation: u32,
}

impl AdaptiveThreshold {
    /// Create new adaptive threshold
    pub const fn new(base_threshold: u32) -> Self {
        AdaptiveThreshold {
            base_threshold,
            multiplier: 100,
            load_level: 50,
            recent_variation: 20,
        }
    }

    /// Update system load
    pub fn set_load_level(&mut self, load: u32) {
        self.load_level = load.min(100);
        self.adapt_threshold();
    }

    /// Update measurement variation
    pub fn set_variation(&mut self, variation: u32) {
        self.recent_variation = variation.min(100);
        self.adapt_threshold();
    }

    /// Adapt threshold based on system state
    fn adapt_threshold(&mut self) {
        // Higher load => more lenient threshold
        let load_factor = 100 + (self.load_level / 2); // 100-150

        // Higher variation => more lenient threshold
        let variation_factor = 100 + (self.recent_variation / 5); // 100-120

        self.multiplier = ((load_factor as u64 * variation_factor as u64) / 100) as u32;
    }

    /// Get effective threshold
    pub fn effective_threshold(&self) -> u32 {
        ((self.base_threshold as u64 * self.multiplier as u64) / 100) as u32
    }

    /// Check if measurement is within threshold
    pub fn within_threshold(&self, baseline: u32, actual: u32) -> bool {
        if baseline == 0 {
            return true;
        }
        let _percent_change =
            ((actual as u64 * 10000) / baseline as u64) as u32;
        let threshold = self.effective_threshold();

        // If actual >= baseline, always OK
        if actual >= baseline {
            return true;
        }

        // If degraded, check against threshold
        let degradation = baseline.saturating_sub(actual);
        let degradation_percent =
            ((degradation as u64 * 10000) / baseline as u64) as u32;

        degradation_percent <= threshold
    }

    /// Get current sensitivity (1-100)
    pub fn sensitivity(&self) -> u32 {
        // Higher multiplier = more tolerance = lower sensitivity
        (100u32).saturating_sub(self.multiplier.saturating_sub(100))
    }
}

/// Rollback decision maker
pub struct RollbackDecision {
    /// Regression detected
    regression: RegressionResult,
    /// Rollback triggered
    pub triggered: bool,
    /// Reason code
    pub reason: RegressionRollbackReason,
}

impl RollbackDecision {
    /// Create decision from regression result
    pub fn from_regression(regression: RegressionResult) -> Self {
        let triggered = regression.detected && regression.is_significant();
        let reason = if regression.detected {
            RegressionRollbackReason::RegressionDetected
        } else {
            RegressionRollbackReason::NoRegression
        };

        RollbackDecision {
            regression,
            triggered,
            reason,
        }
    }

    /// Create decision from trend regression
    pub fn from_trend(trending: bool) -> Self {
        RollbackDecision {
            regression: RegressionResult::no_regression(),
            triggered: trending,
            reason: if trending {
                RegressionRollbackReason::TrendRegression
            } else {
                RegressionRollbackReason::NoRegression
            },
        }
    }

    /// Get confidence in decision (percent)
    pub fn confidence(&self) -> u32 {
        if self.triggered {
            self.regression.z_score.min(10000) / 100 // z-score as confidence
        } else {
            100 // high confidence in no rollback decision
        }
    }
}

/// Regression-triggered rollback reason
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RegressionRollbackReason {
    NoRegression = 0,
    RegressionDetected = 1,
    TrendRegression = 2,
    ManualOverride = 3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_baseline_creation() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        assert_eq!(baseline.throughput, 1000);
        assert_eq!(baseline.latency, 100);
        assert_eq!(baseline.memory, 512);
    }

    #[test]
    fn test_performance_baseline_update() {
        let mut baseline = PerformanceBaseline::new(1000, 100, 512);
        baseline.update(1100, 110, 550);

        // EMA: 1000 * 0.7 + 1100 * 0.3 = 1030
        assert!(baseline.throughput > 1000 && baseline.throughput < 1100);
    }

    #[test]
    fn test_regression_result_no_regression() {
        let result = RegressionResult::no_regression();
        assert!(!result.detected);
        assert_eq!(result.severity, 0);
    }

    #[test]
    fn test_regression_result_detected() {
        let result = RegressionResult::detected(150, 250, 30);
        assert!(result.detected);
        assert_eq!(result.severity, 150);
        assert!(result.is_significant());
    }

    #[test]
    fn test_regression_result_significance() {
        let result1 = RegressionResult::detected(150, 250, 30);
        assert!(result1.is_significant()); // p = 0.03

        let result2 = RegressionResult::detected(50, 100, 100);
        assert!(!result2.is_significant()); // p = 0.1
    }

    #[test]
    fn test_regression_detector_creation() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        let detector = RegressionDetector::new(baseline);
        assert_eq!(detector.entry_count, 0);
    }

    #[test]
    fn test_regression_detector_no_regression() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        let mut detector = RegressionDetector::new(baseline);

        // Small improvement (no regression)
        let result = detector.detect_regression(1050);
        assert!(!result.detected);
    }

    #[test]
    fn test_regression_detector_regression() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        let mut detector = RegressionDetector::new(baseline);

        // 5% degradation (detected if threshold is 2%)
        let result = detector.detect_regression(950);
        assert!(result.detected);
        assert!(result.severity > 200);
    }

    #[test]
    fn test_regression_detector_threshold() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        let mut detector = RegressionDetector::new(baseline);
        detector.set_threshold(100); // 1% threshold

        // 0.5% degradation should not trigger
        let result = detector.detect_regression(995);
        assert!(!result.detected);
    }

    #[test]
    fn test_regression_detector_trend() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        let mut detector = RegressionDetector::new(baseline);

        // Add measurements showing degradation trend
        for _ in 0..8 {
            detector.detect_regression(950);
        }

        let trending = detector.detect_trend_regression();
        assert!(trending);
    }

    #[test]
    fn test_regression_detector_avg_history() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        let mut detector = RegressionDetector::new(baseline);

        detector.detect_regression(1000);
        detector.detect_regression(1100);
        detector.detect_regression(900);

        let avg = detector.avg_from_history();
        assert_eq!(avg, 1000);
    }

    #[test]
    fn test_regression_detector_update_baseline() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        let mut detector = RegressionDetector::new(baseline);

        detector.detect_regression(1050);
        detector.detect_regression(1100);
        detector.detect_regression(1000);

        let old_baseline = detector.baseline.throughput;
        detector.update_baseline();
        let new_baseline = detector.baseline.throughput;

        assert!(new_baseline > old_baseline);
    }

    #[test]
    fn test_adaptive_threshold_creation() {
        let threshold = AdaptiveThreshold::new(200);
        assert_eq!(threshold.base_threshold, 200);
        assert_eq!(threshold.multiplier, 100);
    }

    #[test]
    fn test_adaptive_threshold_load() {
        let mut threshold = AdaptiveThreshold::new(200);
        threshold.set_load_level(80);

        // Higher load should increase multiplier
        assert!(threshold.multiplier > 100);
    }

    #[test]
    fn test_adaptive_threshold_variation() {
        let mut threshold = AdaptiveThreshold::new(200);
        threshold.set_variation(80);

        // Higher variation should increase multiplier
        assert!(threshold.multiplier > 100);
    }

    #[test]
    fn test_adaptive_threshold_effective() {
        let mut threshold = AdaptiveThreshold::new(200);
        let effective = threshold.effective_threshold();
        assert_eq!(effective, 200);

        threshold.set_load_level(100);
        let effective2 = threshold.effective_threshold();
        assert!(effective2 > effective);
    }

    #[test]
    fn test_adaptive_threshold_within_threshold() {
        let mut threshold = AdaptiveThreshold::new(200); // 2%
        assert!(threshold.within_threshold(1000, 990)); // 1% OK
        assert!(!threshold.within_threshold(1000, 970)); // 3% not OK
    }

    #[test]
    fn test_rollback_decision_no_regression() {
        let regression = RegressionResult::no_regression();
        let decision = RollbackDecision::from_regression(regression);
        assert!(!decision.triggered);
    }

    #[test]
    fn test_rollback_decision_regression() {
        let regression = RegressionResult::detected(150, 300, 30);
        let decision = RollbackDecision::from_regression(regression);
        assert!(decision.triggered);
    }

    #[test]
    fn test_rollback_decision_confidence() {
        let regression = RegressionResult::detected(150, 300, 30);
        let decision = RollbackDecision::from_regression(regression);
        let confidence = decision.confidence();
        assert!(confidence > 0);
    }

    #[test]
    fn test_rollback_decision_trend() {
        let decision = RollbackDecision::from_trend(true);
        assert!(decision.triggered);
        assert_eq!(decision.reason, RegressionRollbackReason::TrendRegression);
    }

    #[test]
    fn test_z_score_conversion() {
        let p1 = RegressionDetector::z_score_to_p_value(30000);
        assert!(p1 < 10); // z > 3.0

        let p2 = RegressionDetector::z_score_to_p_value(20000);
        assert!(p2 < 50); // z > 2.0

        let p3 = RegressionDetector::z_score_to_p_value(5000);
        assert!(p3 < 1000); // z > 0.5
    }

    #[test]
    fn test_regression_detection_integration() {
        let baseline = PerformanceBaseline::new(1000, 100, 512);
        let mut detector = RegressionDetector::new(baseline);
        let mut threshold = AdaptiveThreshold::new(200);

        // Simulate measurements
        let measurements = [1000, 1050, 980, 975, 990, 960, 950, 940];

        for measurement in &measurements {
            let result = detector.detect_regression(*measurement);
            let within = threshold.within_threshold(1000, *measurement);

            if result.detected {
                let decision = RollbackDecision::from_regression(result);
                assert!(decision.triggered || !within);
            }
        }
    }
}
