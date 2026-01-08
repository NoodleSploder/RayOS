//! Request Prioritization & Queuing
//!
//! Manage SLA-bound request scheduling with fairness.

#![no_std]

use core::cmp;

/// Request priority level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestPriority {
    Critical,  // SLA-bound, low latency
    High,
    Normal,
    Low,
    Batch,
}

/// SLA definition
#[derive(Clone, Copy)]
pub struct SLA {
    pub service_id: u32,
    pub p95_latency_ms: u32,
    pub error_rate_threshold: u8,
    pub priority_level: u8,  // 0=Critical, 1=High, 2=Normal, 3=Low, 4=Batch
}

/// Queued request
#[derive(Clone, Copy)]
pub struct QueuedRequest {
    pub request_id: u32,
    pub priority: u8,
    pub arrival_time: u64,
    pub sla_deadline: u64,
    pub origin: u32,  // service_id or user_id
}

/// Priority queue
#[derive(Clone, Copy)]
pub struct PriorityQueue {
    pub priority_level: u8,
    pub queue_depth: u16,
    pub oldest_request_age: u64,
    pub total_requests: u32,
}

/// Priority scheduler
pub struct PriorityScheduler {
    queues: [PriorityQueue; 5],  // One queue per priority level
    
    slas: [SLA; 128],
    sla_count: u8,
    
    requests: [QueuedRequest; 512],
    request_count: u16,
    
    total_enqueued: u32,
    total_dequeued: u32,
    preempted_requests: u16,
}

impl PriorityScheduler {
    /// Create new priority scheduler
    pub fn new() -> Self {
        let mut queues = [PriorityQueue {
            priority_level: 0,
            queue_depth: 0,
            oldest_request_age: 0,
            total_requests: 0,
        }; 5];
        
        // Initialize all 5 priority levels
        for i in 0..5 {
            queues[i].priority_level = i as u8;
        }
        
        PriorityScheduler {
            queues,
            slas: [SLA {
                service_id: 0,
                p95_latency_ms: 1000,
                error_rate_threshold: 10,
                priority_level: 2,
            }; 128],
            sla_count: 0,
            
            requests: [QueuedRequest {
                request_id: 0,
                priority: 2,
                arrival_time: 0,
                sla_deadline: 0,
                origin: 0,
            }; 512],
            request_count: 0,
            
            total_enqueued: 0,
            total_dequeued: 0,
            preempted_requests: 0,
        }
    }
    
    /// Define SLA for a service
    pub fn define_sla(&mut self, service_id: u32, p95_latency_ms: u32) -> Option<u32> {
        if (self.sla_count as usize) >= 128 {
            return None;
        }
        
        let sla_id = self.sla_count as u32;
        self.slas[self.sla_count as usize] = SLA {
            service_id,
            p95_latency_ms,
            error_rate_threshold: 10,
            priority_level: 2,
        };
        self.sla_count += 1;
        Some(sla_id)
    }
    
    /// Enqueue a request
    pub fn enqueue_request(&mut self, request_id: u32, priority: u8, origin: u32) -> bool {
        if (self.request_count as usize) >= 512 {
            return false;
        }
        
        let priority_bounded = cmp::min(priority, 4);
        
        self.requests[self.request_count as usize] = QueuedRequest {
            request_id,
            priority: priority_bounded,
            arrival_time: 0,
            sla_deadline: 0,
            origin,
        };
        
        self.queues[priority_bounded as usize].queue_depth += 1;
        self.queues[priority_bounded as usize].total_requests += 1;
        self.request_count += 1;
        self.total_enqueued += 1;
        
        true
    }
    
    /// Dequeue next request (respects priority)
    pub fn dequeue_next(&mut self) -> Option<QueuedRequest> {
        // Search from highest to lowest priority
        for priority in 0..5 {
            for i in 0..(self.request_count as usize) {
                if self.requests[i].priority == priority {
                    let request = self.requests[i];
                    
                    // Remove from queue
                    self.requests.copy_within((i + 1).., i);
                    self.request_count -= 1;
                    self.queues[priority as usize].queue_depth -= 1;
                    self.total_dequeued += 1;
                    
                    return Some(request);
                }
            }
        }
        None
    }
    
    /// Get SLA for service
    pub fn get_sla(&self, service_id: u32) -> Option<SLA> {
        for i in 0..(self.sla_count as usize) {
            if self.slas[i].service_id == service_id {
                return Some(self.slas[i]);
            }
        }
        None
    }
    
    /// Update SLA parameters
    pub fn update_sla(&mut self, service_id: u32, p95_latency_ms: u32) -> bool {
        for i in 0..(self.sla_count as usize) {
            if self.slas[i].service_id == service_id {
                self.slas[i].p95_latency_ms = p95_latency_ms;
                return true;
            }
        }
        false
    }
    
    /// Preempt a low-priority request
    pub fn preempt_request(&mut self, request_id: u32) -> bool {
        for i in 0..(self.request_count as usize) {
            if self.requests[i].request_id == request_id {
                let priority = self.requests[i].priority;
                self.requests.copy_within((i + 1).., i);
                self.request_count -= 1;
                self.queues[priority as usize].queue_depth -= 1;
                self.preempted_requests += 1;
                return true;
            }
        }
        false
    }
    
    /// Get queue statistics
    pub fn get_queue_stats(&self, priority: u8) -> Option<(u16, u32, u64)> {
        if (priority as usize) < 5 {
            let q = self.queues[priority as usize];
            Some((q.queue_depth, q.total_requests, q.oldest_request_age))
        } else {
            None
        }
    }
    
    /// Get scheduler statistics
    pub fn get_scheduler_stats(&self) -> (u32, u32, u16) {
        (self.total_enqueued, self.total_dequeued, self.preempted_requests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_priority_queue_creation() {
        let ps = PriorityScheduler::new();
        assert_eq!(ps.queues.len(), 5);
    }
    
    #[test]
    fn test_request_enqueue() {
        let mut ps = PriorityScheduler::new();
        let enqueued = ps.enqueue_request(1, 2, 1);
        assert!(enqueued);
    }
    
    #[test]
    fn test_sla_enforcement() {
        let mut ps = PriorityScheduler::new();
        let sla_id = ps.define_sla(1, 100);
        assert!(sla_id.is_some());
    }
}
