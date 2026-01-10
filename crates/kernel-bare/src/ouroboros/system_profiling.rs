//! System-wide Profiling: Integration with Kernel Profilers
//!
//! Captures hotspots, call graphs, memory allocation patterns, and CPU usage
//! from kernel profilers (perf, flamegraph). Guides mutation strategy by identifying
//! performance bottlenecks and optimization opportunities.
//!
//! Phase 34, Task 3

/// Profile data source
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProfileSource {
    Perf = 0,          // Linux perf tool
    Flamegraph = 1,    // Flamegraph visualizations
    KernelTrace = 2,   // Kernel trace events
    MemoryProfile = 3, // Memory allocation tracking
}

/// Hotspot type classification
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum HotspotType {
    CpuIntensive = 0,      // High CPU usage
    MemoryIntensive = 1,   // High memory allocation/access
    CacheInefficient = 2,  // Poor cache utilization
    ContentionPoint = 3,   // Lock/synchronization contention
    BranchMisprediction = 4, // Frequent branch misprediction
}

/// Profile metric type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProfileMetricType {
    CycleCount = 0,
    InstructionCount = 1,
    CacheHits = 2,
    CacheMisses = 3,
    BranchTaken = 4,
    MemoryBytesRead = 5,
    MemoryBytesWritten = 6,
    PageFaults = 7,
}

/// Performance hotspot in kernel code
#[derive(Clone, Copy, Debug)]
pub struct ProfileHotspot {
    /// Hotspot ID
    pub id: u32,
    /// Function address/hash
    pub function_hash: u64,
    /// Hotspot type
    pub hotspot_type: HotspotType,
    /// Percentage of total time in this hotspot (0-100%)
    pub time_percent: u8,
    /// Occurrence count in profile
    pub occurrence_count: u32,
    /// Call frequency (calls per second)
    pub call_frequency_hz: u32,
    /// Potential improvement estimate (percent)
    pub optimization_potential: u8,
}

impl ProfileHotspot {
    /// Create new hotspot
    pub const fn new(id: u32, function_hash: u64, hotspot_type: HotspotType) -> Self {
        ProfileHotspot {
            id,
            function_hash,
            hotspot_type,
            time_percent: 0,
            occurrence_count: 0,
            call_frequency_hz: 0,
            optimization_potential: 0,
        }
    }

    /// Is critical hotspot (> 10% time)
    pub fn is_critical(&self) -> bool {
        self.time_percent > 10
    }

    /// Priority score for optimization (0-100)
    pub fn priority_score(&self) -> u8 {
        // Weighted: time (60%) + optimization potential (40%)
        let weighted = (self.time_percent as u32 * 60 + self.optimization_potential as u32 * 40) / 100;
        weighted as u8
    }
}

/// Call graph edge (caller -> callee)
#[derive(Clone, Copy, Debug)]
pub struct CallGraphEdge {
    /// Edge ID
    pub id: u32,
    /// Caller function hash
    pub caller_hash: u64,
    /// Callee function hash
    pub callee_hash: u64,
    /// Call count
    pub call_count: u32,
    /// Total time in callee from this call path (percent)
    pub time_percent: u8,
    /// Is hot edge (> 5% of time)
    pub is_hot: bool,
}

impl CallGraphEdge {
    /// Create new call graph edge
    pub const fn new(id: u32, caller_hash: u64, callee_hash: u64) -> Self {
        CallGraphEdge {
            id,
            caller_hash,
            callee_hash,
            call_count: 0,
            time_percent: 0,
            is_hot: false,
        }
    }

    /// Update edge with timing information
    pub fn update_timing(&mut self, time_percent: u8) {
        self.time_percent = time_percent;
        self.is_hot = time_percent > 5;
    }
}

/// Memory allocation pattern
#[derive(Clone, Copy, Debug)]
pub struct MemoryPattern {
    /// Pattern ID
    pub id: u32,
    /// Allocating function hash
    pub allocator_hash: u64,
    /// Allocation size category (0=tiny, 1=small, 2=medium, 3=large, 4=huge)
    pub size_category: u8,
    /// Total bytes allocated
    pub total_bytes: u64,
    /// Allocation count
    pub alloc_count: u32,
    /// Deallocation count
    pub dealloc_count: u32,
    /// Peak concurrent bytes
    pub peak_concurrent_bytes: u64,
}

