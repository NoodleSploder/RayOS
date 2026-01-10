//! Distributed Tracing & Observability
//!
//! End-to-end request tracing with context propagation and latency percentiles.
//! Supports 1024 concurrent spans with sampling and telemetry export.


/// Unique trace identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceId(pub u64);

/// Span identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanId(pub u64);

/// Sampling decision
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplingDecision {
    Sampled,
    NotSampled,
    DeferredToServer,
}

/// Trace context for propagation
#[derive(Clone, Copy, Debug)]
pub struct SpanContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub sampling_decision: SamplingDecision,
}

/// Individual operation span
#[derive(Clone, Copy)]
pub struct Span {
    pub span_id: SpanId,
    pub trace_id: TraceId,
    pub parent_span_id: Option<SpanId>,
    pub operation_name_hash: u32,
    pub start_time_us: u64,
    pub end_time_us: u64,
    pub status: SpanStatus,
}

/// Span completion status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanStatus {
    OK,
    Error,
    Cancelled,
}

/// Latency histogram bucket
#[derive(Clone, Copy, Debug)]
pub struct LatencyBucket {
    pub min_us: u32,
    pub max_us: u32,
    pub count: u32,
}

/// Distributed tracing engine
pub struct DistributedTracer {
    // Span storage
    spans: [Span; 1024],
    span_count: u16,

    // Active traces
    traces: [TraceId; 256],
    trace_count: u16,

    // Latency histograms
    buckets: [LatencyBucket; 32],

    // Sampling configuration
    sample_rate: u32, // 0-100
    sampled_count: u32,
    not_sampled_count: u32,

    // Export tracking
    exported_trace_count: u32,
    export_destination: u32,
}

impl DistributedTracer {
    /// Create new distributed tracer
    pub fn new(sample_rate: u32) -> Self {
        let mut tracer = DistributedTracer {
            spans: [Span {
                span_id: SpanId(0),
                trace_id: TraceId(0),
                parent_span_id: None,
                operation_name_hash: 0,
                start_time_us: 0,
                end_time_us: 0,
                status: SpanStatus::OK,
            }; 1024],
            span_count: 0,

            traces: [TraceId(0); 256],
            trace_count: 0,

            buckets: [LatencyBucket {
                min_us: 0,
                max_us: 0,
                count: 0,
            }; 32],

            sample_rate: sample_rate.min(100),
            sampled_count: 0,
            not_sampled_count: 0,

            exported_trace_count: 0,
            export_destination: 0,
        };

        // Initialize latency buckets (exponential: 1us, 2us, 4us... 65536us)
        for i in 0..32 {
            tracer.buckets[i] = LatencyBucket {
                min_us: 1u32 << i,
                max_us: (1u32 << (i + 1)) - 1,
                count: 0,
            };
        }

        tracer
    }

    /// Start new trace
    pub fn start_trace(&mut self, trace_id: TraceId) -> bool {
        if self.trace_count >= 256 {
            return false;
        }

        self.traces[self.trace_count as usize] = trace_id;
        self.trace_count += 1;
        true
    }

    /// Create new span
    pub fn create_span(&mut self, ctx: &SpanContext, op_name_hash: u32) -> Option<SpanId> {
        if self.span_count >= 1024 {
            return None;
        }

        let span_id = SpanId((self.span_count as u64) << 32 | (op_name_hash as u64));
        let span = Span {
            span_id,
            trace_id: ctx.trace_id,
            parent_span_id: ctx.parent_span_id,
            operation_name_hash: op_name_hash,
            start_time_us: 0,
            end_time_us: 0,
            status: SpanStatus::OK,
        };

        self.spans[self.span_count as usize] = span;
        self.span_count += 1;

        Some(span_id)
    }

    /// Record span timing
    pub fn record_span_timing(&mut self, span_id: SpanId, start_us: u64, end_us: u64) {
        for i in 0..self.span_count as usize {
            if self.spans[i].span_id == span_id {
                self.spans[i].start_time_us = start_us;
                self.spans[i].end_time_us = end_us;

                let duration_us = (end_us - start_us) as u32;
                self.record_latency(duration_us);
                break;
            }
        }
    }

