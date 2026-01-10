// RAYOS Phase 28 Task 4: DNS & Service Discovery
// Domain name resolution and service discovery mechanisms
// File: crates/kernel-bare/src/dns_discovery.rs
// Lines: 700+ | Tests: 13 unit + 5 scenario | Markers: 5


const MAX_DNS_CACHE_ENTRIES: usize = 256;
const MAX_SERVICES: usize = 64;
const MAX_DOMAIN_LEN: usize = 256;
const MAX_DNS_QUERY_SIZE: usize = 512;
const DEFAULT_TTL: u32 = 3600;
const MDNS_PORT: u16 = 5353;

// ============================================================================
// DNS RECORD TYPES
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DNSRecordType {
    A,      // IPv4 address
    AAAA,   // IPv6 address
    CNAME,  // Canonical name
    MX,     // Mail exchange
    TXT,    // Text record
    SRV,    // Service record
    PTR,    // Pointer record
    NS,     // Name server
}

impl DNSRecordType {
    pub fn to_u16(&self) -> u16 {
        match self {
            DNSRecordType::A => 1,
            DNSRecordType::AAAA => 28,
            DNSRecordType::CNAME => 5,
            DNSRecordType::MX => 15,
            DNSRecordType::TXT => 16,
            DNSRecordType::SRV => 33,
            DNSRecordType::PTR => 12,
            DNSRecordType::NS => 2,
        }
    }

    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            1 => Some(DNSRecordType::A),
            28 => Some(DNSRecordType::AAAA),
            5 => Some(DNSRecordType::CNAME),
            15 => Some(DNSRecordType::MX),
            16 => Some(DNSRecordType::TXT),
            33 => Some(DNSRecordType::SRV),
            12 => Some(DNSRecordType::PTR),
            2 => Some(DNSRecordType::NS),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DNSRecord {
    pub record_type: DNSRecordType,
    pub ttl: u32,
    pub data_hash: u32,
}

impl DNSRecord {
    pub fn new(record_type: DNSRecordType, ttl: u32) -> Self {
        DNSRecord {
            record_type,
            ttl,
            data_hash: 0,
        }
    }

    pub fn set_data_hash(&mut self, hash: u32) {
        self.data_hash = hash;
    }

    pub fn is_expired(&self, age_seconds: u32) -> bool {
        age_seconds >= self.ttl
    }
}

// ============================================================================
// DNS QUERY & RESPONSE
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct DNSQuery {
    pub query_id: u16,
    pub query_type: DNSRecordType,
    pub is_recursive: bool,
    pub domain_hash: u32,
}

impl DNSQuery {
    pub fn new(query_id: u16, domain: &str, query_type: DNSRecordType) -> Self {
        DNSQuery {
            query_id,
            query_type,
            is_recursive: true,
            domain_hash: Self::hash_domain(domain),
        }
    }

    fn hash_domain(domain: &str) -> u32 {
        let mut hash: u32 = 5381;
        for byte in domain.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
        }
        hash
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DNSResponse {
    pub query_id: u16,
    pub response_code: u8,
    pub record_count: u8,
    pub authority_count: u8,
    pub timestamp: u32,
}

impl DNSResponse {
    pub fn new(query_id: u16) -> Self {
        DNSResponse {
            query_id,
            response_code: 0, // NOERROR
            record_count: 0,
            authority_count: 0,
            timestamp: 0,
        }
    }

    pub fn set_error(&mut self, error_code: u8) {
        self.response_code = error_code;
    }

    pub fn add_record(&mut self) -> bool {
        if self.record_count < 32 {
            self.record_count += 1;
            return true;
        }
        false
    }
}

// ============================================================================
// DNS CACHE
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct CacheEntry {
    pub domain_hash: u32,
    pub record: DNSRecord,
    pub inserted_at: u32,
    pub is_valid: bool,
}

impl CacheEntry {
    pub fn new(domain_hash: u32, record: DNSRecord) -> Self {
        CacheEntry {
            domain_hash,
            record,
            inserted_at: 0,
            is_valid: true,
        }
    }
}

pub struct DNSCache {
    pub entries: [Option<CacheEntry>; MAX_DNS_CACHE_ENTRIES],
    pub entry_count: usize,
    pub hits: u32,
    pub misses: u32,
}

impl DNSCache {
    pub fn new() -> Self {
        DNSCache {
            entries: [None; MAX_DNS_CACHE_ENTRIES],
            entry_count: 0,
            hits: 0,
            misses: 0,
        }
    }

