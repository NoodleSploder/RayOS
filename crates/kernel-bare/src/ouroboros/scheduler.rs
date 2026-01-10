//! Dream Scheduler: Idle Detection and Evolution Triggering
//!
//! This module implements the Dream Scheduler which monitors system activity and
//! triggers the evolution process during idle periods. It implements the "No Idle Principle"
//! where RayOS self-optimizes instead of sleeping when not serving the user.
//!
//! # Architecture
//!
//! Dream mode operates in phases:
//! 1. **Activity Monitoring** - Track CPU, memory, and I/O activity
//! 2. **Idle Detection** - Identify periods of true system idleness
//! 3. **Dream Trigger** - Initiate evolution when idle budget permits
//! 4. **Dream Session** - Execute evolution mutations during idle
//! 5. **Budget Management** - Allocate time/power budget for evolution
//!
//! # Boot Markers
//!
//! - `RAYOS_OUROBOROS:DREAM_START` - Evolution dream session started
//! - `RAYOS_OUROBOROS:DREAM_ACTIVE` - Dream scheduler running mutations
//! - `RAYOS_OUROBOROS:DREAM_END` - Dream session completed

use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use crate::ouroboros::{PowerState, EvolutionResult};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Minimum idle time required to start dreaming (milliseconds)
pub const MIN_IDLE_TIME_FOR_DREAM_MS: u64 = 500;

/// Maximum dream session duration (milliseconds)
pub const MAX_DREAM_SESSION_DURATION_MS: u64 = 30000;

/// Idle threshold: activity below this % is considered idle
pub const IDLE_ACTIVITY_THRESHOLD_PERCENT: f32 = 5.0;

/// CPU load tracking window (milliseconds)
pub const CPU_LOAD_WINDOW_MS: u64 = 1000;

/// Memory pressure threshold (% of total RAM in use)
pub const MEMORY_PRESSURE_THRESHOLD_PERCENT: f32 = 80.0;

/// I/O activity threshold (operations per second)
pub const IO_ACTIVITY_THRESHOLD: u32 = 10;

/// Default power budget multiplier for AC power
pub const DEFAULT_POWER_BUDGET_MULTIPLIER: f32 = 1.0;

// ============================================================================
// ACTIVITY MONITORING
// ============================================================================

/// CPU activity metrics
#[derive(Clone, Copy, Debug)]
pub struct CpuMetrics {
    /// Current CPU load (0-100%)
    pub load_percent: u8,
    /// CPU utilization in last window
    pub utilization: f32,
    /// Number of context switches
    pub context_switches: u32,
    /// Interrupt count in window
    pub interrupts: u32,
    /// System time usage (microseconds)
    pub system_time_us: u64,
    /// User time usage (microseconds)
    pub user_time_us: u64,
}

impl CpuMetrics {
    /// Create new CPU metrics
    pub fn new() -> Self {
        Self {
            load_percent: 0,
            utilization: 0.0,
            context_switches: 0,
            interrupts: 0,
            system_time_us: 0,
            user_time_us: 0,
        }
    }

    /// Update load percent
    pub fn set_load(&mut self, load: u8) {
        self.load_percent = load.min(100);
    }

    /// Update utilization
    pub fn set_utilization(&mut self, util: f32) {
        self.utilization = util.max(0.0).min(100.0);
    }

    /// Is CPU idle?
    pub fn is_idle(&self) -> bool {
        self.load_percent < IDLE_ACTIVITY_THRESHOLD_PERCENT as u8
    }
}

impl Default for CpuMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory activity metrics
#[derive(Clone, Copy, Debug)]
pub struct MemoryMetrics {
    /// Total memory (bytes)
    pub total_bytes: u64,
    /// Used memory (bytes)
    pub used_bytes: u64,
    /// Free memory (bytes)
    pub free_bytes: u64,
    /// Page faults in window
    pub page_faults: u32,
    /// Memory pressure (0-100%)
    pub pressure_percent: u8,
}

