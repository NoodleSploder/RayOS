//! Circuit Breaker & Resilience Patterns
//!
//! Implement fault tolerance patterns for service calls.

#![no_std]

use core::cmp;

/// Circuit breaker state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Retry policy
#[derive(Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u8,
    pub initial_backoff_ms: u32,
    pub max_backoff_ms: u32,
    pub backoff_multiplier: u8,
}

/// Bulkhead configuration
#[derive(Clone, Copy)]
pub struct BulkheadConfig {
    pub max_concurrent_calls: u16,
    pub max_wait_duration_ms: u32,
    pub queue_size: u16,
}

/// Circuit breaker
#[derive(Clone, Copy)]
pub struct CircuitBreaker {
    pub service_id: u32,
    pub state: CircuitState,
    pub failure_count: u16,
    pub success_count: u16,
    pub failure_threshold: u8,
    pub success_threshold: u8,
    pub last_failure_time: u64,
    pub timeout_ms: u32,
}

/// Resilience policy
#[derive(Clone, Copy)]
pub struct ResiliencePolicy {
    pub service_id: u32,
    pub timeout_ms: u32,
    pub retry_policy: RetryPolicy,
    pub bulkhead: BulkheadConfig,
}

/// Bulkhead
#[derive(Clone, Copy)]
pub struct Bulkhead {
    pub service_id: u32,
    pub current_calls: u16,
    pub queued_calls: u16,
    pub rejected_calls: u32,
}

/// Resilience manager
pub struct ResilienceManager {
    breakers: [CircuitBreaker; 128],
    breaker_count: u8,
    
    policies: [ResiliencePolicy; 64],
    policy_count: u8,
    
    bulkheads: [Bulkhead; 64],
    bulkhead_count: u8,
    
    total_calls: u32,
    failed_calls: u16,
    retried_calls: u16,
    timeout_calls: u16,
}

impl ResilienceManager {
    /// Create new resilience manager
    pub fn new() -> Self {
        ResilienceManager {
            breakers: [CircuitBreaker {
                service_id: 0,
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                failure_threshold: 5,
                success_threshold: 2,
                last_failure_time: 0,
                timeout_ms: 5000,
            }; 128],
            breaker_count: 0,
            
            policies: [ResiliencePolicy {
                service_id: 0,
                timeout_ms: 5000,
                retry_policy: RetryPolicy {
                    max_retries: 3,
                    initial_backoff_ms: 100,
                    max_backoff_ms: 10000,
                    backoff_multiplier: 2,
                },
                bulkhead: BulkheadConfig {
                    max_concurrent_calls: 100,
                    max_wait_duration_ms: 5000,
                    queue_size: 256,
                },
            }; 64],
            policy_count: 0,
            
            bulkheads: [Bulkhead {
                service_id: 0,
                current_calls: 0,
                queued_calls: 0,
                rejected_calls: 0,
            }; 64],
            bulkhead_count: 0,
            
            total_calls: 0,
            failed_calls: 0,
            retried_calls: 0,
            timeout_calls: 0,
        }
    }
    
    /// Register a circuit breaker for a service
    pub fn register_breaker(&mut self, service_id: u32) -> Option<u32> {
        if (self.breaker_count as usize) >= 128 {
            return None;
        }
        
        let breaker_id = self.breaker_count as u32;
        self.breakers[self.breaker_count as usize] = CircuitBreaker {
            service_id,
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            failure_threshold: 5,
            success_threshold: 2,
            last_failure_time: 0,
            timeout_ms: 5000,
        };
        self.breaker_count += 1;
        Some(breaker_id)
    }
    
    /// Register a resilience policy
    pub fn register_policy(&mut self, service_id: u32, timeout_ms: u32) -> Option<u32> {
        if (self.policy_count as usize) >= 64 {
            return None;
        }
        
        let policy_id = self.policy_count as u32;
        self.policies[self.policy_count as usize] = ResiliencePolicy {
            service_id,
            timeout_ms,
            retry_policy: RetryPolicy {
                max_retries: 3,
                initial_backoff_ms: 100,
                max_backoff_ms: 10000,
                backoff_multiplier: 2,
            },
            bulkhead: BulkheadConfig {
                max_concurrent_calls: 100,
                max_wait_duration_ms: 5000,
                queue_size: 256,
            },
        };
        self.policy_count += 1;
        Some(policy_id)
    }
    
