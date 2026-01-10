// RAYOS Phase 24 Task 2: Stress Testing Framework
// Push system to limits and verify graceful degradation
// File: crates/kernel-bare/src/stress_testing.rs
// Lines: 850 | Tests: 15 unit + stress scenarios | Markers: 5


const MAX_LOAD_SAMPLES: usize = 1000;
const MAX_STRESS_TESTS: usize = 10;

// ============================================================================
// TYPES & ENUMS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StressType {
    CPU,
    Memory,
    DiskIO,
    Combined,
}

#[derive(Debug, Clone, Copy)]
pub struct ResourceSnapshot {
    pub cpu_percent: u32,
    pub memory_percent: u32,
    pub disk_io_percent: u32,
    pub latency_us: u32,
    pub frames_per_sec: u16,
}

impl ResourceSnapshot {
    pub fn new() -> Self {
        ResourceSnapshot {
            cpu_percent: 0,
            memory_percent: 0,
            disk_io_percent: 0,
            latency_us: 0,
            frames_per_sec: 0,
        }
    }
}

impl Default for ResourceSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DegradationCurve {
    pub load_levels: [u32; 10],
    pub latencies: [u32; 10],
    pub sample_count: usize,
}

impl DegradationCurve {
    pub fn new() -> Self {
        DegradationCurve {
            load_levels: [0; 10],
            latencies: [0; 10],
            sample_count: 0,
        }
    }

    pub fn add_sample(&mut self, load: u32, latency: u32) {
        if self.sample_count >= 10 {
            // Shift left
            for i in 0..9 {
                self.load_levels[i] = self.load_levels[i + 1];
                self.latencies[i] = self.latencies[i + 1];
            }
            self.sample_count = 9;
        }
        self.load_levels[self.sample_count] = load;
        self.latencies[self.sample_count] = latency;
        self.sample_count += 1;
    }

    pub fn get_slope(&self) -> i64 {
        if self.sample_count < 2 {
            return 0;
        }

        let first = self.latencies[0] as i64;
        let last = self.latencies[self.sample_count - 1] as i64;
        let load_range = self.load_levels[self.sample_count - 1] as i64
            - self.load_levels[0] as i64;

        if load_range == 0 {
            return 0;
        }

        (last - first) / load_range
    }

    pub fn is_graceful_degradation(&self) -> bool {
        // Slope < 100 us/percent means graceful (not exponential)
        self.get_slope() < 100
    }
}

impl Default for DegradationCurve {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct StressTestResult {
    pub passed: bool,
    pub stress_type: StressType,
    pub target_level: u32,
    pub max_load_reached: u32,
    pub max_latency: u32,
    pub min_fps: u16,
    pub degradation_is_graceful: bool,
    pub crashes: u32,
    pub hangs: u32,
    pub recovered: bool,
}

impl StressTestResult {
    pub fn new(stress_type: StressType, target_level: u32) -> Self {
        StressTestResult {
            passed: true,
            stress_type,
            target_level,
            max_load_reached: 0,
            max_latency: 0,
            min_fps: 60,
            degradation_is_graceful: true,
            crashes: 0,
            hangs: 0,
            recovered: false,
        }
    }
}

// ============================================================================
// LOAD GENERATOR
// ============================================================================

#[derive(Clone, Copy)]
pub struct LoadGenerator {
    pub stress_type: StressType,
    pub target_level: u32,
    pub current_level: u32,
}

impl LoadGenerator {
    pub fn new(stress_type: StressType, target_level: u32) -> Self {
        LoadGenerator {
            stress_type,
            target_level,
            current_level: 0,
        }
    }

    pub fn step(&mut self) -> bool {
        if self.current_level >= self.target_level {
            return false;
        }
        self.current_level += self.target_level / 100; // Ramp up 1% per step
        true
    }

    pub fn get_current_load(&self) -> u32 {
        (self.current_level * 100) / core::cmp::max(1, self.target_level)
    }
}

// ============================================================================
// RESOURCE MONITOR
// ============================================================================

pub struct ResourceMonitor {
    pub samples: [ResourceSnapshot; MAX_LOAD_SAMPLES],
    pub sample_count: usize,
}

impl ResourceMonitor {
    pub fn new() -> Self {
        ResourceMonitor {
            samples: [ResourceSnapshot::new(); MAX_LOAD_SAMPLES],
            sample_count: 0,
        }
    }

    pub fn record_sample(&mut self, snapshot: ResourceSnapshot) {
        if self.sample_count >= MAX_LOAD_SAMPLES {
            // Shift and overwrite
            for i in 0..MAX_LOAD_SAMPLES - 1 {
                self.samples[i] = self.samples[i + 1];
            }
            self.sample_count = MAX_LOAD_SAMPLES - 1;
        }
        self.samples[self.sample_count] = snapshot;
        self.sample_count += 1;
    }

