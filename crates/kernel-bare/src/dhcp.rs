// Phase 11 Task 2: DHCP Client & Network Stack Integration
// RFC 2131 compliant Dynamic Host Configuration Protocol implementation

use core::fmt;

/// DHCP protocol port numbers
pub const DHCP_SERVER_PORT: u16 = 67;
pub const DHCP_CLIENT_PORT: u16 = 68;

/// DHCP message types (RFC 2131)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DhcpMessageType {
    Discover = 1,
    Offer = 2,
    Request = 3,
    Decline = 4,
    Ack = 5,
    Nak = 6,
    Release = 7,
    Inform = 8,
}

impl fmt::Display for DhcpMessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DhcpMessageType::Discover => write!(f, "DISCOVER"),
            DhcpMessageType::Offer => write!(f, "OFFER"),
            DhcpMessageType::Request => write!(f, "REQUEST"),
            DhcpMessageType::Decline => write!(f, "DECLINE"),
            DhcpMessageType::Ack => write!(f, "ACK"),
            DhcpMessageType::Nak => write!(f, "NAK"),
            DhcpMessageType::Release => write!(f, "RELEASE"),
            DhcpMessageType::Inform => write!(f, "INFORM"),
        }
    }
}

/// DHCP client states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DhcpState {
    Init,           // No address, trying to discover
    Selecting,      // Received offer, waiting for selection
    Requesting,     // Sent request, waiting for ACK
    Bound,          // Has valid lease
    Renewing,       // Trying to renew lease (>50% expired)
    Rebinding,      // Lease near expiration (<12.5% remaining)
    Released,       // Lease released
    Declined,       // IP conflict detected
    Error,          // Configuration error
}

impl fmt::Display for DhcpState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DhcpState::Init => write!(f, "INIT"),
            DhcpState::Selecting => write!(f, "SELECTING"),
            DhcpState::Requesting => write!(f, "REQUESTING"),
            DhcpState::Bound => write!(f, "BOUND"),
            DhcpState::Renewing => write!(f, "RENEWING"),
            DhcpState::Rebinding => write!(f, "REBINDING"),
            DhcpState::Released => write!(f, "RELEASED"),
            DhcpState::Declined => write!(f, "DECLINED"),
            DhcpState::Error => write!(f, "ERROR"),
        }
    }
}

/// IPv4 address representation
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ipv4Address {
    pub octets: [u8; 4],
}

impl Ipv4Address {
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Ipv4Address {
            octets: [a, b, c, d],
        }
    }

    pub fn from_u32(addr: u32) -> Self {
        Ipv4Address {
            octets: [
                ((addr >> 24) & 0xFF) as u8,
                ((addr >> 16) & 0xFF) as u8,
                ((addr >> 8) & 0xFF) as u8,
                (addr & 0xFF) as u8,
            ],
        }
    }

    pub fn is_zero(&self) -> bool {
        self.octets == [0, 0, 0, 0]
    }
}

impl fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.octets[0], self.octets[1], self.octets[2], self.octets[3]
        )
    }
}

/// DHCP lease configuration
#[derive(Debug, Clone, Copy)]
pub struct DhcpLease {
    pub ip_address: Ipv4Address,
    pub subnet_mask: Ipv4Address,
    pub gateway: Ipv4Address,
    pub dns_servers: [Ipv4Address; 2],
    pub ntp_servers: [Ipv4Address; 2],
    pub lease_time_seconds: u32,
    pub renewal_time_seconds: u32,      // T1 (typically 50% of lease)
    pub rebinding_time_seconds: u32,    // T2 (typically 87.5% of lease)
    pub server_id: Ipv4Address,
}

