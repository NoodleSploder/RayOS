
const MAX_NETWORKS: usize = 8;
const MAX_INTERFACES_PER_BRIDGE: usize = 16;
const MAX_VLANS: usize = 16;
const MAX_SWITCHES: usize = 8;
const MAX_PACKET_SAMPLES: usize = 256;

/// Network type enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetworkType {
    Isolated,
    Bridged,
    Overlay,
    Direct,
}

/// Network lifecycle state enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetworkState {
    Created,
    Configuring,
    Active,
    Suspended,
    Failed,
    Deactivating,
    Destroyed,
}

/// Virtual network interface
#[derive(Clone, Copy, Debug)]
pub struct VirtualInterface {
    pub interface_id: u32,
    pub vm_id: u32,
    pub mac_address: u64,
    pub ipv4_address: u32,
    pub mtu: u16,
    pub bandwidth_limit: u32,
    pub state: u8,
}

impl VirtualInterface {
    pub fn new(interface_id: u32, vm_id: u32, mac_address: u64, ipv4: u32) -> Self {
        VirtualInterface {
            interface_id,
            vm_id,
            mac_address,
            ipv4_address: ipv4,
            mtu: 1500,
            bandwidth_limit: 0,
            state: 0,
        }
    }
}

/// Network bridge for layer 2 switching
#[derive(Clone, Copy, Debug)]
pub struct NetworkBridge {
    pub bridge_id: u32,
    pub interface_count: u8,
    pub vlan_enabled: bool,
    pub stp_enabled: bool,
    pub mtu: u16,
    pub forwarding_table_entries: u32,
    pub state: u8,
}

impl NetworkBridge {
    pub fn new(bridge_id: u32) -> Self {
        NetworkBridge {
            bridge_id,
            interface_count: 0,
            vlan_enabled: false,
            stp_enabled: false,
            mtu: 1500,
            forwarding_table_entries: 0,
            state: 0,
        }
    }

    pub fn can_add_interface(&self) -> bool {
        (self.interface_count as usize) < MAX_INTERFACES_PER_BRIDGE
    }

    pub fn add_interface(&mut self) -> bool {
        if self.can_add_interface() {
            self.interface_count += 1;
            true
        } else {
            false
        }
    }

    pub fn remove_interface(&mut self) -> bool {
        if self.interface_count > 0 {
            self.interface_count -= 1;
            true
        } else {
            false
        }
    }
}

/// Virtual switch for network routing
#[derive(Clone, Copy, Debug)]
pub struct VirtualSwitch {
    pub switch_id: u32,
    pub bridge_count: u8,
    pub active_connections: u16,
    pub routing_enabled: bool,
    pub nat_enabled: bool,
    pub state: u8,
}

impl VirtualSwitch {
    pub fn new(switch_id: u32) -> Self {
        VirtualSwitch {
            switch_id,
            bridge_count: 0,
            active_connections: 0,
            routing_enabled: false,
            nat_enabled: false,
            state: 0,
        }
    }
}

/// Network packet statistics
#[derive(Clone, Copy, Debug)]
pub struct NetworkPacketStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_dropped: u32,
    pub errors: u32,
    pub collisions: u16,
}

impl NetworkPacketStats {
    pub fn new() -> Self {
        NetworkPacketStats {
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            packets_dropped: 0,
            errors: 0,
            collisions: 0,
        }
    }

    pub fn record_sent(&mut self, bytes: u64) {
        self.packets_sent += 1;
        self.bytes_sent += bytes;
    }

    pub fn record_received(&mut self, bytes: u64) {
        self.packets_received += 1;
        self.bytes_received += bytes;
    }

    pub fn total_packets(&self) -> u64 {
        self.packets_sent + self.packets_received
    }

    pub fn total_bytes(&self) -> u64 {
        self.bytes_sent + self.bytes_received
    }
}