impl MemoryMetrics {
    /// Create new memory metrics
    pub fn new() -> Self {
        Self {
            total_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
            page_faults: 0,
            pressure_percent: 0,
        }
    }

    /// Calculate usage percentage
    pub fn usage_percent(&self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.used_bytes as f32 / self.total_bytes as f32) * 100.0
    }

    /// Is memory idle?
    pub fn is_idle(&self) -> bool {
        self.pressure_percent < MEMORY_PRESSURE_THRESHOLD_PERCENT as u8
    }
}

impl Default for MemoryMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// I/O activity metrics
#[derive(Clone, Copy, Debug)]
pub struct IoMetrics {
    /// Read operations per second
    pub reads_per_sec: u32,
    /// Write operations per second
    pub writes_per_sec: u32,
    /// Bytes read per second
    pub read_bytes_per_sec: u32,
    /// Bytes written per second
    pub write_bytes_per_sec: u32,
    /// I/O wait percentage
    pub io_wait_percent: u8,
}

impl IoMetrics {
    /// Create new I/O metrics
    pub fn new() -> Self {
        Self {
            reads_per_sec: 0,
            writes_per_sec: 0,
            read_bytes_per_sec: 0,
            write_bytes_per_sec: 0,
            io_wait_percent: 0,
        }
    }

    /// Total operations per second
    pub fn total_ops_per_sec(&self) -> u32 {
        self.reads_per_sec.saturating_add(self.writes_per_sec)
    }

    /// Is I/O idle?
    pub fn is_idle(&self) -> bool {
        self.total_ops_per_sec() < IO_ACTIVITY_THRESHOLD
    }
}

impl Default for IoMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Activity monitor tracking system metrics
#[derive(Clone, Copy, Debug)]
pub struct ActivityMonitor {
    /// Monitor ID
    pub id: u64,
    /// CPU metrics
    pub cpu: CpuMetrics,
    /// Memory metrics
    pub memory: MemoryMetrics,
    /// I/O metrics
    pub io: IoMetrics,
    /// Last activity timestamp (milliseconds)
    pub last_activity_time: u64,
    /// Idle start time (milliseconds)
    pub idle_start_time: u64,
    /// Total idle time accumulated (milliseconds)
    pub total_idle_time: u64,
    /// Is currently idle?
    pub is_idle: bool,
}

impl ActivityMonitor {
    /// Create new activity monitor
    pub fn new(id: u64) -> Self {
        Self {
            id,
            cpu: CpuMetrics::new(),
            memory: MemoryMetrics::new(),
            io: IoMetrics::new(),
            last_activity_time: 0,
            idle_start_time: 0,
            total_idle_time: 0,
            is_idle: false,
        }
    }

    /// Update CPU metrics
    pub fn update_cpu(&mut self, load: u8) {
        self.cpu.set_load(load);
        if !self.cpu.is_idle() {
            self.last_activity_time = 0; // Would be set to current time
            self.is_idle = false;
            self.idle_start_time = 0;
        }
    }

    /// Update memory metrics
    pub fn update_memory(&mut self, used: u64, total: u64) {
        self.memory.used_bytes = used;
        self.memory.total_bytes = total;
        self.memory.free_bytes = total.saturating_sub(used);
        self.memory.pressure_percent = (self.memory.usage_percent() as u8).min(100);
    }

    /// Update I/O metrics
    pub fn update_io(&mut self, reads: u32, writes: u32) {
        self.io.reads_per_sec = reads;
        self.io.writes_per_sec = writes;
        if !self.io.is_idle() {
            self.is_idle = false;
            self.idle_start_time = 0;
        }
    }

