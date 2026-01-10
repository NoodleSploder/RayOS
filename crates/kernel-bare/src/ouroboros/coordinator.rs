//! Evolution Coordinator: Main Evolution Loop Orchestrator
//!
//! This module implements the central orchestrator that coordinates all phases of
//! the self-evolution process. It manages the complete mutation lifecycle from
//! genome analysis through mutation creation, testing, selection, and patching.
//!
//! # Architecture
//!
//! The evolution loop executes in phases:
//! 1. **Analysis** - Genome repository identifies optimization opportunities
//! 2. **Generation** - Mutation engine creates code variations
//! 3. **Testing** - Selection arena validates mutations in sandbox
//! 4. **Selection** - Tournament selector identifies winners
//! 5. **Patching** - Live patcher applies winning mutations
//! 6. **Monitoring** - Evolution coordinator tracks outcomes
//!
//! # Boot Markers
//!
//! - `RAYOS_OUROBOROS:EVOLUTION_START` - Evolution cycle initiated
//! - `RAYOS_OUROBOROS:EVOLUTION_COMPLETE` - Evolution cycle finished
//! - `RAYOS_OUROBOROS:EVOLUTION_APPROVED` - Mutation approved for live patching

use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use crate::ouroboros::{EvolutionResult, ApprovalMode, MutationSeverity, PowerState};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum mutations per evolution cycle
pub const MAX_MUTATIONS_PER_CYCLE: usize = 32;

/// Maximum entries in mutation history
pub const MAX_MUTATION_HISTORY: usize = 256;

/// Maximum entries in winner registry
pub const MAX_WINNERS: usize = 64;

/// Maximum entries in approval queue
pub const MAX_APPROVAL_QUEUE: usize = 32;

/// Evolution timeout (milliseconds)
pub const EVOLUTION_TIMEOUT_MS: u64 = 120000; // 2 minutes

// ============================================================================
// MUTATION HISTORY
// ============================================================================

/// Historical record of applied mutations
#[derive(Clone, Copy, Debug)]
pub struct HistoryEntry {
    /// Entry ID
    pub id: u32,
    /// Mutation ID
    pub mutation_id: u32,
    /// Timestamp of mutation
    pub timestamp: u64,
    /// Mutation severity
    pub severity: MutationSeverity,
    /// Was mutation approved?
    pub was_approved: bool,
    /// Improvement achieved (basis points, 1 = 0.01%)
    pub improvement_basis_points: i32,
    /// Status of mutation
    pub status: HistoryStatus,
}

/// Status of historical mutation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HistoryStatus {
    /// Mutation applied and stable
    Applied = 0,
    /// Mutation rolled back
    RolledBack = 1,
    /// Mutation rejected by tests
    Rejected = 2,
    /// Mutation pending evaluation
    Pending = 3,
}

impl HistoryEntry {
    /// Create new history entry
    pub fn new(id: u32, mutation_id: u32, severity: MutationSeverity) -> Self {
        Self {
            id,
            mutation_id,
            timestamp: 0,
            severity,
            was_approved: false,
            improvement_basis_points: 0,
            status: HistoryStatus::Pending,
        }
    }

    /// Mark as applied
    pub fn mark_applied(&mut self) {
        self.status = HistoryStatus::Applied;
    }

    /// Mark as rolled back
    pub fn mark_rolled_back(&mut self) {
        self.status = HistoryStatus::RolledBack;
    }

    /// Mark as rejected
    pub fn mark_rejected(&mut self) {
        self.status = HistoryStatus::Rejected;
    }

    /// Set improvement value
    pub fn set_improvement(&mut self, basis_points: i32) {
        self.improvement_basis_points = basis_points;
    }
}

/// Mutation history log
#[derive(Clone, Copy, Debug)]
pub struct MutationHistory {
    /// Number of entries
    pub entry_count: u32,
    /// Total mutations applied
    pub applied_count: u32,
    /// Total mutations rejected
    pub rejected_count: u32,
    /// Total mutations rolled back
    pub rollback_count: u32,
    /// Total accumulated improvement (basis points)
    pub total_improvement_basis_points: i64,
}

impl MutationHistory {
    /// Create new mutation history
    pub fn new() -> Self {
        Self {
            entry_count: 0,
            applied_count: 0,
            rejected_count: 0,
            rollback_count: 0,
            total_improvement_basis_points: 0,
        }
    }

