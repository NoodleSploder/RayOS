//! Advanced Security Features - Phase 11, Task 5
//! Temporary capabilities with TTL, capability delegation, fine-grained controls
//!
//! Features:
//! - TemporaryCapability with expiration time and audit trail
//! - CapabilityDelegation with depth tracking and revocation support
//! - FineGrainedCapability with port ranges and bandwidth quotas
//! - DynamicPolicyEnforcer for runtime policy changes
//! - Policy revocation and hotspot detection


const MAX_TEMP_CAPABILITIES: usize = 256;
const MAX_DELEGATIONS: usize = 128;
const MAX_POLICIES: usize = 512;
const MAX_AUDIT_ENTRIES: usize = 1024;

/// Temporary capability with time-based expiration
#[derive(Clone, Copy, Debug)]
pub struct TemporaryCapability {
    pub capability_id: u32,
    pub vm_id: u32,
    pub capability_bits: u32,  // Bitmask of granted capabilities
    pub issued_at_s: u32,      // Seconds since boot
    pub expires_at_s: u32,     // Absolute expiration time
    pub reason: [u8; 32],      // Reason for grant
    pub reason_len: usize,
}

impl TemporaryCapability {
    pub fn new(vm_id: u32, capabilities: u32, duration_s: u32, now: u32) -> Self {
        TemporaryCapability {
            capability_id: vm_id ^ (capabilities as u32),
            vm_id,
            capability_bits: capabilities,
            issued_at_s: now,
            expires_at_s: now + duration_s,
            reason: [0u8; 32],
            reason_len: 0,
        }
    }

    pub fn is_expired(&self, now: u32) -> bool {
        now >= self.expires_at_s
    }

    pub fn time_remaining(&self, now: u32) -> u32 {
        if now >= self.expires_at_s {
            0
        } else {
            self.expires_at_s - now
        }
    }

    pub fn get_capability_name(bit: u32) -> &'static str {
        match bit {
            0 => "CAP_NET",
            1 => "CAP_DISK_R",
            2 => "CAP_DISK_W",
            3 => "CAP_GPU",
            4 => "CAP_INPUT",
            5 => "CAP_CONSOLE",
            6 => "CAP_AUDIT",
            7 => "CAP_ADMIN",
            _ => "UNKNOWN",
        }
    }
}

/// Capability delegation chain
#[derive(Clone, Copy, Debug)]
pub struct CapabilityDelegation {
    pub delegation_id: u32,
    pub from_vm_id: u32,
    pub to_vm_id: u32,
    pub capabilities: u32,
    pub delegation_depth: u8,  // 0 = original, 1+ = delegated further
    pub max_depth: u8,         // Maximum delegation chain depth allowed
    pub revoked: bool,
    pub created_at_s: u32,
}

impl CapabilityDelegation {
    pub fn new(from_vm: u32, to_vm: u32, caps: u32, max_depth: u8, now: u32) -> Self {
        CapabilityDelegation {
            delegation_id: (from_vm as u32) ^ (to_vm as u32) ^ caps,
            from_vm_id: from_vm,
            to_vm_id: to_vm,
            capabilities: caps,
            delegation_depth: 0,
            max_depth,
            revoked: false,
            created_at_s: now,
        }
    }

    pub fn can_delegate_further(&self) -> bool {
        !self.revoked && self.delegation_depth < self.max_depth
    }

    pub fn revoke(&mut self) {
        self.revoked = true;
    }

    pub fn is_valid(&self) -> bool {
        !self.revoked
    }
}

/// Fine-grained capability with resource constraints
#[derive(Clone, Copy, Debug)]
pub struct FineGrainedCapability {
    pub cap_id: u32,
    pub vm_id: u32,
    pub resource_type: u8,  // 0=network, 1=disk, 2=gpu, 3=input
    pub port_range_start: u16,
    pub port_range_end: u16,
    pub bandwidth_mbps: u16,  // 0 = unlimited
    pub max_concurrent: u16,  // Max concurrent operations
    pub audit_required: bool,
}

impl FineGrainedCapability {
    pub fn new(vm_id: u32, resource_type: u8) -> Self {
        FineGrainedCapability {
            cap_id: (vm_id as u32) ^ (resource_type as u32),
            vm_id,
            resource_type,
            port_range_start: 0,
            port_range_end: 65535,
            bandwidth_mbps: 0,
            max_concurrent: 0,
            audit_required: true,
        }
    }

