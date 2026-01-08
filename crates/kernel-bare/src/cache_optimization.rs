/// CPU Cache Optimization
///
/// Optimizes CPU cache usage patterns and coherency protocols
/// Supports L1, L2, L3 with multiple replacement policies

use core::cmp::min;

const MAX_CACHE_LINES: usize = 512;
const MAX_CACHE_LEVELS: usize = 3;
const CACHE_LINE_SIZE: usize = 64;

/// CPU cache levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLevel {
    L1,
    L2,
    L3,
}

impl CacheLevel {
    pub fn size_kb(&self) -> u32 {
        match self {
            CacheLevel::L1 => 32,
            CacheLevel::L2 => 256,
            CacheLevel::L3 => 8192,
        }
    }

    pub fn latency_cycles(&self) -> u32 {
        match self {
            CacheLevel::L1 => 4,
            CacheLevel::L2 => 12,
            CacheLevel::L3 => 40,
        }
    }
}

/// Cache line state (MESI-like protocol)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLineState {
    Invalid,
    Shared,
    Exclusive,
    Dirty,
}

/// Individual cache line
#[derive(Debug, Clone, Copy)]
pub struct CacheLine {
    pub address: usize,
    pub state: CacheLineState,
    pub access_count: u32,
    pub last_access_time: u64,
    pub dirty: bool,
}

impl CacheLine {
    pub fn new(address: usize) -> Self {
        Self {
            address,
            state: CacheLineState::Invalid,
            access_count: 0,
            last_access_time: 0,
            dirty: false,
        }
    }

    pub fn record_access(&mut self, time: u64) {
        self.access_count = self.access_count.saturating_add(1);
        self.last_access_time = time;
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.state = CacheLineState::Dirty;
    }

    pub fn is_valid(&self) -> bool {
        self.state != CacheLineState::Invalid
    }
}

/// Cache replacement policies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    LRU,  // Least Recently Used
    LFU,  // Least Frequently Used
    ARC,  // Adaptive Replacement Cache
}

/// Cache layer with policy
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u32,
    pub coherency_events: u32,
    pub prefetch_count: u32,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            hits: 0,
            misses: 0,
            evictions: 0,
            coherency_events: 0,
            prefetch_count: 0,
        }
    }

    pub fn hit_ratio(&self) -> u32 {
        let total = self.hits.saturating_add(self.misses);
        if total == 0 { 0 } else { ((self.hits * 100) / total) as u32 }
    }

    pub fn record_hit(&mut self) {
        self.hits = self.hits.saturating_add(1);
    }

    pub fn record_miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }
}

/// CPU Cache Optimizer
pub struct CacheOptimizer {
    lines: [CacheLine; MAX_CACHE_LINES],
    line_count: u32,
    levels: [CacheLevel; MAX_CACHE_LEVELS],
    policy: CachePolicy,
    stats: CacheStats,
    prefetch_enabled: bool,
    clock_time: u64,
}

impl CacheOptimizer {
    pub fn new(policy: CachePolicy) -> Self {
        Self {
            lines: [CacheLine::new(0); MAX_CACHE_LINES],
            line_count: 0,
            levels: [CacheLevel::L1, CacheLevel::L2, CacheLevel::L3],
            policy,
            stats: CacheStats::new(),
            prefetch_enabled: true,
            clock_time: 0,
        }
    }

    pub fn access(&mut self, address: usize, write: bool) -> bool {
        self.clock_time = self.clock_time.saturating_add(1);

        // Search for address in cache
        for i in 0..(self.line_count as usize) {
            if self.lines[i].address == address && self.lines[i].is_valid() {
                self.lines[i].record_access(self.clock_time);
                if write {
                    self.lines[i].mark_dirty();
                }
                self.stats.record_hit();
                return true;
            }
        }

        // Cache miss
        self.stats.record_miss();

        // Add new line if space available
        if (self.line_count as usize) < MAX_CACHE_LINES {
            let idx = self.line_count as usize;
            self.lines[idx] = CacheLine::new(address);
            self.lines[idx].state = CacheLineState::Exclusive;
            self.lines[idx].record_access(self.clock_time);
            if write {
                self.lines[idx].mark_dirty();
            }
            self.line_count += 1;
            return false;
        }

        // Need to evict - use policy
        self.evict_line();
        false
    }

