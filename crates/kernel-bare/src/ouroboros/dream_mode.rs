//! Dream Mode Controller for Ouroboros Engine
//!
//! Manages idle-triggered evolution sessions with budget allocation,
//! power management integration, and user control.
//!
//! Phase 33, Task 3

use core::mem;

/// Dream session mutation approval mode
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DreamApprovalMode {
    Automatic = 0,   // Apply mutations automatically
    Notify = 1,      // Notify user, apply after delay
    Manual = 2,      // Require user approval for each mutation
}

/// Dream mode state
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DreamModeState {
    Idle = 0,         // Waiting for idle trigger
    Active = 1,       // Evolution in progress
    Paused = 2,       // Paused by user or system
    Throttled = 3,    // Paused due to power/thermal
    Ended = 4,        // Session complete
}

/// Thermal throttle level
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ThermalThrottle {
    None = 0,       // No throttling
    Moderate = 1,   // Reduce batch size
    Severe = 2,     // Reduce frequency
    Critical = 3,   // Stop evolution
}

/// Power throttle level
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PowerThrottle {
    None = 0,         // Normal operation
    LowBattery = 1,   // Reduce mutation frequency
    Critical = 2,     // Stop evolution
}

/// Dream mode session state
#[derive(Clone, Copy, Debug)]
pub struct DreamModeSession {
    /// Session ID
    pub session_id: u64,
    /// Current state
    pub state: DreamModeState,
    /// Time budget remaining (ms)
    pub time_budget_remaining_ms: u32,
    /// Memory budget remaining (KB)
    pub memory_budget_remaining_kb: u32,
    /// Cycles completed
    pub cycles_completed: u32,
    /// Mutations attempted
    pub mutations_attempted: u32,
    /// Mutations succeeded
    pub mutations_succeeded: u32,
    /// Session start time (ms since boot)
    pub start_time_ms: u64,
    /// Total elapsed time (ms)
    pub elapsed_time_ms: u32,
}

impl DreamModeSession {
    /// Create new dream session
    pub const fn new(session_id: u64, start_time_ms: u64, time_budget_ms: u32, memory_budget_kb: u32) -> Self {
        DreamModeSession {
            session_id,
            state: DreamModeState::Active,
            time_budget_remaining_ms: time_budget_ms,
            memory_budget_remaining_kb: memory_budget_kb,
            cycles_completed: 0,
            mutations_attempted: 0,
            mutations_succeeded: 0,
            start_time_ms,
            elapsed_time_ms: 0,
        }
    }

    /// Check if budget exhausted
    pub fn budget_exhausted(&self) -> bool {
        self.time_budget_remaining_ms == 0 || self.memory_budget_remaining_kb == 0
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

    /// Spend time from budget
    pub fn spend_time(&mut self, time_ms: u32) -> bool {
        if self.time_budget_remaining_ms >= time_ms {
            self.time_budget_remaining_ms -= time_ms;
            self.elapsed_time_ms = self.elapsed_time_ms.saturating_add(time_ms);
            true
        } else {
            self.time_budget_remaining_ms = 0;
            self.elapsed_time_ms = self.elapsed_time_ms.saturating_add(self.time_budget_remaining_ms);
            false
        }
    }

    /// Spend memory from budget
    pub fn spend_memory(&mut self, memory_kb: u32) -> bool {
        if self.memory_budget_remaining_kb >= memory_kb {
            self.memory_budget_remaining_kb -= memory_kb;
            true
        } else {
            self.memory_budget_remaining_kb = 0;
            false
        }
    }

    /// Record cycle completion
    pub fn record_cycle(&mut self, mutations: u32, succeeded: u32) {
        self.cycles_completed += 1;
        self.mutations_attempted += mutations;
        self.mutations_succeeded += succeeded;
    }
}

/// Dream Mode Controller
pub struct DreamModeController {
    /// Approval mode for mutations
    approval_mode: DreamApprovalMode,
    /// Current dream session
    current_session: Option<DreamModeSession>,
    /// Session counter
    session_counter: u64,
    /// Overall dream mode enabled
    enabled: bool,
    /// User paused evolution
    user_paused: bool,
    /// Thermal throttle level
    thermal_throttle: ThermalThrottle,
    /// Power throttle level
    power_throttle: PowerThrottle,
    /// Total sessions completed
    total_sessions: u32,
    /// Total mutations across all sessions
    total_mutations_all_sessions: u32,
    /// Total successful mutations
    total_succeeded_all_sessions: u32,
}

impl DreamModeController {
    /// Create new dream mode controller
    pub const fn new(approval_mode: DreamApprovalMode) -> Self {
        DreamModeController {
            approval_mode,
            current_session: None,
            session_counter: 0,
            enabled: true,
            user_paused: false,
            thermal_throttle: ThermalThrottle::None,
            power_throttle: PowerThrottle::None,
            total_sessions: 0,
            total_mutations_all_sessions: 0,
            total_succeeded_all_sessions: 0,
        }
    }

