//! Evolution Module Integration: Unified Orchestration
//!
//! Integrates all Phase 34 modules (live patching, profiling, feedback loops,
//! autonomous optimization, multi-objective optimization, and web dashboard)
//! into a cohesive evolution coordinator. Manages module communication, state
//! synchronization, and evolution phase transitions.
//!
//! Phase 35, Task 1

/// Evolution phase type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum EvolutionPhase {
    Idle = 0,
    Profiling = 1,
    Mutation = 2,
    Testing = 3,
    Selection = 4,
    Patching = 5,
    Learning = 6,
}

impl EvolutionPhase {
    /// Get phase name
    pub const fn name(&self) -> &'static str {
        match self {
            EvolutionPhase::Idle => "Idle",
            EvolutionPhase::Profiling => "Profiling",
            EvolutionPhase::Mutation => "Mutation",
            EvolutionPhase::Testing => "Testing",
            EvolutionPhase::Selection => "Selection",
            EvolutionPhase::Patching => "Patching",
            EvolutionPhase::Learning => "Learning",
        }
    }

    /// Is active phase (not idle)
    pub const fn is_active(&self) -> bool {
        !matches!(self, EvolutionPhase::Idle)
    }
}

/// Module readiness status
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ModuleStatus {
    Uninitialized = 0,
    Ready = 1,
    Processing = 2,
    Idle = 3,
    Error = 4,
}

/// Evolution module type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ModuleType {
    Profiler = 0,
    Mutator = 1,
    Selector = 2,
    Patcher = 3,
    FeedbackLoop = 4,
    Optimizer = 5,
    Dashboard = 6,
}

/// Module health check result
#[derive(Clone, Copy, Debug)]
pub struct ModuleHealth {
    /// Module type
    pub module: ModuleType,
    /// Current status
    pub status: ModuleStatus,
    /// Last heartbeat (ms since start)
    pub last_heartbeat_ms: u32,
    /// Error count
    pub error_count: u16,
    /// Success count
    pub success_count: u16,
}

impl ModuleHealth {
    /// Create new module health check
    pub const fn new(module: ModuleType) -> Self {
        ModuleHealth {
            module,
            status: ModuleStatus::Uninitialized,
            last_heartbeat_ms: 0,
            error_count: 0,
            success_count: 0,
        }
    }

    /// Get health percentage (0-100)
    pub fn health_percent(&self) -> u8 {
        let total = self.error_count as u32 + self.success_count as u32;
        if total == 0 {
            return 50;  // Uninitialized
        }
        ((self.success_count as u32 * 100) / total).min(100) as u8
    }

    /// Is healthy (>80% success rate)
    pub fn is_healthy(&self) -> bool {
        self.health_percent() >= 80
    }

    /// Record success
    pub fn record_success(&mut self) {
        self.success_count += 1;
        self.status = ModuleStatus::Ready;
    }

    /// Record error
    pub fn record_error(&mut self) {
        self.error_count += 1;
        self.status = ModuleStatus::Error;
    }
}

/// Phase transition record
#[derive(Clone, Copy, Debug)]
pub struct PhaseTransition {
    /// Transition ID
    pub id: u32,
    /// From phase
    pub from_phase: EvolutionPhase,
    /// To phase
    pub to_phase: EvolutionPhase,
    /// Duration (ms)
    pub duration_ms: u32,
    /// Successful transition
    pub success: bool,
}

impl PhaseTransition {
    /// Create new phase transition
    pub const fn new(id: u32, from: EvolutionPhase, to: EvolutionPhase) -> Self {
        PhaseTransition {
            id,
            from_phase: from,
            to_phase: to,
            duration_ms: 0,
            success: true,
        }
    }
}

/// Module communication message
#[derive(Clone, Copy, Debug)]
pub struct ModuleMessage {
    /// Message ID
    pub id: u32,
    /// Source module
    pub source: ModuleType,
    /// Destination module
    pub dest: ModuleType,
    /// Message type
    pub msg_type: u8,
    /// Data payload size
    pub payload_size: u16,
}

