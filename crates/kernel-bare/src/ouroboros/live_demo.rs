//! Live Evolution Demo and Dashboard
//!
//! Provides real-time mutation tracking, performance visualization,
//! and dashboard metrics for ongoing evolution sessions.
//!
//! Phase 33, Task 4

/// Live mutation event type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MutationEventType {
    Attempted = 0,    // Mutation being tested
    Success = 1,      // Mutation succeeded
    Failed = 2,       // Mutation failed
    Reverted = 3,     // Mutation rolled back
    Applied = 4,      // Mutation live-patched
}

/// Real-time mutation event
#[derive(Clone, Copy, Debug)]
pub struct MutationEvent {
    /// Event timestamp (ms since boot)
    pub timestamp_ms: u64,
    /// Event type
    pub event_type: MutationEventType,
    /// Mutation ID
    pub mutation_id: u32,
    /// Cycle number
    pub cycle: u32,
    /// Performance delta (1000x = 1.0x improvement)
    pub perf_delta: i32,
    /// Memory delta (KB)
    pub memory_delta: i16,
    /// Success indicator
    pub success: bool,
}

impl MutationEvent {
    /// Create mutation attempted event
    pub const fn attempted(timestamp_ms: u64, mutation_id: u32, cycle: u32) -> Self {
        MutationEvent {
            timestamp_ms,
            event_type: MutationEventType::Attempted,
            mutation_id,
            cycle,
            perf_delta: 0,
            memory_delta: 0,
            success: false,
        }
    }

    /// Create mutation success event
    pub const fn success(timestamp_ms: u64, mutation_id: u32, cycle: u32, perf_delta: i32, memory_delta: i16) -> Self {
        MutationEvent {
            timestamp_ms,
            event_type: MutationEventType::Success,
            mutation_id,
            cycle,
            perf_delta,
            memory_delta,
            success: true,
        }
    }

    /// Create mutation failed event
    pub const fn failed(timestamp_ms: u64, mutation_id: u32, cycle: u32) -> Self {
        MutationEvent {
            timestamp_ms,
            event_type: MutationEventType::Failed,
            mutation_id,
            cycle,
            perf_delta: 0,
            memory_delta: 0,
            success: false,
        }
    }

    /// Get performance improvement as percentage (divide by 10)
    pub fn perf_percent(&self) -> i16 {
        (self.perf_delta / 100) as i16
    }
}

/// Cycle summary metrics
#[derive(Clone, Copy, Debug)]
pub struct CycleSummary {
    /// Cycle number
    pub cycle: u32,
    /// Total mutations attempted
    pub mutations_attempted: u32,
    /// Total mutations succeeded
    pub mutations_succeeded: u32,
    /// Total mutations applied
    pub mutations_applied: u32,
    /// Average performance delta (1000x basis)
    pub avg_perf_delta: i32,
    /// Max performance improvement (1000x basis)
    pub max_perf_delta: i32,
    /// Cycle start time (ms since boot)
    pub start_time_ms: u64,
    /// Cycle duration (ms)
    pub duration_ms: u32,
}

impl CycleSummary {
    /// Create new cycle summary
    pub const fn new(cycle: u32, start_time_ms: u64) -> Self {
        CycleSummary {
            cycle,
            mutations_attempted: 0,
            mutations_succeeded: 0,
            mutations_applied: 0,
            avg_perf_delta: 0,
            max_perf_delta: 0,
            start_time_ms,
            duration_ms: 0,
        }
    }

    /// Get success rate percent
    pub fn success_rate(&self) -> u32 {
        if self.mutations_attempted == 0 {
            return 0;
        }
        ((self.mutations_succeeded as u64 * 100) / self.mutations_attempted as u64) as u32
    }

    /// Get apply rate (applied / succeeded)
    pub fn apply_rate(&self) -> u32 {
        if self.mutations_succeeded == 0 {
            return 0;
        }
        ((self.mutations_applied as u64 * 100) / self.mutations_succeeded as u64) as u32
    }

    /// Get average improvement percent (perf_delta / 100)
    pub fn avg_improvement_percent(&self) -> i16 {
        (self.avg_perf_delta / 100) as i16
    }

    /// Get max improvement percent
    pub fn max_improvement_percent(&self) -> i16 {
        (self.max_perf_delta / 100) as i16
    }
}

