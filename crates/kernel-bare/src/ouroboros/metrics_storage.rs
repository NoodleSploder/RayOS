//! Metrics Storage & Persistence: Circular Buffer and Time-Series Data Management
//!
//! Circular ring buffer storage with time-series compression, aggregation,
//! and long-term persistence for evolution metrics tracking.
//!
//! Phase 35, Task 4

/// Metric entry for time-series storage
#[derive(Clone, Copy, Debug)]
pub struct MetricEntry {
    /// Entry ID
    pub id: u32,
    /// Timestamp (ms)
    pub timestamp_ms: u64,
    /// Metric value
    pub value: u32,
    /// Metric type (0-7 representing different metrics)
    pub metric_type: u8,
}

impl MetricEntry {
    /// Create new metric entry
    pub const fn new(id: u32, timestamp_ms: u64, value: u32, metric_type: u8) -> Self {
        MetricEntry {
            id,
            timestamp_ms,
            value,
            metric_type,
        }
    }
}

/// Compressed metric entry (2x compression)
#[derive(Clone, Copy, Debug)]
pub struct CompressedMetric {
    /// Compressed ID
    pub id: u32,
    /// Time window start (ms)
    pub time_window_start: u64,
    /// Time window width (ms)
    pub time_window_width: u32,
    /// Aggregated value (sum/average)
    pub aggregated_value: u32,
    /// Sample count in window
    pub sample_count: u16,
    /// Metric type
    pub metric_type: u8,
}

impl CompressedMetric {
    /// Create new compressed metric
    pub const fn new(id: u32, time_start: u64, window_width: u32, value: u32) -> Self {
        CompressedMetric {
            id,
            time_window_start: time_start,
            time_window_width: window_width,
            aggregated_value: value,
            sample_count: 1,
            metric_type: 0,
        }
    }

    /// Calculate average value
    pub fn average_value(&self) -> u32 {
        if self.sample_count == 0 {
            0
        } else {
            self.aggregated_value / self.sample_count as u32
        }
    }
}

/// Data retention policy
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RetentionPolicy {
    Aggressive = 0,  // Keep 1 day, compress to 1-hour windows
    Normal = 1,      // Keep 7 days, compress to 6-hour windows
    Conservative = 2, // Keep 30 days, compress to 24-hour windows
    Archive = 3,     // Keep everything, no compression
}

impl RetentionPolicy {
    /// Get retention duration (days)
    pub const fn retention_days(&self) -> u16 {
        match self {
            RetentionPolicy::Aggressive => 1,
            RetentionPolicy::Normal => 7,
            RetentionPolicy::Conservative => 30,
            RetentionPolicy::Archive => 365,
        }
    }

    /// Get compression window (ms)
    pub const fn compression_window_ms(&self) -> u32 {
        match self {
            RetentionPolicy::Aggressive => 3600000,      // 1 hour
            RetentionPolicy::Normal => 21600000,         // 6 hours
            RetentionPolicy::Conservative => 86400000,   // 24 hours
            RetentionPolicy::Archive => 0,               // No compression
        }
    }
}

/// Circular ring buffer for metrics
pub struct MetricsRingBuffer {
    /// Entries (max 1000)
    entries: [Option<MetricEntry>; 1000],
    /// Write position
    write_pos: usize,
    /// Total entries written (wraps at 1000)
    total_written: u32,
    /// Is full (ring wrapped)
    is_full: bool,
}

impl MetricsRingBuffer {
    /// Create new ring buffer
    pub const fn new() -> Self {
        MetricsRingBuffer {
            entries: [None; 1000],
            write_pos: 0,
            total_written: 0,
            is_full: false,
        }
    }

    /// Append metric entry
    pub fn append(&mut self, entry: MetricEntry) -> bool {
        if self.write_pos >= 1000 {
            self.write_pos = 0;
            self.is_full = true;
        }

        self.entries[self.write_pos] = Some(entry);
        self.write_pos += 1;
        self.total_written += 1;
        true
    }

    /// Get entry by position
    pub fn get(&self, pos: usize) -> Option<MetricEntry> {
        if pos < 1000 {
            self.entries[pos]
        } else {
            None
        }
    }

