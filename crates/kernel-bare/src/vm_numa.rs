// NUMA-Aware Memory Optimization
// NUMA node management, locality optimization, and memory affinity

use core::fmt;

// NUMA node type
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NumaNodeType {
    Local = 0,      // Local to CPU
    Remote = 1,     // Remote NUMA node
    Faraway = 2,    // Very distant NUMA node
}

impl fmt::Display for NumaNodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "Local"),
            Self::Remote => write!(f, "Remote"),
            Self::Faraway => write!(f, "Faraway"),
        }
    }
}

// NUMA node information
#[derive(Copy, Clone, Debug)]
pub struct NumaNode {
    pub node_id: u32,              // Node identifier (0-7)
    pub total_memory_mb: u32,      // Total memory on node
    pub available_memory_mb: u32,  // Available memory
    pub allocated_memory_mb: u32,  // Currently allocated
    pub cpus_mask: u32,            // CPU affinity mask
    pub latency_ns: u32,           // Latency to local CPU
    pub bandwidth_gbps: u16,       // Bandwidth in GB/s
    pub vm_count: u32,             // VMs using this node
    pub access_count: u64,         // Total memory accesses
    pub cache_misses: u32,         // Last-level cache misses
}

impl NumaNode {
    pub fn new(node_id: u32, total_memory_mb: u32) -> Self {
        Self {
            node_id,
            total_memory_mb,
            available_memory_mb: total_memory_mb,
            allocated_memory_mb: 0,
            cpus_mask: 1 << node_id, // CPU affinity to local CPUs
            latency_ns: 50,          // Base latency
            bandwidth_gbps: 40,      // Base bandwidth
            vm_count: 0,
            access_count: 0,
            cache_misses: 0,
        }
    }

    pub fn can_allocate(&self, size_mb: u32) -> bool {
        size_mb <= self.available_memory_mb
    }

    pub fn allocate(&mut self, size_mb: u32) -> bool {
        if self.can_allocate(size_mb) {
            self.available_memory_mb -= size_mb;
            self.allocated_memory_mb += size_mb;
            true
        } else {
            false
        }
    }

    pub fn deallocate(&mut self, size_mb: u32) {
        self.allocated_memory_mb = self.allocated_memory_mb.saturating_sub(size_mb);
        self.available_memory_mb = self.available_memory_mb.saturating_add(size_mb);
    }

    pub fn get_utilization_percent(&self) -> u8 {
        if self.total_memory_mb == 0 {
            return 0;
        }
        ((self.allocated_memory_mb as u64 * 100) / self.total_memory_mb as u64) as u8
    }
}

// Memory page affinity tracking
#[derive(Copy, Clone, Debug)]
pub struct MemoryPageAffinity {
    pub page_num: u32,             // Page number
    pub numa_node: u8,             // Current NUMA node
    pub access_count: u32,         // Accesses to this page
    pub remote_access_count: u32,  // Remote accesses
    pub migration_pending: bool,   // Pending migration
    pub last_access_node: u8,      // Last access source
}

impl MemoryPageAffinity {
    pub fn new(page_num: u32, numa_node: u8) -> Self {
        Self {
            page_num,
            numa_node,
            access_count: 0,
            remote_access_count: 0,
            migration_pending: false,
            last_access_node: numa_node,
        }
    }

    pub fn record_access(&mut self, accessing_node: u8) {
        self.access_count += 1;
        self.last_access_node = accessing_node;

        if accessing_node != self.numa_node {
            self.remote_access_count += 1;
        }
    }

    pub fn should_migrate(&self) -> bool {
        if self.access_count < 10 {
            return false; // Wait for enough accesses
        }

        // Migrate if > 50% accesses from remote
        let remote_ratio = (self.remote_access_count as u64 * 100) / self.access_count as u64;
        remote_ratio > 50
    }
}

// Virtual machine memory configuration
#[derive(Copy, Clone, Debug)]
pub struct VmMemoryConfig {
    pub vm_id: u32,                // VM identifier
    pub total_memory_mb: u32,      // Total memory allocated
    pub numa_node: u8,             // Preferred NUMA node
    pub allow_remote: bool,        // Allow remote allocation
    pub locality_score: u16,       // 0-1000, how local memory is
    pub swap_pages: u32,           // Pages in swap
    pub page_faults: u32,          // Page faults count
    pub huge_pages: u16,           // 2MB huge pages count
}

impl VmMemoryConfig {
    pub fn new(vm_id: u32, total_memory_mb: u32) -> Self {
        Self {
            vm_id,
            total_memory_mb,
            numa_node: 0,
            allow_remote: true,
            locality_score: 1000,
            swap_pages: 0,
            page_faults: 0,
            huge_pages: 0,
        }
    }
}

// Memory optimization policy
#[derive(Copy, Clone, Debug)]
pub struct MemoryOptimizationPolicy {
    pub vm_id: u32,                    // Target VM
    pub policy_id: u32,                // Policy identifier
    pub enable_numa_affinity: bool,    // Enable NUMA awareness
    pub enable_kswapd: bool,           // Enable background swap
    pub enable_huge_pages: bool,       // Enable THP
    pub enable_page_migration: bool,   // Enable automatic migration
    pub memory_pressure_threshold: u8, // Trigger optimization at X%
    pub page_migration_threshold: u8,  // Migrate if X% remote accesses
}

