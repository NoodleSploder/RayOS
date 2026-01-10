//! Advanced Observability for Ouroboros Engine
//!
//! Metrics collection, tracing, and statistical analysis of evolution cycles.
//! Provides visibility into mutation effectiveness and system behavior.
//!
//! Phase 32, Task 4


/// Key performance indicators for evolution cycles
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionKpi {
    /// Average fitness improvement (scaled by 1000)
    pub avg_fitness_improvement: u32,
    /// Mutation acceptance rate (percent)
    pub acceptance_rate: u32,
    /// Test suite execution time (ms)
    pub test_duration_avg: u32,
    /// Patch application latency (us)
    pub patch_latency_avg: u32,
    /// Memory usage per cycle (KB)
    pub memory_per_cycle: u32,
}

impl EvolutionKpi {
    /// Create default KPIs
    pub const fn new() -> Self {
        EvolutionKpi {
            avg_fitness_improvement: 0,
            acceptance_rate: 0,
            test_duration_avg: 0,
            patch_latency_avg: 0,
            memory_per_cycle: 0,
        }
    }
}

/// Percentile statistics for performance analysis
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Percentiles {
    /// 50th percentile (median)
    pub p50: u32,
    /// 95th percentile
    pub p95: u32,
    /// 99th percentile
    pub p99: u32,
}

impl Percentiles {
    /// Create new percentiles
    pub const fn new(p50: u32, p95: u32, p99: u32) -> Self {
        Percentiles { p50, p95, p99 }
    }
}

/// Time-series metrics collector
pub struct MetricsCollector {
    /// Fitness scores ring buffer (last 64 cycles)
    fitness_scores: [u32; 64],
    /// Test durations ring buffer (ms)
    test_durations: [u32; 64],
    /// Patch latencies ring buffer (us)
    patch_latencies: [u32; 64],
    /// Memory usages ring buffer (KB)
    memory_usages: [u32; 64],
    /// Write position in ring buffer
    write_pos: usize,
    /// Number of entries collected
    entry_count: usize,
}

impl MetricsCollector {
    /// Create new metrics collector
    pub const fn new() -> Self {
        MetricsCollector {
            fitness_scores: [0u32; 64],
            test_durations: [0u32; 64],
            patch_latencies: [0u32; 64],
            memory_usages: [0u32; 64],
            write_pos: 0,
            entry_count: 0,
        }
    }

    /// Record metrics for a cycle
    pub fn record_cycle(
        &mut self,
        fitness: u32,
        test_duration: u32,
        patch_latency: u32,
        memory: u32,
    ) {
        self.fitness_scores[self.write_pos] = fitness;
        self.test_durations[self.write_pos] = test_duration;
        self.patch_latencies[self.write_pos] = patch_latency;
        self.memory_usages[self.write_pos] = memory;

        self.write_pos = (self.write_pos + 1) % 64;
        if self.entry_count < 64 {
            self.entry_count += 1;
        }
    }

    /// Calculate average fitness improvement
    pub fn avg_fitness(&self) -> u32 {
        if self.entry_count == 0 {
            return 0;
        }
        let mut sum = 0u64;
        for i in 0..self.entry_count {
            sum += self.fitness_scores[i] as u64;
        }
        (sum / self.entry_count as u64) as u32
    }

    /// Calculate fitness percentiles
    pub fn fitness_percentiles(&self) -> Percentiles {
        if self.entry_count == 0 {
            return Percentiles::new(0, 0, 0);
        }

        let mut sorted = [0u32; 64];
        for i in 0..self.entry_count {
            sorted[i] = self.fitness_scores[i];
        }

        // Simple bubble sort for small arrays
        for i in 0..self.entry_count {
            for j in i + 1..self.entry_count {
                if sorted[i] > sorted[j] {
                    let tmp = sorted[i];
                    sorted[i] = sorted[j];
                    sorted[j] = tmp;
                }
            }
        }

        let p50_idx = (self.entry_count / 2).max(1) - 1;
        let p95_idx = ((self.entry_count * 95) / 100).max(1) - 1;
        let p99_idx = ((self.entry_count * 99) / 100).max(1) - 1;

        Percentiles::new(sorted[p50_idx], sorted[p95_idx], sorted[p99_idx])
    }

    /// Get entry count
    pub fn len(&self) -> usize {
        self.entry_count
    }

    /// Clear metrics
    pub fn clear(&mut self) {
        self.fitness_scores = [0u32; 64];
        self.test_durations = [0u32; 64];
        self.patch_latencies = [0u32; 64];
        self.memory_usages = [0u32; 64];
        self.write_pos = 0;
        self.entry_count = 0;
    }
}