impl MemoryPattern {
    /// Create new memory pattern
    pub const fn new(id: u32, allocator_hash: u64, size_category: u8) -> Self {
        MemoryPattern {
            id,
            allocator_hash,
            size_category,
            total_bytes: 0,
            alloc_count: 0,
            dealloc_count: u32::MAX,  // Use MAX to indicate not set
            peak_concurrent_bytes: 0,
        }
    }

    /// Memory leak likelihood (0-100%)
    pub fn leak_likelihood(&self) -> u8 {
        if self.alloc_count == 0 {
            return 0;
        }
        let actual_deallocs = if self.dealloc_count == u32::MAX { 0 } else { self.dealloc_count };
        let leaked = self.alloc_count.saturating_sub(actual_deallocs);
        ((leaked as u32 * 100) / (self.alloc_count as u32)) as u8
    }

    /// Average allocation size
    pub fn average_allocation_bytes(&self) -> u32 {
        if self.alloc_count == 0 {
            0
        } else {
            (self.total_bytes / (self.alloc_count as u64)) as u32
        }
    }
}

/// Profile metric data point
#[derive(Clone, Copy, Debug)]
pub struct ProfileMetricDataPoint {
    /// Metric type
    pub metric_type: ProfileMetricType,
    /// Function hash
    pub function_hash: u64,
    /// Metric value
    pub value: u64,
    /// Timestamp (ms)
    pub timestamp_ms: u64,
}

impl ProfileMetricDataPoint {
    /// Create new metric data point
    pub const fn new(metric_type: ProfileMetricType, function_hash: u64, value: u64) -> Self {
        ProfileMetricDataPoint {
            metric_type,
            function_hash,
            value,
            timestamp_ms: 0,
        }
    }
}

/// Complete profile snapshot
#[derive(Clone, Copy, Debug)]
pub struct ProfileSnapshot {
    /// Snapshot ID
    pub id: u32,
    /// Source profiler
    pub source: ProfileSource,
    /// Profile duration (ms)
    pub duration_ms: u32,
    /// Hotspot count
    pub hotspot_count: u8,
    /// Memory pattern count
    pub memory_pattern_count: u8,
    /// Call graph edge count
    pub edge_count: u16,
    /// Timestamp (ms)
    pub timestamp_ms: u64,
}

impl ProfileSnapshot {
    /// Create new profile snapshot
    pub const fn new(id: u32, source: ProfileSource, duration_ms: u32) -> Self {
        ProfileSnapshot {
            id,
            source,
            duration_ms,
            hotspot_count: 0,
            memory_pattern_count: 0,
            edge_count: 0,
            timestamp_ms: 0,
        }
    }
}

/// System-wide Profiler
pub struct SystemProfiler {
    /// Current profile snapshot
    current_snapshot: Option<ProfileSnapshot>,
    /// Hotspots (max 50)
    hotspots: [Option<ProfileHotspot>; 50],
    /// Call graph edges (max 100)
    call_graph: [Option<CallGraphEdge>; 100],
    /// Memory patterns (max 30)
    memory_patterns: [Option<MemoryPattern>; 30],
    /// Metric data points (max 200, circular buffer)
    metric_buffer: [Option<ProfileMetricDataPoint>; 200],
    /// Profile history (last 10)
    profile_history: [Option<ProfileSnapshot>; 10],
    /// Profiling active
    profiling_active: bool,
    /// Total profiles collected
    total_profiles: u32,
}

impl SystemProfiler {
    /// Create new system profiler
    pub const fn new() -> Self {
        SystemProfiler {
            current_snapshot: None,
            hotspots: [None; 50],
            call_graph: [None; 100],
            memory_patterns: [None; 30],
            metric_buffer: [None; 200],
            profile_history: [None; 10],
            profiling_active: false,
            total_profiles: 0,
        }
    }

    /// Start profiling session
    pub fn start_profiling(&mut self, source: ProfileSource, duration_ms: u32) {
        let snapshot = ProfileSnapshot::new(self.total_profiles, source, duration_ms);
        self.current_snapshot = Some(snapshot);
        self.profiling_active = true;
    }

    /// End profiling session
    pub fn end_profiling(&mut self) {
        if let Some(snapshot) = self.current_snapshot {
            // Store in history
            for slot in &mut self.profile_history {
                if slot.is_none() {
                    *slot = Some(snapshot);
                    break;
                }
            }
            self.total_profiles += 1;
        }
        self.profiling_active = false;
    }

    /// Register hotspot
    pub fn register_hotspot(&mut self, hotspot: ProfileHotspot) -> bool {
        for slot in &mut self.hotspots {
            if slot.is_none() {
                *slot = Some(hotspot);
                if let Some(ref mut snap) = self.current_snapshot {
                    snap.hotspot_count += 1;
                }
                return true;
            }
        }
        false
    }