    /// Start dream session
    pub fn start_session(&mut self, current_time_ms: u64, time_budget_ms: u32, memory_budget_kb: u32) -> bool {
        if self.current_session.is_some() {
            return false; // Session already active
        }

        if !self.enabled || self.user_paused {
            return false;
        }

        if self.thermal_throttle == ThermalThrottle::Critical || self.power_throttle == PowerThrottle::Critical {
            return false;
        }

        let mut session = DreamModeSession::new(self.session_counter, current_time_ms, time_budget_ms, memory_budget_kb);

        // Apply throttling to budget
        match self.thermal_throttle {
            ThermalThrottle::Moderate => {
                session.time_budget_remaining_ms = (session.time_budget_remaining_ms * 75) / 100; // 25% reduction
            }
            ThermalThrottle::Severe => {
                session.time_budget_remaining_ms = (session.time_budget_remaining_ms * 50) / 100; // 50% reduction
            }
            _ => {}
        }

        match self.power_throttle {
            PowerThrottle::LowBattery => {
                session.time_budget_remaining_ms = (session.time_budget_remaining_ms * 50) / 100; // 50% reduction
            }
            _ => {}
        }

        self.current_session = Some(session);
        self.session_counter += 1;
        true
    }

    /// End current dream session
    pub fn end_session(&mut self) -> Option<DreamModeSession> {
        if let Some(mut session) = self.current_session.take() {
            session.state = DreamModeState::Ended;
            self.total_sessions += 1;
            self.total_mutations_all_sessions += session.mutations_attempted;
            self.total_succeeded_all_sessions += session.mutations_succeeded;
            return Some(session);
        }
        None
    }

    /// Pause dream session (user triggered)
    pub fn pause(&mut self) {
        self.user_paused = true;
        if let Some(ref mut session) = self.current_session {
            session.state = DreamModeState::Paused;
        }
    }

    /// Resume dream session (user triggered)
    pub fn resume(&mut self) {
        self.user_paused = false;
        if let Some(ref mut session) = self.current_session {
            if session.state == DreamModeState::Paused {
                session.state = DreamModeState::Active;
            }
        }
    }

    /// Update thermal state - affects budget
    pub fn set_thermal_throttle(&mut self, throttle: ThermalThrottle) {
        self.thermal_throttle = throttle;

        if throttle == ThermalThrottle::Critical && self.current_session.is_some() {
            if let Some(ref mut session) = self.current_session {
                session.state = DreamModeState::Throttled;
            }
        }
    }

    /// Update power state - affects budget
    pub fn set_power_throttle(&mut self, throttle: PowerThrottle) {
        self.power_throttle = throttle;

        if throttle == PowerThrottle::Critical && self.current_session.is_some() {
            if let Some(ref mut session) = self.current_session {
                session.state = DreamModeState::Throttled;
            }
        }
    }

    /// Record cycle in current session
    pub fn record_cycle(&mut self, mutations: u32, succeeded: u32, elapsed_ms: u32) -> bool {
        if let Some(ref mut session) = self.current_session {
            session.record_cycle(mutations, succeeded);
            return session.spend_time(elapsed_ms);
        }
        false
    }

    /// Check if can continue evolution
    pub fn can_continue(&self) -> bool {
        if !self.enabled || self.user_paused {
            return false;
        }

        if self.thermal_throttle == ThermalThrottle::Critical || self.power_throttle == PowerThrottle::Critical {
            return false;
        }

        if let Some(session) = self.current_session {
            return !session.budget_exhausted();
        }

        false
    }

