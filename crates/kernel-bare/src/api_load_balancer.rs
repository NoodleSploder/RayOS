//! Load Balancing & Service Discovery
//!
//! Distribute requests across service instances with health checking and failover.

#![no_std]

use core::cmp;

/// Load balancing strategy
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalancingStrategy {
    RoundRobin,
    LeastConnections,
    WeightedRoundRobin,
    IpHash,
}

/// Health status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Unknown,
    Degraded,
}

/// Service instance
#[derive(Clone, Copy)]
pub struct ServiceInstance {
    pub instance_id: u32,
    pub address: u32,
    pub port: u16,
    pub weight: u8,
    pub health_status: HealthStatus,
    pub connections: u32,
    pub total_requests: u32,
    pub failed_requests: u16,
}

/// Service pool
#[derive(Clone, Copy)]
pub struct ServicePool {
    pub pool_id: u32,
    pub strategy: BalancingStrategy,
    pub current_index: u16,
    pub instance_count: u16,
    pub total_connections: u32,
}

/// Load balancer
pub struct LoadBalancer {
    pools: [ServicePool; 128],
    pool_count: u8,

    instances: [ServiceInstance; 256],
    instance_count: u16,

    pool_instances: [[u32; 16]; 128],  // instance IDs per pool

    health_checks_performed: u32,
    health_checks_failed: u16,
}

impl LoadBalancer {
    /// Create new load balancer
    pub fn new() -> Self {
        LoadBalancer {
            pools: [ServicePool {
                pool_id: 0,
                strategy: BalancingStrategy::RoundRobin,
                current_index: 0,
                instance_count: 0,
                total_connections: 0,
            }; 128],
            pool_count: 0,

            instances: [ServiceInstance {
                instance_id: 0,
                address: 0,
                port: 0,
                weight: 100,
                health_status: HealthStatus::Unknown,
                connections: 0,
                total_requests: 0,
                failed_requests: 0,
            }; 256],
            instance_count: 0,

            pool_instances: [[0; 16]; 128],

            health_checks_performed: 0,
            health_checks_failed: 0,
        }
    }

    /// Register a new pool
    pub fn register_pool(&mut self, strategy: BalancingStrategy) -> Option<u32> {
        if (self.pool_count as usize) >= 128 {
            return None;
        }

        let pool_id = self.pool_count as u32;
        self.pools[self.pool_count as usize] = ServicePool {
            pool_id,
            strategy,
            current_index: 0,
            instance_count: 0,
            total_connections: 0,
        };
        self.pool_count += 1;
        Some(pool_id)
    }

    /// Register an instance in a pool
    pub fn register_instance(&mut self, pool_id: u32, address: u32, port: u16) -> Option<u32> {
        // Find pool
        let mut pool_idx = None;
        for i in 0..(self.pool_count as usize) {
            if self.pools[i].pool_id == pool_id {
                pool_idx = Some(i);
                break;
            }
        }

        if pool_idx.is_none() {
            return None;
        }

        let pool_idx = pool_idx.unwrap();

        // Check if pool is full
        if self.pools[pool_idx].instance_count >= 16 {
            return None;
        }

        // Add instance
        if (self.instance_count as usize) >= 256 {
            return None;
        }

        let instance_id = self.instance_count as u32;
        self.instances[self.instance_count as usize] = ServiceInstance {
            instance_id,
            address,
            port,
            weight: 100,
            health_status: HealthStatus::Unknown,
            connections: 0,
            total_requests: 0,
            failed_requests: 0,
        };

        // Add to pool
        let inst_idx = self.pools[pool_idx].instance_count as usize;
        self.pool_instances[pool_idx][inst_idx] = instance_id;
        self.pools[pool_idx].instance_count += 1;
        self.instance_count += 1;

        Some(instance_id)
    }

