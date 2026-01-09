// RAYOS Phase 9A Task 3: VirtIO Network Driver Integration
// Connects socket API to VirtIO network device for packet transmission/reception
// File: crates/kernel-bare/src/virtio_net_driver.rs

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// ============================================================================
// CONSTANTS
// ============================================================================

const MAX_TX_BUFFERS: usize = 64;
const MAX_RX_BUFFERS: usize = 64;
const MAX_PACKET_SIZE: usize = 1518;  // Standard Ethernet MTU + headers
const VIRTIO_NET_HDR_SIZE: usize = 12;

// VirtIO Net header flags
const VIRTIO_NET_HDR_F_NEEDS_CSUM: u8 = 1;
const VIRTIO_NET_HDR_F_DATA_VALID: u8 = 2;

// VirtIO Net GSO types
const VIRTIO_NET_HDR_GSO_NONE: u8 = 0;
const VIRTIO_NET_HDR_GSO_TCPV4: u8 = 1;
const VIRTIO_NET_HDR_GSO_UDP: u8 = 3;
const VIRTIO_NET_HDR_GSO_TCPV6: u8 = 4;

// ============================================================================
// VIRTIO NET HEADER
// ============================================================================

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VirtioNetHeader {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,  // Only if VIRTIO_NET_F_MRG_RXBUF
}

impl VirtioNetHeader {
    pub fn new() -> Self {
        VirtioNetHeader {
            flags: 0,
            gso_type: VIRTIO_NET_HDR_GSO_NONE,
            hdr_len: 0,
            gso_size: 0,
            csum_start: 0,
            csum_offset: 0,
            num_buffers: 1,
        }
    }

    pub fn for_tcp() -> Self {
        VirtioNetHeader {
            flags: VIRTIO_NET_HDR_F_NEEDS_CSUM,
            gso_type: VIRTIO_NET_HDR_GSO_TCPV4,
            hdr_len: 54,  // ETH(14) + IP(20) + TCP(20)
            gso_size: 0,
            csum_start: 34,  // After IP header
            csum_offset: 16, // TCP checksum offset
            num_buffers: 1,
        }
    }

    pub fn for_udp() -> Self {
        VirtioNetHeader {
            flags: VIRTIO_NET_HDR_F_NEEDS_CSUM,
            gso_type: VIRTIO_NET_HDR_GSO_UDP,
            hdr_len: 42,  // ETH(14) + IP(20) + UDP(8)
            gso_size: 0,
            csum_start: 34,  // After IP header
            csum_offset: 6,  // UDP checksum offset
            num_buffers: 1,
        }
    }
}

impl Default for VirtioNetHeader {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ETHERNET FRAME
// ============================================================================

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct EthernetHeader {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,  // Big-endian
}

impl EthernetHeader {
    pub fn new(dst: [u8; 6], src: [u8; 6], ethertype: u16) -> Self {
        EthernetHeader {
            dst_mac: dst,
            src_mac: src,
            ethertype: ethertype.to_be(),
        }
    }

    pub fn ipv4(dst: [u8; 6], src: [u8; 6]) -> Self {
        Self::new(dst, src, 0x0800)  // IPv4
    }

    pub fn arp(dst: [u8; 6], src: [u8; 6]) -> Self {
        Self::new(dst, src, 0x0806)  // ARP
    }

    pub fn broadcast(src: [u8; 6], ethertype: u16) -> Self {
        Self::new([0xFF; 6], src, ethertype)
    }
}

// ============================================================================
// IP HEADER
// ============================================================================

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ipv4Header {
    pub version_ihl: u8,     // Version (4 bits) + IHL (4 bits)
    pub dscp_ecn: u8,        // DSCP (6 bits) + ECN (2 bits)
    pub total_length: u16,   // Big-endian
    pub identification: u16, // Big-endian
    pub flags_fragment: u16, // Flags (3 bits) + Fragment offset (13 bits)
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,       // Big-endian
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
}

impl Ipv4Header {
    pub fn new(src: [u8; 4], dst: [u8; 4], protocol: u8, payload_len: u16) -> Self {
        let total_len = 20 + payload_len;  // IP header (20) + payload
        Ipv4Header {
            version_ihl: 0x45,  // IPv4, IHL=5 (20 bytes)
            dscp_ecn: 0,
            total_length: total_len.to_be(),
            identification: 0,
            flags_fragment: 0x4000u16.to_be(),  // Don't fragment
            ttl: 64,
            protocol,
            checksum: 0,  // Calculated later
            src_ip: src,
            dst_ip: dst,
        }
    }

