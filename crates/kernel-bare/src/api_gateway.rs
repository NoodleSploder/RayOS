//! API Gateway Core & Request Routing
//!
//! Core gateway infrastructure with route matching, service registry, and request dispatching.

#![no_std]

use core::cmp;

/// Route pattern type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutePattern {
    Exact,
    Prefix,
    Wildcard,
    Regex,
}

/// Service health status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceStatus {
    Healthy,
    Unhealthy,
    Unknown,
    Degraded,
}

/// Service endpoint definition
#[derive(Clone, Copy)]
pub struct ServiceEndpoint {
    pub service_id: u32,
    pub host: u32,
    pub port: u16,
    pub weight: u8,
    pub health_status: ServiceStatus,
    pub request_count: u32,
}

/// API request
#[derive(Clone, Copy)]
pub struct ApiRequest {
    pub method: u8,           // 0=GET, 1=POST, 2=PUT, 3=DELETE, 4=PATCH, 5=HEAD
    pub path_hash: u32,       // Hash of path
    pub path_len: u16,
    pub body_len: u16,
    pub request_id: u32,
    pub timestamp: u64,
}

/// API response
#[derive(Clone, Copy)]
pub struct ApiResponse {
    pub status_code: u16,     // HTTP status
    pub body_len: u16,
    pub response_time_us: u32,
    pub service_id: u32,
}

/// Routing rule
#[derive(Clone, Copy)]
pub struct Route {
    pub route_id: u32,
    pub pattern_type: RoutePattern,
    pub path_prefix: [u8; 64],
    pub prefix_len: u8,
    pub service_id: u32,
    pub priority: u8,
    pub enabled: bool,
}

/// API Gateway
pub struct ApiGateway {
    services: [ServiceEndpoint; 256],
    service_count: u16,

    routes: [Route; 512],
    route_count: u16,

    request_queue: [ApiRequest; 256],
    queue_head: u8,
    queue_tail: u8,

    total_requests: u32,
    total_responses: u32,
    errors: u16,
}

impl ApiGateway {
    /// Create new API gateway
    pub fn new() -> Self {
        ApiGateway {
            services: [ServiceEndpoint {
                service_id: 0,
                host: 0,
                port: 0,
                weight: 0,
                health_status: ServiceStatus::Unknown,
                request_count: 0,
            }; 256],
            service_count: 0,

            routes: [Route {
                route_id: 0,
                pattern_type: RoutePattern::Exact,
                path_prefix: [0; 64],
                prefix_len: 0,
                service_id: 0,
                priority: 0,
                enabled: false,
            }; 512],
            route_count: 0,

            request_queue: [ApiRequest {
                method: 0,
                path_hash: 0,
                path_len: 0,
                body_len: 0,
                request_id: 0,
                timestamp: 0,
            }; 256],
            queue_head: 0,
            queue_tail: 0,

            total_requests: 0,
            total_responses: 0,
            errors: 0,
        }
    }

    /// Register a new service
    pub fn register_service(&mut self, service_id: u32, host: u32, port: u16) -> bool {
        if (self.service_count as usize) >= 256 {
            return false;
        }

        // Check if service already exists
        for i in 0..(self.service_count as usize) {
            if self.services[i].service_id == service_id {
                return false; // Already registered
            }
        }

        self.services[self.service_count as usize] = ServiceEndpoint {
            service_id,
            host,
            port,
            weight: 100,
            health_status: ServiceStatus::Unknown,
            request_count: 0,
        };
        self.service_count += 1;
        true
    }

    /// Add a routing rule
    pub fn add_route(&mut self, pattern: RoutePattern, path: &[u8], service_id: u32, priority: u8) -> bool {
        if (self.route_count as usize) >= 512 {
            return false;
        }

        // Verify service exists
        let mut service_exists = false;
        for i in 0..(self.service_count as usize) {
            if self.services[i].service_id == service_id {
                service_exists = true;
                break;
            }
        }

        if !service_exists {
            return false;
        }

        let path_len = cmp::min(path.len(), 64);
        let mut path_prefix = [0u8; 64];
        path_prefix[..path_len].copy_from_slice(&path[..path_len]);

        let route_id = self.route_count as u32;
        self.routes[self.route_count as usize] = Route {
            route_id,
            pattern_type: pattern,
            path_prefix,
            prefix_len: path_len as u8,
            service_id,
            priority,
            enabled: true,
        };
        self.route_count += 1;
        true
    }

