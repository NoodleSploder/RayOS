//! Mutation Impact Analysis: Before/After Measurement and Effectiveness
//!
//! Comprehensive analysis of mutation effectiveness by measuring before/after
//! performance, calculating variance and statistical significance, and computing
//! impact scores for mutation evaluation.
//!
//! Phase 35, Task 3

/// Impact measurement type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ImpactMetric {
    LatencyChange = 0,      // Latency improvement (negative is good)
    ThroughputGain = 1,     // Operations per second improvement
    ResourceSavings = 2,    // Memory/CPU reduction
    EfficiencyGain = 3,     // Efficiency score improvement
    Overall = 4,            // Combined impact score
}

/// Statistical result from analysis
#[derive(Clone, Copy, Debug)]
pub struct StatisticalResult {
    /// Result ID
    pub id: u32,
    /// Mean value
    pub mean: i32,
    /// Variance (sum of squared differences)
    pub variance: u32,
    /// Standard deviation (estimated)
    pub std_dev: u16,
    /// Sample count
    pub samples: u16,
    /// Is significant at 95% confidence
    pub is_significant: bool,
}

impl StatisticalResult {
    /// Create new statistical result
    pub const fn new(id: u32, mean: i32, variance: u32) -> Self {
        // Estimate std dev from variance
        let std_dev = if variance == 0 {
            0
        } else {
            (variance / 100) as u16  // Simplified sqrt estimate
        };

        StatisticalResult {
            id,
            mean,
            variance,
            std_dev,
            samples: 0,
            is_significant: variance > 100,  // Simple significance heuristic
        }
    }

    /// Set sample count
    pub fn with_samples(mut self, count: u16) -> Self {
        self.samples = count;
        self
    }

    /// Check if improvement is significant
    pub fn significant_improvement(&self, threshold: u16) -> bool {
        self.is_significant && self.std_dev > 0 && self.mean.abs() as u16 >= threshold
    }
}

/// Baseline measurement for mutation
#[derive(Clone, Copy, Debug)]
pub struct MutationBaseline {
    /// Baseline ID
    pub id: u32,
    /// Mutation identifier
    pub mutation_id: u32,
    /// Baseline latency (ms)
    pub baseline_latency: u32,
    /// Baseline throughput (ops/sec)
    pub baseline_throughput: u32,
    /// Baseline memory (MB)
    pub baseline_memory: u16,
    /// Baseline efficiency score
    pub baseline_efficiency: u8,
}

impl MutationBaseline {
    /// Create new mutation baseline
    pub const fn new(id: u32, mutation_id: u32) -> Self {
        MutationBaseline {
            id,
            mutation_id,
            baseline_latency: 0,
            baseline_throughput: 0,
            baseline_memory: 0,
            baseline_efficiency: 0,
        }
    }
}

/// Measured mutation performance
#[derive(Clone, Copy, Debug)]
pub struct MutationMeasurement {
    /// Measurement ID
    pub id: u32,
    /// Baseline ID
    pub baseline_id: u32,
    /// Measured latency (ms)
    pub measured_latency: u32,
    /// Measured throughput (ops/sec)
    pub measured_throughput: u32,
    /// Measured memory (MB)
    pub measured_memory: u16,
    /// Measured efficiency score
    pub measured_efficiency: u8,
}

impl MutationMeasurement {
    /// Create new mutation measurement
    pub const fn new(id: u32, baseline_id: u32) -> Self {
        MutationMeasurement {
            id,
            baseline_id,
            measured_latency: 0,
            measured_throughput: 0,
            measured_memory: 0,
            measured_efficiency: 0,
        }
    }

    /// Calculate latency change (negative is improvement)
    pub fn latency_change(&self, baseline_latency: u32) -> i32 {
        (self.measured_latency as i32) - (baseline_latency as i32)
    }

    /// Calculate throughput gain (positive is improvement)
    pub fn throughput_gain(&self, baseline_throughput: u32) -> i32 {
        (self.measured_throughput as i32) - (baseline_throughput as i32)
    }