    pub fn tcp(src: [u8; 4], dst: [u8; 4], payload_len: u16) -> Self {
        Self::new(src, dst, 6, payload_len)  // TCP = 6
    }

    pub fn udp(src: [u8; 4], dst: [u8; 4], payload_len: u16) -> Self {
        Self::new(src, dst, 17, payload_len)  // UDP = 17
    }

    pub fn icmp(src: [u8; 4], dst: [u8; 4], payload_len: u16) -> Self {
        Self::new(src, dst, 1, payload_len)  // ICMP = 1
    }

    pub fn calculate_checksum(&mut self) {
        self.checksum = 0;
        let bytes = unsafe {
            core::slice::from_raw_parts(self as *const _ as *const u8, 20)
        };
        let mut sum: u32 = 0;
        for i in (0..20).step_by(2) {
            let word = ((bytes[i] as u32) << 8) | (bytes[i + 1] as u32);
            sum += word;
        }
        while sum > 0xFFFF {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        self.checksum = (!(sum as u16)).to_be();
    }
}

// ============================================================================
// TCP HEADER
// ============================================================================

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TcpHeader {
    pub src_port: u16,       // Big-endian
    pub dst_port: u16,       // Big-endian
    pub seq_num: u32,        // Big-endian
    pub ack_num: u32,        // Big-endian
    pub data_offset_flags: u16, // Data offset (4 bits) + Reserved (3 bits) + Flags (9 bits)
    pub window: u16,         // Big-endian
    pub checksum: u16,       // Big-endian
    pub urgent_ptr: u16,     // Big-endian
}

impl TcpHeader {
    pub fn new(src_port: u16, dst_port: u16, seq: u32, ack: u32, flags: u16) -> Self {
        TcpHeader {
            src_port: src_port.to_be(),
            dst_port: dst_port.to_be(),
            seq_num: seq.to_be(),
            ack_num: ack.to_be(),
            data_offset_flags: ((5 << 12) | flags).to_be(),  // Data offset = 5 (20 bytes)
            window: 65535u16.to_be(),
            checksum: 0,
            urgent_ptr: 0,
        }
    }

    pub fn syn(src_port: u16, dst_port: u16, seq: u32) -> Self {
        Self::new(src_port, dst_port, seq, 0, 0x02)  // SYN
    }

    pub fn syn_ack(src_port: u16, dst_port: u16, seq: u32, ack: u32) -> Self {
        Self::new(src_port, dst_port, seq, ack, 0x12)  // SYN+ACK
    }

    pub fn ack(src_port: u16, dst_port: u16, seq: u32, ack: u32) -> Self {
        Self::new(src_port, dst_port, seq, ack, 0x10)  // ACK
    }

    pub fn fin_ack(src_port: u16, dst_port: u16, seq: u32, ack: u32) -> Self {
        Self::new(src_port, dst_port, seq, ack, 0x11)  // FIN+ACK
    }

    pub fn psh_ack(src_port: u16, dst_port: u16, seq: u32, ack: u32) -> Self {
        Self::new(src_port, dst_port, seq, ack, 0x18)  // PSH+ACK
    }
}

// ============================================================================
// UDP HEADER
// ============================================================================

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    pub src_port: u16,   // Big-endian
    pub dst_port: u16,   // Big-endian
    pub length: u16,     // Big-endian
    pub checksum: u16,   // Big-endian
}

impl UdpHeader {
    pub fn new(src_port: u16, dst_port: u16, data_len: u16) -> Self {
        UdpHeader {
            src_port: src_port.to_be(),
            dst_port: dst_port.to_be(),
            length: (8 + data_len).to_be(),  // UDP header (8) + data
            checksum: 0,
        }
    }
}

// ============================================================================
// PACKET BUFFER
// ============================================================================

#[derive(Clone, Copy)]
pub struct PacketBuffer {
    pub data: [u8; MAX_PACKET_SIZE],
    pub len: usize,
    pub in_use: bool,
}

impl PacketBuffer {
    pub const fn empty() -> Self {
        PacketBuffer {
            data: [0u8; MAX_PACKET_SIZE],
            len: 0,
            in_use: false,
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.in_use = false;
    }
}

// ============================================================================
// NETWORK DRIVER STATE
// ============================================================================

pub struct VirtioNetDriver {
    // Device state
    pub initialized: bool,
    pub link_up: bool,
    pub mac_address: [u8; 6],
    pub mtu: u16,