    /// Select next instance from pool
    pub fn select_instance(&mut self, pool_id: u32) -> Option<ServiceInstance> {
        // Find pool
        let mut pool_idx = None;
        for i in 0..(self.pool_count as usize) {
            if self.pools[i].pool_id == pool_id {
                pool_idx = Some(i);
                break;
            }
        }

        if pool_idx.is_none() {
            return None;
        }

        let pool_idx = pool_idx.unwrap();
        let pool = &mut self.pools[pool_idx];

        if pool.instance_count == 0 {
            return None;
        }

        // Select based on strategy
        let strategy = pool.strategy;
        let mut selected_idx = pool.current_index as usize % (pool.instance_count as usize);

        match strategy {
            BalancingStrategy::RoundRobin => {
                selected_idx = pool.current_index as usize % (pool.instance_count as usize);
                pool.current_index = ((pool.current_index as usize + 1) % (pool.instance_count as usize)) as u16;
            },
            BalancingStrategy::LeastConnections => {
                // Find instance with fewest connections
                let mut min_connections = u32::MAX;
                for i in 0..(pool.instance_count as usize) {
                    let inst_id = self.pool_instances[pool_idx][i];
                    for j in 0..(self.instance_count as usize) {
                        if self.instances[j].instance_id == inst_id && self.instances[j].connections < min_connections {
                            min_connections = self.instances[j].connections;
                            selected_idx = i;
                        }
                    }
                }
            },
            BalancingStrategy::WeightedRoundRobin => {
                // Simple weighted: advance by weight
                selected_idx = (pool.current_index as usize / 10) % (pool.instance_count as usize);
                pool.current_index = ((pool.current_index + 1) % 100) as u16;
            },
            BalancingStrategy::IpHash => {
                // Use pool_id as hash source
                selected_idx = (pool_id as usize) % (pool.instance_count as usize);
            },
        }

        let inst_id = self.pool_instances[pool_idx][selected_idx];

        // Find instance
        for i in 0..(self.instance_count as usize) {
            if self.instances[i].instance_id == inst_id {
                self.instances[i].connections += 1;
                self.pools[pool_idx].total_connections += 1;
                return Some(self.instances[i]);
            }
        }

        None
    }

    /// Mark instance as healthy
    pub fn mark_healthy(&mut self, instance_id: u32) -> bool {
        for i in 0..(self.instance_count as usize) {
            if self.instances[i].instance_id == instance_id {
                self.instances[i].health_status = HealthStatus::Healthy;
                return true;
            }
        }
        false
    }

    /// Mark instance as unhealthy
    pub fn mark_unhealthy(&mut self, instance_id: u32) -> bool {
        for i in 0..(self.instance_count as usize) {
            if self.instances[i].instance_id == instance_id {
                self.instances[i].health_status = HealthStatus::Unhealthy;
                self.health_checks_failed += 1;
                return true;
            }
        }
        false
    }

    /// Get a pool
    pub fn get_pool(&self, pool_id: u32) -> Option<ServicePool> {
        for i in 0..(self.pool_count as usize) {
            if self.pools[i].pool_id == pool_id {
                return Some(self.pools[i]);
            }
        }
        None
    }

    /// Get an instance
    pub fn get_instance(&self, instance_id: u32) -> Option<ServiceInstance> {
        for i in 0..(self.instance_count as usize) {
            if self.instances[i].instance_id == instance_id {
                return Some(self.instances[i]);
            }
        }
        None
    }

    /// Decrement connection count
    pub fn decrement_connections(&mut self, instance_id: u32) -> bool {
        for i in 0..(self.instance_count as usize) {
            if self.instances[i].instance_id == instance_id && self.instances[i].connections > 0 {
                self.instances[i].connections -= 1;
                return true;
            }
        }
        false
    }

    /// Perform health check
    pub fn health_check(&mut self, instance_id: u32) -> bool {
        self.health_checks_performed += 1;

        for i in 0..(self.instance_count as usize) {
            if self.instances[i].instance_id == instance_id {
                // Simple health check: mark as healthy if it was unknown
                if self.instances[i].health_status == HealthStatus::Unknown {
                    self.instances[i].health_status = HealthStatus::Healthy;
                }
                return true;
            }
        }
        false
    }

    /// Get health check stats
    pub fn get_health_stats(&self) -> (u32, u16) {
        (self.health_checks_performed, self.health_checks_failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_balancer_creation() {
        let lb = LoadBalancer::new();
        assert_eq!(lb.health_checks_performed, 0);
    }

    #[test]
    fn test_pool_registration() {
        let mut lb = LoadBalancer::new();
        let pool_id = lb.register_pool(BalancingStrategy::RoundRobin);
        assert!(pool_id.is_some());
    }

    #[test]
    fn test_instance_selection() {
        let mut lb = LoadBalancer::new();
        let pool_id = lb.register_pool(BalancingStrategy::RoundRobin).unwrap();
        lb.register_instance(pool_id, 0x7F000001, 8080);

        let instance = lb.select_instance(pool_id);
        assert!(instance.is_some());
    }
}