impl MemoryOptimizationPolicy {
    pub fn new(vm_id: u32, policy_id: u32) -> Self {
        Self {
            vm_id,
            policy_id,
            enable_numa_affinity: true,
            enable_kswapd: true,
            enable_huge_pages: true,
            enable_page_migration: true,
            memory_pressure_threshold: 80,
            page_migration_threshold: 60,
        }
    }
}

// Central NUMA and memory manager
pub struct NumaMemoryManager {
    numa_nodes: [Option<NumaNode>; 8],          // Up to 8 NUMA nodes
    vm_memory_configs: [Option<VmMemoryConfig>; 16], // VM memory configs
    memory_policies: [Option<MemoryOptimizationPolicy>; 16], // Optimization policies
    page_affinities: [Option<MemoryPageAffinity>; 256], // Track page affinities
    active_nodes: u32,
    active_vms: u32,
    total_system_memory_mb: u32,
    pages_migrated_total: u64,
}

impl NumaMemoryManager {
    pub const fn new() -> Self {
        const NONE_NODE: Option<NumaNode> = None;
        const NONE_CONFIG: Option<VmMemoryConfig> = None;
        const NONE_POLICY: Option<MemoryOptimizationPolicy> = None;
        const NONE_AFFINITY: Option<MemoryPageAffinity> = None;

        Self {
            numa_nodes: [NONE_NODE; 8],
            vm_memory_configs: [NONE_CONFIG; 16],
            memory_policies: [NONE_POLICY; 16],
            page_affinities: [NONE_AFFINITY; 256],
            active_nodes: 0,
            active_vms: 0,
            total_system_memory_mb: 0,
            pages_migrated_total: 0,
        }
    }

    pub fn add_numa_node(&mut self, node_id: u32, total_memory_mb: u32) -> bool {
        if node_id >= 8 || self.numa_nodes[node_id as usize].is_some() {
            return false;
        }

        let node = NumaNode::new(node_id, total_memory_mb);
        self.numa_nodes[node_id as usize] = Some(node);
        self.active_nodes += 1;
        self.total_system_memory_mb += total_memory_mb;
        true
    }

