//! Kernel Integration for Ouroboros Engine
//!
//! Hooks the self-evolving Ouroboros Engine into the RayOS kernel boot sequence,
//! idle detection, and scheduler. Enables autonomous evolution during idle periods.
//!
//! Phase 33, Task 1

use core::mem;

/// Evolution budget for a dream session
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionBudget {
    /// Total CPU time budget (ms)
    pub total_time_ms: u32,
    /// Memory budget (KB)
    pub memory_budget_kb: u32,
    /// Maximum mutations per cycle
    pub max_mutations_per_cycle: u32,
    /// Minimum improvement required (scaled by 100)
    pub min_improvement_threshold: u32,
}

impl EvolutionBudget {
    /// Create default budget (5 minutes, 100MB)
    pub const fn default() -> Self {
        EvolutionBudget {
            total_time_ms: 5 * 60 * 1000, // 5 minutes
            memory_budget_kb: 100 * 1024, // 100 MB
            max_mutations_per_cycle: 16,
            min_improvement_threshold: 100, // 1% improvement
        }
    }

    /// Create budget for quick evolution (30 seconds)
    pub const fn quick() -> Self {
        EvolutionBudget {
            total_time_ms: 30 * 1000,
            memory_budget_kb: 50 * 1024,
            max_mutations_per_cycle: 8,
            min_improvement_threshold: 200,
        }
    }

    /// Create budget for thorough evolution (20 minutes)
    pub const fn thorough() -> Self {
        EvolutionBudget {
            total_time_ms: 20 * 60 * 1000,
            memory_budget_kb: 256 * 1024,
            max_mutations_per_cycle: 32,
            min_improvement_threshold: 50,
        }
    }
}

/// Kernel integration status
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum IntegrationStatus {
    Uninitialized = 0,
    Initializing = 1,
    Ready = 2,
    DreamActive = 3,
    Error = 4,
}

/// Kernel-specific power state for evolution decisions
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum KernelPowerState {
    Normal = 0,
    LowPower = 1,
    Critical = 2,
    Plugged = 3,
}

/// Kernel-specific thermal state
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum KernelThermalState {
    Cool = 0,
    Warm = 1,
    Hot = 2,
    Critical = 3,
}

/// Kernel integration configuration
#[derive(Clone, Copy, Debug)]
pub struct KernelIntegrationConfig {
    /// Enable evolution on startup
    pub enable_on_boot: bool,
    /// Idle threshold (ms) before starting dream mode
    pub idle_threshold_ms: u32,
    /// Standard budget for evolution sessions
    pub budget: EvolutionBudget,
    /// Approval mode: 0=automatic, 1=notify, 2=manual
    pub approval_mode: u8,
    /// Pause evolution if thermal > threshold
    pub thermal_limit_celsius: u32,
}

impl KernelIntegrationConfig {
    /// Create default configuration
    pub const fn default() -> Self {
        KernelIntegrationConfig {
            enable_on_boot: true,
            idle_threshold_ms: 5 * 60 * 1000, // 5 minutes
            budget: EvolutionBudget::default(),
            approval_mode: 0, // automatic
            thermal_limit_celsius: 80,
        }
    }

    /// Create aggressive configuration (more frequent evolution)
    pub const fn aggressive() -> Self {
        KernelIntegrationConfig {
            enable_on_boot: true,
            idle_threshold_ms: 2 * 60 * 1000, // 2 minutes
            budget: EvolutionBudget::thorough(),
            approval_mode: 1, // notify
            thermal_limit_celsius: 75,
        }
    }

    /// Create conservative configuration (rare evolution)
    pub const fn conservative() -> Self {
        KernelIntegrationConfig {
            enable_on_boot: true,
            idle_threshold_ms: 30 * 60 * 1000, // 30 minutes
            budget: EvolutionBudget::quick(),
            approval_mode: 2, // manual
            thermal_limit_celsius: 70,
        }
    }
}

