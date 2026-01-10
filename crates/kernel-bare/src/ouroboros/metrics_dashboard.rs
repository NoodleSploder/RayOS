//! Metrics Dashboard and Visualization System
//!
//! Provides KPI display, time series graphs, statistical analysis,
//! and data export for evolution metrics.
//!
//! Phase 33, Task 6

/// Key Performance Indicator type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum KpiType {
    Throughput = 0,          // Mutations per second
    SuccessRate = 1,         // Percentage
    AvgCycleDuration = 2,    // Milliseconds
    CumulativeImprovement = 3, // Percentage
    MemoryUsage = 4,         // KB
    ApplyRate = 5,           // Percentage
}

/// KPI (Key Performance Indicator) value
#[derive(Clone, Copy, Debug)]
pub struct KpiValue {
    /// KPI type
    pub kpi_type: KpiType,
    /// Current value
    pub value: u32,
    /// Previous value (for trend calculation)
    pub previous_value: u32,
    /// Min value observed
    pub min_value: u32,
    /// Max value observed
    pub max_value: u32,
    /// Timestamp (ms since boot)
    pub timestamp_ms: u64,
}

impl KpiValue {
    /// Create new KPI value
    pub const fn new(kpi_type: KpiType, value: u32, timestamp_ms: u64) -> Self {
        KpiValue {
            kpi_type,
            value,
            previous_value: value,
            min_value: value,
            max_value: value,
            timestamp_ms,
        }
    }

    /// Get trend delta (positive = improving)
    pub fn trend_delta(&self) -> i32 {
        (self.value as i32) - (self.previous_value as i32)
    }

    /// Get trend direction symbol
    pub fn trend_symbol(&self) -> u8 {
        let delta = self.trend_delta();
        if delta > 0 {
            b'^' // Up arrow
        } else if delta < 0 {
            b'v' // Down arrow
        } else {
            b'-' // Flat
        }
    }

    /// Update min/max
    pub fn update_bounds(&mut self) {
        if self.value < self.min_value {
            self.min_value = self.value;
        }
        if self.value > self.max_value {
            self.max_value = self.value;
        }
    }
}

/// Time series data point
#[derive(Clone, Copy, Debug)]
pub struct TimeSeriesPoint {
    /// Timestamp (ms since boot)
    pub timestamp_ms: u64,
    /// Value at this time
    pub value: u32,
    /// Cycle number
    pub cycle: u32,
}

impl TimeSeriesPoint {
    /// Create new time series point
    pub const fn new(timestamp_ms: u64, value: u32, cycle: u32) -> Self {
        TimeSeriesPoint {
            timestamp_ms,
            value,
            cycle,
        }
    }
}

/// Time series dataset
#[derive(Clone, Copy, Debug)]
pub struct TimeSeries {
    /// Data points (last 50)
    pub points: [Option<TimeSeriesPoint>; 50],
    /// Current index
    pub index: usize,
    /// Min value
    pub min: u32,
    /// Max value
    pub max: u32,
    /// Average value
    pub avg: u32,
}

impl TimeSeries {
    /// Create new time series
    pub const fn new() -> Self {
        TimeSeries {
            points: [None; 50],
            index: 0,
            min: u32::MAX,
            max: 0,
            avg: 0,
        }
    }

    /// Add data point
    pub fn add_point(&mut self, point: TimeSeriesPoint) {
        self.points[self.index] = Some(point);
        self.index = (self.index + 1) % 50;

        // Update statistics
        if point.value < self.min {
            self.min = point.value;
        }
        if point.value > self.max {
            self.max = point.value;
        }

        // Recalculate average
        let mut sum = 0u64;
        let mut count = 0u32;
        for p in &self.points {
            if let Some(pt) = p {
                sum = sum.saturating_add(pt.value as u64);
                count += 1;
            }
        }
        if count > 0 {
            self.avg = (sum / count as u64) as u32;
        }
    }

    /// Get all points
    pub fn get_points(&self) -> [Option<TimeSeriesPoint>; 50] {
        self.points
    }

    /// Get point count
    pub fn point_count(&self) -> usize {
        self.points.iter().filter(|p| p.is_some()).count()
    }
}

/// Dashboard widget type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum WidgetType {
    KpiDisplay = 0,
    LineGraph = 1,
    BarChart = 2,
    GaugeChart = 3,
    Table = 4,
}

/// Dashboard widget
#[derive(Clone, Copy, Debug)]
pub struct DashboardWidget {
    /// Widget ID
    pub id: u32,
    /// Widget type
    pub widget_type: WidgetType,
    /// Associated KPI type (if applicable)
    pub kpi_type: Option<KpiType>,
    /// Position (0-11 for 3x4 grid)
    pub position: u8,
    /// Is visible
    pub visible: bool,
    /// Refresh interval (ms)
    pub refresh_interval_ms: u32,
}