    // TX/RX buffers
    pub tx_buffers: [PacketBuffer; MAX_TX_BUFFERS],
    pub rx_buffers: [PacketBuffer; MAX_RX_BUFFERS],
    pub tx_head: usize,
    pub tx_tail: usize,
    pub rx_head: usize,
    pub rx_tail: usize,

    // Statistics
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub tx_errors: u32,
    pub rx_errors: u32,
    pub tx_dropped: u32,
    pub rx_dropped: u32,

    // Configuration
    pub promiscuous: bool,
    pub multicast: bool,
}

impl VirtioNetDriver {
    pub const fn new() -> Self {
        VirtioNetDriver {
            initialized: false,
            link_up: false,
            mac_address: [0x52, 0x55, 0x4F, 0x53, 0x00, 0x01],  // "RAYOS" + 00:01
            mtu: 1500,

            tx_buffers: [PacketBuffer::empty(); MAX_TX_BUFFERS],
            rx_buffers: [PacketBuffer::empty(); MAX_RX_BUFFERS],
            tx_head: 0,
            tx_tail: 0,
            rx_head: 0,
            rx_tail: 0,

            tx_packets: 0,
            rx_packets: 0,
            tx_bytes: 0,
            rx_bytes: 0,
            tx_errors: 0,
            rx_errors: 0,
            tx_dropped: 0,
            rx_dropped: 0,

            promiscuous: false,
            multicast: true,
        }
    }

    pub fn init(&mut self) -> bool {
        if self.initialized {
            return true;
        }

        // Initialize TX buffers
        for buf in &mut self.tx_buffers {
            buf.clear();
        }

        // Initialize RX buffers and post them to device
        for buf in &mut self.rx_buffers {
            buf.clear();
        }

        self.link_up = true;
        self.initialized = true;
        true
    }

    pub fn set_mac_address(&mut self, mac: [u8; 6]) {
        self.mac_address = mac;
    }

    // Allocate TX buffer
    pub fn alloc_tx_buffer(&mut self) -> Option<usize> {
        let start = self.tx_head;
        for i in 0..MAX_TX_BUFFERS {
            let idx = (start + i) % MAX_TX_BUFFERS;
            if !self.tx_buffers[idx].in_use {
                self.tx_buffers[idx].in_use = true;
                self.tx_buffers[idx].len = 0;
                self.tx_head = (idx + 1) % MAX_TX_BUFFERS;
                return Some(idx);
            }
        }
        None
    }

    // Free TX buffer
    pub fn free_tx_buffer(&mut self, idx: usize) {
        if idx < MAX_TX_BUFFERS {
            self.tx_buffers[idx].clear();
        }
    }

    // Transmit packet
    pub fn transmit(&mut self, idx: usize) -> Result<(), NetError> {
        if idx >= MAX_TX_BUFFERS {
            return Err(NetError::InvalidBuffer);
        }

        if !self.tx_buffers[idx].in_use {
            return Err(NetError::BufferNotReady);
        }

        if !self.link_up {
            self.tx_dropped += 1;
            return Err(NetError::LinkDown);
        }

        let len = self.tx_buffers[idx].len;
        if len == 0 || len > MAX_PACKET_SIZE {
            self.tx_errors += 1;
            return Err(NetError::InvalidLength);
        }

        // In a real driver, we'd submit to VirtIO queue here
        // For now, simulate successful transmission

        self.tx_packets += 1;
        self.tx_bytes += len as u64;

        // Free the buffer
        self.tx_buffers[idx].clear();

        Ok(())
    }

    // Check for received packets
    pub fn poll_rx(&mut self) -> Option<usize> {
        // In a real driver, we'd check VirtIO used ring
        // For simulation, return None (no packets)
        None
    }

