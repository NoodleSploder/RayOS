//! Performance Measurement System: Latency and Throughput Tracking
//!
//! Comprehensive performance measurement across all evolution operations.
//! Tracks latency, throughput, resource utilization, and computes efficiency
//! scores for mutation evaluation and system optimization.
//!
//! Phase 35, Task 2

/// Performance metric type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PerfMetricType {
    Latency = 0,           // Execution time (ms)
    Throughput = 1,        // Operations per second
    CpuUsage = 2,          // CPU percentage
    MemoryUsage = 3,       // Memory in MB
    CacheHitRate = 4,      // Cache hits percentage
    BranchMisprediction = 5, // Branch misses percentage
    PowerConsumption = 6,  // Power in mW
    Efficiency = 7,        // Operations per watt
}

/// Performance measurement point
#[derive(Clone, Copy, Debug)]
pub struct PerfMeasurement {
    /// Measurement ID
    pub id: u32,
    /// Metric type
    pub metric_type: PerfMetricType,
    /// Measured value
    pub value: u32,
    /// Timestamp (ms)
    pub timestamp_ms: u64,
}

impl PerfMeasurement {
    /// Create new performance measurement
    pub const fn new(id: u32, metric_type: PerfMetricType, value: u32) -> Self {
        PerfMeasurement {
            id,
            metric_type,
            value,
            timestamp_ms: 0,
        }
    }

    /// Set timestamp
    pub fn with_timestamp(mut self, ts_ms: u64) -> Self {
        self.timestamp_ms = ts_ms;
        self
    }
}

/// Latency sample
#[derive(Clone, Copy, Debug)]
pub struct LatencySample {
    /// Sample ID
    pub id: u32,
    /// Operation name hash
    pub operation_hash: u32,
    /// Latency (ms)
    pub latency_ms: u32,
}

impl LatencySample {
    /// Create new latency sample
    pub const fn new(id: u32, operation_hash: u32, latency_ms: u32) -> Self {
        LatencySample {
            id,
            operation_hash,
            latency_ms,
        }
    }
}

/// Throughput measurement
#[derive(Clone, Copy, Debug)]
pub struct ThroughputSample {
    /// Sample ID
    pub id: u32,
    /// Operation hash
    pub operation_hash: u32,
    /// Operations completed
    pub operations: u32,
    /// Time window (ms)
    pub time_window_ms: u32,
}

impl ThroughputSample {
    /// Create new throughput sample
    pub const fn new(id: u32, operation_hash: u32, ops: u32, window_ms: u32) -> Self {
        ThroughputSample {
            id,
            operation_hash,
            operations: ops,
            time_window_ms: window_ms,
        }
    }

    /// Calculate ops per second
    pub fn ops_per_second(&self) -> u32 {
        if self.time_window_ms == 0 {
            0
        } else {
            (self.operations as u64 * 1000 / self.time_window_ms as u64) as u32
        }
    }
}

/// Resource utilization snapshot
#[derive(Clone, Copy, Debug)]
pub struct ResourceUtilization {
    /// Snapshot ID
    pub id: u32,
    /// CPU usage (0-100%)
    pub cpu_percent: u8,
    /// Memory usage (MB)
    pub memory_mb: u16,
    /// Cache hit rate (0-100%)
    pub cache_hit_rate: u8,
    /// Power consumption (mW)
    pub power_mw: u16,
}

impl ResourceUtilization {
    /// Create new resource utilization snapshot
    pub const fn new(id: u32) -> Self {
        ResourceUtilization {
            id,
            cpu_percent: 0,
            memory_mb: 0,
            cache_hit_rate: 0,
            power_mw: 0,
        }
    }

    /// Get efficiency score (ops per watt, normalized 0-100)
    pub fn efficiency_score(&self) -> u8 {
        if self.power_mw == 0 {
            0
        } else {
            let efficiency = (self.cache_hit_rate as u32 * 100) / self.power_mw as u32;
            (efficiency.min(100)) as u8
        }
    }
}

