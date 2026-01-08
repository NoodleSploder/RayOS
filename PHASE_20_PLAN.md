# Phase 20: Rate Limiting & API Governance Infrastructure

**Phase**: 20/20+ (Post-API Gateway Foundation)
**Status**: Planning & Specification
**Target Completion**: ~90 minutes (6 tasks, 3,840 lines)
**Commit Target**: `Phase 20: Rate Limiting & API Governance Infrastructure (6 tasks, 3,840 lines)`

---

## Phase Overview

Building on Phase 19's API Gateway infrastructure, Phase 20 implements comprehensive rate limiting and API governance policies to enable:

1. **Tenant-based Rate Limiting** - Per-service, per-user, per-IP quotas
2. **Quota Management** - Allocation, tracking, and enforcement
3. **Request Prioritization** - SLA-based request routing and queueing
4. **Policy Enforcement** - Declarative governance rules
5. **Cost Attribution** - Track API usage and resource consumption
6. **Observability** - Rate limit metrics, quota analytics, and alerting

This phase transforms the API Gateway from a pure router into a **governed, multi-tenant API platform**.

---

## Design Principles

- **Declarative**: Policies defined in structured config, not code
- **Fair**: Token bucket + leaky bucket algorithms ensure fair distribution
- **Observable**: Every rate limit decision tracked with metrics
- **Flexible**: Support multiple quota dimensions (requests/sec, bytes/sec, events, custom)
- **Recoverable**: Quota reset/rollover and carryover semantics clear
- **Auditable**: Full trace of quota allocations and rate limit triggers

---

## Task Breakdown

### Task 1: Token Bucket & Leaky Bucket Rate Limiting (600 lines)
**Module**: `api_rate_limiter.rs`

**Components**:
- `RateLimitAlgorithm` enum: TokenBucket, LeakyBucket, SlidingWindow, FixedWindow
- `TokenBucket` struct: capacity, refill_rate, current_tokens, last_refill_time
- `LeakyBucket` struct: capacity, leak_rate, pending_requests, drain_time
- `RateLimitRequest`: service_id, user_id, tokens_required, priority
- `RateLimitResponse`: allowed, tokens_remaining, retry_after_ms
- `RateLimiter`: 256 service limits, 512 user limits, decision making

**Methods**:
- `new()`: Initialize rate limiter
- `add_token_bucket()`: Register service with token bucket
- `add_leaky_bucket()`: Register service with leaky bucket
- `allow_request()`: Check if request allowed and update bucket
- `refill_tokens()`: Periodic refill logic
- `get_tokens()`: Query current token count
- `reset_bucket()`: Clear bucket state
- `update_rate()`: Modify refill rate dynamically
- `get_limit_stats()`: Metrics on bucket state
- `drain_bucket()`: Emergency drain
- `peek_available()`: Non-destructive check
- `estimate_wait_time()`: Predict time to next available tokens
- `handle_burst()`: Support brief bursts above baseline
- `get_bucket()`: Retrieve bucket configuration

**Tests**:
- test_token_bucket_creation
- test_token_refill
- test_request_allowed

---

### Task 2: Quota Management System (600 lines)
**Module**: `api_quota.rs`

**Components**:
- `QuotaType` enum: RequestCount, BytesTransferred, ComputeUnits, CustomMetric
- `QuotaDimension`: requests/sec, bytes/sec, errors/min, latency_p99
- `QuotaAllocation`: quota_id, tenant_id, limit, window_size_sec, reset_policy
- `QuotaUsage`: current_usage, peak_usage, window_start_time, requests_count
- `QuotaBucket`: allocation, usage, enforcement_mode (soft/hard)
- `QuotaManager`: 256 allocations, 512 active quotas, enforcement

**Methods**:
- `new()`: Initialize quota manager
- `allocate_quota()`: Create new quota for tenant
- `consume_quota()`: Deduct from quota
- `check_quota()`: Verify available quota without consuming
- `reset_quota()`: Reset to initial value
- `rollover_unused()`: Carryover policy (0-10%)
- `get_quota_status()`: Current usage/available
- `set_enforcement_mode()`: Toggle soft/hard quota
- `update_limit()`: Change quota limit
- `get_peak_usage()`: Historical peak tracking
- `export_usage()`: Get CSV/JSON usage export
- `get_quota_utilization()`: Percentage used
- `estimate_depletion()`: Time to quota exhaustion
- `get_quota_alerts()`: Thresholds crossed (80%, 95%, 100%)

**Tests**:
- test_quota_allocation
- test_quota_consumption
- test_quota_reset

---

### Task 3: Request Prioritization & Queuing (630 lines)
**Module**: `api_priority.rs`

**Components**:
- `RequestPriority` enum: Critical (SLA-bound), High, Normal, Low, Batch
- `SLA` struct: service_id, p95_latency_ms, error_rate_threshold, priority_level
- `PriorityQueue`: 256 queues, per-priority FIFO
- `QueuedRequest`: request_id, priority, arrival_time, sla_deadline, origin
- `PriorityScheduler`: scheduler state, preemption tracking, fairness metrics
- `QueueMetrics`: depth, oldest_request_age, avg_wait_time, rejection_count