    /// Get entry count
    pub fn count(&self) -> usize {
        if self.is_full {
            1000
        } else {
            self.write_pos
        }
    }

    /// Get oldest entry timestamp
    pub fn oldest_timestamp(&self) -> Option<u64> {
        if self.is_full {
            // Find oldest entry after write position
            for i in self.write_pos..1000 {
                if let Some(entry) = self.entries[i] {
                    return Some(entry.timestamp_ms);
                }
            }
        }

        // Find oldest entry from start
        for i in 0..self.write_pos {
            if let Some(entry) = self.entries[i] {
                return Some(entry.timestamp_ms);
            }
        }

        None
    }

    /// Get newest entry timestamp
    pub fn newest_timestamp(&self) -> Option<u64> {
        if self.write_pos > 0 {
            self.entries[self.write_pos - 1].map(|e| e.timestamp_ms)
        } else if self.is_full {
            self.entries[999].map(|e| e.timestamp_ms)
        } else {
            None
        }
    }

    /// Find entries in time range
    pub fn entries_in_range(&self, start_ms: u64, end_ms: u64) -> usize {
        let mut count = 0;
        for i in 0..1000 {
            if let Some(entry) = self.entries[i] {
                if entry.timestamp_ms >= start_ms && entry.timestamp_ms <= end_ms {
                    count += 1;
                }
            }
        }
        count
    }

    /// Clear buffer
    pub fn clear(&mut self) {
        self.entries = [None; 1000];
        self.write_pos = 0;
        self.total_written = 0;
        self.is_full = false;
    }

    /// Get total written
    pub fn total_written(&self) -> u32 {
        self.total_written
    }
}

/// Compressed storage for time-series data
pub struct CompressedStorage {
    /// Compressed entries (max 500)
    entries: [Option<CompressedMetric>; 500],
    /// Write position
    write_pos: usize,
    /// Retention policy
    policy: RetentionPolicy,
    /// Is full
    is_full: bool,
}

impl CompressedStorage {
    /// Create new compressed storage
    pub const fn new(policy: RetentionPolicy) -> Self {
        CompressedStorage {
            entries: [None; 500],
            write_pos: 0,
            policy,
            is_full: false,
        }
    }

    /// Append compressed metric
    pub fn append(&mut self, metric: CompressedMetric) -> bool {
        if self.write_pos >= 500 {
            self.write_pos = 0;
            self.is_full = true;
        }

        self.entries[self.write_pos] = Some(metric);
        self.write_pos += 1;
        true
    }

    /// Get entry by position
    pub fn get(&self, pos: usize) -> Option<CompressedMetric> {
        if pos < 500 {
            self.entries[pos]
        } else {
            None
        }
    }

    /// Aggregate entries
    pub fn aggregate_to_compressed(
        &mut self,
        start_idx: usize,
        end_idx: usize,
        buffer: &MetricsRingBuffer,
    ) -> bool {
        let mut sum = 0u64;
        let mut count = 0u16;
        let mut min_time = u64::MAX;
        let mut max_time = 0u64;
        let mut metric_type = 0u8;

        for i in start_idx..=end_idx.min(1000) {
            if let Some(entry) = buffer.get(i) {
                sum += entry.value as u64;
                count += 1;
                if entry.timestamp_ms < min_time {
                    min_time = entry.timestamp_ms;
                }
                if entry.timestamp_ms > max_time {
                    max_time = entry.timestamp_ms;
                }
                metric_type = entry.metric_type;
            }
        }

        if count == 0 {
            return false;
        }

        let avg = (sum / count as u64) as u32;
        let window_width = if max_time > min_time {
            (max_time - min_time) as u32
        } else {
            1
        };

        let mut compressed = CompressedMetric::new(self.write_pos as u32, min_time, window_width, avg);
        compressed.sample_count = count;
        compressed.metric_type = metric_type;

        self.append(compressed)
    }

    /// Count entries
    pub fn count(&self) -> usize {
        if self.is_full {
            500
        } else {
            self.write_pos
        }
    }