impl ModuleMessage {
    /// Create new module message
    pub const fn new(id: u32, source: ModuleType, dest: ModuleType, msg_type: u8) -> Self {
        ModuleMessage {
            id,
            source,
            dest,
            msg_type,
            payload_size: 0,
        }
    }

    /// Set payload size
    pub fn with_size(mut self, size: u16) -> Self {
        self.payload_size = size;
        self
    }
}

/// Metric aggregation snapshot
#[derive(Clone, Copy, Debug)]
pub struct MetricAggregation {
    /// Aggregation ID
    pub id: u32,
    /// Total mutations evaluated
    pub total_mutations: u32,
    /// Successful mutations
    pub successful_mutations: u32,
    /// Average improvement percent
    pub avg_improvement_percent: i8,
    /// System uptime (seconds)
    pub system_uptime_seconds: u32,
    /// Active modules count
    pub active_modules: u8,
    /// Frontier size
    pub frontier_size: u16,
}

impl MetricAggregation {
    /// Create new metric aggregation
    pub const fn new(id: u32, total: u32, successful: u32) -> Self {
        MetricAggregation {
            id,
            total_mutations: total,
            successful_mutations: successful,
            avg_improvement_percent: 0,
            system_uptime_seconds: 0,
            active_modules: 0,
            frontier_size: 0,
        }
    }

    /// Get success rate (0-100)
    pub fn success_rate(&self) -> u8 {
        if self.total_mutations == 0 {
            0
        } else {
            ((self.successful_mutations as u32 * 100) / self.total_mutations as u32) as u8
        }
    }
}

/// Evolution session metadata
#[derive(Clone, Copy, Debug)]
pub struct EvolutionSession {
    /// Session ID
    pub id: u32,
    /// Start time (ms)
    pub start_time_ms: u64,
    /// Current phase
    pub current_phase: EvolutionPhase,
    /// Is paused
    pub paused: bool,
    /// Total cycles completed
    pub total_cycles: u32,
    /// Metrics snapshot
    pub metrics: MetricAggregation,
}

impl EvolutionSession {
    /// Create new evolution session
    pub const fn new(id: u32) -> Self {
        EvolutionSession {
            id,
            start_time_ms: 0,
            current_phase: EvolutionPhase::Idle,
            paused: false,
            total_cycles: 0,
            metrics: MetricAggregation::new(id, 0, 0),
        }
    }

    /// Get elapsed time (ms)
    pub fn elapsed_ms(&self, current_time_ms: u64) -> u64 {
        if self.start_time_ms == 0 {
            0
        } else {
            current_time_ms - self.start_time_ms
        }
    }
}

/// Evolution Coordinator
pub struct EvolutionCoordinator {
    /// Current evolution session
    session: EvolutionSession,
    /// Module health status (7 modules)
    module_health: [ModuleHealth; 7],
    /// Phase transition history (max 100)
    phase_history: [Option<PhaseTransition>; 100],
    /// Module messages in transit (max 50)
    messages: [Option<ModuleMessage>; 50],
    /// Total transitions
    total_transitions: u32,
    /// Total messages processed
    total_messages: u32,
}

impl EvolutionCoordinator {
    /// Create new evolution coordinator
    pub const fn new() -> Self {
        let modules = [
            ModuleHealth::new(ModuleType::Profiler),
            ModuleHealth::new(ModuleType::Mutator),
            ModuleHealth::new(ModuleType::Selector),
            ModuleHealth::new(ModuleType::Patcher),
            ModuleHealth::new(ModuleType::FeedbackLoop),
            ModuleHealth::new(ModuleType::Optimizer),
            ModuleHealth::new(ModuleType::Dashboard),
        ];

        EvolutionCoordinator {
            session: EvolutionSession::new(0),
            module_health: modules,
            phase_history: [None; 100],
            messages: [None; 50],
            total_transitions: 0,
            total_messages: 0,
        }
    }

