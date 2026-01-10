//! Evolution Reporting & Visualization: Comprehensive Report Generation and Data Visualization
//!
//! Report generation in multiple formats (HTML, text, JSON) with metrics aggregation,
//! visualization data structures, and comprehensive evolution performance reporting.
//!
//! Phase 35, Task 6

/// Report format type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ReportFormat {
    Text = 0,
    Html = 1,
    Json = 2,
}

impl ReportFormat {
    /// Get format extension
    pub const fn extension(&self) -> &'static str {
        match self {
            ReportFormat::Text => ".txt",
            ReportFormat::Html => ".html",
            ReportFormat::Json => ".json",
        }
    }

    /// Get MIME type
    pub const fn mime_type(&self) -> &'static str {
        match self {
            ReportFormat::Text => "text/plain",
            ReportFormat::Html => "text/html",
            ReportFormat::Json => "application/json",
        }
    }
}

/// Report section type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ReportSection {
    Summary = 0,
    Metrics = 1,
    Evolution = 2,
    Performance = 3,
    Alerts = 4,
    Recommendations = 5,
}

impl ReportSection {
    /// Get section name
    pub const fn name(&self) -> &'static str {
        match self {
            ReportSection::Summary => "Summary",
            ReportSection::Metrics => "Metrics",
            ReportSection::Evolution => "Evolution",
            ReportSection::Performance => "Performance",
            ReportSection::Alerts => "Alerts",
            ReportSection::Recommendations => "Recommendations",
        }
    }
}

/// Metric summary for reports
#[derive(Clone, Copy, Debug)]
pub struct MetricSummary {
    /// Summary ID
    pub id: u32,
    /// Metric name hash
    pub metric_hash: u32,
    /// Current value
    pub current_value: u32,
    /// Average value
    pub average_value: u32,
    /// Min value
    pub min_value: u32,
    /// Max value
    pub max_value: u32,
    /// Change percent (-100 to +100)
    pub change_percent: i8,
}

impl MetricSummary {
    /// Create new metric summary
    pub const fn new(id: u32, metric_hash: u32, current: u32) -> Self {
        MetricSummary {
            id,
            metric_hash,
            current_value: current,
            average_value: 0,
            min_value: 0,
            max_value: 0,
            change_percent: 0,
        }
    }

    /// Set average
    pub fn with_average(mut self, avg: u32) -> Self {
        self.average_value = avg;
        self
    }

    /// Set min/max
    pub fn with_bounds(mut self, min: u32, max: u32) -> Self {
        self.min_value = min;
        self.max_value = max;
        self
    }

    /// Set change percent
    pub fn with_change(mut self, change: i8) -> Self {
        self.change_percent = change;
        self
    }
}

/// Chart data point for visualization
#[derive(Clone, Copy, Debug)]
pub struct ChartPoint {
    /// X coordinate (timestamp or index)
    pub x: u32,
    /// Y coordinate (metric value)
    pub y: u32,
}

impl ChartPoint {
    /// Create new chart point
    pub const fn new(x: u32, y: u32) -> Self {
        ChartPoint { x, y }
    }
}

/// Chart data for visualization
#[derive(Clone, Copy, Debug)]
pub struct Chart {
    /// Chart ID
    pub id: u32,
    /// Chart title hash
    pub title_hash: u32,
    /// Points (max 100)
    points_count: u16,
    /// Min Y value
    pub min_y: u32,
    /// Max Y value
    pub max_y: u32,
}

impl Chart {
    /// Create new chart
    pub const fn new(id: u32, title_hash: u32) -> Self {
        Chart {
            id,
            title_hash,
            points_count: 0,
            min_y: u32::MAX,
            max_y: 0,
        }
    }

    /// Add point to chart
    pub fn add_point(&mut self, point: ChartPoint) -> bool {
        if self.points_count >= 100 {
            return false;
        }

        if point.y < self.min_y {
            self.min_y = point.y;
        }
        if point.y > self.max_y {
            self.max_y = point.y;
        }

        self.points_count += 1;
        true
    }

