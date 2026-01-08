//! Scalability Layer - Phase 11, Task 6
//! Support for 64+ VMs with hierarchical policies, distributed enforcement, and load balancing
//!
//! Features:
//! - HierarchicalPolicyEngine: Parent/child VM relationships
//! - PolicyDistribution: Broadcast policies to multiple VMs
//! - LoadBalancedFirewall: Distribute firewall decisions across 64 VMs
//! - VM Grouping: Organize VMs into zones with shared policies
//! - Distributed enforcement with per-zone coordination

use core::fmt::Write;

const MAX_VMS: usize = 64;
const MAX_VM_GROUPS: usize = 16;
const MAX_ZONE_RULES: usize = 256;
const MAX_POLICY_BROADCAST: usize = 512;

/// VM zone for policy grouping
#[derive(Clone, Copy, Debug)]
pub struct VmZone {
    pub zone_id: u32,
    pub name: [u8; 32],
    pub name_len: usize,
    pub vm_count: u32,
    pub max_vms: u32,
    pub policy_version: u32,
    pub inherited_from_parent: bool,
}

impl VmZone {
    pub fn new(zone_id: u32) -> Self {
        VmZone {
            zone_id,
            name: [0u8; 32],
            name_len: 0,
            vm_count: 0,
            max_vms: 64,
            policy_version: 0,
            inherited_from_parent: false,
        }
    }

    pub fn can_add_vm(&self) -> bool {
        self.vm_count < self.max_vms
    }
}

/// Hierarchical VM relationship
#[derive(Clone, Copy, Debug)]
pub struct VmHierarchy {
    pub vm_id: u32,
    pub parent_vm_id: u32,  // 0 = no parent
    pub children_count: u32,
    pub policy_inheritance: bool,
    pub zone_id: u32,
    pub depth_level: u8,  // 0 = root, 1 = child, etc.
}

impl VmHierarchy {
    pub fn new(vm_id: u32) -> Self {
        VmHierarchy {
            vm_id,
            parent_vm_id: 0,
            children_count: 0,
            policy_inheritance: true,
            zone_id: 0,
            depth_level: 0,
        }
    }

    pub fn set_parent(&mut self, parent: u32, depth: u8) {
        self.parent_vm_id = parent;
        self.depth_level = depth;
    }
}

/// Zone-level policy rule
#[derive(Clone, Copy, Debug)]
pub struct ZonePolicy {
    pub policy_id: u32,
    pub zone_id: u32,
    pub rule_type: u8,  // 0=firewall, 1=capability, 2=resource
    pub port_start: u16,
    pub port_end: u16,
    pub action: u8,  // 0=allow, 1=deny
    pub priority: u8,
    pub affects_children: bool,
}

impl ZonePolicy {
    pub fn new(zone_id: u32, rule_type: u8) -> Self {
        ZonePolicy {
            policy_id: zone_id ^ (rule_type as u32),
            zone_id,
            rule_type,
            port_start: 0,
            port_end: 65535,
            action: 0,  // Allow
            priority: 128,
            affects_children: true,
        }
    }

    pub fn port_in_range(&self, port: u16) -> bool {
        port >= self.port_start && port <= self.port_end
    }
}

/// Broadcast policy for distribution
#[derive(Clone, Copy, Debug)]
pub struct BroadcastPolicy {
    pub broadcast_id: u32,
    pub source_vm_id: u32,
    pub target_vm_count: u32,
    pub policy_type: u8,
    pub priority: u32,
    pub timestamp_s: u32,
    pub delivered_count: u32,
    pub failed_count: u32,
}

impl BroadcastPolicy {
    pub fn new(source_vm: u32, policy_type: u8) -> Self {
        BroadcastPolicy {
            broadcast_id: source_vm ^ (policy_type as u32),
            source_vm_id: source_vm,
            target_vm_count: 0,
            policy_type,
            priority: 128,
            timestamp_s: 0,
            delivered_count: 0,
            failed_count: 0,
        }
    }

    pub fn success_rate(&self) -> u32 {
        if self.target_vm_count == 0 {
            100
        } else {
            (self.delivered_count * 100) / self.target_vm_count
        }
    }
}