/// Performance baseline for comparison
#[derive(Clone, Copy, Debug)]
pub struct PerfBaseline {
    /// Baseline ID
    pub id: u32,
    /// Target operation hash
    pub operation_hash: u32,
    /// Baseline latency (ms)
    pub baseline_latency_ms: u32,
    /// Baseline throughput (ops/sec)
    pub baseline_throughput: u32,
    /// Baseline power (mW)
    pub baseline_power_mw: u16,
    /// Baseline memory (MB)
    pub baseline_memory_mb: u16,
}

impl PerfBaseline {
    /// Create new performance baseline
    pub const fn new(id: u32, op_hash: u32) -> Self {
        PerfBaseline {
            id,
            operation_hash: op_hash,
            baseline_latency_ms: 0,
            baseline_throughput: 0,
            baseline_power_mw: 0,
            baseline_memory_mb: 0,
        }
    }
}

/// Performance comparison result
#[derive(Clone, Copy, Debug)]
pub struct PerfComparison {
    /// Comparison ID
    pub id: u32,
    /// Baseline ID
    pub baseline_id: u32,
    /// Current measurement
    pub current_measurement: u32,
    /// Improvement percent (-100 to +100)
    pub improvement_percent: i8,
    /// Is improvement
    pub is_improved: bool,
}

impl PerfComparison {
    /// Create new performance comparison
    pub const fn new(id: u32, baseline_id: u32, current: u32, baseline: u32) -> Self {
        let improvement = if baseline == 0 {
            0
        } else {
            ((current as i32 - baseline as i32) * 100) / baseline as i32
        };

        // Manually clamp improvement to [-100, 100]
        let clamped = if improvement > 100 {
            100
        } else if improvement < -100 {
            -100
        } else {
            improvement
        };

        let is_improved = improvement < 0;  // Lower is better for latency

        PerfComparison {
            id,
            baseline_id,
            current_measurement: current,
            improvement_percent: clamped as i8,
            is_improved,
        }
    }
}

/// Phase performance statistics
#[derive(Clone, Copy, Debug)]
pub struct PhasePerformance {
    /// Phase performance ID
    pub id: u32,
    /// Phase identifier
    pub phase_id: u8,
    /// Total time (ms)
    pub total_time_ms: u32,
    /// Average latency (ms)
    pub avg_latency_ms: u16,
    /// Peak CPU usage (%)
    pub peak_cpu_percent: u8,
    /// Peak memory (MB)
    pub peak_memory_mb: u16,
    /// Operations completed
    pub operations: u32,
}

impl PhasePerformance {
    /// Create new phase performance stats
    pub const fn new(id: u32, phase_id: u8) -> Self {
        PhasePerformance {
            id,
            phase_id,
            total_time_ms: 0,
            avg_latency_ms: 0,
            peak_cpu_percent: 0,
            peak_memory_mb: 0,
            operations: 0,
        }
    }

    /// Calculate average throughput (ops per second)
    pub fn avg_throughput(&self) -> u32 {
        if self.total_time_ms == 0 {
            0
        } else {
            (self.operations as u64 * 1000 / self.total_time_ms as u64) as u32
        }
    }
}

/// Performance Measurement System
pub struct PerformanceMetrics {
    /// Latency samples (max 200)
    latency_samples: [Option<LatencySample>; 200],
    /// Throughput samples (max 200)
    throughput_samples: [Option<ThroughputSample>; 200],
    /// Resource utilization snapshots (max 100)
    resource_snapshots: [Option<ResourceUtilization>; 100],
    /// Performance baselines (max 50)
    baselines: [Option<PerfBaseline>; 50],
    /// Performance comparisons (max 100)
    comparisons: [Option<PerfComparison>; 100],
    /// Phase performance stats (max 50)
    phase_stats: [Option<PhasePerformance>; 50],
    /// Total measurements recorded
    total_measurements: u32,
}