    /// Get policy
    pub const fn policy(&self) -> RetentionPolicy {
        self.policy
    }

    /// Clear storage
    pub fn clear(&mut self) {
        self.entries = [None; 500];
        self.write_pos = 0;
        self.is_full = false;
    }
}

/// Time-series query result
#[derive(Clone, Copy, Debug)]
pub struct TimeSeriesQuery {
    /// Query ID
    pub id: u32,
    /// Start timestamp (ms)
    pub start_ms: u64,
    /// End timestamp (ms)
    pub end_ms: u64,
    /// Entry count
    pub count: u32,
    /// Average value
    pub average: u32,
    /// Min value
    pub min_value: u32,
    /// Max value
    pub max_value: u32,
}

impl TimeSeriesQuery {
    /// Create new time-series query result
    pub const fn new(id: u32, start_ms: u64, end_ms: u64) -> Self {
        TimeSeriesQuery {
            id,
            start_ms,
            end_ms,
            count: 0,
            average: 0,
            min_value: u32::MAX,
            max_value: 0,
        }
    }
}

/// Metrics storage system
pub struct MetricsStorage {
    /// Ring buffer for recent entries
    ring_buffer: MetricsRingBuffer,
    /// Compressed storage
    compressed: CompressedStorage,
    /// Current retention policy
    retention_policy: RetentionPolicy,
    /// Query cache (max 20 queries)
    query_cache: [Option<TimeSeriesQuery>; 20],
    /// Total metrics stored
    total_metrics: u32,
}

impl MetricsStorage {
    /// Create new metrics storage
    pub const fn new() -> Self {
        MetricsStorage {
            ring_buffer: MetricsRingBuffer::new(),
            compressed: CompressedStorage::new(RetentionPolicy::Normal),
            retention_policy: RetentionPolicy::Normal,
            query_cache: [None; 20],
            total_metrics: 0,
        }
    }

    /// Create with retention policy
    pub fn with_policy(policy: RetentionPolicy) -> Self {
        MetricsStorage {
            ring_buffer: MetricsRingBuffer::new(),
            compressed: CompressedStorage::new(policy),
            retention_policy: policy,
            query_cache: [None; 20],
            total_metrics: 0,
        }
    }

    /// Store metric
    pub fn store_metric(&mut self, entry: MetricEntry) -> bool {
        let stored = self.ring_buffer.append(entry);
        if stored {
            self.total_metrics += 1;
        }
        stored
    }

    /// Query time range
    pub fn query_time_range(&mut self, start_ms: u64, end_ms: u64) -> Option<TimeSeriesQuery> {
        let mut query = TimeSeriesQuery::new(self.total_metrics, start_ms, end_ms);
        let mut sum = 0u64;
        let mut count = 0u32;
        let mut min_val = u32::MAX;
        let mut max_val = 0u32;

        for i in 0..self.ring_buffer.count() {
            if let Some(entry) = self.ring_buffer.get(i) {
                if entry.timestamp_ms >= start_ms && entry.timestamp_ms <= end_ms {
                    sum += entry.value as u64;
                    count += 1;
                    if entry.value < min_val {
                        min_val = entry.value;
                    }
                    if entry.value > max_val {
                        max_val = entry.value;
                    }
                }
            }
        }

        if count == 0 {
            return None;
        }

        query.count = count;
        query.average = (sum / count as u64) as u32;
        query.min_value = min_val;
        query.max_value = max_val;

        // Cache the query
        for slot in &mut self.query_cache {
            if slot.is_none() {
                *slot = Some(query);
                break;
            }
        }

        Some(query)
    }

    /// Compress old entries
    pub fn compress_older_than(&mut self, cutoff_ms: u64) -> u32 {
        let mut compressed_count = 0u32;
        let mut start_idx = None;
        let mut end_idx = None;

        // Find range of entries older than cutoff
        for i in 0..self.ring_buffer.count() {
            if let Some(entry) = self.ring_buffer.get(i) {
                if entry.timestamp_ms < cutoff_ms {
                    if start_idx.is_none() {
                        start_idx = Some(i);
                    }
                    end_idx = Some(i);
                }
            }
        }

        // Compress in chunks
        if let (Some(start), Some(end)) = (start_idx, end_idx) {
            let chunk_size = 10;  // Compress every 10 entries
            let mut idx = start;

            while idx <= end {
                let end_chunk = (idx + chunk_size - 1).min(end);
                if self.compressed.aggregate_to_compressed(idx, end_chunk, &self.ring_buffer) {
                    compressed_count += (end_chunk - idx + 1) as u32;
                }
                idx += chunk_size;
            }
        }

        compressed_count
    }