    /// Remove a routing rule
    pub fn remove_route(&mut self, route_id: u32) -> bool {
        for i in 0..(self.route_count as usize) {
            if self.routes[i].route_id == route_id {
                self.routes[i].enabled = false;
                return true;
            }
        }
        false
    }

    /// Route a request to the appropriate service
    pub fn route_request(&self, path: &[u8], method: u8) -> Option<u32> {
        let mut best_match_service = None;
        let mut best_match_priority = 0u8;

        for i in 0..(self.route_count as usize) {
            if !self.routes[i].enabled {
                continue;
            }

            let route = &self.routes[i];
            let matches = match route.pattern_type {
                RoutePattern::Exact => {
                    path.len() == route.prefix_len as usize &&
                    path == &route.path_prefix[..(route.prefix_len as usize)]
                },
                RoutePattern::Prefix => {
                    path.len() >= route.prefix_len as usize &&
                    path[..(route.prefix_len as usize)] == route.path_prefix[..(route.prefix_len as usize)]
                },
                RoutePattern::Wildcard => {
                    // Simple wildcard: * matches anything
                    route.prefix_len == 1 && route.path_prefix[0] == b'*'
                },
                RoutePattern::Regex => {
                    // Simplified regex: just check prefix for now
                    path.len() >= route.prefix_len as usize &&
                    path[..(route.prefix_len as usize)] == route.path_prefix[..(route.prefix_len as usize)]
                },
            };

            if matches && route.priority >= best_match_priority {
                best_match_priority = route.priority;
                best_match_service = Some(route.service_id);
            }
        }

        best_match_service
    }

    /// Dispatch request to a service
    pub fn dispatch_request(&mut self, service_id: u32, request: ApiRequest) -> bool {
        // Find service and increment request count
        for i in 0..(self.service_count as usize) {
            if self.services[i].service_id == service_id {
                self.services[i].request_count += 1;
                self.total_requests += 1;

                // Add to request queue
                if (((self.queue_tail as usize + 1) % 256) != (self.queue_head as usize)) {
                    self.request_queue[self.queue_tail as usize] = request;
                    self.queue_tail = ((self.queue_tail as usize + 1) % 256) as u8;
                    return true;
                }
                return false; // Queue full
            }
        }

        false // Service not found
    }

    /// Get a service by ID
    pub fn get_service(&self, service_id: u32) -> Option<ServiceEndpoint> {
        for i in 0..(self.service_count as usize) {
            if self.services[i].service_id == service_id {
                return Some(self.services[i]);
            }
        }
        None
    }

    /// Check if service is healthy
    pub fn is_service_healthy(&self, service_id: u32) -> bool {
        for i in 0..(self.service_count as usize) {
            if self.services[i].service_id == service_id {
                return self.services[i].health_status == ServiceStatus::Healthy;
            }
        }
        false
    }

    /// Get route count
    pub fn get_route_count(&self) -> u16 {
        let mut count = 0u16;
        for i in 0..(self.route_count as usize) {
            if self.routes[i].enabled {
                count += 1;
            }
        }
        count
    }

    /// Get service count
    pub fn get_service_count(&self) -> u16 {
        self.service_count
    }

    /// Get total requests
    pub fn get_total_requests(&self) -> u32 {
        self.total_requests
    }

    /// Record response
    pub fn record_response(&mut self) {
        self.total_responses += 1;
    }

    /// Record error
    pub fn record_error(&mut self) {
        self.errors += 1;
    }

    /// Get error count
    pub fn get_error_count(&self) -> u16 {
        self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_creation() {
        let gateway = ApiGateway::new();
        assert_eq!(gateway.get_service_count(), 0);
        assert_eq!(gateway.get_total_requests(), 0);
    }

    #[test]
    fn test_service_registration() {
        let mut gateway = ApiGateway::new();
        assert!(gateway.register_service(1, 0x7F000001, 8080));
        assert_eq!(gateway.get_service_count(), 1);
    }

    #[test]
    fn test_route_matching() {
        let mut gateway = ApiGateway::new();
        gateway.register_service(1, 0x7F000001, 8080);
        gateway.add_route(RoutePattern::Prefix, b"/api", 1, 10);

        let matched = gateway.route_request(b"/api/users", 0);
        assert_eq!(matched, Some(1));
    }
}