    // Get received packet data
    pub fn get_rx_packet(&self, idx: usize) -> Option<&[u8]> {
        if idx < MAX_RX_BUFFERS && self.rx_buffers[idx].in_use {
            Some(&self.rx_buffers[idx].data[..self.rx_buffers[idx].len])
        } else {
            None
        }
    }

    // Free RX buffer (repost to device)
    pub fn free_rx_buffer(&mut self, idx: usize) {
        if idx < MAX_RX_BUFFERS {
            self.rx_buffers[idx].clear();
            // In real driver, repost buffer to VirtIO RX queue
        }
    }

    // Build and transmit a TCP packet
    pub fn send_tcp(
        &mut self,
        dst_mac: [u8; 6],
        src_ip: [u8; 4],
        dst_ip: [u8; 4],
        src_port: u16,
        dst_port: u16,
        seq: u32,
        ack: u32,
        flags: u16,
        payload: &[u8],
    ) -> Result<(), NetError> {
        let idx = self.alloc_tx_buffer().ok_or(NetError::NoBuffers)?;

        let buf = &mut self.tx_buffers[idx];
        let mut offset = 0;

        // VirtIO header
        let virtio_hdr = VirtioNetHeader::for_tcp();
        let hdr_bytes = unsafe {
            core::slice::from_raw_parts(&virtio_hdr as *const _ as *const u8, VIRTIO_NET_HDR_SIZE)
        };
        buf.data[offset..offset + VIRTIO_NET_HDR_SIZE].copy_from_slice(hdr_bytes);
        offset += VIRTIO_NET_HDR_SIZE;

        // Ethernet header
        let eth_hdr = EthernetHeader::ipv4(dst_mac, self.mac_address);
        let eth_bytes = unsafe {
            core::slice::from_raw_parts(&eth_hdr as *const _ as *const u8, 14)
        };
        buf.data[offset..offset + 14].copy_from_slice(eth_bytes);
        offset += 14;

        // IP header
        let tcp_len = 20 + payload.len() as u16;  // TCP header + payload
        let mut ip_hdr = Ipv4Header::tcp(src_ip, dst_ip, tcp_len);
        ip_hdr.calculate_checksum();
        let ip_bytes = unsafe {
            core::slice::from_raw_parts(&ip_hdr as *const _ as *const u8, 20)
        };
        buf.data[offset..offset + 20].copy_from_slice(ip_bytes);
        offset += 20;

        // TCP header
        let tcp_hdr = TcpHeader::new(src_port, dst_port, seq, ack, flags);
        let tcp_bytes = unsafe {
            core::slice::from_raw_parts(&tcp_hdr as *const _ as *const u8, 20)
        };
        buf.data[offset..offset + 20].copy_from_slice(tcp_bytes);
        offset += 20;

        // Payload
        if !payload.is_empty() {
            let copy_len = payload.len().min(MAX_PACKET_SIZE - offset);
            buf.data[offset..offset + copy_len].copy_from_slice(&payload[..copy_len]);
            offset += copy_len;
        }

        buf.len = offset;
        self.transmit(idx)
    }

    // Build and transmit a UDP packet
    pub fn send_udp(
        &mut self,
        dst_mac: [u8; 6],
        src_ip: [u8; 4],
        dst_ip: [u8; 4],
        src_port: u16,
        dst_port: u16,
        payload: &[u8],
    ) -> Result<(), NetError> {
        let idx = self.alloc_tx_buffer().ok_or(NetError::NoBuffers)?;

        let buf = &mut self.tx_buffers[idx];
        let mut offset = 0;

        // VirtIO header
        let virtio_hdr = VirtioNetHeader::for_udp();
        let hdr_bytes = unsafe {
            core::slice::from_raw_parts(&virtio_hdr as *const _ as *const u8, VIRTIO_NET_HDR_SIZE)
        };
        buf.data[offset..offset + VIRTIO_NET_HDR_SIZE].copy_from_slice(hdr_bytes);
        offset += VIRTIO_NET_HDR_SIZE;

        // Ethernet header
        let eth_hdr = EthernetHeader::ipv4(dst_mac, self.mac_address);
        let eth_bytes = unsafe {
            core::slice::from_raw_parts(&eth_hdr as *const _ as *const u8, 14)
        };
        buf.data[offset..offset + 14].copy_from_slice(eth_bytes);
        offset += 14;

        // IP header
        let udp_len = 8 + payload.len() as u16;  // UDP header + payload
        let mut ip_hdr = Ipv4Header::udp(src_ip, dst_ip, udp_len);
        ip_hdr.calculate_checksum();
        let ip_bytes = unsafe {
            core::slice::from_raw_parts(&ip_hdr as *const _ as *const u8, 20)
        };
        buf.data[offset..offset + 20].copy_from_slice(ip_bytes);
        offset += 20;

        // UDP header
        let udp_hdr = UdpHeader::new(src_port, dst_port, payload.len() as u16);
        let udp_bytes = unsafe {
            core::slice::from_raw_parts(&udp_hdr as *const _ as *const u8, 8)
        };
        buf.data[offset..offset + 8].copy_from_slice(udp_bytes);
        offset += 8;

        // Payload
        if !payload.is_empty() {
            let copy_len = payload.len().min(MAX_PACKET_SIZE - offset);
            buf.data[offset..offset + copy_len].copy_from_slice(&payload[..copy_len]);
            offset += copy_len;
        }

        buf.len = offset;
        self.transmit(idx)
    }