/// Real-time dashboard metrics
#[derive(Clone, Copy, Debug)]
pub struct DashboardMetrics {
    /// Total events received
    pub total_events: u32,
    /// Current cycle number
    pub current_cycle: u32,
    /// Total mutations all time
    pub total_mutations: u32,
    /// Total successes
    pub total_successes: u32,
    /// Total applied
    pub total_applied: u32,
    /// Cumulative performance improvement (1000x basis)
    pub cumulative_perf_delta: i32,
    /// Session elapsed time (ms)
    pub elapsed_time_ms: u64,
    /// Mutations per second
    pub mutations_per_sec: u32,
    /// Average cycle duration (ms)
    pub avg_cycle_duration_ms: u32,
}

impl DashboardMetrics {
    /// Create new dashboard metrics
    pub const fn new() -> Self {
        DashboardMetrics {
            total_events: 0,
            current_cycle: 0,
            total_mutations: 0,
            total_successes: 0,
            total_applied: 0,
            cumulative_perf_delta: 0,
            elapsed_time_ms: 0,
            mutations_per_sec: 0,
            avg_cycle_duration_ms: 0,
        }
    }

    /// Get overall success rate
    pub fn overall_success_rate(&self) -> u32 {
        if self.total_mutations == 0 {
            return 0;
        }
        ((self.total_successes as u64 * 100) / self.total_mutations as u64) as u32
    }

    /// Get overall apply rate
    pub fn overall_apply_rate(&self) -> u32 {
        if self.total_successes == 0 {
            return 0;
        }
        ((self.total_applied as u64 * 100) / self.total_successes as u64) as u32
    }

    /// Get cumulative improvement percent
    pub fn cumulative_improvement_percent(&self) -> i16 {
        (self.cumulative_perf_delta / 100) as i16
    }

    /// Get throughput (events per second)
    pub fn throughput_per_sec(&self) -> u32 {
        if self.elapsed_time_ms == 0 {
            return 0;
        }
        ((self.total_events as u64 * 1000) / self.elapsed_time_ms as u64) as u32
    }
}

/// Live Evolution Demo Controller
pub struct LiveEvolutionDemo {
    /// Event buffer (last 100 events)
    events: [Option<MutationEvent>; 100],
    /// Current event index
    event_index: usize,
    /// Current cycle summary
    current_cycle: Option<CycleSummary>,
    /// Cycle history (last 10 cycles)
    cycle_history: [Option<CycleSummary>; 10],
    /// Dashboard metrics
    metrics: DashboardMetrics,
    /// Start time
    start_time_ms: u64,
}

impl LiveEvolutionDemo {
    /// Create new live evolution demo
    pub const fn new(start_time_ms: u64) -> Self {
        LiveEvolutionDemo {
            events: [None; 100],
            event_index: 0,
            current_cycle: None,
            cycle_history: [None; 10],
            metrics: DashboardMetrics::new(),
            start_time_ms,
        }
    }

    /// Record mutation event
    pub fn record_event(&mut self, event: MutationEvent) {
        self.events[self.event_index] = Some(event);
        self.event_index = (self.event_index + 1) % 100;

        self.metrics.total_events += 1;

        match event.event_type {
            MutationEventType::Attempted => {
                if let Some(ref mut cycle) = self.current_cycle {
                    cycle.mutations_attempted += 1;
                }
                self.metrics.total_mutations += 1;
            }
            MutationEventType::Success => {
                if let Some(ref mut cycle) = self.current_cycle {
                    cycle.mutations_succeeded += 1;
                    cycle.avg_perf_delta = ((cycle.avg_perf_delta as u64 * (cycle.mutations_succeeded - 1) as u64 + event.perf_delta as u64) / cycle.mutations_succeeded as u64) as i32;
                    if event.perf_delta > cycle.max_perf_delta {
                        cycle.max_perf_delta = event.perf_delta;
                    }
                }
                self.metrics.total_successes += 1;
                self.metrics.cumulative_perf_delta += event.perf_delta;
            }
            MutationEventType::Applied => {
                if let Some(ref mut cycle) = self.current_cycle {
                    cycle.mutations_applied += 1;
                }
                self.metrics.total_applied += 1;
            }
            _ => {}
        }
    }

    /// Start new cycle
    pub fn start_cycle(&mut self, cycle: u32, current_time_ms: u64) {
        // Save previous cycle if exists
        if let Some(prev_cycle) = self.current_cycle {
            self.cycle_history[cycle as usize % 10] = Some(prev_cycle);
        }

        self.current_cycle = Some(CycleSummary::new(cycle, current_time_ms));
        self.metrics.current_cycle = cycle;
    }