    /// Calculate memory savings (positive is improvement)
    pub fn memory_savings(&self, baseline_memory: u16) -> i32 {
        (baseline_memory as i32) - (self.measured_memory as i32)
    }

    /// Calculate efficiency gain (positive is improvement)
    pub fn efficiency_gain(&self, baseline_efficiency: u8) -> i32 {
        (self.measured_efficiency as i32) - (baseline_efficiency as i32)
    }
}

/// Impact analysis result
#[derive(Clone, Copy, Debug)]
pub struct MutationImpact {
    /// Impact ID
    pub id: u32,
    /// Measurement ID
    pub measurement_id: u32,
    /// Latency impact percent (-100 to +100)
    pub latency_impact_percent: i8,
    /// Throughput impact percent
    pub throughput_impact_percent: i8,
    /// Memory impact percent (negative is good)
    pub memory_impact_percent: i8,
    /// Efficiency impact percent
    pub efficiency_impact_percent: i8,
    /// Overall impact score (0-100)
    pub overall_score: u8,
    /// Is overall improvement
    pub is_improved: bool,
}

impl MutationImpact {
    /// Create new mutation impact
    pub const fn new(id: u32, measurement_id: u32) -> Self {
        MutationImpact {
            id,
            measurement_id,
            latency_impact_percent: 0,
            throughput_impact_percent: 0,
            memory_impact_percent: 0,
            efficiency_impact_percent: 0,
            overall_score: 50,
            is_improved: false,
        }
    }

    /// Calculate impact percentages
    pub fn calculate_from_baseline(
        id: u32,
        measurement_id: u32,
        meas: &MutationMeasurement,
        baseline: &MutationBaseline,
    ) -> Self {
        let latency_change = meas.latency_change(baseline.baseline_latency);
        let throughput_change = meas.throughput_gain(baseline.baseline_throughput);
        let memory_change = meas.memory_savings(baseline.baseline_memory);
        let efficiency_change = meas.efficiency_gain(baseline.baseline_efficiency);

        // Calculate percentages with clamping
        let latency_pct = if baseline.baseline_latency == 0 {
            0
        } else {
            let pct = (latency_change * 100) / baseline.baseline_latency as i32;
            if pct > 100 { 100 } else if pct < -100 { -100 } else { pct }
        } as i8;

        let throughput_pct = if baseline.baseline_throughput == 0 {
            0
        } else {
            let pct = (throughput_change * 100) / baseline.baseline_throughput as i32;
            if pct > 100 { 100 } else if pct < -100 { -100 } else { pct }
        } as i8;

        let memory_pct = if baseline.baseline_memory == 0 {
            0
        } else {
            let pct = (memory_change * 100) / baseline.baseline_memory as i32;
            if pct > 100 { 100 } else if pct < -100 { -100 } else { pct }
        } as i8;

        let efficiency_pct = if baseline.baseline_efficiency == 0 {
            0
        } else {
            let pct = (efficiency_change * 100) / baseline.baseline_efficiency as i32;
            if pct > 100 { 100 } else if pct < -100 { -100 } else { pct }
        } as i8;

        // Calculate overall score (favor latency and throughput improvements)
        let latency_score = if latency_pct < 0 { (-latency_pct) as u32 } else { 0 };
        let throughput_score = if throughput_pct > 0 { throughput_pct as u32 } else { 0 };
        let memory_score = if memory_pct > 0 { memory_pct as u32 } else { 0 };
        let efficiency_score = if efficiency_pct > 0 { efficiency_pct as u32 } else { 0 };

        let overall = ((latency_score * 2 + throughput_score + memory_score + efficiency_score) / 5)
            .min(100) as u8;

        let is_improved = latency_pct < 0 || throughput_pct > 0 || memory_pct > 0 || efficiency_pct > 0;

        MutationImpact {
            id,
            measurement_id,
            latency_impact_percent: latency_pct,
            throughput_impact_percent: throughput_pct,
            memory_impact_percent: memory_pct,
            efficiency_impact_percent: efficiency_pct,
            overall_score: overall,
            is_improved,
        }
    }
}