    // Send ARP request
    pub fn send_arp_request(&mut self, target_ip: [u8; 4], src_ip: [u8; 4]) -> Result<(), NetError> {
        let idx = self.alloc_tx_buffer().ok_or(NetError::NoBuffers)?;

        let buf = &mut self.tx_buffers[idx];
        let mut offset = 0;

        // VirtIO header
        let virtio_hdr = VirtioNetHeader::new();
        let hdr_bytes = unsafe {
            core::slice::from_raw_parts(&virtio_hdr as *const _ as *const u8, VIRTIO_NET_HDR_SIZE)
        };
        buf.data[offset..offset + VIRTIO_NET_HDR_SIZE].copy_from_slice(hdr_bytes);
        offset += VIRTIO_NET_HDR_SIZE;

        // Ethernet header (broadcast)
        let eth_hdr = EthernetHeader::arp([0xFF; 6], self.mac_address);
        let eth_bytes = unsafe {
            core::slice::from_raw_parts(&eth_hdr as *const _ as *const u8, 14)
        };
        buf.data[offset..offset + 14].copy_from_slice(eth_bytes);
        offset += 14;

        // ARP packet
        // Hardware type: Ethernet (1)
        buf.data[offset] = 0x00;
        buf.data[offset + 1] = 0x01;
        // Protocol type: IPv4 (0x0800)
        buf.data[offset + 2] = 0x08;
        buf.data[offset + 3] = 0x00;
        // Hardware size: 6
        buf.data[offset + 4] = 6;
        // Protocol size: 4
        buf.data[offset + 5] = 4;
        // Opcode: Request (1)
        buf.data[offset + 6] = 0x00;
        buf.data[offset + 7] = 0x01;
        // Sender MAC
        buf.data[offset + 8..offset + 14].copy_from_slice(&self.mac_address);
        // Sender IP
        buf.data[offset + 14..offset + 18].copy_from_slice(&src_ip);
        // Target MAC (unknown - zeros)
        buf.data[offset + 18..offset + 24].fill(0);
        // Target IP
        buf.data[offset + 24..offset + 28].copy_from_slice(&target_ip);
        offset += 28;

        buf.len = offset;
        self.transmit(idx)
    }