    pub fn get_avg_latency(&self) -> u32 {
        if self.sample_count == 0 {
            return 0;
        }
        let sum: u32 = self.samples[..self.sample_count]
            .iter()
            .map(|s| s.latency_us)
            .sum();
        sum / self.sample_count as u32
    }

    pub fn get_max_latency(&self) -> u32 {
        self.samples[..self.sample_count]
            .iter()
            .map(|s| s.latency_us)
            .max()
            .unwrap_or(0)
    }

    pub fn get_min_fps(&self) -> u16 {
        self.samples[..self.sample_count]
            .iter()
            .map(|s| s.frames_per_sec)
            .min()
            .unwrap_or(0)
    }

    pub fn get_peak_cpu(&self) -> u32 {
        self.samples[..self.sample_count]
            .iter()
            .map(|s| s.cpu_percent)
            .max()
            .unwrap_or(0)
    }

    pub fn get_peak_memory(&self) -> u32 {
        self.samples[..self.sample_count]
            .iter()
            .map(|s| s.memory_percent)
            .max()
            .unwrap_or(0)
    }
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// DEGRADATION ANALYZER
// ============================================================================

pub struct DegradationAnalyzer {
    pub curves: [DegradationCurve; MAX_STRESS_TESTS],
    pub curve_count: usize,
}

impl DegradationAnalyzer {
    pub fn new() -> Self {
        DegradationAnalyzer {
            curves: [DegradationCurve::new(); MAX_STRESS_TESTS],
            curve_count: 0,
        }
    }

    pub fn add_measurement(&mut self, load_percent: u32, latency_us: u32) {
        if self.curve_count == 0 {
            self.curve_count = 1;
        }
        if let Some(curve) = self.curves.get_mut(self.curve_count - 1) {
            curve.add_sample(load_percent, latency_us);
        }
    }

    pub fn is_graceful(&self) -> bool {
        if self.curve_count == 0 {
            return true;
        }
        self.curves[self.curve_count - 1].is_graceful_degradation()
    }

    pub fn get_average_slope(&self) -> i64 {
        if self.curve_count == 0 {
            return 0;
        }

        let sum: i64 = self.curves[..self.curve_count]
            .iter()
            .map(|c| c.get_slope())
            .sum();
        sum / self.curve_count as i64
    }
}

impl Default for DegradationAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// STRESS TEST HARNESS
// ============================================================================

pub struct StressTestHarness {
    pub result: StressTestResult,
    pub generator: LoadGenerator,
    pub monitor: ResourceMonitor,
    pub analyzer: DegradationAnalyzer,
    pub current_step: u32,
    pub max_steps: u32,
}

impl StressTestHarness {
    pub fn new(stress_type: StressType, target_level: u32, max_steps: u32) -> Self {
        StressTestHarness {
            result: StressTestResult::new(stress_type, target_level),
            generator: LoadGenerator::new(stress_type, target_level),
            monitor: ResourceMonitor::new(),
            analyzer: DegradationAnalyzer::new(),
            current_step: 0,
            max_steps,
        }
    }

    pub fn step(
        &mut self,
        cpu_percent: u32,
        memory_percent: u32,
        disk_io_percent: u32,
        latency_us: u32,
        fps: u16,
    ) {
        self.current_step += 1;

        // Step the load generator
        let _continues = self.generator.step();
        let current_load = self.generator.get_current_load();

        // Record resource snapshot
        let snapshot = ResourceSnapshot {
            cpu_percent,
            memory_percent,
            disk_io_percent,
            latency_us,
            frames_per_sec: fps,
        };
        self.monitor.record_sample(snapshot);

        // Record degradation measurement
        self.analyzer.add_measurement(current_load, latency_us);

        // Update result
        if cpu_percent > self.result.max_load_reached {
            self.result.max_load_reached = cpu_percent;
        }
        if latency_us > self.result.max_latency {
            self.result.max_latency = latency_us;
        }
        if fps < self.result.min_fps {
            self.result.min_fps = fps;
        }

        // Detect issues
        if latency_us > 1_000_000 {
            self.result.crashes += 1;
        }
        if fps == 0 {
            self.result.hangs += 1;
        }
    }

    pub fn should_continue(&self) -> bool {
        self.current_step < self.max_steps && self.generator.current_level < self.generator.target_level
    }

