//! API Monitoring & Metrics
//!
//! Collect and track API performance metrics.


use core::cmp;

/// Metric type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricType {
    RequestCount,
    ResponseTime,
    ErrorRate,
    Throughput,
    Latency,
}

/// Percentile
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Percentile {
    P50,
    P95,
    P99,
    P999,
}

/// Metric data point
#[derive(Clone, Copy)]
pub struct MetricDataPoint {
    pub metric_id: u32,
    pub value: u64,
    pub timestamp: u64,
    pub service_id: u32,
}

/// Latency bucket
#[derive(Clone, Copy)]
pub struct LatencyBucket {
    pub lower_bound_ms: u32,
    pub upper_bound_ms: u32,
    pub count: u32,
}

/// Service metrics
#[derive(Clone, Copy)]
pub struct ServiceMetrics {
    pub service_id: u32,
    pub total_requests: u32,
    pub successful_requests: u32,
    pub failed_requests: u16,
    pub total_response_time_us: u64,
    pub min_response_time_us: u32,
    pub max_response_time_us: u32,
    pub error_rate_percent: u8,
}

/// API metrics collector
pub struct ApiMetricsCollector {
    metrics: [MetricDataPoint; 512],
    metric_count: u16,

    service_metrics: [ServiceMetrics; 64],
    service_count: u8,

    latency_buckets: [LatencyBucket; 16],

    percentiles: [u64; 4],  // P50, P95, P99, P999

    total_requests: u32,
    total_errors: u16,
    window_start_time: u64,
}

impl ApiMetricsCollector {
    /// Create new metrics collector
    pub fn new() -> Self {
        ApiMetricsCollector {
            metrics: [MetricDataPoint {
                metric_id: 0,
                value: 0,
                timestamp: 0,
                service_id: 0,
            }; 512],
            metric_count: 0,

            service_metrics: [ServiceMetrics {
                service_id: 0,
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                total_response_time_us: 0,
                min_response_time_us: u32::MAX,
                max_response_time_us: 0,
                error_rate_percent: 0,
            }; 64],
            service_count: 0,

            latency_buckets: [LatencyBucket {
                lower_bound_ms: 0,
                upper_bound_ms: 0,
                count: 0,
            }; 16],

            percentiles: [0; 4],

            total_requests: 0,
            total_errors: 0,
            window_start_time: 0,
        }
    }

    /// Register a service for metrics collection
    pub fn register_service(&mut self, service_id: u32) -> Option<u32> {
        if (self.service_count as usize) >= 64 {
            return None;
        }

        let service_idx = self.service_count as usize;
        self.service_metrics[service_idx] = ServiceMetrics {
            service_id,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_response_time_us: 0,
            min_response_time_us: u32::MAX,
            max_response_time_us: 0,
            error_rate_percent: 0,
        };
        self.service_count += 1;
        Some(service_id)
    }

    /// Record a request metric
    pub fn record_request(&mut self, service_id: u32, response_time_us: u32, success: bool) -> bool {
        self.total_requests += 1;

        // Find service metrics
        let mut service_idx = None;
        for i in 0..(self.service_count as usize) {
            if self.service_metrics[i].service_id == service_id {
                service_idx = Some(i);
                break;
            }
        }

        if service_idx.is_none() {
            return false;
        }

        let idx = service_idx.unwrap();

        // Update service metrics
        self.service_metrics[idx].total_requests += 1;
        self.service_metrics[idx].total_response_time_us += response_time_us as u64;

        if response_time_us < self.service_metrics[idx].min_response_time_us {
            self.service_metrics[idx].min_response_time_us = response_time_us;
        }

        if response_time_us > self.service_metrics[idx].max_response_time_us {
            self.service_metrics[idx].max_response_time_us = response_time_us;
        }

        if success {
            self.service_metrics[idx].successful_requests += 1;
        } else {
            self.service_metrics[idx].failed_requests += 1;
            self.total_errors += 1;
        }

        // Record metric data point
        if (self.metric_count as usize) < 512 {
            self.metrics[self.metric_count as usize] = MetricDataPoint {
                metric_id: self.metric_count as u32,
                value: response_time_us as u64,
                timestamp: 0,
                service_id,
            };
            self.metric_count += 1;
        }

        // Bucket into latency range
        let bucket_idx = (response_time_us / 100) as usize;
        if bucket_idx < 16 {
            self.latency_buckets[bucket_idx].count += 1;
        }

        true
    }