    /// Initialize coordinator
    pub fn initialize(&mut self) {
        self.session.start_time_ms = 0;  // Would be set to actual time
        for health in &mut self.module_health {
            health.status = ModuleStatus::Ready;
        }
    }

    /// Start evolution session
    pub fn start_session(&mut self, session_id: u32) {
        self.session = EvolutionSession::new(session_id);
        self.session.start_time_ms = 0;  // Would be set to actual time
        self.session.current_phase = EvolutionPhase::Profiling;
    }

    /// Transition to new phase
    pub fn transition_phase(&mut self, from: EvolutionPhase, to: EvolutionPhase) -> bool {
        let can_trans = self.can_transition(from, to);

        // Find empty slot
        for slot in &mut self.phase_history {
            if slot.is_none() {
                let mut transition = PhaseTransition::new(self.total_transitions, from, to);
                transition.success = can_trans;
                *slot = Some(transition);
                self.total_transitions += 1;

                if transition.success {
                    self.session.current_phase = to;
                }

                return transition.success;
            }
        }
        false
    }

    /// Check if transition is valid
    fn can_transition(&self, from: EvolutionPhase, to: EvolutionPhase) -> bool {
        // Valid transitions: Idle -> Profiling, Profiling -> Mutation, Mutation -> Testing,
        // Testing -> Selection, Selection -> Patching, Patching -> Learning, Learning -> Idle
        match (from, to) {
            (EvolutionPhase::Idle, EvolutionPhase::Profiling) => true,
            (EvolutionPhase::Profiling, EvolutionPhase::Mutation) => true,
            (EvolutionPhase::Mutation, EvolutionPhase::Testing) => true,
            (EvolutionPhase::Testing, EvolutionPhase::Selection) => true,
            (EvolutionPhase::Selection, EvolutionPhase::Patching) => true,
            (EvolutionPhase::Patching, EvolutionPhase::Learning) => true,
            (EvolutionPhase::Learning, EvolutionPhase::Idle) => true,
            // Emergency transitions
            (_, EvolutionPhase::Idle) => true,  // Can always return to idle
            _ => false,
        }
    }

    /// Send message between modules
    pub fn send_message(&mut self, message: ModuleMessage) -> bool {
        for slot in &mut self.messages {
            if slot.is_none() {
                *slot = Some(message);
                self.total_messages += 1;
                return true;
            }
        }
        false
    }

    /// Get next message for module
    pub fn get_message(&mut self, for_module: ModuleType) -> Option<ModuleMessage> {
        for slot in &mut self.messages {
            if let Some(msg) = slot {
                if msg.dest == for_module {
                    let msg_copy = *msg;
                    *slot = None;
                    return Some(msg_copy);
                }
            }
        }
        None
    }

    /// Update module health
    pub fn update_module_health(&mut self, module: ModuleType, success: bool) {
        for health in &mut self.module_health {
            if health.module == module {
                if success {
                    health.record_success();
                } else {
                    health.record_error();
                }
                return;
            }
        }
    }

    /// Get module health
    pub fn get_module_health(&self, module: ModuleType) -> Option<ModuleHealth> {
        for health in &self.module_health {
            if health.module == module {
                return Some(*health);
            }
        }
        None
    }

    /// Get overall system health (0-100)
    pub fn system_health(&self) -> u8 {
        let mut total = 0u32;
        let mut count = 0u32;

        for health in &self.module_health {
            total += health.health_percent() as u32;
            count += 1;
        }

        if count == 0 {
            50
        } else {
            (total / count) as u8
        }
    }

    /// Get all module statuses
    pub fn module_statuses(&self) -> [ModuleHealth; 7] {
        self.module_health
    }

    /// Update session metrics
    pub fn update_metrics(&mut self, aggregation: MetricAggregation) {
        self.session.metrics = aggregation;
    }

    /// Complete current cycle
    pub fn complete_cycle(&mut self) {
        self.session.total_cycles += 1;
    }

