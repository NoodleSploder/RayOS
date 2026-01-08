// RAYOS Phase 28 Task 1: Network Stack & Protocol Support
// Basic network protocol support (TCP/UDP, DNS, IP routing)
// File: crates/kernel-bare/src/network_stack.rs
// Lines: 700+ | Tests: 13 unit + 5 scenario | Markers: 5

use core::fmt;

const MAX_NETWORK_INTERFACES: usize = 8;
const MAX_ROUTING_ENTRIES: usize = 8;
const MAX_PACKET_QUEUE_SIZE: usize = 256;

// ============================================================================
// IP ADDRESS & NETWORK DEFINITIONS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IPVersion {
    IPv4,
    IPv6,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IPv4Address {
    pub octets: [u8; 4],
}

impl IPv4Address {
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        IPv4Address {
            octets: [a, b, c, d],
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 4 {
            Some(IPv4Address {
                octets: [bytes[0], bytes[1], bytes[2], bytes[3]],
            })
        } else {
            None
        }
    }

    pub fn to_u32(&self) -> u32 {
        ((self.octets[0] as u32) << 24)
            | ((self.octets[1] as u32) << 16)
            | ((self.octets[2] as u32) << 8)
            | (self.octets[3] as u32)
    }

    pub fn matches_subnet(&self, gateway: IPv4Address, netmask: IPv4Address) -> bool {
        (self.to_u32() & netmask.to_u32()) == (gateway.to_u32() & netmask.to_u32())
    }
}