/// Variance accumulator for measurements
#[derive(Clone, Copy, Debug)]
pub struct VarianceAccumulator {
    /// Accumulator ID
    pub id: u32,
    /// Sum of values
    pub sum: u64,
    /// Sum of squared values
    pub sum_of_squares: u64,
    /// Sample count
    pub count: u32,
    /// Minimum value
    pub min: u32,
    /// Maximum value
    pub max: u32,
}

impl VarianceAccumulator {
    /// Create new variance accumulator
    pub const fn new(id: u32) -> Self {
        VarianceAccumulator {
            id,
            sum: 0,
            sum_of_squares: 0,
            count: 0,
            min: u32::MAX,
            max: 0,
        }
    }

    /// Add a sample
    pub fn add_sample(&mut self, value: u32) {
        self.sum += value as u64;
        self.sum_of_squares += (value as u64) * (value as u64);
        self.count += 1;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
    }

    /// Calculate mean
    pub fn mean(&self) -> u32 {
        if self.count == 0 {
            0
        } else {
            (self.sum / self.count as u64) as u32
        }
    }

    /// Calculate variance
    pub fn variance(&self) -> u32 {
        if self.count == 0 {
            return 0;
        }

        let mean_val = self.mean() as u64;
        let mean_sq = mean_val * mean_val;

        let avg_of_sq = self.sum_of_squares / self.count as u64;
        if avg_of_sq > mean_sq {
            (avg_of_sq - mean_sq) as u32
        } else {
            0
        }
    }

    /// Calculate standard deviation estimate
    pub fn std_dev(&self) -> u16 {
        let var = self.variance();
        if var == 0 {
            0
        } else {
            (var / 100) as u16  // Simplified sqrt estimate
        }
    }

    /// Get range
    pub fn range(&self) -> u32 {
        self.max.saturating_sub(self.min)
    }
}

/// Significance test result
#[derive(Clone, Copy, Debug)]
pub struct SignificanceTest {
    /// Test ID
    pub id: u32,
    /// Baseline mean
    pub baseline_mean: u32,
    /// Measured mean
    pub measured_mean: u32,
    /// Baseline std dev
    pub baseline_std_dev: u16,
    /// Measured std dev
    pub measured_std_dev: u16,
    /// T-statistic (simplified)
    pub t_statistic: u16,
    /// P-value estimate (0-100, >95 is significant)
    pub p_value_estimate: u8,
    /// Is significant
    pub is_significant: bool,
}

impl SignificanceTest {
    /// Create new significance test
    pub const fn new(id: u32, baseline_mean: u32, measured_mean: u32) -> Self {
        // Simplified t-statistic calculation
        let diff = if baseline_mean > measured_mean {
            baseline_mean - measured_mean
        } else {
            measured_mean - baseline_mean
        };

        // Clamp t-stat to 1000
        let t_stat_raw = diff / 10;
        let t_stat = if t_stat_raw > 1000 { 1000 } else { t_stat_raw } as u16;

        // Simple significance: if t-stat > 50, consider it significant
        let is_sig = t_stat > 50;
        let p_estimate = if is_sig { 98 } else { 30 };

        SignificanceTest {
            id,
            baseline_mean,
            measured_mean,
            baseline_std_dev: 0,
            measured_std_dev: 0,
            t_statistic: t_stat,
            p_value_estimate: p_estimate,
            is_significant: is_sig,
        }
    }

    /// Set standard deviations
    pub fn with_std_devs(mut self, baseline_std: u16, measured_std: u16) -> Self {
        self.baseline_std_dev = baseline_std;
        self.measured_std_dev = measured_std;
        self
    }
}