impl PerformanceMetrics {
    /// Create new performance metrics system
    pub const fn new() -> Self {
        PerformanceMetrics {
            latency_samples: [None; 200],
            throughput_samples: [None; 200],
            resource_snapshots: [None; 100],
            baselines: [None; 50],
            comparisons: [None; 100],
            phase_stats: [None; 50],
            total_measurements: 0,
        }
    }

    /// Record latency sample
    pub fn record_latency(&mut self, sample: LatencySample) -> bool {
        for slot in &mut self.latency_samples {
            if slot.is_none() {
                *slot = Some(sample);
                self.total_measurements += 1;
                return true;
            }
        }
        false
    }

    /// Record throughput sample
    pub fn record_throughput(&mut self, sample: ThroughputSample) -> bool {
        for slot in &mut self.throughput_samples {
            if slot.is_none() {
                *slot = Some(sample);
                self.total_measurements += 1;
                return true;
            }
        }
        false
    }

    /// Record resource utilization snapshot
    pub fn record_resources(&mut self, snapshot: ResourceUtilization) -> bool {
        for slot in &mut self.resource_snapshots {
            if slot.is_none() {
                *slot = Some(snapshot);
                return true;
            }
        }
        false
    }

    /// Establish baseline
    pub fn set_baseline(&mut self, baseline: PerfBaseline) -> bool {
        for slot in &mut self.baselines {
            if slot.is_none() {
                *slot = Some(baseline);
                return true;
            }
        }
        false
    }

    /// Compare against baseline
    pub fn compare_to_baseline(&mut self, baseline_id: u32, current: u32) -> bool {
        // Find baseline
        let baseline_value = {
            let mut found = None;
            for baseline in &self.baselines {
                if let Some(b) = baseline {
                    if b.id == baseline_id {
                        found = Some(b.baseline_latency_ms);
                        break;
                    }
                }
            }
            found
        };

        if let Some(baseline_val) = baseline_value {
            let comparison = PerfComparison::new(self.total_measurements, baseline_id, current, baseline_val);
            for slot in &mut self.comparisons {
                if slot.is_none() {
                    *slot = Some(comparison);
                    return true;
                }
            }
        }
        false
    }

    /// Record phase performance statistics
    pub fn record_phase_performance(&mut self, phase_perf: PhasePerformance) -> bool {
        for slot in &mut self.phase_stats {
            if slot.is_none() {
                *slot = Some(phase_perf);
                return true;
            }
        }
        false
    }

    /// Get average latency for operation
    pub fn avg_latency_for_operation(&self, operation_hash: u32) -> u32 {
        let mut total = 0u64;
        let mut count = 0u64;

        for sample in &self.latency_samples {
            if let Some(s) = sample {
                if s.operation_hash == operation_hash {
                    total += s.latency_ms as u64;
                    count += 1;
                }
            }
        }

        if count == 0 {
            0
        } else {
            (total / count) as u32
        }
    }

    /// Get average throughput for operation
    pub fn avg_throughput_for_operation(&self, operation_hash: u32) -> u32 {
        let mut total = 0u64;
        let mut count = 0u64;

        for sample in &self.throughput_samples {
            if let Some(s) = sample {
                if s.operation_hash == operation_hash {
                    total += s.ops_per_second() as u64;
                    count += 1;
                }
            }
        }

        if count == 0 {
            0
        } else {
            (total / count) as u32
        }
    }

    /// Get peak CPU usage
    pub fn peak_cpu_usage(&self) -> u8 {
        let mut peak = 0u8;
        for snapshot in &self.resource_snapshots {
            if let Some(s) = snapshot {
                peak = peak.max(s.cpu_percent);
            }
        }
        peak
    }

    /// Get average memory usage
    pub fn avg_memory_usage(&self) -> u16 {
        let mut total = 0u32;
        let mut count = 0u32;

        for snapshot in &self.resource_snapshots {
            if let Some(s) = snapshot {
                total += s.memory_mb as u32;
                count += 1;
            }
        }

        if count == 0 {
            0
        } else {
            (total / count) as u16
        }
    }