    /// Register a bulkhead for a service
    pub fn register_bulkhead(&mut self, service_id: u32, max_concurrent: u16) -> Option<u32> {
        if (self.bulkhead_count as usize) >= 64 {
            return None;
        }
        
        let bulkhead_id = self.bulkhead_count as u32;
        self.bulkheads[self.bulkhead_count as usize] = Bulkhead {
            service_id,
            current_calls: 0,
            queued_calls: 0,
            rejected_calls: 0,
        };
        self.bulkhead_count += 1;
        Some(bulkhead_id)
    }
    
    /// Check if call is allowed
    pub fn can_execute(&mut self, service_id: u32) -> bool {
        self.total_calls += 1;
        
        // Find breaker
        for i in 0..(self.breaker_count as usize) {
            if self.breakers[i].service_id == service_id {
                let state = self.breakers[i].state;
                if state == CircuitState::Open {
                    return false;
                }
                return true;
            }
        }
        
        true
    }
    
    /// Record successful call
    pub fn record_success(&mut self, service_id: u32) -> bool {
        for i in 0..(self.breaker_count as usize) {
            if self.breakers[i].service_id == service_id {
                self.breakers[i].failure_count = 0;
                self.breakers[i].success_count += 1;
                
                // Transition to closed if in half-open
                if self.breakers[i].state == CircuitState::HalfOpen
                    && self.breakers[i].success_count >= (self.breakers[i].success_threshold as u16) {
                    self.breakers[i].state = CircuitState::Closed;
                }
                
                return true;
            }
        }
        false
    }
    
    /// Record failed call
    pub fn record_failure(&mut self, service_id: u32) -> bool {
        self.failed_calls += 1;
        
        for i in 0..(self.breaker_count as usize) {
            if self.breakers[i].service_id == service_id {
                self.breakers[i].failure_count += 1;
                self.breakers[i].success_count = 0;
                self.breakers[i].last_failure_time = 0;
                
                // Transition to open if failures exceed threshold
                if self.breakers[i].failure_count >= (self.breakers[i].failure_threshold as u16) {
                    self.breakers[i].state = CircuitState::Open;
                }
                
                return true;
            }
        }
        false
    }
    
    /// Record timeout
    pub fn record_timeout(&mut self, service_id: u32) -> bool {
        self.timeout_calls += 1;
        self.record_failure(service_id)
    }
    
    /// Record retry
    pub fn record_retry(&mut self, service_id: u32) -> bool {
        self.retried_calls += 1;
        true
    }
    
    /// Try to transition to half-open
    pub fn try_half_open(&mut self, service_id: u32) -> bool {
        for i in 0..(self.breaker_count as usize) {
            if self.breakers[i].service_id == service_id && self.breakers[i].state == CircuitState::Open {
                self.breakers[i].state = CircuitState::HalfOpen;
                self.breakers[i].success_count = 0;
                return true;
            }
        }
        false
    }
    
    /// Acquire bulkhead slot
    pub fn acquire_slot(&mut self, service_id: u32) -> bool {
        for i in 0..(self.bulkhead_count as usize) {
            if self.bulkheads[i].service_id == service_id {
                // Check if we have capacity
                if self.bulkheads[i].current_calls < 100 {
                    self.bulkheads[i].current_calls += 1;
                    return true;
                } else {
                    self.bulkheads[i].rejected_calls += 1;
                    return false;
                }
            }
        }
        false
    }
    
    /// Release bulkhead slot
    pub fn release_slot(&mut self, service_id: u32) -> bool {
        for i in 0..(self.bulkhead_count as usize) {
            if self.bulkheads[i].service_id == service_id && self.bulkheads[i].current_calls > 0 {
                self.bulkheads[i].current_calls -= 1;
                return true;
            }
        }
        false
    }
    
    /// Get breaker state
    pub fn get_breaker(&self, service_id: u32) -> Option<CircuitBreaker> {
        for i in 0..(self.breaker_count as usize) {
            if self.breakers[i].service_id == service_id {
                return Some(self.breakers[i]);
            }
        }
        None
    }
    
    /// Get resilience stats
    pub fn get_stats(&self) -> (u32, u16, u16, u16) {
        (self.total_calls, self.failed_calls, self.retried_calls, self.timeout_calls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resilience_manager_creation() {
        let rm = ResilienceManager::new();
        assert_eq!(rm.total_calls, 0);
    }
    
    #[test]
    fn test_breaker_registration() {
        let mut rm = ResilienceManager::new();
        let breaker_id = rm.register_breaker(1);
        assert!(breaker_id.is_some());
    }
    
    #[test]
    fn test_circuit_breaker_flow() {
        let mut rm = ResilienceManager::new();
        rm.register_breaker(1);
        
        // Initially can execute
        assert!(rm.can_execute(1));
        
        // Record success
        rm.record_success(1);
        assert!(rm.can_execute(1));
    }
}