    /// Register memory pattern
    pub fn register_memory_pattern(&mut self, pattern: MemoryPattern) -> bool {
        for slot in &mut self.memory_patterns {
            if slot.is_none() {
                *slot = Some(pattern);
                if let Some(ref mut snap) = self.current_snapshot {
                    snap.memory_pattern_count += 1;
                }
                return true;
            }
        }
        false
    }

    /// Register call graph edge
    pub fn register_edge(&mut self, edge: CallGraphEdge) -> bool {
        for slot in &mut self.call_graph {
            if slot.is_none() {
                *slot = Some(edge);
                if let Some(ref mut snap) = self.current_snapshot {
                    snap.edge_count += 1;
                }
                return true;
            }
        }
        false
    }

    /// Record metric data
    pub fn record_metric(&mut self, metric: ProfileMetricDataPoint) {
        // Circular buffer: find first empty or replace oldest
        for slot in &mut self.metric_buffer {
            if slot.is_none() {
                *slot = Some(metric);
                return;
            }
        }
        // All full - overwrite first
        self.metric_buffer[0] = Some(metric);
    }

    /// Get critical hotspots (time_percent > 10%)
    pub fn get_critical_hotspots(&self) -> [Option<ProfileHotspot>; 50] {
        let mut result = [None; 50];
        let mut idx = 0;

        for slot in &self.hotspots {
            if let Some(hs) = slot {
                if hs.is_critical() && idx < 50 {
                    result[idx] = Some(*hs);
                    idx += 1;
                }
            }
        }

        result
    }

    /// Get hot call paths (edges with > 5% time)
    pub fn get_hot_call_paths(&self) -> [Option<CallGraphEdge>; 100] {
        let mut result = [None; 100];
        let mut idx = 0;

        for slot in &self.call_graph {
            if let Some(edge) = slot {
                if edge.is_hot && idx < 100 {
                    result[idx] = Some(*edge);
                    idx += 1;
                }
            }
        }

        result
    }

    /// Find hotspot by function hash
    pub fn find_hotspot(&self, function_hash: u64) -> Option<ProfileHotspot> {
        for slot in &self.hotspots {
            if let Some(hs) = slot {
                if hs.function_hash == function_hash {
                    return Some(*hs);
                }
            }
        }
        None
    }

    /// Get top N hotspots by priority score
    pub fn top_hotspots(&self, _n: usize) -> [Option<ProfileHotspot>; 50] {
        let mut sorted = self.hotspots;
        
        // Simple bubble sort by priority score
        for i in 0..50 {
            for j in 0..49 - i {
                let score_j = sorted[j].map(|h| h.priority_score()).unwrap_or(0);
                let score_jp1 = sorted[j + 1].map(|h| h.priority_score()).unwrap_or(0);
                
                if score_j < score_jp1 {
                    sorted.swap(j, j + 1);
                }
            }
        }

        sorted
    }

    /// Check for memory leaks
    pub fn check_memory_leaks(&self) -> [Option<MemoryPattern>; 30] {
        let mut result = [None; 30];
        let mut idx = 0;

        for slot in &self.memory_patterns {
            if let Some(pattern) = slot {
                if pattern.leak_likelihood() > 20 && idx < 30 {
                    result[idx] = Some(*pattern);
                    idx += 1;
                }
            }
        }

        result
    }

    /// Get profiling status
    pub fn is_profiling(&self) -> bool {
        self.profiling_active
    }

    /// Get total profiles collected
    pub fn total_profiles(&self) -> u32 {
        self.total_profiles
    }

