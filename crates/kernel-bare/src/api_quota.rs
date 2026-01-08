//! Quota Management System
//!
//! Track and enforce tenant quotas across multiple dimensions.

#![no_std]

use core::cmp;

/// Quota type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuotaType {
    RequestCount,
    BytesTransferred,
    ComputeUnits,
    CustomMetric,
}

/// Quota allocation
#[derive(Clone, Copy)]
pub struct QuotaAllocation {
    pub quota_id: u32,
    pub tenant_id: u32,
    pub quota_type: QuotaType,
    pub limit: u64,
    pub window_size_sec: u32,
    pub reset_policy: u8,  // 0=monthly, 1=daily, 2=hourly
}

/// Quota usage tracking
#[derive(Clone, Copy)]
pub struct QuotaUsage {
    pub quota_id: u32,
    pub current_usage: u64,
    pub peak_usage: u64,
    pub window_start_time: u64,
    pub requests_count: u32,
}

/// Quota bucket
#[derive(Clone, Copy)]
pub struct QuotaBucket {
    pub allocation: QuotaAllocation,
    pub usage: QuotaUsage,
    pub enforcement_mode: u8,  // 0=soft, 1=hard
}

/// Quota manager
pub struct QuotaManager {
    allocations: [QuotaAllocation; 256],
    allocation_count: u8,
    
    quotas: [QuotaBucket; 512],
    quota_count: u16,
    
    tenant_quotas: [[u32; 32]; 128],  // tenant_id -> quota_ids
    
    total_quota_checks: u32,
    quota_violations: u16,
    quota_resets: u16,
}

impl QuotaManager {
    /// Create new quota manager
    pub fn new() -> Self {
        QuotaManager {
            allocations: [QuotaAllocation {
                quota_id: 0,
                tenant_id: 0,
                quota_type: QuotaType::RequestCount,
                limit: 10000,
                window_size_sec: 86400,
                reset_policy: 0,
            }; 256],
            allocation_count: 0,
            
            quotas: [QuotaBucket {
                allocation: QuotaAllocation {
                    quota_id: 0,
                    tenant_id: 0,
                    quota_type: QuotaType::RequestCount,
                    limit: 10000,
                    window_size_sec: 86400,
                    reset_policy: 0,
                },
                usage: QuotaUsage {
                    quota_id: 0,
                    current_usage: 0,
                    peak_usage: 0,
                    window_start_time: 0,
                    requests_count: 0,
                },
                enforcement_mode: 1,
            }; 512],
            quota_count: 0,
            
            tenant_quotas: [[0; 32]; 128],
            
            total_quota_checks: 0,
            quota_violations: 0,
            quota_resets: 0,
        }
    }
    
    /// Allocate a new quota for a tenant
    pub fn allocate_quota(&mut self, tenant_id: u32, quota_type: QuotaType, limit: u64) -> Option<u32> {
        if (self.quota_count as usize) >= 512 {
            return None;
        }
        
        let quota_id = self.quota_count as u32;
        let allocation = QuotaAllocation {
            quota_id,
            tenant_id,
            quota_type,
            limit,
            window_size_sec: 86400,
            reset_policy: 0,
        };
        
        self.quotas[self.quota_count as usize] = QuotaBucket {
            allocation,
            usage: QuotaUsage {
                quota_id,
                current_usage: 0,
                peak_usage: 0,
                window_start_time: 0,
                requests_count: 0,
            },
            enforcement_mode: 1,
        };
        
        // Add to tenant's quota list
        if (tenant_id as usize) < 128 {
            for i in 0..32 {
                if self.tenant_quotas[tenant_id as usize][i] == 0 {
                    self.tenant_quotas[tenant_id as usize][i] = quota_id;
                    break;
                }
            }
        }
        
        self.quota_count += 1;
        Some(quota_id)
    }
    