    /// Calculate percentile for a service
    pub fn get_percentile(&self, service_id: u32, percentile: Percentile) -> u32 {
        let mut matching_metrics = [0u32; 128];
        let mut count = 0;

        for i in 0..(self.metric_count as usize) {
            if self.metrics[i].service_id == service_id && count < 128 {
                matching_metrics[count] = self.metrics[i].value as u32;
                count += 1;
            }
        }

        if count == 0 {
            return 0;
        }

        // Simple percentile calculation
        let percentile_idx = match percentile {
            Percentile::P50 => count / 2,
            Percentile::P95 => (count * 95) / 100,
            Percentile::P99 => (count * 99) / 100,
            Percentile::P999 => (count * 999) / 1000,
        };

        if percentile_idx < count {
            matching_metrics[percentile_idx]
        } else {
            0
        }
    }

    /// Get service metrics
    pub fn get_service_metrics(&self, service_id: u32) -> Option<ServiceMetrics> {
        for i in 0..(self.service_count as usize) {
            if self.service_metrics[i].service_id == service_id {
                return Some(self.service_metrics[i]);
            }
        }
        None
    }

    /// Calculate average response time
    pub fn get_average_response_time(&self, service_id: u32) -> u32 {
        for i in 0..(self.service_count as usize) {
            if self.service_metrics[i].service_id == service_id {
                if self.service_metrics[i].total_requests > 0 {
                    return (self.service_metrics[i].total_response_time_us
                        / (self.service_metrics[i].total_requests as u64)) as u32;
                }
            }
        }
        0
    }

    /// Get error rate
    pub fn get_error_rate(&self, service_id: u32) -> u8 {
        for i in 0..(self.service_count as usize) {
            if self.service_metrics[i].service_id == service_id {
                if self.service_metrics[i].total_requests > 0 {
                    let error_rate = (self.service_metrics[i].failed_requests as u32 * 100)
                        / self.service_metrics[i].total_requests;
                    return cmp::min(error_rate as u8, 100);
                }
            }
        }
        0
    }

    /// Get throughput (requests per second)
    pub fn get_throughput(&self, service_id: u32) -> u32 {
        for i in 0..(self.service_count as usize) {
            if self.service_metrics[i].service_id == service_id {
                // Simple: return requests per 60 seconds
                return self.service_metrics[i].total_requests / cmp::max(1, 60);
            }
        }
        0
    }

    /// Reset metrics window
    pub fn reset_window(&mut self) {
        self.window_start_time = 0;
        self.metric_count = 0;

        for i in 0..(self.service_count as usize) {
            self.service_metrics[i].total_requests = 0;
            self.service_metrics[i].successful_requests = 0;
            self.service_metrics[i].failed_requests = 0;
            self.service_metrics[i].total_response_time_us = 0;
            self.service_metrics[i].min_response_time_us = u32::MAX;
            self.service_metrics[i].max_response_time_us = 0;
        }
    }

    /// Get total requests
    pub fn get_total_requests(&self) -> u32 {
        self.total_requests
    }

    /// Get total errors
    pub fn get_total_errors(&self) -> u16 {
        self.total_errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_creation() {
        let mc = ApiMetricsCollector::new();
        assert_eq!(mc.total_requests, 0);
    }

    #[test]
    fn test_service_registration() {
        let mut mc = ApiMetricsCollector::new();
        let service_id = mc.register_service(1);
        assert!(service_id.is_some());
    }

    #[test]
    fn test_metric_recording() {
        let mut mc = ApiMetricsCollector::new();
        mc.register_service(1);
        mc.record_request(1, 500, true);

        assert_eq!(mc.get_total_requests(), 1);
        let metrics = mc.get_service_metrics(1);
        assert!(metrics.is_some());
    }
}
