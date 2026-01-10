//! Performance Analysis Tools for Evolution Metrics
//!
//! Provides before/after performance comparison, trend analysis,
//! statistical analysis, and optimization recommendations.
//!
//! Phase 33, Task 5

/// Performance snapshot for comparison
#[derive(Clone, Copy, Debug)]
pub struct PerformanceSnapshot {
    /// Cycle number
    pub cycle: u32,
    /// Throughput (mutations/sec)
    pub throughput: u32,
    /// Average cycle duration (ms)
    pub cycle_duration_ms: u32,
    /// Memory footprint (KB)
    pub memory_kb: u32,
    /// Success rate percent
    pub success_rate: u32,
}

impl PerformanceSnapshot {
    /// Create new snapshot
    pub const fn new(cycle: u32, throughput: u32, cycle_duration_ms: u32, memory_kb: u32, success_rate: u32) -> Self {
        PerformanceSnapshot {
            cycle,
            throughput,
            cycle_duration_ms,
            memory_kb,
            success_rate,
        }
    }
}

/// Performance comparison result
#[derive(Clone, Copy, Debug)]
pub struct ComparisonResult {
    /// Throughput delta (1000x = 1.0x improvement)
    pub throughput_delta: i32,
    /// Cycle duration delta (ms) - negative is better
    pub duration_delta_ms: i32,
    /// Memory delta (KB) - negative is better
    pub memory_delta_kb: i16,
    /// Success rate delta (percent points)
    pub success_delta: i16,
    /// Overall improvement score (0-1000)
    pub overall_score: u32,
}

impl ComparisonResult {
    /// Compare two snapshots
    pub fn compare(before: PerformanceSnapshot, after: PerformanceSnapshot) -> Self {
        let throughput_delta = (after.throughput as i32) - (before.throughput as i32);
        let duration_delta_ms = (after.cycle_duration_ms as i32) - (before.cycle_duration_ms as i32);
        let memory_delta_kb = (after.memory_kb as i16) - (before.memory_kb as i16);
        let success_delta = (after.success_rate as i16) - (before.success_rate as i16);

        // Calculate overall score: higher throughput + lower duration + lower memory + higher success
        let mut score = 0u32;
        if throughput_delta > 0 {
            score = score.saturating_add((throughput_delta * 5) as u32);
        }
        if duration_delta_ms < 0 {
            score = score.saturating_add(((-duration_delta_ms) * 2) as u32);
        }
        if memory_delta_kb < 0 {
            score = score.saturating_add(((-memory_delta_kb as i32) * 1) as u32);
        }
        if success_delta > 0 {
            score = score.saturating_add((success_delta as u32) * 10);
        }

        ComparisonResult {
            throughput_delta,
            duration_delta_ms,
            memory_delta_kb,
            success_delta,
            overall_score: score.min(1000),
        }
    }

    /// Get improvement percentage for throughput (delta / before * 100, scaled)
    pub fn throughput_improvement_percent(&self) -> i16 {
        (self.throughput_delta / 10) as i16
    }
}

/// Trend direction
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TrendDirection {
    Improving = 0,    // Getting better
    Stable = 1,       // Staying same
    Degrading = 2,    // Getting worse
}

/// Performance trend over multiple cycles
#[derive(Clone, Copy, Debug)]
pub struct PerformanceTrend {
    /// Number of data points
    pub data_points: u32,
    /// Trend direction
    pub direction: TrendDirection,
    /// Slope (change per cycle)
    pub slope: i32,
    /// Min observed value
    pub min_value: u32,
    /// Max observed value
    pub max_value: u32,
    /// Average value
    pub avg_value: u32,
    /// Volatility (std dev approximation)
    pub volatility: u32,
}

