// Phase 10 Task 3: Process Sandboxing & Capability Enforcement
// ==============================================================
// Enforces capability-based security at I/O boundaries
// Validates device access against per-VM capability grants

use crate::security::{Capability, SecurityPolicy, AUDIT_CAPABILITY_DENIAL, AUDIT_NETWORK_ACCESS,
                       AUDIT_DISK_ACCESS, AUDIT_GPU_ACCESS, AUDIT_INPUT_ACCESS};

/// Device access request types
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceAccessType {
    GpuRead = 0x01,
    GpuWrite = 0x02,
    NetworkTx = 0x03,
    NetworkRx = 0x04,
    DiskRead = 0x05,
    DiskWrite = 0x06,
    InputEvent = 0x07,
    ConsoleRead = 0x08,
    ConsoleWrite = 0x09,
}

/// Access control decision
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessDecision {
    Allow,
    Deny,
    Audit,
}

/// Policy enforcement engine
pub struct PolicyEnforcer {
    vm_policies: [SecurityPolicy; 8],  // Support 8 VMs max
    vm_count: usize,
}

impl PolicyEnforcer {
    pub fn new() -> Self {
        // Initialize with default policies (minimal capabilities)
        let mut enforcer = PolicyEnforcer {
            vm_policies: [SecurityPolicy::new(0); 8],
            vm_count: 0,
        };

        // Initialize VM IDs
        for i in 0..8 {
            enforcer.vm_policies[i] = SecurityPolicy::new(i as u32);
        }

        enforcer
    }

    /// Register a new VM with capabilities
    pub fn register_vm(&mut self, vm_id: u32, capabilities: u32) -> bool {
        if self.vm_count >= 8 {
            return false;
        }

        let idx = self.vm_count;
        self.vm_policies[idx].vm_id = vm_id;
        self.vm_policies[idx].capabilities = capabilities;
        self.vm_count += 1;
        true
    }

    /// Find policy for a VM
    fn get_policy(&self, vm_id: u32) -> Option<&SecurityPolicy> {
        for policy in &self.vm_policies[..self.vm_count] {
            if policy.vm_id == vm_id {
                return Some(policy);
            }
        }
        None
    }

    /// Find mutable policy for a VM
    fn get_policy_mut(&mut self, vm_id: u32) -> Option<&mut SecurityPolicy> {
        for policy in &mut self.vm_policies[..self.vm_count] {
            if policy.vm_id == vm_id {
                return Some(policy);
            }
        }
        None
    }

    /// Enforce access control for a device operation
    pub fn check_device_access(
        &self,
        vm_id: u32,
        access: DeviceAccessType,
    ) -> AccessDecision {
        let policy = match self.get_policy(vm_id) {
            Some(p) => p,
            None => return AccessDecision::Deny,
        };

        // Map device access to capability requirement
        let required_cap = match access {
            DeviceAccessType::GpuRead | DeviceAccessType::GpuWrite => Capability::CAP_GPU,
            DeviceAccessType::NetworkTx | DeviceAccessType::NetworkRx => Capability::CAP_NETWORK,
            DeviceAccessType::DiskRead => Capability::CAP_DISK_READ,
            DeviceAccessType::DiskWrite => Capability::CAP_DISK_WRITE,
            DeviceAccessType::InputEvent => Capability::CAP_INPUT,
            DeviceAccessType::ConsoleRead | DeviceAccessType::ConsoleWrite => Capability::CAP_CONSOLE,
        };

        // Check if VM has the required capability
        if policy.has_capability(required_cap) {
            AccessDecision::Allow
        } else {
            AccessDecision::Deny
        }
    }

    /// Grant a capability to a VM
    pub fn grant_capability(&mut self, vm_id: u32, cap: Capability) -> bool {
        if let Some(policy) = self.get_policy_mut(vm_id) {
            policy.grant_capability(cap);
            return true;
        }
        false
    }

    /// Revoke a capability from a VM
    pub fn revoke_capability(&mut self, vm_id: u32, cap: Capability) -> bool {
        if let Some(policy) = self.get_policy_mut(vm_id) {
            policy.revoke_capability(cap);
            return true;
        }
        false
    }

    /// Get audit event type for a device access
    pub fn get_audit_event_type(access: DeviceAccessType) -> u32 {
        match access {
            DeviceAccessType::NetworkTx | DeviceAccessType::NetworkRx => AUDIT_NETWORK_ACCESS,
            DeviceAccessType::DiskRead | DeviceAccessType::DiskWrite => AUDIT_DISK_ACCESS,
            DeviceAccessType::GpuRead | DeviceAccessType::GpuWrite => AUDIT_GPU_ACCESS,
            DeviceAccessType::InputEvent => AUDIT_INPUT_ACCESS,
            DeviceAccessType::ConsoleRead | DeviceAccessType::ConsoleWrite => 0x0A, // AUDIT_CONSOLE_ACCESS
        }
    }
}

/// Default VM capability profiles
pub mod profiles {
    use crate::security::Capability;