/// Detailed trace of mutation execution
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceEntry {
    /// Mutation ID
    pub mutation_id: u32,
    /// Pre-mutation performance baseline
    pub pre_performance: u32,
    /// Post-mutation performance
    pub post_performance: u32,
    /// Memory delta (bytes)
    pub memory_delta: i32,
    /// Execution duration (ms)
    pub duration_ms: u32,
    /// Whether accepted
    pub accepted: bool,
}

impl TraceEntry {
    /// Create new trace entry
    pub const fn new(mutation_id: u32) -> Self {
        TraceEntry {
            mutation_id,
            pre_performance: 0,
            post_performance: 0,
            memory_delta: 0,
            duration_ms: 0,
            accepted: false,
        }
    }

    /// Calculate performance improvement
    pub fn improvement(&self) -> i32 {
        (self.post_performance as i32) - (self.pre_performance as i32)
    }
}

/// Trace buffer for detailed execution history
pub struct TraceBuffer {
    /// Trace entries ring buffer
    entries: [Option<TraceEntry>; 256],
    /// Write position
    write_pos: usize,
    /// Entry count
    count: usize,
}

impl TraceBuffer {
    /// Create new trace buffer
    pub const fn new() -> Self {
        TraceBuffer {
            entries: [None; 256],
            write_pos: 0,
            count: 0,
        }
    }

    /// Record trace entry
    pub fn record(&mut self, entry: TraceEntry) {
        self.entries[self.write_pos] = Some(entry);
        self.write_pos = (self.write_pos + 1) % 256;
        if self.count < 256 {
            self.count += 1;
        }
    }

    /// Get entry at index
    pub fn get(&self, index: usize) -> Option<TraceEntry> {
        if index >= self.count {
            return None;
        }
        let actual_pos = (self.write_pos + 256 - self.count + index) % 256;
        self.entries[actual_pos]
    }

    /// Count accepted mutations
    pub fn count_accepted(&self) -> usize {
        let mut count = 0;
        for i in 0..self.count {
            if let Some(entry) = self.get(i) {
                if entry.accepted {
                    count += 1;
                }
            }
        }
        count
    }

    /// Get average improvement
    pub fn avg_improvement(&self) -> i32 {
        if self.count == 0 {
            return 0;
        }
        let mut sum = 0i64;
        for i in 0..self.count {
            if let Some(entry) = self.get(i) {
                sum += entry.improvement() as i64;
            }
        }
        (sum / self.count as i64) as i32
    }

    /// Get length
    pub fn len(&self) -> usize {
        self.count
    }

    /// Clear buffer
    pub fn clear(&mut self) {
        self.entries = [None; 256];
        self.write_pos = 0;
        self.count = 0;
    }
}

/// Performance profiler for component analysis
pub struct PerformanceProfiler {
    /// Genome parsing time (us)
    genome_parse_time: u32,
    /// Mutation generation time (us)
    mutation_gen_time: u32,
    /// Test execution time (us)
    test_exec_time: u32,
    /// Patch application time (us)
    patch_apply_time: u32,
    /// Total evolution time (us)
    total_time: u32,
    /// Sample count
    samples: u32,
}

impl PerformanceProfiler {
    /// Create new profiler
    pub const fn new() -> Self {
        PerformanceProfiler {
            genome_parse_time: 0,
            mutation_gen_time: 0,
            test_exec_time: 0,
            patch_apply_time: 0,
            total_time: 0,
            samples: 0,
        }
    }

    /// Record component timing
    pub fn record_timing(
        &mut self,
        parse: u32,
        mutation: u32,
        test: u32,
        patch: u32,
    ) {
        // Accumulate with overflow protection
        self.genome_parse_time = self.genome_parse_time.saturating_add(parse);
        self.mutation_gen_time = self.mutation_gen_time.saturating_add(mutation);
        self.test_exec_time = self.test_exec_time.saturating_add(test);
        self.patch_apply_time = self.patch_apply_time.saturating_add(patch);
        self.total_time = self.total_time.saturating_add(parse + mutation + test + patch);
        self.samples = self.samples.saturating_add(1);
    }

    /// Get component time percentage
    pub fn parse_percent(&self) -> u32 {
        if self.total_time == 0 {
            return 0;
        }
        (self.genome_parse_time as u64 * 100 / self.total_time as u64) as u32
    }

    pub fn mutation_percent(&self) -> u32 {
        if self.total_time == 0 {
            return 0;
        }
        (self.mutation_gen_time as u64 * 100 / self.total_time as u64) as u32
    }