    /// Get point count
    pub const fn point_count(&self) -> u16 {
        self.points_count
    }

    /// Get Y range
    pub const fn y_range(&self) -> u32 {
        self.max_y.saturating_sub(self.min_y)
    }
}

/// Evolution cycle summary
#[derive(Clone, Copy, Debug)]
pub struct EvolutionCycleSummary {
    /// Cycle ID
    pub id: u32,
    /// Cycle number
    pub cycle_number: u32,
    /// Mutations generated
    pub mutations_generated: u32,
    /// Mutations tested
    pub mutations_tested: u32,
    /// Improvements found
    pub improvements: u32,
    /// Regressions found
    pub regressions: u32,
    /// Success rate percent
    pub success_rate: u8,
    /// Total time (ms)
    pub total_time_ms: u32,
}

impl EvolutionCycleSummary {
    /// Create new cycle summary
    pub const fn new(id: u32, cycle: u32) -> Self {
        EvolutionCycleSummary {
            id,
            cycle_number: cycle,
            mutations_generated: 0,
            mutations_tested: 0,
            improvements: 0,
            regressions: 0,
            success_rate: 0,
            total_time_ms: 0,
        }
    }

    /// Calculate efficiency score
    pub const fn efficiency_score(&self) -> u8 {
        if self.mutations_tested == 0 {
            0
        } else {
            let score = (self.improvements as u64 * 100) / self.mutations_tested as u64;
            let clamped = if score > 100 { 100u64 } else { score };
            clamped as u8
        }
    }
}

/// Report metadata
#[derive(Clone, Copy, Debug)]
pub struct ReportMetadata {
    /// Report ID
    pub id: u32,
    /// Report format
    pub format: ReportFormat,
    /// Generated timestamp (ms)
    pub generated_ms: u64,
    /// Reporting period start (ms)
    pub period_start_ms: u64,
    /// Reporting period end (ms)
    pub period_end_ms: u64,
    /// Report size (bytes)
    pub size_bytes: u32,
}

impl ReportMetadata {
    /// Create new report metadata
    pub const fn new(id: u32, format: ReportFormat, generated_ms: u64) -> Self {
        ReportMetadata {
            id,
            format,
            generated_ms,
            period_start_ms: 0,
            period_end_ms: 0,
            size_bytes: 0,
        }
    }

    /// Set reporting period
    pub fn with_period(mut self, start_ms: u64, end_ms: u64) -> Self {
        self.period_start_ms = start_ms;
        self.period_end_ms = end_ms;
        self
    }

    /// Set size
    pub fn with_size(mut self, size: u32) -> Self {
        self.size_bytes = size;
        self
    }
}

/// Report builder
pub struct ReportBuilder {
    /// Report metadata
    pub metadata: ReportMetadata,
    /// Metric summaries (max 32)
    metrics: [Option<MetricSummary>; 32],
    /// Charts (max 16)
    charts: [Option<Chart>; 16],
    /// Evolution cycles (max 20)
    cycles: [Option<EvolutionCycleSummary>; 20],
    /// Report sections included
    sections_mask: u8,
    /// Custom notes
    notes_hash: u32,
}

impl ReportBuilder {
    /// Create new report builder
    pub const fn new(id: u32, format: ReportFormat, timestamp_ms: u64) -> Self {
        ReportBuilder {
            metadata: ReportMetadata::new(id, format, timestamp_ms),
            metrics: [None; 32],
            charts: [None; 16],
            cycles: [None; 20],
            sections_mask: 0,
            notes_hash: 0,
        }
    }

    /// Add metric summary
    pub fn add_metric(&mut self, metric: MetricSummary) -> bool {
        for slot in &mut self.metrics {
            if slot.is_none() {
                *slot = Some(metric);
                return true;
            }
        }
        false
    }

    /// Add chart
    pub fn add_chart(&mut self, chart: Chart) -> bool {
        for slot in &mut self.charts {
            if slot.is_none() {
                *slot = Some(chart);
                return true;
            }
        }
        false
    }