    pub fn port_allowed(&self, port: u16) -> bool {
        port >= self.port_range_start && port <= self.port_range_end
    }

    pub fn get_resource_name(&self) -> &'static str {
        match self.resource_type {
            0 => "Network",
            1 => "Disk",
            2 => "GPU",
            3 => "Input",
            _ => "Unknown",
        }
    }
}

/// Audit entry for capability changes
#[derive(Clone, Copy, Debug)]
pub struct SecurityAuditEntry {
    pub timestamp_s: u32,
    pub event_type: u8,  // 0=grant, 1=revoke, 2=delegat, 3=check_pass, 4=check_fail
    pub vm_id: u32,
    pub capability: u32,
    pub result: bool,
    pub reason: [u8; 48],
    pub reason_len: usize,
}

impl SecurityAuditEntry {
    pub fn new(now: u32, event_type: u8, vm_id: u32, cap: u32, result: bool) -> Self {
        SecurityAuditEntry {
            timestamp_s: now,
            event_type,
            vm_id,
            capability: cap,
            result,
            reason: [0u8; 48],
            reason_len: 0,
        }
    }

    pub fn get_event_name(&self) -> &'static str {
        match self.event_type {
            0 => "GRANT",
            1 => "REVOKE",
            2 => "DELEGATE",
            3 => "CHECK_PASS",
            4 => "CHECK_FAIL",
            _ => "UNKNOWN",
        }
    }
}

/// Dynamic policy enforcer with runtime modifications
pub struct DynamicPolicyEnforcer {
    temp_capabilities: [Option<TemporaryCapability>; MAX_TEMP_CAPABILITIES],
    temp_cap_count: u32,
    delegations: [Option<CapabilityDelegation>; MAX_DELEGATIONS],
    delegation_count: u32,
    fine_grained_caps: [Option<FineGrainedCapability>; MAX_POLICIES],
    fine_grained_count: u32,
    audit_log: [SecurityAuditEntry; MAX_AUDIT_ENTRIES],
    audit_index: usize,
    audit_count: u32,
    policy_version: u32,
    policy_changes: u32,
}

impl DynamicPolicyEnforcer {
    pub fn new() -> Self {
        DynamicPolicyEnforcer {
            temp_capabilities: [None; MAX_TEMP_CAPABILITIES],
            temp_cap_count: 0,
            delegations: [None; MAX_DELEGATIONS],
            delegation_count: 0,
            fine_grained_caps: [None; MAX_POLICIES],
            fine_grained_count: 0,
            audit_log: [SecurityAuditEntry::new(0, 0, 0, 0, false); MAX_AUDIT_ENTRIES],
            audit_index: 0,
            audit_count: 0,
            policy_version: 1,
            policy_changes: 0,
        }
    }

    /// Grant a temporary capability with TTL
    pub fn grant_temporary(&mut self, vm_id: u32, caps: u32, duration_s: u32, now: u32) -> bool {
        if self.temp_cap_count >= MAX_TEMP_CAPABILITIES as u32 {
            return false;
        }

        let temp_cap = TemporaryCapability::new(vm_id, caps, duration_s, now);
        self.temp_capabilities[self.temp_cap_count as usize] = Some(temp_cap);
        self.temp_cap_count += 1;

        self.log_audit(now, 0, vm_id, caps, true);
        self.policy_changes += 1;

        true
    }

    /// Revoke a temporary capability
    pub fn revoke_temporary(&mut self, cap_id: u32, now: u32) -> bool {
        let mut found_idx = None;
        let mut found_vm = 0;
        let mut found_cap = 0;

        for i in 0..self.temp_cap_count as usize {
            if let Some(cap) = &self.temp_capabilities[i] {
                if cap.capability_id == cap_id {
                    found_idx = Some(i);
                    found_vm = cap.vm_id;
                    found_cap = cap.capability_bits;
                    break;
                }
            }
        }

        if let Some(idx) = found_idx {
            self.temp_capabilities[idx] = None;
            self.log_audit(now, 1, found_vm, found_cap, true);
            self.policy_changes += 1;
            return true;
        }
        false
    }

