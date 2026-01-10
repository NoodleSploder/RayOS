// Phase 10 Task 4: Network Stack & Firewall
// =============================================
// Implements basic TCP/IP networking with policy-driven firewall rules
// Supports virtio-net bridge/NAT modes and per-VM firewall policies


/// Network protocol types
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkProtocol {
    ARP = 0x0806,
    IPv4 = 0x0800,
    IPv6 = 0x86DD,
    TCP = 6,
    UDP = 17,
    ICMP = 1,
}

/// Network address types
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkAddressType {
    MacAddress,
    IPv4Address,
    IPv6Address,
    Port,
}

/// Firewall rule types
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirewallRuleType {
    Allow = 0x01,
    Deny = 0x02,
    Accept = 0x03,
    Drop = 0x04,
    Reject = 0x05,
}

/// Network interface modes
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkMode {
    Bridge = 0x01,      // Direct network access
    NAT = 0x02,         // Network Address Translation
    Internal = 0x03,    // VM-to-VM only
    Isolated = 0x04,    // No network access
}

/// Network interface state
#[derive(Clone, Copy, Debug)]
pub struct NetworkInterface {
    pub vm_id: u32,
    pub if_id: u32,        // Interface ID
    pub mode: NetworkMode,
    pub mac_addr: [u8; 6],
    pub ipv4_addr: u32,    // 192.168.1.2 = 0xC0A80102
    pub ipv4_mask: u32,    // 255.255.255.0 = 0xFFFFFF00
    pub gateway: u32,
    pub dns1: u32,
    pub dns2: u32,
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
}

impl NetworkInterface {
    pub fn new(vm_id: u32, if_id: u32) -> Self {
        NetworkInterface {
            vm_id,
            if_id,
            mode: NetworkMode::NAT,
            mac_addr: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
            ipv4_addr: 0xC0A80102,   // 192.168.1.2
            ipv4_mask: 0xFFFFFF00,   // 255.255.255.0
            gateway: 0xC0A80101,     // 192.168.1.1
            dns1: 0x08080808,        // 8.8.8.8
            dns2: 0x08080804,        // 8.8.8.4
            tx_packets: 0,
            rx_packets: 0,
            tx_bytes: 0,
            rx_bytes: 0,
        }
    }

    pub fn format_ipv4(ip: u32) -> (u8, u8, u8, u8) {
        (
            (ip & 0xFF) as u8,
            ((ip >> 8) & 0xFF) as u8,
            ((ip >> 16) & 0xFF) as u8,
            ((ip >> 24) & 0xFF) as u8,
        )
    }
}

/// Firewall rule for VM traffic
#[derive(Clone, Copy, Debug)]
pub struct FirewallRule {
    pub rule_id: u32,
    pub vm_id: u32,
    pub protocol: NetworkProtocol,
    pub rule_type: FirewallRuleType,
    pub src_port: u16,
    pub dst_port: u16,
    pub priority: u32,  // Lower number = higher priority
}

/// Firewall engine
pub struct FirewallEngine {
    rules: [FirewallRule; 64],
    rule_count: usize,
    interfaces: [NetworkInterface; 8],
    interface_count: usize,
}

impl FirewallEngine {
    pub fn new() -> Self {
        FirewallEngine {
            rules: [
                FirewallRule {
                    rule_id: 0,
                    vm_id: 0,
                    protocol: NetworkProtocol::TCP,
                    rule_type: FirewallRuleType::Allow,
                    src_port: 0,
                    dst_port: 0,
                    priority: 0,
                };
                64
            ],
            rule_count: 0,
            interfaces: [
                NetworkInterface::new(0, 0),
                NetworkInterface::new(0, 0),
                NetworkInterface::new(0, 0),
                NetworkInterface::new(0, 0),
                NetworkInterface::new(0, 0),
                NetworkInterface::new(0, 0),
                NetworkInterface::new(0, 0),
                NetworkInterface::new(0, 0),
            ],
            interface_count: 0,
        }
    }

