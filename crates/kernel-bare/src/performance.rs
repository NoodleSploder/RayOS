// Phase 11 Task 4: Performance Optimization
// Fast-path optimizations for <1µs latency and sub-100ns policy checks


/// Hash table for firewall rules (O(1) lookup)
pub struct FirewallHashTable {
    buckets: [[Option<u32>; 4]; 64],  // 64 buckets, 4 collisions per bucket
    rule_count: u32,
    hits: u32,
    misses: u32,
    collisions: u32,
}

impl FirewallHashTable {
    pub fn new() -> Self {
        FirewallHashTable {
            buckets: [[None; 4]; 64],
            rule_count: 0,
            hits: 0,
            misses: 0,
            collisions: 0,
        }
    }

    /// Insert rule with hash-based indexing
    pub fn insert(&mut self, rule_id: u32, port: u16) -> bool {
        if self.rule_count >= 256 {
            return false; // Table full
        }

        let bucket_idx = (port as usize) % 64;
        let mut inserted = false;

        for slot in 0..4 {
            if self.buckets[bucket_idx][slot].is_none() {
                self.buckets[bucket_idx][slot] = Some(rule_id);
                inserted = true;
                if slot > 0 {
                    self.collisions += 1;
                }
                break;
            }
        }

        if inserted {
            self.rule_count += 1;
        }

        inserted
    }

    /// Lookup rule by port (O(1) average case)
    pub fn lookup(&mut self, port: u16) -> Option<u32> {
        let bucket_idx = (port as usize) % 64;

        for rule_id in &self.buckets[bucket_idx] {
            if rule_id.is_some() {
                self.hits += 1;
                return *rule_id;
            }
        }

        self.misses += 1;
        None
    }

    pub fn get_statistics(&self) -> (u32, u32, u32, u32) {
        (self.rule_count, self.hits, self.misses, self.collisions)
    }
}

/// Ring buffer for metrics (zero-copy export)
pub struct MetricsRingBuffer {
    buffer: [u64; 512],  // 512 metric samples
    write_idx: usize,
    read_idx: usize,
    count: u32,
    max_value: u64,
    min_value: u64,
}

impl MetricsRingBuffer {
    pub fn new() -> Self {
        MetricsRingBuffer {
            buffer: [0; 512],
            write_idx: 0,
            read_idx: 0,
            count: 0,
            max_value: 0,
            min_value: u64::MAX,
        }
    }

    /// Write metric sample (O(1) lock-free operation)
    pub fn write_sample(&mut self, value: u64) {
        self.buffer[self.write_idx] = value;
        self.write_idx = (self.write_idx + 1) % 512;

        if self.count < 512 {
            self.count += 1;
        } else {
            self.read_idx = (self.read_idx + 1) % 512;
        }

        if value > self.max_value {
            self.max_value = value;
        }
        if value < self.min_value {
            self.min_value = value;
        }
    }

    /// Get all samples for export (zero-copy reference)
    pub fn get_samples(&self) -> &[u64] {
        if self.count < 512 {
            &self.buffer[0..self.count as usize]
        } else {
            &self.buffer
        }
    }

    /// Calculate statistics without copying
    pub fn calculate_stats(&self) -> (u64, u64, u64) {
        let samples = self.get_samples();
        if samples.is_empty() {
            return (0, 0, 0);
        }

        let mut sum: u64 = 0;
        for &sample in samples {
            sum += sample;
        }

        let avg = sum / samples.len() as u64;
        (self.min_value, avg, self.max_value)
    }

    pub fn get_count(&self) -> u32 {
        self.count
    }
}

/// Per-VM capability cache for O(1) lookups
pub struct CapabilityCache {
    cache: [u32; 64],  // Bitmask of capabilities per VM (0-63)
    vm_count: u32,
    hits: u32,
    misses: u32,
    invalidations: u32,
}

impl CapabilityCache {
    pub fn new() -> Self {
        CapabilityCache {
            cache: [0; 64],
            vm_count: 0,
            hits: 0,
            misses: 0,
            invalidations: 0,
        }
    }

    /// Register VM with capability bitmask
    /// Bit positions: 0=NET, 1=DISK_R, 2=DISK_W, 3=GPU, 4=INPUT, 5=CONSOLE, 6=AUDIT, 7=ADMIN
    pub fn register_vm(&mut self, vm_id: usize, capabilities: u32) -> bool {
        if vm_id >= 64 {
            return false;
        }

        self.cache[vm_id] = capabilities;
        if vm_id as u32 >= self.vm_count {
            self.vm_count = vm_id as u32 + 1;
        }

        true
    }

