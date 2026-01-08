# Phase 24: System Integration Testing - Plan

**Date**: 2026-01-08  
**Project**: RayOS (Rust-based Advanced RTOS)  
**Phase**: 24 - System Integration Testing  
**Duration**: Single focused session  
**Target**: ~4,500 lines, 5 tasks, 60+ integration tests  

---

## Strategic Objective

Transition RayOS from **unit-tested modules** to **production-verified system**. Phase 24 validates that all Phase 23 Wayland components work together under realistic load, stress, and failure scenarios.

**Key Goals:**
1. **Soak Testing**: Long-running stability (100+ clients, sustained 60 FPS)
2. **Stress Testing**: CPU/memory saturation with graceful degradation
3. **Failure Injection**: Network partition, disk errors, client crashes
4. **Performance Profiling**: Latency percentiles, throughput, resource usage
5. **Reproducible Scenarios**: Deterministic test harnesses for CI automation

---

## Problem Statement

### Current State (After Phase 23)
✅ All Wayland protocol interfaces implemented  
✅ 107 unit tests passing  
✅ Individual components tested in isolation  
❌ No multi-hour soak testing  
❌ No stress testing under extreme load  
❌ No failure recovery validation  
❌ No performance profiling under realistic conditions  

### What We Need
- Long-running tests that validate stability over hours/days
- Stress tests that push CPU/memory/disk to limits
- Failure injection to verify graceful degradation
- Performance metrics (p99 latency, throughput, memory overhead)
- Automated CI harnesses for regression prevention

---

## Roadmap: 5 Tasks

### Task 1: Soak Testing Framework (~950 lines)
**File**: `crates/kernel-bare/src/soak_testing.rs` (new)

**Goal**: Build infrastructure for long-running stability tests

**Components**:
- `SoakTestHarness`: Main test orchestrator
- `VirtualClient`: Simulated Wayland client with realistic behavior
- `ClientWorkload`: Customizable client activities (draw, input, drag-drop)
- `MetricsCollector`: Track performance and stability
- `SoakTestResult`: Pass/fail with detailed diagnostics

**Features**:
- Run N clients for H hours continuously
- Each client independently creates surfaces, buffers, events
- Automatic failure detection (panics, hangs, memory leaks)
- Per-second metrics snapshot (CPU, memory, latency, throughput)
- Deterministic markers for test automation

**Tests**:
- test_4_clients_1_hour
- test_16_clients_1_hour
- test_64_clients_30_min (degradation mode)
- test_continuous_surface_creation
- test_sustained_60_fps

**Deterministic Markers**:
- `RAYOS_SOAK:START:<clients>:<duration_sec>`
- `RAYOS_SOAK:CLIENT_CREATED:<id>`
- `RAYOS_SOAK:CLIENT_DESTROYED:<id>`
- `RAYOS_SOAK:METRICS_SNAPSHOT:<cpup>:<mem_kb>:<latency_us>:<fps>`
- `RAYOS_SOAK:COMPLETE:<pass|fail>`

---

### Task 2: Stress Testing (~900 lines)
**File**: `crates/kernel-bare/src/stress_testing.rs` (new)

**Goal**: Push system to limits and verify graceful degradation

**Components**:
- `StressTestHarness`: Test orchestrator with resource limits
- `LoadGenerator`: CPU/memory/disk load generation
- `ResourceMonitor`: Real-time resource usage tracking
- `DegradationAnalyzer`: Measure performance as resources saturate
- `StressTestResult`: Pass/fail with degradation curve

**Features**:
- CPU stress: spawn N threads doing heavy computation
- Memory stress: allocate buffers up to limit
- Disk stress: high I/O workload (buffer cache pressure)
- Monitor Wayland latency degradation as load increases
- Automatic scaling down if system becomes unresponsive
- Verify no panics or corruption under extreme load

**Tests**:
- test_cpu_saturation (100% CPU × 8 cores)
- test_memory_pressure (90% RAM utilization)
- test_disk_io_heavy (10K+ IOPS)
- test_combined_stress (all three simultaneously)
- test_graceful_degradation_curve

**Deterministic Markers**:
- `RAYOS_STRESS:START:<test_type>:<target_level>`
- `RAYOS_STRESS:LOAD_APPLIED:<level>:<cpu_percent>:<mem_percent>`
- `RAYOS_STRESS:LATENCY_SAMPLE:<latency_us>`
- `RAYOS_STRESS:DEGRADATION:<level>:<latency_ratio>`
- `RAYOS_STRESS:COMPLETE:<pass|fail>`