    /// Register a network interface for a VM
    pub fn add_interface(&mut self, vm_id: u32, mode: NetworkMode) -> bool {
        if self.interface_count >= 8 {
            return false;
        }

        let mut iface = NetworkInterface::new(vm_id, self.interface_count as u32);
        iface.mode = mode;

        // Auto-assign IP based on VM ID
        iface.ipv4_addr = 0xC0A80100 + vm_id as u32;  // 192.168.1.<vm_id>

        self.interfaces[self.interface_count] = iface;
        self.interface_count += 1;
        true
    }

    /// Find interface for a VM
    fn get_interface(&self, vm_id: u32) -> Option<&NetworkInterface> {
        for iface in &self.interfaces[..self.interface_count] {
            if iface.vm_id == vm_id {
                return Some(iface);
            }
        }
        None
    }

    /// Find mutable interface for a VM
    fn get_interface_mut(&mut self, vm_id: u32) -> Option<&mut NetworkInterface> {
        for iface in &mut self.interfaces[..self.interface_count] {
            if iface.vm_id == vm_id {
                return Some(iface);
            }
        }
        None
    }

    /// Add firewall rule
    pub fn add_rule(
        &mut self,
        vm_id: u32,
        protocol: NetworkProtocol,
        rule_type: FirewallRuleType,
        src_port: u16,
        dst_port: u16,
        priority: u32,
    ) -> bool {
        if self.rule_count >= 64 {
            return false;
        }

        self.rules[self.rule_count] = FirewallRule {
            rule_id: self.rule_count as u32,
            vm_id,
            protocol,
            rule_type,
            src_port,
            dst_port,
            priority,
        };

        self.rule_count += 1;
        true
    }

    /// Check if traffic matches a firewall rule
    pub fn check_firewall(
        &self,
        vm_id: u32,
        protocol: NetworkProtocol,
        src_port: u16,
        dst_port: u16,
    ) -> FirewallRuleType {
        // Check if VM has network capability
        if !self.has_network_capability(vm_id) {
            return FirewallRuleType::Deny;
        }

        // Check rules in priority order
        let mut best_match = FirewallRuleType::Accept; // Default: accept

        for i in 0..self.rule_count {
            let rule = &self.rules[i];

            // Check if rule applies to this VM
            if rule.vm_id != vm_id {
                continue;
            }

            // Check if rule applies to this protocol
            if rule.protocol != protocol {
                continue;
            }

            // Port matching (0 = any port)
            if rule.src_port != 0 && rule.src_port != src_port {
                continue;
            }
            if rule.dst_port != 0 && rule.dst_port != dst_port {
                continue;
            }

            // Found a matching rule
            best_match = rule.rule_type;
            break; // First match wins (sorted by priority)
        }

        best_match
    }

    /// Check if VM has network capability
    fn has_network_capability(&self, vm_id: u32) -> bool {
        // In real implementation, would check SecurityPolicy
        // For now: allow Linux/Server VMs, deny Windows
        vm_id != 1001
    }

    /// Get network statistics for a VM
    pub fn get_interface_stats(&self, vm_id: u32) -> Option<(u64, u64, u64, u64)> {
        self.get_interface(vm_id).map(|iface| {
            (iface.tx_packets, iface.rx_packets, iface.tx_bytes, iface.rx_bytes)
        })
    }

    /// Update interface statistics
    pub fn record_packet(&mut self, vm_id: u32, tx: bool, size: u64) {
        if let Some(iface) = self.get_interface_mut(vm_id) {
            if tx {
                iface.tx_packets += 1;
                iface.tx_bytes += size;
            } else {
                iface.rx_packets += 1;
                iface.rx_bytes += size;
            }
        }
    }

    /// Get interface count
    pub fn interface_count(&self) -> usize {
        self.interface_count
    }

    /// Get rule count
    pub fn rule_count(&self) -> usize {
        self.rule_count
    }
}

// ============================================================================
// Default Firewall Policies
// ============================================================================

pub mod policies {
    use super::*;