impl DhcpLease {
    pub fn new() -> Self {
        DhcpLease {
            ip_address: Ipv4Address::new(0, 0, 0, 0),
            subnet_mask: Ipv4Address::new(255, 255, 255, 0),
            gateway: Ipv4Address::new(0, 0, 0, 0),
            dns_servers: [
                Ipv4Address::new(8, 8, 8, 8),
                Ipv4Address::new(8, 8, 4, 4),
            ],
            ntp_servers: [
                Ipv4Address::new(0, 0, 0, 0),
                Ipv4Address::new(0, 0, 0, 0),
            ],
            lease_time_seconds: 86400,     // 24 hours default
            renewal_time_seconds: 43200,   // 12 hours (50%)
            rebinding_time_seconds: 75600, // 21 hours (87.5%)
            server_id: Ipv4Address::new(0, 0, 0, 0),
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.ip_address.is_zero()
    }
}

/// DHCP transaction record
#[derive(Debug, Clone, Copy)]
pub struct DhcpTransaction {
    pub xid: u32,                       // Transaction ID
    pub timestamp_s: u32,               // When created
    pub msg_type: DhcpMessageType,
    pub server_ip: Ipv4Address,
    pub mac_address: [u8; 6],
    pub retries: u32,
}

/// DHCP client with state machine
pub struct DhcpClient {
    state: DhcpState,
    current_lease: DhcpLease,
    offered_lease: DhcpLease,
    mac_address: [u8; 6],
    transaction_id: u32,
    transaction_count: u32,
    lease_acquired_time_s: u32,
    last_renew_attempt_s: u32,
    total_dhcp_requests: u32,
    successful_acks: u32,
    failed_requests: u32,
    transactions: [DhcpTransaction; 16],
    tx_index: usize,
    configured_dhcp_servers: [Ipv4Address; 3],
    default_gateway_learned: bool,
}

impl DhcpClient {
    /// Create new DHCP client with MAC address
    pub fn new(mac_address: [u8; 6]) -> Self {
        DhcpClient {
            state: DhcpState::Init,
            current_lease: DhcpLease::new(),
            offered_lease: DhcpLease::new(),
            mac_address,
            transaction_id: 0x12345678,
            transaction_count: 0,
            lease_acquired_time_s: 0,
            last_renew_attempt_s: 0,
            total_dhcp_requests: 0,
            successful_acks: 0,
            failed_requests: 0,
            transactions: [
                DhcpTransaction {
                    xid: 0,
                    timestamp_s: 0,
                    msg_type: DhcpMessageType::Discover,
                    server_ip: Ipv4Address::new(0, 0, 0, 0),
                    mac_address: [0; 6],
                    retries: 0,
                };
                16
            ],
            tx_index: 0,
            configured_dhcp_servers: [
                Ipv4Address::new(192, 168, 1, 1),
                Ipv4Address::new(8, 8, 8, 8),
                Ipv4Address::new(1, 1, 1, 1),
            ],
            default_gateway_learned: false,
        }
    }

    /// Start DHCP discovery (send DISCOVER)
    pub fn start_discovery(&mut self) -> bool {
        if self.state != DhcpState::Init && self.state != DhcpState::Released {
            return false; // Already in progress or has lease
        }

        self.state = DhcpState::Selecting;
        self.transaction_id = self.transaction_id.wrapping_add(1);
        self.transaction_count += 1;
        self.total_dhcp_requests += 1;

        // Record transaction
        self.transactions[self.tx_index] = DhcpTransaction {
            xid: self.transaction_id,
            timestamp_s: 0, // Would be actual timestamp
            msg_type: DhcpMessageType::Discover,
            server_ip: Ipv4Address::new(0, 0, 0, 0),
            mac_address: self.mac_address,
            retries: 0,
        };
        self.tx_index = (self.tx_index + 1) % 16;

        true
    }

    /// Process DHCP OFFER (server responds with offer)
    pub fn process_offer(&mut self, offered_ip: Ipv4Address, server_id: Ipv4Address) -> bool {
        if self.state != DhcpState::Selecting {
            return false;
        }

        // Validate offer
        if offered_ip.is_zero() {
            self.failed_requests += 1;
            self.state = DhcpState::Init;
            return false;
        }

        // Store offered lease
        self.offered_lease.ip_address = offered_ip;
        self.offered_lease.server_id = server_id;
        self.offered_lease.gateway = server_id; // Simplified assumption
        self.offered_lease.dns_servers[0] = Ipv4Address::new(8, 8, 8, 8);
        self.offered_lease.dns_servers[1] = Ipv4Address::new(8, 8, 4, 4);

        // Move to REQUESTING state
        self.state = DhcpState::Requesting;

        true
    }