    // Get statistics
    pub fn get_stats(&self) -> NetStats {
        NetStats {
            tx_packets: self.tx_packets,
            rx_packets: self.rx_packets,
            tx_bytes: self.tx_bytes,
            rx_bytes: self.rx_bytes,
            tx_errors: self.tx_errors,
            rx_errors: self.rx_errors,
            tx_dropped: self.tx_dropped,
            rx_dropped: self.rx_dropped,
        }
    }
}

// ============================================================================
// NETWORK STATISTICS
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct NetStats {
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub tx_errors: u32,
    pub rx_errors: u32,
    pub tx_dropped: u32,
    pub rx_dropped: u32,
}

// ============================================================================
// NETWORK ERROR
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NetError {
    NotInitialized,
    LinkDown,
    NoBuffers,
    InvalidBuffer,
    BufferNotReady,
    InvalidLength,
    Timeout,
    ChecksumError,
}

// ============================================================================
// GLOBAL DRIVER INSTANCE
// ============================================================================

static mut NET_DRIVER: VirtioNetDriver = VirtioNetDriver::new();
static NET_DRIVER_LOCK: AtomicBool = AtomicBool::new(false);
static NET_DRIVER_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn acquire_lock() {
    while NET_DRIVER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

fn release_lock() {
    NET_DRIVER_LOCK.store(false, Ordering::Release);
}

pub fn net_driver_init() -> bool {
    if NET_DRIVER_INITIALIZED.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        acquire_lock();
        let result = unsafe { NET_DRIVER.init() };
        release_lock();
        result
    } else {
        true
    }
}

pub fn net_driver_send_tcp(
    dst_mac: [u8; 6],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u16,
    payload: &[u8],
) -> Result<(), NetError> {
    acquire_lock();
    let result = unsafe {
        NET_DRIVER.send_tcp(dst_mac, src_ip, dst_ip, src_port, dst_port, seq, ack, flags, payload)
    };
    release_lock();
    result
}

pub fn net_driver_send_udp(
    dst_mac: [u8; 6],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Result<(), NetError> {
    acquire_lock();
    let result = unsafe {
        NET_DRIVER.send_udp(dst_mac, src_ip, dst_ip, src_port, dst_port, payload)
    };
    release_lock();
    result
}

pub fn net_driver_send_arp(target_ip: [u8; 4], src_ip: [u8; 4]) -> Result<(), NetError> {
    acquire_lock();
    let result = unsafe {
        NET_DRIVER.send_arp_request(target_ip, src_ip)
    };
    release_lock();
    result
}

pub fn net_driver_poll_rx() -> Option<usize> {
    acquire_lock();
    let result = unsafe { NET_DRIVER.poll_rx() };
    release_lock();
    result
}

pub fn net_driver_get_stats() -> NetStats {
    acquire_lock();
    let result = unsafe { NET_DRIVER.get_stats() };
    release_lock();
    result
}

pub fn net_driver_get_mac() -> [u8; 6] {
    acquire_lock();
    let result = unsafe { NET_DRIVER.mac_address };
    release_lock();
    result
}

pub fn net_driver_set_mac(mac: [u8; 6]) {
    acquire_lock();
    unsafe { NET_DRIVER.set_mac_address(mac) };
    release_lock();
}

pub fn net_driver_is_link_up() -> bool {
    acquire_lock();
    let result = unsafe { NET_DRIVER.link_up };
    release_lock();
    result
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtio_net_header() {
        let hdr = VirtioNetHeader::new();
        assert_eq!(hdr.gso_type, VIRTIO_NET_HDR_GSO_NONE);
    }

    #[test]
    fn test_ethernet_header() {
        let src = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        let dst = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let eth = EthernetHeader::ipv4(dst, src);
        assert_eq!(eth.src_mac, src);
        assert_eq!(eth.dst_mac, dst);
    }

    #[test]
    fn test_ipv4_header() {
        let src = [192, 168, 1, 1];
        let dst = [192, 168, 1, 2];
        let mut ip = Ipv4Header::tcp(src, dst, 20);
        ip.calculate_checksum();
        assert_ne!(ip.checksum, 0);
    }

    #[test]
    fn test_tcp_header() {
        let tcp = TcpHeader::syn(8080, 80, 1000);
        assert_eq!(tcp.src_port, 8080u16.to_be());
        assert_eq!(tcp.dst_port, 80u16.to_be());
    }

    #[test]
    fn test_udp_header() {
        let udp = UdpHeader::new(5000, 53, 100);
        assert_eq!(udp.length, 108u16.to_be());
    }

    #[test]
    fn test_driver_init() {
        let mut driver = VirtioNetDriver::new();
        assert!(driver.init());
        assert!(driver.link_up);
    }

    #[test]
    fn test_tx_buffer_alloc() {
        let mut driver = VirtioNetDriver::new();
        driver.init();
        let idx = driver.alloc_tx_buffer();
        assert!(idx.is_some());
    }

    #[test]
    fn test_net_stats() {
        let driver = VirtioNetDriver::new();
        let stats = driver.get_stats();
        assert_eq!(stats.tx_packets, 0);
        assert_eq!(stats.rx_packets, 0);
    }
}