/// Virtual network definition
#[derive(Clone, Copy, Debug)]
pub struct VirtualNetwork {
    pub network_id: u32,
    pub network_type: NetworkType,
    pub state: NetworkState,
    pub interface_count: u32,
    pub bridge_count: u8,
    pub vlan_count: u8,
    pub mtu: u16,
    pub checksum: u32,
}

impl VirtualNetwork {
    pub fn new(network_id: u32, network_type: NetworkType) -> Self {
        VirtualNetwork {
            network_id,
            network_type,
            state: NetworkState::Created,
            interface_count: 0,
            bridge_count: 0,
            vlan_count: 0,
            mtu: 1500,
            checksum: 0,
        }
    }

    pub fn validate_state_transition(&self, new_state: NetworkState) -> bool {
        match (self.state, new_state) {
            (NetworkState::Created, NetworkState::Configuring) => true,
            (NetworkState::Configuring, NetworkState::Active) => true,
            (NetworkState::Active, NetworkState::Suspended) => true,
            (NetworkState::Suspended, NetworkState::Active) => true,
            (NetworkState::Active, NetworkState::Failed) => true,
            (NetworkState::Failed, NetworkState::Configuring) => true,
            (NetworkState::Active, NetworkState::Deactivating) => true,
            (NetworkState::Deactivating, NetworkState::Destroyed) => true,
            _ => false,
        }
    }
}

/// Virtual Network Manager
pub struct VirtualNetworkManager {
    networks: [Option<VirtualNetwork>; MAX_NETWORKS],
    bridges: [Option<NetworkBridge>; 16],
    switches: [Option<VirtualSwitch>; MAX_SWITCHES],
    interfaces: [Option<VirtualInterface>; 64],
    packet_stats: [Option<NetworkPacketStats>; MAX_PACKET_SAMPLES],
    active_network_count: u32,
    interface_id_counter: u32,
    bridge_id_counter: u32,
    active_interface_count: u32,
}

impl VirtualNetworkManager {
    pub fn new() -> Self {
        VirtualNetworkManager {
            networks: [None; MAX_NETWORKS],
            bridges: [None; 16],
            switches: [None; MAX_SWITCHES],
            interfaces: [None; 64],
            packet_stats: [None; MAX_PACKET_SAMPLES],
            active_network_count: 0,
            interface_id_counter: 1000,
            bridge_id_counter: 2000,
            active_interface_count: 0,
        }
    }

    pub fn create_network(&mut self, network_type: NetworkType) -> u32 {
        for i in 0..MAX_NETWORKS {
            if self.networks[i].is_none() {
                let network_id = i as u32 + 1;
                let network = VirtualNetwork::new(network_id, network_type);
                self.networks[i] = Some(network);
                self.active_network_count += 1;
                return network_id;
            }
        }
        0
    }