    /// Consume quota
    pub fn consume_quota(&mut self, quota_id: u32, amount: u64) -> bool {
        self.total_quota_checks += 1;
        
        if (quota_id as usize) >= (self.quota_count as usize) {
            return false;
        }
        
        let quota = &mut self.quotas[quota_id as usize];
        
        // Check if quota exceeded
        if quota.usage.current_usage + amount > quota.allocation.limit {
            self.quota_violations += 1;
            
            // If soft mode, allow but log
            if quota.enforcement_mode == 0 {
                quota.usage.current_usage += amount;
                return true;
            }
            
            return false;
        }
        
        quota.usage.current_usage += amount;
        quota.usage.requests_count += 1;
        
        // Update peak usage
        if quota.usage.current_usage > quota.usage.peak_usage {
            quota.usage.peak_usage = quota.usage.current_usage;
        }
        
        true
    }
    
    /// Check quota without consuming
    pub fn check_quota(&self, quota_id: u32, amount: u64) -> bool {
        if (quota_id as usize) >= (self.quota_count as usize) {
            return false;
        }
        
        let quota = &self.quotas[quota_id as usize];
        quota.usage.current_usage + amount <= quota.allocation.limit
    }
    
    /// Reset quota to initial value
    pub fn reset_quota(&mut self, quota_id: u32) -> bool {
        if (quota_id as usize) >= (self.quota_count as usize) {
            return false;
        }
        
        self.quotas[quota_id as usize].usage.current_usage = 0;
        self.quota_resets += 1;
        true
    }
    
    /// Get quota status
    pub fn get_quota_status(&self, quota_id: u32) -> Option<(u64, u64, u32)> {
        if (quota_id as usize) < (self.quota_count as usize) {
            let quota = &self.quotas[quota_id as usize];
            Some((quota.usage.current_usage, quota.allocation.limit, quota.usage.requests_count))
        } else {
            None
        }
    }
    
    /// Set enforcement mode (soft=0 or hard=1)
    pub fn set_enforcement_mode(&mut self, quota_id: u32, mode: u8) -> bool {
        if (quota_id as usize) < (self.quota_count as usize) {
            self.quotas[quota_id as usize].enforcement_mode = cmp::min(mode, 1);
            return true;
        }
        false
    }
    
    /// Update quota limit
    pub fn update_limit(&mut self, quota_id: u32, new_limit: u64) -> bool {
        if (quota_id as usize) < (self.quota_count as usize) {
            self.quotas[quota_id as usize].allocation.limit = new_limit;
            return true;
        }
        false
    }
    
    /// Get quota utilization percentage
    pub fn get_quota_utilization(&self, quota_id: u32) -> u8 {
        if (quota_id as usize) < (self.quota_count as usize) {
            let quota = &self.quotas[quota_id as usize];
            let percent = (quota.usage.current_usage * 100) / cmp::max(quota.allocation.limit, 1);
            cmp::min(percent as u8, 100)
        } else {
            0
        }
    }
    
    /// Get quota statistics
    pub fn get_quota_stats(&self) -> (u32, u16, u16) {
        (self.total_quota_checks, self.quota_violations, self.quota_resets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_quota_allocation() {
        let mut qm = QuotaManager::new();
        let quota_id = qm.allocate_quota(1, QuotaType::RequestCount, 1000);
        assert!(quota_id.is_some());
    }
    
    #[test]
    fn test_quota_consumption() {
        let mut qm = QuotaManager::new();
        let quota_id = qm.allocate_quota(1, QuotaType::RequestCount, 1000).unwrap();
        let allowed = qm.consume_quota(quota_id, 500);
        assert!(allowed);
    }
    
    #[test]
    fn test_quota_reset() {
        let mut qm = QuotaManager::new();
        let quota_id = qm.allocate_quota(1, QuotaType::RequestCount, 1000).unwrap();
        qm.consume_quota(quota_id, 500);
        let reset = qm.reset_quota(quota_id);
        assert!(reset);
    }
}