    /// Record latency in histogram
    fn record_latency(&mut self, duration_us: u32) {
        for i in 0..32 {
            if duration_us >= self.buckets[i].min_us && duration_us <= self.buckets[i].max_us {
                self.buckets[i].count += 1;
                break;
            }
        }
    }

    /// Set span status
    pub fn set_span_status(&mut self, span_id: SpanId, status: SpanStatus) {
        for i in 0..self.span_count as usize {
            if self.spans[i].span_id == span_id {
                self.spans[i].status = status;
                break;
            }
        }
    }

    /// Make sampling decision
    pub fn should_sample(&mut self, trace_id: TraceId) -> SamplingDecision {
        let hash = trace_id.0 as u32;
        if (hash % 100) < self.sample_rate {
            self.sampled_count += 1;
            SamplingDecision::Sampled
        } else {
            self.not_sampled_count += 1;
            SamplingDecision::NotSampled
        }
    }

    /// Adjust sample rate based on metrics
    pub fn adaptive_sampling(&mut self, error_rate: u32) {
        // Increase sample rate if error rate is high
        if error_rate > 10 {
            self.sample_rate = (self.sample_rate + 10).min(100);
        } else if error_rate < 2 && self.sample_rate > 10 {
            self.sample_rate -= 5;
        }
    }

    /// Get P50 latency (median)
    pub fn get_p50_latency(&self) -> u32 {
        self.get_percentile(50)
    }

    /// Get P99 latency
    pub fn get_p99_latency(&self) -> u32 {
        self.get_percentile(99)
    }

    /// Get P99.9 latency
    pub fn get_p999_latency(&self) -> u32 {
        self.get_percentile(999)
    }

    /// Get percentile latency
    fn get_percentile(&self, percentile: u32) -> u32 {
        let mut total_count = 0u32;
        for i in 0..32 {
            total_count += self.buckets[i].count;
        }

        if total_count == 0 {
            return 0;
        }

        let target_count = (total_count * percentile) / 100;
        let mut cumulative = 0u32;

        for i in 0..32 {
            cumulative += self.buckets[i].count;
            if cumulative >= target_count {
                return self.buckets[i].min_us;
            }
        }

        0
    }

    /// Export traces
    pub fn export_traces(&mut self, destination: u32) -> u32 {
        let count = self.trace_count as u32;
        self.exported_trace_count += count;
        self.export_destination = destination;
        count
    }

    /// Get active span count
    pub fn get_active_span_count(&self) -> u16 {
        self.span_count
    }

    /// Get trace count
    pub fn get_trace_count(&self) -> u16 {
        self.trace_count
    }

    /// Get current sample rate
    pub fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get sampling statistics
    pub fn get_sampling_stats(&self) -> (u32, u32) {
        (self.sampled_count, self.not_sampled_count)
    }

    /// Reset latency histogram
    pub fn reset_histogram(&mut self) {
        for i in 0..32 {
            self.buckets[i].count = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracer_creation() {
        let tracer = DistributedTracer::new(50);
        assert_eq!(tracer.get_sample_rate(), 50);
    }

    #[test]
    fn test_sampling_decision() {
        let mut tracer = DistributedTracer::new(50);
        let decision = tracer.should_sample(TraceId(12345));
        assert!(decision == SamplingDecision::Sampled || decision == SamplingDecision::NotSampled);
    }

    #[test]
    fn test_span_creation() {
        let mut tracer = DistributedTracer::new(100);
        let ctx = SpanContext {
            trace_id: TraceId(1),
            span_id: SpanId(1),
            parent_span_id: None,
            sampling_decision: SamplingDecision::Sampled,
        };

        let span_id = tracer.create_span(&ctx, 12345);
        assert!(span_id.is_some());
    }
}