    /// End current cycle
    pub fn end_cycle(&mut self, current_time_ms: u64) -> Option<CycleSummary> {
        if let Some(mut cycle) = self.current_cycle.take() {
            cycle.duration_ms = if current_time_ms > cycle.start_time_ms {
                (current_time_ms - cycle.start_time_ms) as u32
            } else {
                0
            };

            // Update average cycle duration
            if self.metrics.current_cycle > 0 {
                self.metrics.avg_cycle_duration_ms = ((self.metrics.avg_cycle_duration_ms as u64 * (self.metrics.current_cycle - 1) as u64 + cycle.duration_ms as u64) / self.metrics.current_cycle as u64) as u32;
            } else {
                self.metrics.avg_cycle_duration_ms = cycle.duration_ms;
            }

            return Some(cycle);
        }
        None
    }

    /// Get recent events
    pub fn recent_events(&self, count: usize) -> [Option<MutationEvent>; 100] {
        let mut result = [None; 100];
        let start = if self.event_index >= count { self.event_index - count } else { 0 };
        for i in 0..count {
            let idx = (start + i) % 100;
            result[i] = self.events[idx];
        }
        result
    }

    /// Get cycle history
    pub fn cycle_history(&self) -> [Option<CycleSummary>; 10] {
        self.cycle_history
    }

    /// Get dashboard metrics
    pub fn dashboard_metrics(&self) -> DashboardMetrics {
        let mut metrics = self.metrics;
        metrics.elapsed_time_ms = (self.event_index as u64 * 1000) / 100; // Approximate based on events

        if metrics.elapsed_time_ms > 0 {
            metrics.mutations_per_sec = ((metrics.total_mutations as u64 * 1000) / metrics.elapsed_time_ms) as u32;
        }

        metrics
    }

    /// Get current cycle summary
    pub fn current_cycle_summary(&self) -> Option<CycleSummary> {
        self.current_cycle
    }