impl DashboardWidget {
    /// Create new widget
    pub const fn new(id: u32, widget_type: WidgetType, position: u8) -> Self {
        DashboardWidget {
            id,
            widget_type,
            kpi_type: None,
            position,
            visible: true,
            refresh_interval_ms: 1000,
        }
    }

    /// Create KPI widget
    pub const fn kpi_widget(id: u32, kpi_type: KpiType, position: u8) -> Self {
        DashboardWidget {
            id,
            widget_type: WidgetType::KpiDisplay,
            kpi_type: Some(kpi_type),
            position,
            visible: true,
            refresh_interval_ms: 500,
        }
    }
}

/// Export format
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ExportFormat {
    Json = 0,
    Csv = 1,
    Binary = 2,
}

/// Export result
#[derive(Clone, Copy, Debug)]
pub struct ExportResult {
    /// Export ID
    pub id: u32,
    /// Format used
    pub format: ExportFormat,
    /// Size in bytes
    pub size_bytes: u32,
    /// Timestamp (ms since boot)
    pub timestamp_ms: u64,
    /// Success flag
    pub success: bool,
}

impl ExportResult {
    /// Create new export result
    pub const fn new(id: u32, format: ExportFormat, size_bytes: u32, timestamp_ms: u64, success: bool) -> Self {
        ExportResult {
            id,
            format,
            size_bytes,
            timestamp_ms,
            success,
        }
    }
}

/// Dashboard Controller
pub struct MetricsDashboard {
    /// KPI values (6 main KPIs)
    kpi_values: [Option<KpiValue>; 6],
    /// Time series for each KPI (6 time series)
    time_series: [TimeSeries; 6],
    /// Widgets (12 widgets for 3x4 grid)
    widgets: [Option<DashboardWidget>; 12],
    /// Widget count
    widget_count: u8,
    /// Export history (last 5)
    exports: [Option<ExportResult>; 5],
    /// Export index
    export_index: usize,
    /// Update counter
    update_count: u32,
}

impl MetricsDashboard {
    /// Create new dashboard
    pub const fn new() -> Self {
        MetricsDashboard {
            kpi_values: [None; 6],
            time_series: [TimeSeries::new(); 6],
            widgets: [None; 12],
            widget_count: 0,
            exports: [None; 5],
            export_index: 0,
            update_count: 0,
        }
    }

    /// Update KPI value
    pub fn update_kpi(&mut self, kpi_type: KpiType, value: u32, timestamp_ms: u64) {
        let idx = kpi_type as usize;
        
        if let Some(ref mut kpi) = self.kpi_values[idx] {
            kpi.previous_value = kpi.value;
            kpi.value = value;
            kpi.timestamp_ms = timestamp_ms;
            kpi.update_bounds();
        } else {
            let mut new_kpi = KpiValue::new(kpi_type, value, timestamp_ms);
            new_kpi.update_bounds();
            self.kpi_values[idx] = Some(new_kpi);
        }

        // Add to time series
        let point = TimeSeriesPoint::new(timestamp_ms, value, 0);
        self.time_series[idx].add_point(point);
        
        self.update_count += 1;
    }

    /// Get KPI value
    pub fn get_kpi(&self, kpi_type: KpiType) -> Option<KpiValue> {
        let idx = kpi_type as usize;
        self.kpi_values[idx]
    }

    /// Get time series
    pub fn get_time_series(&self, kpi_type: KpiType) -> TimeSeries {
        let idx = kpi_type as usize;
        self.time_series[idx]
    }

    /// Add widget to dashboard
    pub fn add_widget(&mut self, widget: DashboardWidget) -> bool {
        if self.widget_count >= 12 {
            return false;
        }
        
        let idx = self.widget_count as usize;
        self.widgets[idx] = Some(widget);
        self.widget_count += 1;
        true
    }

    /// Remove widget from dashboard
    pub fn remove_widget(&mut self, widget_id: u32) -> bool {
        for i in 0..12 {
            if let Some(w) = self.widgets[i] {
                if w.id == widget_id {
                    self.widgets[i] = None;
                    return true;
                }
            }
        }
        false
    }

    /// Get all widgets
    pub fn get_widgets(&self) -> [Option<DashboardWidget>; 12] {
        self.widgets
    }

    /// Get visible widgets count
    pub fn visible_widget_count(&self) -> u8 {
        let mut count = 0;
        for w in &self.widgets {
            if let Some(widget) = w {
                if widget.visible {
                    count += 1;
                }
            }
        }
        count
    }