    /// Pause evolution
    pub fn pause(&mut self) -> bool {
        self.session.paused = true;
        true
    }

    /// Resume evolution
    pub fn resume(&mut self) -> bool {
        self.session.paused = false;
        true
    }

    /// Stop evolution session
    pub fn stop_session(&mut self) {
        self.session.current_phase = EvolutionPhase::Idle;
        self.session.paused = false;
    }

    /// Get current session
    pub fn current_session(&self) -> EvolutionSession {
        self.session
    }

    /// Get phase history
    pub fn phase_history(&self) -> [Option<PhaseTransition>; 100] {
        self.phase_history
    }

    /// Get coordinator statistics
    pub fn statistics(&self) -> (u32, u32, u8, u8) {
        let phase_transitions = self.total_transitions;
        let messages_processed = self.total_messages;
        let healthy_modules = self.module_health.iter().filter(|m| m.is_healthy()).count() as u8;
        let system_health = self.system_health();

        (phase_transitions, messages_processed, healthy_modules, system_health)
    }

    /// Get message queue size
    pub fn message_queue_size(&self) -> usize {
        self.messages.iter().filter(|s| s.is_some()).count()
    }

    /// Get phase history size
    pub fn phase_history_size(&self) -> usize {
        self.phase_history.iter().filter(|s| s.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evolution_phase_enum() {
        assert_eq!(EvolutionPhase::Idle as u8, 0);
        assert_eq!(EvolutionPhase::Patching as u8, 5);
        assert_eq!(EvolutionPhase::Learning as u8, 6);
    }

    #[test]
    fn test_evolution_phase_name() {
        assert_eq!(EvolutionPhase::Profiling.name(), "Profiling");
        assert_eq!(EvolutionPhase::Mutation.name(), "Mutation");
    }

    #[test]
    fn test_evolution_phase_is_active() {
        assert!(!EvolutionPhase::Idle.is_active());
        assert!(EvolutionPhase::Profiling.is_active());
    }

    #[test]
    fn test_module_status_enum() {
        assert_eq!(ModuleStatus::Uninitialized as u8, 0);
        assert_eq!(ModuleStatus::Error as u8, 4);
    }

    #[test]
    fn test_module_type_enum() {
        assert_eq!(ModuleType::Profiler as u8, 0);
        assert_eq!(ModuleType::Dashboard as u8, 6);
    }

    #[test]
    fn test_module_health_creation() {
        let health = ModuleHealth::new(ModuleType::Profiler);
        assert_eq!(health.module, ModuleType::Profiler);
        assert_eq!(health.status, ModuleStatus::Uninitialized);
    }

    #[test]
    fn test_module_health_percent() {
        let mut health = ModuleHealth::new(ModuleType::Profiler);
        health.success_count = 80;
        health.error_count = 20;
        assert_eq!(health.health_percent(), 80);
    }

    #[test]
    fn test_module_health_is_healthy() {
        let mut health = ModuleHealth::new(ModuleType::Profiler);
        health.success_count = 85;
        health.error_count = 15;
        assert!(health.is_healthy());
    }

    #[test]
    fn test_module_health_record_success() {
        let mut health = ModuleHealth::new(ModuleType::Profiler);
        health.record_success();
        assert_eq!(health.success_count, 1);
        assert_eq!(health.status, ModuleStatus::Ready);
    }

    #[test]
    fn test_module_health_record_error() {
        let mut health = ModuleHealth::new(ModuleType::Profiler);
        health.record_error();
        assert_eq!(health.error_count, 1);
        assert_eq!(health.status, ModuleStatus::Error);
    }

    #[test]
    fn test_phase_transition_creation() {
        let trans = PhaseTransition::new(1, EvolutionPhase::Idle, EvolutionPhase::Profiling);
        assert_eq!(trans.from_phase, EvolutionPhase::Idle);
        assert_eq!(trans.to_phase, EvolutionPhase::Profiling);
    }

    #[test]
    fn test_module_message_creation() {
        let msg = ModuleMessage::new(1, ModuleType::Profiler, ModuleType::Mutator, 0);
        assert_eq!(msg.source, ModuleType::Profiler);
        assert_eq!(msg.dest, ModuleType::Mutator);
    }

    #[test]
    fn test_module_message_with_size() {
        let msg = ModuleMessage::new(1, ModuleType::Profiler, ModuleType::Mutator, 0).with_size(256);
        assert_eq!(msg.payload_size, 256);
    }

    #[test]
    fn test_metric_aggregation_creation() {
        let agg = MetricAggregation::new(1, 100, 80);
        assert_eq!(agg.total_mutations, 100);
        assert_eq!(agg.successful_mutations, 80);
    }

    #[test]
    fn test_metric_aggregation_success_rate() {
        let agg = MetricAggregation::new(1, 100, 75);
        assert_eq!(agg.success_rate(), 75);
    }

    #[test]
    fn test_evolution_session_creation() {
        let session = EvolutionSession::new(1);
        assert_eq!(session.id, 1);
        assert_eq!(session.current_phase, EvolutionPhase::Idle);
    }

    #[test]
    fn test_evolution_session_elapsed_ms() {
        let mut session = EvolutionSession::new(1);
        session.start_time_ms = 1000;
        assert_eq!(session.elapsed_ms(2000), 1000);
    }

    #[test]
    fn test_evolution_coordinator_creation() {
        let coord = EvolutionCoordinator::new();
        assert_eq!(coord.total_transitions, 0);
        assert_eq!(coord.message_queue_size(), 0);
    }

    #[test]
    fn test_evolution_coordinator_initialize() {
        let mut coord = EvolutionCoordinator::new();
        coord.initialize();
        for health in coord.module_health {
            assert_eq!(health.status, ModuleStatus::Ready);
        }
    }

    #[test]
    fn test_evolution_coordinator_start_session() {
        let mut coord = EvolutionCoordinator::new();
        coord.start_session(1);
        assert_eq!(coord.session.id, 1);
        assert_eq!(coord.session.current_phase, EvolutionPhase::Profiling);
    }

    #[test]
    fn test_evolution_coordinator_transition_phase() {
        let mut coord = EvolutionCoordinator::new();
        assert!(coord.transition_phase(EvolutionPhase::Idle, EvolutionPhase::Profiling));
        assert_eq!(coord.session.current_phase, EvolutionPhase::Profiling);
    }

    #[test]
    fn test_evolution_coordinator_invalid_transition() {
        let mut coord = EvolutionCoordinator::new();
        assert!(!coord.transition_phase(EvolutionPhase::Testing, EvolutionPhase::Mutation));
    }

    #[test]
    fn test_evolution_coordinator_send_message() {
        let mut coord = EvolutionCoordinator::new();
        let msg = ModuleMessage::new(1, ModuleType::Profiler, ModuleType::Mutator, 0);
        assert!(coord.send_message(msg));
        assert_eq!(coord.message_queue_size(), 1);
    }

    #[test]
    fn test_evolution_coordinator_get_message() {
        let mut coord = EvolutionCoordinator::new();
        let msg = ModuleMessage::new(1, ModuleType::Profiler, ModuleType::Mutator, 0);
        coord.send_message(msg);
        let retrieved = coord.get_message(ModuleType::Mutator);
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_evolution_coordinator_update_module_health() {
        let mut coord = EvolutionCoordinator::new();
        coord.update_module_health(ModuleType::Profiler, true);
        let health = coord.get_module_health(ModuleType::Profiler);
        assert!(health.is_some());
        assert_eq!(health.unwrap().success_count, 1);
    }

    #[test]
    fn test_evolution_coordinator_system_health() {
        let mut coord = EvolutionCoordinator::new();
        coord.initialize();
        for _ in 0..5 {
            coord.update_module_health(ModuleType::Profiler, true);
        }
        assert!(coord.system_health() > 50);
    }

    #[test]
    fn test_evolution_coordinator_complete_cycle() {
        let mut coord = EvolutionCoordinator::new();
        coord.start_session(1);
        coord.complete_cycle();
        assert_eq!(coord.session.total_cycles, 1);
    }

    #[test]
    fn test_evolution_coordinator_pause_resume() {
        let mut coord = EvolutionCoordinator::new();
        coord.start_session(1);
        assert!(coord.pause());
        assert!(coord.session.paused);
        assert!(coord.resume());
        assert!(!coord.session.paused);
    }

    #[test]
    fn test_evolution_coordinator_stop_session() {
        let mut coord = EvolutionCoordinator::new();
        coord.start_session(1);
        coord.transition_phase(EvolutionPhase::Idle, EvolutionPhase::Profiling);
        coord.stop_session();
        assert_eq!(coord.session.current_phase, EvolutionPhase::Idle);
    }

    #[test]
    fn test_evolution_coordinator_statistics() {
        let mut coord = EvolutionCoordinator::new();
        coord.initialize();
        coord.start_session(1);
        coord.transition_phase(EvolutionPhase::Idle, EvolutionPhase::Profiling);

        let (transitions, messages, healthy, health) = coord.statistics();
        assert_eq!(transitions, 1);
        assert!(healthy > 0);
        assert!(health > 0);
    }

    #[test]
    fn test_evolution_coordinator_full_cycle() {
        let mut coord = EvolutionCoordinator::new();
        coord.initialize();
        coord.start_session(1);

        assert!(coord.transition_phase(EvolutionPhase::Idle, EvolutionPhase::Profiling));
        assert!(coord.transition_phase(EvolutionPhase::Profiling, EvolutionPhase::Mutation));
        assert!(coord.transition_phase(EvolutionPhase::Mutation, EvolutionPhase::Testing));
        assert!(coord.transition_phase(EvolutionPhase::Testing, EvolutionPhase::Selection));

        assert_eq!(coord.session.current_phase, EvolutionPhase::Selection);
    }

    #[test]
    fn test_evolution_coordinator_max_messages() {
        let mut coord = EvolutionCoordinator::new();

        // Send 50 messages
        for i in 0..50 {
            let msg = ModuleMessage::new(i, ModuleType::Profiler, ModuleType::Mutator, 0);
            assert!(coord.send_message(msg));
        }

        // 51st should fail
        let msg = ModuleMessage::new(50, ModuleType::Profiler, ModuleType::Mutator, 0);
        assert!(!coord.send_message(msg));
    }

    #[test]
    fn test_evolution_coordinator_message_routing() {
        let mut coord = EvolutionCoordinator::new();

        let msg1 = ModuleMessage::new(1, ModuleType::Profiler, ModuleType::Mutator, 0);
        let msg2 = ModuleMessage::new(2, ModuleType::Mutator, ModuleType::Selector, 0);

        coord.send_message(msg1);
        coord.send_message(msg2);

        // Mutator gets msg1
        let retrieved = coord.get_message(ModuleType::Mutator);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, 1);

        // Selector gets msg2
        let retrieved = coord.get_message(ModuleType::Selector);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, 2);
    }

    #[test]
    fn test_module_health_uninitialized_health() {
        let health = ModuleHealth::new(ModuleType::Profiler);
        assert_eq!(health.health_percent(), 50);  // Default uninitialized
    }

    #[test]
    fn test_evolution_phase_emergency_transition() {
        let mut coord = EvolutionCoordinator::new();
        coord.start_session(1);
        coord.transition_phase(EvolutionPhase::Idle, EvolutionPhase::Profiling);
        coord.transition_phase(EvolutionPhase::Profiling, EvolutionPhase::Mutation);

        // Emergency return to idle from Mutation
        assert!(coord.transition_phase(EvolutionPhase::Mutation, EvolutionPhase::Idle));
        assert_eq!(coord.session.current_phase, EvolutionPhase::Idle);
    }
}