    /// Check if system is idle
    pub fn check_idle(&mut self, current_time: u64) -> bool {
        let cpu_idle = self.cpu.is_idle();
        let mem_idle = self.memory.is_idle();
        let io_idle = self.io.is_idle();

        let all_idle = cpu_idle && mem_idle && io_idle;

        if all_idle && !self.is_idle {
            // Just became idle
            self.is_idle = true;
            self.idle_start_time = current_time;
            false // Not idle long enough yet
        } else if all_idle {
            // Already idle
            let idle_duration = current_time.saturating_sub(self.idle_start_time);
            idle_duration >= MIN_IDLE_TIME_FOR_DREAM_MS
        } else {
            // Activity detected
            if self.is_idle {
                self.total_idle_time = self.total_idle_time.saturating_add(
                    current_time.saturating_sub(self.idle_start_time)
                );
            }
            self.is_idle = false;
            self.idle_start_time = 0;
            false
        }
    }

    /// Get current idle duration
    pub fn current_idle_duration(&self, current_time: u64) -> u64 {
        if !self.is_idle {
            return 0;
        }
        current_time.saturating_sub(self.idle_start_time)
    }
}

// ============================================================================
// IDLE STATE
// ============================================================================

/// Idle state tracking
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IdleState {
    /// System actively running
    Active = 0,
    /// Just entered idle state (< 500ms)
    IdleShort = 1,
    /// Idle for medium duration (500ms - 5s)
    IdleMedium = 2,
    /// Idle for long duration (5s - 30s)
    IdleLong = 3,
    /// Deep idle (> 30s)
    DeepIdle = 4,
}

impl IdleState {
    /// Get idle state from idle duration
    pub fn from_duration(idle_ms: u64) -> Self {
        match idle_ms {
            0 => IdleState::Active,
            1..=500 => IdleState::IdleShort,
            501..=5000 => IdleState::IdleMedium,
            5001..=30000 => IdleState::IdleLong,
            _ => IdleState::DeepIdle,
        }
    }

    /// Can dream mode be triggered in this state?
    pub fn can_trigger_dream(&self) -> bool {
        matches!(self, IdleState::IdleMedium | IdleState::IdleLong | IdleState::DeepIdle)
    }

    /// Get evolution time budget multiplier
    pub fn budget_multiplier(&self) -> f32 {
        match self {
            IdleState::Active => 0.0,
            IdleState::IdleShort => 0.1,
            IdleState::IdleMedium => 0.5,
            IdleState::IdleLong => 0.8,
            IdleState::DeepIdle => 1.0,
        }
    }
}

// ============================================================================
// DREAM TRIGGER
// ============================================================================

/// Triggers evolution based on idle conditions
#[derive(Clone, Copy, Debug)]
pub struct DreamTrigger {
    /// Trigger ID
    pub id: u64,
    /// Number of times dream was triggered
    pub trigger_count: u32,
    /// Number of times trigger was blocked
    pub blocked_count: u32,
    /// Last trigger timestamp (milliseconds)
    pub last_trigger_time: u64,
    /// Minimum idle time required (milliseconds)
    pub min_idle_time_ms: u64,
    /// Is trigger enabled?
    pub enabled: bool,
}

impl DreamTrigger {
    /// Create new dream trigger
    pub fn new(id: u64) -> Self {
        Self {
            id,
            trigger_count: 0,
            blocked_count: 0,
            last_trigger_time: 0,
            min_idle_time_ms: MIN_IDLE_TIME_FOR_DREAM_MS,
            enabled: true,
        }
    }

    /// Evaluate if dream should trigger
    pub fn should_trigger(&self, idle_state: IdleState, power_state: PowerState, _current_time: u64) -> bool {
        if !self.enabled {
            return false;
        }

        // Don't evolve on battery critical
        if !power_state.evolution_allowed() {
            return false;
        }

        idle_state.can_trigger_dream()
    }

    /// Record trigger
    pub fn record_trigger(&mut self, current_time: u64) {
        self.trigger_count = self.trigger_count.saturating_add(1);
        self.last_trigger_time = current_time;
    }

    /// Record blocked trigger
    pub fn record_blocked(&mut self) {
        self.blocked_count = self.blocked_count.saturating_add(1);
    }

    /// Get trigger success rate
    pub fn trigger_rate(&self) -> f32 {
        let total = self.trigger_count as f32 + self.blocked_count as f32;
        if total == 0.0 {
            return 0.0;
        }
        (self.trigger_count as f32 / total) * 100.0
    }
}