/// Dream session tracking
#[derive(Clone, Copy, Debug)]
pub struct DreamSessionInfo {
    /// Session ID (incremented per session)
    pub session_id: u64,
    /// Time when session started (ms since boot)
    pub start_time_ms: u64,
    /// Idle time when session triggered (ms)
    pub idle_time_ms: u32,
    /// Cycles completed in this session
    pub cycles_completed: u32,
    /// Mutations attempted
    pub mutations_attempted: u32,
    /// Mutations succeeded
    pub mutations_succeeded: u32,
    /// Time elapsed (ms)
    pub elapsed_time_ms: u32,
}

impl DreamSessionInfo {
    /// Create new session info
    pub const fn new(session_id: u64, start_time_ms: u64, idle_time_ms: u32) -> Self {
        DreamSessionInfo {
            session_id,
            start_time_ms,
            idle_time_ms,
            cycles_completed: 0,
            mutations_attempted: 0,
            mutations_succeeded: 0,
            elapsed_time_ms: 0,
        }
    }

    /// Get success rate percent
    pub fn success_rate(&self) -> u32 {
        if self.mutations_attempted == 0 {
            return 0;
        }
        ((self.mutations_succeeded as u64 * 100) / self.mutations_attempted as u64) as u32
    }

    /// Get cycles per minute
    pub fn cycles_per_minute(&self) -> u32 {
        if self.elapsed_time_ms == 0 {
            return 0;
        }
        ((self.cycles_completed as u64 * 60 * 1000) / self.elapsed_time_ms as u64) as u32
    }
}

/// Kernel Ouroboros Integration
pub struct KernelOuroborosIntegration {
    /// Configuration
    config: KernelIntegrationConfig,
    /// Integration status
    status: IntegrationStatus,
    /// Is evolution enabled
    is_enabled: bool,
    /// Current power state
    power_state: KernelPowerState,
    /// Current thermal state
    thermal_state: KernelThermalState,
    /// Dream mode active
    dream_active: bool,
    /// Current session info
    current_session: Option<DreamSessionInfo>,
    /// Session counter
    session_counter: u64,
    /// Idle time accumulator (ms)
    idle_accumulator_ms: u32,
    /// Last activity time (ms since boot)
    last_activity_time_ms: u64,
}

impl KernelOuroborosIntegration {
    /// Create new kernel integration
    pub const fn new(config: KernelIntegrationConfig) -> Self {
        KernelOuroborosIntegration {
            config,
            status: IntegrationStatus::Uninitialized,
            is_enabled: config.enable_on_boot,
            power_state: KernelPowerState::Normal,
            thermal_state: KernelThermalState::Cool,
            dream_active: false,
            current_session: None,
            session_counter: 0,
            idle_accumulator_ms: 0,
            last_activity_time_ms: 0,
        }
    }

    /// Initialize during kernel startup
    pub fn on_kernel_ready(&mut self, current_time_ms: u64) -> bool {
        if self.status != IntegrationStatus::Uninitialized {
            return false;
        }

        self.status = IntegrationStatus::Initializing;
        self.last_activity_time_ms = current_time_ms;
        self.idle_accumulator_ms = 0;

        // Emit initialization marker (would call telemetry in real kernel)
        // RAYOS_OUROBOROS:INITIALIZED

        self.status = IntegrationStatus::Ready;
        true
    }

    /// Handle activity detection (reset idle timer)
    pub fn on_activity(&mut self, current_time_ms: u64) {
        self.last_activity_time_ms = current_time_ms;
        self.idle_accumulator_ms = 0;

        // If dream was active, wake up
        if self.dream_active {
            self.on_dream_end();
        }
    }

    /// Handle scheduler tick (check for idle)
    pub fn on_scheduler_tick(&mut self, current_time_ms: u64, time_delta_ms: u32) -> bool {
        if !self.is_enabled || self.status != IntegrationStatus::Ready {
            return false;
        }

        // Check power state - don't evolve on low power
        if self.power_state == KernelPowerState::Critical {
            return false;
        }

        // Check thermal state - throttle if hot
        if self.thermal_state == KernelThermalState::Critical {
            return false;
        }

        // Accumulate idle time
        self.idle_accumulator_ms = self.idle_accumulator_ms.saturating_add(time_delta_ms);

        // Check if idle threshold reached
        if self.idle_accumulator_ms >= self.config.idle_threshold_ms && !self.dream_active {
            return self.on_idle_detect(current_time_ms);
        }

        false
    }

