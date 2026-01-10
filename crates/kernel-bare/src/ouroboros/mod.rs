//! The Ouroboros Engine: RayOS's Self-Evolving Metabolism
//!
//! Named after the ancient symbol of a serpent eating its own tail, this module
//! embodies the principle that RayOS should never be static—it is a living system
//! that continuously evolves into better versions of itself.
//!
//! # Core Philosophy: The "No Idle Principle"
//!
//! When RayOS is not actively serving the user, it does not sleep—it dreams.
//! During dream mode, the Ouroboros Engine activates, mutating its own code,
//! testing variations in sandboxes, and live-patching the winners.
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                        OUROBOROS ENGINE                              │
//! │                   "The System That Evolves Itself"                   │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │                                                                      │
//! │  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────────┐  │
//! │  │   GENOME         │  │    MUTATION      │  │    SELECTION      │  │
//! │  │   REPOSITORY     │  │    ENGINE        │  │    ARENA          │  │
//! │  │                  │  │                  │  │                   │  │
//! │  │  Source code     │  │  Code           │  │  Sandbox          │  │
//! │  │  as mutable      │──►  transformation  │──►  testing &        │  │
//! │  │  genome          │  │  & variation     │  │  fitness scoring  │  │
//! │  └────────▲─────────┘  └─────────────────┘  └─────────┬─────────┘  │
//! │           │                                           │             │
//! │           │            ┌─────────────────┐            │             │
//! │           └────────────┤   LIVE PATCHER  │◄───────────┘             │
//! │                        │                 │                          │
//! │                        │  Hot-swap       │                          │
//! │                        │  winning        │                          │
//! │                        │  mutations      │                          │
//! │                        └────────┬────────┘                          │
//! │                                 │                                   │
//! │  ┌──────────────────────────────▼───────────────────────────────┐  │
//! │  │                    DREAM SCHEDULER                            │  │
//! │  │  Monitors user activity → Triggers evolution during idle     │  │
//! │  └───────────────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Modules
//!
//! - [`genome`]: Source code as mutable genome with AST representation
//! - [`mutation`]: Code transformation and variation generation
//! - [`selection`]: Sandbox testing and fitness scoring
//! - [`patcher`]: Hot-swap winning mutations without reboot
//! - [`scheduler`]: Idle detection and dream mode scheduling
//! - [`coordinator`]: Central orchestrator for the evolution loop
//!
//! # Safety Guarantees
//!
//! 1. **Isolated Testing**: All mutations are tested in sandboxes before affecting live code
//! 2. **Full Reversibility**: Complete rollback log enables instant reversion
//! 3. **Safe Patch Points**: Code only patched at safe points between syscalls
//! 4. **User Control**: Configurable approval modes from automatic to fully manual
//!
//! # Integration with Sentient Substrate
//!
//! The Ouroboros Engine integrates with other pillars:
//! - **Bicameral Kernel**: System 2 suggests intelligent mutations, System 1 monitors metrics
//! - **Neural File System**: Vector Store tracks mutation history, Epiphany suggests improvements
//! - **Logic as Geometry**: Geometric fitness landscapes for multi-objective optimization
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use ouroboros::{OuroborosEngine, EvolutionConfig, ApprovalMode};
//!
//! // Create the Ouroboros Engine
//! let config = EvolutionConfig {
//!     approval_mode: ApprovalMode::ApproveMajor,
//!     idle_threshold_seconds: 300, // 5 minutes
//!     max_mutations_per_session: 10,
//!     ..Default::default()
//! };
//!
//! let engine = OuroborosEngine::new(config);
//!
//! // Start monitoring (runs in background)
//! engine.start_monitoring();
//!
//! // Query evolution statistics
//! let stats = engine.stats();
//! println!("Mutations applied: {}", stats.successful_mutations);
//! println!("Performance gain: {:.1}%", stats.cumulative_improvement_percent);
//! ```
//!
//! # Boot Markers
//!
//! The Ouroboros Engine emits the following markers:
//! - `RAYOS_OUROBOROS:INITIALIZED` - Engine initialized and ready
//! - `RAYOS_OUROBOROS:DREAM_STARTED` - Dream mode evolution session begun
//! - `RAYOS_OUROBOROS:DREAM_ENDED` - Dream mode session complete
//! - `RAYOS_OUROBOROS:MUTATION_APPLIED` - Successful mutation applied to live code
//! - `RAYOS_OUROBOROS:ROLLBACK` - Mutation reverted due to failure

#![allow(dead_code)]

pub mod genome;
pub mod mutation;   // Phase 31, Task 2
pub mod selection;  // Phase 31, Task 3
// pub mod patcher;      // Phase 31, Task 4
// pub mod scheduler;    // Phase 31, Task 5
// pub mod coordinator;  // Phase 31, Task 6