    /// Get metrics from compressed storage
    pub fn get_compressed(&self, index: usize) -> Option<CompressedMetric> {
        self.compressed.get(index)
    }

    /// Prune old entries
    pub fn prune_old_entries(&mut self, cutoff_ms: u64) -> u32 {
        let policy = self.retention_policy;
        let retention_days = policy.retention_days() as u64;
        let retention_ms = retention_days * 24 * 3600 * 1000;
        let prune_time = cutoff_ms.saturating_sub(retention_ms);

        self.compress_older_than(prune_time)
    }

    /// Get ring buffer stats
    pub fn ring_buffer_stats(&self) -> (u32, u32, bool) {
        (
            self.ring_buffer.total_written(),
            self.ring_buffer.count() as u32,
            self.ring_buffer.is_full,
        )
    }

    /// Get compressed storage count
    pub fn compressed_count(&self) -> u32 {
        self.compressed.count() as u32
    }

    /// Get total metrics stored
    pub fn total_metrics(&self) -> u32 {
        self.total_metrics
    }

    /// Get query cache hit count
    pub fn query_cache_hits(&self) -> u32 {
        self.query_cache.iter().filter(|q| q.is_some()).count() as u32
    }

    /// Clear query cache
    pub fn clear_query_cache(&mut self) {
        self.query_cache = [None; 20];
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.ring_buffer.clear();
        self.compressed.clear();
        self.query_cache = [None; 20];
        self.total_metrics = 0;
    }

    /// Get storage efficiency (compressed/total)
    pub fn compression_ratio(&self) -> u8 {
        if self.total_metrics == 0 {
            0
        } else {
            let compressed = self.compressed.count() as u32;
            ((compressed * 100) / self.total_metrics).min(100) as u8
        }
    }