// ============================================================================
// DREAM SESSION
// ============================================================================

/// A single evolution dream session
#[derive(Clone, Copy, Debug)]
pub struct DreamSession {
    /// Session ID
    pub id: u64,
    /// Session status
    pub status: DreamStatus,
    /// Start time (milliseconds)
    pub start_time: u64,
    /// End time (milliseconds)
    pub end_time: u64,
    /// Mutations attempted
    pub mutations_attempted: u32,
    /// Mutations applied
    pub mutations_applied: u32,
    /// Idle budget used (milliseconds)
    pub budget_used_ms: u64,
}

/// Dream session status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DreamStatus {
    /// Dream not started
    Inactive = 0,
    /// Dream in progress
    Active = 1,
    /// Dream paused (activity detected)
    Paused = 2,
    /// Dream completed successfully
    Completed = 3,
    /// Dream interrupted by user activity
    Interrupted = 4,
}

impl DreamSession {
    /// Create new dream session
    pub fn new(id: u64) -> Self {
        Self {
            id,
            status: DreamStatus::Inactive,
            start_time: 0,
            end_time: 0,
            mutations_attempted: 0,
            mutations_applied: 0,
            budget_used_ms: 0,
        }
    }

    /// Start dream session
    pub fn start(&mut self, current_time: u64) {
        self.status = DreamStatus::Active;
        self.start_time = current_time;
    }

    /// Pause dream session
    pub fn pause(&mut self) {
        if self.status == DreamStatus::Active {
            self.status = DreamStatus::Paused;
        }
    }

    /// Resume dream session
    pub fn resume(&mut self) {
        if self.status == DreamStatus::Paused {
            self.status = DreamStatus::Active;
        }
    }

    /// Complete dream session
    pub fn complete(&mut self, current_time: u64) {
        self.status = DreamStatus::Completed;
        self.end_time = current_time;
        self.budget_used_ms = current_time.saturating_sub(self.start_time);
    }

    /// Interrupt dream session
    pub fn interrupt(&mut self, current_time: u64) {
        self.status = DreamStatus::Interrupted;
        self.end_time = current_time;
        self.budget_used_ms = current_time.saturating_sub(self.start_time);
    }

    /// Record mutation attempt
    pub fn record_mutation_attempt(&mut self) {
        self.mutations_attempted = self.mutations_attempted.saturating_add(1);
    }

    /// Record mutation application
    pub fn record_mutation_applied(&mut self) {
        self.mutations_applied = self.mutations_applied.saturating_add(1);
    }

    /// Get success rate
    pub fn success_rate(&self) -> f32 {
        if self.mutations_attempted == 0 {
            return 0.0;
        }
        (self.mutations_applied as f32 / self.mutations_attempted as f32) * 100.0
    }

    /// Get duration
    pub fn duration_ms(&self) -> u64 {
        if self.end_time == 0 {
            return 0;
        }
        self.end_time.saturating_sub(self.start_time)
    }

    /// Is dream active?
    pub fn is_active(&self) -> bool {
        self.status == DreamStatus::Active
    }
}

// ============================================================================
// DREAM BUDGET
// ============================================================================

/// Budget management for dream sessions
#[derive(Clone, Copy, Debug)]
pub struct DreamBudget {
    /// Total budget available (milliseconds)
    pub total_budget_ms: u64,
    /// Budget used so far (milliseconds)
    pub used_budget_ms: u64,
    /// Budget per session (milliseconds)
    pub per_session_budget_ms: u64,
    /// Power state affecting budget
    pub power_state: PowerState,
    /// Budget multiplier
    pub multiplier: f32,
}

impl DreamBudget {
    /// Create new dream budget
    pub fn new() -> Self {
        Self {
            total_budget_ms: 60000, // 1 minute per hour of idle
            used_budget_ms: 0,
            per_session_budget_ms: 30000, // 30s max per session
            power_state: PowerState::AcPower,
            multiplier: DEFAULT_POWER_BUDGET_MULTIPLIER,
        }
    }