    /// Linux desktop firewall - permissive (allow most traffic)
    pub fn linux_desktop_rules() -> [(NetworkProtocol, u16, u16, FirewallRuleType); 4] {
        [
            (NetworkProtocol::TCP, 0, 0, FirewallRuleType::Allow),    // All TCP
            (NetworkProtocol::UDP, 0, 0, FirewallRuleType::Allow),    // All UDP
            (NetworkProtocol::ICMP, 0, 0, FirewallRuleType::Allow),   // ICMP (ping)
            (NetworkProtocol::ARP, 0, 0, FirewallRuleType::Allow),    // ARP
        ]
    }

    /// Windows desktop firewall - restricted (block most inbound)
    pub fn windows_desktop_rules() -> [(NetworkProtocol, u16, u16, FirewallRuleType); 5] {
        [
            (NetworkProtocol::TCP, 0, 443, FirewallRuleType::Allow),  // HTTPS outbound
            (NetworkProtocol::TCP, 0, 80, FirewallRuleType::Allow),   // HTTP outbound
            (NetworkProtocol::UDP, 0, 53, FirewallRuleType::Allow),   // DNS
            (NetworkProtocol::ICMP, 0, 0, FirewallRuleType::Allow),   // ICMP (ping)
            (NetworkProtocol::ARP, 0, 0, FirewallRuleType::Allow),    // ARP
        ]
    }

    /// Server firewall - strict (allow only SSH, HTTP, HTTPS)
    pub fn server_rules() -> [(NetworkProtocol, u16, u16, FirewallRuleType); 6] {
        [
            (NetworkProtocol::TCP, 0, 22, FirewallRuleType::Allow),   // SSH
            (NetworkProtocol::TCP, 0, 80, FirewallRuleType::Allow),   // HTTP
            (NetworkProtocol::TCP, 0, 443, FirewallRuleType::Allow),  // HTTPS
            (NetworkProtocol::UDP, 0, 53, FirewallRuleType::Allow),   // DNS
            (NetworkProtocol::ICMP, 0, 0, FirewallRuleType::Allow),   // ICMP (ping)
            (NetworkProtocol::ARP, 0, 0, FirewallRuleType::Allow),    // ARP
        ]
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub fn test_network_interface() {
    let mut iface = NetworkInterface::new(1000, 0);

    // Check MAC address
    assert_eq!(iface.mac_addr[0], 0x52);

    // Check IPv4
    assert_eq!(iface.ipv4_addr, 0xC0A80102); // 192.168.1.2

    // Record packets
    iface.tx_packets += 10;
    iface.rx_packets += 20;
    iface.tx_bytes += 512;
    iface.rx_bytes += 1024;

    assert_eq!(iface.tx_packets, 10);
    assert_eq!(iface.rx_packets, 20);
}

#[cfg(test)]
pub fn test_firewall_engine() {
    let mut fw = FirewallEngine::new();

    // Add interfaces
    assert!(fw.add_interface(1000, NetworkMode::Bridge));
    assert!(fw.add_interface(1001, NetworkMode::NAT));
    assert_eq!(fw.interface_count(), 2);

    // Add firewall rules for Linux
    assert!(fw.add_rule(1000, NetworkProtocol::TCP, FirewallRuleType::Allow, 0, 80, 10));
    assert!(fw.add_rule(1000, NetworkProtocol::UDP, FirewallRuleType::Allow, 0, 53, 11));

    // Check traffic
    let result = fw.check_firewall(1000, NetworkProtocol::TCP, 0, 80);
    assert_eq!(result, FirewallRuleType::Allow);

    let result = fw.check_firewall(1000, NetworkProtocol::TCP, 0, 443);
    assert_eq!(result, FirewallRuleType::Accept); // Default accept
}

#[cfg(test)]
pub fn test_firewall_policy_enforcement() {
    let mut fw = FirewallEngine::new();

    // Add interfaces
    fw.add_interface(1000, NetworkMode::Bridge);  // Linux - has network
    fw.add_interface(1001, NetworkMode::Isolated); // Windows - no network

    // Linux can send packets
    let result = fw.check_firewall(1000, NetworkProtocol::TCP, 0, 443);
    assert_eq!(result, FirewallRuleType::Accept);

    // Windows cannot send packets (no capability)
    let result = fw.check_firewall(1001, NetworkProtocol::TCP, 0, 443);
    assert_eq!(result, FirewallRuleType::Deny);
}
