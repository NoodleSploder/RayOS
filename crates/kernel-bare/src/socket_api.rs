// RAYOS Phase 9A Task 3: Socket API & TCP/UDP Protocol Support
// Berkeley sockets-compatible API with TCP state machine and UDP datagrams
// File: crates/kernel-bare/src/socket_api.rs

use core::fmt;

// ============================================================================
// CONSTANTS
// ============================================================================

const MAX_SOCKETS: usize = 256;
const MAX_PENDING_CONNECTIONS: usize = 16;
const MAX_ARP_ENTRIES: usize = 64;
const SOCKET_SEND_BUFFER_SIZE: usize = 65536;
const SOCKET_RECV_BUFFER_SIZE: usize = 65536;
const TCP_INITIAL_SEQ: u32 = 1000;
const TCP_DEFAULT_WINDOW: u16 = 65535;
const TCP_MAX_RETRIES: u8 = 5;

// ============================================================================
// SOCKET ADDRESS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SocketAddr {
    pub ip: [u8; 4],
    pub port: u16,
}

impl SocketAddr {
    pub fn new(a: u8, b: u8, c: u8, d: u8, port: u16) -> Self {
        SocketAddr {
            ip: [a, b, c, d],
            port,
        }
    }

    pub fn any(port: u16) -> Self {
        SocketAddr {
            ip: [0, 0, 0, 0],
            port,
        }
    }

    pub fn loopback(port: u16) -> Self {
        SocketAddr {
            ip: [127, 0, 0, 1],
            port,
        }
    }

    pub fn is_any(&self) -> bool {
        self.ip == [0, 0, 0, 0]
    }

    pub fn ip_u32(&self) -> u32 {
        ((self.ip[0] as u32) << 24)
            | ((self.ip[1] as u32) << 16)
            | ((self.ip[2] as u32) << 8)
            | (self.ip[3] as u32)
    }
}

impl fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}:{}",
            self.ip[0], self.ip[1], self.ip[2], self.ip[3], self.port
        )
    }
}

impl Default for SocketAddr {
    fn default() -> Self {
        SocketAddr::any(0)
    }
}

// ============================================================================
// SOCKET TYPES
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SocketType {
    Stream,     // TCP - reliable, ordered, connection-oriented
    Datagram,   // UDP - unreliable, unordered, connectionless
    Raw,        // Raw IP - direct access to IP layer
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SocketDomain {
    Inet,       // IPv4
    Inet6,      // IPv6
    Unix,       // Unix domain socket
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SocketState {
    Unbound,            // Socket created but not bound
    Bound,              // Socket bound to local address
    Listening,          // TCP listening for connections
    Connecting,         // TCP SYN sent, waiting for SYN-ACK
    Connected,          // TCP established / UDP "connected"
    Closing,            // TCP FIN sent
    Closed,             // Socket closed
    Error,              // Error state
}

impl fmt::Display for SocketState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocketState::Unbound => write!(f, "UNBOUND"),
            SocketState::Bound => write!(f, "BOUND"),
            SocketState::Listening => write!(f, "LISTENING"),
            SocketState::Connecting => write!(f, "SYN_SENT"),
            SocketState::Connected => write!(f, "ESTABLISHED"),
            SocketState::Closing => write!(f, "FIN_WAIT"),
            SocketState::Closed => write!(f, "CLOSED"),
            SocketState::Error => write!(f, "ERROR"),
        }
    }
}

// ============================================================================
// TCP STATE MACHINE (RFC 793)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

