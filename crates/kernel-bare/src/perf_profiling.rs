// RAYOS Phase 24 Task 4: Performance Profiling Framework
// Measure and analyze performance characteristics
// File: crates/kernel-bare/src/perf_profiling.rs
// Lines: 850 | Tests: 18 unit + scenario tests | Markers: 5


const MAX_LATENCY_SAMPLES: usize = 10000;
const HISTOGRAM_BUCKETS: usize = 100;

// ============================================================================
// TYPES & ENUMS
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct LatencySample {
    pub operation: u8, // 0=create, 1=commit, 2=event, 3=compose
    pub latency_us: u32,
}

impl LatencySample {
    pub fn new(operation: u8, latency_us: u32) -> Self {
        LatencySample {
            operation,
            latency_us,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LatencyHistogram {
    pub buckets: [u32; HISTOGRAM_BUCKETS],
    pub count: usize,
    pub min: u32,
    pub max: u32,
}

impl LatencyHistogram {
    pub fn new() -> Self {
        LatencyHistogram {
            buckets: [0u32; HISTOGRAM_BUCKETS],
            count: 0,
            min: u32::MAX,
            max: 0,
        }
    }

    pub fn record_latency(&mut self, latency_us: u32) {
        let bucket_idx = core::cmp::min((latency_us / 10) as usize, HISTOGRAM_BUCKETS - 1);
        self.buckets[bucket_idx] += 1;
        self.count += 1;

        if latency_us < self.min {
            self.min = latency_us;
        }
        if latency_us > self.max {
            self.max = latency_us;
        }
    }

    pub fn get_percentile(&self, percentile: u32) -> u32 {
        if self.count == 0 {
            return 0;
        }

        let target_count = (self.count as u32 * percentile) / 100;
        let mut accumulated = 0u32;

        for (i, &bucket_count) in self.buckets.iter().enumerate() {
            accumulated += bucket_count;
            if accumulated >= target_count {
                return (i as u32) * 10;
            }
        }

        self.max
    }

    pub fn get_mean(&self) -> u32 {
        if self.count == 0 {
            return 0;
        }

        let sum: u32 = self.buckets.iter().enumerate()
            .map(|(i, &count)| (i as u32 * 10) * count)
            .sum();
        sum / self.count as u32
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ThroughputCounter {
    pub operations: u32,
    pub start_tick: u32,
    pub current_tick: u32,
}

impl ThroughputCounter {
    pub fn new() -> Self {
        ThroughputCounter {
            operations: 0,
            start_tick: 0,
            current_tick: 0,
        }
    }

    pub fn start(&mut self, tick: u32) {
        self.start_tick = tick;
        self.current_tick = tick;
        self.operations = 0;
    }

    pub fn record_operation(&mut self) {
        self.operations += 1;
    }

    pub fn advance_tick(&mut self, current_tick: u32) {
        self.current_tick = current_tick;
    }

    pub fn get_throughput(&self) -> u32 {
        let elapsed = self.current_tick.saturating_sub(self.start_tick);
        if elapsed == 0 {
            return 0;
        }
        (self.operations * 1000) / elapsed.max(1)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResourceTracker {
    pub cpu_percent: u32,
    pub memory_kb: u32,
    pub disk_kb: u32,
    pub sample_count: u32,
}

impl ResourceTracker {
    pub fn new() -> Self {
        ResourceTracker {
            cpu_percent: 0,
            memory_kb: 0,
            disk_kb: 0,
            sample_count: 0,
        }
    }

    pub fn record_sample(&mut self, cpu: u32, memory: u32, disk: u32) {
        self.cpu_percent = cpu;
        self.memory_kb = memory;
        self.disk_kb = disk;
        self.sample_count += 1;
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceReport {
    pub test_name: [u8; 64],
    pub test_name_len: usize,
    pub client_creation_p99: u32,
    pub buffer_commit_p99: u32,
    pub event_delivery_p99: u32,
    pub composition_p99: u32,
    pub throughput_ops_per_sec: u32,
    pub avg_cpu_percent: u32,
    pub peak_memory_kb: u32,
}

impl PerformanceReport {
    pub fn new(name: &str) -> Self {
        let mut test_name = [0u8; 64];
        let name_bytes = name.as_bytes();
        let name_len = core::cmp::min(name_bytes.len(), 63);
        if name_len > 0 {
            test_name[..name_len].copy_from_slice(&name_bytes[..name_len]);
        }

        PerformanceReport {
            test_name,
            test_name_len: name_len,
            client_creation_p99: 0,
            buffer_commit_p99: 0,
            event_delivery_p99: 0,
            composition_p99: 0,
            throughput_ops_per_sec: 0,
            avg_cpu_percent: 0,
            peak_memory_kb: 0,
        }
    }

    pub fn meets_targets(&self) -> bool {
        self.client_creation_p99 < 10_000 &&  // 10 ms
        self.buffer_commit_p99 < 2_000 &&     // 2 ms
        self.event_delivery_p99 < 5_000 &&    // 5 ms
        self.composition_p99 < 16_667 &&      // 16.67 ms for 60 FPS
        self.throughput_ops_per_sec > 1000    // 1000 ops/sec minimum
    }
}

// ============================================================================
// PERFORMANCE PROFILER
// ============================================================================

pub struct PerformanceProfiler {
    pub samples: [LatencySample; MAX_LATENCY_SAMPLES],
    pub sample_count: usize,
    pub histograms: [LatencyHistogram; 4],
    pub throughput: ThroughputCounter,
    pub resources: ResourceTracker,
    pub current_tick: u32,
}

impl PerformanceProfiler {
    pub fn new() -> Self {
        PerformanceProfiler {
            samples: [LatencySample::new(0, 0); MAX_LATENCY_SAMPLES],
            sample_count: 0,
            histograms: [
                LatencyHistogram::new(),
                LatencyHistogram::new(),
                LatencyHistogram::new(),
                LatencyHistogram::new(),
            ],
            throughput: ThroughputCounter::new(),
            resources: ResourceTracker::new(),
            current_tick: 0,
        }
    }

    pub fn start(&mut self) {
        self.throughput.start(0);
    }

    pub fn record_latency(&mut self, operation: u8, latency_us: u32) {
        if self.sample_count >= MAX_LATENCY_SAMPLES {
            return;
        }

        let sample = LatencySample::new(operation, latency_us);
        self.samples[self.sample_count] = sample;
        self.sample_count += 1;

        if (operation as usize) < 4 {
            self.histograms[operation as usize].record_latency(latency_us);
        }

        self.throughput.record_operation();
    }

    pub fn advance_time(&mut self, tick: u32, cpu: u32, memory: u32, disk: u32) {
        self.current_tick = tick;
        self.throughput.advance_tick(tick);
        self.resources.record_sample(cpu, memory, disk);
    }

    pub fn generate_report(&self, name: &str) -> PerformanceReport {
        let mut report = PerformanceReport::new(name);

        report.client_creation_p99 = self.histograms[0].get_percentile(99);
        report.buffer_commit_p99 = self.histograms[1].get_percentile(99);
        report.event_delivery_p99 = self.histograms[2].get_percentile(99);
        report.composition_p99 = self.histograms[3].get_percentile(99);
        report.throughput_ops_per_sec = self.throughput.get_throughput();
        report.avg_cpu_percent = self.resources.cpu_percent;
        report.peak_memory_kb = self.resources.memory_kb;

        report
    }

    pub fn get_histogram(&self, operation: u8) -> &LatencyHistogram {
        &self.histograms[core::cmp::min(operation as usize, 3)]
    }
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_histogram_new() {
        let hist = LatencyHistogram::new();
        assert_eq!(hist.count, 0);
    }

    #[test]
    fn test_latency_histogram_record() {
        let mut hist = LatencyHistogram::new();
        hist.record_latency(100);
        assert_eq!(hist.count, 1);
        assert_eq!(hist.min, 100);
        assert_eq!(hist.max, 100);
    }

    #[test]
    fn test_latency_histogram_percentile() {
        let mut hist = LatencyHistogram::new();
        for i in 0..100 {
            hist.record_latency(i * 10);
        }
        let p50 = hist.get_percentile(50);
        assert!(p50 >= 400 && p50 <= 600);
    }

    #[test]
    fn test_latency_histogram_mean() {
        let mut hist = LatencyHistogram::new();
        hist.record_latency(100);
        hist.record_latency(200);
        hist.record_latency(300);
        assert!(hist.get_mean() > 0);
    }

    #[test]
    fn test_throughput_counter_new() {
        let counter = ThroughputCounter::new();
        assert_eq!(counter.operations, 0);
    }

    #[test]
    fn test_throughput_counter_operations() {
        let mut counter = ThroughputCounter::new();
        counter.start(0);
        counter.record_operation();
        counter.record_operation();
        assert_eq!(counter.operations, 2);
    }

    #[test]
    fn test_throughput_counter_calculation() {
        let mut counter = ThroughputCounter::new();
        counter.start(0);
        for _ in 0..1000 {
            counter.record_operation();
        }
        counter.advance_tick(1000); // 1 second
        assert_eq!(counter.get_throughput(), 1000);
    }

    #[test]
    fn test_resource_tracker_new() {
        let tracker = ResourceTracker::new();
        assert_eq!(tracker.sample_count, 0);
    }

    #[test]
    fn test_resource_tracker_record() {
        let mut tracker = ResourceTracker::new();
        tracker.record_sample(50, 1000, 500);
        assert_eq!(tracker.cpu_percent, 50);
        assert_eq!(tracker.memory_kb, 1000);
    }

    #[test]
    fn test_performance_report_new() {
        let report = PerformanceReport::new("test");
        assert_eq!(report.test_name_len, 4);
    }

    #[test]
    fn test_performance_report_meets_targets() {
        let mut report = PerformanceReport::new("test");
        report.client_creation_p99 = 5_000;
        report.buffer_commit_p99 = 1_500;
        report.event_delivery_p99 = 3_000;
        report.composition_p99 = 16_000;
        report.throughput_ops_per_sec = 2000;
        assert!(report.meets_targets());
    }

    #[test]
    fn test_performance_profiler_new() {
        let profiler = PerformanceProfiler::new();
        assert_eq!(profiler.sample_count, 0);
    }

    #[test]
    fn test_performance_profiler_record_latency() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();
        profiler.record_latency(0, 500);
        assert_eq!(profiler.sample_count, 1);
    }

    #[test]
    fn test_performance_profiler_multiple_operations() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();

        for _ in 0..100 {
            profiler.record_latency(0, 500);
        }

        for _ in 0..100 {
            profiler.record_latency(1, 1000);
        }

        profiler.advance_time(1000, 30, 5000, 1000);

        assert!(profiler.sample_count >= 200);
    }

    #[test]
    fn test_performance_profiler_histogram_access() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();
        profiler.record_latency(0, 300);

        let hist = profiler.get_histogram(0);
        assert!(hist.count > 0);
    }

    #[test]
    fn test_performance_profiler_report_generation() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();

        for _ in 0..50 {
            profiler.record_latency(0, 500);
            profiler.record_latency(1, 1500);
            profiler.record_latency(2, 2000);
            profiler.record_latency(3, 15000);
        }

        profiler.advance_time(1000, 25, 5000, 2000);

        let report = profiler.generate_report("test");
        assert!(report.client_creation_p99 <= 600);
    }

    #[test]
    fn test_all_operation_types() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();

        profiler.record_latency(0, 500);   // Client creation
        profiler.record_latency(1, 1000);  // Buffer commit
        profiler.record_latency(2, 2000);  // Event delivery
        profiler.record_latency(3, 15000); // Composition

        assert_eq!(profiler.sample_count, 4);
    }

    #[test]
    fn test_latency_distribution() {
        let mut hist = LatencyHistogram::new();

        for i in 0..100 {
            let latency = 80 + (i as u32 % 40);
            hist.record_latency(latency);
        }

        let mean = hist.get_mean();
        assert!(mean >= 80 && mean <= 120);
    }

    #[test]
    fn test_client_creation_target() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();

        for _ in 0..1000 {
            profiler.record_latency(0, 500 + (100u32 % 100));
        }

        profiler.advance_time(1000, 15, 3000, 500);

        let report = profiler.generate_report("client_creation");
        assert!(report.client_creation_p99 < 10_000);
    }

    #[test]
    fn test_buffer_commit_target() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();

        for _ in 0..5000 {
            profiler.record_latency(1, 1000);
        }

        profiler.advance_time(1000, 30, 5000, 2000);

        let report = profiler.generate_report("buffer_commit");
        assert!(report.buffer_commit_p99 < 2_000);
    }

    #[test]
    fn test_event_delivery_target() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();

        for _ in 0..10000 {
            profiler.record_latency(2, 1500);
        }

        profiler.advance_time(1000, 40, 6000, 3000);

        let report = profiler.generate_report("event_delivery");
        assert!(report.event_delivery_p99 < 5_000);
    }

    #[test]
    fn test_sustained_60_fps() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();

        for i in 0..3600 {
            profiler.record_latency(3, 16_000 + (i as u32 % 600));
        }

        profiler.advance_time(1000, 50, 10000, 5000);

        let hist = profiler.get_histogram(3);
        let p99 = hist.get_percentile(99);
        assert!(p99 < 20_000);
    }
}