pub use genome::{
    SourceGenome, GenomeRegion, AstNode, AstNodeType, DependencyGraph, HotspotTracker, Hotspot,
    MutationPoint, MutationType, GenomeChecksum,
};
pub use mutation::{
    Mutator, MutationCandidate, MutationBatch, MutationStatus, BatchStatus,
    RefactoringOp, RefactoringType, OptimizationOp, OptimizationType,
    MutationStrategy, LlmGuidedMutator,
};
pub use selection::{
    Sandbox, SandboxStatus, TestSuite, TestCase, TestCategory, BenchmarkSuite, Benchmark,
    FitnessMetric, MetricType, FitnessScore, TournamentSelector, SelectionResult,
};

// ============================================================================
// COMMON TYPES AND CONSTANTS
// ============================================================================

/// Boot marker prefix for Ouroboros Engine events
pub const MARKER_PREFIX: &str = "RAYOS_OUROBOROS";

/// Version of the Ouroboros Engine protocol
pub const ENGINE_VERSION: u32 = 1;

/// Maximum number of concurrent mutations being tested
pub const MAX_CONCURRENT_MUTATIONS: usize = 16;

/// Maximum mutation history entries retained
pub const MAX_MUTATION_HISTORY: usize = 4096;

/// Default idle threshold before entering dream mode (5 minutes)
pub const DEFAULT_IDLE_THRESHOLD_MS: u64 = 5 * 60 * 1000;

/// Minimum idle threshold (30 seconds)
pub const MIN_IDLE_THRESHOLD_MS: u64 = 30 * 1000;

/// Maximum idle threshold (1 hour)
pub const MAX_IDLE_THRESHOLD_MS: u64 = 60 * 60 * 1000;

/// Grace period after patch before automatic rollback on failure (10 seconds)
pub const PATCH_GRACE_PERIOD_MS: u64 = 10 * 1000;

// ============================================================================
// COMMON ENUMS
// ============================================================================

/// Result of an evolution operation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EvolutionResult {
    /// Operation succeeded
    Success = 0,
    /// Operation is pending (async)
    Pending = 1,
    /// Mutation was rejected by tests
    RejectedByTests = 2,
    /// Mutation was rejected by user
    RejectedByUser = 3,
    /// Mutation caused regression
    RegressionDetected = 4,
    /// Mutation was rolled back
    RolledBack = 5,
    /// Resource limit exceeded
    ResourceLimitExceeded = 6,
    /// Internal error
    InternalError = 7,
}

/// User approval modes for mutations
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ApprovalMode {
    /// All passing mutations applied automatically (power user mode)
    Automatic = 0,
    /// Applied automatically, user notified of changes
    #[default]
    Notify = 1,
    /// Minor refactors automatic, major changes need approval
    ApproveMajor = 2,
    /// Every mutation requires explicit user approval
    ApproveAll = 3,
}

/// Classification of mutation severity
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum MutationSeverity {
    /// Trivial change (whitespace, comments)
    Trivial = 0,
    /// Minor refactoring (variable rename, extract constant)
    Minor = 1,
    /// Moderate change (extract function, inline)
    Moderate = 2,
    /// Major change (algorithm replacement, API change)
    Major = 3,
    /// Critical change (core system modification)
    Critical = 4,
}

impl MutationSeverity {
    /// Returns whether this severity requires approval given the mode
    pub fn requires_approval(self, mode: ApprovalMode) -> bool {
        match mode {
            ApprovalMode::Automatic => false,
            ApprovalMode::Notify => false,
            ApprovalMode::ApproveMajor => self >= MutationSeverity::Major,
            ApprovalMode::ApproveAll => true,
        }
    }
}

/// Power source state for power-aware evolution
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PowerState {
    /// AC power connected
    AcPower = 0,
    /// Battery above 50%
    BatteryHigh = 1,
    /// Battery between 20% and 50%
    BatteryMedium = 2,
    /// Battery between 5% and 20%
    BatteryLow = 3,
    /// Battery below 5%
    BatteryCritical = 4,
}

impl PowerState {
    /// Returns the evolution budget multiplier for this power state
    pub fn budget_multiplier(self) -> f32 {
        match self {
            PowerState::AcPower => 1.0,
            PowerState::BatteryHigh => 0.7,
            PowerState::BatteryMedium => 0.4,
            PowerState::BatteryLow => 0.1,
            PowerState::BatteryCritical => 0.0,
        }
    }

    /// Returns whether evolution is allowed in this power state
    pub fn evolution_allowed(self) -> bool {
        self != PowerState::BatteryCritical
    }
}

// ============================================================================
// COMMON TRAITS
// ============================================================================

/// Trait for components that can emit boot markers
pub trait MarkerEmitter {
    /// Emit a boot marker with the given event name
    fn emit_marker(&self, event: &str);

    /// Emit a boot marker with event name and payload
    fn emit_marker_with_payload(&self, event: &str, payload: &str);
}