    /// Add evolution cycle
    pub fn add_cycle(&mut self, cycle: EvolutionCycleSummary) -> bool {
        for slot in &mut self.cycles {
            if slot.is_none() {
                *slot = Some(cycle);
                return true;
            }
        }
        false
    }

    /// Include section
    pub fn include_section(&mut self, section: ReportSection) -> &mut Self {
        self.sections_mask |= 1 << (section as u8);
        self
    }

    /// Check if section included
    pub const fn has_section(&self, section: ReportSection) -> bool {
        (self.sections_mask & (1 << (section as u8))) != 0
    }

    /// Set notes
    pub fn set_notes(&mut self, notes_hash: u32) -> &mut Self {
        self.notes_hash = notes_hash;
        self
    }

    /// Get metric count
    pub fn metric_count(&self) -> u32 {
        self.metrics.iter().filter(|m| m.is_some()).count() as u32
    }

    /// Get chart count
    pub fn chart_count(&self) -> u32 {
        self.charts.iter().filter(|c| c.is_some()).count() as u32
    }

    /// Get cycle count
    pub fn cycle_count(&self) -> u32 {
        self.cycles.iter().filter(|cy| cy.is_some()).count() as u32
    }

    /// Estimate report size
    pub fn estimate_size(&self) -> u32 {
        let mut size = 1000u32;  // Base size

        // Add metric data
        size = size.saturating_add(self.metric_count() * 50);

        // Add chart data
        size = size.saturating_add(self.chart_count() * 200);

        // Add cycle data
        size = size.saturating_add(self.cycle_count() * 100);

        size
    }

    /// Build report and get size estimate
    pub fn build(&mut self) -> u32 {
        let size = self.estimate_size();
        self.metadata = self.metadata.with_size(size);
        size
    }

    /// Get metadata
    pub const fn metadata(&self) -> ReportMetadata {
        self.metadata
    }

    /// Get section count
    pub fn section_count(&self) -> u8 {
        let mut count = 0u8;
        for i in 0..6 {
            if (self.sections_mask & (1 << i)) != 0 {
                count += 1;
            }
        }
        count
    }
}

/// Report system
pub struct ReportingSystem {
    /// Generated reports (max 50)
    reports: [Option<ReportMetadata>; 50],
    /// Report templates (max 10)
    templates_hash: [u32; 10],
    /// Total reports generated
    total_reports: u32,
}

impl ReportingSystem {
    /// Create new reporting system
    pub const fn new() -> Self {
        ReportingSystem {
            reports: [None; 50],
            templates_hash: [0; 10],
            total_reports: 0,
        }
    }

    /// Generate report
    pub fn generate_report(&mut self, metadata: ReportMetadata) -> bool {
        for slot in &mut self.reports {
            if slot.is_none() {
                *slot = Some(metadata);
                self.total_reports += 1;
                return true;
            }
        }
        false
    }

    /// Get report count
    pub fn report_count(&self) -> u32 {
        self.reports.iter().filter(|r| r.is_some()).count() as u32
    }

    /// Get reports by format
    pub fn reports_by_format(&self, format: ReportFormat) -> u32 {
        self.reports
            .iter()
            .filter(|r| r.map(|report| report.format == format).unwrap_or(false))
            .count() as u32
    }

    /// Get latest report
    pub fn latest_report(&self) -> Option<ReportMetadata> {
        self.reports
            .iter()
            .filter_map(|r| *r)
            .max_by_key(|r| r.id)
    }

    /// Get total reports generated
    pub fn total_reports_generated(&self) -> u32 {
        self.total_reports
    }

    /// Register template
    pub fn register_template(&mut self, template_hash: u32) -> bool {
        for slot in &mut self.templates_hash {
            if *slot == 0 {
                *slot = template_hash;
                return true;
            }
        }
        false
    }

    /// Get template count
    pub fn template_count(&self) -> u32 {
        self.templates_hash.iter().filter(|h| **h != 0).count() as u32
    }