    /// Check capability (O(1) bitmask operation)
    pub fn has_capability(&mut self, vm_id: usize, cap_bit: u8) -> bool {
        if vm_id >= 64 || cap_bit >= 8 {
            self.misses += 1;
            return false;
        }

        let has_cap = (self.cache[vm_id] & (1 << cap_bit)) != 0;
        if has_cap {
            self.hits += 1;
        } else {
            self.misses += 1;
        }

        has_cap
    }

    /// Invalidate VM capability cache
    pub fn invalidate_vm(&mut self, vm_id: usize) -> bool {
        if vm_id >= 64 {
            return false;
        }

        self.cache[vm_id] = 0;
        self.invalidations += 1;
        true
    }

    pub fn get_statistics(&self) -> (u32, u32, u32, u32) {
        (self.vm_count, self.hits, self.misses, self.invalidations)
    }
}

/// Fast-path firewall rule matcher
pub struct FastPathFirewall {
    hash_table: FirewallHashTable,
    deny_list: [u16; 64],  // Quick deny for blocked ports
    deny_count: u32,
    lookups: u32,
    fast_path_hits: u32,
}

impl FastPathFirewall {
    pub fn new() -> Self {
        FastPathFirewall {
            hash_table: FirewallHashTable::new(),
            deny_list: [0; 64],
            deny_count: 0,
            lookups: 0,
            fast_path_hits: 0,
        }
    }

    /// Check if port is denied (fast path)
    pub fn check_fast_deny(&mut self, port: u16) -> bool {
        self.lookups += 1;

        for &denied_port in &self.deny_list[0..self.deny_count as usize] {
            if denied_port == port {
                self.fast_path_hits += 1;
                return true; // Denied
            }
        }

        false // Not in deny list
    }

    /// Check port in hash table (slower path)
    pub fn check_hash_table(&mut self, port: u16) -> Option<u32> {
        self.hash_table.lookup(port)
    }

    /// Add port to deny list
    pub fn add_deny(&mut self, port: u16) -> bool {
        if self.deny_count >= 64 {
            return false;
        }

        self.deny_list[self.deny_count as usize] = port;
        self.deny_count += 1;
        true
    }

    pub fn get_performance_stats(&self) -> (u32, u32, f32) {
        let hit_rate = if self.lookups > 0 {
            (self.fast_path_hits as f32 / self.lookups as f32) * 100.0
        } else {
            0.0
        };

        (self.lookups, self.fast_path_hits, hit_rate)
    }
}

/// Latency profiler for operation timing
#[derive(Debug, Clone, Copy)]
pub struct LatencyMeasurement {
    pub operation: &'static str,
    pub duration_us: u32,
    pub timestamp_us: u64,
}

pub struct LatencyProfiler {
    measurements: [LatencyMeasurement; 256],
    index: usize,
    total_ops: u32,
    min_latency_us: u32,
    max_latency_us: u32,
}

impl LatencyProfiler {
    pub fn new() -> Self {
        LatencyProfiler {
            measurements: [
                LatencyMeasurement {
                    operation: "unknown",
                    duration_us: 0,
                    timestamp_us: 0,
                };
                256
            ],
            index: 0,
            total_ops: 0,
            min_latency_us: u32::MAX,
            max_latency_us: 0,
        }
    }

    /// Record operation latency
    pub fn record(&mut self, operation: &'static str, duration_us: u32) {
        self.measurements[self.index] = LatencyMeasurement {
            operation,
            duration_us,
            timestamp_us: 0,
        };

        self.index = (self.index + 1) % 256;
        self.total_ops += 1;

        if duration_us < self.min_latency_us {
            self.min_latency_us = duration_us;
        }
        if duration_us > self.max_latency_us {
            self.max_latency_us = duration_us;
        }
    }

    /// Calculate average latency
    pub fn calculate_average(&self) -> u32 {
        if self.total_ops == 0 {
            return 0;
        }

        let sample_count = self.total_ops.min(256);
        let mut sum: u64 = 0;

        for i in 0..sample_count as usize {
            sum += self.measurements[i].duration_us as u64;
        }

        (sum / sample_count as u64) as u32
    }

    pub fn get_statistics(&self) -> (u32, u32, u32, u32) {
        (
            self.total_ops,
            self.min_latency_us,
            self.calculate_average(),
            self.max_latency_us,
        )
    }

    pub fn get_measurements(&self) -> &[LatencyMeasurement] {
        let count = self.total_ops.min(256) as usize;
        &self.measurements[0..count]
    }
}