    /// Handle idle detection - start dream mode
    fn on_idle_detect(&mut self, current_time_ms: u64) -> bool {
        if self.dream_active {
            return false;
        }

        self.status = IntegrationStatus::DreamActive;
        self.dream_active = true;

        let session = DreamSessionInfo::new(self.session_counter, current_time_ms, self.idle_accumulator_ms);
        self.current_session = Some(session);
        self.session_counter += 1;

        // Would emit: RAYOS_OUROBOROS:DREAM_STARTED

        true
    }

    /// End dream session
    pub fn on_dream_end(&mut self) {
        if !self.dream_active {
            return;
        }

        self.dream_active = false;
        self.status = IntegrationStatus::Ready;
        self.idle_accumulator_ms = 0;

        if let Some(session) = self.current_session {
            // Log session stats
            // success_rate, cycles_per_minute, elapsed_time_ms
            // Would emit: RAYOS_OUROBOROS:DREAM_ENDED
        }

        self.current_session = None;
    }

    /// Update power state
    pub fn set_power_state(&mut self, state: KernelPowerState) {
        self.power_state = state;

        // Pause evolution if entering critical power
        if state == KernelPowerState::Critical && self.dream_active {
            self.on_dream_end();
        }
    }

    /// Update thermal state
    pub fn set_thermal_state(&mut self, state: KernelThermalState) {
        self.thermal_state = state;

        // Pause evolution if overheating
        if state == KernelThermalState::Critical && self.dream_active {
            self.on_dream_end();
        }
    }

    /// Check if ready to allocate budget
    pub fn can_allocate_budget(&self) -> bool {
        self.dream_active
            && self.thermal_state != KernelThermalState::Critical
            && self.power_state != KernelPowerState::Critical
    }

    /// Get remaining budget for current session
    pub fn remaining_budget(&self) -> EvolutionBudget {
        if let Some(session) = self.current_session {
            let elapsed = session.elapsed_time_ms;
            let remaining_ms = self.config.budget.total_time_ms.saturating_sub(elapsed);
            let remaining_memory = (self.config.budget.memory_budget_kb * remaining_ms as u32)
                / self.config.budget.total_time_ms.max(1);

            return EvolutionBudget {
                total_time_ms: remaining_ms,
                memory_budget_kb: remaining_memory,
                max_mutations_per_cycle: self.config.budget.max_mutations_per_cycle,
                min_improvement_threshold: self.config.budget.min_improvement_threshold,
            };
        }

        EvolutionBudget::default()
    }

    /// Update session progress
    pub fn record_cycle_complete(&mut self, mutations_attempted: u32, mutations_succeeded: u32) {
        if let Some(ref mut session) = self.current_session {
            session.cycles_completed += 1;
            session.mutations_attempted += mutations_attempted;
            session.mutations_succeeded += mutations_succeeded;
        }
    }

    /// Get current status
    pub fn status(&self) -> IntegrationStatus {
        self.status
    }

    /// Get current session info
    pub fn current_session_info(&self) -> Option<DreamSessionInfo> {
        self.current_session
    }

    /// Check if dream active
    pub fn is_dream_active(&self) -> bool {
        self.dream_active
    }

    /// Get approval mode
    pub fn approval_mode(&self) -> u8 {
        self.config.approval_mode
    }

    /// Enable evolution
    pub fn enable(&mut self) {
        self.is_enabled = true;
    }