    /// Record entry
    pub fn record(&mut self, entry: &HistoryEntry) {
        self.entry_count = self.entry_count.saturating_add(1);

        match entry.status {
            HistoryStatus::Applied => {
                self.applied_count = self.applied_count.saturating_add(1);
            }
            HistoryStatus::Rejected => {
                self.rejected_count = self.rejected_count.saturating_add(1);
            }
            HistoryStatus::RolledBack => {
                self.rollback_count = self.rollback_count.saturating_add(1);
            }
            HistoryStatus::Pending => {}
        }

        self.total_improvement_basis_points =
            self.total_improvement_basis_points.saturating_add(entry.improvement_basis_points as i64);
    }

    /// Get success rate
    pub fn success_rate(&self) -> f32 {
        if self.entry_count == 0 {
            return 0.0;
        }
        (self.applied_count as f32 / self.entry_count as f32) * 100.0
    }

    /// Get average improvement
    pub fn avg_improvement_basis_points(&self) -> f32 {
        if self.applied_count == 0 {
            return 0.0;
        }
        self.total_improvement_basis_points as f32 / self.applied_count as f32
    }

    /// Get average improvement as percentage
    pub fn avg_improvement_percent(&self) -> f32 {
        self.avg_improvement_basis_points() / 100.0
    }
}

impl Default for MutationHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// WINNER REGISTRY
// ============================================================================

/// Record of a winning mutation
#[derive(Clone, Copy, Debug)]
pub struct Winner {
    /// Winner ID
    pub id: u32,
    /// Mutation ID
    pub mutation_id: u32,
    /// Fitness score (0-255)
    pub fitness_score: u8,
    /// Speedup achieved (1.0 = no change, 2.0 = 2x faster)
    pub speedup: f32,
    /// Memory improvement (1.0 = no change, 1.5 = 50% less memory)
    pub memory_improvement: f32,
    /// Timestamp of win
    pub timestamp: u64,
    /// Is patched to live system?
    pub patched: bool,
}

impl Winner {
    /// Create new winner record
    pub fn new(id: u32, mutation_id: u32, fitness_score: u8) -> Self {
        Self {
            id,
            mutation_id,
            fitness_score,
            speedup: 1.0,
            memory_improvement: 1.0,
            timestamp: 0,
            patched: false,
        }
    }

    /// Mark as patched
    pub fn mark_patched(&mut self) {
        self.patched = true;
    }
}

/// Registry of winning mutations
#[derive(Clone, Copy, Debug)]
pub struct WinnerRegistry {
    /// Number of winners
    pub winner_count: u32,
    /// Total winners ever recorded
    pub total_winners: u32,
    /// Average fitness score of winners
    pub avg_fitness_score: u8,
    /// Highest fitness score seen
    pub highest_fitness_score: u8,
    /// Total winners patched
    pub patched_count: u32,
}

impl WinnerRegistry {
    /// Create new winner registry
    pub fn new() -> Self {
        Self {
            winner_count: 0,
            total_winners: 0,
            avg_fitness_score: 0,
            highest_fitness_score: 0,
            patched_count: 0,
        }
    }

    /// Record a winner
    pub fn record_winner(&mut self, winner: &Winner) -> Result<(), EvolutionResult> {
        if self.winner_count >= MAX_WINNERS as u32 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }

        self.winner_count = self.winner_count.saturating_add(1);
        self.total_winners = self.total_winners.saturating_add(1);
        self.highest_fitness_score = self.highest_fitness_score.max(winner.fitness_score);

        // Update average fitness score
        let total_fitness = (self.avg_fitness_score as u32 * (self.winner_count - 1) as u32)
            + winner.fitness_score as u32;
        self.avg_fitness_score = (total_fitness / self.winner_count as u32) as u8;

        Ok(())
    }

    /// Record patch application
    pub fn record_patch(&mut self) {
        self.patched_count = self.patched_count.saturating_add(1);
    }

    /// Get patch rate
    pub fn patch_rate(&self) -> f32 {
        if self.total_winners == 0 {
            return 0.0;
        }
        (self.patched_count as f32 / self.total_winners as f32) * 100.0
    }
}

impl Default for WinnerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// APPROVAL QUEUE
// ============================================================================

/// Pending mutation awaiting approval
#[derive(Clone, Copy, Debug)]
pub struct PendingApproval {
    /// Pending ID
    pub id: u32,
    /// Mutation ID
    pub mutation_id: u32,
    /// Severity level
    pub severity: MutationSeverity,
    /// Timestamp queued
    pub timestamp: u64,
    /// Requires approval?
    pub requires_approval: bool,
}