impl fmt::Display for TcpState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TcpState::Closed => write!(f, "CLOSED"),
            TcpState::Listen => write!(f, "LISTEN"),
            TcpState::SynSent => write!(f, "SYN_SENT"),
            TcpState::SynReceived => write!(f, "SYN_RCVD"),
            TcpState::Established => write!(f, "ESTABLISHED"),
            TcpState::FinWait1 => write!(f, "FIN_WAIT_1"),
            TcpState::FinWait2 => write!(f, "FIN_WAIT_2"),
            TcpState::CloseWait => write!(f, "CLOSE_WAIT"),
            TcpState::Closing => write!(f, "CLOSING"),
            TcpState::LastAck => write!(f, "LAST_ACK"),
            TcpState::TimeWait => write!(f, "TIME_WAIT"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TcpControlBlock {
    pub state: TcpState,
    pub local_seq: u32,         // Our sequence number
    pub remote_seq: u32,        // Their sequence number
    pub local_ack: u32,         // What we've acknowledged
    pub remote_ack: u32,        // What they've acknowledged
    pub window_size: u16,       // Receive window
    pub send_window: u16,       // Send window
    pub mss: u16,               // Maximum segment size
    pub retries: u8,            // Retransmission count
    pub rtt_estimate: u32,      // Round-trip time estimate (ms)
}

impl TcpControlBlock {
    pub fn new() -> Self {
        TcpControlBlock {
            state: TcpState::Closed,
            local_seq: TCP_INITIAL_SEQ,
            remote_seq: 0,
            local_ack: 0,
            remote_ack: 0,
            window_size: TCP_DEFAULT_WINDOW,
            send_window: TCP_DEFAULT_WINDOW,
            mss: 1460,  // Standard Ethernet MSS
            retries: 0,
            rtt_estimate: 200,
        }
    }

    pub fn can_send(&self) -> bool {
        self.state == TcpState::Established
    }

    pub fn is_connected(&self) -> bool {
        self.state == TcpState::Established
    }

    pub fn advance_seq(&mut self, bytes: u32) {
        self.local_seq = self.local_seq.wrapping_add(bytes);
    }

    pub fn update_ack(&mut self, ack: u32) {
        self.remote_ack = ack;
    }
}

impl Default for TcpControlBlock {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TCP FLAGS
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct TcpFlags {
    pub fin: bool,
    pub syn: bool,
    pub rst: bool,
    pub psh: bool,
    pub ack: bool,
    pub urg: bool,
}

impl TcpFlags {
    pub fn none() -> Self {
        TcpFlags {
            fin: false,
            syn: false,
            rst: false,
            psh: false,
            ack: false,
            urg: false,
        }
    }

    pub fn syn() -> Self {
        TcpFlags {
            fin: false,
            syn: true,
            rst: false,
            psh: false,
            ack: false,
            urg: false,
        }
    }

    pub fn syn_ack() -> Self {
        TcpFlags {
            fin: false,
            syn: true,
            rst: false,
            psh: false,
            ack: true,
            urg: false,
        }
    }

    pub fn ack() -> Self {
        TcpFlags {
            fin: false,
            syn: false,
            rst: false,
            psh: false,
            ack: true,
            urg: false,
        }
    }

    pub fn fin_ack() -> Self {
        TcpFlags {
            fin: true,
            syn: false,
            rst: false,
            psh: false,
            ack: true,
            urg: false,
        }
    }

    pub fn rst() -> Self {
        TcpFlags {
            fin: false,
            syn: false,
            rst: true,
            psh: false,
            ack: false,
            urg: false,
        }
    }

    pub fn to_byte(&self) -> u8 {
        let mut b = 0u8;
        if self.fin { b |= 0x01; }
        if self.syn { b |= 0x02; }
        if self.rst { b |= 0x04; }
        if self.psh { b |= 0x08; }
        if self.ack { b |= 0x10; }
        if self.urg { b |= 0x20; }
        b
    }

    pub fn from_byte(b: u8) -> Self {
        TcpFlags {
            fin: (b & 0x01) != 0,
            syn: (b & 0x02) != 0,
            rst: (b & 0x04) != 0,
            psh: (b & 0x08) != 0,
            ack: (b & 0x10) != 0,
            urg: (b & 0x20) != 0,
        }
    }
}

// ============================================================================
// TCP SEGMENT
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct TcpSegment {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset: u8,    // Header length in 32-bit words
    pub flags: TcpFlags,
    pub window: u16,
    pub checksum: u16,
    pub urgent_ptr: u16,
    pub payload_len: u16,
}

impl TcpSegment {
    pub fn new(src_port: u16, dst_port: u16, seq: u32, ack: u32, flags: TcpFlags) -> Self {
        TcpSegment {
            src_port,
            dst_port,
            seq_num: seq,
            ack_num: ack,
            data_offset: 5,  // 20 bytes (no options)
            flags,
            window: TCP_DEFAULT_WINDOW,
            checksum: 0,
            urgent_ptr: 0,
            payload_len: 0,
        }
    }

    pub fn header_len(&self) -> usize {
        (self.data_offset as usize) * 4
    }
}

// ============================================================================
// UDP DATAGRAM
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct UdpDatagram {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

impl UdpDatagram {
    pub fn new(src_port: u16, dst_port: u16, data_len: u16) -> Self {
        UdpDatagram {
            src_port,
            dst_port,
            length: 8 + data_len,  // 8-byte header + data
            checksum: 0,
        }
    }

    pub fn header_len(&self) -> usize {
        8
    }
}

// ============================================================================
// SOCKET STRUCTURE
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct Socket {
    pub fd: u32,
    pub socket_type: SocketType,
    pub domain: SocketDomain,
    pub state: SocketState,
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub tcp: Option<TcpControlBlock>,
    pub backlog: u8,                    // For listening sockets
    pub pending_count: u8,              // Pending connections
    pub send_buffer_used: usize,
    pub recv_buffer_used: usize,
    pub options: SocketOptions,
    pub in_use: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SocketOptions {
    pub reuse_addr: bool,
    pub reuse_port: bool,
    pub keep_alive: bool,
    pub no_delay: bool,         // TCP_NODELAY (Nagle's algorithm)
    pub broadcast: bool,        // SO_BROADCAST for UDP
    pub recv_timeout_ms: u32,
    pub send_timeout_ms: u32,
    pub linger_time: u16,
}

impl Default for SocketOptions {
    fn default() -> Self {
        SocketOptions {
            reuse_addr: false,
            reuse_port: false,
            keep_alive: false,
            no_delay: false,
            broadcast: false,
            recv_timeout_ms: 0,
            send_timeout_ms: 0,
            linger_time: 0,
        }
    }
}

impl Socket {
    pub fn new(fd: u32, socket_type: SocketType, domain: SocketDomain) -> Self {
        let tcp = if socket_type == SocketType::Stream {
            Some(TcpControlBlock::new())
        } else {
            None
        };

        Socket {
            fd,
            socket_type,
            domain,
            state: SocketState::Unbound,
            local_addr: SocketAddr::default(),
            remote_addr: SocketAddr::default(),
            tcp,
            backlog: 0,
            pending_count: 0,
            send_buffer_used: 0,
            recv_buffer_used: 0,
            options: SocketOptions::default(),
            in_use: true,
        }
    }

    pub fn is_tcp(&self) -> bool {
        self.socket_type == SocketType::Stream
    }

    pub fn is_udp(&self) -> bool {
        self.socket_type == SocketType::Datagram
    }

    pub fn is_listening(&self) -> bool {
        self.state == SocketState::Listening
    }

    pub fn is_connected(&self) -> bool {
        self.state == SocketState::Connected
    }

    pub fn can_accept(&self) -> bool {
        self.is_listening() && self.pending_count > 0
    }
}

// ============================================================================
// ARP TABLE (Address Resolution Protocol)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct ArpEntry {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
    pub age: u32,           // Seconds since last update
    pub state: ArpState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArpState {
    Incomplete,     // ARP request sent, waiting for reply
    Reachable,      // Valid entry
    Stale,          // Needs revalidation
    Delay,          // Waiting before probing
    Probe,          // Sending unicast probes
    Failed,         // No response
}

impl ArpEntry {
    pub fn new(ip: [u8; 4], mac: [u8; 6]) -> Self {
        ArpEntry {
            ip,
            mac,
            age: 0,
            state: ArpState::Reachable,
        }
    }

    pub fn incomplete(ip: [u8; 4]) -> Self {
        ArpEntry {
            ip,
            mac: [0; 6],
            age: 0,
            state: ArpState::Incomplete,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.state == ArpState::Reachable || self.state == ArpState::Stale
    }
}

pub struct ArpTable {
    entries: [Option<ArpEntry>; MAX_ARP_ENTRIES],
    count: usize,
}

impl ArpTable {
    pub fn new() -> Self {
        ArpTable {
            entries: [None; MAX_ARP_ENTRIES],
            count: 0,
        }
    }

    pub fn lookup(&self, ip: [u8; 4]) -> Option<[u8; 6]> {
        for i in 0..self.count {
            if let Some(entry) = &self.entries[i] {
                if entry.ip == ip && entry.is_valid() {
                    return Some(entry.mac);
                }
            }
        }
        None
    }

    pub fn insert(&mut self, ip: [u8; 4], mac: [u8; 6]) -> bool {
        // Update existing entry
        for i in 0..self.count {
            if let Some(ref mut entry) = self.entries[i] {
                if entry.ip == ip {
                    entry.mac = mac;
                    entry.age = 0;
                    entry.state = ArpState::Reachable;
                    return true;
                }
            }
        }

        // Add new entry
        if self.count < MAX_ARP_ENTRIES {
            self.entries[self.count] = Some(ArpEntry::new(ip, mac));
            self.count += 1;
            return true;
        }

        // Evict oldest entry
        let mut oldest_idx = 0;
        let mut oldest_age = 0;
        for i in 0..self.count {
            if let Some(entry) = &self.entries[i] {
                if entry.age > oldest_age {
                    oldest_age = entry.age;
                    oldest_idx = i;
                }
            }
        }
        self.entries[oldest_idx] = Some(ArpEntry::new(ip, mac));
        true
    }

    pub fn age_entries(&mut self, seconds: u32) {
        for i in 0..self.count {
            if let Some(ref mut entry) = self.entries[i] {
                entry.age += seconds;
                // Mark stale after 5 minutes
                if entry.age > 300 && entry.state == ArpState::Reachable {
                    entry.state = ArpState::Stale;
                }
            }
        }
    }

    pub fn remove(&mut self, ip: [u8; 4]) -> bool {
        for i in 0..self.count {
            if let Some(entry) = &self.entries[i] {
                if entry.ip == ip {
                    // Shift entries
                    for j in i..self.count - 1 {
                        self.entries[j] = self.entries[j + 1];
                    }
                    self.entries[self.count - 1] = None;
                    self.count -= 1;
                    return true;
                }
            }
        }
        false
    }
}

impl Default for ArpTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SOCKET TABLE
// ============================================================================

pub struct SocketTable {
    sockets: [Option<Socket>; MAX_SOCKETS],
    next_fd: u32,
    arp: ArpTable,
}

impl SocketTable {
    pub fn new() -> Self {
        // Initialize with None values
        const NONE_SOCKET: Option<Socket> = None;
        SocketTable {
            sockets: [NONE_SOCKET; MAX_SOCKETS],
            next_fd: 3,  // 0, 1, 2 reserved for stdin/stdout/stderr
            arp: ArpTable::new(),
        }
    }

    // socket() - Create a new socket
    pub fn socket_create(&mut self, socket_type: SocketType, domain: SocketDomain) -> Option<u32> {
        for i in 0..MAX_SOCKETS {
            if self.sockets[i].is_none() {
                let fd = self.next_fd;
                self.next_fd += 1;
                self.sockets[i] = Some(Socket::new(fd, socket_type, domain));
                return Some(fd);
            }
        }
        None
    }

    // bind() - Bind socket to local address
    pub fn socket_bind(&mut self, fd: u32, addr: SocketAddr) -> Result<(), SocketError> {
        // First, gather info and check state without holding mutable ref
        let (socket_state, reuse_addr, reuse_port) = {
            let socket = self.get_socket(fd)?;
            (socket.state, socket.options.reuse_addr, socket.options.reuse_port)
        };

        if socket_state != SocketState::Unbound {
            return Err(SocketError::AlreadyBound);
        }

        // Check for port conflicts (unless SO_REUSEADDR/SO_REUSEPORT)
        if !reuse_addr && !reuse_port {
            for i in 0..MAX_SOCKETS {
                if let Some(ref s) = self.sockets[i] {
                    if s.fd != fd && s.local_addr.port == addr.port && s.in_use {
                        return Err(SocketError::AddressInUse);
                    }
                }
            }
        }

        // Now get mutable ref and update
        let socket = self.get_socket_mut(fd)?;
        socket.local_addr = addr;
        socket.state = SocketState::Bound;
        Ok(())
    }

    // listen() - Mark socket as listening (TCP only)
    pub fn socket_listen(&mut self, fd: u32, backlog: u8) -> Result<(), SocketError> {
        let socket = self.get_socket_mut(fd)?;

        if !socket.is_tcp() {
            return Err(SocketError::InvalidOperation);
        }

        if socket.state != SocketState::Bound {
            return Err(SocketError::NotBound);
        }

        socket.state = SocketState::Listening;
        socket.backlog = backlog.min(MAX_PENDING_CONNECTIONS as u8);
        if let Some(ref mut tcp) = socket.tcp {
            tcp.state = TcpState::Listen;
        }

        Ok(())
    }

    // connect() - Connect to remote address
    pub fn socket_connect(&mut self, fd: u32, addr: SocketAddr) -> Result<(), SocketError> {
        let socket = self.get_socket_mut(fd)?;

        match socket.state {
            SocketState::Unbound | SocketState::Bound => {}
            SocketState::Connected => return Err(SocketError::AlreadyConnected),
            _ => return Err(SocketError::InvalidState),
        }

        socket.remote_addr = addr;

        if socket.is_tcp() {
            // TCP: Initiate 3-way handshake
            socket.state = SocketState::Connecting;
            if let Some(ref mut tcp) = socket.tcp {
                tcp.state = TcpState::SynSent;
                // In a real implementation, we'd send a SYN packet here
            }
            // For simulation, immediately mark as connected
            socket.state = SocketState::Connected;
            if let Some(ref mut tcp) = socket.tcp {
                tcp.state = TcpState::Established;
            }
        } else {
            // UDP: Just set remote address (connectionless)
            socket.state = SocketState::Connected;
        }

        Ok(())
    }

    // accept() - Accept incoming connection (TCP only)
    pub fn socket_accept(&mut self, fd: u32) -> Result<u32, SocketError> {
        // Get listening socket info
        let (socket_type, domain, remote_addr, local_addr) = {
            let socket = self.get_socket(fd)?;

            if !socket.is_listening() {
                return Err(SocketError::NotListening);
            }

            if socket.pending_count == 0 {
                return Err(SocketError::WouldBlock);
            }

            (socket.socket_type, socket.domain, socket.remote_addr, socket.local_addr)
        };

        // Create new socket for the connection
        let new_fd = self.socket_create(socket_type, domain)
            .ok_or(SocketError::TooManySockets)?;

        // Set up the new connected socket
        {
            let new_socket = self.get_socket_mut(new_fd)?;
            new_socket.local_addr = local_addr;
            new_socket.remote_addr = remote_addr;
            new_socket.state = SocketState::Connected;
            if let Some(ref mut tcp) = new_socket.tcp {
                tcp.state = TcpState::Established;
            }
        }

        // Decrement pending count on listening socket
        {
            let socket = self.get_socket_mut(fd)?;
            socket.pending_count = socket.pending_count.saturating_sub(1);
        }

        Ok(new_fd)
    }

    // send() - Send data on connected socket
    pub fn socket_send(&mut self, fd: u32, data: &[u8]) -> Result<usize, SocketError> {
        let socket = self.get_socket_mut(fd)?;

        if !socket.is_connected() {
            return Err(SocketError::NotConnected);
        }

        if socket.is_tcp() {
            if let Some(ref tcp) = socket.tcp {
                if !tcp.can_send() {
                    return Err(SocketError::NotConnected);
                }
            }
        }

        // Simulate sending (in real implementation, queue to TX buffer)
        let bytes_sent = data.len().min(SOCKET_SEND_BUFFER_SIZE - socket.send_buffer_used);
        socket.send_buffer_used += bytes_sent;

        if socket.is_tcp() {
            if let Some(ref mut tcp) = socket.tcp {
                tcp.advance_seq(bytes_sent as u32);
            }
        }

        Ok(bytes_sent)
    }

    // recv() - Receive data from connected socket
    pub fn socket_recv(&mut self, fd: u32, _buf_size: usize) -> Result<usize, SocketError> {
        let socket = self.get_socket_mut(fd)?;

        if !socket.is_connected() && !socket.is_listening() {
            return Err(SocketError::NotConnected);
        }

        if socket.recv_buffer_used == 0 {
            return Err(SocketError::WouldBlock);
        }

        let bytes_read = socket.recv_buffer_used;
        socket.recv_buffer_used = 0;

        Ok(bytes_read)
    }

    // sendto() - Send data to specific address (UDP)
    pub fn socket_sendto(&mut self, fd: u32, data: &[u8], addr: SocketAddr) -> Result<usize, SocketError> {
        let socket = self.get_socket_mut(fd)?;

        if !socket.is_udp() {
            return Err(SocketError::InvalidOperation);
        }

        // UDP can send without being connected
        if socket.state == SocketState::Unbound {
            return Err(SocketError::NotBound);
        }

        // Set remote address temporarily
        let _target = addr;

        // Simulate sending
        let bytes_sent = data.len().min(65507);  // Max UDP payload
        socket.send_buffer_used += bytes_sent;

        Ok(bytes_sent)
    }

    // recvfrom() - Receive data with source address (UDP)
    pub fn socket_recvfrom(&mut self, fd: u32, _buf_size: usize) -> Result<(usize, SocketAddr), SocketError> {
        let socket = self.get_socket_mut(fd)?;

        if !socket.is_udp() {
            return Err(SocketError::InvalidOperation);
        }

        if socket.recv_buffer_used == 0 {
            return Err(SocketError::WouldBlock);
        }

        let bytes_read = socket.recv_buffer_used;
        let from_addr = socket.remote_addr;
        socket.recv_buffer_used = 0;

        Ok((bytes_read, from_addr))
    }

    // close() - Close socket
    pub fn socket_close(&mut self, fd: u32) -> Result<(), SocketError> {
        let socket = self.get_socket_mut(fd)?;

        if socket.is_tcp() && socket.is_connected() {
            // TCP: Initiate graceful close
            socket.state = SocketState::Closing;
            if let Some(ref mut tcp) = socket.tcp {
                tcp.state = TcpState::FinWait1;
            }
            // For simulation, immediately close
        }

        socket.state = SocketState::Closed;
        socket.in_use = false;

        // Find and remove from table
        for i in 0..MAX_SOCKETS {
            if let Some(ref s) = self.sockets[i] {
                if s.fd == fd {
                    self.sockets[i] = None;
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    // setsockopt() - Set socket option
    pub fn socket_setopt(&mut self, fd: u32, opt: SocketOption) -> Result<(), SocketError> {
        let socket = self.get_socket_mut(fd)?;

        match opt {
            SocketOption::ReuseAddr(v) => socket.options.reuse_addr = v,
            SocketOption::ReusePort(v) => socket.options.reuse_port = v,
            SocketOption::KeepAlive(v) => socket.options.keep_alive = v,
            SocketOption::NoDelay(v) => socket.options.no_delay = v,
            SocketOption::Broadcast(v) => socket.options.broadcast = v,
            SocketOption::RecvTimeout(ms) => socket.options.recv_timeout_ms = ms,
            SocketOption::SendTimeout(ms) => socket.options.send_timeout_ms = ms,
            SocketOption::Linger(t) => socket.options.linger_time = t,
        }

        Ok(())
    }

    // getsockopt() - Get socket option
    pub fn socket_getopt(&self, fd: u32, opt_type: SocketOptionType) -> Result<SocketOption, SocketError> {
        let socket = self.get_socket(fd)?;

        Ok(match opt_type {
            SocketOptionType::ReuseAddr => SocketOption::ReuseAddr(socket.options.reuse_addr),
            SocketOptionType::ReusePort => SocketOption::ReusePort(socket.options.reuse_port),
            SocketOptionType::KeepAlive => SocketOption::KeepAlive(socket.options.keep_alive),
            SocketOptionType::NoDelay => SocketOption::NoDelay(socket.options.no_delay),
            SocketOptionType::Broadcast => SocketOption::Broadcast(socket.options.broadcast),
            SocketOptionType::RecvTimeout => SocketOption::RecvTimeout(socket.options.recv_timeout_ms),
            SocketOptionType::SendTimeout => SocketOption::SendTimeout(socket.options.send_timeout_ms),
            SocketOptionType::Linger => SocketOption::Linger(socket.options.linger_time),
        })
    }

    // shutdown() - Shutdown part of full-duplex connection
    pub fn socket_shutdown(&mut self, fd: u32, how: ShutdownHow) -> Result<(), SocketError> {
        let socket = self.get_socket_mut(fd)?;

        if !socket.is_connected() {
            return Err(SocketError::NotConnected);
        }

        match how {
            ShutdownHow::Read => {
                socket.recv_buffer_used = 0;
            }
            ShutdownHow::Write => {
                socket.send_buffer_used = 0;
                if socket.is_tcp() {
                    if let Some(ref mut tcp) = socket.tcp {
                        tcp.state = TcpState::FinWait1;
                    }
                }
            }
            ShutdownHow::Both => {
                socket.recv_buffer_used = 0;
                socket.send_buffer_used = 0;
                socket.state = SocketState::Closing;
            }
        }

        Ok(())
    }

    // Helper: get socket by fd
    fn get_socket(&self, fd: u32) -> Result<&Socket, SocketError> {
        for i in 0..MAX_SOCKETS {
            if let Some(ref s) = self.sockets[i] {
                if s.fd == fd && s.in_use {
                    return Ok(s);
                }
            }
        }
        Err(SocketError::BadDescriptor)
    }

    // Helper: get mutable socket by fd
    fn get_socket_mut(&mut self, fd: u32) -> Result<&mut Socket, SocketError> {
        for i in 0..MAX_SOCKETS {
            if let Some(ref s) = self.sockets[i] {
                if s.fd == fd && s.in_use {
                    return Ok(self.sockets[i].as_mut().unwrap());
                }
            }
        }
        Err(SocketError::BadDescriptor)
    }

    // ARP operations
    pub fn arp_lookup(&self, ip: [u8; 4]) -> Option<[u8; 6]> {
        self.arp.lookup(ip)
    }

    pub fn arp_insert(&mut self, ip: [u8; 4], mac: [u8; 6]) {
        self.arp.insert(ip, mac);
    }

    // Statistics
    pub fn active_sockets(&self) -> usize {
        self.sockets.iter().filter(|s| s.is_some()).count()
    }

    pub fn listening_sockets(&self) -> usize {
        self.sockets.iter()
            .filter(|s| s.as_ref().map(|sock| sock.is_listening()).unwrap_or(false))
            .count()
    }

    pub fn connected_sockets(&self) -> usize {
        self.sockets.iter()
            .filter(|s| s.as_ref().map(|sock| sock.is_connected()).unwrap_or(false))
            .count()
    }
}

impl Default for SocketTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SOCKET OPTIONS
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub enum SocketOption {
    ReuseAddr(bool),
    ReusePort(bool),
    KeepAlive(bool),
    NoDelay(bool),
    Broadcast(bool),
    RecvTimeout(u32),
    SendTimeout(u32),
    Linger(u16),
}

#[derive(Debug, Clone, Copy)]
pub enum SocketOptionType {
    ReuseAddr,
    ReusePort,
    KeepAlive,
    NoDelay,
    Broadcast,
    RecvTimeout,
    SendTimeout,
    Linger,
}

#[derive(Debug, Clone, Copy)]
pub enum ShutdownHow {
    Read,
    Write,
    Both,
}

// ============================================================================
// SOCKET ERRORS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SocketError {
    BadDescriptor,
    AddressInUse,
    AlreadyBound,
    AlreadyConnected,
    NotBound,
    NotConnected,
    NotListening,
    InvalidOperation,
    InvalidState,
    WouldBlock,
    TooManySockets,
    ConnectionRefused,
    ConnectionReset,
    Timeout,
    NetworkUnreachable,
    HostUnreachable,
}

impl fmt::Display for SocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocketError::BadDescriptor => write!(f, "Bad file descriptor"),
            SocketError::AddressInUse => write!(f, "Address already in use"),
            SocketError::AlreadyBound => write!(f, "Socket already bound"),
            SocketError::AlreadyConnected => write!(f, "Socket already connected"),
            SocketError::NotBound => write!(f, "Socket not bound"),
            SocketError::NotConnected => write!(f, "Socket not connected"),
            SocketError::NotListening => write!(f, "Socket not listening"),
            SocketError::InvalidOperation => write!(f, "Invalid operation for socket type"),
            SocketError::InvalidState => write!(f, "Invalid socket state"),
            SocketError::WouldBlock => write!(f, "Operation would block"),
            SocketError::TooManySockets => write!(f, "Too many open sockets"),
            SocketError::ConnectionRefused => write!(f, "Connection refused"),
            SocketError::ConnectionReset => write!(f, "Connection reset by peer"),
            SocketError::Timeout => write!(f, "Operation timed out"),
            SocketError::NetworkUnreachable => write!(f, "Network unreachable"),
            SocketError::HostUnreachable => write!(f, "Host unreachable"),
        }
    }
}

// ============================================================================
// GLOBAL SOCKET TABLE
// ============================================================================

use core::sync::atomic::{AtomicBool, Ordering};

static mut SOCKET_TABLE: Option<SocketTable> = None;
static SOCKET_TABLE_LOCK: AtomicBool = AtomicBool::new(false);
static SOCKET_TABLE_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn acquire_lock() {
    while SOCKET_TABLE_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

fn release_lock() {
    SOCKET_TABLE_LOCK.store(false, Ordering::Release);
}

pub fn socket_init() {
    if SOCKET_TABLE_INITIALIZED.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        acquire_lock();
        unsafe {
            SOCKET_TABLE = Some(SocketTable::new());
        }
        release_lock();
    }
}

pub fn socket_create(socket_type: SocketType, domain: SocketDomain) -> Option<u32> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().and_then(|t| t.socket_create(socket_type, domain))
    };
    release_lock();
    result
}

pub fn socket_bind(fd: u32, addr: SocketAddr) -> Result<(), SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_bind(fd, addr)
    };
    release_lock();
    result
}

pub fn socket_listen(fd: u32, backlog: u8) -> Result<(), SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_listen(fd, backlog)
    };
    release_lock();
    result
}