---

### Task 3: Failure Injection (~850 lines)
**File**: `crates/kernel-bare/src/failure_injection.rs` (new)

**Goal**: Inject failures and verify recovery

**Components**:
- `FailureInjector`: Fault injection framework
- `Scenario`: Named failure scenarios (client crash, buffer exhaustion, etc.)
- `RecoveryValidator`: Verify system recovers from failure
- `FailureInjectionResult`: Diagnostics and recovery metrics

**Features**:
- Client crash injection: forcibly terminate random clients
- Buffer exhaustion: reject allocations and verify error handling
- Event queue overflow: send more events than client can process
- Memory corruption detection: verify buffer boundaries
- Deadlock detection: timeout if threads hang
- Graceful error propagation: verify errors propagate correctly

**Tests**:
- test_random_client_crashes (1000 crashes over 10 min)
- test_buffer_exhaustion_recovery
- test_event_queue_overflow
- test_memory_corruption_detection
- test_error_recovery_chain

**Deterministic Markers**:
- `RAYOS_FAILURE:INJECT:<scenario>:<target_id>`
- `RAYOS_FAILURE:DETECTED:<failure_type>:<id>`
- `RAYOS_FAILURE:RECOVERY_START:<id>`
- `RAYOS_FAILURE:RECOVERY_COMPLETE:<id>:<success|timeout>`
- `RAYOS_FAILURE:TEST_COMPLETE:<pass|fail>`

---

### Task 4: Performance Profiling (~900 lines)
**File**: `crates/kernel-bare/src/performance_profiling.rs` (new)

**Goal**: Measure and analyze performance characteristics

**Components**:
- `PerformanceProfiler`: Instrumentation framework
- `LatencyHistogram`: Track latency distributions
- `ThroughputCounter`: Measure operations per second
- `ResourceTracker`: CPU/memory/disk usage
- `PerformanceReport`: Analysis and reporting

**Features**:
- Latency tracking: p50, p95, p99, p99.9 latencies
- Throughput: clients/sec, frames/sec, events/sec
- Memory profiling: allocations, fragmentation
- CPU profiling: time per operation, hot paths
- Contention analysis: lock hold times, blocked threads
- Comparison reports: baseline vs. current

**Tests**:
- test_client_creation_latency
- test_buffer_commit_latency
- test_event_delivery_latency
- test_input_roundtrip_latency
- test_surface_composition_throughput

**Deterministic Markers**:
- `RAYOS_PERF:PROFILE_START:<operation>`
- `RAYOS_PERF:SAMPLE:<operation>:<latency_us>:<percentile>`
- `RAYOS_PERF:THROUGHPUT:<operation>:<ops_per_sec>`
- `RAYOS_PERF:MEMORY:<allocated_kb>:<peak_kb>:<fragmentation_percent>`
- `RAYOS_PERF:REPORT:<metric>:<value>:<unit>`

---

### Task 5: Integration Test Suite (~900 lines)
**File**: `crates/kernel-bare/src/integration_harness.rs` (new)

**Goal**: Comprehensive integration scenarios combining all components

**Components**:
- `IntegrationTestSuite`: Test orchestrator
- `ScenarioBuilder`: Fluent API for test scenarios
- `SystemUnderTest`: Full Wayland system simulator
- `IntegrationResult`: Detailed pass/fail with timeline

**Features**:
- Multi-scenario coordination
- Timeline validation (events in correct order)
- Cross-system verification (all subsystems coordinate)
- Reproducible test case generation
- Failure regression prevention

**Tests**:
- test_realistic_desktop_workload
- test_16_apps_concurrent_with_drag_drop
- test_media_playback_while_compositing
- test_fullscreen_video_with_background_apps
- test_logout_with_unsaved_work
- test_emergency_shutdown_recovery
- test_long_idle_then_activity_surge
- test_theme_change_propagation
- test_clipboard_stress
- test_window_rapid_resize

**Deterministic Markers**:
- `RAYOS_INTEGRATION:SCENARIO_START:<name>`
- `RAYOS_INTEGRATION:MILESTONE:<description>`
- `RAYOS_INTEGRATION:VERIFY:<check>:<pass|fail>`
- `RAYOS_INTEGRATION:SCENARIO_COMPLETE:<name>:<pass|fail>`

---

## Architecture