/// Impact Analysis System
pub struct ImpactAnalyzer {
    /// Baselines (max 50)
    baselines: [Option<MutationBaseline>; 50],
    /// Measurements (max 100)
    measurements: [Option<MutationMeasurement>; 100],
    /// Impact results (max 100)
    impacts: [Option<MutationImpact>; 100],
    /// Statistical results (max 50)
    statistical: [Option<StatisticalResult>; 50],
    /// Significance tests (max 50)
    significance_tests: [Option<SignificanceTest>; 50],
    /// Total analyses performed
    total_analyses: u32,
}

impl ImpactAnalyzer {
    /// Create new impact analyzer
    pub const fn new() -> Self {
        ImpactAnalyzer {
            baselines: [None; 50],
            measurements: [None; 100],
            impacts: [None; 100],
            statistical: [None; 50],
            significance_tests: [None; 50],
            total_analyses: 0,
        }
    }

    /// Store baseline
    pub fn store_baseline(&mut self, baseline: MutationBaseline) -> bool {
        for slot in &mut self.baselines {
            if slot.is_none() {
                *slot = Some(baseline);
                return true;
            }
        }
        false
    }

    /// Store measurement
    pub fn store_measurement(&mut self, measurement: MutationMeasurement) -> bool {
        for slot in &mut self.measurements {
            if slot.is_none() {
                *slot = Some(measurement);
                return true;
            }
        }
        false
    }

    /// Analyze mutation impact
    pub fn analyze_impact(
        &mut self,
        measurement: &MutationMeasurement,
        baseline: &MutationBaseline,
    ) -> bool {
        let impact = MutationImpact::calculate_from_baseline(
            self.total_analyses,
            measurement.id,
            measurement,
            baseline,
        );

        for slot in &mut self.impacts {
            if slot.is_none() {
                *slot = Some(impact);
                self.total_analyses += 1;
                return true;
            }
        }
        false
    }

    /// Store statistical result
    pub fn store_statistical(&mut self, result: StatisticalResult) -> bool {
        for slot in &mut self.statistical {
            if slot.is_none() {
                *slot = Some(result);
                return true;
            }
        }
        false
    }

    /// Store significance test
    pub fn store_significance_test(&mut self, test: SignificanceTest) -> bool {
        for slot in &mut self.significance_tests {
            if slot.is_none() {
                *slot = Some(test);
                return true;
            }
        }
        false
    }

    /// Get average impact score
    pub fn avg_impact_score(&self) -> u8 {
        let mut total = 0u32;
        let mut count = 0u32;

        for impact in &self.impacts {
            if let Some(imp) = impact {
                total += imp.overall_score as u32;
                count += 1;
            }
        }

        if count == 0 {
            50
        } else {
            (total / count) as u8
        }
    }

    /// Count improvements
    pub fn count_improvements(&self) -> usize {
        self.impacts
            .iter()
            .filter(|imp| imp.map(|i| i.is_improved).unwrap_or(false))
            .count()
    }

    /// Count regressions
    pub fn count_regressions(&self) -> usize {
        self.impacts
            .iter()
            .filter(|imp| imp.map(|i| !i.is_improved).unwrap_or(false))
            .count()
    }

    /// Get improvement rate percent
    pub fn improvement_rate(&self) -> u8 {
        let total = self.impacts.iter().filter(|imp| imp.is_some()).count();
        if total == 0 {
            0
        } else {
            let improvements = self.count_improvements();
            ((improvements * 100) / total) as u8
        }
    }

    /// Get best impact
    pub fn best_impact(&self) -> Option<u8> {
        self.impacts
            .iter()
            .filter_map(|imp| imp.map(|i| i.overall_score))
            .max()
    }

    /// Get worst impact
    pub fn worst_impact(&self) -> Option<u8> {
        self.impacts
            .iter()
            .filter_map(|imp| imp.map(|i| i.overall_score))
            .min()
    }