pub fn socket_connect(fd: u32, addr: SocketAddr) -> Result<(), SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_connect(fd, addr)
    };
    release_lock();
    result
}

pub fn socket_accept(fd: u32) -> Result<u32, SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_accept(fd)
    };
    release_lock();
    result
}

pub fn socket_send(fd: u32, data: &[u8]) -> Result<usize, SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_send(fd, data)
    };
    release_lock();
    result
}

pub fn socket_recv(fd: u32, buf_size: usize) -> Result<usize, SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_recv(fd, buf_size)
    };
    release_lock();
    result
}

pub fn socket_sendto(fd: u32, data: &[u8], addr: SocketAddr) -> Result<usize, SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_sendto(fd, data, addr)
    };
    release_lock();
    result
}

pub fn socket_recvfrom(fd: u32, buf_size: usize) -> Result<(usize, SocketAddr), SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_recvfrom(fd, buf_size)
    };
    release_lock();
    result
}

pub fn socket_close(fd: u32) -> Result<(), SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_close(fd)
    };
    release_lock();
    result
}

pub fn socket_setopt(fd: u32, opt: SocketOption) -> Result<(), SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_setopt(fd, opt)
    };
    release_lock();
    result
}

pub fn socket_shutdown(fd: u32, how: ShutdownHow) -> Result<(), SocketError> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_mut().ok_or(SocketError::BadDescriptor)?.socket_shutdown(fd, how)
    };
    release_lock();
    result
}