    /// Get current session info
    pub fn current_session(&self) -> Option<DreamModeSession> {
        self.current_session
    }

    /// Get approval mode
    pub fn approval_mode(&self) -> DreamApprovalMode {
        self.approval_mode
    }

    /// Set approval mode
    pub fn set_approval_mode(&mut self, mode: DreamApprovalMode) {
        self.approval_mode = mode;
    }

    /// Get session statistics
    pub fn session_stats(&self) -> Option<(u32, u32, u32)> {
        if let Some(session) = self.current_session {
            return Some((session.cycles_completed, session.mutations_attempted, session.mutations_succeeded));
        }
        None
    }

    /// Get overall statistics
    pub fn overall_stats(&self) -> (u32, u32, u32) {
        (
            self.total_sessions,
            self.total_mutations_all_sessions,
            self.total_succeeded_all_sessions,
        )
    }

    /// Get overall success rate
    pub fn overall_success_rate(&self) -> u32 {
        if self.total_mutations_all_sessions == 0 {
            return 0;
        }
        ((self.total_succeeded_all_sessions as u64 * 100) / self.total_mutations_all_sessions as u64) as u32
    }

    /// Enable/disable dream mode
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled && self.current_session.is_some() {
            self.end_session();
        }
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Check if session active
    pub fn has_active_session(&self) -> bool {
        self.current_session.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dream_session_creation() {
        let session = DreamModeSession::new(1, 1000, 60000, 102400);
        assert_eq!(session.session_id, 1);
        assert_eq!(session.state, DreamModeState::Active);
        assert_eq!(session.time_budget_remaining_ms, 60000);
    }

    #[test]
    fn test_dream_session_budget_exhausted() {
        let mut session = DreamModeSession::new(1, 1000, 100, 100);
        assert!(!session.budget_exhausted());
        session.time_budget_remaining_ms = 0;
        assert!(session.budget_exhausted());
    }

    #[test]
    fn test_dream_session_success_rate() {
        let mut session = DreamModeSession::new(1, 1000, 60000, 102400);
        session.mutations_attempted = 10;
        session.mutations_succeeded = 7;
        assert_eq!(session.success_rate(), 70);
    }

    #[test]
    fn test_dream_session_cycles_per_minute() {
        let mut session = DreamModeSession::new(1, 1000, 60000, 102400);
        session.cycles_completed = 6;
        session.elapsed_time_ms = 60 * 1000; // 1 minute
        assert_eq!(session.cycles_per_minute(), 6);
    }

    #[test]
    fn test_dream_session_spend_time() {
        let mut session = DreamModeSession::new(1, 1000, 1000, 102400);
        assert!(session.spend_time(500));
        assert_eq!(session.time_budget_remaining_ms, 500);
        assert!(session.spend_time(500));
        assert_eq!(session.time_budget_remaining_ms, 0);
        assert!(!session.spend_time(100)); // Can't spend more
    }

    #[test]
    fn test_dream_session_spend_memory() {
        let mut session = DreamModeSession::new(1, 1000, 60000, 1000);
        assert!(session.spend_memory(500));
        assert_eq!(session.memory_budget_remaining_kb, 500);
        assert!(session.spend_memory(500));
        assert_eq!(session.memory_budget_remaining_kb, 0);
        assert!(!session.spend_memory(100)); // Can't spend more
    }

    #[test]
    fn test_dream_session_record_cycle() {
        let mut session = DreamModeSession::new(1, 1000, 60000, 102400);
        session.record_cycle(8, 6);
        assert_eq!(session.cycles_completed, 1);
        assert_eq!(session.mutations_attempted, 8);
        assert_eq!(session.mutations_succeeded, 6);
    }

    #[test]
    fn test_dream_mode_controller_creation() {
        let controller = DreamModeController::new(DreamApprovalMode::Automatic);
        assert_eq!(controller.approval_mode(), DreamApprovalMode::Automatic);
        assert!(!controller.has_active_session());
    }

    #[test]
    fn test_dream_mode_controller_start_session() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        assert!(controller.start_session(1000, 60000, 102400));
        assert!(controller.has_active_session());
    }