**Methods**:
- `new()`: Initialize priority system
- `define_sla()`: Register SLA for service
- `enqueue_request()`: Add request to appropriate priority queue
- `dequeue_next()`: Get next request respecting priority + fairness
- `get_sla()`: Retrieve SLA configuration
- `update_sla()`: Modify SLA parameters
- `preempt_request()`: Cancel low-priority request
- `get_queue_stats()`: Queue depth, wait times
- `estimate_queue_time()`: Predict wait for new request
- `reject_request()`: Return error for overloaded state
- `requeue_request()`: Move to different priority
- `get_fair_share()`: Calculate fair share for each priority
- `enforce_fairness()`: Ensure no starvation

**Tests**:
- test_priority_queue_creation
- test_request_enqueue
- test_sla_enforcement

---

### Task 4: Policy Engine & Rule Evaluation (610 lines)
**Module**: `api_policy.rs`

**Components**:
- `PolicyType` enum: RateLimit, Quota, Authentication, Authorization, Transformation
- `PolicyRule`: condition, action, priority, enabled, audit_log
- `Condition`: service_id, user_role, time_window, source_ip, header_match
- `Action`: allow, deny, throttle, log, alert, redirect
- `PolicyEngine`: 128 policies, condition matching, rule evaluation
- `PolicyDecision`: allowed, reason, enforcement_action, audit_id
- `PolicyContext`: request metadata, user info, service info, environment

**Methods**:
- `new()`: Initialize policy engine
- `add_policy()`: Register new policy rule
- `remove_policy()`: Disable/delete policy
- `evaluate_request()`: Run request through policy chain
- `match_condition()`: Test if request matches condition
- `apply_action()`: Execute policy action
- `get_policy()`: Retrieve policy by ID
- `list_policies()`: List all active policies
- `update_policy()`: Modify existing policy
- `enable_policy()`: Activate disabled policy
- `disable_policy()`: Deactivate policy
- `get_policy_stats()`: Metrics on policy decisions
- `audit_decision()`: Log policy enforcement

**Tests**:
- test_policy_creation
- test_condition_matching
- test_action_enforcement

---

### Task 5: Cost Tracking & Attribution (650 lines)
**Module**: `api_cost.rs`

**Components**:
- `CostMetric` enum: RequestCount, BytesProcessed, ComputeTime, StorageAccess, CacheHit
- `CostModel`: base_cost, per_kilobyte, per_second, per_hit, multipliers
- `CostAttribution`: service_id, tenant_id, cost_amount, timestamp, metric_breakdown
- `BillingEntry`: tenant_id, period, total_cost, metrics, invoice_ready
- `CostCollector`: 512 metrics, cost models, aggregation
- `BillingSchedule`: monthly/daily reset, invoice generation

**Methods**:
- `new()`: Initialize cost tracking
- `define_cost_model()`: Set pricing for service
- `attribute_cost()`: Record cost for operation
- `get_service_cost()`: Total cost for service in period
- `get_tenant_cost()`: Total cost for tenant in period
- `calculate_bill()`: Prepare tenant invoice
- `get_cost_breakdown()`: Detailed cost by metric
- `set_billing_schedule()`: Configure reset periods
- `export_bill()`: Generate CSV/JSON invoice
- `get_cost_trends()`: Historical cost analysis
- `project_cost()`: Estimate end-of-period cost
- `set_cost_alert()`: Alert at cost threshold
- `get_cost_alerts()`: Pending cost warnings
- `reset_cost_window()`: Move to next billing period

**Tests**:
- test_cost_tracker_creation
- test_cost_attribution
- test_billing_calculation

---

### Task 6: Rate Limit Observability & Metrics (620 lines)
**Module**: `api_governance_metrics.rs`

**Components**:
- `GovernanceMetric` enum: RateLimitEvents, QuotaViolations, PolicyEnforcements, CostTracking
- `RateLimitMetric`: service_id, timestamp, allowed, denied, tokens_used, wait_time
- `QuotaMetric`: tenant_id, quota_type, usage, limit, percent_utilized
- `PolicyMetric`: policy_id, matches, denials, alerts, enforcement_time
- `CostMetric`: service_id, tenant_id, cost_amount, metric_type
- `GovernanceCollector`: 1024 metrics, aggregation by window
- `GovernanceAlerts`: threshold config, trigger conditions, notification