impl PerformanceTrend {
    /// Analyze throughput trend from snapshots
    pub fn analyze_throughput(snapshots: &[PerformanceSnapshot]) -> Self {
        if snapshots.is_empty() {
            return PerformanceTrend {
                data_points: 0,
                direction: TrendDirection::Stable,
                slope: 0,
                min_value: 0,
                max_value: 0,
                avg_value: 0,
                volatility: 0,
            };
        }

        let mut min = snapshots[0].throughput;
        let mut max = snapshots[0].throughput;
        let mut sum = snapshots[0].throughput as u64;

        for snapshot in &snapshots[1..] {
            if snapshot.throughput < min {
                min = snapshot.throughput;
            }
            if snapshot.throughput > max {
                max = snapshot.throughput;
            }
            sum = sum.saturating_add(snapshot.throughput as u64);
        }

        let avg = (sum / snapshots.len() as u64) as u32;

        // Calculate slope: (last - first) / data_points
        let slope = if snapshots.len() > 1 {
            (snapshots[snapshots.len() - 1].throughput as i32) - (snapshots[0].throughput as i32)
        } else {
            0
        };

        let direction = if slope > 50 {
            TrendDirection::Improving
        } else if slope < -50 {
            TrendDirection::Degrading
        } else {
            TrendDirection::Stable
        };

        PerformanceTrend {
            data_points: snapshots.len() as u32,
            direction,
            slope,
            min_value: min,
            max_value: max,
            avg_value: avg,
            volatility: (max - min) / 2,
        }
    }
}

/// Recommendation priority
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RecommendationPriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

/// Performance optimization recommendation
#[derive(Clone, Copy, Debug)]
pub struct Recommendation {
    /// Recommendation ID
    pub id: u32,
    /// Priority level
    pub priority: RecommendationPriority,
    /// Estimated improvement (percent)
    pub estimated_improvement: u32,
    /// Effort level (1-10)
    pub effort: u8,
    /// Category (e.g., "batching", "caching", "parallelization")
    pub category: u8,
    /// Confidence score (0-100)
    pub confidence: u8,
}

impl Recommendation {
    /// Create new recommendation
    pub const fn new(id: u32, priority: RecommendationPriority, estimated_improvement: u32, effort: u8, category: u8, confidence: u8) -> Self {
        Recommendation {
            id,
            priority,
            estimated_improvement,
            effort,
            category,
            confidence,
        }
    }

    /// Calculate ROI score (improvement / effort)
    pub fn roi_score(&self) -> u32 {
        if self.effort == 0 {
            return 0;
        }
        (self.estimated_improvement as u32 * 100) / self.effort as u32
    }
}

/// Analysis report
#[derive(Clone, Copy, Debug)]
pub struct AnalysisReport {
    /// Report ID
    pub id: u32,
    /// Baseline cycle
    pub baseline_cycle: u32,
    /// Current cycle
    pub current_cycle: u32,
    /// Comparison result
    pub comparison: ComparisonResult,
    /// Throughput trend
    pub throughput_trend: PerformanceTrend,
    /// Top recommendation count
    pub top_recommendations: u32,
}

impl AnalysisReport {
    /// Create new analysis report
    pub const fn new(id: u32, baseline_cycle: u32, current_cycle: u32, comparison: ComparisonResult, throughput_trend: PerformanceTrend) -> Self {
        AnalysisReport {
            id,
            baseline_cycle,
            current_cycle,
            comparison,
            throughput_trend,
            top_recommendations: 0,
        }
    }
}

/// Performance Analyzer
pub struct PerformanceAnalyzer {
    /// Snapshot history (last 5)
    snapshots: [Option<PerformanceSnapshot>; 5],
    /// Current index
    snapshot_index: usize,
    /// Recommendation history (last 10)
    recommendations: [Option<Recommendation>; 10],
    /// Recommendation index
    recommendation_index: usize,
    /// Report counter
    report_counter: u32,
}

impl PerformanceAnalyzer {
    /// Create new analyzer
    pub const fn new() -> Self {
        PerformanceAnalyzer {
            snapshots: [None; 5],
            snapshot_index: 0,
            recommendations: [None; 10],
            recommendation_index: 0,
            report_counter: 0,
        }
    }

    /// Record performance snapshot
    pub fn record_snapshot(&mut self, snapshot: PerformanceSnapshot) {
        self.snapshots[self.snapshot_index] = Some(snapshot);
        self.snapshot_index = (self.snapshot_index + 1) % 5;
    }

    /// Get snapshot history
    pub fn snapshot_history(&self) -> [Option<PerformanceSnapshot>; 5] {
        self.snapshots
    }

    /// Add recommendation
    pub fn add_recommendation(&mut self, rec: Recommendation) {
        self.recommendations[self.recommendation_index] = Some(rec);
        self.recommendation_index = (self.recommendation_index + 1) % 10;
    }