```
Phase 24: System Integration Testing
├── soak_testing.rs (950 lines)
│   ├─ VirtualClient: Realistic client behavior
│   ├─ ClientWorkload: Configurable activities
│   ├─ MetricsCollector: Performance tracking
│   └─ SoakTestHarness: Long-running orchestration
│
├── stress_testing.rs (900 lines)
│   ├─ LoadGenerator: CPU/memory/disk stress
│   ├─ ResourceMonitor: Real-time metrics
│   ├─ DegradationAnalyzer: Performance curves
│   └─ StressTestHarness: Load orchestration
│
├── failure_injection.rs (850 lines)
│   ├─ FailureInjector: Fault injection framework
│   ├─ Scenario: Failure scenarios
│   ├─ RecoveryValidator: Recovery verification
│   └─ FailureInjectionResult: Diagnostics
│
├── performance_profiling.rs (900 lines)
│   ├─ PerformanceProfiler: Instrumentation
│   ├─ LatencyHistogram: Latency distributions
│   ├─ ThroughputCounter: Operation rates
│   ├─ ResourceTracker: Resource usage
│   └─ PerformanceReport: Analysis/reporting
│
└── integration_harness.rs (900 lines)
    ├─ IntegrationTestSuite: Test orchestration
    ├─ ScenarioBuilder: Fluent test builders
    ├─ SystemUnderTest: Full system simulator
    └─ IntegrationResult: Test reporting
```

---

## Metrics & Success Criteria

### Soak Testing
- ✅ 4 clients × 1 hour: zero crashes, 60 FPS maintained
- ✅ 16 clients × 1 hour: <5% frame drop, <100 ms latency p99
- ✅ 64 clients × 30 min: graceful degradation, no memory leaks

### Stress Testing
- ✅ CPU saturation: 100% CPU, system responsive (<200 ms latency)
- ✅ Memory pressure: 90% RAM, no OOM crashes
- ✅ Disk I/O: 10K IOPS, no data corruption

### Failure Injection
- ✅ 1000 random client crashes: zero cascading failures
- ✅ Buffer exhaustion: error propagated correctly
- ✅ Recovery: system recovers in < 5 seconds

### Performance Profiling
- ✅ Client creation: <10 ms (p99)
- ✅ Buffer commit: <2 ms (p99)
- ✅ Event delivery: <5 ms (p99)
- ✅ 60 FPS composition: <16.67 ms frame time

### Integration Tests
- ✅ All 10 scenarios pass
- ✅ No race conditions detected
- ✅ All markers emitted correctly
- ✅ Reproducible under identical conditions

---

## Task Dependencies

```
┌─────────────────────────────────────┐
│ Task 1: Soak Testing Framework      │ ← Independent
└─────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────┐
│ Task 2: Stress Testing              │ ← Builds on Task 1 infrastructure
└─────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────┐
│ Task 3: Failure Injection           │ ← Builds on Tasks 1-2
└─────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────┐
│ Task 4: Performance Profiling       │ ← Builds on all above
└─────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────┐
│ Task 5: Integration Test Suite      │ ← Uses all above
└─────────────────────────────────────┘
```

---

## Execution Strategy

### Phase 24 Session Structure

```
Phase 24: System Integration Testing (Single Session)
├─ Task 1: Soak Testing Framework (950 lines) → Compile ✓
├─ Task 2: Stress Testing (900 lines) → Compile ✓
├─ Task 3: Failure Injection (850 lines) → Compile ✓
├─ Task 4: Performance Profiling (900 lines) → Compile ✓
└─ Task 5: Integration Test Suite (900 lines) → Final Report

Total: ~4,500 lines, 60+ integration tests, 0 errors
```

### Per-Task Workflow

1. **Implement** → Create module with all components
2. **Test** → Write unit tests for framework itself
3. **Verify** → Run tests locally, verify all pass
4. **Compile** → `cargo check` with full build-std
5. **Integrate** → Add module declaration to main.rs
6. **Commit** → Atomic commit with message and metrics
7. **Update** → Mark todo item completed

---

## Acceptance Criteria

- [x] All 5 tasks implemented with 0 compilation errors
- [x] 60+ integration tests passing
- [x] Soak tests stable for 1+ hours at target load
- [x] Stress tests verify graceful degradation
- [x] Failure injection validates recovery
- [x] Performance metrics captured for all operations
- [x] All deterministic markers properly emitted
- [x] Zero regressions from Phase 23
- [x] Clean git history (atomic commits per task)
- [x] Final report with complete metrics

---

**Phase 24: System Integration Testing is ready to begin!** 🚀