    pub fn test_percent(&self) -> u32 {
        if self.total_time == 0 {
            return 0;
        }
        (self.test_exec_time as u64 * 100 / self.total_time as u64) as u32
    }

    pub fn patch_percent(&self) -> u32 {
        if self.total_time == 0 {
            return 0;
        }
        (self.patch_apply_time as u64 * 100 / self.total_time as u64) as u32
    }

    /// Get average time per sample
    pub fn avg_total_time(&self) -> u32 {
        if self.samples == 0 {
            return 0;
        }
        self.total_time / self.samples
    }

    /// Identify bottleneck (slowest component)
    pub fn bottleneck(&self) -> ComponentBottleneck {
        let parse_pct = self.parse_percent();
        let mutation_pct = self.mutation_percent();
        let test_pct = self.test_percent();
        let patch_pct = self.patch_percent();

        if parse_pct >= mutation_pct && parse_pct >= test_pct && parse_pct >= patch_pct {
            ComponentBottleneck::GenomeParsing
        } else if mutation_pct >= test_pct && mutation_pct >= patch_pct {
            ComponentBottleneck::MutationGeneration
        } else if test_pct >= patch_pct {
            ComponentBottleneck::TestExecution
        } else {
            ComponentBottleneck::PatchApplication
        }
    }

    /// Reset profiler
    pub fn reset(&mut self) {
        self.genome_parse_time = 0;
        self.mutation_gen_time = 0;
        self.test_exec_time = 0;
        self.patch_apply_time = 0;
        self.total_time = 0;
        self.samples = 0;
    }
}