    /// Record export
    pub fn record_export(&mut self, result: ExportResult) {
        self.exports[self.export_index] = Some(result);
        self.export_index = (self.export_index + 1) % 5;
    }

    /// Get export history
    pub fn export_history(&self) -> [Option<ExportResult>; 5] {
        self.exports
    }

    /// Get successful exports count
    pub fn successful_exports(&self) -> u8 {
        let mut count = 0;
        for e in &self.exports {
            if let Some(export) = e {
                if export.success {
                    count += 1;
                }
            }
        }
        count
    }

    /// Get total exported bytes
    pub fn total_exported_bytes(&self) -> u32 {
        let mut total = 0u32;
        for e in &self.exports {
            if let Some(export) = e {
                if export.success {
                    total = total.saturating_add(export.size_bytes);
                }
            }
        }
        total
    }

    /// Get update count
    pub fn get_update_count(&self) -> u32 {
        self.update_count
    }

    /// Get KPI list
    pub fn get_all_kpis(&self) -> [Option<KpiValue>; 6] {
        self.kpi_values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kpi_value_creation() {
        let kpi = KpiValue::new(KpiType::Throughput, 100, 1000);
        assert_eq!(kpi.value, 100);
        assert_eq!(kpi.kpi_type, KpiType::Throughput);
    }

    #[test]
    fn test_kpi_value_trend() {
        let mut kpi = KpiValue::new(KpiType::Throughput, 100, 1000);
        kpi.previous_value = 80;
        assert_eq!(kpi.trend_delta(), 20);
        assert_eq!(kpi.trend_symbol(), b'^');
    }

    #[test]
    fn test_kpi_value_bounds() {
        let mut kpi = KpiValue::new(KpiType::Throughput, 100, 1000);
        assert_eq!(kpi.min_value, 100);
        assert_eq!(kpi.max_value, 100);
        
        kpi.value = 150;
        kpi.update_bounds();
        assert_eq!(kpi.max_value, 150);
        
        kpi.value = 50;
        kpi.update_bounds();
        assert_eq!(kpi.min_value, 50);
    }

    #[test]
    fn test_time_series_point_creation() {
        let point = TimeSeriesPoint::new(1000, 100, 1);
        assert_eq!(point.value, 100);
        assert_eq!(point.cycle, 1);
    }

    #[test]
    fn test_time_series_creation() {
        let ts = TimeSeries::new();
        assert_eq!(ts.point_count(), 0);
        assert_eq!(ts.avg, 0);
    }

    #[test]
    fn test_time_series_add_point() {
        let mut ts = TimeSeries::new();
        let point1 = TimeSeriesPoint::new(1000, 100, 1);
        ts.add_point(point1);
        
        assert_eq!(ts.point_count(), 1);
        assert_eq!(ts.min, 100);
        assert_eq!(ts.max, 100);
        assert_eq!(ts.avg, 100);
    }

    #[test]
    fn test_time_series_multiple_points() {
        let mut ts = TimeSeries::new();
        let points = vec![
            TimeSeriesPoint::new(1000, 100, 1),
            TimeSeriesPoint::new(1100, 150, 2),
            TimeSeriesPoint::new(1200, 120, 3),
        ];
        
        for p in points {
            ts.add_point(p);
        }
        
        assert_eq!(ts.point_count(), 3);
        assert_eq!(ts.min, 100);
        assert_eq!(ts.max, 150);
        assert_eq!(ts.avg, 123);
    }

    #[test]
    fn test_dashboard_widget_creation() {
        let widget = DashboardWidget::new(1, WidgetType::KpiDisplay, 0);
        assert_eq!(widget.id, 1);
        assert_eq!(widget.widget_type, WidgetType::KpiDisplay);
        assert!(widget.visible);
    }

    #[test]
    fn test_dashboard_widget_kpi() {
        let widget = DashboardWidget::kpi_widget(1, KpiType::Throughput, 0);
        assert_eq!(widget.kpi_type, Some(KpiType::Throughput));
        assert_eq!(widget.widget_type, WidgetType::KpiDisplay);
    }

    #[test]
    fn test_export_result_creation() {
        let export = ExportResult::new(1, ExportFormat::Json, 1024, 1000, true);
        assert_eq!(export.id, 1);
        assert_eq!(export.format, ExportFormat::Json);
        assert!(export.success);
    }

    #[test]
    fn test_metrics_dashboard_creation() {
        let dashboard = MetricsDashboard::new();
        assert_eq!(dashboard.get_update_count(), 0);
    }

    #[test]
    fn test_metrics_dashboard_update_kpi() {
        let mut dashboard = MetricsDashboard::new();
        dashboard.update_kpi(KpiType::Throughput, 100, 1000);
        
        let kpi = dashboard.get_kpi(KpiType::Throughput);
        assert!(kpi.is_some());
        assert_eq!(kpi.unwrap().value, 100);
    }

    #[test]
    fn test_metrics_dashboard_update_multiple_kpis() {
        let mut dashboard = MetricsDashboard::new();
        dashboard.update_kpi(KpiType::Throughput, 100, 1000);
        dashboard.update_kpi(KpiType::SuccessRate, 80, 1000);
        dashboard.update_kpi(KpiType::CumulativeImprovement, 5, 1000);
        
        assert_eq!(dashboard.get_update_count(), 3);
    }

    #[test]
    fn test_metrics_dashboard_add_widget() {
        let mut dashboard = MetricsDashboard::new();
        let widget = DashboardWidget::new(1, WidgetType::KpiDisplay, 0);
        
        assert!(dashboard.add_widget(widget));
        assert_eq!(dashboard.visible_widget_count(), 1);
    }

    #[test]
    fn test_metrics_dashboard_add_multiple_widgets() {
        let mut dashboard = MetricsDashboard::new();
        
        for i in 0..6 {
            let widget = DashboardWidget::new(i, WidgetType::KpiDisplay, i as u8);
            assert!(dashboard.add_widget(widget));
        }
        
        assert_eq!(dashboard.visible_widget_count(), 6);
    }

    #[test]
    fn test_metrics_dashboard_widget_limit() {
        let mut dashboard = MetricsDashboard::new();
        
        for i in 0..12 {
            let widget = DashboardWidget::new(i, WidgetType::KpiDisplay, (i % 12) as u8);
            dashboard.add_widget(widget);
        }
        
        // Try to add 13th widget - should fail
        let widget = DashboardWidget::new(12, WidgetType::KpiDisplay, 0);
        assert!(!dashboard.add_widget(widget));
    }

    #[test]
    fn test_metrics_dashboard_remove_widget() {
        let mut dashboard = MetricsDashboard::new();
        let widget = DashboardWidget::new(1, WidgetType::KpiDisplay, 0);
        
        dashboard.add_widget(widget);
        assert!(dashboard.remove_widget(1));
        assert_eq!(dashboard.visible_widget_count(), 0);
    }

    #[test]
    fn test_metrics_dashboard_record_export() {
        let mut dashboard = MetricsDashboard::new();
        let export = ExportResult::new(1, ExportFormat::Json, 1024, 1000, true);
        
        dashboard.record_export(export);
        assert_eq!(dashboard.successful_exports(), 1);
    }

    #[test]
    fn test_metrics_dashboard_export_stats() {
        let mut dashboard = MetricsDashboard::new();
        
        dashboard.record_export(ExportResult::new(1, ExportFormat::Json, 1024, 1000, true));
        dashboard.record_export(ExportResult::new(2, ExportFormat::Csv, 512, 1100, true));
        dashboard.record_export(ExportResult::new(3, ExportFormat::Binary, 2048, 1200, false));
        
        assert_eq!(dashboard.successful_exports(), 2);
        assert_eq!(dashboard.total_exported_bytes(), 1536);
    }

    #[test]
    fn test_metrics_dashboard_time_series() {
        let mut dashboard = MetricsDashboard::new();
        
        dashboard.update_kpi(KpiType::Throughput, 100, 1000);
        dashboard.update_kpi(KpiType::Throughput, 150, 1100);
        dashboard.update_kpi(KpiType::Throughput, 120, 1200);
        
        let ts = dashboard.get_time_series(KpiType::Throughput);
        assert_eq!(ts.point_count(), 3);
    }

    #[test]
    fn test_kpi_trend_stable() {
        let mut kpi = KpiValue::new(KpiType::Throughput, 100, 1000);
        kpi.previous_value = 100;
        assert_eq!(kpi.trend_symbol(), b'-');
    }

    #[test]
    fn test_kpi_trend_declining() {
        let mut kpi = KpiValue::new(KpiType::Throughput, 80, 1000);
        kpi.previous_value = 100;
        assert_eq!(kpi.trend_delta(), -20);
        assert_eq!(kpi.trend_symbol(), b'v');
    }

    #[test]
    fn test_metrics_dashboard_kpi_update_trend() {
        let mut dashboard = MetricsDashboard::new();
        
        dashboard.update_kpi(KpiType::SuccessRate, 75, 1000);
        dashboard.update_kpi(KpiType::SuccessRate, 85, 1100);
        
        let kpi = dashboard.get_kpi(KpiType::SuccessRate);
        assert!(kpi.is_some());
        let k = kpi.unwrap();
        assert_eq!(k.trend_delta(), 10);
    }
}