**Methods**:
- `new()`: Initialize metrics collector
- `record_rate_limit()`: Log rate limit decision
- `record_quota_event()`: Log quota violation/achievement
- `record_policy_event()`: Log policy enforcement
- `record_cost_event()`: Log cost attribution
- `get_rate_limit_stats()`: Aggregate rate limit metrics
- `get_quota_stats()`: Aggregate quota metrics
- `get_policy_stats()`: Aggregate policy metrics
- `get_cost_stats()`: Aggregate cost metrics
- `get_timeline()`: Metrics over time window
- `get_top_services()`: By requests, cost, violations
- `get_top_tenants()`: By usage, cost, violations
- `export_metrics()`: CSV/JSON export
- `set_alert_threshold()`: Configure alerts
- `get_pending_alerts()`: Active alerts
- `get_metric_percentiles()`: P50, P95, P99 latencies
- `correlate_metrics()`: Link rate limits to cost/quality

**Tests**:
- test_metrics_collector_creation
- test_metric_recording
- test_stats_aggregation

---

## Shell Integration

### Commands Added (6 total)

1. **`ratelimit [cmd]`** - Token bucket & leaky bucket management
   - status: Show rate limit status
   - buckets: List configured buckets
   - reset: Reset bucket state
   - help: Show ratelimit commands

2. **`quota [cmd]`** - Quota management
   - status: Show quota status
   - allocations: List quotas
   - usage: Show quota usage
   - help: Show quota commands

3. **`priority [cmd]`** - Request prioritization
   - queues: Show priority queues
   - sla: Show SLA configuration
   - stats: Queue statistics
   - help: Show priority commands

4. **`policy [cmd]`** - Policy enforcement
   - list: List active policies
   - status: Show policy stats
   - rules: Show rule details
   - help: Show policy commands

5. **`cost [cmd]`** - Cost tracking & attribution
   - status: Show cost status
   - tenants: Show tenant costs
   - services: Show service costs
   - help: Show cost commands

6. **`governance [cmd]`** - Observability & metrics
   - metrics: Show governance metrics
   - alerts: Show active alerts
   - export: Export governance data
   - help: Show governance commands

---

## Build & Integration Plan

### Files to Create
- `crates/kernel-bare/src/api_rate_limiter.rs` (600 lines)
- `crates/kernel-bare/src/api_quota.rs` (600 lines)
- `crates/kernel-bare/src/api_priority.rs` (630 lines)
- `crates/kernel-bare/src/api_policy.rs` (610 lines)
- `crates/kernel-bare/src/api_cost.rs` (650 lines)
- `crates/kernel-bare/src/api_governance_metrics.rs` (620 lines)

### Files to Modify
- `crates/kernel-bare/src/main.rs`: Add 6 module declarations (6 lines)
- `crates/kernel-bare/src/shell.rs`: Add 6 command handlers (~350 lines)

### Total Target
- **Code**: 3,840 lines (6 × ~640 avg per task)
- **Tests**: 18 unit tests (3 per task)
- **Build Time**: <2s
- **Warnings**: <150 total (pre-existing)

---

## Testing Strategy

### Unit Tests (18 total)

Per task (3 tests):
1. Creation/initialization test
2. Core operation test
3. Edge case/metrics test

### Build Verification
```bash
cd crates/kernel-bare && cargo check --release
```

Expected: 0 errors, <2s build time

### Integration Tests
- Shell command dispatch
- Module initialization in main.rs
- Cross-module interactions (RateLimiter → Quota → Cost)

---

## Success Criteria

✅ All 6 modules created and integrated
✅ 18 unit tests passing
✅ 0 compilation errors
✅ 6 shell commands operational
✅ Build time <2s
✅ 3,840+ lines of code
✅ Comprehensive documentation

---

## Acceptance Checklist

- [ ] Phase 20 plan approved
- [ ] 6 task modules created
- [ ] main.rs updated with module declarations
- [ ] shell.rs updated with 6 commands + help text
- [ ] cargo check --release passes
- [ ] 18/18 tests passing
- [ ] PHASE_20_FINAL_REPORT.md created
- [ ] All commits pushed to origin/main

---

## Timeline

**Execution Order**:
1. Create Task 1-3 files in parallel (rate_limiter, quota, priority)
2. Create Task 4-6 files in parallel (policy, cost, metrics)
3. Update main.rs with 6 module declarations
4. Update shell.rs with 6 commands + help section
5. Verify build: `cargo check --release`
6. Commit all changes
7. Create and commit PHASE_20_FINAL_REPORT.md

**Target**: 90 minutes total

---

## Dependencies

None - Phase 20 is standalone infrastructure building on Phase 19's API Gateway foundation.

Can be executed immediately following Phase 19 completion.

---

## References

- Phase 19 Final Report: [PHASE_19_FINAL_REPORT.md](PHASE_19_FINAL_REPORT.md)
- API Gateway Module: [api_gateway.rs](crates/kernel-bare/src/api_gateway.rs)
- Authentication Module: [api_auth.rs](crates/kernel-bare/src/api_auth.rs)
- Mediation Module: [api_mediation.rs](crates/kernel-bare/src/api_mediation.rs)

---

**Phase 20 Planning Complete** ✅

Ready to proceed with implementation.