pub fn arp_lookup(ip: [u8; 4]) -> Option<[u8; 6]> {
    acquire_lock();
    let result = unsafe {
        SOCKET_TABLE.as_ref().and_then(|t| t.arp_lookup(ip))
    };
    release_lock();
    result
}

pub fn arp_insert(ip: [u8; 4], mac: [u8; 6]) {
    acquire_lock();
    unsafe {
        if let Some(ref mut t) = SOCKET_TABLE {
            t.arp_insert(ip, mac);
        }
    }
    release_lock();
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_addr_new() {
        let addr = SocketAddr::new(192, 168, 1, 1, 8080);
        assert_eq!(addr.port, 8080);
        assert_eq!(addr.ip, [192, 168, 1, 1]);
    }

    #[test]
    fn test_socket_addr_loopback() {
        let addr = SocketAddr::loopback(80);
        assert_eq!(addr.ip, [127, 0, 0, 1]);
    }

    #[test]
    fn test_tcp_flags() {
        let syn = TcpFlags::syn();
        assert!(syn.syn);
        assert!(!syn.ack);
        assert_eq!(syn.to_byte(), 0x02);
    }

    #[test]
    fn test_tcp_flags_from_byte() {
        let flags = TcpFlags::from_byte(0x12);  // SYN+ACK
        assert!(flags.syn);
        assert!(flags.ack);
    }

    #[test]
    fn test_tcp_control_block_new() {
        let tcb = TcpControlBlock::new();
        assert_eq!(tcb.state, TcpState::Closed);
        assert_eq!(tcb.local_seq, TCP_INITIAL_SEQ);
    }

    #[test]
    fn test_socket_table_create() {
        let mut table = SocketTable::new();
        let fd = table.socket_create(SocketType::Stream, SocketDomain::Inet);
        assert!(fd.is_some());
        assert!(fd.unwrap() >= 3);
    }

    #[test]
    fn test_socket_bind() {
        let mut table = SocketTable::new();
        let fd = table.socket_create(SocketType::Stream, SocketDomain::Inet).unwrap();
        let addr = SocketAddr::any(8080);
        let result = table.socket_bind(fd, addr);
        assert!(result.is_ok());
    }

    #[test]
    fn test_socket_listen() {
        let mut table = SocketTable::new();
        let fd = table.socket_create(SocketType::Stream, SocketDomain::Inet).unwrap();
        let addr = SocketAddr::any(8080);
        table.socket_bind(fd, addr).unwrap();
        let result = table.socket_listen(fd, 5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_socket_connect() {
        let mut table = SocketTable::new();
        let fd = table.socket_create(SocketType::Stream, SocketDomain::Inet).unwrap();
        let addr = SocketAddr::new(127, 0, 0, 1, 80);
        let result = table.socket_connect(fd, addr);
        assert!(result.is_ok());
    }

    #[test]
    fn test_udp_socket() {
        let mut table = SocketTable::new();
        let fd = table.socket_create(SocketType::Datagram, SocketDomain::Inet).unwrap();
        let local = SocketAddr::any(5000);
        table.socket_bind(fd, local).unwrap();
        let remote = SocketAddr::new(8, 8, 8, 8, 53);
        let result = table.socket_sendto(fd, b"query", remote);
        assert!(result.is_ok());
    }

    #[test]
    fn test_arp_table() {
        let mut arp = ArpTable::new();
        let ip = [192, 168, 1, 1];
        let mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        assert!(arp.insert(ip, mac));
        assert_eq!(arp.lookup(ip), Some(mac));
    }

    #[test]
    fn test_socket_close() {
        let mut table = SocketTable::new();
        let fd = table.socket_create(SocketType::Stream, SocketDomain::Inet).unwrap();
        let result = table.socket_close(fd);
        assert!(result.is_ok());
    }

    #[test]
    fn test_socket_options() {
        let mut table = SocketTable::new();
        let fd = table.socket_create(SocketType::Stream, SocketDomain::Inet).unwrap();
        table.socket_setopt(fd, SocketOption::ReuseAddr(true)).unwrap();
        let opt = table.socket_getopt(fd, SocketOptionType::ReuseAddr).unwrap();
        match opt {
            SocketOption::ReuseAddr(v) => assert!(v),
            _ => panic!("Wrong option type"),
        }
    }

    #[test]
    fn test_tcp_segment_new() {
        let seg = TcpSegment::new(8080, 80, 1000, 0, TcpFlags::syn());
        assert_eq!(seg.src_port, 8080);
        assert_eq!(seg.dst_port, 80);
        assert!(seg.flags.syn);
    }

    #[test]
    fn test_udp_datagram_new() {
        let dgram = UdpDatagram::new(5000, 53, 100);
        assert_eq!(dgram.length, 108);  // 8 + 100
    }
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_tcp_server_flow() {
        let mut table = SocketTable::new();

        // Create server socket
        let server_fd = table.socket_create(SocketType::Stream, SocketDomain::Inet).unwrap();
        let addr = SocketAddr::any(8080);
        table.socket_bind(server_fd, addr).unwrap();
        table.socket_listen(server_fd, 5).unwrap();

        assert_eq!(table.listening_sockets(), 1);
    }

    #[test]
    fn test_tcp_client_flow() {
        let mut table = SocketTable::new();

        // Create client socket
        let client_fd = table.socket_create(SocketType::Stream, SocketDomain::Inet).unwrap();
        let server_addr = SocketAddr::new(192, 168, 1, 100, 80);
        table.socket_connect(client_fd, server_addr).unwrap();

        // Send data
        let sent = table.socket_send(client_fd, b"GET / HTTP/1.1\r\n").unwrap();
        assert!(sent > 0);

        assert_eq!(table.connected_sockets(), 1);
    }

    #[test]
    fn test_udp_echo_flow() {
        let mut table = SocketTable::new();

        // Create UDP socket
        let fd = table.socket_create(SocketType::Datagram, SocketDomain::Inet).unwrap();
        let local = SocketAddr::any(5000);
        table.socket_bind(fd, local).unwrap();

        // Send to DNS server
        let dns = SocketAddr::new(8, 8, 8, 8, 53);
        let sent = table.socket_sendto(fd, b"\x00\x01", dns).unwrap();
        assert_eq!(sent, 2);
    }

    #[test]
    fn test_arp_resolution_flow() {
        let mut table = SocketTable::new();

        // Add ARP entries
        table.arp_insert([192, 168, 1, 1], [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
        table.arp_insert([192, 168, 1, 2], [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x02]);

        // Lookup
        let mac1 = table.arp_lookup([192, 168, 1, 1]);
        assert!(mac1.is_some());
        assert_eq!(mac1.unwrap()[5], 0x01);

        let mac_unknown = table.arp_lookup([10, 0, 0, 1]);
        assert!(mac_unknown.is_none());
    }

    #[test]
    fn test_multiple_sockets() {
        let mut table = SocketTable::new();

        // Create multiple sockets
        for port in 8000..8010 {
            let fd = table.socket_create(SocketType::Stream, SocketDomain::Inet).unwrap();
            let addr = SocketAddr::any(port);
            table.socket_bind(fd, addr).unwrap();
        }

        assert_eq!(table.active_sockets(), 10);
    }
}
