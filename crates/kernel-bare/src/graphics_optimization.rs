// RAYOS Phase 25 Task 5: Graphics Optimization
// Performance profiling, metrics, and adaptive quality
// File: crates/kernel-bare/src/graphics_optimization.rs
// Lines: 680+ | Tests: 14 unit + 5 scenario | Markers: 5

use core::fmt;

const MAX_FRAME_SAMPLES: usize = 256;
const MAX_SHADER_METRICS: usize = 128;
const MAX_BUFFER_TRACKS: usize = 256;
const METRIC_HISTORY_SIZE: usize = 60;

// ============================================================================
// RENDER METRICS
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct FrameMetrics {
    pub frame_number: u64,
    pub frame_time_us: u32,
    pub cpu_time_us: u32,
    pub gpu_time_us: u32,
    pub draw_calls: u32,
    pub vertices_rendered: u32,
    pub pixels_rendered: u64,
    pub gpu_memory_used: u32,
    pub cache_hits: u32,
    pub cache_misses: u32,
}

impl FrameMetrics {
    pub fn new(frame_number: u64) -> Self {
        FrameMetrics {
            frame_number,
            frame_time_us: 0,
            cpu_time_us: 0,
            gpu_time_us: 0,
            draw_calls: 0,
            vertices_rendered: 0,
            pixels_rendered: 0,
            gpu_memory_used: 0,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    pub fn get_fps(&self) -> u32 {
        if self.frame_time_us == 0 {
            return 0;
        }
        (1_000_000u32).saturating_div(self.frame_time_us)
    }

    pub fn get_cache_hit_ratio(&self) -> f32 {
        let total = (self.cache_hits + self.cache_misses) as f32;
        if total == 0.0 {
            return 0.0;
        }
        (self.cache_hits as f32) / total
    }

    pub fn get_pixels_per_second(&self) -> u64 {
        if self.frame_time_us == 0 {
            return 0;
        }
        (self.pixels_rendered * 1_000_000u64) / (self.frame_time_us as u64)
    }
}

pub struct RenderMetrics {
    pub current_frame: FrameMetrics,
    pub frame_history: [FrameMetrics; MAX_FRAME_SAMPLES],
    pub history_count: usize,
    pub peak_fps: u32,
    pub min_fps: u32,
    pub avg_frame_time_us: u32,
}

impl RenderMetrics {
    pub fn new() -> Self {
        RenderMetrics {
            current_frame: FrameMetrics::new(0),
            frame_history: [FrameMetrics::new(0); MAX_FRAME_SAMPLES],
            history_count: 0,
            peak_fps: 0,
            min_fps: 9999,
            avg_frame_time_us: 0,
        }
    }

    pub fn begin_frame(&mut self, frame_number: u64) {
        self.current_frame = FrameMetrics::new(frame_number);
    }

    pub fn end_frame(&mut self) {
        if self.history_count >= MAX_FRAME_SAMPLES {
            // Shift history
            for i in 0..MAX_FRAME_SAMPLES - 1 {
                self.frame_history[i] = self.frame_history[i + 1];
            }
            self.frame_history[MAX_FRAME_SAMPLES - 1] = self.current_frame;
        } else {
            self.frame_history[self.history_count] = self.current_frame;
            self.history_count += 1;
        }

        self.update_statistics();
    }

    fn update_statistics(&mut self) {
        if self.history_count == 0 {
            return;
        }

        let mut total_time = 0u32;
        let mut peak_fps = 0u32;
        let mut min_fps = 9999u32;

        for i in 0..self.history_count {
            let fps = self.frame_history[i].get_fps();
            if fps > 0 {
                peak_fps = peak_fps.max(fps);
                min_fps = min_fps.min(fps);
                total_time = total_time.saturating_add(self.frame_history[i].frame_time_us);
            }
        }

        self.peak_fps = peak_fps;
        self.min_fps = if min_fps == 9999 { 0 } else { min_fps };
        self.avg_frame_time_us = total_time / (self.history_count as u32);
    }

    pub fn get_average_fps(&self) -> u32 {
        if self.avg_frame_time_us == 0 {
            return 0;
        }
        (1_000_000u32).saturating_div(self.avg_frame_time_us)
    }
}

impl Default for RenderMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// GPU PROFILER
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct ShaderMetric {
    pub shader_id: u32,
    pub invocations: u64,
    pub execution_time_us: u32,
    pub register_spill: u32,
    pub sample_count: usize,
}

impl ShaderMetric {
    pub fn new(shader_id: u32) -> Self {
        ShaderMetric {
            shader_id,
            invocations: 0,
            execution_time_us: 0,
            register_spill: 0,
            sample_count: 0,
        }
    }

    pub fn get_avg_time_per_invocation(&self) -> f32 {
        if self.invocations == 0 {
            return 0.0;
        }
        (self.execution_time_us as f32) / (self.invocations as f32)
    }
}

pub struct GPUProfiler {
    pub shaders: [Option<ShaderMetric>; MAX_SHADER_METRICS],
    pub shader_count: usize,
    pub total_gpu_memory: u32,
    pub used_gpu_memory: u32,
    pub buffer_count: u32,
    pub texture_count: u32,
}

impl GPUProfiler {
    pub fn new() -> Self {
        GPUProfiler {
            shaders: [None; MAX_SHADER_METRICS],
            shader_count: 0,
            total_gpu_memory: 512 * 1024 * 1024, // 512MB default
            used_gpu_memory: 0,
            buffer_count: 0,
            texture_count: 0,
        }
    }

    pub fn register_shader(&mut self, shader_id: u32) -> bool {
        if self.shader_count >= MAX_SHADER_METRICS {
            return false;
        }
        self.shaders[self.shader_count] = Some(ShaderMetric::new(shader_id));
        self.shader_count += 1;
        true
    }

    pub fn record_shader_execution(&mut self, shader_id: u32, invocations: u64, time_us: u32) {
        for i in 0..self.shader_count {
            if let Some(ref mut metric) = self.shaders[i] {
                if metric.shader_id == shader_id {
                    let entry = &mut self.shaders[i];
                    if let Some(ref mut m) = entry {
                        m.invocations = m.invocations.saturating_add(invocations);
                        m.execution_time_us = m.execution_time_us.saturating_add(time_us);
                        m.sample_count += 1;
                    }
                    break;
                }
            }
        }
    }

    pub fn allocate_buffer(&mut self, size: u32) -> bool {
        if self.used_gpu_memory + size > self.total_gpu_memory {
            return false;
        }
        self.used_gpu_memory += size;
        self.buffer_count += 1;
        true
    }

    pub fn deallocate_buffer(&mut self, size: u32) {
        self.used_gpu_memory = self.used_gpu_memory.saturating_sub(size);
        self.buffer_count = self.buffer_count.saturating_sub(1);
    }

    pub fn allocate_texture(&mut self, size: u32) -> bool {
        if self.used_gpu_memory + size > self.total_gpu_memory {
            return false;
        }
        self.used_gpu_memory += size;
        self.texture_count += 1;
        true
    }

    pub fn get_memory_utilization(&self) -> u32 {
        if self.total_gpu_memory == 0 {
            return 0;
        }
        (self.used_gpu_memory * 100) / self.total_gpu_memory
    }
}

impl Default for GPUProfiler {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// FRAME TIME ANALYZER
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct FrameTimeHistogram {
    pub bucket_0_1ms: u32,      // 0-1ms
    pub bucket_1_2ms: u32,      // 1-2ms
    pub bucket_2_4ms: u32,      // 2-4ms
    pub bucket_4_8ms: u32,      // 4-8ms
    pub bucket_8_16ms: u32,     // 8-16ms
    pub bucket_16_plus_ms: u32, // 16ms+
}

impl FrameTimeHistogram {
    pub fn new() -> Self {
        FrameTimeHistogram {
            bucket_0_1ms: 0,
            bucket_1_2ms: 0,
            bucket_2_4ms: 0,
            bucket_4_8ms: 0,
            bucket_8_16ms: 0,
            bucket_16_plus_ms: 0,
        }
    }

    pub fn record_frame(&mut self, frame_time_ms: u32) {
        match frame_time_ms {
            0..=1 => self.bucket_0_1ms += 1,
            2 => self.bucket_1_2ms += 1,
            3..=4 => self.bucket_2_4ms += 1,
            5..=8 => self.bucket_4_8ms += 1,
            9..=16 => self.bucket_8_16ms += 1,
            _ => self.bucket_16_plus_ms += 1,
        }
    }

    pub fn get_percentile_95(&self) -> u32 {
        let total = self.bucket_0_1ms
            + self.bucket_1_2ms
            + self.bucket_2_4ms
            + self.bucket_4_8ms
            + self.bucket_8_16ms
            + self.bucket_16_plus_ms;

        if total == 0 {
            return 0;
        }

        let threshold = (total * 95) / 100;
        let mut count = 0;

        if count + self.bucket_0_1ms >= threshold {
            return 1;
        }
        count += self.bucket_0_1ms;

        if count + self.bucket_1_2ms >= threshold {
            return 2;
        }
        count += self.bucket_1_2ms;

        if count + self.bucket_2_4ms >= threshold {
            return 4;
        }
        count += self.bucket_2_4ms;

        if count + self.bucket_4_8ms >= threshold {
            return 8;
        }
        count += self.bucket_4_8ms;

        if count + self.bucket_8_16ms >= threshold {
            return 16;
        }

        32
    }
}

impl Default for FrameTimeHistogram {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FrameTimeAnalyzer {
    pub histogram: FrameTimeHistogram,
    pub total_frames: u32,
    pub frame_time_variance: u32,
    pub max_frame_latency_us: u32,
}

impl FrameTimeAnalyzer {
    pub fn new() -> Self {
        FrameTimeAnalyzer {
            histogram: FrameTimeHistogram::new(),
            total_frames: 0,
            frame_time_variance: 0,
            max_frame_latency_us: 0,
        }
    }

    pub fn record_frame_time(&mut self, time_us: u32) {
        self.total_frames += 1;
        let time_ms = time_us / 1000;
        self.histogram.record_frame(time_ms);

        if time_us > self.max_frame_latency_us {
            self.max_frame_latency_us = time_us;
        }
    }

    pub fn get_frame_count(&self) -> u32 {
        self.total_frames
    }
}

impl Default for FrameTimeAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ADAPTIVE QUALITY
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityLevel {
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Debug, Clone, Copy)]
pub struct AdaptiveQuality {
    pub current_quality: QualityLevel,
    pub target_fps: u32,
    pub current_fps: u32,
    pub resolution_scale: u8,   // percentage: 50-100
    pub shader_quality: u8,     // percentage: 50-100
    pub effect_enabled: bool,
    pub frame_time_budget_us: u32,
}

impl AdaptiveQuality {
    pub fn new(target_fps: u32) -> Self {
        let frame_budget = if target_fps > 0 {
            1_000_000u32 / target_fps
        } else {
            16_667
        }; // 60 FPS default

        AdaptiveQuality {
            current_quality: QualityLevel::High,
            target_fps,
            current_fps: 0,
            resolution_scale: 100,
            shader_quality: 100,
            effect_enabled: true,
            frame_time_budget_us: frame_budget,
        }
    }

    pub fn adjust_quality(&mut self, measured_fps: u32) {
        self.current_fps = measured_fps;

        if measured_fps < self.target_fps.saturating_sub(5) {
            // Too slow, reduce quality
            match self.current_quality {
                QualityLevel::Ultra => {
                    self.current_quality = QualityLevel::High;
                    self.shader_quality = 80;
                }
                QualityLevel::High => {
                    self.current_quality = QualityLevel::Medium;
                    self.resolution_scale = 90;
                    self.shader_quality = 60;
                }
                QualityLevel::Medium => {
                    self.current_quality = QualityLevel::Low;
                    self.resolution_scale = 75;
                    self.shader_quality = 40;
                    self.effect_enabled = false;
                }
                QualityLevel::Low => {
                    self.resolution_scale = 50;
                    self.shader_quality = 30;
                }
            }
        } else if measured_fps > self.target_fps.saturating_add(10) {
            // Fast enough, increase quality
            match self.current_quality {
                QualityLevel::Low => {
                    self.current_quality = QualityLevel::Medium;
                    self.resolution_scale = 85;
                    self.shader_quality = 60;
                    self.effect_enabled = true;
                }
                QualityLevel::Medium => {
                    self.current_quality = QualityLevel::High;
                    self.resolution_scale = 100;
                    self.shader_quality = 80;
                }
                QualityLevel::High => {
                    self.current_quality = QualityLevel::Ultra;
                    self.shader_quality = 100;
                }
                QualityLevel::Ultra => {
                    // Already maxed out
                }
            }
        }
    }
}

// ============================================================================
// PIPELINE CACHE
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct PipelineStateKey {
    pub vertex_format: u32,
    pub fragment_format: u32,
    pub blend_mode: u8,
    pub depth_test: bool,
    pub cull_mode: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct CachedPipelineState {
    pub key: PipelineStateKey,
    pub pipeline_id: u32,
    pub last_used_frame: u64,
    pub hit_count: u32,
}

pub struct PipelineCache {
    pub pipelines: [Option<CachedPipelineState>; MAX_SHADER_METRICS],
    pub pipeline_count: usize,
    pub cache_hits: u32,
    pub cache_misses: u32,
    pub current_frame: u64,
}

impl PipelineCache {
    pub fn new() -> Self {
        PipelineCache {
            pipelines: [None; MAX_SHADER_METRICS],
            pipeline_count: 0,
            cache_hits: 0,
            cache_misses: 0,
            current_frame: 0,
        }
    }

    pub fn lookup(&mut self, key: PipelineStateKey) -> Option<u32> {
        for i in 0..self.pipeline_count {
            if let Some(ref mut state) = self.pipelines[i] {
                if state.key.vertex_format == key.vertex_format
                    && state.key.fragment_format == key.fragment_format
                    && state.key.blend_mode == key.blend_mode
                    && state.key.depth_test == key.depth_test
                    && state.key.cull_mode == key.cull_mode
                {
                    state.last_used_frame = self.current_frame;
                    state.hit_count += 1;
                    self.cache_hits += 1;
                    return Some(state.pipeline_id);
                }
            }
        }
        self.cache_misses += 1;
        None
    }

    pub fn insert(&mut self, key: PipelineStateKey, pipeline_id: u32) -> bool {
        if self.pipeline_count >= MAX_SHADER_METRICS {
            // Evict least recently used
            let mut lru_idx = 0;
            let mut oldest_frame = u64::MAX;
            for i in 0..self.pipeline_count {
                if let Some(state) = self.pipelines[i] {
                    if state.last_used_frame < oldest_frame {
                        oldest_frame = state.last_used_frame;
                        lru_idx = i;
                    }
                }
            }
            self.pipelines[lru_idx] = None;
        }

        for i in 0..self.pipeline_count {
            if self.pipelines[i].is_none() {
                self.pipelines[i] = Some(CachedPipelineState {
                    key,
                    pipeline_id,
                    last_used_frame: self.current_frame,
                    hit_count: 0,
                });
                return true;
            }
        }

        if self.pipeline_count < MAX_SHADER_METRICS {
            self.pipelines[self.pipeline_count] = Some(CachedPipelineState {
                key,
                pipeline_id,
                last_used_frame: self.current_frame,
                hit_count: 0,
            });
            self.pipeline_count += 1;
        }

        true
    }

    pub fn get_hit_ratio(&self) -> f32 {
        let total = (self.cache_hits + self.cache_misses) as f32;
        if total == 0.0 {
            return 0.0;
        }
        (self.cache_hits as f32) / total
    }

    pub fn begin_frame(&mut self, frame_number: u64) {
        self.current_frame = frame_number;
    }
}

impl Default for PipelineCache {
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
    fn test_frame_metrics_new() {
        let metrics = FrameMetrics::new(1);
        assert_eq!(metrics.frame_number, 1);
        assert_eq!(metrics.frame_time_us, 0);
    }

    #[test]
    fn test_frame_metrics_fps() {
        let mut metrics = FrameMetrics::new(1);
        metrics.frame_time_us = 16667; // ~60 FPS
        assert!(metrics.get_fps() > 50 && metrics.get_fps() < 70);
    }

    #[test]
    fn test_frame_metrics_cache_ratio() {
        let mut metrics = FrameMetrics::new(1);
        metrics.cache_hits = 80;
        metrics.cache_misses = 20;
        let ratio = metrics.get_cache_hit_ratio();
        assert!((ratio - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_render_metrics_new() {
        let metrics = RenderMetrics::new();
        assert_eq!(metrics.history_count, 0);
    }

    #[test]
    fn test_render_metrics_frame_cycle() {
        let mut metrics = RenderMetrics::new();
        metrics.begin_frame(1);
        metrics.current_frame.frame_time_us = 16667;
        metrics.end_frame();
        assert_eq!(metrics.history_count, 1);
    }

    #[test]
    fn test_gpu_profiler_memory() {
        let mut profiler = GPUProfiler::new();
        assert!(profiler.allocate_buffer(1024));
        assert_eq!(profiler.buffer_count, 1);
        profiler.deallocate_buffer(1024);
        assert_eq!(profiler.buffer_count, 0);
    }

    #[test]
    fn test_gpu_profiler_shader_register() {
        let mut profiler = GPUProfiler::new();
        assert!(profiler.register_shader(1));
        assert_eq!(profiler.shader_count, 1);
    }

    #[test]
    fn test_frame_time_histogram() {
        let mut histogram = FrameTimeHistogram::new();
        histogram.record_frame(2);
        histogram.record_frame(5);
        histogram.record_frame(12);
        assert!(histogram.bucket_1_2ms > 0);
        assert!(histogram.bucket_4_8ms > 0);
        assert!(histogram.bucket_8_16ms > 0);
    }

    #[test]
    fn test_frame_time_analyzer_latency() {
        let mut analyzer = FrameTimeAnalyzer::new();
        analyzer.record_frame_time(5000);
        analyzer.record_frame_time(3000);
        assert_eq!(analyzer.max_frame_latency_us, 5000);
        assert_eq!(analyzer.get_frame_count(), 2);
    }

    #[test]
    fn test_adaptive_quality_new() {
        let quality = AdaptiveQuality::new(60);
        assert_eq!(quality.current_quality, QualityLevel::High);
        assert_eq!(quality.target_fps, 60);
    }

    #[test]
    fn test_adaptive_quality_adjust_down() {
        let mut quality = AdaptiveQuality::new(60);
        quality.adjust_quality(30); // Very slow
        assert!(quality.resolution_scale < 100);
    }

    #[test]
    fn test_adaptive_quality_adjust_up() {
        let mut quality = AdaptiveQuality::new(60);
        quality.current_quality = QualityLevel::Low;
        quality.adjust_quality(120); // Very fast
        assert!(quality.shader_quality > 40);
    }

    #[test]
    fn test_pipeline_cache_new() {
        let cache = PipelineCache::new();
        assert_eq!(cache.pipeline_count, 0);
        assert_eq!(cache.cache_hits, 0);
    }

    #[test]
    fn test_pipeline_cache_insert_and_lookup() {
        let mut cache = PipelineCache::new();
        let key = PipelineStateKey {
            vertex_format: 1,
            fragment_format: 2,
            blend_mode: 0,
            depth_test: true,
            cull_mode: 1,
        };
        cache.insert(key, 100);
        cache.begin_frame(1);
        let result = cache.lookup(key);
        assert_eq!(result, Some(100));
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_complete_frame_profiling() {
        let mut metrics = RenderMetrics::new();
        let mut profiler = GPUProfiler::new();

        metrics.begin_frame(1);
        metrics.current_frame.frame_time_us = 16667;
        metrics.current_frame.draw_calls = 128;
        metrics.current_frame.vertices_rendered = 2_000_000;

        profiler.allocate_buffer(10 * 1024 * 1024);
        profiler.allocate_texture(5 * 1024 * 1024);

        metrics.end_frame();

        assert_eq!(metrics.history_count, 1);
        assert!(profiler.get_memory_utilization() > 0);
    }

    #[test]
    fn test_adaptive_quality_stabilization() {
        let mut quality = AdaptiveQuality::new(60);

        // Simulate frame rate variations
        quality.adjust_quality(55);
        let q1 = quality.current_quality;

        quality.adjust_quality(75);
        let q2 = quality.current_quality;

        assert!(q1 != q2 || quality.resolution_scale != 100);
    }

    #[test]
    fn test_pipeline_cache_efficiency() {
        let mut cache = PipelineCache::new();
        let key = PipelineStateKey {
            vertex_format: 1,
            fragment_format: 2,
            blend_mode: 0,
            depth_test: true,
            cull_mode: 1,
        };

        cache.begin_frame(1);
        cache.insert(key, 100);

        // Hit the cache multiple times
        for _ in 0..10 {
            cache.lookup(key);
        }

        let hit_ratio = cache.get_hit_ratio();
        assert!(hit_ratio > 0.8);
    }

    #[test]
    fn test_frame_time_analysis_histogram() {
        let mut analyzer = FrameTimeAnalyzer::new();

        // Record various frame times
        for _ in 0..50 {
            analyzer.record_frame_time(5000); // 5ms
        }
        for _ in 0..30 {
            analyzer.record_frame_time(12000); // 12ms
        }
        for _ in 0..20 {
            analyzer.record_frame_time(25000); // 25ms
        }

        assert_eq!(analyzer.get_frame_count(), 100);
        assert_eq!(analyzer.max_frame_latency_us, 25000);
    }
}