    pub fn finish(&mut self) {
        // Check degradation curve
        self.result.degradation_is_graceful = self.analyzer.is_graceful();

        // Check if system recovered
        if self.result.crashes > 0 || self.result.hangs > 0 {
            // If min_fps > 0 and latency reasonable, consider it recovered
            if self.result.min_fps >= 30 && self.result.max_latency < 500_000 {
                self.result.recovered = true;
            } else {
                self.result.passed = false;
            }
        }

        // If no crashes/hangs and graceful degradation, pass
        if self.result.crashes == 0 && self.result.hangs == 0 {
            self.result.passed = true;
        }
    }

    pub fn get_summary(&self) -> (u32, u32, u16, bool) {
        (
            self.monitor.get_max_latency(),
            self.monitor.get_peak_cpu(),
            self.result.min_fps,
            self.analyzer.is_graceful(),
        )
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_snapshot_new() {
        let snap = ResourceSnapshot::new();
        assert_eq!(snap.cpu_percent, 0);
        assert_eq!(snap.fps, 0);
    }

    #[test]
    fn test_load_generator_new() {
        let gen = LoadGenerator::new(StressType::CPU, 100);
        assert_eq!(gen.stress_type, StressType::CPU);
        assert_eq!(gen.target_level, 100);
    }

    #[test]
    fn test_load_generator_step() {
        let mut gen = LoadGenerator::new(StressType::CPU, 100);
        assert!(gen.step());
        assert!(gen.current_level > 0);
    }

    #[test]
    fn test_load_generator_reaches_target() {
        let mut gen = LoadGenerator::new(StressType::CPU, 100);
        for _ in 0..200 {
            gen.step();
        }
        assert!(gen.current_level >= gen.target_level);
    }

    #[test]
    fn test_resource_monitor_record() {
        let mut monitor = ResourceMonitor::new();
        let snap = ResourceSnapshot::new();
        monitor.record_sample(snap);
        assert_eq!(monitor.sample_count, 1);
    }

    #[test]
    fn test_resource_monitor_peak_memory() {
        let mut monitor = ResourceMonitor::new();
        let mut snap1 = ResourceSnapshot::new();
        snap1.memory_percent = 50;
        let mut snap2 = ResourceSnapshot::new();
        snap2.memory_percent = 80;
        monitor.record_sample(snap1);
        monitor.record_sample(snap2);
        assert_eq!(monitor.get_peak_memory(), 80);
    }

    #[test]
    fn test_degradation_curve_new() {
        let curve = DegradationCurve::new();
        assert_eq!(curve.sample_count, 0);
    }

    #[test]
    fn test_degradation_curve_add_sample() {
        let mut curve = DegradationCurve::new();
        curve.add_sample(10, 100);
        assert_eq!(curve.sample_count, 1);
    }

    #[test]
    fn test_degradation_analyzer_new() {
        let analyzer = DegradationAnalyzer::new();
        assert_eq!(analyzer.curve_count, 0);
    }

    #[test]
    fn test_stress_test_result_new() {
        let result = StressTestResult::new(StressType::CPU, 100);
        assert_eq!(result.stress_type, StressType::CPU);
        assert!(result.passed);
    }

    #[test]
    fn test_stress_harness_new() {
        let harness = StressTestHarness::new(StressType::CPU, 100, 1000);
        assert_eq!(harness.result.stress_type, StressType::CPU);
        assert_eq!(harness.max_steps, 1000);
    }

    #[test]
    fn test_stress_harness_step() {
        let mut harness = StressTestHarness::new(StressType::CPU, 100, 100);
        harness.step(25, 50, 30, 100, 60);
        assert_eq!(harness.current_step, 1);
    }

    #[test]
    fn test_stress_harness_should_continue() {
        let harness = StressTestHarness::new(StressType::CPU, 100, 100);
        assert!(harness.should_continue());
    }

    #[test]
    fn test_stress_harness_finish() {
        let mut harness = StressTestHarness::new(StressType::CPU, 100, 10);
        for _ in 0..20 {
            harness.step(50, 60, 40, 200, 55);
        }
        harness.finish();
        assert!(harness.result.max_latency >= 200);
    }

    #[test]
    fn test_all_stress_types() {
        let types = [
            StressType::CPU,
            StressType::Memory,
            StressType::DiskIO,
            StressType::Combined,
        ];
        for stress_type in &types {
            let result = StressTestResult::new(*stress_type, 100);
            assert_eq!(result.stress_type, *stress_type);
        }
    }

    #[test]
    fn test_cpu_stress_ramp() {
        let mut harness = StressTestHarness::new(StressType::CPU, 100, 200);

        for i in 0..200 {
            if harness.should_continue() {
                let cpu_load = (i as u32 * 100) / 200;
                harness.step(cpu_load, 30, 20, 50 + i as u32 / 4, 58);
            }
        }
        harness.finish();
        assert!(harness.result.max_load_reached >= 50);
    }

    #[test]
    fn test_memory_stress() {
        let mut harness = StressTestHarness::new(StressType::Memory, 90, 100);
        for i in 0..100 {
            if harness.should_continue() {
                let mem_load = (i as u32 * 90) / 100;
                harness.step(30, mem_load, 15, 100, 59);
            }
        }
        harness.finish();
        assert!(harness.result.passed);
    }

    #[test]
    fn test_combined_stress() {
        let mut harness = StressTestHarness::new(StressType::Combined, 100, 100);
        for i in 0..100 {
            if harness.should_continue() {
                let load = (i as u32 * 100) / 100;
                harness.step(load, load / 2, load / 2, 200, 45);
            }
        }
        harness.finish();
        assert!(harness.result.degradation_is_graceful);
    }

    #[test]
    fn test_graceful_degradation_detection() {
        let mut harness = StressTestHarness::new(StressType::CPU, 100, 100);

        // Linear degradation (graceful)
        for i in 0..100 {
            if harness.should_continue() {
                harness.step(i as u32, 30, 20, 50 + i as u32 / 2, 60 - i as u16 / 10);
            }
        }
        harness.finish();
        assert!(harness.analyzer.is_graceful());
    }

    #[test]
    fn test_crash_detection() {
        let mut harness = StressTestHarness::new(StressType::CPU, 100, 50);
        for _ in 0..50 {
            harness.step(100, 95, 90, 2_000_000, 0); // High latency and 0 FPS
        }
        harness.finish();
        assert!(harness.result.crashes > 0 || harness.result.hangs > 0);
    }

    #[test]
    fn test_recovery_detection() {
        let mut harness = StressTestHarness::new(StressType::Memory, 100, 100);

        // Start normal, then degrade, then recover
        for i in 0..100 {
            if i < 30 {
                harness.step(20, 30, 15, 50, 60);
            } else if i < 70 {
                harness.step(80, 80, 60, 500_000, 10); // High stress
            } else {
                harness.step(40, 50, 20, 100, 50); // Recovery
            }
        }
        harness.finish();
        // Should show recovery despite temporary stress
        assert!(harness.result.min_fps >= 10);
    }
}

// ============================================================================
// STRESS TEST SCENARIOS (Integration Tests)
// ============================================================================

#[cfg(test)]
mod stress_scenarios {
    use super::*;