/// Trait for components that can be checkpointed and restored
pub trait Checkpointable {
    /// Create a checkpoint of current state
    fn checkpoint(&self) -> Result<Checkpoint, EvolutionResult>;

    /// Restore from a checkpoint
    fn restore(&mut self, checkpoint: &Checkpoint) -> Result<(), EvolutionResult>;
}

/// A checkpoint of component state
#[derive(Clone)]
pub struct Checkpoint {
    /// Unique identifier for this checkpoint
    pub id: u64,
    /// Timestamp when checkpoint was created
    pub timestamp: u64,
    /// Serialized state data
    pub data: CheckpointData,
}

/// Checkpoint data storage (fixed-size for no_std)
#[derive(Clone)]
pub struct CheckpointData {
    /// Raw checkpoint bytes
    bytes: [u8; 4096],
    /// Actual length of valid data
    len: usize,
}

impl CheckpointData {
    /// Create empty checkpoint data
    pub const fn new() -> Self {
        Self {
            bytes: [0u8; 4096],
            len: 0,
        }
    }

    /// Get the data as a slice
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Set data from a slice
    pub fn set(&mut self, data: &[u8]) -> Result<(), EvolutionResult> {
        if data.len() > self.bytes.len() {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.bytes[..data.len()].copy_from_slice(data);
        self.len = data.len();
        Ok(())
    }
}

impl Default for CheckpointData {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// STATISTICS
// ============================================================================

/// Global statistics for the Ouroboros Engine
#[derive(Clone, Copy, Debug, Default)]
pub struct EvolutionStats {
    /// Total mutations attempted
    pub mutations_attempted: u64,
    /// Mutations that passed testing
    pub mutations_passed: u64,
    /// Mutations applied to live code
    pub mutations_applied: u64,
    /// Mutations rolled back
    pub mutations_rolled_back: u64,
    /// Mutations pending user approval
    pub mutations_pending_approval: u32,
    /// Dream mode sessions completed
    pub dream_sessions: u64,
    /// Total time spent in dream mode (ms)
    pub dream_time_ms: u64,
    /// Cumulative performance improvement estimate (basis points, 100 = 1%)
    pub improvement_basis_points: i32,
    /// Last dream session timestamp
    pub last_dream_timestamp: u64,
}

impl EvolutionStats {
    /// Returns the mutation success rate as a percentage
    pub fn success_rate_percent(&self) -> f32 {
        if self.mutations_attempted == 0 {
            return 0.0;
        }
        (self.mutations_applied as f32 / self.mutations_attempted as f32) * 100.0
    }

    /// Returns the cumulative improvement as a percentage
    pub fn improvement_percent(&self) -> f32 {
        self.improvement_basis_points as f32 / 100.0
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutation_severity_approval() {
        assert!(!MutationSeverity::Trivial.requires_approval(ApprovalMode::Automatic));
        assert!(!MutationSeverity::Critical.requires_approval(ApprovalMode::Automatic));

        assert!(!MutationSeverity::Trivial.requires_approval(ApprovalMode::Notify));
        assert!(!MutationSeverity::Critical.requires_approval(ApprovalMode::Notify));

        assert!(!MutationSeverity::Minor.requires_approval(ApprovalMode::ApproveMajor));
        assert!(!MutationSeverity::Moderate.requires_approval(ApprovalMode::ApproveMajor));
        assert!(MutationSeverity::Major.requires_approval(ApprovalMode::ApproveMajor));
        assert!(MutationSeverity::Critical.requires_approval(ApprovalMode::ApproveMajor));

        assert!(MutationSeverity::Trivial.requires_approval(ApprovalMode::ApproveAll));
        assert!(MutationSeverity::Critical.requires_approval(ApprovalMode::ApproveAll));
    }

    #[test]
    fn test_power_state_budget() {
        assert!((PowerState::AcPower.budget_multiplier() - 1.0).abs() < 0.001);
        assert!((PowerState::BatteryCritical.budget_multiplier() - 0.0).abs() < 0.001);
        assert!(PowerState::AcPower.evolution_allowed());
        assert!(!PowerState::BatteryCritical.evolution_allowed());
    }

    #[test]
    fn test_evolution_stats() {
        let mut stats = EvolutionStats::default();
        stats.mutations_attempted = 100;
        stats.mutations_applied = 75;
        stats.improvement_basis_points = 350; // 3.5%

        assert!((stats.success_rate_percent() - 75.0).abs() < 0.001);
        assert!((stats.improvement_percent() - 3.5).abs() < 0.001);
    }

    #[test]
    fn test_checkpoint_data() {
        let mut data = CheckpointData::new();
        assert_eq!(data.as_slice().len(), 0);

        let test_bytes = [1u8, 2, 3, 4, 5];
        data.set(&test_bytes).unwrap();
        assert_eq!(data.as_slice(), &test_bytes);
    }
}