    /// Linux desktop VM - full access
    pub fn linux_desktop_capabilities() -> u32 {
        (Capability::CAP_GPU as u32) |
        (Capability::CAP_INPUT as u32) |
        (Capability::CAP_DISK_READ as u32) |
        (Capability::CAP_DISK_WRITE as u32) |
        (Capability::CAP_NETWORK as u32) |
        (Capability::CAP_CONSOLE as u32) |
        (Capability::CAP_AUDIT as u32)
    }

    /// Windows VM - restricted networking
    pub fn windows_desktop_capabilities() -> u32 {
        (Capability::CAP_GPU as u32) |
        (Capability::CAP_INPUT as u32) |
        (Capability::CAP_DISK_READ as u32) |
        (Capability::CAP_DISK_WRITE as u32) |
        (Capability::CAP_CONSOLE as u32) |
        (Capability::CAP_AUDIT as u32)
        // NOTE: CAP_NETWORK explicitly denied
    }

    /// Server VM - minimal UI, full network & disk
    pub fn server_capabilities() -> u32 {
        (Capability::CAP_DISK_READ as u32) |
        (Capability::CAP_DISK_WRITE as u32) |
        (Capability::CAP_NETWORK as u32) |
        (Capability::CAP_CONSOLE as u32) |
        (Capability::CAP_AUDIT as u32)
        // NOTE: CAP_GPU and CAP_INPUT denied (no UI)
    }

    /// Restricted guest - minimal access
    pub fn restricted_capabilities() -> u32 {
        (Capability::CAP_CONSOLE as u32) |
        (Capability::CAP_AUDIT as u32)
        // NOTE: Network, disk, GPU, input all denied
    }
}

// ============================================================================
// Integration with existing VMM device models
// ============================================================================
// These functions are called from device handlers to enforce policies

/// Check if a virtio-gpu operation is allowed
pub fn check_gpu_access(vm_id: u32, write: bool) -> bool {
    // In real implementation, would call PolicyEnforcer singleton
    // For now: always allow (audit would be logged separately)
    true
}

/// Check if a virtio-net operation is allowed
pub fn check_network_access(vm_id: u32, _tx: bool) -> bool {
    // In real implementation, would call PolicyEnforcer singleton
    // For now: always allow for VM 1000 (Linux), deny for others
    vm_id == 1000
}

/// Check if a virtio-blk operation is allowed
pub fn check_disk_access(vm_id: u32, write: bool) -> bool {
    // In real implementation, would call PolicyEnforcer singleton
    // For now: allow reads for all, restrict writes
    !write || vm_id == 1000
}

/// Check if a virtio-input operation is allowed
pub fn check_input_access(vm_id: u32) -> bool {
    // In real implementation, would call PolicyEnforcer singleton
    // For now: only allow for VM 1000 (focused window)
    vm_id == 1000
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub fn test_policy_enforcer() {
    let mut enforcer = PolicyEnforcer::new();

    // Register Linux desktop VM
    assert!(enforcer.register_vm(1000, profiles::linux_desktop_capabilities()));

    // Register Windows VM
    assert!(enforcer.register_vm(1001, profiles::windows_desktop_capabilities()));

    // Test Linux VM can access network
    let decision = enforcer.check_device_access(1000, DeviceAccessType::NetworkTx);
    assert_eq!(decision, AccessDecision::Allow);

    // Test Windows VM cannot access network
    let decision = enforcer.check_device_access(1001, DeviceAccessType::NetworkTx);
    assert_eq!(decision, AccessDecision::Deny);
}

#[cfg(test)]
pub fn test_capability_grant_revoke() {
    let mut enforcer = PolicyEnforcer::new();
    enforcer.register_vm(100, 0); // No capabilities initially

    // Verify access denied
    let decision = enforcer.check_device_access(100, DeviceAccessType::DiskRead);
    assert_eq!(decision, AccessDecision::Deny);

    // Grant capability
    assert!(enforcer.grant_capability(100, Capability::CAP_DISK_READ));

    // Verify access allowed
    let decision = enforcer.check_device_access(100, DeviceAccessType::DiskRead);
    assert_eq!(decision, AccessDecision::Allow);

    // Revoke capability
    assert!(enforcer.revoke_capability(100, Capability::CAP_DISK_READ));

    // Verify access denied again
    let decision = enforcer.check_device_access(100, DeviceAccessType::DiskRead);
    assert_eq!(decision, AccessDecision::Deny);
}

#[cfg(test)]
pub fn test_default_profiles() {
    let linux_caps = profiles::linux_desktop_capabilities();
    let windows_caps = profiles::windows_desktop_capabilities();
    let server_caps = profiles::server_capabilities();
    let restricted_caps = profiles::restricted_capabilities();

    // Verify capability bits are set correctly
    assert!(linux_caps & (Capability::CAP_NETWORK as u32) != 0);
    assert!(windows_caps & (Capability::CAP_NETWORK as u32) == 0); // Network denied
    assert!(server_caps & (Capability::CAP_GPU as u32) == 0); // GPU denied
    assert_eq!(restricted_caps & (Capability::CAP_NETWORK as u32), 0); // All denied except console
}