    pub fn configure_vm_memory(&mut self, vm_id: u32, total_memory_mb: u32, preferred_node: u8) -> bool {
        if preferred_node >= 8 || self.numa_nodes[preferred_node as usize].is_none() {
            return false;
        }

        let mut config = VmMemoryConfig::new(vm_id, total_memory_mb);
        config.numa_node = preferred_node;

        // Try to allocate from preferred node first
        if let Some(node) = self.numa_nodes[preferred_node as usize].as_mut() {
            if node.allocate(total_memory_mb) {
                node.vm_count += 1;

                for i in 0..16 {
                    if self.vm_memory_configs[i].is_none() {
                        self.vm_memory_configs[i] = Some(config);
                        self.active_vms += 1;
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn create_optimization_policy(&mut self, vm_id: u32, policy_id: u32) -> bool {
        let policy = MemoryOptimizationPolicy::new(vm_id, policy_id);

        for i in 0..16 {
            if self.memory_policies[i].is_none() {
                self.memory_policies[i] = Some(policy);
                return true;
            }
        }

        false
    }

    pub fn record_page_access(&mut self, page_num: u32, accessing_node: u8, numa_node: u8) {
        if page_num >= 256 {
            return;
        }

        if let Some(affinity) = self.page_affinities[page_num as usize].as_mut() {
            affinity.record_access(accessing_node);
        } else if self.page_affinities[page_num as usize].is_none() {
            let mut affinity = MemoryPageAffinity::new(page_num, numa_node);
            affinity.record_access(accessing_node);
            self.page_affinities[page_num as usize] = Some(affinity);
        }
    }

    pub fn migrate_page(&mut self, page_num: u32, target_node: u8) -> bool {
        if page_num >= 256 || target_node >= 8 {
            return false;
        }

        if let Some(affinity) = self.page_affinities[page_num as usize].as_mut() {
            let source_node = affinity.numa_node;

            // Deallocate from source
            if let Some(src) = self.numa_nodes[source_node as usize].as_mut() {
                src.deallocate(4); // 4KB page
            }

            // Allocate on target
            if let Some(tgt) = self.numa_nodes[target_node as usize].as_mut() {
                if tgt.allocate(4) {
                    affinity.numa_node = target_node;
                    affinity.migration_pending = false;
                    self.pages_migrated_total += 1;
                    return true;
                }
            }
        }

        false
    }

    pub fn get_node_stats(&self, node_id: u32) -> Option<(u32, u32, u32, u8)> {
        if node_id >= 8 {
            return None;
        }

        if let Some(node) = self.numa_nodes[node_id as usize] {
            return Some((node.allocated_memory_mb, node.available_memory_mb, node.vm_count, node.get_utilization_percent()));
        }

        None
    }

    pub fn get_vm_memory_locality(&self, vm_id: u32) -> Option<(u8, u16)> {
        for config in self.vm_memory_configs.iter() {
            if let Some(c) = config {
                if c.vm_id == vm_id {
                    return Some((c.numa_node, c.locality_score));
                }
            }
        }

        None
    }

    pub fn optimize_memory_placement(&mut self) -> u32 {
        let mut migrations_triggered = 0;

        for page_num in 0..256 {
            if let Some(affinity) = self.page_affinities[page_num].as_mut() {
                if affinity.should_migrate() {
                    // Find best target node based on access pattern
                    let target_node = affinity.last_access_node;
                    if self.migrate_page(page_num as u32, target_node) {
                        migrations_triggered += 1;
                    }
                }
            }
        }

        migrations_triggered
    }

    pub fn get_system_stats(&self) -> (u32, u32, u32, u64) {
        (self.active_nodes, self.active_vms, self.total_system_memory_mb, self.pages_migrated_total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_node_creation() {
        let node = NumaNode::new(0, 8192);
        assert_eq!(node.node_id, 0);
        assert_eq!(node.total_memory_mb, 8192);
        assert_eq!(node.available_memory_mb, 8192);
    }

    #[test]
    fn test_numa_allocation() {
        let mut node = NumaNode::new(0, 8192);
        assert!(node.allocate(2048));
        assert_eq!(node.allocated_memory_mb, 2048);
        assert_eq!(node.available_memory_mb, 6144);
    }

    #[test]
    fn test_numa_overallocation() {
        let mut node = NumaNode::new(0, 8192);
        assert!(node.allocate(8192));
        assert!(!node.allocate(1));
    }

    #[test]
    fn test_page_affinity_tracking() {
        let mut affinity = MemoryPageAffinity::new(0, 0);
        affinity.record_access(0);
        assert_eq!(affinity.access_count, 1);
        assert_eq!(affinity.remote_access_count, 0);

        affinity.record_access(1);
        assert_eq!(affinity.access_count, 2);
        assert_eq!(affinity.remote_access_count, 1);
    }

    #[test]
    fn test_page_migration_decision() {
        let mut affinity = MemoryPageAffinity::new(0, 0);

        // Record 10 accesses: 6 remote, 4 local
        for _ in 0..6 {
            affinity.record_access(1); // Remote
        }
        for _ in 0..4 {
            affinity.record_access(0); // Local
        }

        assert!(affinity.should_migrate());
    }

    #[test]
    fn test_manager_numa_nodes() {
        let mut manager = NumaMemoryManager::new();

        for i in 0..4 {
            let added = manager.add_numa_node(i, 4096);
            assert!(added);
        }

        let (nodes, _, total_mem, _) = manager.get_system_stats();
        assert_eq!(nodes, 4);
        assert_eq!(total_mem, 16384);
    }

    #[test]
    fn test_vm_memory_configuration() {
        let mut manager = NumaMemoryManager::new();
        manager.add_numa_node(0, 8192);
        manager.add_numa_node(1, 8192);

        let configured = manager.configure_vm_memory(100, 2048, 0);
        assert!(configured);

        let (_, vms, _, _) = manager.get_system_stats();
        assert_eq!(vms, 1);
    }

    #[test]
    fn test_optimization_policy() {
        let mut manager = NumaMemoryManager::new();
        let created = manager.create_optimization_policy(100, 1);
        assert!(created);
    }

    #[test]
    fn test_page_migration() {
        let mut manager = NumaMemoryManager::new();
        manager.add_numa_node(0, 8192);
        manager.add_numa_node(1, 8192);

        // Record page access
        manager.record_page_access(0, 1, 0);

        let migrated = manager.migrate_page(0, 1);
        assert!(migrated);

        let (_, _, _, total_migrated) = manager.get_system_stats();
        assert_eq!(total_migrated, 1);
    }

    #[test]
    fn test_memory_locality_optimization() {
        let mut manager = NumaMemoryManager::new();

        for i in 0..8 {
            manager.add_numa_node(i, 4096);
        }

        manager.configure_vm_memory(100, 512, 0);

        // Record access patterns
        for page in 0..64 {
            for _ in 0..15 {
                manager.record_page_access(page, 1, 0);
            }
        }

        let migrations = manager.optimize_memory_placement();
        assert!(migrations > 0);
    }

    #[test]
    fn test_node_utilization() {
        let mut node = NumaNode::new(0, 8192);
        assert_eq!(node.get_utilization_percent(), 0);

        node.allocate(4096);
        assert_eq!(node.get_utilization_percent(), 50);

        node.allocate(4096);
        assert_eq!(node.get_utilization_percent(), 100);
    }

    #[test]
    fn test_multiple_vms_numa() {
        let mut manager = NumaMemoryManager::new();

        for i in 0..4 {
            manager.add_numa_node(i, 8192);
        }

        for i in 0..8 {
            let configured = manager.configure_vm_memory(100 + i, 1024, (i % 4) as u8);
            assert!(configured);
        }

        let (_, vms, _, _) = manager.get_system_stats();
        assert_eq!(vms, 8);
    }
}