/// Hierarchical policy engine for 64+ VMs
pub struct HierarchicalPolicyEngine {
    hierarchies: [Option<VmHierarchy>; MAX_VMS],
    hierarchy_count: u32,
    zones: [Option<VmZone>; MAX_VM_GROUPS],
    zone_count: u32,
    zone_policies: [Option<ZonePolicy>; MAX_ZONE_RULES],
    zone_policy_count: u32,
    broadcast_queue: [Option<BroadcastPolicy>; MAX_POLICY_BROADCAST],
    broadcast_count: u32,
    broadcast_index: usize,
    total_broadcasts: u32,
    successful_broadcasts: u32,
}

impl HierarchicalPolicyEngine {
    pub fn new() -> Self {
        HierarchicalPolicyEngine {
            hierarchies: [None; MAX_VMS],
            hierarchy_count: 0,
            zones: [None; MAX_VM_GROUPS],
            zone_count: 0,
            zone_policies: [None; MAX_ZONE_RULES],
            zone_policy_count: 0,
            broadcast_queue: [None; MAX_POLICY_BROADCAST],
            broadcast_count: 0,
            broadcast_index: 0,
            total_broadcasts: 0,
            successful_broadcasts: 0,
        }
    }

    /// Register a VM in the hierarchy
    pub fn register_vm(&mut self, vm_id: u32) -> bool {
        if self.hierarchy_count >= MAX_VMS as u32 {
            return false;
        }

        let hierarchy = VmHierarchy::new(vm_id);
        self.hierarchies[self.hierarchy_count as usize] = Some(hierarchy);
        self.hierarchy_count += 1;

        true
    }