    /// Get efficiency score across all measurements
    pub fn overall_efficiency_score(&self) -> u8 {
        let mut total = 0u32;
        let mut count = 0u32;

        for snapshot in &self.resource_snapshots {
            if let Some(s) = snapshot {
                total += s.efficiency_score() as u32;
                count += 1;
            }
        }

        if count == 0 {
            50
        } else {
            (total / count) as u8
        }
    }

    /// Get improvements count
    pub fn improvements_count(&self) -> usize {
        self.comparisons.iter().filter(|c| c.map(|comp| comp.is_improved).unwrap_or(false)).count()
    }

    /// Get measurements count by type
    pub fn measurements_by_type(&self) -> (usize, usize, usize) {
        let latency_count = self.latency_samples.iter().filter(|s| s.is_some()).count();
        let throughput_count = self.throughput_samples.iter().filter(|s| s.is_some()).count();
        let resource_count = self.resource_snapshots.iter().filter(|s| s.is_some()).count();

        (latency_count, throughput_count, resource_count)
    }

    /// Get total measurements
    pub fn total_measurements(&self) -> u32 {
        self.total_measurements
    }

    /// Get statistics summary
    pub fn statistics(&self) -> (u32, u8, u16, u8) {
        (
            self.total_measurements,
            self.peak_cpu_usage(),
            self.avg_memory_usage(),
            self.overall_efficiency_score(),
        )
    }

