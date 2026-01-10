//! Cost Tracking & Attribution
//!
//! Track API usage costs and generate tenant invoices.



/// Cost metric type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostMetric {
    RequestCount,
    BytesProcessed,
    ComputeTime,
    StorageAccess,
    CacheHit,
}

/// Cost model for a service
#[derive(Clone, Copy)]
pub struct CostModel {
    pub service_id: u32,
    pub base_cost: u32,           // per request in cents
    pub per_kilobyte: u16,
    pub per_second: u16,
    pub per_hit: u16,
}

/// Cost attribution entry
#[derive(Clone, Copy)]
pub struct CostAttribution {
    pub service_id: u32,
    pub tenant_id: u32,
    pub cost_amount: u64,
    pub timestamp: u64,
    pub metric_type: u8,  // RequestCount, BytesProcessed, etc.
}

/// Billing entry for a period
#[derive(Clone, Copy)]
pub struct BillingEntry {
    pub tenant_id: u32,
    pub period_start: u64,
    pub period_end: u64,
    pub total_cost: u64,
    pub request_count: u32,
    pub invoice_ready: bool,
}

/// Cost collector
pub struct CostCollector {
    cost_models: [CostModel; 128],
    model_count: u8,

    attributions: [CostAttribution; 512],
    attribution_count: u16,

    billing_entries: [BillingEntry; 256],
    billing_count: u8,

    tenant_costs: [u64; 256],  // tenant_id -> accumulated cost

    total_cost_tracked: u64,
    billing_periods_closed: u16,
}

impl CostCollector {
    /// Create new cost collector
    pub fn new() -> Self {
        CostCollector {
            cost_models: [CostModel {
                service_id: 0,
                base_cost: 10,
                per_kilobyte: 1,
                per_second: 1,
                per_hit: 0,
            }; 128],
            model_count: 0,

            attributions: [CostAttribution {
                service_id: 0,
                tenant_id: 0,
                cost_amount: 0,
                timestamp: 0,
                metric_type: 0,
            }; 512],
            attribution_count: 0,

            billing_entries: [BillingEntry {
                tenant_id: 0,
                period_start: 0,
                period_end: 0,
                total_cost: 0,
                request_count: 0,
                invoice_ready: false,
            }; 256],
            billing_count: 0,

            tenant_costs: [0; 256],

            total_cost_tracked: 0,
            billing_periods_closed: 0,
        }
    }

    /// Define cost model for a service
    pub fn define_cost_model(&mut self, service_id: u32, base_cost: u32, per_kb: u16) -> Option<u32> {
        if (self.model_count as usize) >= 128 {
            return None;
        }

        let model_id = self.model_count as u32;
        self.cost_models[self.model_count as usize] = CostModel {
            service_id,
            base_cost,
            per_kilobyte: per_kb,
            per_second: 1,
            per_hit: 0,
        };
        self.model_count += 1;
        Some(model_id)
    }

    /// Attribute cost to a tenant
    pub fn attribute_cost(&mut self, service_id: u32, tenant_id: u32, cost_amount: u64) -> bool {
        if (self.attribution_count as usize) >= 512 {
            return false;
        }

        self.attributions[self.attribution_count as usize] = CostAttribution {
            service_id,
            tenant_id,
            cost_amount,
            timestamp: 0,
            metric_type: 0,
        };

        // Add to tenant's total cost
        if (tenant_id as usize) < 256 {
            self.tenant_costs[tenant_id as usize] += cost_amount;
        }

        self.total_cost_tracked += cost_amount;
        self.attribution_count += 1;
        true
    }

    /// Get service cost in period
    pub fn get_service_cost(&self, service_id: u32) -> u64 {
        let mut total = 0u64;
        for i in 0..(self.attribution_count as usize) {
            if self.attributions[i].service_id == service_id {
                total += self.attributions[i].cost_amount;
            }
        }
        total
    }

    /// Get tenant cost in period
    pub fn get_tenant_cost(&self, tenant_id: u32) -> u64 {
        if (tenant_id as usize) < 256 {
            self.tenant_costs[tenant_id as usize]
        } else {
            0
        }
    }

    /// Calculate bill for tenant
    pub fn calculate_bill(&mut self, tenant_id: u32, period_end: u64) -> Option<BillingEntry> {
        let cost = self.get_tenant_cost(tenant_id);

        let entry = BillingEntry {
            tenant_id,
            period_start: 0,
            period_end,
            total_cost: cost,
            request_count: 0,
            invoice_ready: true,
        };

        Some(entry)
    }

    /// Get cost breakdown by metric
    pub fn get_cost_breakdown(&self, tenant_id: u32) -> (u64, u32, u32) {
        let mut total_cost = 0u64;
        let mut request_count = 0u32;
        let service_count = 0u32;

        for i in 0..(self.attribution_count as usize) {
            if self.attributions[i].tenant_id == tenant_id {
                total_cost += self.attributions[i].cost_amount;
                request_count += 1;
            }
        }

        (total_cost, request_count, service_count)
    }

    /// Project cost at end of period
    pub fn project_cost(&self, tenant_id: u32, days_elapsed: u32, total_days: u32) -> u64 {
        let current_cost = self.get_tenant_cost(tenant_id);

        if days_elapsed == 0 {
            return 0;
        }

        let daily_rate = current_cost / (days_elapsed as u64);
        daily_rate * (total_days as u64)
    }

    /// Get cost statistics
    pub fn get_cost_stats(&self) -> (u64, u16, u16) {
        (self.total_cost_tracked, self.billing_periods_closed, self.attribution_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_tracker_creation() {
        let cc = CostCollector::new();
        assert_eq!(cc.total_cost_tracked, 0);
    }

    #[test]
    fn test_cost_attribution() {
        let mut cc = CostCollector::new();
        let attributed = cc.attribute_cost(1, 1, 100);
        assert!(attributed);
    }

    #[test]
    fn test_billing_calculation() {
        let mut cc = CostCollector::new();
        cc.attribute_cost(1, 1, 100);
        let bill = cc.calculate_bill(1, 0);
        assert!(bill.is_some());
    }
}