    fn evict_line(&mut self) {
        if self.line_count == 0 {
            return;
        }

        let target_idx = match self.policy {
            CachePolicy::LRU => self.find_lru(),
            CachePolicy::LFU => self.find_lfu(),
            CachePolicy::ARC => self.find_arc(),
        };

        if target_idx < (self.line_count as usize) {
            // Move last line to target position
            self.lines[target_idx] = self.lines[(self.line_count - 1) as usize];
            self.line_count -= 1;
            self.stats.evictions = self.stats.evictions.saturating_add(1);
        }
    }

    fn find_lru(&self) -> usize {
        let mut min_time = u64::MAX;
        let mut min_idx = 0;

        for i in 0..(self.line_count as usize) {
            if self.lines[i].last_access_time < min_time {
                min_time = self.lines[i].last_access_time;
                min_idx = i;
            }
        }
        min_idx
    }

    fn find_lfu(&self) -> usize {
        let mut min_count = u32::MAX;
        let mut min_idx = 0;

        for i in 0..(self.line_count as usize) {
            if self.lines[i].access_count < min_count {
                min_count = self.lines[i].access_count;
                min_idx = i;
            }
        }
        min_idx
    }

    fn find_arc(&self) -> usize {
        // Simplified ARC: balance between frequency and recency
        let mut min_score = u32::MAX;
        let mut min_idx = 0;

        for i in 0..(self.line_count as usize) {
            let recency_score = (self.clock_time.saturating_sub(self.lines[i].last_access_time)) as u32;
            let score = (recency_score / 2) + (self.lines[i].access_count / 2);

            if score < min_score {
                min_score = score;
                min_idx = i;
            }
        }
        min_idx
    }

    pub fn prefetch(&mut self, address: usize) {
        if !self.prefetch_enabled || (self.line_count as usize) >= MAX_CACHE_LINES {
            return;
        }

        // Check if not already cached
        for i in 0..(self.line_count as usize) {
            if self.lines[i].address == address {
                return;
            }
        }

        // Add prefetched line
        let idx = self.line_count as usize;
        self.lines[idx] = CacheLine::new(address);
        self.lines[idx].state = CacheLineState::Shared;
        self.line_count += 1;
        self.stats.prefetch_count = self.stats.prefetch_count.saturating_add(1);
    }

    pub fn coherency_invalidate(&mut self, address: usize) {
        for i in 0..(self.line_count as usize) {
            if self.lines[i].address == address {
                self.lines[i].state = CacheLineState::Invalid;
                self.stats.coherency_events = self.stats.coherency_events.saturating_add(1);
                break;
            }
        }
    }

    pub fn flush_dirty_lines(&mut self) -> u32 {
        let mut flushed: u32 = 0;
        for i in 0..(self.line_count as usize) {
            if self.lines[i].dirty {
                self.lines[i].dirty = false;
                flushed = flushed.saturating_add(1);
            }
        }
        flushed
    }

    pub fn get_stats(&self) -> CacheStats {
        self.stats
    }

    pub fn get_line_count(&self) -> u32 {
        self.line_count
    }

    pub fn get_policy(&self) -> CachePolicy {
        self.policy
    }

    pub fn enable_prefetch(&mut self, enabled: bool) {
        self.prefetch_enabled = enabled;
    }

    pub fn get_avg_latency(&self) -> u32 {
        let total_accesses = self.stats.hits.saturating_add(self.stats.misses);
        if total_accesses == 0 {
            return 0;
        }

        let l1_latency = (self.stats.hits * 4) / total_accesses;
        let l3_latency = (self.stats.misses * 40) / total_accesses;
        (l1_latency + l3_latency) as u32
    }
}

// Module documentation
// Bare metal compatible cache optimization
// Supports dynamic prefetching and coherency protocol tracking
// Tests run via shell interface: cache [status|policies|stats|help]