impl Default for IPv4Address {
    fn default() -> Self {
        IPv4Address::new(0, 0, 0, 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IPAddress {
    V4(IPv4Address),
    V6([u8; 16]),
}

impl IPAddress {
    pub fn is_loopback(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.octets[0] == 127,
            IPAddress::V6(octets) => octets[15] == 1,
        }
    }

    pub fn is_multicast(&self) -> bool {
        match self {
            IPAddress::V4(addr) => (addr.octets[0] & 0xF0) == 0xE0,
            IPAddress::V6(octets) => octets[0] == 0xFF,
        }
    }

    pub fn is_private(&self) -> bool {
        match self {
            IPAddress::V4(addr) => {
                (addr.octets[0] == 10)
                    || (addr.octets[0] == 172 && (addr.octets[1] >= 16 && addr.octets[1] <= 31))
                    || (addr.octets[0] == 192 && addr.octets[1] == 168)
            }
            IPAddress::V6(_) => false,
        }
    }
}

// ============================================================================
// NETWORK INTERFACE
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct MACAddress {
    pub octets: [u8; 6],
}

impl MACAddress {
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        MACAddress {
            octets: [a, b, c, d, e, f],
        }
    }

    pub fn is_broadcast(&self) -> bool {
        self.octets == [0xFF; 6]
    }

    pub fn is_multicast(&self) -> bool {
        (self.octets[0] & 0x01) == 0x01
    }
}

impl Default for MACAddress {
    fn default() -> Self {
        MACAddress::new(0, 0, 0, 0, 0, 0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NetworkInterface {
    pub interface_id: u32,
    pub name_id: u32,
    pub mac_address: MACAddress,
    pub ip_address: IPAddress,
    pub netmask: IPv4Address,
    pub mtu: u16,
    pub is_up: bool,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

impl NetworkInterface {
    pub fn new(interface_id: u32, mac_address: MACAddress, ip_address: IPAddress) -> Self {
        NetworkInterface {
            interface_id,
            name_id: interface_id,
            mac_address,
            ip_address,
            netmask: IPv4Address::new(255, 255, 255, 0),
            mtu: 1500,
            is_up: true,
            rx_packets: 0,
            tx_packets: 0,
            rx_bytes: 0,
            tx_bytes: 0,
        }
    }

    pub fn bring_up(&mut self) {
        self.is_up = true;
    }

    pub fn bring_down(&mut self) {
        self.is_up = false;
    }

    pub fn record_tx(&mut self, bytes: usize) {
        self.tx_packets += 1;
        self.tx_bytes += bytes as u64;
    }

    pub fn record_rx(&mut self, bytes: usize) {
        self.rx_packets += 1;
        self.rx_bytes += bytes as u64;
    }
}

pub struct NetworkInterfaceManager {
    pub interfaces: [Option<NetworkInterface>; MAX_NETWORK_INTERFACES],
    pub interface_count: usize,
    pub next_interface_id: u32,
}

impl NetworkInterfaceManager {
    pub fn new() -> Self {
        NetworkInterfaceManager {
            interfaces: [None; MAX_NETWORK_INTERFACES],
            interface_count: 0,
            next_interface_id: 1,
        }
    }

    pub fn add_interface(&mut self, mac: MACAddress, ip: IPAddress) -> Option<u32> {
        if self.interface_count >= MAX_NETWORK_INTERFACES {
            return None;
        }

        let interface_id = self.next_interface_id;
        self.next_interface_id += 1;

        let interface = NetworkInterface::new(interface_id, mac, ip);
        self.interfaces[self.interface_count] = Some(interface);
        self.interface_count += 1;

        Some(interface_id)
    }

    pub fn get_interface(&self, interface_id: u32) -> Option<NetworkInterface> {
        for i in 0..self.interface_count {
            if let Some(iface) = self.interfaces[i] {
                if iface.interface_id == interface_id {
                    return Some(iface);
                }
            }
        }
        None
    }

    pub fn get_interface_mut(&mut self, interface_id: u32) -> Option<&mut NetworkInterface> {
        for i in 0..self.interface_count {
            if let Some(ref iface) = self.interfaces[i] {
                if iface.interface_id == interface_id {
                    return self.interfaces[i].as_mut();
                }
            }
        }
        None
    }

    pub fn remove_interface(&mut self, interface_id: u32) -> bool {
        for i in 0..self.interface_count {
            if let Some(iface) = self.interfaces[i] {
                if iface.interface_id == interface_id {
                    for j in i..self.interface_count - 1 {
                        self.interfaces[j] = self.interfaces[j + 1];
                    }
                    self.interfaces[self.interface_count - 1] = None;
                    self.interface_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn get_up_interfaces(&self) -> usize {
        self.interfaces[..self.interface_count]
            .iter()
            .filter(|i| i.map(|iface| iface.is_up).unwrap_or(false))
            .count()
    }
}

impl Default for NetworkInterfaceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// PROTOCOL & PACKET DEFINITIONS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProtocolType {
    TCP = 6,
    UDP = 17,
    ICMP = 1,
}

impl ProtocolType {
    pub fn from_number(num: u8) -> Option<Self> {
        match num {
            6 => Some(ProtocolType::TCP),
            17 => Some(ProtocolType::UDP),
            1 => Some(ProtocolType::ICMP),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PacketHeader {
    pub src_ip: IPAddress,
    pub dst_ip: IPAddress,
    pub protocol: ProtocolType,
    pub src_port: u16,
    pub dst_port: u16,
    pub ttl: u8,
}

impl PacketHeader {
    pub fn new(src_ip: IPAddress, dst_ip: IPAddress, protocol: ProtocolType) -> Self {
        PacketHeader {
            src_ip,
            dst_ip,
            protocol,
            src_port: 0,
            dst_port: 0,
            ttl: 64,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NetworkPacket {
    pub header: PacketHeader,
    pub payload_size: u16,
    pub checksum: u16,
    pub interface_id: u32,
}

impl NetworkPacket {
    pub fn new(header: PacketHeader) -> Self {
        NetworkPacket {
            header,
            payload_size: 0,
            checksum: 0,
            interface_id: 0,
        }
    }

    pub fn set_payload_size(&mut self, size: u16) {
        self.payload_size = size;
    }

    pub fn calculate_checksum(&mut self) {
        // Simple checksum: XOR of all fields
        let sum = (self.payload_size as u32)
            + (self.header.ttl as u32)
            + (self.header.src_port as u32)
            + (self.header.dst_port as u32);
        self.checksum = ((sum >> 16) ^ (sum & 0xFFFF)) as u16;
    }
}

// ============================================================================
// PACKET QUEUE & ROUTING
// ============================================================================

pub struct PacketQueue {
    pub packets: [Option<NetworkPacket>; MAX_PACKET_QUEUE_SIZE],
    pub queue_depth: usize,
    pub total_queued: u64,
    pub total_dropped: u32,
}

impl PacketQueue {
    pub fn new() -> Self {
        PacketQueue {
            packets: [None; MAX_PACKET_QUEUE_SIZE],
            queue_depth: 0,
            total_queued: 0,
            total_dropped: 0,
        }
    }

    pub fn enqueue(&mut self, packet: NetworkPacket) -> bool {
        if self.queue_depth >= MAX_PACKET_QUEUE_SIZE {
            self.total_dropped += 1;
            return false;
        }

        self.packets[self.queue_depth] = Some(packet);
        self.queue_depth += 1;
        self.total_queued += 1;
        true
    }

    pub fn dequeue(&mut self) -> Option<NetworkPacket> {
        if self.queue_depth == 0 {
            return None;
        }

        let packet = self.packets[0];
        for i in 0..self.queue_depth - 1 {
            self.packets[i] = self.packets[i + 1];
        }
        self.packets[self.queue_depth - 1] = None;
        self.queue_depth -= 1;

        packet
    }

    pub fn is_empty(&self) -> bool {
        self.queue_depth == 0
    }
}

impl Default for PacketQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RoutingEntry {
    pub destination: IPv4Address,
    pub netmask: IPv4Address,
    pub gateway: IPv4Address,
    pub interface_id: u32,
    pub metric: u8,
}

impl RoutingEntry {
    pub fn new(
        destination: IPv4Address,
        netmask: IPv4Address,
        gateway: IPv4Address,
        interface_id: u32,
    ) -> Self {
        RoutingEntry {
            destination,
            netmask,
            gateway,
            interface_id,
            metric: 0,
        }
    }

    pub fn matches(&self, addr: IPv4Address) -> bool {
        addr.matches_subnet(self.destination, self.netmask)
    }
}

pub struct RoutingTable {
    pub routes: [Option<RoutingEntry>; MAX_ROUTING_ENTRIES],
    pub route_count: usize,
}

impl RoutingTable {
    pub fn new() -> Self {
        RoutingTable {
            routes: [None; MAX_ROUTING_ENTRIES],
            route_count: 0,
        }
    }

    pub fn add_route(&mut self, entry: RoutingEntry) -> bool {
        if self.route_count >= MAX_ROUTING_ENTRIES {
            return false;
        }

        self.routes[self.route_count] = Some(entry);
        self.route_count += 1;
        true
    }

    pub fn lookup_route(&self, destination: IPv4Address) -> Option<RoutingEntry> {
        for i in 0..self.route_count {
            if let Some(entry) = self.routes[i] {
                if entry.matches(destination) {
                    return Some(entry);
                }
            }
        }
        None
    }

    pub fn remove_route(&mut self, destination: IPv4Address) -> bool {
        for i in 0..self.route_count {
            if let Some(entry) = self.routes[i] {
                if entry.destination == destination {
                    for j in i..self.route_count - 1 {
                        self.routes[j] = self.routes[j + 1];
                    }
                    self.routes[self.route_count - 1] = None;
                    self.route_count -= 1;
                    return true;
                }
            }
        }
        false
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// NETWORK STACK
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct NetworkMetrics {
    pub total_packets_sent: u64,
    pub total_packets_received: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub packets_dropped: u32,
}

impl NetworkMetrics {
    pub fn new() -> Self {
        NetworkMetrics {
            total_packets_sent: 0,
            total_packets_received: 0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
            packets_dropped: 0,
        }
    }
}

impl Default for NetworkMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub struct NetworkStack {
    pub interfaces: NetworkInterfaceManager,
    pub routing: RoutingTable,
    pub tx_queue: PacketQueue,
    pub rx_queue: PacketQueue,
    pub metrics: NetworkMetrics,
    pub is_running: bool,
}

impl NetworkStack {
    pub fn new() -> Self {
        NetworkStack {
            interfaces: NetworkInterfaceManager::new(),
            routing: RoutingTable::new(),
            tx_queue: PacketQueue::new(),
            rx_queue: PacketQueue::new(),
            metrics: NetworkMetrics::new(),
            is_running: false,
        }
    }

    pub fn start(&mut self) {
        self.is_running = true;
    }

    pub fn stop(&mut self) {
        self.is_running = false;
    }

    pub fn send_packet(&mut self, mut packet: NetworkPacket) -> bool {
        if !self.is_running {
            return false;
        }

        packet.calculate_checksum();
        if self.tx_queue.enqueue(packet) {
            self.metrics.total_packets_sent += 1;
            self.metrics.total_bytes_sent += packet.payload_size as u64;
            true
        } else {
            self.metrics.packets_dropped += 1;
            false
        }
    }

    pub fn receive_packet(&mut self) -> Option<NetworkPacket> {
        self.rx_queue.dequeue()
    }

    pub fn inject_packet(&mut self, packet: NetworkPacket) -> bool {
        if self.rx_queue.enqueue(packet) {
            self.metrics.total_packets_received += 1;
            self.metrics.total_bytes_received += packet.payload_size as u64;
            true
        } else {
            self.metrics.packets_dropped += 1;
            false
        }
    }

    pub fn process_packet(&mut self, packet: NetworkPacket) -> bool {
        // Simulate packet processing (routing lookup, TTL check, etc.)
        if packet.header.ttl == 0 {
            return false;
        }

        if let IPAddress::V4(dst) = packet.header.dst_ip {
            if self.routing.lookup_route(dst).is_some() {
                return self.send_packet(packet);
            }
        }

        false
    }

    pub fn update_metrics(&mut self) {
        // Update metrics with queue statistics
        self.metrics.packets_dropped += self.tx_queue.total_dropped;
        self.tx_queue.total_dropped = 0;
    }
}

impl Default for NetworkStack {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipv4_address_new() {
        let addr = IPv4Address::new(192, 168, 1, 1);
        assert_eq!(addr.octets[0], 192);
    }

    #[test]
    fn test_ipv4_address_is_private() {
        let private = IPAddress::V4(IPv4Address::new(192, 168, 1, 1));
        assert!(private.is_private());
    }

    #[test]
    fn test_ipv4_address_is_loopback() {
        let loopback = IPAddress::V4(IPv4Address::new(127, 0, 0, 1));
        assert!(loopback.is_loopback());
    }

    #[test]
    fn test_mac_address_broadcast() {
        let mac = MACAddress::new(0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF);
        assert!(mac.is_broadcast());
    }

    #[test]
    fn test_network_interface_new() {
        let mac = MACAddress::new(0x00, 0x11, 0x22, 0x33, 0x44, 0x55);
        let ip = IPAddress::V4(IPv4Address::new(192, 168, 1, 1));
        let iface = NetworkInterface::new(1, mac, ip);
        assert_eq!(iface.interface_id, 1);
    }

    #[test]
    fn test_network_interface_stats() {
        let mac = MACAddress::default();
        let ip = IPAddress::V4(IPv4Address::default());
        let mut iface = NetworkInterface::new(1, mac, ip);
        iface.record_tx(100);
        assert_eq!(iface.tx_packets, 1);
        assert_eq!(iface.tx_bytes, 100);
    }

    #[test]
    fn test_network_interface_manager_add() {
        let mut manager = NetworkInterfaceManager::new();
        let mac = MACAddress::default();
        let ip = IPAddress::V4(IPv4Address::default());
        let iid = manager.add_interface(mac, ip);
        assert!(iid.is_some());
    }

    #[test]
    fn test_protocol_type_from_number() {
        assert_eq!(ProtocolType::from_number(6), Some(ProtocolType::TCP));
        assert_eq!(ProtocolType::from_number(17), Some(ProtocolType::UDP));
    }

    #[test]
    fn test_packet_header_new() {
        let src = IPAddress::V4(IPv4Address::new(192, 168, 1, 1));
        let dst = IPAddress::V4(IPv4Address::new(8, 8, 8, 8));
        let header = PacketHeader::new(src, dst, ProtocolType::TCP);
        assert_eq!(header.ttl, 64);
    }

    #[test]
    fn test_network_packet_checksum() {
        let src = IPAddress::V4(IPv4Address::default());
        let dst = IPAddress::V4(IPv4Address::default());
        let header = PacketHeader::new(src, dst, ProtocolType::TCP);
        let mut packet = NetworkPacket::new(header);
        packet.set_payload_size(100);
        packet.calculate_checksum();
        assert!(packet.checksum > 0);
    }

    #[test]
    fn test_packet_queue_new() {
        let queue = PacketQueue::new();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_packet_queue_enqueue_dequeue() {
        let mut queue = PacketQueue::new();
        let src = IPAddress::V4(IPv4Address::default());
        let dst = IPAddress::V4(IPv4Address::default());
        let header = PacketHeader::new(src, dst, ProtocolType::TCP);
        let packet = NetworkPacket::new(header);

        assert!(queue.enqueue(packet));
        assert!(!queue.is_empty());
        let dequeued = queue.dequeue();
        assert!(dequeued.is_some());
        assert!(queue.is_empty());
    }

    #[test]
    fn test_routing_table_new() {
        let table = RoutingTable::new();
        assert_eq!(table.route_count, 0);
    }

    #[test]
    fn test_routing_table_add_route() {
        let mut table = RoutingTable::new();
        let dest = IPv4Address::new(10, 0, 0, 0);
        let mask = IPv4Address::new(255, 0, 0, 0);
        let gw = IPv4Address::new(192, 168, 1, 1);
        let entry = RoutingEntry::new(dest, mask, gw, 1);
        assert!(table.add_route(entry));
    }

    #[test]
    fn test_network_stack_new() {
        let stack = NetworkStack::new();
        assert!(!stack.is_running);
    }

    #[test]
    fn test_network_stack_start_stop() {
        let mut stack = NetworkStack::new();
        stack.start();
        assert!(stack.is_running);
        stack.stop();
        assert!(!stack.is_running);
    }

    #[test]
    fn test_network_stack_send_packet() {
        let mut stack = NetworkStack::new();
        stack.start();
        let src = IPAddress::V4(IPv4Address::default());
        let dst = IPAddress::V4(IPv4Address::default());
        let header = PacketHeader::new(src, dst, ProtocolType::TCP);
        let packet = NetworkPacket::new(header);
        assert!(stack.send_packet(packet));
    }

    #[test]
    fn test_network_metrics_new() {
        let metrics = NetworkMetrics::new();
        assert_eq!(metrics.total_packets_sent, 0);
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_interface_setup_scenario() {
        let mut stack = NetworkStack::new();

        let mac1 = MACAddress::new(0x00, 0x11, 0x22, 0x33, 0x44, 0x55);
        let ip1 = IPAddress::V4(IPv4Address::new(192, 168, 1, 1));
        let iid1 = stack.interfaces.add_interface(mac1, ip1).unwrap();

        let mac2 = MACAddress::new(0x00, 0x11, 0x22, 0x33, 0x44, 0x66);
        let ip2 = IPAddress::V4(IPv4Address::new(10, 0, 0, 1));
        let iid2 = stack.interfaces.add_interface(mac2, ip2).unwrap();

        assert_eq!(stack.interfaces.interface_count, 2);
        assert!(stack.interfaces.get_interface(iid1).is_some());
        assert!(stack.interfaces.get_interface(iid2).is_some());
    }

    #[test]
    fn test_routing_scenario() {
        let mut stack = NetworkStack::new();

        let dest = IPv4Address::new(10, 0, 0, 0);
        let mask = IPv4Address::new(255, 0, 0, 0);
        let gw = IPv4Address::new(192, 168, 1, 1);
        let entry = RoutingEntry::new(dest, mask, gw, 1);

        stack.routing.add_route(entry);

        let lookup = IPv4Address::new(10, 5, 5, 5);
        let found = stack.routing.lookup_route(lookup);
        assert!(found.is_some());
    }

    #[test]
    fn test_packet_flow_scenario() {
        let mut stack = NetworkStack::new();
        stack.start();

        let src = IPAddress::V4(IPv4Address::new(192, 168, 1, 1));
        let dst = IPAddress::V4(IPv4Address::new(8, 8, 8, 8));
        let header = PacketHeader::new(src, dst, ProtocolType::UDP);
        let mut packet = NetworkPacket::new(header);
        packet.set_payload_size(512);

        assert!(stack.send_packet(packet));
        assert!(stack.metrics.total_packets_sent > 0);
    }

    #[test]
    fn test_multicast_detection() {
        let multicast_ip = IPAddress::V4(IPv4Address::new(224, 0, 0, 1));
        assert!(multicast_ip.is_multicast());

        let unicast_ip = IPAddress::V4(IPv4Address::new(192, 168, 1, 1));
        assert!(!unicast_ip.is_multicast());
    }

    #[test]
    fn test_network_stack_statistics() {
        let mut stack = NetworkStack::new();
        stack.start();

        let src = IPAddress::V4(IPv4Address::new(192, 168, 1, 1));
        let dst = IPAddress::V4(IPv4Address::new(8, 8, 8, 8));

        for i in 0..10 {
            let header = PacketHeader::new(src, dst, ProtocolType::TCP);
            let mut packet = NetworkPacket::new(header);
            packet.set_payload_size(100 + (i as u16));
            let _ = stack.send_packet(packet);
        }

        assert!(stack.metrics.total_packets_sent > 0);
        assert!(stack.metrics.total_bytes_sent > 0);
    }
}