    pub fn delete_network(&mut self, network_id: u32) -> bool {
        let idx = (network_id as usize) - 1;
        if idx < MAX_NETWORKS {
            if let Some(net) = self.networks[idx] {
                if net.state == NetworkState::Destroyed || net.state == NetworkState::Created {
                    self.networks[idx] = None;
                    self.active_network_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn get_network(&self, network_id: u32) -> Option<VirtualNetwork> {
        let idx = (network_id as usize) - 1;
        if idx < MAX_NETWORKS {
            self.networks[idx]
        } else {
            None
        }
    }

    pub fn transition_network_state(&mut self, network_id: u32, new_state: NetworkState) -> bool {
        let idx = (network_id as usize) - 1;
        if idx < MAX_NETWORKS {
            if let Some(mut net) = self.networks[idx] {
                if net.validate_state_transition(new_state) {
                    net.state = new_state;
                    self.networks[idx] = Some(net);
                    return true;
                }
            }
        }
        false
    }

    pub fn create_bridge(&mut self, network_id: u32) -> u32 {
        for i in 0..16 {
            if self.bridges[i].is_none() {
                let bridge_id = self.bridge_id_counter;
                self.bridge_id_counter += 1;

                let bridge = NetworkBridge::new(bridge_id);
                let idx = (network_id as usize) - 1;
                if idx < MAX_NETWORKS {
                    if let Some(mut net) = self.networks[idx] {
                        net.bridge_count += 1;
                        self.networks[idx] = Some(net);
                    }
                }

                self.bridges[i] = Some(bridge);
                return bridge_id;
            }
        }
        0
    }

    pub fn delete_bridge(&mut self, bridge_id: u32) -> bool {
        for i in 0..16 {
            if let Some(bridge) = self.bridges[i] {
                if bridge.bridge_id == bridge_id {
                    self.bridges[i] = None;
                    return true;
                }
            }
        }
        false
    }

    pub fn add_interface_to_network(&mut self, network_id: u32, vm_id: u32, mac: u64, ipv4: u32) -> u32 {
        for i in 0..64 {
            if self.interfaces[i].is_none() {
                let interface_id = self.interface_id_counter;
                self.interface_id_counter += 1;

                let iface = VirtualInterface::new(interface_id, vm_id, mac, ipv4);
                self.interfaces[i] = Some(iface);
                self.active_interface_count += 1;

                let idx = (network_id as usize) - 1;
                if idx < MAX_NETWORKS {
                    if let Some(mut net) = self.networks[idx] {
                        net.interface_count += 1;
                        self.networks[idx] = Some(net);
                    }
                }

                self.packet_stats[i % MAX_PACKET_SAMPLES] = Some(NetworkPacketStats::new());
                return interface_id;
            }
        }
        0
    }

    pub fn remove_interface(&mut self, interface_id: u32) -> bool {
        for i in 0..64 {
            if let Some(iface) = self.interfaces[i] {
                if iface.interface_id == interface_id {
                    self.active_interface_count -= 1;
                    self.interfaces[i] = None;
                    return true;
                }
            }
        }
        false
    }

    pub fn get_interface(&self, interface_id: u32) -> Option<VirtualInterface> {
        for i in 0..64 {
            if let Some(iface) = self.interfaces[i] {
                if iface.interface_id == interface_id {
                    return Some(iface);
                }
            }
        }
        None
    }

    pub fn record_packet(&mut self, interface_id: u32, bytes: u64, is_sent: bool) {
        for i in 0..64 {
            if let Some(iface) = self.interfaces[i] {
                if iface.interface_id == interface_id {
                    if let Some(mut stats) = self.packet_stats[i % MAX_PACKET_SAMPLES] {
                        if is_sent {
                            stats.record_sent(bytes);
                        } else {
                            stats.record_received(bytes);
                        }
                        self.packet_stats[i % MAX_PACKET_SAMPLES] = Some(stats);
                    }
                    return;
                }
            }
        }
    }

    pub fn get_interface_stats(&self, interface_id: u32) -> Option<NetworkPacketStats> {
        for i in 0..64 {
            if let Some(iface) = self.interfaces[i] {
                if iface.interface_id == interface_id {
                    return self.packet_stats[i % MAX_PACKET_SAMPLES];
                }
            }
        }
        None
    }

    pub fn set_bandwidth_limit(&mut self, interface_id: u32, limit_mbps: u32) -> bool {
        for i in 0..64 {
            if let Some(mut iface) = self.interfaces[i] {
                if iface.interface_id == interface_id {
                    iface.bandwidth_limit = limit_mbps;
                    self.interfaces[i] = Some(iface);
                    return true;
                }
            }
        }
        false
    }

    pub fn get_active_network_count(&self) -> u32 {
        self.active_network_count
    }

    pub fn get_active_interface_count(&self) -> u32 {
        self.active_interface_count
    }

    pub fn enable_vlan(&mut self, network_id: u32) -> bool {
        let idx = (network_id as usize) - 1;
        if idx < MAX_NETWORKS {
            #[allow(unused_assignments)]
            if let Some(mut net) = self.networks[idx] {
                net.vlan_count += 1;
                if net.vlan_count > MAX_VLANS as u8 {
                    net.vlan_count = MAX_VLANS as u8;
                    return false;
                }
                self.networks[idx] = Some(net);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_network() {
        let mut manager = VirtualNetworkManager::new();
        let net_id = manager.create_network(NetworkType::Bridged);
        assert!(net_id > 0);
        assert_eq!(manager.get_active_network_count(), 1);
    }

    #[test]
    fn test_network_state_transitions() {
        let net = VirtualNetwork::new(1, NetworkType::Isolated);
        assert!(net.validate_state_transition(NetworkState::Configuring));
        assert!(!net.validate_state_transition(NetworkState::Active));
    }

    #[test]
    fn test_create_bridge() {
        let mut manager = VirtualNetworkManager::new();
        let net_id = manager.create_network(NetworkType::Bridged);
        manager.transition_network_state(net_id, NetworkState::Configuring);
        let bridge_id = manager.create_bridge(net_id);
        assert!(bridge_id > 0);
    }

    #[test]
    fn test_add_interface() {
        let mut manager = VirtualNetworkManager::new();
        let net_id = manager.create_network(NetworkType::Bridged);
        manager.transition_network_state(net_id, NetworkState::Configuring);
        manager.transition_network_state(net_id, NetworkState::Active);

        let iface_id = manager.add_interface_to_network(net_id, 1, 0x0050_5600_1234, 0xC0A8_0001);
        assert!(iface_id > 0);
        assert_eq!(manager.get_active_interface_count(), 1);
    }

    #[test]
    fn test_remove_interface() {
        let mut manager = VirtualNetworkManager::new();
        let net_id = manager.create_network(NetworkType::Bridged);
        manager.transition_network_state(net_id, NetworkState::Configuring);
        manager.transition_network_state(net_id, NetworkState::Active);

        let iface_id = manager.add_interface_to_network(net_id, 1, 0x0050_5600_1234, 0xC0A8_0001);
        assert!(manager.remove_interface(iface_id));
        assert_eq!(manager.get_active_interface_count(), 0);
    }

    #[test]
    fn test_packet_statistics() {
        let mut manager = VirtualNetworkManager::new();
        let net_id = manager.create_network(NetworkType::Bridged);
        manager.transition_network_state(net_id, NetworkState::Configuring);
        manager.transition_network_state(net_id, NetworkState::Active);

        let iface_id = manager.add_interface_to_network(net_id, 1, 0x0050_5600_1234, 0xC0A8_0001);
        manager.record_packet(iface_id, 1500, true);
        manager.record_packet(iface_id, 1500, false);

        let stats = manager.get_interface_stats(iface_id);
        assert!(stats.is_some());
        let s = stats.unwrap();
        assert_eq!(s.packets_sent, 1);
        assert_eq!(s.packets_received, 1);
    }

    #[test]
    fn test_bandwidth_limit() {
        let mut manager = VirtualNetworkManager::new();
        let net_id = manager.create_network(NetworkType::Bridged);
        manager.transition_network_state(net_id, NetworkState::Configuring);
        manager.transition_network_state(net_id, NetworkState::Active);

        let iface_id = manager.add_interface_to_network(net_id, 1, 0x0050_5600_1234, 0xC0A8_0001);
        assert!(manager.set_bandwidth_limit(iface_id, 100));

        let iface = manager.get_interface(iface_id);
        assert!(iface.is_some());
        assert_eq!(iface.unwrap().bandwidth_limit, 100);
    }

    #[test]
    fn test_vlan_support() {
        let mut manager = VirtualNetworkManager::new();
        let net_id = manager.create_network(NetworkType::Bridged);
        manager.transition_network_state(net_id, NetworkState::Configuring);
        manager.transition_network_state(net_id, NetworkState::Active);

        assert!(manager.enable_vlan(net_id));

        let net = manager.get_network(net_id);
        assert!(net.is_some());
        assert_eq!(net.unwrap().vlan_count, 1);
    }

    #[test]
    fn test_delete_network() {
        let mut manager = VirtualNetworkManager::new();
        let net_id = manager.create_network(NetworkType::Isolated);
        assert!(manager.delete_network(net_id));
        assert_eq!(manager.get_active_network_count(), 0);
    }
}