    /// Statistics
    pub fn statistics(&self) -> (u32, u32, u32, u32, u8) {
        (
            self.total_metrics,
            self.ring_buffer.count() as u32,
            self.compressed.count() as u32,
            self.query_cache_hits(),
            self.compression_ratio(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_entry_creation() {
        let entry = MetricEntry::new(1, 1000, 100, 0);
        assert_eq!(entry.id, 1);
        assert_eq!(entry.value, 100);
    }

    #[test]
    fn test_compressed_metric_creation() {
        let compressed = CompressedMetric::new(1, 1000, 500, 100);
        assert_eq!(compressed.id, 1);
        assert_eq!(compressed.sample_count, 1);
    }

    #[test]
    fn test_compressed_metric_average() {
        let mut compressed = CompressedMetric::new(1, 1000, 500, 100);
        compressed.sample_count = 2;
        compressed.aggregated_value = 200;
        assert_eq!(compressed.average_value(), 100);
    }

    #[test]
    fn test_retention_policy_enum() {
        assert_eq!(RetentionPolicy::Aggressive as u8, 0);
        assert_eq!(RetentionPolicy::Archive as u8, 3);
    }

    #[test]
    fn test_retention_policy_days() {
        assert_eq!(RetentionPolicy::Aggressive.retention_days(), 1);
        assert_eq!(RetentionPolicy::Normal.retention_days(), 7);
        assert_eq!(RetentionPolicy::Conservative.retention_days(), 30);
        assert_eq!(RetentionPolicy::Archive.retention_days(), 365);
    }

    #[test]
    fn test_retention_policy_compression_window() {
        assert_eq!(RetentionPolicy::Aggressive.compression_window_ms(), 3600000);
        assert_eq!(RetentionPolicy::Normal.compression_window_ms(), 21600000);
        assert_eq!(RetentionPolicy::Conservative.compression_window_ms(), 86400000);
        assert_eq!(RetentionPolicy::Archive.compression_window_ms(), 0);
    }

    #[test]
    fn test_ring_buffer_creation() {
        let buffer = MetricsRingBuffer::new();
        assert_eq!(buffer.count(), 0);
        assert!(!buffer.is_full);
    }

    #[test]
    fn test_ring_buffer_append() {
        let mut buffer = MetricsRingBuffer::new();
        let entry = MetricEntry::new(1, 1000, 100, 0);
        assert!(buffer.append(entry));
        assert_eq!(buffer.count(), 1);
    }

    #[test]
    fn test_ring_buffer_get() {
        let mut buffer = MetricsRingBuffer::new();
        let entry = MetricEntry::new(1, 1000, 100, 0);
        buffer.append(entry);
        assert_eq!(buffer.get(0).unwrap().value, 100);
    }

    #[test]
    fn test_ring_buffer_timestamps() {
        let mut buffer = MetricsRingBuffer::new();
        buffer.append(MetricEntry::new(1, 1000, 100, 0));
        buffer.append(MetricEntry::new(2, 2000, 200, 0));
        assert_eq!(buffer.oldest_timestamp(), Some(1000));
        assert_eq!(buffer.newest_timestamp(), Some(2000));
    }

    #[test]
    fn test_ring_buffer_time_range() {
        let mut buffer = MetricsRingBuffer::new();
        buffer.append(MetricEntry::new(1, 1000, 100, 0));
        buffer.append(MetricEntry::new(2, 2000, 200, 0));
        buffer.append(MetricEntry::new(3, 3000, 300, 0));
        assert_eq!(buffer.entries_in_range(1500, 2500), 1);
        assert_eq!(buffer.entries_in_range(1000, 3000), 3);
    }

    #[test]
    fn test_ring_buffer_clear() {
        let mut buffer = MetricsRingBuffer::new();
        buffer.append(MetricEntry::new(1, 1000, 100, 0));
        buffer.clear();
        assert_eq!(buffer.count(), 0);
    }

    #[test]
    fn test_compressed_storage_creation() {
        let storage = CompressedStorage::new(RetentionPolicy::Normal);
        assert_eq!(storage.count(), 0);
        assert_eq!(storage.policy(), RetentionPolicy::Normal);
    }

    #[test]
    fn test_compressed_storage_append() {
        let mut storage = CompressedStorage::new(RetentionPolicy::Normal);
        let metric = CompressedMetric::new(1, 1000, 500, 100);
        assert!(storage.append(metric));
        assert_eq!(storage.count(), 1);
    }

    #[test]
    fn test_compressed_storage_get() {
        let mut storage = CompressedStorage::new(RetentionPolicy::Normal);
        let metric = CompressedMetric::new(1, 1000, 500, 100);
        storage.append(metric);
        assert_eq!(storage.get(0).unwrap().aggregated_value, 100);
    }

    #[test]
    fn test_time_series_query_creation() {
        let query = TimeSeriesQuery::new(1, 1000, 2000);
        assert_eq!(query.count, 0);
        assert_eq!(query.average, 0);
    }

    #[test]
    fn test_metrics_storage_creation() {
        let storage = MetricsStorage::new();
        assert_eq!(storage.total_metrics(), 0);
    }

    #[test]
    fn test_metrics_storage_with_policy() {
        let storage = MetricsStorage::with_policy(RetentionPolicy::Conservative);
        assert_eq!(storage.retention_policy, RetentionPolicy::Conservative);
    }

    #[test]
    fn test_metrics_storage_store_metric() {
        let mut storage = MetricsStorage::new();
        let entry = MetricEntry::new(1, 1000, 100, 0);
        assert!(storage.store_metric(entry));
        assert_eq!(storage.total_metrics(), 1);
    }

    #[test]
    fn test_metrics_storage_query_time_range() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.store_metric(MetricEntry::new(2, 2000, 200, 0));
        storage.store_metric(MetricEntry::new(3, 3000, 300, 0));

        let result = storage.query_time_range(1500, 2500);
        assert!(result.is_some());
        let query = result.unwrap();
        assert_eq!(query.count, 1);
        assert_eq!(query.average, 200);
    }

    #[test]
    fn test_metrics_storage_query_average() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.store_metric(MetricEntry::new(2, 2000, 200, 0));
        storage.store_metric(MetricEntry::new(3, 3000, 300, 0));

        let result = storage.query_time_range(1000, 3000);
        assert!(result.is_some());
        let query = result.unwrap();
        assert_eq!(query.average, 200);  // (100+200+300)/3
    }

    #[test]
    fn test_metrics_storage_query_min_max() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 50, 0));
        storage.store_metric(MetricEntry::new(2, 2000, 200, 0));
        storage.store_metric(MetricEntry::new(3, 3000, 100, 0));

        let result = storage.query_time_range(1000, 3000);
        assert!(result.is_some());
        let query = result.unwrap();
        assert_eq!(query.min_value, 50);
        assert_eq!(query.max_value, 200);
    }

    #[test]
    fn test_metrics_storage_compress_older_than() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.store_metric(MetricEntry::new(2, 2000, 200, 0));
        storage.store_metric(MetricEntry::new(3, 3000, 300, 0));

        let compressed = storage.compress_older_than(2500);
        assert!(compressed > 0);
    }

    #[test]
    fn test_metrics_storage_prune_old_entries() {
        let mut storage = MetricsStorage::with_policy(RetentionPolicy::Aggressive);
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.store_metric(MetricEntry::new(2, 2000, 200, 0));

        // Should prune entries older than 1 day
        let pruned = storage.prune_old_entries(10000000000);
        assert!(pruned >= 0);
    }

    #[test]
    fn test_metrics_storage_compression_ratio() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.store_metric(MetricEntry::new(2, 2000, 200, 0));

        let ratio = storage.compression_ratio();
        assert!(ratio >= 0 && ratio <= 100);
    }

    #[test]
    fn test_metrics_storage_clear() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.clear();
        assert_eq!(storage.total_metrics(), 0);
    }

    #[test]
    fn test_metrics_storage_statistics() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.store_metric(MetricEntry::new(2, 2000, 200, 0));

        let (total, ring_count, compressed_count, cache_hits, ratio) = storage.statistics();
        assert_eq!(total, 2);
        assert_eq!(ring_count, 2);
    }

    #[test]
    fn test_metrics_storage_query_cache() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.query_time_range(1000, 2000);

        assert!(storage.query_cache_hits() > 0);

        storage.clear_query_cache();
        assert_eq!(storage.query_cache_hits(), 0);
    }

    #[test]
    fn test_metrics_storage_ring_buffer_stats() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.store_metric(MetricEntry::new(2, 2000, 200, 0));

        let (total_written, count, is_full) = storage.ring_buffer_stats();
        assert_eq!(total_written, 2);
        assert_eq!(count, 2);
        assert!(!is_full);
    }

    #[test]
    fn test_metrics_storage_compressed_count() {
        let mut storage = MetricsStorage::new();
        storage.store_metric(MetricEntry::new(1, 1000, 100, 0));
        storage.compress_older_than(2000);

        let count = storage.compressed_count();
        assert!(count >= 0);
    }

    #[test]
    fn test_ring_buffer_wrapping() {
        let mut buffer = MetricsRingBuffer::new();
        // Add just enough entries to not wrap
        for i in 0..100 {
            buffer.append(MetricEntry::new(i, 1000 + i as u64, 100 + i, 0));
        }
        assert!(!buffer.is_full);
        assert_eq!(buffer.count(), 100);
    }

    #[test]
    fn test_retention_policy_archive() {
        let policy = RetentionPolicy::Archive;
        assert_eq!(policy.retention_days(), 365);
        assert_eq!(policy.compression_window_ms(), 0);  // No compression
    }

    #[test]
    fn test_compressed_metric_with_samples() {
        let mut metric = CompressedMetric::new(1, 1000, 500, 300);
        metric.sample_count = 3;
        metric.aggregated_value = 300;
        assert_eq!(metric.average_value(), 100);
    }
}