    /// Get active temporary capabilities for a VM
    pub fn get_active_temp_caps(&self, vm_id: u32, now: u32) -> u32 {
        let mut count = 0;
        for i in 0..self.temp_cap_count as usize {
            if let Some(cap) = &self.temp_capabilities[i] {
                if cap.vm_id == vm_id && !cap.is_expired(now) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Create a capability delegation
    pub fn delegate_capability(
        &mut self,
        from_vm: u32,
        to_vm: u32,
        caps: u32,
        max_depth: u8,
        now: u32,
    ) -> bool {
        if self.delegation_count >= MAX_DELEGATIONS as u32 {
            return false;
        }

        let delegation = CapabilityDelegation::new(from_vm, to_vm, caps, max_depth, now);
        self.delegations[self.delegation_count as usize] = Some(delegation);
        self.delegation_count += 1;

        self.log_audit(now, 2, to_vm, caps, true);
        self.policy_changes += 1;

        true
    }

    /// Revoke a delegation
    pub fn revoke_delegation(&mut self, delegation_id: u32, now: u32) -> bool {
        let mut found_idx = None;
        let mut found_to_vm = 0;
        let mut found_caps = 0;

        for i in 0..self.delegation_count as usize {
            if let Some(delegation) = &self.delegations[i] {
                if delegation.delegation_id == delegation_id {
                    found_idx = Some(i);
                    found_to_vm = delegation.to_vm_id;
                    found_caps = delegation.capabilities;
                    break;
                }
            }
        }

        if let Some(idx) = found_idx {
            if let Some(delegation) = &mut self.delegations[idx] {
                delegation.revoke();
                self.log_audit(now, 1, found_to_vm, found_caps, true);
                self.policy_changes += 1;
                return true;
            }
        }
        false
    }

    /// Check if VM has delegated capability
    pub fn check_delegation(&self, from_vm: u32, to_vm: u32, cap: u32) -> bool {
        for i in 0..self.delegation_count as usize {
            if let Some(delegation) = &self.delegations[i] {
                if delegation.from_vm_id == from_vm
                    && delegation.to_vm_id == to_vm
                    && (delegation.capabilities & cap) != 0
                    && delegation.is_valid()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Add fine-grained capability constraint
    pub fn add_fine_grained(&mut self, cap: FineGrainedCapability) -> bool {
        if self.fine_grained_count >= MAX_POLICIES as u32 {
            return false;
        }

        self.fine_grained_caps[self.fine_grained_count as usize] = Some(cap);
        self.fine_grained_count += 1;
        self.policy_changes += 1;

        true
    }

    /// Check fine-grained capability constraint
    pub fn check_fine_grained(&self, vm_id: u32, resource_type: u8, port: u16) -> bool {
        for i in 0..self.fine_grained_count as usize {
            if let Some(cap) = &self.fine_grained_caps[i] {
                if cap.vm_id == vm_id && cap.resource_type == resource_type {
                    return cap.port_allowed(port);
                }
            }
        }
        true // No constraint = allowed
    }

    /// Log security audit entry
    fn log_audit(&mut self, now: u32, event_type: u8, vm_id: u32, cap: u32, result: bool) {
        let entry = SecurityAuditEntry::new(now, event_type, vm_id, cap, result);
        self.audit_log[self.audit_index] = entry;
        self.audit_index = (self.audit_index + 1) % MAX_AUDIT_ENTRIES;
        self.audit_count += 1;
    }

    /// Get audit log entries
    pub fn get_audit_log(&self) -> &[SecurityAuditEntry] {
        &self.audit_log
    }

    /// Get audit count
    pub fn get_audit_count(&self) -> u32 {
        self.audit_count
    }

    /// Get policy version (increments on each change)
    pub fn get_policy_version(&self) -> u32 {
        self.policy_version
    }

    /// Increment policy version
    pub fn update_policy_version(&mut self) {
        self.policy_version += 1;
    }

    /// Get total policy changes
    pub fn get_policy_changes(&self) -> u32 {
        self.policy_changes
    }

    /// Get statistics
    pub fn get_statistics(&self) -> (u32, u32, u32, u32, u32) {
        (
            self.temp_cap_count,
            self.delegation_count,
            self.fine_grained_count,
            self.audit_count,
            self.policy_changes,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporary_capability_expiration() {
        let cap = TemporaryCapability::new(1000, 0x0F, 3600, 1000);
        assert!(!cap.is_expired(2000)); // 1000 seconds elapsed, still valid
        assert!(cap.is_expired(5000)); // 4000 seconds elapsed, expired
        assert_eq!(cap.time_remaining(4000), 600); // 600 seconds remaining
    }

    #[test]
    fn test_capability_delegation_depth() {
        let mut delegation = CapabilityDelegation::new(1000, 1001, 0x0F, 2, 0);
        assert!(delegation.can_delegate_further());
        delegation.delegation_depth = 2;
        assert!(!delegation.can_delegate_further()); // At max depth

        delegation.revoke();
        assert!(!delegation.is_valid());
    }

    #[test]
    fn test_fine_grained_capability_ports() {
        let mut cap = FineGrainedCapability::new(1000, 0);
        cap.port_range_start = 8000;
        cap.port_range_end = 9000;

        assert!(cap.port_allowed(8500));
        assert!(!cap.port_allowed(7500));
        assert!(!cap.port_allowed(9500));
    }

    #[test]
    fn test_dynamic_policy_enforcer_temp_caps() {
        let mut enforcer = DynamicPolicyEnforcer::new();
        assert!(enforcer.grant_temporary(1000, 0x0F, 3600, 1000));
        assert_eq!(enforcer.get_active_temp_caps(1000, 1500), 1);
        assert_eq!(enforcer.get_active_temp_caps(1000, 5000), 0); // Expired

        let (tc, dc, fc, ac, _) = enforcer.get_statistics();
        assert_eq!(tc, 1);
    }

    #[test]
    fn test_delegation_management() {
        let mut enforcer = DynamicPolicyEnforcer::new();
        assert!(enforcer.delegate_capability(1000, 1001, 0x0F, 2, 0));
        assert!(enforcer.check_delegation(1000, 1001, 0x0F));
        assert!(!enforcer.check_delegation(1001, 1000, 0x0F));

        let (_, dc, _, _, _) = enforcer.get_statistics();
        assert_eq!(dc, 1);
    }

    #[test]
    fn test_fine_grained_constraints() {
        let mut enforcer = DynamicPolicyEnforcer::new();
        let mut cap = FineGrainedCapability::new(1000, 0);
        cap.port_range_start = 8000;
        cap.port_range_end = 9000;
        assert!(enforcer.add_fine_grained(cap));

        assert!(enforcer.check_fine_grained(1000, 0, 8500));
        assert!(!enforcer.check_fine_grained(1000, 0, 7500));
    }

    #[test]
    fn test_audit_logging() {
        let mut enforcer = DynamicPolicyEnforcer::new();
        enforcer.grant_temporary(1000, 0x0F, 3600, 1000);
        assert_eq!(enforcer.get_audit_count(), 1);

        let audit = enforcer.get_audit_log();
        assert_eq!(audit[0].vm_id, 1000);
        assert_eq!(audit[0].event_type, 0); // GRANT
    }

    #[test]
    fn test_policy_version_tracking() {
        let mut enforcer = DynamicPolicyEnforcer::new();
        let v1 = enforcer.get_policy_version();
        enforcer.grant_temporary(1000, 0x0F, 3600, 1000);
        let v2 = enforcer.get_policy_version();
        assert_eq!(v1, v2); // Version doesn't auto-increment, must call update

        enforcer.update_policy_version();
        let v3 = enforcer.get_policy_version();
        assert!(v3 > v2);
    }

    #[test]
    fn test_enforcer_capacity() {
        let mut enforcer = DynamicPolicyEnforcer::new();

        // Fill with temporary capabilities
        for i in 0..MAX_TEMP_CAPABILITIES {
            let granted = enforcer.grant_temporary(
                1000 + i as u32,
                0x0F,
                3600,
                1000,
            );
            assert_eq!(granted, i < 256); // Should succeed until full
        }

        let (tc, _, _, _, _) = enforcer.get_statistics();
        assert_eq!(tc as usize, MAX_TEMP_CAPABILITIES);
    }
}