    #[test]
    fn test_dream_mode_controller_start_session_twice() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        assert!(controller.start_session(1000, 60000, 102400));
        assert!(!controller.start_session(1100, 60000, 102400)); // Can't start twice
    }

    #[test]
    fn test_dream_mode_controller_end_session() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        controller.start_session(1000, 60000, 102400);
        let session = controller.end_session();
        assert!(session.is_some());
        assert!(!controller.has_active_session());
        assert_eq!(controller.total_sessions, 1);
    }

    #[test]
    fn test_dream_mode_controller_pause_resume() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        controller.start_session(1000, 60000, 102400);

        controller.pause();
        assert!(!controller.can_continue());

        controller.resume();
        assert!(controller.can_continue());
    }

    #[test]
    fn test_dream_mode_controller_thermal_throttle() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        controller.set_thermal_throttle(ThermalThrottle::Moderate);
        assert!(controller.start_session(1000, 100000, 102400));

        let session = controller.current_session();
        assert!(session.is_some());
        // Moderate throttle should reduce budget by 25%
        assert!(session.unwrap().time_budget_remaining_ms < 100000);
    }

    #[test]
    fn test_dream_mode_controller_power_throttle() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        controller.set_power_throttle(PowerThrottle::LowBattery);
        assert!(controller.start_session(1000, 100000, 102400));

        let session = controller.current_session();
        assert!(session.is_some());
        // Low battery should reduce budget by 50%
        assert!(session.unwrap().time_budget_remaining_ms < 100000);
    }

    #[test]
    fn test_dream_mode_controller_critical_thermal() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        controller.start_session(1000, 60000, 102400);

        controller.set_thermal_throttle(ThermalThrottle::Critical);
        assert!(!controller.can_continue()); // Stopped by critical thermal
    }

    #[test]
    fn test_dream_mode_controller_critical_power() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        controller.start_session(1000, 60000, 102400);

        controller.set_power_throttle(PowerThrottle::Critical);
        assert!(!controller.can_continue()); // Stopped by critical power
    }

    #[test]
    fn test_dream_mode_controller_record_cycle() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        controller.start_session(1000, 60000, 102400);

        assert!(controller.record_cycle(8, 6, 100));

        let session = controller.current_session();
        assert!(session.is_some());
        let s = session.unwrap();
        assert_eq!(s.cycles_completed, 1);
        assert_eq!(s.mutations_attempted, 8);
        assert_eq!(s.mutations_succeeded, 6);
    }

    #[test]
    fn test_dream_mode_controller_overall_stats() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);

        controller.start_session(1000, 60000, 102400);
        controller.record_cycle(10, 7, 100);
        controller.end_session();

        let (sessions, mutations, succeeded) = controller.overall_stats();
        assert_eq!(sessions, 1);
        assert_eq!(mutations, 10);
        assert_eq!(succeeded, 7);
    }

    #[test]
    fn test_dream_mode_controller_overall_success_rate() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);

        controller.start_session(1000, 60000, 102400);
        controller.record_cycle(10, 7, 100);
        controller.end_session();

        assert_eq!(controller.overall_success_rate(), 70);
    }

    #[test]
    fn test_dream_mode_controller_enable_disable() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        assert!(controller.is_enabled());

        controller.set_enabled(false);
        assert!(!controller.is_enabled());

        controller.set_enabled(true);
        assert!(controller.is_enabled());
    }

    #[test]
    fn test_dream_mode_controller_approval_modes() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        assert_eq!(controller.approval_mode(), DreamApprovalMode::Automatic);

        controller.set_approval_mode(DreamApprovalMode::Manual);
        assert_eq!(controller.approval_mode(), DreamApprovalMode::Manual);
    }

    #[test]
    fn test_dream_mode_controller_disable_stops_session() {
        let mut controller = DreamModeController::new(DreamApprovalMode::Automatic);
        controller.start_session(1000, 60000, 102400);
        assert!(controller.has_active_session());

        controller.set_enabled(false);
        assert!(!controller.has_active_session());
    }
}