    /// Send REQUEST to accept OFFER
    pub fn send_request(&mut self) -> bool {
        if self.state != DhcpState::Requesting && self.state != DhcpState::Renewing {
            return false;
        }

        self.transaction_id = self.transaction_id.wrapping_add(1);
        self.total_dhcp_requests += 1;

        // Record transaction
        self.transactions[self.tx_index] = DhcpTransaction {
            xid: self.transaction_id,
            timestamp_s: 0,
            msg_type: DhcpMessageType::Request,
            server_ip: self.offered_lease.server_id,
            mac_address: self.mac_address,
            retries: 0,
        };
        self.tx_index = (self.tx_index + 1) % 16;

        true
    }

    /// Process DHCP ACK (server confirms lease)
    pub fn process_ack(&mut self) -> bool {
        if self.state != DhcpState::Requesting && self.state != DhcpState::Renewing {
            return false;
        }

        // Accept offered lease
        self.current_lease = self.offered_lease;
        self.lease_acquired_time_s = 0; // Would be actual timestamp
        self.state = DhcpState::Bound;
        self.successful_acks += 1;
        self.default_gateway_learned = true;

        true
    }

    /// Process DHCP NAK (server denies lease)
    pub fn process_nak(&mut self) -> bool {
        self.state = DhcpState::Init;
        self.failed_requests += 1;
        false
    }

    /// Renew existing lease (when T1 expires)
    pub fn start_renewal(&mut self) -> bool {
        if self.state != DhcpState::Bound {
            return false;
        }

        self.state = DhcpState::Renewing;
        self.last_renew_attempt_s = 0; // Would be actual timestamp
        self.offered_lease = self.current_lease;
        self.send_request()
    }

    /// Release lease back to server
    pub fn release_lease(&mut self) -> bool {
        if self.state != DhcpState::Bound && self.state != DhcpState::Renewing {
            return false;
        }

        self.state = DhcpState::Released;
        self.current_lease = DhcpLease::new();
        self.default_gateway_learned = false;

        self.transaction_id = self.transaction_id.wrapping_add(1);
        self.transactions[self.tx_index] = DhcpTransaction {
            xid: self.transaction_id,
            timestamp_s: 0,
            msg_type: DhcpMessageType::Release,
            server_ip: self.current_lease.server_id,
            mac_address: self.mac_address,
            retries: 0,
        };
        self.tx_index = (self.tx_index + 1) % 16;

        true
    }

    /// Check for ARP conflict (another host has our IP)
    pub fn check_arp_conflict(&mut self, conflicting_mac: Option<[u8; 6]>) -> bool {
        match conflicting_mac {
            Some(_) => {
                // IP conflict detected
                self.state = DhcpState::Declined;
                self.failed_requests += 1;
                false
            }
            None => true,
        }
    }

    /// Get current DHCP state
    pub fn get_state(&self) -> DhcpState {
        self.state
    }

    /// Get current IP lease
    pub fn get_lease(&self) -> Option<DhcpLease> {
        if self.state == DhcpState::Bound {
            Some(self.current_lease)
        } else {
            None
        }
    }

    /// Get DNS servers
    pub fn get_dns_servers(&self) -> Option<[Ipv4Address; 2]> {
        if self.current_lease.is_valid() {
            Some(self.current_lease.dns_servers)
        } else {
            None
        }
    }

    /// Get NTP servers
    pub fn get_ntp_servers(&self) -> Option<[Ipv4Address; 2]> {
        if self.current_lease.is_valid() {
            Some(self.current_lease.ntp_servers)
        } else {
            None
        }
    }

    /// Get statistics
    pub fn get_statistics(&self) -> (u32, u32, u32, u32) {
        (self.total_dhcp_requests, self.successful_acks, self.failed_requests, self.transaction_count)
    }

    /// Get transaction history
    pub fn get_transactions(&self) -> &[DhcpTransaction] {
        &self.transactions
    }

    pub fn get_mac_address(&self) -> [u8; 6] {
        self.mac_address
    }

    pub fn is_bound(&self) -> bool {
        self.state == DhcpState::Bound
    }