impl PendingApproval {
    /// Create new pending approval
    pub fn new(id: u32, mutation_id: u32, severity: MutationSeverity) -> Self {
        Self {
            id,
            mutation_id,
            severity,
            timestamp: 0,
            requires_approval: false,
        }
    }
}

/// Queue of mutations pending approval
#[derive(Clone, Copy, Debug)]
pub struct ApprovalQueue {
    /// Number of pending approvals
    pub pending_count: u32,
    /// Total processed
    pub processed_count: u32,
    /// Total approved
    pub approved_count: u32,
    /// Total rejected
    pub rejected_count: u32,
}

impl ApprovalQueue {
    /// Create new approval queue
    pub fn new() -> Self {
        Self {
            pending_count: 0,
            processed_count: 0,
            approved_count: 0,
            rejected_count: 0,
        }
    }

    /// Add pending approval
    pub fn enqueue(&mut self) -> Result<(), EvolutionResult> {
        if self.pending_count >= MAX_APPROVAL_QUEUE as u32 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.pending_count = self.pending_count.saturating_add(1);
        Ok(())
    }

    /// Record approval decision
    pub fn process_decision(&mut self, approved: bool) {
        if self.pending_count > 0 {
            self.pending_count = self.pending_count.saturating_sub(1);
        }
        self.processed_count = self.processed_count.saturating_add(1);

        if approved {
            self.approved_count = self.approved_count.saturating_add(1);
        } else {
            self.rejected_count = self.rejected_count.saturating_add(1);
        }
    }

    /// Get approval rate
    pub fn approval_rate(&self) -> f32 {
        if self.processed_count == 0 {
            return 0.0;
        }
        (self.approved_count as f32 / self.processed_count as f32) * 100.0
    }
}

impl Default for ApprovalQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// EVOLUTION CONFIG
// ============================================================================

/// Configuration for evolution process
#[derive(Clone, Copy, Debug)]
pub struct EvolutionConfig {
    /// Approval mode for mutations
    pub approval_mode: ApprovalMode,
    /// Maximum mutations per cycle
    pub max_mutations_per_cycle: u32,
    /// Evolution timeout (milliseconds)
    pub timeout_ms: u64,
    /// Power state constraint
    pub power_state: PowerState,
    /// Is evolution enabled?
    pub enabled: bool,
}

impl EvolutionConfig {
    /// Create new evolution config
    pub fn new() -> Self {
        Self {
            approval_mode: ApprovalMode::Notify,
            max_mutations_per_cycle: MAX_MUTATIONS_PER_CYCLE as u32,
            timeout_ms: EVOLUTION_TIMEOUT_MS,
            power_state: PowerState::AcPower,
            enabled: true,
        }
    }

    /// Set approval mode
    pub fn set_approval_mode(&mut self, mode: ApprovalMode) {
        self.approval_mode = mode;
    }

    /// Set power state
    pub fn set_power_state(&mut self, state: PowerState) {
        self.power_state = state;
    }

    /// Check if evolution is allowed
    pub fn is_allowed(&self) -> bool {
        self.enabled && self.power_state.evolution_allowed()
    }
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// OUROBOROS ENGINE
// ============================================================================

/// Main evolution engine orchestrator
pub struct OuroborosEngine {
    /// Engine ID
    pub id: u64,
    /// Configuration
    pub config: EvolutionConfig,
    /// Mutation history
    pub history: MutationHistory,
    /// Winner registry
    pub winners: WinnerRegistry,
    /// Approval queue
    pub approval_queue: ApprovalQueue,
    /// Total evolution cycles executed
    pub evolution_cycles: u32,
    /// Current cycle mutations
    pub current_cycle_mutations: u32,
    /// Total mutations processed
    pub total_mutations_processed: u32,
}

impl OuroborosEngine {
    /// Create new Ouroboros engine
    pub fn new(id: u64) -> Self {
        Self {
            id,
            config: EvolutionConfig::new(),
            history: MutationHistory::new(),
            winners: WinnerRegistry::new(),
            approval_queue: ApprovalQueue::new(),
            evolution_cycles: 0,
            current_cycle_mutations: 0,
            total_mutations_processed: 0,
        }
    }

    /// Start new evolution cycle
    pub fn start_evolution_cycle(&mut self) -> Result<(), EvolutionResult> {
        if !self.config.is_allowed() {
            return Err(EvolutionResult::InternalError);
        }

        self.evolution_cycles = self.evolution_cycles.saturating_add(1);
        self.current_cycle_mutations = 0;

        Ok(())
    }