/// Performance optimization engine
pub struct PerformanceOptimizer {
    firewall_hash: FastPathFirewall,
    capability_cache: CapabilityCache,
    metrics_buffer: MetricsRingBuffer,
    latency_profiler: LatencyProfiler,
    optimization_level: u8,  // 0-3 (higher = more aggressive)
}

impl PerformanceOptimizer {
    pub fn new(opt_level: u8) -> Self {
        PerformanceOptimizer {
            firewall_hash: FastPathFirewall::new(),
            capability_cache: CapabilityCache::new(),
            metrics_buffer: MetricsRingBuffer::new(),
            latency_profiler: LatencyProfiler::new(),
            optimization_level: opt_level.min(3),
        }
    }

    pub fn set_optimization_level(&mut self, level: u8) {
        self.optimization_level = level.min(3);
    }

    pub fn get_firewall_mut(&mut self) -> &mut FastPathFirewall {
        &mut self.firewall_hash
    }

    pub fn get_capability_cache_mut(&mut self) -> &mut CapabilityCache {
        &mut self.capability_cache
    }

    pub fn get_metrics_buffer_mut(&mut self) -> &mut MetricsRingBuffer {
        &mut self.metrics_buffer
    }

    pub fn get_latency_profiler_mut(&mut self) -> &mut LatencyProfiler {
        &mut self.latency_profiler
    }

    pub fn get_optimization_level(&self) -> u8 {
        self.optimization_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firewall_hash_table() {
        let mut table = FirewallHashTable::new();

        assert!(table.insert(1, 443));
        assert!(table.insert(2, 80));
        assert!(table.insert(3, 22));

        assert_eq!(table.lookup(443), Some(1));
        assert_eq!(table.lookup(80), Some(2));
        assert_eq!(table.lookup(22), Some(3));
        assert_eq!(table.lookup(8080), None);

        let (rules, hits, misses, _) = table.get_statistics();
        assert_eq!(rules, 3);
        assert_eq!(hits, 3);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_metrics_ring_buffer() {
        let mut buffer = MetricsRingBuffer::new();

        buffer.write_sample(100);
        buffer.write_sample(200);
        buffer.write_sample(150);

        let samples = buffer.get_samples();
        assert_eq!(samples[0], 100);
        assert_eq!(samples[1], 200);
        assert_eq!(samples[2], 150);

        let (min, avg, max) = buffer.calculate_stats();
        assert_eq!(min, 100);
        assert_eq!(max, 200);
        assert!(avg >= 100 && avg <= 200);
    }

    #[test]
    fn test_capability_cache() {
        let mut cache = CapabilityCache::new();

        // VM 1000 has NETWORK and DISK_READ
        assert!(cache.register_vm(0, 0b00000011));
        assert!(cache.has_capability(0, 0)); // NETWORK
        assert!(cache.has_capability(0, 1)); // DISK_READ
        assert!(!cache.has_capability(0, 3)); // GPU

        let (vms, hits, misses, _) = cache.get_statistics();
        assert_eq!(vms, 1);
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_fast_path_firewall() {
        let mut fw = FastPathFirewall::new();

        fw.add_deny(445);
        fw.add_deny(135);

        assert!(fw.check_fast_deny(445)); // Fast path hit
        assert!(fw.check_fast_deny(135)); // Fast path hit
        assert!(!fw.check_fast_deny(443)); // Fast path miss

        let (lookups, hits, hit_rate) = fw.get_performance_stats();
        assert_eq!(lookups, 3);
        assert_eq!(hits, 2);
        assert!(hit_rate > 60.0 && hit_rate < 70.0);
    }

    #[test]
    fn test_latency_profiler() {
        let mut profiler = LatencyProfiler::new();

        profiler.record("policy_check", 50);
        profiler.record("firewall_match", 100);
        profiler.record("policy_check", 60);

        let (ops, min_lat, avg_lat, max_lat) = profiler.get_statistics();
        assert_eq!(ops, 3);
        assert_eq!(min_lat, 50);
        assert_eq!(max_lat, 100);
        assert!(avg_lat >= 50 && avg_lat <= 100);
    }

    #[test]
    fn test_performance_optimizer() {
        let mut optimizer = PerformanceOptimizer::new(2);

        assert_eq!(optimizer.get_optimization_level(), 2);

        optimizer.get_capability_cache_mut().register_vm(0, 0xFF);
        assert!(optimizer.get_capability_cache_mut().has_capability(0, 0));
    }
}