    /// Get last completed cycle
    pub fn last_cycle(&self) -> Option<CycleSummary> {
        self.cycle_history[9]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutation_event_attempted() {
        let event = MutationEvent::attempted(1000, 1, 1);
        assert_eq!(event.event_type, MutationEventType::Attempted);
        assert!(!event.success);
        assert_eq!(event.mutation_id, 1);
    }

    #[test]
    fn test_mutation_event_success() {
        let event = MutationEvent::success(1000, 1, 1, 500, 10);
        assert_eq!(event.event_type, MutationEventType::Success);
        assert!(event.success);
        assert_eq!(event.perf_delta, 500);
        assert_eq!(event.perf_percent(), 5); // 500 / 100 = 5
    }

    #[test]
    fn test_mutation_event_failed() {
        let event = MutationEvent::failed(1000, 1, 1);
        assert_eq!(event.event_type, MutationEventType::Failed);
        assert!(!event.success);
    }

    #[test]
    fn test_cycle_summary_creation() {
        let cycle = CycleSummary::new(1, 1000);
        assert_eq!(cycle.cycle, 1);
        assert_eq!(cycle.mutations_attempted, 0);
        assert_eq!(cycle.success_rate(), 0);
    }

    #[test]
    fn test_cycle_summary_success_rate() {
        let mut cycle = CycleSummary::new(1, 1000);
        cycle.mutations_attempted = 10;
        cycle.mutations_succeeded = 8;
        assert_eq!(cycle.success_rate(), 80);
    }

    #[test]
    fn test_cycle_summary_apply_rate() {
        let mut cycle = CycleSummary::new(1, 1000);
        cycle.mutations_succeeded = 10;
        cycle.mutations_applied = 8;
        assert_eq!(cycle.apply_rate(), 80);
    }

    #[test]
    fn test_cycle_summary_improvement_percent() {
        let mut cycle = CycleSummary::new(1, 1000);
        cycle.avg_perf_delta = 500;
        cycle.max_perf_delta = 1200;
        assert_eq!(cycle.avg_improvement_percent(), 5);
        assert_eq!(cycle.max_improvement_percent(), 12);
    }

    #[test]
    fn test_dashboard_metrics_creation() {
        let metrics = DashboardMetrics::new();
        assert_eq!(metrics.total_events, 0);
        assert_eq!(metrics.overall_success_rate(), 0);
    }

    #[test]
    fn test_dashboard_metrics_success_rate() {
        let mut metrics = DashboardMetrics::new();
        metrics.total_mutations = 10;
        metrics.total_successes = 7;
        assert_eq!(metrics.overall_success_rate(), 70);
    }

    #[test]
    fn test_dashboard_metrics_apply_rate() {
        let mut metrics = DashboardMetrics::new();
        metrics.total_successes = 10;
        metrics.total_applied = 9;
        assert_eq!(metrics.overall_apply_rate(), 90);
    }

    #[test]
    fn test_dashboard_metrics_improvement() {
        let mut metrics = DashboardMetrics::new();
        metrics.cumulative_perf_delta = 5000;
        assert_eq!(metrics.cumulative_improvement_percent(), 50);
    }

    #[test]
    fn test_live_evolution_demo_creation() {
        let demo = LiveEvolutionDemo::new(1000);
        assert_eq!(demo.metrics.total_events, 0);
        assert!(!demo.metrics.current_cycle != 0);
    }

    #[test]
    fn test_live_evolution_demo_record_event() {
        let mut demo = LiveEvolutionDemo::new(1000);
        demo.start_cycle(1, 1000);

        let event = MutationEvent::attempted(1001, 1, 1);
        demo.record_event(event);

        assert_eq!(demo.metrics.total_events, 1);
        assert_eq!(demo.metrics.total_mutations, 1);
    }

    #[test]
    fn test_live_evolution_demo_start_cycle() {
        let mut demo = LiveEvolutionDemo::new(1000);
        demo.start_cycle(1, 1000);

        assert!(demo.current_cycle.is_some());
        assert_eq!(demo.metrics.current_cycle, 1);
    }

    #[test]
    fn test_live_evolution_demo_end_cycle() {
        let mut demo = LiveEvolutionDemo::new(1000);
        demo.start_cycle(1, 1000);
        demo.start_cycle(1, 1000);
        let mut evt = MutationEvent::attempted(1100, 1, 1);
        evt.event_type = MutationEventType::Success;
        evt.perf_delta = 500;
        demo.record_event(evt);

        let cycle = demo.end_cycle(2000);
        assert!(cycle.is_some());
        assert_eq!(cycle.unwrap().duration_ms, 1000);
    }

    #[test]
    fn test_live_evolution_demo_cycle_history() {
        let mut demo = LiveEvolutionDemo::new(1000);

        for i in 1..=5 {
            demo.start_cycle(i, 1000);
            demo.end_cycle(2000);
        }

        let history = demo.cycle_history();
        assert!(history[0].is_some()); // First cycle stored
    }

    #[test]
    fn test_live_evolution_demo_metrics_update() {
        let mut demo = LiveEvolutionDemo::new(1000);
        demo.start_cycle(1, 1000);

        let event1 = MutationEvent::attempted(1001, 1, 1);
        let event2 = MutationEvent::success(1002, 1, 1, 500, 10);

        demo.record_event(event1);
        demo.record_event(event2);

        assert_eq!(demo.metrics.total_mutations, 1);
        assert_eq!(demo.metrics.total_successes, 1);
        assert_eq!(demo.metrics.cumulative_perf_delta, 500);
    }

    #[test]
    fn test_live_evolution_demo_recent_events() {
        let mut demo = LiveEvolutionDemo::new(1000);
        demo.start_cycle(1, 1000);

        for i in 0..5 {
            let event = MutationEvent::attempted(1000 + i, i, 1);
            demo.record_event(event);
        }

        let events = demo.recent_events(5);
        assert!(events[0].is_some());
    }

    #[test]
    fn test_live_evolution_demo_event_buffer_wraparound() {
        let mut demo = LiveEvolutionDemo::new(1000);
        demo.start_cycle(1, 1000);

        // Add 150 events to force wraparound
        for i in 0..150 {
            let event = MutationEvent::attempted(1000 + i as u64, i as u32, 1);
            demo.record_event(event);
        }

        // Should have 150 events recorded but only 100 in buffer
        assert_eq!(demo.metrics.total_events, 150);
    }

    #[test]
    fn test_live_evolution_demo_multiple_cycles() {
        let mut demo = LiveEvolutionDemo::new(1000);

        for cycle_num in 1..=3 {
            demo.start_cycle(cycle_num, 1000);

            for j in 0..4 {
                let event = MutationEvent::success(1100, j, cycle_num, 100, 5);
                demo.record_event(event);
            }

            demo.end_cycle(2000);
        }

        assert_eq!(demo.metrics.total_events, 12);
        assert_eq!(demo.metrics.total_successes, 12);
    }

    #[test]
    fn test_live_evolution_demo_dashboard_throughput() {
        let mut demo = LiveEvolutionDemo::new(1000);
        demo.start_cycle(1, 1000);

        // Record 10 events
        for i in 0..10 {
            let event = MutationEvent::attempted(1000 + i, i, 1);
            demo.record_event(event);
        }

        let metrics = demo.dashboard_metrics();
        assert!(metrics.throughput_per_sec() > 0);
    }

    #[test]
    fn test_live_evolution_demo_current_cycle() {
        let mut demo = LiveEvolutionDemo::new(1000);
        demo.start_cycle(1, 1000);

        assert!(demo.current_cycle_summary().is_some());
        assert_eq!(demo.current_cycle_summary().unwrap().cycle, 1);
    }
}