    /// Clear old measurements (keep last N)
    pub fn prune_old(&mut self, keep_last: usize) {
        let (lat_count, thru_count, _) = self.measurements_by_type();
        let total_meas = lat_count + thru_count;

        if total_meas <= keep_last {
            return;
        }

        let skip = total_meas - keep_last;
        let mut skipped = 0;

        for slot in &mut self.latency_samples {
            if slot.is_some() && skipped < skip {
                *slot = None;
                skipped += 1;
            }
        }

        for slot in &mut self.throughput_samples {
            if slot.is_some() && skipped < skip {
                *slot = None;
                skipped += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perf_metric_type_enum() {
        assert_eq!(PerfMetricType::Latency as u8, 0);
        assert_eq!(PerfMetricType::Efficiency as u8, 7);
    }

    #[test]
    fn test_perf_measurement_creation() {
        let meas = PerfMeasurement::new(1, PerfMetricType::Latency, 100);
        assert_eq!(meas.id, 1);
        assert_eq!(meas.value, 100);
    }

    #[test]
    fn test_latency_sample_creation() {
        let sample = LatencySample::new(1, 0x1234, 50);
        assert_eq!(sample.latency_ms, 50);
    }

    #[test]
    fn test_throughput_sample_creation() {
        let sample = ThroughputSample::new(1, 0x1234, 1000, 1000);
        assert_eq!(sample.operations, 1000);
    }

    #[test]
    fn test_throughput_sample_ops_per_second() {
        let sample = ThroughputSample::new(1, 0x1234, 1000, 1000);
        assert_eq!(sample.ops_per_second(), 1000);
    }

    #[test]
    fn test_throughput_sample_ops_per_second_half_second() {
        let sample = ThroughputSample::new(1, 0x1234, 500, 500);
        assert_eq!(sample.ops_per_second(), 1000);
    }

    #[test]
    fn test_resource_utilization_creation() {
        let res = ResourceUtilization::new(1);
        assert_eq!(res.id, 1);
        assert_eq!(res.cpu_percent, 0);
    }

    #[test]
    fn test_resource_utilization_efficiency_score() {
        let mut res = ResourceUtilization::new(1);
        res.cache_hit_rate = 80;
        res.power_mw = 1000;
        let score = res.efficiency_score();
        assert!(score >= 7 && score <= 9);  // (80 * 100) / 1000 = 8
    }

    #[test]
    fn test_perf_baseline_creation() {
        let baseline = PerfBaseline::new(1, 0x1234);
        assert_eq!(baseline.operation_hash, 0x1234);
    }

    #[test]
    fn test_perf_comparison_improvement() {
        let comp = PerfComparison::new(1, 1, 40, 50);  // 40 < 50, improved
        assert!(comp.is_improved);
        assert!(comp.improvement_percent < 0);
    }

    #[test]
    fn test_perf_comparison_regression() {
        let comp = PerfComparison::new(1, 1, 60, 50);  // 60 > 50, regressed
        assert!(!comp.is_improved);
        assert!(comp.improvement_percent > 0);
    }

    #[test]
    fn test_phase_performance_creation() {
        let phase_perf = PhasePerformance::new(1, 1);
        assert_eq!(phase_perf.phase_id, 1);
    }

    #[test]
    fn test_phase_performance_avg_throughput() {
        let mut phase_perf = PhasePerformance::new(1, 1);
        phase_perf.total_time_ms = 1000;
        phase_perf.operations = 5000;
        assert_eq!(phase_perf.avg_throughput(), 5000);  // 5000 ops per second
    }

    #[test]
    fn test_performance_metrics_creation() {
        let metrics = PerformanceMetrics::new();
        assert_eq!(metrics.total_measurements(), 0);
    }

    #[test]
    fn test_performance_metrics_record_latency() {
        let mut metrics = PerformanceMetrics::new();
        let sample = LatencySample::new(1, 0x1234, 50);
        assert!(metrics.record_latency(sample));
        assert_eq!(metrics.total_measurements(), 1);
    }

    #[test]
    fn test_performance_metrics_record_throughput() {
        let mut metrics = PerformanceMetrics::new();
        let sample = ThroughputSample::new(1, 0x1234, 1000, 1000);
        assert!(metrics.record_throughput(sample));
        assert_eq!(metrics.total_measurements(), 1);
    }

    #[test]
    fn test_performance_metrics_record_resources() {
        let mut metrics = PerformanceMetrics::new();
        let snapshot = ResourceUtilization::new(1);
        assert!(metrics.record_resources(snapshot));
    }

    #[test]
    fn test_performance_metrics_set_baseline() {
        let mut metrics = PerformanceMetrics::new();
        let baseline = PerfBaseline::new(1, 0x1234);
        assert!(metrics.set_baseline(baseline));
    }

    #[test]
    fn test_performance_metrics_compare_to_baseline() {
        let mut metrics = PerformanceMetrics::new();
        let mut baseline = PerfBaseline::new(1, 0x1234);
        baseline.baseline_latency_ms = 100;
        metrics.set_baseline(baseline);
        assert!(metrics.compare_to_baseline(1, 80));  // Better than baseline
    }

    #[test]
    fn test_performance_metrics_avg_latency() {
        let mut metrics = PerformanceMetrics::new();
        metrics.record_latency(LatencySample::new(1, 0x1234, 50));
        metrics.record_latency(LatencySample::new(2, 0x1234, 60));
        let avg = metrics.avg_latency_for_operation(0x1234);
        assert!(avg >= 54 && avg <= 56);  // Average of 50 and 60
    }

    #[test]
    fn test_performance_metrics_avg_throughput() {
        let mut metrics = PerformanceMetrics::new();
        metrics.record_throughput(ThroughputSample::new(1, 0x1234, 1000, 1000));
        metrics.record_throughput(ThroughputSample::new(2, 0x1234, 2000, 1000));
        let avg = metrics.avg_throughput_for_operation(0x1234);
        assert!(avg >= 1499 && avg <= 1501);  // Average of 1000 and 2000
    }

    #[test]
    fn test_performance_metrics_peak_cpu() {
        let mut metrics = PerformanceMetrics::new();
        let mut res1 = ResourceUtilization::new(1);
        res1.cpu_percent = 60;
        let mut res2 = ResourceUtilization::new(2);
        res2.cpu_percent = 80;
        metrics.record_resources(res1);
        metrics.record_resources(res2);
        assert_eq!(metrics.peak_cpu_usage(), 80);
    }

    #[test]
    fn test_performance_metrics_avg_memory() {
        let mut metrics = PerformanceMetrics::new();
        let mut res1 = ResourceUtilization::new(1);
        res1.memory_mb = 100;
        let mut res2 = ResourceUtilization::new(2);
        res2.memory_mb = 200;
        metrics.record_resources(res1);
        metrics.record_resources(res2);
        assert_eq!(metrics.avg_memory_usage(), 150);
    }

    #[test]
    fn test_performance_metrics_overall_efficiency() {
        let mut metrics = PerformanceMetrics::new();
        let mut res = ResourceUtilization::new(1);
        res.cache_hit_rate = 80;
        res.power_mw = 1000;
        metrics.record_resources(res);
        let score = metrics.overall_efficiency_score();
        assert!(score >= 7 && score <= 9);
    }

    #[test]
    fn test_performance_metrics_improvements_count() {
        let mut metrics = PerformanceMetrics::new();
        let mut baseline = PerfBaseline::new(1, 0x1234);
        baseline.baseline_latency_ms = 100;
        metrics.set_baseline(baseline);
        metrics.compare_to_baseline(1, 80);
        metrics.compare_to_baseline(1, 120);
        assert_eq!(metrics.improvements_count(), 1);
    }

    #[test]
    fn test_performance_metrics_measurements_by_type() {
        let mut metrics = PerformanceMetrics::new();
        metrics.record_latency(LatencySample::new(1, 0x1234, 50));
        metrics.record_latency(LatencySample::new(2, 0x1234, 60));
        metrics.record_throughput(ThroughputSample::new(1, 0x1234, 1000, 1000));
        let mut res = ResourceUtilization::new(1);
        res.cpu_percent = 50;
        metrics.record_resources(res);

        let (lat, thru, res_count) = metrics.measurements_by_type();
        assert_eq!(lat, 2);
        assert_eq!(thru, 1);
        assert_eq!(res_count, 1);
    }

    #[test]
    fn test_performance_metrics_statistics() {
        let mut metrics = PerformanceMetrics::new();
        let mut res = ResourceUtilization::new(1);
        res.cpu_percent = 80;
        res.memory_mb = 256;
        metrics.record_resources(res);
        metrics.record_latency(LatencySample::new(1, 0x1234, 50));

        let (total, peak_cpu, avg_mem, efficiency) = metrics.statistics();
        assert_eq!(total, 1);
        assert_eq!(peak_cpu, 80);
        assert_eq!(avg_mem, 256);
    }

    #[test]
    fn test_performance_metrics_max_latency_samples() {
        let mut metrics = PerformanceMetrics::new();
        for i in 0..200 {
            let sample = LatencySample::new(i, 0x1234, 50);
            assert!(metrics.record_latency(sample));
        }
        // 201st should fail
        let sample = LatencySample::new(200, 0x1234, 50);
        assert!(!metrics.record_latency(sample));
    }

    #[test]
    fn test_performance_metrics_max_baselines() {
        let mut metrics = PerformanceMetrics::new();
        for i in 0..50 {
            let baseline = PerfBaseline::new(i, 0x1000 + i as u32);
            assert!(metrics.set_baseline(baseline));
        }
        // 51st should fail
        let baseline = PerfBaseline::new(50, 0x2000);
        assert!(!metrics.set_baseline(baseline));
    }

    #[test]
    fn test_performance_metrics_record_phase() {
        let mut metrics = PerformanceMetrics::new();
        let mut phase_perf = PhasePerformance::new(1, 1);
        phase_perf.total_time_ms = 1000;
        phase_perf.operations = 5000;
        assert!(metrics.record_phase_performance(phase_perf));
    }

    #[test]
    fn test_throughput_sample_zero_window() {
        let sample = ThroughputSample::new(1, 0x1234, 1000, 0);
        assert_eq!(sample.ops_per_second(), 0);  // Division by zero protection
    }

    #[test]
    fn test_resource_utilization_zero_power() {
        let res = ResourceUtilization::new(1);
        assert_eq!(res.efficiency_score(), 0);  // No power = no efficiency
    }
}