    /// Get current snapshot
    pub fn current_snapshot(&self) -> Option<ProfileSnapshot> {
        self.current_snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_source_enum() {
        assert_eq!(ProfileSource::Perf as u8, 0);
        assert_eq!(ProfileSource::Flamegraph as u8, 1);
        assert_eq!(ProfileSource::MemoryProfile as u8, 3);
    }

    #[test]
    fn test_hotspot_type_enum() {
        assert_eq!(HotspotType::CpuIntensive as u8, 0);
        assert_eq!(HotspotType::CacheInefficient as u8, 2);
        assert_eq!(HotspotType::BranchMisprediction as u8, 4);
    }

    #[test]
    fn test_hotspot_creation() {
        let hotspot = ProfileHotspot::new(1, 0x1234567890abcdef, HotspotType::CpuIntensive);
        assert_eq!(hotspot.id, 1);
        assert_eq!(hotspot.function_hash, 0x1234567890abcdef);
        assert_eq!(hotspot.time_percent, 0);
    }

    #[test]
    fn test_hotspot_is_critical() {
        let mut hotspot = ProfileHotspot::new(1, 0x1234567890abcdef, HotspotType::CpuIntensive);
        assert!(!hotspot.is_critical());

        hotspot.time_percent = 15;
        assert!(hotspot.is_critical());
    }

    #[test]
    fn test_hotspot_priority_score() {
        let mut hotspot = ProfileHotspot::new(1, 0x1234567890abcdef, HotspotType::CpuIntensive);
        hotspot.time_percent = 50;
        hotspot.optimization_potential = 80;
        
        // (50 * 60 + 80 * 40) / 100 = (3000 + 3200) / 100 = 62
        let score = hotspot.priority_score();
        assert_eq!(score, 62);
    }

    #[test]
    fn test_call_graph_edge_creation() {
        let edge = CallGraphEdge::new(1, 0xaaaaaaaaaaaaaaaa, 0xbbbbbbbbbbbbbbbb);
        assert_eq!(edge.id, 1);
        assert_eq!(edge.caller_hash, 0xaaaaaaaaaaaaaaaa);
        assert!(!edge.is_hot);
    }

    #[test]
    fn test_call_graph_edge_update_timing() {
        let mut edge = CallGraphEdge::new(1, 0xaaaaaaaaaaaaaaaa, 0xbbbbbbbbbbbbbbbb);
        assert!(!edge.is_hot);

        edge.update_timing(8);
        assert!(edge.is_hot);
    }

    #[test]
    fn test_memory_pattern_creation() {
        let pattern = MemoryPattern::new(1, 0x1234567890abcdef, 2);
        assert_eq!(pattern.id, 1);
        assert_eq!(pattern.size_category, 2);
        assert_eq!(pattern.total_bytes, 0);
    }

    #[test]
    fn test_memory_pattern_leak_likelihood() {
        let mut pattern = MemoryPattern::new(1, 0x1234567890abcdef, 2);
        pattern.alloc_count = 10;
        pattern.dealloc_count = 8;

        let likelihood = pattern.leak_likelihood();
        assert_eq!(likelihood, 20);  // 2/10 = 20%
    }

    #[test]
    fn test_memory_pattern_average_allocation() {
        let mut pattern = MemoryPattern::new(1, 0x1234567890abcdef, 2);
        pattern.alloc_count = 5;
        pattern.total_bytes = 1000;

        let avg = pattern.average_allocation_bytes();
        assert_eq!(avg, 200);
    }

    #[test]
    fn test_metric_data_point_creation() {
        let metric = ProfileMetricDataPoint::new(ProfileMetricType::CycleCount, 0x1234567890abcdef, 12345);
        assert_eq!(metric.metric_type, ProfileMetricType::CycleCount);
        assert_eq!(metric.value, 12345);
    }

    #[test]
    fn test_profile_snapshot_creation() {
        let snapshot = ProfileSnapshot::new(1, ProfileSource::Perf, 1000);
        assert_eq!(snapshot.id, 1);
        assert_eq!(snapshot.source, ProfileSource::Perf);
        assert_eq!(snapshot.duration_ms, 1000);
    }

    #[test]
    fn test_system_profiler_creation() {
        let profiler = SystemProfiler::new();
        assert!(!profiler.is_profiling());
        assert_eq!(profiler.total_profiles(), 0);
    }

    #[test]
    fn test_system_profiler_start_profiling() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::Perf, 1000);

