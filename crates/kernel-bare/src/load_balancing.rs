/// Load Balancing & Traffic Management
///
/// Intelligent request distribution across backend servers with health checking,
/// session affinity, and multiple load balancing strategies.

use core::cmp::min;

const MAX_BACKENDS: usize = 32;
const MAX_LOAD_BALANCERS: usize = 8;
const MAX_HEALTHCHECK_INTERVAL: u32 = 60;
const MIN_HEALTHCHECK_INTERVAL: u32 = 1;

/// Load balancing policy enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoadBalancingPolicy {
    RoundRobin = 0,
    LeastConnections = 1,
    IpHash = 2,
    Weighted = 3,
    Random = 4,
}

/// Backend server state enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackendState {
    Healthy,
    Unhealthy,
    Draining,
    Offline,
}

/// Health check status
#[derive(Clone, Copy, Debug)]
pub struct HealthCheckStatus {
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_check_timestamp: u64,
    pub is_healthy: bool,
}

impl HealthCheckStatus {
    pub fn new(interval: u32) -> Self {
        HealthCheckStatus {
            interval_seconds: min(interval, MAX_HEALTHCHECK_INTERVAL),
            timeout_seconds: 5,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_check_timestamp: 0,
            is_healthy: true,
        }
    }

    pub fn record_success(&mut self) {
        self.consecutive_successes += 1;
        self.consecutive_failures = 0;
        self.is_healthy = true;
    }

    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        if self.consecutive_failures >= 3 {
            self.is_healthy = false;
        }
    }
}

/// Backend server
#[derive(Clone, Copy, Debug)]
pub struct BackendServer {
    pub server_id: u32,
    pub state: BackendState,
    pub weight: u32,
    pub active_connections: u32,
    pub total_requests: u64,
    pub total_errors: u32,
    pub health_check: HealthCheckStatus,
}

impl BackendServer {
    pub fn new(server_id: u32) -> Self {
        BackendServer {
            server_id,
            state: BackendState::Healthy,
            weight: 100,
            active_connections: 0,
            total_requests: 0,
            total_errors: 0,
            health_check: HealthCheckStatus::new(10),
        }
    }

    pub fn get_error_rate(&self) -> u32 {
        if self.total_requests == 0 {
            0
        } else {
            ((self.total_errors as u64 * 100) / self.total_requests) as u32
        }
    }
}

/// Session affinity (sticky session)
#[derive(Clone, Copy, Debug)]
pub struct SessionAffinity {
    pub client_id: u32,
    pub backend_id: u32,
    pub timeout_seconds: u32,
    pub last_access: u64,
}

impl SessionAffinity {
    pub fn new(client_id: u32, backend_id: u32, timeout: u32) -> Self {
        SessionAffinity {
            client_id,
            backend_id,
            timeout_seconds: timeout,
            last_access: 0,
        }
    }

    pub fn is_expired(&self, current_time: u64) -> bool {
        if self.last_access == 0 {
            false
        } else {
            (current_time - self.last_access) > (self.timeout_seconds as u64)
        }
    }
}

/// Load balancer statistics
#[derive(Clone, Copy, Debug)]
pub struct LoadBalancerStats {
    pub total_requests: u64,
    pub total_errors: u32,
    pub active_connections: u32,
    pub requests_per_second: u32,
    pub average_latency_ms: u32,
    pub backend_count: u32,
}

impl LoadBalancerStats {
    pub fn new() -> Self {
        LoadBalancerStats {
            total_requests: 0,
            total_errors: 0,
            active_connections: 0,
            requests_per_second: 0,
            average_latency_ms: 0,
            backend_count: 0,
        }
    }

    pub fn get_error_rate(&self) -> u32 {
        if self.total_requests == 0 {
            0
        } else {
            ((self.total_errors as u64 * 100) / self.total_requests) as u32
        }
    }
}

/// Load Balancer
pub struct LoadBalancer {
    pub balancer_id: u32,
    pub policy: LoadBalancingPolicy,
    pub backends: [Option<BackendServer>; MAX_BACKENDS],
    pub sessions: [Option<SessionAffinity>; 128],
    pub stats: LoadBalancerStats,
    pub round_robin_index: u32,
    pub active_backend_count: u32,
    pub session_count: u32,
}

impl LoadBalancer {
    pub fn new(balancer_id: u32, policy: LoadBalancingPolicy) -> Self {
        LoadBalancer {
            balancer_id,
            policy,
            backends: [None; MAX_BACKENDS],
            sessions: [None; 128],
            stats: LoadBalancerStats::new(),
            round_robin_index: 0,
            active_backend_count: 0,
            session_count: 0,
        }
    }