    /// Complete evolution cycle
    pub fn complete_evolution_cycle(&mut self) {
        self.evolution_cycles = self.evolution_cycles.saturating_sub(1).saturating_add(1);
    }

    /// Process mutation result
    pub fn process_mutation_result(
        &mut self,
        accepted: bool,
        improvement: i32,
        severity: MutationSeverity,
    ) -> Result<(), EvolutionResult> {
        self.total_mutations_processed = self.total_mutations_processed.saturating_add(1);

        if accepted {
            let mut entry = HistoryEntry::new(
                self.total_mutations_processed,
                self.total_mutations_processed,
                severity,
            );
            entry.mark_applied();
            entry.set_improvement(improvement);
            self.history.record(&entry);
        }

        Ok(())
    }

    /// Queue mutation for approval
    pub fn queue_for_approval(&mut self, severity: MutationSeverity) -> Result<(), EvolutionResult> {
        let requires_approval = severity.requires_approval(self.config.approval_mode);

        if requires_approval {
            self.approval_queue.enqueue()?;
        }

        Ok(())
    }

    /// Get overall evolution statistics
    pub fn get_statistics(&self) -> EvolutionStatistics {
        EvolutionStatistics {
            evolution_cycles: self.evolution_cycles,
            total_mutations_processed: self.total_mutations_processed,
            mutations_applied: self.history.applied_count,
            mutations_rejected: self.history.rejected_count,
            winners_total: self.winners.total_winners,
            winners_patched: self.winners.patched_count,
            success_rate: self.history.success_rate(),
            avg_improvement_percent: self.history.avg_improvement_percent(),
        }
    }

    /// Enable/disable evolution
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }
}

/// Ouroboros evolution statistics
#[derive(Clone, Copy, Debug)]
pub struct EvolutionStatistics {
    /// Evolution cycles completed
    pub evolution_cycles: u32,
    /// Total mutations processed
    pub total_mutations_processed: u32,
    /// Mutations applied
    pub mutations_applied: u32,
    /// Mutations rejected
    pub mutations_rejected: u32,
    /// Total winners
    pub winners_total: u32,
    /// Winners patched
    pub winners_patched: u32,
    /// Success rate percentage
    pub success_rate: f32,
    /// Average improvement percentage
    pub avg_improvement_percent: f32,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_entry_creation() {
        let entry = HistoryEntry::new(1, 1, MutationSeverity::Minor);
        assert_eq!(entry.id, 1);
        assert_eq!(entry.mutation_id, 1);
        assert_eq!(entry.status, HistoryStatus::Pending);
    }

    #[test]
    fn test_history_entry_status_transitions() {
        let mut entry = HistoryEntry::new(1, 1, MutationSeverity::Minor);
        entry.mark_applied();
        assert_eq!(entry.status, HistoryStatus::Applied);

        let mut entry2 = HistoryEntry::new(2, 2, MutationSeverity::Minor);
        entry2.mark_rejected();
        assert_eq!(entry2.status, HistoryStatus::Rejected);
    }

    #[test]
    fn test_history_entry_improvement() {
        let mut entry = HistoryEntry::new(1, 1, MutationSeverity::Minor);
        entry.set_improvement(150); // 1.5% improvement
        assert_eq!(entry.improvement_basis_points, 150);
    }

    #[test]
    fn test_mutation_history_creation() {
        let hist = MutationHistory::new();
        assert_eq!(hist.entry_count, 0);
        assert_eq!(hist.applied_count, 0);
    }

    #[test]
    fn test_mutation_history_recording() {
        let mut hist = MutationHistory::new();
        let mut entry = HistoryEntry::new(1, 1, MutationSeverity::Minor);
        entry.mark_applied();
        entry.set_improvement(100);

        hist.record(&entry);
        assert_eq!(hist.entry_count, 1);
        assert_eq!(hist.applied_count, 1);
        assert_eq!(hist.total_improvement_basis_points, 100);
    }