/// Identified bottleneck component
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ComponentBottleneck {
    GenomeParsing = 1,
    MutationGeneration = 2,
    TestExecution = 3,
    PatchApplication = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evolution_kpi_creation() {
        let kpi = EvolutionKpi::new();
        assert_eq!(kpi.avg_fitness_improvement, 0);
        assert_eq!(kpi.acceptance_rate, 0);
    }

    #[test]
    fn test_percentiles_creation() {
        let p = Percentiles::new(50, 95, 99);
        assert_eq!(p.p50, 50);
        assert_eq!(p.p95, 95);
        assert_eq!(p.p99, 99);
    }

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.len(), 0);
        assert_eq!(collector.avg_fitness(), 0);
    }

    #[test]
    fn test_metrics_collector_record() {
        let mut collector = MetricsCollector::new();
        collector.record_cycle(100, 50, 100, 512);

        assert_eq!(collector.len(), 1);
        assert_eq!(collector.avg_fitness(), 100);
    }

    #[test]
    fn test_metrics_collector_multiple_records() {
        let mut collector = MetricsCollector::new();
        for i in 0..10 {
            collector.record_cycle(100 + i * 10, 50 + i, 100 + i, 512);
        }

        assert_eq!(collector.len(), 10);
        assert!(collector.avg_fitness() > 100);
    }

    #[test]
    fn test_metrics_collector_percentiles() {
        let mut collector = MetricsCollector::new();
        for i in 0..10 {
            collector.record_cycle(i * 10, 50, 100, 512);
        }

        let p = collector.fitness_percentiles();
        assert!(p.p50 > 0);
        assert!(p.p95 >= p.p50);
        assert!(p.p99 >= p.p95);
    }

    #[test]
    fn test_metrics_collector_clear() {
        let mut collector = MetricsCollector::new();
        collector.record_cycle(100, 50, 100, 512);
        assert_eq!(collector.len(), 1);

        collector.clear();
        assert_eq!(collector.len(), 0);
        assert_eq!(collector.avg_fitness(), 0);
    }

    #[test]
    fn test_trace_entry_creation() {
        let entry = TraceEntry::new(42);
        assert_eq!(entry.mutation_id, 42);
        assert!(!entry.accepted);
    }

    #[test]
    fn test_trace_entry_improvement() {
        let mut entry = TraceEntry::new(1);
        entry.pre_performance = 100;
        entry.post_performance = 150;

        assert_eq!(entry.improvement(), 50);
    }

    #[test]
    fn test_trace_buffer_creation() {
        let buffer = TraceBuffer::new();
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_trace_buffer_record() {
        let mut buffer = TraceBuffer::new();
        let entry = TraceEntry::new(1);
        buffer.record(entry);

        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.get(0).unwrap().mutation_id, 1);
    }

    #[test]
    fn test_trace_buffer_count_accepted() {
        let mut buffer = TraceBuffer::new();
        let mut entry1 = TraceEntry::new(1);
        entry1.accepted = true;
        let entry2 = TraceEntry::new(2);

        buffer.record(entry1);
        buffer.record(entry2);

        assert_eq!(buffer.count_accepted(), 1);
    }

    #[test]
    fn test_trace_buffer_avg_improvement() {
        let mut buffer = TraceBuffer::new();
        let mut entry1 = TraceEntry::new(1);
        entry1.pre_performance = 100;
        entry1.post_performance = 150;

        let mut entry2 = TraceEntry::new(2);
        entry2.pre_performance = 100;
        entry2.post_performance = 120;

        buffer.record(entry1);
        buffer.record(entry2);

        let avg = buffer.avg_improvement();
        assert_eq!(avg, 35); // (50 + 20) / 2
    }

    #[test]
    fn test_trace_buffer_wraparound() {
        let mut buffer = TraceBuffer::new();
        for i in 0..300 {
            let entry = TraceEntry::new(i);
            buffer.record(entry);
        }

        assert_eq!(buffer.len(), 256); // capped at 256
    }

    #[test]
    fn test_performance_profiler_creation() {
        let profiler = PerformanceProfiler::new();
        assert_eq!(profiler.samples, 0);
        assert_eq!(profiler.avg_total_time(), 0);
    }

    #[test]
    fn test_performance_profiler_record_timing() {
        let mut profiler = PerformanceProfiler::new();
        profiler.record_timing(100, 200, 300, 50);

        assert_eq!(profiler.samples, 1);
        assert_eq!(profiler.avg_total_time(), 650);
    }

    #[test]
    fn test_performance_profiler_percentages() {
        let mut profiler = PerformanceProfiler::new();
        profiler.record_timing(100, 200, 300, 400);

        // 100 + 200 + 300 + 400 = 1000 total
        // parse = 100/1000 = 10%
        assert_eq!(profiler.parse_percent(), 10);
        // mutation = 200/1000 = 20%
        assert_eq!(profiler.mutation_percent(), 20);
        // test = 300/1000 = 30%
        assert_eq!(profiler.test_percent(), 30);
        // patch = 400/1000 = 40%
        assert_eq!(profiler.patch_percent(), 40);
    }

    #[test]
    fn test_performance_profiler_bottleneck() {
        let mut profiler = PerformanceProfiler::new();
        profiler.record_timing(100, 200, 500, 100); // test is slowest

        assert_eq!(profiler.bottleneck(), ComponentBottleneck::TestExecution);
    }

    #[test]
    fn test_performance_profiler_bottleneck_parsing() {
        let mut profiler = PerformanceProfiler::new();
        profiler.record_timing(500, 100, 100, 100); // parse is slowest

        assert_eq!(profiler.bottleneck(), ComponentBottleneck::GenomeParsing);
    }

    #[test]
    fn test_performance_profiler_reset() {
        let mut profiler = PerformanceProfiler::new();
        profiler.record_timing(100, 200, 300, 400);
        assert_eq!(profiler.samples, 1);

        profiler.reset();
        assert_eq!(profiler.samples, 0);
        assert_eq!(profiler.total_time, 0);
    }

    #[test]
    fn test_performance_profiler_multiple_samples() {
        let mut profiler = PerformanceProfiler::new();
        for _ in 0..5 {
            profiler.record_timing(100, 100, 100, 100);
        }

        assert_eq!(profiler.samples, 5);
        assert_eq!(profiler.avg_total_time(), 400);
    }

    #[test]
    fn test_trace_buffer_improvement_negative() {
        let mut buffer = TraceBuffer::new();
        let mut entry = TraceEntry::new(1);
        entry.pre_performance = 150;
        entry.post_performance = 100;

        buffer.record(entry);
        assert_eq!(buffer.avg_improvement(), -50);
    }

    #[test]
    fn test_metrics_collector_wraparound() {
        let mut collector = MetricsCollector::new();
        for i in 0..100 {
            collector.record_cycle(100 + (i % 20), 50, 100, 512);
        }

        assert_eq!(collector.len(), 64); // capped at 64
    }

    #[test]
    fn test_observability_integration() {
        let mut metrics = MetricsCollector::new();
        let mut trace = TraceBuffer::new();
        let mut profiler = PerformanceProfiler::new();

        // Simulate cycle
        let mut entry = TraceEntry::new(1);
        entry.pre_performance = 100;
        entry.post_performance = 125;
        entry.duration_ms = 50;
        entry.accepted = true;

        trace.record(entry);
        metrics.record_cycle(125, 50, 100, 512);
        profiler.record_timing(10, 15, 20, 5);

        assert_eq!(trace.len(), 1);
        assert_eq!(metrics.len(), 1);
        assert_eq!(profiler.samples, 1);
    }
}