    /// Disable evolution
    pub fn disable(&mut self) {
        self.is_enabled = false;
        if self.dream_active {
            self.on_dream_end();
        }
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evolution_budget_defaults() {
        let budget = EvolutionBudget::default();
        assert_eq!(budget.total_time_ms, 5 * 60 * 1000);
        assert_eq!(budget.memory_budget_kb, 100 * 1024);
    }

    #[test]
    fn test_evolution_budget_quick() {
        let budget = EvolutionBudget::quick();
        assert_eq!(budget.total_time_ms, 30 * 1000);
        assert!(budget.memory_budget_kb < EvolutionBudget::default().memory_budget_kb);
    }

    #[test]
    fn test_evolution_budget_thorough() {
        let budget = EvolutionBudget::thorough();
        assert!(budget.total_time_ms > EvolutionBudget::default().total_time_ms);
        assert!(budget.max_mutations_per_cycle > EvolutionBudget::default().max_mutations_per_cycle);
    }

    #[test]
    fn test_kernel_config_default() {
        let config = KernelIntegrationConfig::default();
        assert!(config.enable_on_boot);
        assert_eq!(config.approval_mode, 0);
    }

    #[test]
    fn test_kernel_config_aggressive() {
        let config = KernelIntegrationConfig::aggressive();
        assert!(config.idle_threshold_ms < KernelIntegrationConfig::default().idle_threshold_ms);
        assert_eq!(config.approval_mode, 1);
    }

    #[test]
    fn test_kernel_config_conservative() {
        let config = KernelIntegrationConfig::conservative();
        assert!(config.idle_threshold_ms > KernelIntegrationConfig::default().idle_threshold_ms);
        assert_eq!(config.approval_mode, 2);
    }

    #[test]
    fn test_dream_session_info_creation() {
        let session = DreamSessionInfo::new(1, 1000, 500);
        assert_eq!(session.session_id, 1);
        assert_eq!(session.start_time_ms, 1000);
        assert_eq!(session.idle_time_ms, 500);
        assert_eq!(session.cycles_completed, 0);
    }

    #[test]
    fn test_dream_session_info_success_rate() {
        let mut session = DreamSessionInfo::new(1, 1000, 500);
        session.mutations_attempted = 10;
        session.mutations_succeeded = 7;
        assert_eq!(session.success_rate(), 70);
    }

    #[test]
    fn test_dream_session_info_success_rate_zero() {
        let session = DreamSessionInfo::new(1, 1000, 500);
        assert_eq!(session.success_rate(), 0);
    }

    #[test]
    fn test_dream_session_info_cycles_per_minute() {
        let mut session = DreamSessionInfo::new(1, 1000, 500);
        session.cycles_completed = 10;
        session.elapsed_time_ms = 60 * 1000; // 1 minute
        assert_eq!(session.cycles_per_minute(), 10);
    }

    #[test]
    fn test_kernel_integration_creation() {
        let config = KernelIntegrationConfig::default();
        let integration = KernelOuroborosIntegration::new(config);
        assert_eq!(integration.status, IntegrationStatus::Uninitialized);
        assert!(!integration.dream_active);
    }

    #[test]
    fn test_kernel_integration_on_kernel_ready() {
        let config = KernelIntegrationConfig::default();
        let mut integration = KernelOuroborosIntegration::new(config);
        assert!(integration.on_kernel_ready(1000));
        assert_eq!(integration.status, IntegrationStatus::Ready);
    }

    #[test]
    fn test_kernel_integration_on_kernel_ready_twice() {
        let config = KernelIntegrationConfig::default();
        let mut integration = KernelOuroborosIntegration::new(config);
        assert!(integration.on_kernel_ready(1000));
        assert!(!integration.on_kernel_ready(2000)); // Can't init twice
    }

    #[test]
    fn test_kernel_integration_on_activity() {
        let config = KernelIntegrationConfig::default();
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);
        integration.on_activity(2000);
        assert_eq!(integration.idle_accumulator_ms, 0);
    }

    #[test]
    fn test_kernel_integration_scheduler_tick() {
        let config = KernelIntegrationConfig::default();
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);

