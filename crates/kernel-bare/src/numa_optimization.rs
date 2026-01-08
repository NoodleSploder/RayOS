// PHASE 15: Task 1 - NUMA-Aware Memory Access Optimization
// NUMA (Non-Uniform Memory Access) architecture optimization
// Implements locality-aware memory allocation and access tracking
// No-std compatible (bare metal kernel)

/// NUMA node identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NUMANode {
    Node0, Node1, Node2, Node3,
    Node4, Node5, Node6, Node7,
    Node8, Node9, Node10, Node11,
    Node12, Node13, Node14, Node15,
}

impl NUMANode {
    pub fn id(&self) -> usize {
        match self {
            NUMANode::Node0 => 0,
            NUMANode::Node1 => 1,
            NUMANode::Node2 => 2,
            NUMANode::Node3 => 3,
            NUMANode::Node4 => 4,
            NUMANode::Node5 => 5,
            NUMANode::Node6 => 6,
            NUMANode::Node7 => 7,
            NUMANode::Node8 => 8,
            NUMANode::Node9 => 9,
            NUMANode::Node10 => 10,
            NUMANode::Node11 => 11,
            NUMANode::Node12 => 12,
            NUMANode::Node13 => 13,
            NUMANode::Node14 => 14,
            NUMANode::Node15 => 15,
        }
    }
}

/// Memory zone characteristics
#[derive(Debug, Clone, Copy)]
pub struct MemoryZone {
    pub node: NUMANode,
    pub size_mb: usize,
    pub bandwidth_gbps: u32,
    pub latency_ns: u32,
    pub available: usize,
}

impl MemoryZone {
    pub fn new(node: NUMANode, size_mb: usize) -> Self {
        let (bandwidth, latency) = match node.id() % 4 {
            0 => (100, 45),    // Local access
            1 => (80, 65),     // Adjacent node
            2 => (60, 85),     // Far node
            _ => (40, 120),    // Distant node
        };

        Self {
            node,
            size_mb,
            bandwidth_gbps: bandwidth,
            latency_ns: latency,
            available: size_mb,
        }
    }

    pub fn allocate(&mut self, amount: usize) -> bool {
        if self.available >= amount {
            self.available -= amount;
            true
        } else {
            false
        }
    }

    pub fn deallocate(&mut self, amount: usize) {
        self.available = (self.available + amount).min(self.size_mb);
    }
}

/// Memory access pattern characteristics
#[derive(Debug, Clone, Copy)]
pub struct AccessPattern {
    pub read_ratio: u32,        // 0-100
    pub write_ratio: u32,       // 0-100
    pub sequential: bool,
    pub cache_friendly: bool,
    pub access_frequency: u32,  // accesses per second
}

/// NUMA locality policies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalityPolicy {
    LocalFirst,    // Prefer local node memory
    Interleaved,   // Distribute across nodes
    Performance,   // Prioritize bandwidth/latency
}

/// Page placement strategy (fixed-size entry)
#[derive(Debug, Clone, Copy)]
pub struct PagePlacement {
    pub virtual_addr: usize,
    pub physical_addr: usize,
    pub page_size: usize,
    pub node: NUMANode,
    pub accessed_local: u64,
    pub accessed_remote: u64,
}

impl PagePlacement {
    pub fn new(vaddr: usize, paddr: usize, node: NUMANode) -> Self {
        Self {
            virtual_addr: vaddr,
            physical_addr: paddr,
            page_size: 4096,
            node,
            accessed_local: 0,
            accessed_remote: 0,
        }
    }

    pub fn record_access(&mut self, local: bool) {
        if local {
            self.accessed_local = self.accessed_local.saturating_add(1);
        } else {
            self.accessed_remote = self.accessed_remote.saturating_add(1);
        }
    }

    pub fn local_ratio(&self) -> u32 {
        let total = self.accessed_local + self.accessed_remote;
        if total == 0 { 100 } else { ((self.accessed_local * 100) / total) as u32 }
    }
}

/// Latency tracking for remote accesses
#[derive(Debug, Clone, Copy)]
pub struct LatencyTracker {
    pub local_accesses: u64,
    pub remote_accesses: u64,
    pub total_latency_ns: u64,
    pub max_latency_ns: u32,
    pub min_latency_ns: u32,
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self {
            local_accesses: 0,
            remote_accesses: 0,
            total_latency_ns: 0,
            max_latency_ns: 0,
            min_latency_ns: u32::MAX,
        }
    }

    pub fn record_local(&mut self, latency_ns: u32) {
        self.local_accesses = self.local_accesses.saturating_add(1);
        self.total_latency_ns = self.total_latency_ns.saturating_add(latency_ns as u64);
        if latency_ns > self.max_latency_ns {
            self.max_latency_ns = latency_ns;
        }
        if latency_ns < self.min_latency_ns {
            self.min_latency_ns = latency_ns;
        }
    }

    pub fn record_remote(&mut self, latency_ns: u32) {
        self.remote_accesses = self.remote_accesses.saturating_add(1);
        self.total_latency_ns = self.total_latency_ns.saturating_add(latency_ns as u64);
        if latency_ns > self.max_latency_ns {
            self.max_latency_ns = latency_ns;
        }
        if latency_ns < self.min_latency_ns {
            self.min_latency_ns = latency_ns;
        }
    }

    pub fn average_latency_ns(&self) -> u32 {
        let total = self.local_accesses.saturating_add(self.remote_accesses);
        if total == 0 { 0 } else { (self.total_latency_ns / total) as u32 }
    }

    pub fn remote_penalty(&self) -> u32 {
        if self.remote_accesses == 0 { 0 } else {
            let remote_avg = self.total_latency_ns / self.remote_accesses;
            if remote_avg > 45 { (remote_avg - 45) as u32 } else { 0 }
        }
    }
}