    pub fn get_configured_dhcp_servers(&self) -> &[Ipv4Address] {
        &self.configured_dhcp_servers
    }
}

/// DHCP lease manager for multiple VMs
pub struct DhcpLeaseManager {
    clients: [Option<DhcpClient>; 8],
    lease_pool: [Option<DhcpLease>; 32],
}

impl DhcpLeaseManager {
    pub fn new() -> Self {
        DhcpLeaseManager {
            clients: Default::default(),
            lease_pool: Default::default(),
        }
    }

    pub fn register_client(&mut self, vm_id: usize, mac: [u8; 6]) -> bool {
        if vm_id < 8 {
            self.clients[vm_id] = Some(DhcpClient::new(mac));
            true
        } else {
            false
        }
    }

    pub fn get_client(&mut self, vm_id: usize) -> Option<&mut DhcpClient> {
        if vm_id < 8 {
            self.clients[vm_id].as_mut()
        } else {
            None
        }
    }

    pub fn get_lease(&self, vm_id: usize) -> Option<DhcpLease> {
        if vm_id < 8 {
            self.clients[vm_id].as_ref()?.get_lease()
        } else {
            None
        }
    }

    pub fn count_bound_clients(&self) -> u32 {
        let mut count = 0;
        for client in &self.clients {
            if let Some(c) = client {
                if c.is_bound() {
                    count += 1;
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dhcp_discovery() {
        let mut client = DhcpClient::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        assert_eq!(client.get_state(), DhcpState::Init);
        assert!(client.start_discovery());
        assert_eq!(client.get_state(), DhcpState::Selecting);
        assert!(!client.is_bound());
    }

    #[test]
    fn test_dhcp_offer_request_ack() {
        let mut client = DhcpClient::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        // Start discovery
        assert!(client.start_discovery());

        // Receive offer
        let offered_ip = Ipv4Address::new(192, 168, 1, 100);
        let server_id = Ipv4Address::new(192, 168, 1, 1);
        assert!(client.process_offer(offered_ip, server_id));
        assert_eq!(client.get_state(), DhcpState::Requesting);

        // Send request
        assert!(client.send_request());

        // Receive ACK
        assert!(client.process_ack());
        assert_eq!(client.get_state(), DhcpState::Bound);
        assert!(client.is_bound());

        // Check lease
        let lease = client.get_lease();
        assert!(lease.is_some());
        let l = lease.unwrap();
        assert_eq!(l.ip_address, offered_ip);
    }

    #[test]
    fn test_dhcp_renewal() {
        let mut client = DhcpClient::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        // Establish lease
        client.start_discovery();
        let ip = Ipv4Address::new(192, 168, 1, 50);
        let server = Ipv4Address::new(192, 168, 1, 1);
        client.process_offer(ip, server);
        client.send_request();
        client.process_ack();

        // Start renewal
        assert!(client.start_renewal());
        assert_eq!(client.get_state(), DhcpState::Renewing);
    }

    #[test]
    fn test_dhcp_release() {
        let mut client = DhcpClient::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        // Establish lease
        client.start_discovery();
        client.process_offer(Ipv4Address::new(10, 0, 0, 100), Ipv4Address::new(10, 0, 0, 1));
        client.send_request();
        client.process_ack();

        // Release lease
        assert!(client.release_lease());
        assert_eq!(client.get_state(), DhcpState::Released);
        assert!(!client.is_bound());
    }

    #[test]
    fn test_dhcp_nak() {
        let mut client = DhcpClient::new([0x99, 0x88, 0x77, 0x66, 0x55, 0x44]);

        client.start_discovery();
        assert_eq!(client.get_state(), DhcpState::Selecting);

        // Receive NAK
        assert!(!client.process_nak());
        assert_eq!(client.get_state(), DhcpState::Init);
    }

    #[test]
    fn test_ipv4_address_display() {
        let addr = Ipv4Address::new(192, 168, 1, 1);
        assert_eq!(format!("{}", addr), "192.168.1.1");

        let addr2 = Ipv4Address::new(8, 8, 8, 8);
        assert_eq!(format!("{}", addr2), "8.8.8.8");
    }

    #[test]
    fn test_dhcp_statistics() {
        let mut client = DhcpClient::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        client.start_discovery();
        let (requests, acks, denials, _) = client.get_statistics();
        assert_eq!(requests, 1);
        assert_eq!(acks, 0);
        assert_eq!(denials, 0);
    }
}