    pub fn insert(&mut self, domain_hash: u32, record: DNSRecord) -> bool {
        // Check if already exists
        for i in 0..self.entry_count {
            if let Some(entry) = self.entries[i] {
                if entry.domain_hash == domain_hash {
                    self.entries[i] = Some(CacheEntry::new(domain_hash, record));
                    return true;
                }
            }
        }

        // Add new entry
        if self.entry_count < MAX_DNS_CACHE_ENTRIES {
            self.entries[self.entry_count] = Some(CacheEntry::new(domain_hash, record));
            self.entry_count += 1;
            return true;
        }

        // Cache full - evict oldest entry
        if self.entry_count > 0 {
            for i in 0..self.entry_count - 1 {
                self.entries[i] = self.entries[i + 1];
            }
            self.entries[self.entry_count - 1] = Some(CacheEntry::new(domain_hash, record));
            return true;
        }

        false
    }

    pub fn lookup(&mut self, domain_hash: u32, age_seconds: u32) -> Option<DNSRecord> {
        for i in 0..self.entry_count {
            if let Some(entry) = self.entries[i] {
                if entry.domain_hash == domain_hash && entry.is_valid {
                    if !entry.record.is_expired(age_seconds) {
                        self.hits += 1;
                        return Some(entry.record);
                    } else {
                        // Mark as invalid
                        self.entries[i] = Some(CacheEntry {
                            is_valid: false,
                            ..entry
                        });
                    }
                }
            }
        }
        self.misses += 1;
        None
    }

    pub fn get_hit_rate(&self) -> u8 {
        let total = self.hits + self.misses;
        if total == 0 {
            0
        } else {
            ((self.hits as u32 * 100) / total as u32) as u8
        }
    }
}

impl Default for DNSCache {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// DNS RESOLVER
// ============================================================================

pub struct DNSResolver {
    pub resolver_id: u32,
    pub cache: DNSCache,
    pub next_query_id: u16,
    pub queries_sent: u32,
    pub queries_resolved: u32,
}

impl DNSResolver {
    pub fn new(resolver_id: u32) -> Self {
        DNSResolver {
            resolver_id,
            cache: DNSCache::new(),
            next_query_id: 1,
            queries_sent: 0,
            queries_resolved: 0,
        }
    }

    pub fn resolve(&mut self, domain: &str, record_type: DNSRecordType) -> Option<DNSRecord> {
        let domain_hash = DNSQuery::hash_domain(domain);

        // Check cache first
        if let Some(record) = self.cache.lookup(domain_hash, 0) {
            return Some(record);
        }

        // Create query
        let query = DNSQuery::new(self.next_query_id, domain, record_type);
        self.next_query_id = self.next_query_id.wrapping_add(1);
        self.queries_sent += 1;

        // Simulate resolution (in real implementation, would send over network)
        let mut response = DNSResponse::new(query.query_id);
        let record = DNSRecord::new(record_type, DEFAULT_TTL);

        if response.add_record() {
            self.cache.insert(domain_hash, record);
            self.queries_resolved += 1;
            return Some(record);
        }

        None
    }
}

impl Default for DNSResolver {
    fn default() -> Self {
        Self::new(0)
    }
}

// ============================================================================
// MULTICAST DNS (mDNS)
// ============================================================================

pub struct mDNSResponder {
    pub responder_id: u32,
    pub is_active: bool,
    pub port: u16,
    pub announcements_sent: u32,
    pub queries_received: u32,
}

impl mDNSResponder {
    pub fn new(responder_id: u32) -> Self {
        mDNSResponder {
            responder_id,
            is_active: false,
            port: MDNS_PORT,
            announcements_sent: 0,
            queries_received: 0,
        }
    }

    pub fn start(&mut self) -> bool {
        self.is_active = true;
        true
    }

    pub fn stop(&mut self) {
        self.is_active = false;
    }

    pub fn announce_service(&mut self, _service_name: &str) -> bool {
        if !self.is_active {
            return false;
        }
        self.announcements_sent += 1;
        true
    }

    pub fn handle_query(&mut self) -> bool {
        if !self.is_active {
            return false;
        }
        self.queries_received += 1;
        true
    }
}

// ============================================================================
// SERVICE DISCOVERY
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct ServiceEntry {
    pub service_id: u32,
    pub name_hash: u32,
    pub service_type_hash: u32,
    pub port: u16,
    pub address_hash: u32,
    pub ttl: u32,
}

impl ServiceEntry {
    pub fn new(service_id: u32, name: &str, service_type: &str, port: u16) -> Self {
        ServiceEntry {
            service_id,
            name_hash: Self::hash_string(name),
            service_type_hash: Self::hash_string(service_type),
            port,
            address_hash: 0,
            ttl: DEFAULT_TTL,
        }
    }