    #[test]
    fn test_mutation_history_success_rate() {
        let mut hist = MutationHistory::new();
        hist.applied_count = 8;
        hist.entry_count = 10;
        hist.rejected_count = 2;
        assert!((hist.success_rate() - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_winner_creation() {
        let winner = Winner::new(1, 1, 200);
        assert_eq!(winner.id, 1);
        assert_eq!(winner.fitness_score, 200);
        assert!(!winner.patched);
    }

    #[test]
    fn test_winner_patched() {
        let mut winner = Winner::new(1, 1, 200);
        winner.mark_patched();
        assert!(winner.patched);
    }

    #[test]
    fn test_winner_registry_creation() {
        let reg = WinnerRegistry::new();
        assert_eq!(reg.winner_count, 0);
        assert_eq!(reg.highest_fitness_score, 0);
    }

    #[test]
    fn test_winner_registry_record() {
        let mut reg = WinnerRegistry::new();
        let winner = Winner::new(1, 1, 200);
        reg.record_winner(&winner).unwrap();

        assert_eq!(reg.winner_count, 1);
        assert_eq!(reg.total_winners, 1);
        assert_eq!(reg.highest_fitness_score, 200);
    }

    #[test]
    fn test_winner_registry_patch_rate() {
        let mut reg = WinnerRegistry::new();
        let winner = Winner::new(1, 1, 200);
        reg.record_winner(&winner).unwrap();
        reg.record_patch();
        reg.record_patch();

        assert_eq!(reg.patched_count, 2);
        assert!((reg.patch_rate() - 200.0).abs() < 0.01); // 2 patches for 1 winner
    }

    #[test]
    fn test_pending_approval_creation() {
        let pa = PendingApproval::new(1, 1, MutationSeverity::Minor);
        assert_eq!(pa.id, 1);
        assert_eq!(pa.mutation_id, 1);
    }

    #[test]
    fn test_approval_queue_creation() {
        let queue = ApprovalQueue::new();
        assert_eq!(queue.pending_count, 0);
        assert_eq!(queue.processed_count, 0);
    }

    #[test]
    fn test_approval_queue_enqueue() {
        let mut queue = ApprovalQueue::new();
        queue.enqueue().unwrap();
        queue.enqueue().unwrap();
        assert_eq!(queue.pending_count, 2);
    }

    #[test]
    fn test_approval_queue_process() {
        let mut queue = ApprovalQueue::new();
        queue.enqueue().unwrap();
        queue.process_decision(true);

        assert_eq!(queue.pending_count, 0);
        assert_eq!(queue.processed_count, 1);
        assert_eq!(queue.approved_count, 1);
    }

    #[test]
    fn test_approval_queue_approval_rate() {
        let mut queue = ApprovalQueue::new();
        queue.enqueue().unwrap();
        queue.enqueue().unwrap();
        queue.process_decision(true);
        queue.process_decision(false);

        assert!((queue.approval_rate() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_evolution_config_creation() {
        let config = EvolutionConfig::new();
        assert_eq!(config.approval_mode, ApprovalMode::Notify);
        assert!(config.enabled);
    }

    #[test]
    fn test_evolution_config_allowed() {
        let config = EvolutionConfig::new();
        assert!(config.is_allowed());

        let mut config2 = EvolutionConfig::new();
        config2.enabled = false;
        assert!(!config2.is_allowed());
    }

    #[test]
    fn test_evolution_config_power_state() {
        let mut config = EvolutionConfig::new();
        config.set_power_state(PowerState::BatteryCritical);
        assert!(!config.is_allowed());
    }

    #[test]
    fn test_ouroboros_engine_creation() {
        let engine = OuroborosEngine::new(1);
        assert_eq!(engine.id, 1);
        assert_eq!(engine.evolution_cycles, 0);
    }

    #[test]
    fn test_ouroboros_engine_start_cycle() {
        let mut engine = OuroborosEngine::new(1);
        engine.start_evolution_cycle().unwrap();
        assert_eq!(engine.evolution_cycles, 1);
    }

    #[test]
    fn test_ouroboros_engine_process_mutation() {
        let mut engine = OuroborosEngine::new(1);
        engine.process_mutation_result(true, 150, MutationSeverity::Minor).unwrap();

        assert_eq!(engine.total_mutations_processed, 1);
        assert_eq!(engine.history.applied_count, 1);
    }

    #[test]
    fn test_ouroboros_engine_queue_approval() {
        let mut engine = OuroborosEngine::new(1);
        engine.set_enabled(true);
        engine.config.set_approval_mode(ApprovalMode::ApproveMajor);

        // Minor mutations don't require approval in ApproveMajor mode
        engine.queue_for_approval(MutationSeverity::Minor).unwrap();
        assert_eq!(engine.approval_queue.pending_count, 0);

        // Major mutations do require approval
        engine.queue_for_approval(MutationSeverity::Major).unwrap();
        assert_eq!(engine.approval_queue.pending_count, 1);
    }

    #[test]
    fn test_ouroboros_engine_disable() {
        let mut engine = OuroborosEngine::new(1);
        engine.set_enabled(false);
        let result = engine.start_evolution_cycle();
        assert!(result.is_err());
    }
}