        // Single tick shouldn't trigger idle
        assert!(!integration.on_scheduler_tick(1010, 10));
        assert_eq!(integration.idle_accumulator_ms, 10);
    }

    #[test]
    fn test_kernel_integration_idle_threshold() {
        let mut config = KernelIntegrationConfig::default();
        config.idle_threshold_ms = 1000; // 1 second for testing
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);

        // Accumulate idle time
        let mut time = 1000;
        for _ in 0..100 {
            if integration.on_scheduler_tick(time, 20) {
                break; // Idle triggered
            }
            time += 20;
        }

        assert!(integration.dream_active);
    }

    #[test]
    fn test_kernel_integration_activity_resets_idle() {
        let mut config = KernelIntegrationConfig::default();
        config.idle_threshold_ms = 1000;
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);

        // Accumulate some idle time
        integration.on_scheduler_tick(1100, 500);
        assert_eq!(integration.idle_accumulator_ms, 500);

        // Activity resets it
        integration.on_activity(1600);
        assert_eq!(integration.idle_accumulator_ms, 0);
    }

    #[test]
    fn test_kernel_integration_power_state() {
        let config = KernelIntegrationConfig::default();
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);

        assert!(integration.can_allocate_budget());

        integration.set_power_state(KernelPowerState::Critical);
        assert!(!integration.can_allocate_budget());
    }

    #[test]
    fn test_kernel_integration_thermal_state() {
        let config = KernelIntegrationConfig::default();
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);

        assert!(integration.can_allocate_budget());

        integration.set_thermal_state(KernelThermalState::Critical);
        assert!(!integration.can_allocate_budget());
    }

    #[test]
    fn test_kernel_integration_dream_session() {
        let mut config = KernelIntegrationConfig::default();
        config.idle_threshold_ms = 100;
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);

        // Trigger idle
        integration.on_scheduler_tick(1100, 50);
        integration.on_scheduler_tick(1150, 50);

        assert!(integration.dream_active);
        assert!(integration.current_session.is_some());
    }

    #[test]
    fn test_kernel_integration_record_cycle() {
        let mut config = KernelIntegrationConfig::default();
        config.idle_threshold_ms = 100;
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);
        integration.on_scheduler_tick(1100, 50);
        integration.on_scheduler_tick(1150, 50);

        integration.record_cycle_complete(10, 7);
        assert_eq!(integration.current_session.unwrap().cycles_completed, 1);
        assert_eq!(integration.current_session.unwrap().mutations_succeeded, 7);
    }

    #[test]
    fn test_kernel_integration_remaining_budget() {
        let config = KernelIntegrationConfig::default();
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);

        let budget = integration.remaining_budget();
        assert_eq!(budget.total_time_ms, config.budget.total_time_ms);
    }

    #[test]
    fn test_kernel_integration_enable_disable() {
        let config = KernelIntegrationConfig::default();
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);

        assert!(integration.is_enabled());
        integration.disable();
        assert!(!integration.is_enabled());
        integration.enable();
        assert!(integration.is_enabled());
    }

    #[test]
    fn test_kernel_integration_power_stops_dream() {
        let mut config = KernelIntegrationConfig::default();
        config.idle_threshold_ms = 100;
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);
        integration.on_scheduler_tick(1100, 50);
        integration.on_scheduler_tick(1150, 50);

        assert!(integration.dream_active);
        integration.set_power_state(KernelPowerState::Critical);
        assert!(!integration.dream_active);
    }

    #[test]
    fn test_kernel_integration_thermal_stops_dream() {
        let mut config = KernelIntegrationConfig::default();
        config.idle_threshold_ms = 100;
        let mut integration = KernelOuroborosIntegration::new(config);
        integration.on_kernel_ready(1000);
        integration.on_scheduler_tick(1100, 50);
        integration.on_scheduler_tick(1150, 50);

        assert!(integration.dream_active);
        integration.set_thermal_state(KernelThermalState::Critical);
        assert!(!integration.dream_active);
    }

    #[test]
    fn test_kernel_integration_approval_modes() {
        let mut config1 = KernelIntegrationConfig::default();
        config1.approval_mode = 0;
        assert_eq!(KernelOuroborosIntegration::new(config1).approval_mode(), 0);

        let mut config2 = KernelIntegrationConfig::default();
        config2.approval_mode = 2;
        assert_eq!(KernelOuroborosIntegration::new(config2).approval_mode(), 2);
    }
}