    /// Clear old reports
    pub fn clear_before(&mut self, timestamp_ms: u64) -> u32 {
        let mut cleared = 0u32;

        for slot in &mut self.reports {
            if let Some(r) = slot {
                if r.generated_ms < timestamp_ms {
                    cleared += 1;
                    *slot = None;
                }
            }
        }

        cleared
    }

    /// Get average report size
    pub fn avg_report_size(&self) -> u32 {
        let mut total = 0u64;
        let mut count = 0u32;

        for report in &self.reports {
            if let Some(r) = report {
                total += r.size_bytes as u64;
                count += 1;
            }
        }

        if count == 0 {
            0
        } else {
            (total / count as u64) as u32
        }
    }

    /// Statistics
    pub fn statistics(&self) -> (u32, u32, u32, u32, u32) {
        (
            self.total_reports,
            self.report_count(),
            self.template_count(),
            self.avg_report_size(),
            self.reports_by_format(ReportFormat::Html),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_format_enum() {
        assert_eq!(ReportFormat::Text as u8, 0);
        assert_eq!(ReportFormat::Json as u8, 2);
    }

    #[test]
    fn test_report_format_extension() {
        assert_eq!(ReportFormat::Text.extension(), ".txt");
        assert_eq!(ReportFormat::Html.extension(), ".html");
        assert_eq!(ReportFormat::Json.extension(), ".json");
    }

    #[test]
    fn test_report_format_mime() {
        assert_eq!(ReportFormat::Text.mime_type(), "text/plain");
        assert_eq!(ReportFormat::Html.mime_type(), "text/html");
    }

    #[test]
    fn test_report_section_enum() {
        assert_eq!(ReportSection::Summary as u8, 0);
        assert_eq!(ReportSection::Recommendations as u8, 5);
    }

    #[test]
    fn test_report_section_name() {
        assert_eq!(ReportSection::Summary.name(), "Summary");
        assert_eq!(ReportSection::Alerts.name(), "Alerts");
    }

    #[test]
    fn test_metric_summary_creation() {
        let metric = MetricSummary::new(1, 0x1234, 100);
        assert_eq!(metric.id, 1);
        assert_eq!(metric.current_value, 100);
    }

    #[test]
    fn test_metric_summary_with_bounds() {
        let metric = MetricSummary::new(1, 0x1234, 100)
            .with_bounds(50, 200);
        assert_eq!(metric.min_value, 50);
        assert_eq!(metric.max_value, 200);
    }

    #[test]
    fn test_metric_summary_with_change() {
        let metric = MetricSummary::new(1, 0x1234, 100)
            .with_change(20);
        assert_eq!(metric.change_percent, 20);
    }

    #[test]
    fn test_chart_point_creation() {
        let point = ChartPoint::new(10, 50);
        assert_eq!(point.x, 10);
        assert_eq!(point.y, 50);
    }

    #[test]
    fn test_chart_creation() {
        let chart = Chart::new(1, 0x5678);
        assert_eq!(chart.id, 1);
        assert_eq!(chart.point_count(), 0);
    }

    #[test]
    fn test_chart_add_point() {
        let mut chart = Chart::new(1, 0x5678);
        let point = ChartPoint::new(10, 50);
        assert!(chart.add_point(point));
        assert_eq!(chart.point_count(), 1);
    }

    #[test]
    fn test_chart_min_max() {
        let mut chart = Chart::new(1, 0x5678);
        chart.add_point(ChartPoint::new(10, 50));
        chart.add_point(ChartPoint::new(20, 100));
        chart.add_point(ChartPoint::new(30, 75));

        assert_eq!(chart.min_y, 50);
        assert_eq!(chart.max_y, 100);
    }

    #[test]
    fn test_chart_y_range() {
        let mut chart = Chart::new(1, 0x5678);
        chart.add_point(ChartPoint::new(10, 50));
        chart.add_point(ChartPoint::new(20, 150));
        assert_eq!(chart.y_range(), 100);
    }

    #[test]
    fn test_evolution_cycle_summary_creation() {
        let cycle = EvolutionCycleSummary::new(1, 5);
        assert_eq!(cycle.id, 1);
        assert_eq!(cycle.cycle_number, 5);
    }

    #[test]
    fn test_evolution_cycle_efficiency_score() {
        let mut cycle = EvolutionCycleSummary::new(1, 5);
        cycle.mutations_tested = 100;
        cycle.improvements = 30;
        assert_eq!(cycle.efficiency_score(), 30);
    }

    #[test]
    fn test_evolution_cycle_efficiency_zero() {
        let cycle = EvolutionCycleSummary::new(1, 5);
        assert_eq!(cycle.efficiency_score(), 0);
    }

    #[test]
    fn test_report_metadata_creation() {
        let metadata = ReportMetadata::new(1, ReportFormat::Html, 1000);
        assert_eq!(metadata.id, 1);
        assert_eq!(metadata.format, ReportFormat::Html);
    }

    #[test]
    fn test_report_metadata_with_period() {
        let metadata = ReportMetadata::new(1, ReportFormat::Html, 1000)
            .with_period(500, 2000);
        assert_eq!(metadata.period_start_ms, 500);
        assert_eq!(metadata.period_end_ms, 2000);
    }

    #[test]
    fn test_report_metadata_with_size() {
        let metadata = ReportMetadata::new(1, ReportFormat::Html, 1000)
            .with_size(5000);
        assert_eq!(metadata.size_bytes, 5000);
    }

    #[test]
    fn test_report_builder_creation() {
        let builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        assert_eq!(builder.metric_count(), 0);
        assert_eq!(builder.chart_count(), 0);
    }

    #[test]
    fn test_report_builder_add_metric() {
        let mut builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        let metric = MetricSummary::new(1, 0x1234, 100);
        assert!(builder.add_metric(metric));
        assert_eq!(builder.metric_count(), 1);
    }

    #[test]
    fn test_report_builder_add_chart() {
        let mut builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        let chart = Chart::new(1, 0x5678);
        assert!(builder.add_chart(chart));
        assert_eq!(builder.chart_count(), 1);
    }

    #[test]
    fn test_report_builder_add_cycle() {
        let mut builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        let cycle = EvolutionCycleSummary::new(1, 5);
        assert!(builder.add_cycle(cycle));
        assert_eq!(builder.cycle_count(), 1);
    }

    #[test]
    fn test_report_builder_include_section() {
        let mut builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        builder.include_section(ReportSection::Summary);
        builder.include_section(ReportSection::Metrics);
        assert!(builder.has_section(ReportSection::Summary));
        assert!(builder.has_section(ReportSection::Metrics));
        assert!(!builder.has_section(ReportSection::Alerts));
    }

    #[test]
    fn test_report_builder_section_count() {
        let mut builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        builder.include_section(ReportSection::Summary);
        builder.include_section(ReportSection::Metrics);
        builder.include_section(ReportSection::Alerts);
        assert_eq!(builder.section_count(), 3);
    }

    #[test]
    fn test_report_builder_estimate_size() {
        let mut builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        builder.add_metric(MetricSummary::new(1, 0x1234, 100));
        builder.add_chart(Chart::new(1, 0x5678));

        let size = builder.estimate_size();
        assert!(size > 1000);  // Base size + metric + chart
    }

    #[test]
    fn test_report_builder_build() {
        let mut builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        builder.add_metric(MetricSummary::new(1, 0x1234, 100));
        
        let size = builder.build();
        assert!(size > 0);
        assert_eq!(builder.metadata().size_bytes, size);
    }

    #[test]
    fn test_reporting_system_creation() {
        let system = ReportingSystem::new();
        assert_eq!(system.total_reports, 0);
        assert_eq!(system.report_count(), 0);
    }

    #[test]
    fn test_reporting_system_generate_report() {
        let mut system = ReportingSystem::new();
        let metadata = ReportMetadata::new(1, ReportFormat::Html, 1000);
        assert!(system.generate_report(metadata));
        assert_eq!(system.report_count(), 1);
    }

    #[test]
    fn test_reporting_system_reports_by_format() {
        let mut system = ReportingSystem::new();
        system.generate_report(ReportMetadata::new(1, ReportFormat::Html, 1000));
        system.generate_report(ReportMetadata::new(2, ReportFormat::Text, 1000));
        system.generate_report(ReportMetadata::new(3, ReportFormat::Html, 1000));

        assert_eq!(system.reports_by_format(ReportFormat::Html), 2);
        assert_eq!(system.reports_by_format(ReportFormat::Text), 1);
    }

    #[test]
    fn test_reporting_system_latest_report() {
        let mut system = ReportingSystem::new();
        system.generate_report(ReportMetadata::new(1, ReportFormat::Html, 1000));
        system.generate_report(ReportMetadata::new(3, ReportFormat::Html, 2000));
        system.generate_report(ReportMetadata::new(2, ReportFormat::Html, 1500));

        let latest = system.latest_report();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().id, 3);
    }

    #[test]
    fn test_reporting_system_register_template() {
        let mut system = ReportingSystem::new();
        assert!(system.register_template(0x1111));
        assert!(system.register_template(0x2222));
        assert_eq!(system.template_count(), 2);
    }

    #[test]
    fn test_reporting_system_clear_before() {
        let mut system = ReportingSystem::new();
        system.generate_report(ReportMetadata::new(1, ReportFormat::Html, 1000));
        system.generate_report(ReportMetadata::new(2, ReportFormat::Html, 2000));
        system.generate_report(ReportMetadata::new(3, ReportFormat::Html, 3000));

        let cleared = system.clear_before(2500);
        assert_eq!(cleared, 2);
    }

    #[test]
    fn test_reporting_system_avg_report_size() {
        let mut system = ReportingSystem::new();
        system.generate_report(ReportMetadata::new(1, ReportFormat::Html, 1000).with_size(1000));
        system.generate_report(ReportMetadata::new(2, ReportFormat::Html, 1000).with_size(3000));

        let avg = system.avg_report_size();
        assert_eq!(avg, 2000);
    }

    #[test]
    fn test_reporting_system_statistics() {
        let mut system = ReportingSystem::new();
        system.generate_report(ReportMetadata::new(1, ReportFormat::Html, 1000).with_size(5000));
        system.generate_report(ReportMetadata::new(2, ReportFormat::Text, 1000).with_size(2000));
        system.register_template(0x1111);
        system.register_template(0x2222);

        let (total, count, templates, avg_size, html_count) = system.statistics();
        assert_eq!(total, 2);
        assert_eq!(count, 2);
        assert_eq!(templates, 2);
        assert_eq!(avg_size, 3500);
        assert_eq!(html_count, 1);
    }

    #[test]
    fn test_reporting_system_max_reports() {
        let mut system = ReportingSystem::new();
        for i in 0..50 {
            let metadata = ReportMetadata::new(i, ReportFormat::Html, 1000);
            assert!(system.generate_report(metadata));
        }
        // 51st should fail
        let metadata = ReportMetadata::new(50, ReportFormat::Html, 1000);
        assert!(!system.generate_report(metadata));
    }

    #[test]
    fn test_metric_summary_with_average() {
        let metric = MetricSummary::new(1, 0x1234, 100)
            .with_average(90);
        assert_eq!(metric.average_value, 90);
    }

    #[test]
    fn test_report_builder_set_notes() {
        let mut builder = ReportBuilder::new(1, ReportFormat::Html, 1000);
        builder.set_notes(0xABCD);
        assert_eq!(builder.notes_hash, 0xABCD);
    }

    #[test]
    fn test_chart_max_points() {
        let mut chart = Chart::new(1, 0x5678);
        for i in 0..100 {
            assert!(chart.add_point(ChartPoint::new(i, 50 + i)));
        }
        // 101st should fail
        assert!(!chart.add_point(ChartPoint::new(100, 150)));
    }
}