    /// Get significant results count
    pub fn significant_results_count(&self) -> usize {
        self.significance_tests
            .iter()
            .filter(|test| test.map(|t| t.is_significant).unwrap_or(false))
            .count()
    }

    /// Get total analyses
    pub fn total_analyses(&self) -> u32 {
        self.total_analyses
    }

    /// Get statistics
    pub fn statistics(&self) -> (u32, u8, usize, usize, u8) {
        (
            self.total_analyses,
            self.avg_impact_score(),
            self.count_improvements(),
            self.count_regressions(),
            self.improvement_rate(),
        )
    }

    /// Compare two measurement sets
    pub fn compare_measurements(
        &mut self,
        _baseline_meas: &MutationMeasurement,
        new_meas: &MutationMeasurement,
        baseline: &MutationBaseline,
    ) -> Option<u8> {
        // Analyze impact of new measurement vs original baseline
        let impact = MutationImpact::calculate_from_baseline(
            self.total_analyses,
            new_meas.id,
            new_meas,
            baseline,
        );

        for slot in &mut self.impacts {
            if slot.is_none() {
                *slot = Some(impact);
                self.total_analyses += 1;
                return Some(impact.overall_score);
            }
        }
        None
    }

    /// Get measurements for baseline
    pub fn measurements_for_baseline(&self, baseline_id: u32) -> usize {
        self.measurements
            .iter()
            .filter(|m| m.map(|meas| meas.baseline_id == baseline_id).unwrap_or(false))
            .count()
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.baselines = [None; 50];
        self.measurements = [None; 100];
        self.impacts = [None; 100];
        self.statistical = [None; 50];
        self.significance_tests = [None; 50];
        self.total_analyses = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_impact_metric_enum() {
        assert_eq!(ImpactMetric::LatencyChange as u8, 0);
        assert_eq!(ImpactMetric::Overall as u8, 4);
    }

    #[test]
    fn test_statistical_result_creation() {
        let result = StatisticalResult::new(1, 100, 400);
        assert_eq!(result.id, 1);
        assert_eq!(result.mean, 100);
    }

    #[test]
    fn test_mutation_baseline_creation() {
        let baseline = MutationBaseline::new(1, 1001);
        assert_eq!(baseline.mutation_id, 1001);
    }

    #[test]
    fn test_mutation_measurement_creation() {
        let meas = MutationMeasurement::new(1, 1);
        assert_eq!(meas.id, 1);
        assert_eq!(meas.baseline_id, 1);
    }

    #[test]
    fn test_mutation_measurement_latency_change() {
        let meas = MutationMeasurement::new(1, 1);
        let meas_with_data = MutationMeasurement {
            measured_latency: 40,
            ..meas
        };
        assert_eq!(meas_with_data.latency_change(50), -10);
    }

    #[test]
    fn test_mutation_measurement_throughput_gain() {
        let meas = MutationMeasurement::new(1, 1);
        let meas_with_data = MutationMeasurement {
            measured_throughput: 1200,
            ..meas
        };
        assert_eq!(meas_with_data.throughput_gain(1000), 200);
    }

    #[test]
    fn test_mutation_measurement_memory_savings() {
        let meas = MutationMeasurement::new(1, 1);
        let meas_with_data = MutationMeasurement {
            measured_memory: 200,
            ..meas
        };
        assert_eq!(meas_with_data.memory_savings(256), 56);
    }

    #[test]
    fn test_mutation_measurement_efficiency_gain() {
        let meas = MutationMeasurement::new(1, 1);
        let meas_with_data = MutationMeasurement {
            measured_efficiency: 85,
            ..meas
        };
        assert_eq!(meas_with_data.efficiency_gain(80), 5);
    }

    #[test]
    fn test_mutation_impact_creation() {
        let impact = MutationImpact::new(1, 1);
        assert_eq!(impact.id, 1);
        assert!(!impact.is_improved);
    }

    #[test]
    fn test_mutation_impact_calculate_from_baseline() {
        let mut baseline = MutationBaseline::new(1, 1001);
        baseline.baseline_latency = 100;
        baseline.baseline_throughput = 1000;
        baseline.baseline_memory = 256;
        baseline.baseline_efficiency = 80;

        let mut meas = MutationMeasurement::new(1, 1);
        meas.measured_latency = 80;      // 20% improvement
        meas.measured_throughput = 1200; // 20% improvement
        meas.measured_memory = 200;      // 21% savings
        meas.measured_efficiency = 90;   // 12.5% improvement

        let impact = MutationImpact::calculate_from_baseline(1, 1, &meas, &baseline);
        assert!(impact.is_improved);
        assert!(impact.overall_score > 50);
    }

    #[test]
    fn test_mutation_impact_regression() {
        let mut baseline = MutationBaseline::new(1, 1001);
        baseline.baseline_latency = 100;
        baseline.baseline_throughput = 1000;
        baseline.baseline_memory = 256;
        baseline.baseline_efficiency = 80;

        let mut meas = MutationMeasurement::new(1, 1);
        meas.measured_latency = 120;     // 20% worse
        meas.measured_throughput = 800;  // 20% worse
        meas.measured_memory = 300;      // 17% worse
        meas.measured_efficiency = 70;   // 12.5% worse

        let impact = MutationImpact::calculate_from_baseline(1, 1, &meas, &baseline);
        assert!(!impact.is_improved);
        assert!(impact.overall_score < 50);
    }

    #[test]
    fn test_variance_accumulator_creation() {
        let acc = VarianceAccumulator::new(1);
        assert_eq!(acc.count, 0);
        assert_eq!(acc.mean(), 0);
    }

    #[test]
    fn test_variance_accumulator_add_sample() {
        let mut acc = VarianceAccumulator::new(1);
        acc.add_sample(100);
        acc.add_sample(100);
        acc.add_sample(100);
        assert_eq!(acc.count, 3);
        assert_eq!(acc.mean(), 100);
        assert_eq!(acc.variance(), 0);
    }

    #[test]
    fn test_variance_accumulator_variance() {
        let mut acc = VarianceAccumulator::new(1);
        acc.add_sample(50);
        acc.add_sample(100);
        acc.add_sample(150);
        let var = acc.variance();
        assert!(var > 0);
    }

    #[test]
    fn test_variance_accumulator_range() {
        let mut acc = VarianceAccumulator::new(1);
        acc.add_sample(50);
        acc.add_sample(100);
        acc.add_sample(150);
        assert_eq!(acc.range(), 100);
    }

    #[test]
    fn test_variance_accumulator_min_max() {
        let mut acc = VarianceAccumulator::new(1);
        acc.add_sample(50);
        acc.add_sample(100);
        acc.add_sample(150);
        assert_eq!(acc.min, 50);
        assert_eq!(acc.max, 150);
    }

    #[test]
    fn test_significance_test_creation() {
        let test = SignificanceTest::new(1, 100, 120);
        assert_eq!(test.id, 1);
        assert_eq!(test.baseline_mean, 100);
    }

    #[test]
    fn test_significance_test_large_difference() {
        let test = SignificanceTest::new(1, 100, 600);  // 500 difference
        assert!(test.is_significant);
        assert!(test.p_value_estimate >= 95);
    }

    #[test]
    fn test_significance_test_small_difference() {
        let test = SignificanceTest::new(1, 100, 105);  // 5 difference
        assert!(!test.is_significant);
        assert!(test.p_value_estimate < 95);
    }

    #[test]
    fn test_impact_analyzer_creation() {
        let analyzer = ImpactAnalyzer::new();
        assert_eq!(analyzer.total_analyses(), 0);
    }

    #[test]
    fn test_impact_analyzer_store_baseline() {
        let mut analyzer = ImpactAnalyzer::new();
        let baseline = MutationBaseline::new(1, 1001);
        assert!(analyzer.store_baseline(baseline));
    }

    #[test]
    fn test_impact_analyzer_store_measurement() {
        let mut analyzer = ImpactAnalyzer::new();
        let meas = MutationMeasurement::new(1, 1);
        assert!(analyzer.store_measurement(meas));
    }

    #[test]
    fn test_impact_analyzer_analyze_impact() {
        let mut analyzer = ImpactAnalyzer::new();
        let mut baseline = MutationBaseline::new(1, 1001);
        baseline.baseline_latency = 100;
        baseline.baseline_throughput = 1000;
        baseline.baseline_memory = 256;
        baseline.baseline_efficiency = 80;

        let mut meas = MutationMeasurement::new(1, 1);
        meas.measured_latency = 80;
        meas.measured_throughput = 1200;
        meas.measured_memory = 200;
        meas.measured_efficiency = 90;

        assert!(analyzer.analyze_impact(&meas, &baseline));
        assert_eq!(analyzer.total_analyses(), 1);
    }

    #[test]
    fn test_impact_analyzer_count_improvements() {
        let mut analyzer = ImpactAnalyzer::new();
        let mut baseline = MutationBaseline::new(1, 1001);
        baseline.baseline_latency = 100;
        baseline.baseline_throughput = 1000;
        baseline.baseline_memory = 256;
        baseline.baseline_efficiency = 80;

        let mut meas = MutationMeasurement::new(1, 1);
        meas.measured_latency = 80;
        meas.measured_throughput = 1200;
        meas.measured_memory = 200;
        meas.measured_efficiency = 90;

        analyzer.analyze_impact(&meas, &baseline);
        assert_eq!(analyzer.count_improvements(), 1);
    }

    #[test]
    fn test_impact_analyzer_avg_impact_score() {
        let mut analyzer = ImpactAnalyzer::new();
        let mut baseline = MutationBaseline::new(1, 1001);
        baseline.baseline_latency = 100;
        baseline.baseline_throughput = 1000;
        baseline.baseline_memory = 256;
        baseline.baseline_efficiency = 80;

        let mut meas = MutationMeasurement::new(1, 1);
        meas.measured_latency = 80;
        meas.measured_throughput = 1200;
        meas.measured_memory = 200;
        meas.measured_efficiency = 90;

        analyzer.analyze_impact(&meas, &baseline);
        let avg = analyzer.avg_impact_score();
        assert!(avg > 50);
    }

    #[test]
    fn test_impact_analyzer_improvement_rate() {
        let mut analyzer = ImpactAnalyzer::new();
        let mut baseline = MutationBaseline::new(1, 1001);
        baseline.baseline_latency = 100;
        baseline.baseline_throughput = 1000;
        baseline.baseline_memory = 256;
        baseline.baseline_efficiency = 80;

        let mut meas_good = MutationMeasurement::new(1, 1);
        meas_good.measured_latency = 80;
        meas_good.measured_throughput = 1200;
        meas_good.measured_memory = 200;
        meas_good.measured_efficiency = 90;

        let mut meas_bad = MutationMeasurement::new(2, 1);
        meas_bad.measured_latency = 120;
        meas_bad.measured_throughput = 800;
        meas_bad.measured_memory = 300;
        meas_bad.measured_efficiency = 70;

        analyzer.analyze_impact(&meas_good, &baseline);
        analyzer.analyze_impact(&meas_bad, &baseline);

        let rate = analyzer.improvement_rate();
        assert_eq!(rate, 50);  // 1 improvement out of 2
    }

    #[test]
    fn test_impact_analyzer_best_worst_impact() {
        let mut analyzer = ImpactAnalyzer::new();
        let mut baseline = MutationBaseline::new(1, 1001);
        baseline.baseline_latency = 100;
        baseline.baseline_throughput = 1000;
        baseline.baseline_memory = 256;
        baseline.baseline_efficiency = 80;

        let mut meas1 = MutationMeasurement::new(1, 1);
        meas1.measured_latency = 50;   // Great improvement
        meas1.measured_throughput = 2000;
        meas1.measured_memory = 128;
        meas1.measured_efficiency = 95;

        let mut meas2 = MutationMeasurement::new(2, 1);
        meas2.measured_latency = 150;  // Regression
        meas2.measured_throughput = 500;
        meas2.measured_memory = 400;
        meas2.measured_efficiency = 50;

        analyzer.analyze_impact(&meas1, &baseline);
        analyzer.analyze_impact(&meas2, &baseline);

        let best = analyzer.best_impact();
        let worst = analyzer.worst_impact();
        assert!(best.is_some() && best.unwrap() > 75);
        assert!(worst.is_some() && worst.unwrap() < 50);
    }

    #[test]
    fn test_impact_analyzer_statistical_storage() {
        let mut analyzer = ImpactAnalyzer::new();
        let stat = StatisticalResult::new(1, 50, 400);
        assert!(analyzer.store_statistical(stat));
    }

    #[test]
    fn test_impact_analyzer_significance_storage() {
        let mut analyzer = ImpactAnalyzer::new();
        let test = SignificanceTest::new(1, 100, 600);
        assert!(analyzer.store_significance_test(test));
    }

    #[test]
    fn test_impact_analyzer_max_baselines() {
        let mut analyzer = ImpactAnalyzer::new();
        for i in 0..50 {
            let baseline = MutationBaseline::new(i, 1000 + i as u32);
            assert!(analyzer.store_baseline(baseline));
        }
        // 51st should fail
        let baseline = MutationBaseline::new(50, 1050);
        assert!(!analyzer.store_baseline(baseline));
    }

    #[test]
    fn test_impact_analyzer_max_measurements() {
        let mut analyzer = ImpactAnalyzer::new();
        for i in 0..100 {
            let meas = MutationMeasurement::new(i, 1);
            assert!(analyzer.store_measurement(meas));
        }
        // 101st should fail
        let meas = MutationMeasurement::new(100, 1);
        assert!(!analyzer.store_measurement(meas));
    }

    #[test]
    fn test_impact_analyzer_measurements_for_baseline() {
        let mut analyzer = ImpactAnalyzer::new();
        let meas1 = MutationMeasurement::new(1, 1);
        let meas2 = MutationMeasurement::new(2, 1);
        let meas3 = MutationMeasurement::new(3, 2);
        analyzer.store_measurement(meas1);
        analyzer.store_measurement(meas2);
        analyzer.store_measurement(meas3);

        assert_eq!(analyzer.measurements_for_baseline(1), 2);
        assert_eq!(analyzer.measurements_for_baseline(2), 1);
    }

    #[test]
    fn test_impact_analyzer_clear() {
        let mut analyzer = ImpactAnalyzer::new();
        let baseline = MutationBaseline::new(1, 1001);
        analyzer.store_baseline(baseline);
        let meas = MutationMeasurement::new(1, 1);
        analyzer.store_measurement(meas);

        analyzer.clear();
        assert_eq!(analyzer.total_analyses(), 0);
        assert_eq!(analyzer.measurements_for_baseline(1), 0);
    }

    #[test]
    fn test_impact_analyzer_statistics() {
        let mut analyzer = ImpactAnalyzer::new();
        let mut baseline = MutationBaseline::new(1, 1001);
        baseline.baseline_latency = 100;
        baseline.baseline_throughput = 1000;
        baseline.baseline_memory = 256;
        baseline.baseline_efficiency = 80;

        let mut meas = MutationMeasurement::new(1, 1);
        meas.measured_latency = 80;
        meas.measured_throughput = 1200;
        meas.measured_memory = 200;
        meas.measured_efficiency = 90;

        analyzer.analyze_impact(&meas, &baseline);

        let (total, avg, improvements, regressions, rate) = analyzer.statistics();
        assert_eq!(total, 1);
        assert!(avg > 50);
        assert_eq!(improvements, 1);
        assert_eq!(regressions, 0);
        assert_eq!(rate, 100);
    }
}