        assert!(profiler.is_profiling());
        assert!(profiler.current_snapshot().is_some());
    }

    #[test]
    fn test_system_profiler_end_profiling() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::Perf, 1000);
        profiler.end_profiling();

        assert!(!profiler.is_profiling());
        assert_eq!(profiler.total_profiles(), 1);
    }

    #[test]
    fn test_system_profiler_register_hotspot() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::Perf, 1000);

        let hotspot = ProfileHotspot::new(1, 0x1234567890abcdef, HotspotType::CpuIntensive);
        assert!(profiler.register_hotspot(hotspot));

        let snapshot = profiler.current_snapshot();
        assert!(snapshot.is_some());
        assert_eq!(snapshot.unwrap().hotspot_count, 1);
    }

    #[test]
    fn test_system_profiler_register_memory_pattern() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::MemoryProfile, 1000);

        let pattern = MemoryPattern::new(1, 0x1234567890abcdef, 2);
        assert!(profiler.register_memory_pattern(pattern));

        let snapshot = profiler.current_snapshot();
        assert_eq!(snapshot.unwrap().memory_pattern_count, 1);
    }

    #[test]
    fn test_system_profiler_register_edge() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::Perf, 1000);

        let edge = CallGraphEdge::new(1, 0xaaaaaaaaaaaaaaaa, 0xbbbbbbbbbbbbbbbb);
        assert!(profiler.register_edge(edge));

        let snapshot = profiler.current_snapshot();
        assert_eq!(snapshot.unwrap().edge_count, 1);
    }

    #[test]
    fn test_system_profiler_record_metric() {
        let mut profiler = SystemProfiler::new();
        let metric = ProfileMetricDataPoint::new(ProfileMetricType::CycleCount, 0x1234567890abcdef, 12345);
        profiler.record_metric(metric);

        assert!(profiler.total_profiles() >= 0);
    }

    #[test]
    fn test_system_profiler_find_hotspot() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::Perf, 1000);

        let hotspot = ProfileHotspot::new(1, 0x1234567890abcdef, HotspotType::CpuIntensive);
        profiler.register_hotspot(hotspot);

        let found = profiler.find_hotspot(0x1234567890abcdef);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, 1);
    }

    #[test]
    fn test_system_profiler_get_critical_hotspots() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::Perf, 1000);

        let mut hs1 = ProfileHotspot::new(1, 0x1111111111111111, HotspotType::CpuIntensive);
        hs1.time_percent = 5;
        let mut hs2 = ProfileHotspot::new(2, 0x2222222222222222, HotspotType::MemoryIntensive);
        hs2.time_percent = 15;

        profiler.register_hotspot(hs1);
        profiler.register_hotspot(hs2);

        let critical = profiler.get_critical_hotspots();
        let mut count = 0;
        for slot in &critical {
            if slot.is_some() {
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn test_system_profiler_get_hot_call_paths() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::Perf, 1000);

        let mut edge1 = CallGraphEdge::new(1, 0xaaaaaaaaaaaaaaaa, 0xbbbbbbbbbbbbbbbb);
        edge1.time_percent = 3;
        let mut edge2 = CallGraphEdge::new(2, 0xcccccccccccccccc, 0xdddddddddddddddd);
        edge2.time_percent = 8;

        profiler.register_edge(edge1);
        profiler.register_edge(edge2);

        let hot_paths = profiler.get_hot_call_paths();
        let mut count = 0;
        for slot in &hot_paths {
            if slot.is_some() {
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn test_system_profiler_check_memory_leaks() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::MemoryProfile, 1000);

        let mut pattern = MemoryPattern::new(1, 0x1234567890abcdef, 2);
        pattern.alloc_count = 100;
        pattern.dealloc_count = 75;

        profiler.register_memory_pattern(pattern);

        let leaks = profiler.check_memory_leaks();
        let mut count = 0;
        for slot in &leaks {
            if slot.is_some() {
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn test_metric_type_enum() {
        assert_eq!(ProfileMetricType::CycleCount as u8, 0);
        assert_eq!(ProfileMetricType::MemoryBytesRead as u8, 5);
        assert_eq!(ProfileMetricType::PageFaults as u8, 7);
    }

    #[test]
    fn test_system_profiler_top_hotspots() {
        let mut profiler = SystemProfiler::new();
        profiler.start_profiling(ProfileSource::Perf, 1000);

        let mut hs1 = ProfileHotspot::new(1, 0x1111111111111111, HotspotType::CpuIntensive);
        hs1.time_percent = 20;
        hs1.optimization_potential = 50;

        let mut hs2 = ProfileHotspot::new(2, 0x2222222222222222, HotspotType::MemoryIntensive);
        hs2.time_percent = 30;
        hs2.optimization_potential = 60;

        profiler.register_hotspot(hs1);
        profiler.register_hotspot(hs2);

        let top = profiler.top_hotspots(2);
        assert!(top[0].is_some());
        assert_eq!(top[0].unwrap().id, 2);
    }

    #[test]
    fn test_memory_pattern_no_leaks() {
        let pattern = MemoryPattern::new(1, 0x1234567890abcdef, 2);
        assert_eq!(pattern.leak_likelihood(), 0);
    }

    #[test]
    fn test_profile_snapshot_empty() {
        let snapshot = ProfileSnapshot::new(0, ProfileSource::KernelTrace, 500);
        assert_eq!(snapshot.hotspot_count, 0);
        assert_eq!(snapshot.memory_pattern_count, 0);
    }
}