    /// Get top N recommendations by ROI
    pub fn top_recommendations(&self, count: usize) -> [Option<Recommendation>; 10] {
        let mut sorted = self.recommendations;

        // Simple bubble sort by ROI score (descending)
        for i in 0..10 {
            for j in 0..(10 - i - 1) {
                if let (Some(a), Some(b)) = (sorted[j], sorted[j + 1]) {
                    if a.roi_score() < b.roi_score() {
                        sorted.swap(j, j + 1);
                    }
                }
            }
        }

        // Keep top count
        for i in count..10 {
            sorted[i] = None;
        }

        sorted
    }

    /// Analyze performance change
    pub fn analyze_change(&self, snapshot_idx: usize) -> Option<ComparisonResult> {
        if snapshot_idx >= 5 || self.snapshots[snapshot_idx].is_none() {
            return None;
        }

        // Find most recent snapshot
        let mut most_recent = None;
        for i in (snapshot_idx + 1)..5 {
            if let Some(s) = self.snapshots[i] {
                most_recent = Some(s);
                break;
            }
        }

        if let (Some(before), Some(after)) = (self.snapshots[snapshot_idx], most_recent) {
            return Some(ComparisonResult::compare(before, after));
        }

        None
    }

    /// Generate analysis report
    pub fn generate_report(&mut self, snapshot_idx: usize) -> Option<AnalysisReport> {
        if snapshot_idx >= 5 || self.snapshots[snapshot_idx].is_none() {
            return None;
        }

        let snapshot = self.snapshots[snapshot_idx]?;

        // Collect all snapshots for trend analysis
        let mut trend_snapshots = [PerformanceSnapshot::new(0, 0, 0, 0, 0); 5];
        let mut trend_count = 0;

        for i in 0..5 {
            if let Some(s) = self.snapshots[i] {
                trend_snapshots[i] = s;
                trend_count += 1;
            }
        }

        let trend = PerformanceTrend::analyze_throughput(&trend_snapshots[..trend_count]);

        let comparison = if let Some(c) = self.analyze_change(snapshot_idx) {
            c
        } else {
            ComparisonResult::compare(snapshot, snapshot)
        };

        let mut report = AnalysisReport::new(
            self.report_counter,
            snapshot.cycle,
            snapshot.cycle + 1,
            comparison,
            trend,
        );

        self.report_counter += 1;
        report.top_recommendations = 3;

        Some(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_snapshot_creation() {
        let snapshot = PerformanceSnapshot::new(1, 100, 1000, 5120, 75);
        assert_eq!(snapshot.cycle, 1);
        assert_eq!(snapshot.throughput, 100);
    }

    #[test]
    fn test_comparison_result_improvement() {
        let before = PerformanceSnapshot::new(1, 100, 1000, 5120, 75);
        let after = PerformanceSnapshot::new(2, 150, 800, 5000, 80);
        let comparison = ComparisonResult::compare(before, after);

        assert!(comparison.throughput_delta > 0);
        assert!(comparison.duration_delta_ms < 0);
        assert!(comparison.overall_score > 0);
    }

    #[test]
    fn test_comparison_result_degradation() {
        let before = PerformanceSnapshot::new(1, 100, 1000, 5120, 75);
        let after = PerformanceSnapshot::new(2, 50, 1500, 5500, 70);
        let comparison = ComparisonResult::compare(before, after);

        assert!(comparison.throughput_delta < 0);
        assert!(comparison.duration_delta_ms > 0);
    }

    #[test]
    fn test_comparison_throughput_improvement_percent() {
        let before = PerformanceSnapshot::new(1, 100, 1000, 5120, 75);
        let after = PerformanceSnapshot::new(2, 150, 1000, 5120, 75);
        let comparison = ComparisonResult::compare(before, after);

        assert!(comparison.throughput_improvement_percent() > 0);
    }

    #[test]
    fn test_performance_trend_improving() {
        let snapshots = [
            Some(PerformanceSnapshot::new(1, 100, 1000, 5120, 75)),
            Some(PerformanceSnapshot::new(2, 150, 950, 5100, 78)),
            Some(PerformanceSnapshot::new(3, 200, 900, 5080, 80)),
        ];

        let trend = PerformanceTrend::analyze_throughput(&[snapshots[0].unwrap(), snapshots[1].unwrap(), snapshots[2].unwrap()]);
        assert_eq!(trend.direction, TrendDirection::Improving);
        assert!(trend.slope > 0);
    }

    #[test]
    fn test_performance_trend_degrading() {
        let snapshots = [
            PerformanceSnapshot::new(1, 200, 900, 5080, 80),
            PerformanceSnapshot::new(2, 150, 950, 5100, 78),
            PerformanceSnapshot::new(3, 100, 1000, 5120, 75),
        ];

        let trend = PerformanceTrend::analyze_throughput(&snapshots);
        assert_eq!(trend.direction, TrendDirection::Degrading);
        assert!(trend.slope < 0);
    }

    #[test]
    fn test_recommendation_creation() {
        let rec = Recommendation::new(1, RecommendationPriority::High, 15, 5, 1, 85);
        assert_eq!(rec.priority, RecommendationPriority::High);
        assert_eq!(rec.estimated_improvement, 15);
    }

    #[test]
    fn test_recommendation_roi_score() {
        let rec = Recommendation::new(1, RecommendationPriority::High, 100, 5, 1, 85);
        let roi = rec.roi_score();
        assert_eq!(roi, 2000); // 100 / 5 * 100
    }

    #[test]
    fn test_analysis_report_creation() {
        let snapshot = PerformanceSnapshot::new(1, 100, 1000, 5120, 75);
        let comparison = ComparisonResult::compare(snapshot, snapshot);
        let trend = PerformanceTrend {
            data_points: 1,
            direction: TrendDirection::Stable,
            slope: 0,
            min_value: 100,
            max_value: 100,
            avg_value: 100,
            volatility: 0,
        };

        let report = AnalysisReport::new(1, 1, 2, comparison, trend);
        assert_eq!(report.baseline_cycle, 1);
    }

    #[test]
    fn test_performance_analyzer_creation() {
        let analyzer = PerformanceAnalyzer::new();
        assert_eq!(analyzer.report_counter, 0);
    }

    #[test]
    fn test_performance_analyzer_record_snapshot() {
        let mut analyzer = PerformanceAnalyzer::new();
        let snapshot = PerformanceSnapshot::new(1, 100, 1000, 5120, 75);
        analyzer.record_snapshot(snapshot);

        assert!(analyzer.snapshot_history()[0].is_some());
    }

    #[test]
    fn test_performance_analyzer_add_recommendation() {
        let mut analyzer = PerformanceAnalyzer::new();
        let rec = Recommendation::new(1, RecommendationPriority::High, 15, 5, 1, 85);
        analyzer.add_recommendation(rec);

        let recs = analyzer.top_recommendations(1);
        assert!(recs[0].is_some());
    }

    #[test]
    fn test_performance_analyzer_generate_report() {
        let mut analyzer = PerformanceAnalyzer::new();
        let snapshot = PerformanceSnapshot::new(1, 100, 1000, 5120, 75);
        analyzer.record_snapshot(snapshot);

        let report = analyzer.generate_report(0);
        assert!(report.is_some());
    }

    #[test]
    fn test_performance_analyzer_top_recommendations() {
        let mut analyzer = PerformanceAnalyzer::new();

        analyzer.add_recommendation(Recommendation::new(1, RecommendationPriority::High, 10, 5, 1, 85));
        analyzer.add_recommendation(Recommendation::new(2, RecommendationPriority::Medium, 20, 2, 1, 80));
        analyzer.add_recommendation(Recommendation::new(3, RecommendationPriority::Low, 5, 10, 1, 70));

        let top = analyzer.top_recommendations(2);
        assert!(top[0].is_some());
        assert!(top[1].is_some());
        assert!(top[2].is_none());
    }

    #[test]
    fn test_performance_trend_empty() {
        let trend = PerformanceTrend::analyze_throughput(&[]);
        assert_eq!(trend.data_points, 0);
        assert_eq!(trend.direction, TrendDirection::Stable);
    }

    #[test]
    fn test_performance_trend_min_max() {
        let snapshots = [
            PerformanceSnapshot::new(1, 100, 1000, 5120, 75),
            PerformanceSnapshot::new(2, 150, 950, 5100, 78),
            PerformanceSnapshot::new(3, 120, 900, 5080, 80),
        ];

        let trend = PerformanceTrend::analyze_throughput(&snapshots);
        assert_eq!(trend.min_value, 100);
        assert_eq!(trend.max_value, 150);
    }
}