    /// Set parent-child relationship
    pub fn set_vm_parent(&mut self, vm_id: u32, parent_id: u32) -> bool {
        // Find VM and update parent
        for i in 0..self.hierarchy_count as usize {
            if let Some(h) = &mut self.hierarchies[i] {
                if h.vm_id == vm_id {
                    h.set_parent(parent_id, 1);
                    // Find parent and increment children count
                    for j in 0..self.hierarchy_count as usize {
                        if let Some(p) = &mut self.hierarchies[j] {
                            if p.vm_id == parent_id {
                                p.children_count += 1;
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Create a new VM zone
    pub fn create_zone(&mut self, zone_id: u32) -> bool {
        if self.zone_count >= MAX_VM_GROUPS as u32 {
            return false;
        }

        let zone = VmZone::new(zone_id);
        self.zones[self.zone_count as usize] = Some(zone);
        self.zone_count += 1;

        true
    }

    /// Add VM to zone
    pub fn add_vm_to_zone(&mut self, vm_id: u32, zone_id: u32) -> bool {
        // Find zone
        for i in 0..self.zone_count as usize {
            if let Some(zone) = &mut self.zones[i] {
                if zone.zone_id == zone_id && zone.can_add_vm() {
                    // Update hierarchy to assign zone
                    for j in 0..self.hierarchy_count as usize {
                        if let Some(h) = &mut self.hierarchies[j] {
                            if h.vm_id == vm_id {
                                h.zone_id = zone_id;
                                zone.vm_count += 1;
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Add zone-level policy rule
    pub fn add_zone_policy(&mut self, policy: ZonePolicy) -> bool {
        if self.zone_policy_count >= MAX_ZONE_RULES as u32 {
            return false;
        }

        self.zone_policies[self.zone_policy_count as usize] = Some(policy);
        self.zone_policy_count += 1;

        true
    }

    /// Check zone policy for port
    pub fn check_zone_policy(&self, zone_id: u32, port: u16) -> bool {
        let mut allowed = true;
        for i in 0..self.zone_policy_count as usize {
            if let Some(policy) = &self.zone_policies[i] {
                if policy.zone_id == zone_id && policy.port_in_range(port) {
                    allowed = policy.action == 0;  // 0=allow, 1=deny
                }
            }
        }
        allowed
    }

    /// Broadcast policy to multiple VMs
    pub fn broadcast_policy(&mut self, source_vm: u32, target_count: u32, policy_type: u8) -> bool {
        if self.broadcast_count >= MAX_POLICY_BROADCAST as u32 {
            return false;
        }

        let mut broadcast = BroadcastPolicy::new(source_vm, policy_type);
        broadcast.target_vm_count = target_count;
        broadcast.delivered_count = target_count; // Assume success for now

        self.broadcast_queue[self.broadcast_index] = Some(broadcast);
        self.broadcast_index = (self.broadcast_index + 1) % MAX_POLICY_BROADCAST;
        self.broadcast_count += 1;
        self.total_broadcasts += 1;
        self.successful_broadcasts += 1;

        true
    }

    /// Get broadcast queue
    pub fn get_broadcast_queue(&self) -> &[Option<BroadcastPolicy>] {
        &self.broadcast_queue
    }

    /// Get statistics
    pub fn get_statistics(&self) -> (u32, u32, u32, u32, u32, u32) {
        (
            self.hierarchy_count,
            self.zone_count,
            self.zone_policy_count,
            self.broadcast_count,
            self.total_broadcasts,
            self.successful_broadcasts,
        )
    }

    /// Get VM count
    pub fn get_vm_count(&self) -> u32 {
        self.hierarchy_count
    }

    /// Get zone count
    pub fn get_zone_count(&self) -> u32 {
        self.zone_count
    }

    /// Check if all VMs have been registered
    pub fn is_full(&self) -> bool {
        self.hierarchy_count >= MAX_VMS as u32
    }

    /// Get policy tree depth
    pub fn get_policy_depth(&self) -> u8 {
        let mut max_depth = 0;
        for i in 0..self.hierarchy_count as usize {
            if let Some(h) = &self.hierarchies[i] {
                if h.depth_level > max_depth {
                    max_depth = h.depth_level;
                }
            }
        }
        max_depth
    }
}

/// Load-balanced firewall for distributed enforcement
pub struct LoadBalancedFirewall {
    vm_partition: [u32; MAX_VMS],  // Maps port to handling VM
    zone_partition: [u32; MAX_VM_GROUPS],  // Maps zone to primary handler
    rules_per_vm: [u32; MAX_VMS],
    lookups_per_vm: [u32; MAX_VMS],
    load_factor: [u32; MAX_VMS],  // Current load (0-100)
    rebalance_count: u32,
}

impl LoadBalancedFirewall {
    pub fn new() -> Self {
        LoadBalancedFirewall {
            vm_partition: [0; MAX_VMS],
            zone_partition: [0; MAX_VM_GROUPS],
            rules_per_vm: [0; MAX_VMS],
            lookups_per_vm: [0; MAX_VMS],
            load_factor: [1; MAX_VMS],  // Start balanced
            rebalance_count: 0,
        }
    }

    /// Assign port to specific VM for load balancing
    pub fn assign_port(&mut self, port: u16, vm_id: u32) -> bool {
        if vm_id as usize >= MAX_VMS {
            return false;
        }

        self.vm_partition[port as usize] = vm_id;
        if vm_id < MAX_VMS as u32 {
            self.rules_per_vm[vm_id as usize] += 1;
        }
        true
    }

    /// Lookup which VM handles this port
    pub fn lookup_handler(&mut self, port: u16) -> u32 {
        let handler = self.vm_partition[port as usize];
        if handler < MAX_VMS as u32 {
            self.lookups_per_vm[handler as usize] += 1;
        }
        handler
    }

    /// Rebalance load across VMs
    pub fn rebalance(&mut self) -> u32 {
        let mut total_load = 0;
        let mut max_load = 0;
        let mut min_load = u32::MAX;

        // Calculate current load
        for i in 0..MAX_VMS {
            total_load += self.lookups_per_vm[i];
            if self.lookups_per_vm[i] > max_load {
                max_load = self.lookups_per_vm[i];
            }
            if self.lookups_per_vm[i] < min_load {
                min_load = self.lookups_per_vm[i];
            }
        }

        // Update load factors
        for i in 0..MAX_VMS {
            if total_load > 0 {
                self.load_factor[i] = (self.lookups_per_vm[i] * 100) / total_load;
            } else {
                self.load_factor[i] = 1;
            }
        }

        self.rebalance_count += 1;

        // Return imbalance metric (max - min)
        if max_load >= min_load {
            max_load - min_load
        } else {
            0
        }
    }

    /// Get load factor for a VM (0-100)
    pub fn get_load_factor(&self, vm_id: u32) -> u32 {
        if vm_id as usize >= MAX_VMS {
            0
        } else {
            self.load_factor[vm_id as usize]
        }
    }

    /// Get statistics
    pub fn get_statistics(&self) -> (u32, u32, u32, u32) {
        let mut total_rules = 0;
        let mut total_lookups = 0;
        for i in 0..MAX_VMS {
            total_rules += self.rules_per_vm[i];
            total_lookups += self.lookups_per_vm[i];
        }
        (total_rules, total_lookups, self.rebalance_count, MAX_VMS as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_zone_capacity() {
        let mut zone = VmZone::new(1);
        assert!(zone.can_add_vm());
        zone.vm_count = 64;
        assert!(!zone.can_add_vm());
    }

    #[test]
    fn test_hierarchical_policy_engine() {
        let mut engine = HierarchicalPolicyEngine::new();
        assert!(engine.register_vm(1000));
        assert!(engine.register_vm(1001));
        assert_eq!(engine.get_vm_count(), 2);
    }

    #[test]
    fn test_vm_parent_child_relationship() {
        let mut engine = HierarchicalPolicyEngine::new();
        engine.register_vm(1000);
        engine.register_vm(1001);
        assert!(engine.set_vm_parent(1001, 1000));

        let (count, _, _, _, _, _) = engine.get_statistics();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_zone_creation_and_assignment() {
        let mut engine = HierarchicalPolicyEngine::new();
        engine.register_vm(1000);
        assert!(engine.create_zone(100));
        assert!(engine.add_vm_to_zone(1000, 100));

        let (_, zone_count, _, _, _, _) = engine.get_statistics();
        assert_eq!(zone_count, 1);
    }

    #[test]
    fn test_zone_policy_rules() {
        let mut engine = HierarchicalPolicyEngine::new();
        engine.create_zone(100);

        let policy = ZonePolicy::new(100, 0);
        assert!(engine.add_zone_policy(policy));

        assert!(engine.check_zone_policy(100, 8080)); // Default allows all

        let (_, _, rule_count, _, _, _) = engine.get_statistics();
        assert_eq!(rule_count, 1);
    }

    #[test]
    fn test_broadcast_policy() {
        let mut engine = HierarchicalPolicyEngine::new();
        assert!(engine.broadcast_policy(1000, 32, 0));
        assert_eq!(engine.total_broadcasts, 1);

        let (_, _, _, bc, tb, sb) = engine.get_statistics();
        assert_eq!(bc, 1);
        assert_eq!(tb, 1);
        assert_eq!(sb, 1);
    }

    #[test]
    fn test_load_balanced_firewall() {
        let mut firewall = LoadBalancedFirewall::new();
        assert!(firewall.assign_port(8080, 0));
        assert_eq!(firewall.lookup_handler(8080), 0);

        let (rules, _, _, _) = firewall.get_statistics();
        assert_eq!(rules, 1);
    }

    #[test]
    fn test_firewall_load_rebalancing() {
        let mut firewall = LoadBalancedFirewall::new();

        // Assign ports to different VMs
        for port in 8000..8064 {
            let vm_id = (port - 8000) % 8;  // Distribute across 8 VMs
            firewall.assign_port(port, vm_id as u32);
        }

        // Simulate lookups
        for _ in 0..100 {
            firewall.lookup_handler(8000);
        }

        let imbalance = firewall.rebalance();
        assert!(imbalance > 0);  // Should detect imbalance
    }

    #[test]
    fn test_policy_depth_tracking() {
        let mut engine = HierarchicalPolicyEngine::new();
        engine.register_vm(1000);
        engine.register_vm(1001);
        engine.register_vm(1002);

        engine.set_vm_parent(1001, 1000);

        assert_eq!(engine.get_policy_depth(), 1);
    }

    #[test]
    fn test_full_scalability_scenario() {
        let mut engine = HierarchicalPolicyEngine::new();

        // Register 64 VMs
        for vm_id in 1000..1064 {
            assert!(engine.register_vm(vm_id));
        }

        assert!(engine.is_full());
        assert_eq!(engine.get_vm_count(), 64);

        // Create zones
        for zone_id in 100..116 {
            assert!(engine.create_zone(zone_id));
        }
        assert_eq!(engine.get_zone_count(), 16);

        // Broadcast policies to all VMs
        assert!(engine.broadcast_policy(1000, 64, 0));
    }
}