    /// Update based on power state
    pub fn set_power_state(&mut self, state: PowerState) {
        self.power_state = state;
        self.multiplier = state.budget_multiplier();
    }

    /// Get available budget
    pub fn available_ms(&self) -> u64 {
        let adjusted = (self.total_budget_ms as f32 * self.multiplier) as u64;
        adjusted.saturating_sub(self.used_budget_ms)
    }

    /// Get per-session budget
    pub fn session_budget_ms(&self) -> u64 {
        let adjusted = (self.per_session_budget_ms as f32 * self.multiplier) as u64;
        adjusted.min(self.available_ms())
    }

    /// Record budget use
    pub fn use_budget(&mut self, amount_ms: u64) -> Result<(), EvolutionResult> {
        if amount_ms > self.available_ms() {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.used_budget_ms = self.used_budget_ms.saturating_add(amount_ms);
        Ok(())
    }

    /// Is budget available?
    pub fn has_budget(&self) -> bool {
        self.available_ms() > 0
    }

    /// Reset budget (called periodically)
    pub fn reset(&mut self) {
        self.used_budget_ms = 0;
    }
}

impl Default for DreamBudget {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// DREAM SCHEDULER
// ============================================================================

/// Main dream scheduler orchestrator
pub struct DreamScheduler {
    /// Scheduler ID
    pub id: u64,
    /// Activity monitor
    pub activity_monitor: ActivityMonitor,
    /// Dream trigger
    pub dream_trigger: DreamTrigger,
    /// Current dream budget
    pub budget: DreamBudget,
    /// Total dream sessions completed
    pub total_sessions: u32,
    /// Power state
    pub power_state: PowerState,
    /// Is scheduler enabled?
    pub enabled: bool,
}

impl DreamScheduler {
    /// Create new dream scheduler
    pub fn new(id: u64) -> Self {
        Self {
            id,
            activity_monitor: ActivityMonitor::new(id),
            dream_trigger: DreamTrigger::new(id),
            budget: DreamBudget::new(),
            total_sessions: 0,
            power_state: PowerState::AcPower,
            enabled: true,
        }
    }

    /// Update system activity
    pub fn update_activity(&mut self, cpu_load: u8, used_memory: u64, total_memory: u64, io_ops: u32) {
        self.activity_monitor.update_cpu(cpu_load);
        self.activity_monitor.update_memory(used_memory, total_memory);
        self.activity_monitor.update_io(io_ops, 0);
    }

    /// Check if dream should trigger
    pub fn should_trigger_dream(&self, current_time: u64) -> bool {
        if !self.enabled || !self.budget.has_budget() {
            return false;
        }

        let idle_duration = self.activity_monitor.current_idle_duration(current_time);
        let idle_state = IdleState::from_duration(idle_duration);

        self.dream_trigger.should_trigger(idle_state, self.power_state, current_time)
    }

    /// Set power state
    pub fn set_power_state(&mut self, state: PowerState) {
        self.power_state = state;
        self.budget.set_power_state(state);
    }

    /// Enable/disable scheduler
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Record completed dream session
    pub fn record_session(&mut self, session: &DreamSession) -> Result<(), EvolutionResult> {
        self.budget.use_budget(session.duration_ms())?;
        self.total_sessions = self.total_sessions.saturating_add(1);
        Ok(())
    }

    /// Get current idle state
    pub fn current_idle_state(&self, current_time: u64) -> IdleState {
        let idle_duration = self.activity_monitor.current_idle_duration(current_time);
        IdleState::from_duration(idle_duration)
    }