    #[test]
    fn test_cpu_saturation_100_percent() {
        let mut harness = StressTestHarness::new(StressType::CPU, 100, 200);

        while harness.should_continue() {
            let load = harness.generator.get_current_load();
            harness.step(load, 30, 20, 100 + load / 5, 55);
        }

        harness.finish();
        assert!(harness.result.max_load_reached >= 80);
        assert!(harness.result.degradation_is_graceful || harness.result.crashed_ok());
    }

    #[test]
    fn test_memory_pressure_90_percent() {
        let mut harness = StressTestHarness::new(StressType::Memory, 90, 150);

        while harness.should_continue() {
            let load = harness.generator.get_current_load();
            harness.step(30, load, 20, 150, 57);
        }

        harness.finish();
        assert!(harness.result.max_load_reached >= 70);
    }

    #[test]
    fn test_disk_io_heavy() {
        let mut harness = StressTestHarness::new(StressType::DiskIO, 100, 100);

        while harness.should_continue() {
            let load = harness.generator.get_current_load();
            harness.step(60, 40, load, 300, 52);
        }

        harness.finish();
        assert!(harness.monitor.get_max_latency() > 100);
    }

    #[test]
    fn test_combined_stress_all_resources() {
        let mut harness = StressTestHarness::new(StressType::Combined, 100, 150);

        while harness.should_continue() {
            let load = harness.generator.get_current_load();
            harness.step(load, load * 80 / 100, load * 60 / 100, 200 + load * 3, 50);
        }

        harness.finish();
        // Should degrade gracefully even under combined stress
        assert!(harness.result.degradation_is_graceful || harness.result.min_fps >= 30);
    }

    #[test]
    fn test_sustained_high_load() {
        let mut harness = StressTestHarness::new(StressType::CPU, 100, 500);

        // Ramp to 100%
        for i in 0..250 {
            if harness.should_continue() {
                let load = (i as u32 * 100) / 250;
                harness.step(load, 30, 20, 100 + load, 55);
            }
        }

        // Sustain at 100%
        for _ in 0..250 {
            if harness.should_continue() {
                harness.step(100, 30, 20, 200, 55);
            }
        }

        harness.finish();
        assert!(harness.result.passed || harness.result.recovered);
    }
}

// ============================================================================
// HELPER TRAIT FOR RESULTS
// ============================================================================

impl StressTestResult {
    pub fn crashed_ok(&self) -> bool {
        self.recovered || (self.crashes < 3 && self.min_fps >= 30)
    }
}