    pub fn add_backend(&mut self, weight: u32) -> u32 {
        for i in 0..MAX_BACKENDS {
            if self.backends[i].is_none() {
                let server_id = i as u32 + 1;
                let mut server = BackendServer::new(server_id);
                server.weight = weight;
                self.backends[i] = Some(server);
                self.active_backend_count += 1;
                self.stats.backend_count += 1;
                return server_id;
            }
        }
        0
    }

    pub fn remove_backend(&mut self, server_id: u32) -> bool {
        let idx = (server_id as usize) - 1;
        if idx < MAX_BACKENDS {
            if self.backends[idx].is_some() {
                self.backends[idx] = None;
                self.active_backend_count -= 1;
                self.stats.backend_count -= 1;
                return true;
            }
        }
        false
    }

    pub fn select_backend(&mut self, client_id: u32) -> u32 {
        // Check for existing session affinity
        for i in 0..128 {
            if let Some(session) = self.sessions[i] {
                if session.client_id == client_id {
                    return session.backend_id;
                }
            }
        }

        // Select backend based on policy
        match self.policy {
            LoadBalancingPolicy::RoundRobin => self.select_round_robin(),
            LoadBalancingPolicy::LeastConnections => self.select_least_connections(),
            LoadBalancingPolicy::IpHash => self.select_ip_hash(client_id),
            LoadBalancingPolicy::Weighted => self.select_weighted(),
            LoadBalancingPolicy::Random => self.select_random(client_id),
        }
    }

    pub fn select_round_robin(&mut self) -> u32 {
        let mut attempts = 0;
        loop {
            let idx = (self.round_robin_index as usize) % MAX_BACKENDS;
            self.round_robin_index += 1;
            attempts += 1;

            if let Some(backend) = self.backends[idx] {
                if backend.state == BackendState::Healthy {
                    return backend.server_id;
                }
            }

            if attempts >= MAX_BACKENDS {
                return 0; // No healthy backend
            }
        }
    }

    pub fn select_least_connections(&mut self) -> u32 {
        let mut min_connections = u32::MAX;
        let mut selected_id = 0;

        for i in 0..MAX_BACKENDS {
            if let Some(backend) = self.backends[i] {
                if backend.state == BackendState::Healthy
                    && backend.active_connections < min_connections
                {
                    min_connections = backend.active_connections;
                    selected_id = backend.server_id;
                }
            }
        }
        selected_id
    }

    pub fn select_ip_hash(&mut self, client_id: u32) -> u32 {
        let hash = client_id.wrapping_mul(2654435761);
        let mut idx = (hash as usize) % MAX_BACKENDS;
        let mut attempts = 0;

        loop {
            if let Some(backend) = self.backends[idx] {
                if backend.state == BackendState::Healthy {
                    return backend.server_id;
                }
            }
            idx = (idx + 1) % MAX_BACKENDS;
            attempts += 1;

            if attempts >= MAX_BACKENDS {
                return 0;
            }
        }
    }

    pub fn select_weighted(&mut self) -> u32 {
        let total_weight: u32 = self
            .backends
            .iter()
            .filter_map(|&b| {
                b.filter(|srv| srv.state == BackendState::Healthy)
                    .map(|srv| srv.weight)
            })
            .sum();

        if total_weight == 0 {
            return 0;
        }

        let mut cumulative = 0;
        let selector = (self.round_robin_index as u32) % total_weight;
        self.round_robin_index += 1;

        for backend_opt in self.backends.iter() {
            if let Some(backend) = backend_opt {
                if backend.state == BackendState::Healthy {
                    cumulative += backend.weight;
                    if selector < cumulative {
                        return backend.server_id;
                    }
                }
            }
        }
        0
    }

    pub fn select_random(&mut self, client_id: u32) -> u32 {
        let mut hash = client_id;
        hash = hash.wrapping_mul(2654435761);
        let mut idx = (hash as usize) % MAX_BACKENDS;
        let mut attempts = 0;

        loop {
            if let Some(backend) = self.backends[idx] {
                if backend.state == BackendState::Healthy {
                    return backend.server_id;
                }
            }
            idx = (idx + 1) % MAX_BACKENDS;
            attempts += 1;

            if attempts >= MAX_BACKENDS {
                return 0;
            }
        }
    }