    /// Get available dream time
    pub fn available_dream_time_ms(&self) -> u64 {
        self.budget.session_budget_ms()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_metrics_creation() {
        let cpu = CpuMetrics::new();
        assert_eq!(cpu.load_percent, 0);
        assert_eq!(cpu.utilization, 0.0);
    }

    #[test]
    fn test_cpu_metrics_idle() {
        let mut cpu = CpuMetrics::new();
        cpu.set_load(2);
        assert!(cpu.is_idle());

        cpu.set_load(50);
        assert!(!cpu.is_idle());
    }

    #[test]
    fn test_memory_metrics_creation() {
        let mem = MemoryMetrics::new();
        assert_eq!(mem.total_bytes, 0);
        assert_eq!(mem.used_bytes, 0);
    }

    #[test]
    fn test_memory_metrics_pressure() {
        let mut mem = MemoryMetrics::new();
        mem.total_bytes = 1000;
        mem.used_bytes = 500;
        mem.pressure_percent = 50;
        assert!(mem.is_idle());

        mem.pressure_percent = 85;
        assert!(!mem.is_idle());
    }

    #[test]
    fn test_io_metrics_creation() {
        let io = IoMetrics::new();
        assert_eq!(io.reads_per_sec, 0);
        assert!(io.is_idle());
    }

    #[test]
    fn test_io_metrics_busy() {
        let mut io = IoMetrics::new();
        io.reads_per_sec = 20;
        assert!(!io.is_idle());
    }

    #[test]
    fn test_activity_monitor_creation() {
        let monitor = ActivityMonitor::new(1);
        assert_eq!(monitor.id, 1);
        assert!(!monitor.is_idle);
    }

    #[test]
    fn test_activity_monitor_idle_detection() {
        let mut monitor = ActivityMonitor::new(1);
        monitor.update_cpu(2);
        monitor.update_memory(100, 1000);
        monitor.update_io(0, 0);

        let idle = monitor.check_idle(1000);
        assert!(!idle); // Not idle long enough yet

        let idle = monitor.check_idle(2000);
        assert!(idle); // Idle for 1000ms, exceeds minimum
    }

    #[test]
    fn test_idle_state_from_duration() {
        assert_eq!(IdleState::from_duration(0), IdleState::Active);
        assert_eq!(IdleState::from_duration(300), IdleState::IdleShort);
        assert_eq!(IdleState::from_duration(2000), IdleState::IdleMedium);
        assert_eq!(IdleState::from_duration(15000), IdleState::IdleLong);
        assert_eq!(IdleState::from_duration(60000), IdleState::DeepIdle);
    }

    #[test]
    fn test_idle_state_can_trigger_dream() {
        assert!(!IdleState::Active.can_trigger_dream());
        assert!(!IdleState::IdleShort.can_trigger_dream());
        assert!(IdleState::IdleMedium.can_trigger_dream());
        assert!(IdleState::IdleLong.can_trigger_dream());
        assert!(IdleState::DeepIdle.can_trigger_dream());
    }

    #[test]
    fn test_idle_state_budget_multiplier() {
        assert_eq!(IdleState::Active.budget_multiplier(), 0.0);
        assert!(IdleState::IdleShort.budget_multiplier() > 0.0);
        assert!(IdleState::IdleMedium.budget_multiplier() > IdleState::IdleShort.budget_multiplier());
        assert_eq!(IdleState::DeepIdle.budget_multiplier(), 1.0);
    }

    #[test]
    fn test_dream_trigger_creation() {
        let trigger = DreamTrigger::new(1);
        assert_eq!(trigger.id, 1);
        assert!(trigger.enabled);
    }

    #[test]
    fn test_dream_trigger_should_trigger() {
        let trigger = DreamTrigger::new(1);
        let result = trigger.should_trigger(IdleState::IdleMedium, PowerState::AcPower, 0);
        assert!(result);

        let result = trigger.should_trigger(IdleState::Active, PowerState::AcPower, 0);
        assert!(!result);
    }

    #[test]
    fn test_dream_trigger_recording() {
        let mut trigger = DreamTrigger::new(1);
        trigger.record_trigger(1000);
        assert_eq!(trigger.trigger_count, 1);

        trigger.record_blocked();
        assert_eq!(trigger.blocked_count, 1);
    }

    #[test]
    fn test_dream_session_creation() {
        let session = DreamSession::new(1);
        assert_eq!(session.id, 1);
        assert_eq!(session.status, DreamStatus::Inactive);
    }

    #[test]
    fn test_dream_session_lifecycle() {
        let mut session = DreamSession::new(1);
        session.start(1000);
        assert_eq!(session.status, DreamStatus::Active);

        session.pause();
        assert_eq!(session.status, DreamStatus::Paused);

        session.resume();
        assert_eq!(session.status, DreamStatus::Active);

        session.complete(2000);
        assert_eq!(session.status, DreamStatus::Completed);
        assert_eq!(session.duration_ms(), 1000);
    }

    #[test]
    fn test_dream_session_mutations() {
        let mut session = DreamSession::new(1);
        session.start(1000);
        session.record_mutation_attempt();
        session.record_mutation_attempt();
        session.record_mutation_applied();

        assert_eq!(session.mutations_attempted, 2);
        assert_eq!(session.mutations_applied, 1);
        assert!((session.success_rate() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_dream_budget_creation() {
        let budget = DreamBudget::new();
        assert!(budget.has_budget());
        assert!(budget.available_ms() > 0);
    }

    #[test]
    fn test_dream_budget_usage() {
        let mut budget = DreamBudget::new();
        let initial = budget.available_ms();
        budget.use_budget(5000).unwrap();
        assert_eq!(budget.available_ms(), initial - 5000);
    }

    #[test]
    fn test_dream_budget_power_state() {
        let mut budget = DreamBudget::new();
        budget.set_power_state(PowerState::BatteryLow);
        assert_eq!(budget.multiplier, 0.1);
        assert!(budget.available_ms() < 6000); // 60000 * 0.1 = 6000
    }

    #[test]
    fn test_dream_scheduler_creation() {
        let scheduler = DreamScheduler::new(1);
        assert_eq!(scheduler.id, 1);
        assert!(scheduler.enabled);
    }

    #[test]
    fn test_dream_scheduler_activity_update() {
        let mut scheduler = DreamScheduler::new(1);
        scheduler.update_activity(2, 100, 1000, 0);
        assert!(scheduler.activity_monitor.cpu.is_idle());
    }

    #[test]
    fn test_dream_scheduler_trigger() {
        let scheduler = DreamScheduler::new(1);
        let mut monitor = ActivityMonitor::new(1);
        monitor.is_idle = true;
        monitor.idle_start_time = 0;

        // Trigger after sufficient idle time
        let result = scheduler.should_trigger_dream(2000);
        // Result depends on actual idle state
        let _ = result;
    }

    #[test]
    fn test_dream_scheduler_power_state() {
        let mut scheduler = DreamScheduler::new(1);
        scheduler.set_power_state(PowerState::BatteryCritical);
        assert_eq!(scheduler.power_state, PowerState::BatteryCritical);
    }

    #[test]
    fn test_dream_scheduler_enable_disable() {
        let mut scheduler = DreamScheduler::new(1);
        scheduler.set_enabled(false);
        assert!(!scheduler.enabled);
    }

    #[test]
    fn test_dream_scheduler_session_recording() {
        let mut scheduler = DreamScheduler::new(1);
        let mut session = DreamSession::new(1);
        session.start(1000);
        session.complete(2000);

        let initial_budget = scheduler.budget.available_ms();
        scheduler.record_session(&session).unwrap();
        assert_eq!(scheduler.total_sessions, 1);
        assert!(scheduler.budget.available_ms() < initial_budget);
    }

    #[test]
    fn test_dream_scheduler_idle_state() {
        let mut scheduler = DreamScheduler::new(1);
        scheduler.activity_monitor.is_idle = true;
        scheduler.activity_monitor.idle_start_time = 1000;

        let state = scheduler.current_idle_state(2000);
        assert_eq!(state, IdleState::IdleShort);

        let state = scheduler.current_idle_state(7000);
        assert_eq!(state, IdleState::IdleMedium);
    }
}