/// NUMA-aware memory manager (fixed-size, no-std)
pub struct NUMAManager {
    zones: [MemoryZone; 16],
    pages: [PagePlacement; 256],
    page_count: usize,
    policy: LocalityPolicy,
    tracker: LatencyTracker,
}

impl NUMAManager {
    pub fn new(policy: LocalityPolicy) -> Self {
        let nodes = [
            NUMANode::Node0, NUMANode::Node1, NUMANode::Node2, NUMANode::Node3,
            NUMANode::Node4, NUMANode::Node5, NUMANode::Node6, NUMANode::Node7,
            NUMANode::Node8, NUMANode::Node9, NUMANode::Node10, NUMANode::Node11,
            NUMANode::Node12, NUMANode::Node13, NUMANode::Node14, NUMANode::Node15,
        ];

        let zones = [
            MemoryZone::new(nodes[0], 1024),
            MemoryZone::new(nodes[1], 1024),
            MemoryZone::new(nodes[2], 1024),
            MemoryZone::new(nodes[3], 1024),
            MemoryZone::new(nodes[4], 1024),
            MemoryZone::new(nodes[5], 1024),
            MemoryZone::new(nodes[6], 1024),
            MemoryZone::new(nodes[7], 1024),
            MemoryZone::new(nodes[8], 1024),
            MemoryZone::new(nodes[9], 1024),
            MemoryZone::new(nodes[10], 1024),
            MemoryZone::new(nodes[11], 1024),
            MemoryZone::new(nodes[12], 1024),
            MemoryZone::new(nodes[13], 1024),
            MemoryZone::new(nodes[14], 1024),
            MemoryZone::new(nodes[15], 1024),
        ];

        Self {
            zones,
            pages: [PagePlacement::new(0, 0, NUMANode::Node0); 256],
            page_count: 0,
            policy,
            tracker: LatencyTracker::new(),
        }
    }

    pub fn allocate(&mut self, size: usize, preferred_node: NUMANode) -> Option<usize> {
        match self.policy {
            LocalityPolicy::LocalFirst => {
                let idx = preferred_node.id();
                if self.zones[idx].allocate(size) {
                    Some(preferred_node.id())
                } else {
                    for i in 0..16 {
                        if i != idx && self.zones[i].allocate(size) {
                            return Some(i);
                        }
                    }
                    None
                }
            }
            LocalityPolicy::Interleaved => {
                for i in 0..16 {
                    if self.zones[i].allocate(size) {
                        return Some(i);
                    }
                }
                None
            }
            LocalityPolicy::Performance => {
                let mut best_idx = preferred_node.id();
                if !self.zones[best_idx].allocate(size) {
                    for i in 0..16 {
                        if self.zones[i].allocate(size) {
                            best_idx = i;
                            break;
                        }
                    }
                }
                Some(best_idx)
            }
        }
    }

    pub fn deallocate(&mut self, node_idx: usize, size: usize) {
        if node_idx < 16 {
            self.zones[node_idx].deallocate(size);
        }
    }

    pub fn record_access(&mut self, page_idx: usize, local: bool, latency_ns: u32) {
        if page_idx < self.page_count && page_idx < 256 {
            self.pages[page_idx].record_access(local);
            if local {
                self.tracker.record_local(latency_ns);
            } else {
                self.tracker.record_remote(latency_ns);
            }
        }
    }

    pub fn migrate_page(&mut self, page_idx: usize, target_node: NUMANode) {
        if page_idx < self.page_count && page_idx < 256 {
            let page = &mut self.pages[page_idx];
            let old_node_idx = page.node.id();

            self.zones[old_node_idx].deallocate(page.page_size);
            page.node = target_node;
            let _ = self.zones[target_node.id()].allocate(page.page_size);
        }
    }

    pub fn get_zone(&self, node: NUMANode) -> MemoryZone {
        self.zones[node.id()]
    }

    pub fn get_tracker(&self) -> LatencyTracker {
        self.tracker
    }

    pub fn get_avg_latency(&self) -> u32 {
        self.tracker.average_latency_ns()
    }

    pub fn get_remote_penalty(&self) -> u32 {
        self.tracker.remote_penalty()
    }

    pub fn get_total_available(&self) -> usize {
        self.zones.iter().map(|z| z.available).sum()
    }
}
// Tests disabled for bare metal - tested via shell interface
// Unit tests can be run via:
// - Manual shell commands: numaopt status, numaopt zones, numaopt policies, etc.
// - Integrated into kernel test suite