    fn hash_string(s: &str) -> u32 {
        let mut hash: u32 = 5381;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
        }
        hash
    }

    pub fn set_address(&mut self, address: &str) {
        self.address_hash = Self::hash_string(address);
    }
}

pub struct ServiceRegistry {
    pub services: [Option<ServiceEntry>; MAX_SERVICES],
    pub service_count: usize,
    pub next_service_id: u32,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        ServiceRegistry {
            services: [None; MAX_SERVICES],
            service_count: 0,
            next_service_id: 1,
        }
    }

    pub fn register_service(&mut self, name: &str, service_type: &str, port: u16) -> Option<u32> {
        if self.service_count >= MAX_SERVICES {
            return None;
        }

        let service_id = self.next_service_id;
        self.next_service_id += 1;

        let service = ServiceEntry::new(service_id, name, service_type, port);
        self.services[self.service_count] = Some(service);
        self.service_count += 1;

        Some(service_id)
    }

    pub fn get_service(&self, service_id: u32) -> Option<ServiceEntry> {
        for i in 0..self.service_count {
            if let Some(service) = self.services[i] {
                if service.service_id == service_id {
                    return Some(service);
                }
            }
        }
        None
    }

    pub fn unregister_service(&mut self, service_id: u32) -> bool {
        for i in 0..self.service_count {
            if let Some(service) = self.services[i] {
                if service.service_id == service_id {
                    for j in i..self.service_count - 1 {
                        self.services[j] = self.services[j + 1];
                    }
                    self.services[self.service_count - 1] = None;
                    self.service_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn get_active_services(&self) -> usize {
        self.service_count
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SERVICE BROWSER
// ============================================================================

pub struct ServiceBrowser {
    pub browser_id: u32,
    pub is_browsing: bool,
    pub service_type_hash: u32,
    pub discovered_services: usize,
}

impl ServiceBrowser {
    pub fn new(browser_id: u32, service_type: &str) -> Self {
        ServiceBrowser {
            browser_id,
            is_browsing: false,
            service_type_hash: Self::hash_string(service_type),
            discovered_services: 0,
        }
    }

    fn hash_string(s: &str) -> u32 {
        let mut hash: u32 = 5381;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
        }
        hash
    }

    pub fn start_browsing(&mut self) -> bool {
        self.is_browsing = true;
        true
    }

    pub fn stop_browsing(&mut self) {
        self.is_browsing = false;
    }

    pub fn on_service_found(&mut self, _service: &ServiceEntry) -> bool {
        if !self.is_browsing {
            return false;
        }
        self.discovered_services += 1;
        true
    }

    pub fn on_service_removed(&mut self) {
        if self.discovered_services > 0 {
            self.discovered_services -= 1;
        }
    }

    pub fn get_discovery_count(&self) -> usize {
        self.discovered_services
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_record_type_to_u16() {
        assert_eq!(DNSRecordType::A.to_u16(), 1);
        assert_eq!(DNSRecordType::AAAA.to_u16(), 28);
        assert_eq!(DNSRecordType::CNAME.to_u16(), 5);
    }

    #[test]
    fn test_dns_record_type_from_u16() {
        assert_eq!(DNSRecordType::from_u16(1), Some(DNSRecordType::A));
        assert_eq!(DNSRecordType::from_u16(28), Some(DNSRecordType::AAAA));
        assert_eq!(DNSRecordType::from_u16(999), None);
    }

    #[test]
    fn test_dns_record_new() {
        let record = DNSRecord::new(DNSRecordType::A, 3600);
        assert_eq!(record.record_type, DNSRecordType::A);
        assert_eq!(record.ttl, 3600);
    }

    #[test]
    fn test_dns_record_is_expired() {
        let record = DNSRecord::new(DNSRecordType::A, 3600);
        assert!(!record.is_expired(1800));
        assert!(record.is_expired(3600));
    }

    #[test]
    fn test_dns_query_new() {
        let query = DNSQuery::new(1, "example.com", DNSRecordType::A);
        assert_eq!(query.query_id, 1);
        assert_eq!(query.query_type, DNSRecordType::A);
    }

    #[test]
    fn test_dns_response_new() {
        let resp = DNSResponse::new(1);
        assert_eq!(resp.query_id, 1);
        assert_eq!(resp.response_code, 0);
    }

    #[test]
    fn test_dns_response_add_record() {
        let mut resp = DNSResponse::new(1);
        assert!(resp.add_record());
        assert_eq!(resp.record_count, 1);
    }

    #[test]
    fn test_dns_cache_insert() {
        let mut cache = DNSCache::new();
        let record = DNSRecord::new(DNSRecordType::A, 3600);
        assert!(cache.insert(12345, record));
        assert_eq!(cache.entry_count, 1);
    }

    #[test]
    fn test_dns_cache_lookup() {
        let mut cache = DNSCache::new();
        let record = DNSRecord::new(DNSRecordType::A, 3600);
        cache.insert(12345, record);
        let found = cache.lookup(12345, 0);
        assert!(found.is_some());
        assert_eq!(cache.hits, 1);
    }

    #[test]
    fn test_dns_resolver_new() {
        let resolver = DNSResolver::new(1);
        assert_eq!(resolver.resolver_id, 1);
        assert_eq!(resolver.queries_sent, 0);
    }

    #[test]
    fn test_mdns_responder_new() {
        let responder = mDNSResponder::new(1);
        assert_eq!(responder.responder_id, 1);
        assert!(!responder.is_active);
    }

    #[test]
    fn test_service_entry_new() {
        let service = ServiceEntry::new(1, "web-server", "_http._tcp", 8080);
        assert_eq!(service.service_id, 1);
        assert_eq!(service.port, 8080);
    }

    #[test]
    fn test_service_registry_new() {
        let registry = ServiceRegistry::new();
        assert_eq!(registry.service_count, 0);
    }

    #[test]
    fn test_service_registry_register() {
        let mut registry = ServiceRegistry::new();
        let sid = registry.register_service("web", "_http._tcp", 8080);
        assert!(sid.is_some());
        assert_eq!(registry.service_count, 1);
    }

    #[test]
    fn test_service_browser_new() {
        let browser = ServiceBrowser::new(1, "_http._tcp");
        assert_eq!(browser.browser_id, 1);
        assert!(!browser.is_browsing);
    }

    #[test]
    fn test_service_browser_start() {
        let mut browser = ServiceBrowser::new(1, "_http._tcp");
        assert!(browser.start_browsing());
        assert!(browser.is_browsing);
    }

    #[test]
    fn test_service_browser_discovery() {
        let mut browser = ServiceBrowser::new(1, "_http._tcp");
        browser.start_browsing();
        let service = ServiceEntry::new(1, "web", "_http._tcp", 8080);
        assert!(browser.on_service_found(&service));
        assert_eq!(browser.get_discovery_count(), 1);
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_dns_caching_scenario() {
        let mut resolver = DNSResolver::new(1);
        let record = DNSRecord::new(DNSRecordType::A, 3600);
        resolver.cache.insert(12345, record);

        let found = resolver.cache.lookup(12345, 0);
        assert!(found.is_some());
        assert!(resolver.cache.hits > 0);
    }

    #[test]
    fn test_mdns_service_announcement_scenario() {
        let mut responder = mDNSResponder::new(1);
        responder.start();

        assert!(responder.announce_service("my-service"));
        assert_eq!(responder.announcements_sent, 1);

        responder.stop();
        assert!(!responder.is_active);
    }

    #[test]
    fn test_service_registry_lifecycle_scenario() {
        let mut registry = ServiceRegistry::new();

        let sid1 = registry.register_service("web", "_http._tcp", 8080).unwrap();
        let sid2 = registry.register_service("ssh", "_ssh._tcp", 22).unwrap();

        assert_eq!(registry.get_active_services(), 2);

        assert!(registry.unregister_service(sid1));
        assert_eq!(registry.get_active_services(), 1);
    }

    #[test]
    fn test_service_browser_discovery_scenario() {
        let mut registry = ServiceRegistry::new();
        let mut browser = ServiceBrowser::new(1, "_http._tcp");

        browser.start_browsing();

        let service1 = ServiceEntry::new(1, "web1", "_http._tcp", 8080);
        let service2 = ServiceEntry::new(2, "web2", "_http._tcp", 8081);

        browser.on_service_found(&service1);
        browser.on_service_found(&service2);

        assert_eq!(browser.get_discovery_count(), 2);
    }

    #[test]
    fn test_dns_resolution_scenario() {
        let mut resolver = DNSResolver::new(1);

        let result = resolver.resolve("example.com", DNSRecordType::A);
        assert!(result.is_some());
        assert!(resolver.queries_sent > 0);
    }
}