    pub fn create_session(&mut self, client_id: u32, backend_id: u32, timeout: u32) -> bool {
        for i in 0..128 {
            if self.sessions[i].is_none() {
                let session = SessionAffinity::new(client_id, backend_id, timeout);
                self.sessions[i] = Some(session);
                self.session_count += 1;
                return true;
            }
        }
        false
    }

    pub fn remove_session(&mut self, client_id: u32) -> bool {
        for i in 0..128 {
            if let Some(session) = self.sessions[i] {
                if session.client_id == client_id {
                    self.sessions[i] = None;
                    self.session_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn record_request(&mut self, server_id: u32, success: bool, latency_ms: u32) {
        let idx = (server_id as usize) - 1;
        if idx < MAX_BACKENDS {
            if let Some(mut backend) = self.backends[idx] {
                backend.total_requests += 1;
                if !success {
                    backend.total_errors += 1;
                    backend.health_check.record_failure();
                } else {
                    backend.health_check.record_success();
                }
                self.backends[idx] = Some(backend);
            }
        }

        self.stats.total_requests += 1;
        if !success {
            self.stats.total_errors += 1;
        }
        self.stats.average_latency_ms = (self.stats.average_latency_ms + latency_ms) / 2;
    }

    pub fn get_healthy_backend_count(&self) -> u32 {
        let mut count = 0;
        for backend_opt in self.backends.iter() {
            if let Some(backend) = backend_opt {
                if backend.state == BackendState::Healthy {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn set_policy(&mut self, policy: LoadBalancingPolicy) {
        self.policy = policy;
        self.round_robin_index = 0;
    }
}

/// Load Balancer Manager
pub struct LoadBalancerManager {
    balancer_count: u32,
    max_balancers: u32,
}

impl LoadBalancerManager {
    pub fn new() -> Self {
        LoadBalancerManager {
            balancer_count: 0,
            max_balancers: MAX_LOAD_BALANCERS as u32,
        }
    }

    pub fn create_balancer(&mut self, _policy: LoadBalancingPolicy) -> u32 {
        if self.balancer_count < self.max_balancers {
            self.balancer_count += 1;
            self.balancer_count
        } else {
            0
        }
    }

    pub fn get_balancer_count(&self) -> u32 {
        self.balancer_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let mut lb = LoadBalancer::new(1, LoadBalancingPolicy::RoundRobin);
        let id = lb.add_backend(100);
        assert!(id > 0);
        assert_eq!(lb.active_backend_count, 1);
    }

    #[test]
    fn test_round_robin() {
        let mut lb = LoadBalancer::new(1, LoadBalancingPolicy::RoundRobin);
        lb.add_backend(100);
        lb.add_backend(100);
        let first = lb.select_round_robin();
        let second = lb.select_round_robin();
        assert!(first > 0);
        assert!(second > 0);
    }

    #[test]
    fn test_least_connections() {
        let mut lb = LoadBalancer::new(1, LoadBalancingPolicy::LeastConnections);
        lb.add_backend(100);
        lb.add_backend(100);
        let selected = lb.select_least_connections();
        assert!(selected > 0);
    }

    #[test]
    fn test_health_check() {
        let mut hc = HealthCheckStatus::new(10);
        hc.record_success();
        assert!(hc.is_healthy);
        hc.record_failure();
        hc.record_failure();
        hc.record_failure();
        assert!(!hc.is_healthy);
    }

    #[test]
    fn test_session_affinity() {
        let mut lb = LoadBalancer::new(1, LoadBalancingPolicy::RoundRobin);
        let id = lb.add_backend(100);
        lb.create_session(100, id, 300);
        let selected = lb.select_backend(100);
        assert_eq!(selected, id);
    }

    #[test]
    fn test_weighted_selection() {
        let mut lb = LoadBalancer::new(1, LoadBalancingPolicy::Weighted);
        lb.add_backend(100);
        lb.add_backend(50);
        let selected = lb.select_weighted();
        assert!(selected > 0);
    }

    #[test]
    fn test_record_request() {
        let mut lb = LoadBalancer::new(1, LoadBalancingPolicy::RoundRobin);
        lb.add_backend(100);
        lb.record_request(1, true, 10);
        assert_eq!(lb.stats.total_requests, 1);
    }

    #[test]
    fn test_balancer_manager() {
        let mut manager = LoadBalancerManager::new();
        let id = manager.create_balancer(LoadBalancingPolicy::RoundRobin);
        assert!(id > 0);
        assert_eq!(manager.get_balancer_count(), 1);
    }
}
