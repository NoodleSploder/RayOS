//! Mutation Engine: Code Transformation and Variation Generation
//!
//! This module handles the core mutation mechanics of the Ouroboros Engine. It transforms
//! existing code into variations, applies refactoring operations, and generates optimization
//! candidates using both deterministic strategies and LLM-guided intelligent suggestions.
//!
//! # Architecture
//!
//! The mutation system operates in layers:
//! 1. **MutationType** - Enumeration of mutation categories (refactor, optimize, etc.)
//! 2. **MutationStrategy** - Selection of which mutations to attempt based on heuristics
//! 3. **RefactoringOps** - Standard refactoring transformations (extract, inline, rename)
//! 4. **OptimizationOps** - Performance-focused mutations (unroll, cache, vectorize)
//! 5. **LlmGuidedMutator** - System 2 semantic guidance for intelligent mutations
//! 6. **Mutator** - Main executor coordinating all mutation sources
//! 7. **MutationCandidate** - Proposed change with metadata and tracking
//! 8. **MutationBatch** - Group of related mutations for atomic testing
//!
//! # Boot Markers
//!
//! - `RAYOS_OUROBOROS:MUTATED` - Mutation generated
//! - `RAYOS_OUROBOROS:LLM_SUGGESTED` - LLM proposed mutation
//! - `RAYOS_OUROBOROS:BATCH_CREATED` - Mutation batch ready for testing

use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use crate::ouroboros::{
    EvolutionResult, MutationType, MutationSeverity, Checkpoint, CheckpointData, Checkpointable,
};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum number of mutations in a batch
pub const MAX_MUTATIONS_PER_BATCH: usize = 32;

/// Maximum number of refactoring operations tracked
pub const MAX_REFACTORING_OPS: usize = 256;

/// Maximum number of optimization operations tracked
pub const MAX_OPTIMIZATION_OPS: usize = 256;

/// Maximum mutations in flight
pub const MAX_MUTATION_CANDIDATES: usize = 1024;

/// Confidence threshold for LLM suggestions (0-255)
pub const LLM_CONFIDENCE_THRESHOLD: u8 = 128;

/// Base mutation rate (mutations per 1000 lines of code)
pub const BASE_MUTATION_RATE: f32 = 0.5;

// ============================================================================
// REFACTORING OPERATIONS
// ============================================================================

/// Standard refactoring operation
#[derive(Clone, Copy, Debug)]
pub struct RefactoringOp {
    /// Unique operation ID
    pub id: u32,
    /// Type of refactoring
    pub refactoring_type: RefactoringType,
    /// Source location (start line)
    pub line_start: u32,
    pub line_end: u32,
    /// Name involved (function, variable, etc.)
    pub name: [u8; 64],
    pub name_len: u8,
    /// New name (if renaming)
    pub new_name: [u8; 64],
    pub new_name_len: u8,
    /// Estimated benefit (basis points)
    pub estimated_benefit: i32,
    /// Estimated risk level (0-255)
    pub risk_level: u8,
    /// Application count
    pub applied_count: u32,
}

/// Types of refactoring operations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RefactoringType {
    /// Extract code into new function
    ExtractFunction = 0,
    /// Extract code into constant
    ExtractConstant = 1,
    /// Inline function call
    InlineFunction = 2,
    /// Inline variable
    InlineVariable = 3,
    /// Rename identifier
    Rename = 4,
    /// Move code to better location
    Move = 5,
    /// Simplify expression
    SimplifyExpression = 6,
    /// Remove dead code
    DeadCodeElimination = 7,
    /// Consolidate similar code
    Consolidate = 8,
    /// Split large function
    SplitFunction = 9,
}

impl RefactoringOp {
    /// Create a new refactoring operation
    pub fn new(id: u32, refactoring_type: RefactoringType) -> Self {
        Self {
            id,
            refactoring_type,
            line_start: 0,
            line_end: 0,
            name: [0u8; 64],
            name_len: 0,
            new_name: [0u8; 64],
            new_name_len: 0,
            estimated_benefit: 0,
            risk_level: 50,
            applied_count: 0,
        }
    }

    /// Set the name
    pub fn set_name(&mut self, name: &[u8]) -> Result<(), EvolutionResult> {
        if name.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.name[..name.len()].copy_from_slice(name);
        self.name_len = name.len() as u8;
        Ok(())
    }

    /// Set the new name
    pub fn set_new_name(&mut self, name: &[u8]) -> Result<(), EvolutionResult> {
        if name.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.new_name[..name.len()].copy_from_slice(name);
        self.new_name_len = name.len() as u8;
        Ok(())
    }

    /// Calculate severity based on type and scope
    pub fn severity(&self) -> MutationSeverity {
        match self.refactoring_type {
            RefactoringType::Rename => MutationSeverity::Minor,
            RefactoringType::ExtractConstant => MutationSeverity::Minor,
            RefactoringType::DeadCodeElimination => MutationSeverity::Minor,
            RefactoringType::SimplifyExpression => MutationSeverity::Moderate,
            RefactoringType::ExtractFunction => MutationSeverity::Moderate,
            RefactoringType::InlineVariable => MutationSeverity::Moderate,
            RefactoringType::Move => MutationSeverity::Moderate,
            RefactoringType::Consolidate => MutationSeverity::Moderate,
            RefactoringType::InlineFunction => MutationSeverity::Major,
            RefactoringType::SplitFunction => MutationSeverity::Major,
        }
    }
}

// ============================================================================
// OPTIMIZATION OPERATIONS
// ============================================================================

/// Performance-focused mutation operation
#[derive(Clone, Copy, Debug)]
pub struct OptimizationOp {
    /// Unique operation ID
    pub id: u32,
    /// Type of optimization
    pub optimization_type: OptimizationType,
    /// Target location (line)
    pub line_start: u32,
    pub line_end: u32,
    /// Function/region being optimized
    pub target: [u8; 64],
    pub target_len: u8,
    /// Estimated performance improvement (%)
    pub expected_speedup: f32,
    /// Memory improvement estimate (bytes)
    pub memory_savings: i32,
    /// Risk level (0-255)
    pub risk_level: u8,
    /// Application count
    pub applied_count: u32,
}

/// Types of optimization mutations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OptimizationType {
    /// Unroll loops for better ILP
    LoopUnroll = 0,
    /// Vectorize operations with SIMD
    Vectorize = 1,
    /// Cache frequently accessed values
    CacheValue = 2,
    /// Strength reduction (cheaper operations)
    StrengthReduction = 3,
    /// Batch similar operations
    Batching = 4,
    /// Parallelize independent operations
    Parallelize = 5,
    /// Reduce memory allocations
    ReduceAllocations = 6,
    /// Improve branch prediction
    BranchOptimization = 7,
    /// Reduce function call overhead
    InliningOptimization = 8,
    /// Algorithm replacement with faster variant
    AlgorithmReplacement = 9,
}

impl OptimizationOp {
    /// Create a new optimization operation
    pub fn new(id: u32, optimization_type: OptimizationType) -> Self {
        Self {
            id,
            optimization_type,
            line_start: 0,
            line_end: 0,
            target: [0u8; 64],
            target_len: 0,
            expected_speedup: 0.0,
            memory_savings: 0,
            risk_level: 75,
            applied_count: 0,
        }
    }

    /// Set the target
    pub fn set_target(&mut self, target: &[u8]) -> Result<(), EvolutionResult> {
        if target.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.target[..target.len()].copy_from_slice(target);
        self.target_len = target.len() as u8;
        Ok(())
    }

    /// Calculate severity based on type
    pub fn severity(&self) -> MutationSeverity {
        match self.optimization_type {
            OptimizationType::CacheValue => MutationSeverity::Minor,
            OptimizationType::StrengthReduction => MutationSeverity::Minor,
            OptimizationType::BranchOptimization => MutationSeverity::Moderate,
            OptimizationType::ReduceAllocations => MutationSeverity::Moderate,
            OptimizationType::LoopUnroll => MutationSeverity::Moderate,
            OptimizationType::Batching => MutationSeverity::Moderate,
            OptimizationType::InliningOptimization => MutationSeverity::Moderate,
            OptimizationType::Vectorize => MutationSeverity::Major,
            OptimizationType::Parallelize => MutationSeverity::Major,
            OptimizationType::AlgorithmReplacement => MutationSeverity::Critical,
        }
    }
}

// ============================================================================
// MUTATION STRATEGY
// ============================================================================

/// Strategy for selecting which mutations to attempt
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MutationStrategy {
    /// Target performance hotspots
    HotspotFocused = 0,
    /// Target high-complexity functions
    ComplexityReduction = 1,
    /// Target refactoring opportunities
    RefactoringFocused = 2,
    /// Balanced mix of all types
    Balanced = 3,
    /// LLM-guided intelligent selection
    LlmGuided = 4,
    /// Conservative: only safe, proven mutations
    Conservative = 5,
}

impl MutationStrategy {
    /// Calculate the mutation rate multiplier for this strategy
    pub fn rate_multiplier(&self) -> f32 {
        match self {
            MutationStrategy::HotspotFocused => 2.0,
            MutationStrategy::ComplexityReduction => 1.5,
            MutationStrategy::RefactoringFocused => 1.2,
            MutationStrategy::Balanced => 1.0,
            MutationStrategy::LlmGuided => 1.5,
            MutationStrategy::Conservative => 0.5,
        }
    }

    /// Risk tolerance for this strategy (0-255)
    pub fn risk_tolerance(&self) -> u8 {
        match self {
            MutationStrategy::Conservative => 25,
            MutationStrategy::RefactoringFocused => 75,
            MutationStrategy::ComplexityReduction => 100,
            MutationStrategy::Balanced => 128,
            MutationStrategy::HotspotFocused => 150,
            MutationStrategy::LlmGuided => 180,
        }
    }
}

impl Default for MutationStrategy {
    fn default() -> Self {
        MutationStrategy::Balanced
    }
}

// ============================================================================
// MUTATION CANDIDATE
// ============================================================================

/// A proposed code mutation with metadata
#[derive(Clone, Copy, Debug)]
pub struct MutationCandidate {
    /// Unique candidate ID
    pub id: u64,
    /// The mutation type
    pub mutation_type: MutationType,
    /// Severity of this mutation
    pub severity: MutationSeverity,
    /// Source location
    pub line_start: u32,
    pub line_end: u32,
    /// Estimated fitness improvement (basis points)
    pub estimated_improvement: i32,
    /// Risk/confidence level (0-255)
    pub confidence: u8,
    /// Whether this came from LLM
    pub llm_suggested: bool,
    /// Related refactoring operation ID (if any)
    pub refactoring_op_id: Option<u32>,
    /// Related optimization operation ID (if any)
    pub optimization_op_id: Option<u32>,
    /// Status of this candidate
    pub status: MutationStatus,
    /// Times this exact mutation was suggested
    pub suggestion_count: u32,
    /// Times tested (may be tested multiple times in different contexts)
    pub test_count: u32,
}

/// Status of a mutation candidate
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MutationStatus {
    /// Just generated
    Proposed = 0,
    /// Waiting to be tested
    Pending = 1,
    /// Currently being tested
    Testing = 2,
    /// Test passed
    Passed = 3,
    /// Test failed
    Failed = 4,
    /// Approved by user
    Approved = 5,
    /// Applied to live code
    Applied = 6,
    /// Rolled back
    RolledBack = 7,
    /// Rejected by user
    Rejected = 8,
}

impl MutationCandidate {
    /// Create a new mutation candidate
    pub fn new(id: u64, mutation_type: MutationType) -> Self {
        Self {
            id,
            mutation_type,
            severity: MutationSeverity::Trivial,
            line_start: 0,
            line_end: 0,
            estimated_improvement: 0,
            confidence: 128,
            llm_suggested: false,
            refactoring_op_id: None,
            optimization_op_id: None,
            status: MutationStatus::Proposed,
            suggestion_count: 1,
            test_count: 0,
        }
    }

    /// Mark as LLM-suggested
    pub fn mark_llm_suggested(&mut self) {
        self.llm_suggested = true;
    }

    /// Increment suggestion count
    pub fn increment_suggestion_count(&mut self) {
        self.suggestion_count = self.suggestion_count.saturating_add(1);
    }

    /// Increment test count
    pub fn increment_test_count(&mut self) {
        self.test_count = self.test_count.saturating_add(1);
    }

    /// Set status
    pub fn set_status(&mut self, status: MutationStatus) {
        self.status = status;
    }

    /// Check if candidate is ready for testing
    pub fn ready_for_testing(&self) -> bool {
        self.status == MutationStatus::Pending || self.status == MutationStatus::Proposed
    }
}

// ============================================================================
// MUTATION BATCH
// ============================================================================

/// A batch of related mutations tested together
#[derive(Clone, Copy, Debug)]
pub struct MutationBatch {
    /// Unique batch ID
    pub id: u64,
    /// Number of mutations in this batch
    pub mutation_count: u32,
    /// Total estimated improvement (basis points)
    pub total_improvement: i32,
    /// Aggregate risk level (0-255)
    pub aggregate_risk: u8,
    /// Timestamp batch was created
    pub created_at: u64,
    /// Whether this batch can be rolled back atomically
    pub atomic: bool,
    /// Status of the batch
    pub status: BatchStatus,
    /// Number of mutations passed in this batch
    pub mutations_passed: u32,
}

/// Status of a mutation batch
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BatchStatus {
    /// Batch created, waiting for mutations
    Created = 0,
    /// Ready for testing
    Ready = 1,
    /// Being tested
    Testing = 2,
    /// Test passed
    Passed = 3,
    /// Test failed
    Failed = 4,
    /// Partially passed (some mutations passed)
    PartialPass = 5,
    /// Batch applied
    Applied = 6,
    /// Batch rolled back
    RolledBack = 7,
}

impl MutationBatch {
    /// Create a new mutation batch
    pub fn new(id: u64) -> Self {
        Self {
            id,
            mutation_count: 0,
            total_improvement: 0,
            aggregate_risk: 0,
            created_at: 0,
            atomic: true,
            status: BatchStatus::Created,
            mutations_passed: 0,
        }
    }

    /// Mark batch as ready for testing
    pub fn mark_ready(&mut self) {
        if self.mutation_count > 0 {
            self.status = BatchStatus::Ready;
        }
    }

    /// Set testing status
    pub fn mark_testing(&mut self) {
        self.status = BatchStatus::Testing;
    }

    /// Set passed status
    pub fn mark_passed(&mut self) {
        self.status = BatchStatus::Passed;
    }

    /// Set failed status
    pub fn mark_failed(&mut self) {
        self.status = BatchStatus::Failed;
    }

    /// Is batch complete?
    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            BatchStatus::Passed | BatchStatus::Failed | BatchStatus::PartialPass | BatchStatus::RolledBack
        )
    }
}

// ============================================================================
// LLM-GUIDED MUTATOR
// ============================================================================

/// System 2 (LLM) guided mutation suggestions
pub struct LlmGuidedMutator {
    /// Number of suggestions made
    pub suggestion_count: u32,
    /// Number of suggestions accepted
    pub acceptance_count: u32,
    /// Average confidence of suggestions
    pub avg_confidence: f32,
    /// Last suggestion timestamp
    pub last_suggestion_at: u64,
}

impl LlmGuidedMutator {
    /// Create a new LLM-guided mutator
    pub fn new() -> Self {
        Self {
            suggestion_count: 0,
            acceptance_count: 0,
            avg_confidence: 0.0,
            last_suggestion_at: 0,
        }
    }

    /// Generate an LLM-suggested mutation
    pub fn suggest_mutation(
        &mut self,
        mutation_type: MutationType,
    ) -> MutationCandidate {
        let mut candidate = MutationCandidate::new(self.suggestion_count as u64, mutation_type);
        candidate.mark_llm_suggested();
        candidate.confidence = 180; // Higher confidence for LLM suggestions
        self.suggestion_count = self.suggestion_count.saturating_add(1);
        candidate
    }

    /// Record acceptance of LLM suggestion
    pub fn record_acceptance(&mut self, confidence: u8) {
        self.acceptance_count = self.acceptance_count.saturating_add(1);
        let total = self.acceptance_count as f32;
        self.avg_confidence = ((self.avg_confidence * (total - 1.0)) + confidence as f32) / total;
    }

    /// Get acceptance rate as percentage
    pub fn acceptance_rate(&self) -> f32 {
        if self.suggestion_count == 0 {
            return 0.0;
        }
        (self.acceptance_count as f32 / self.suggestion_count as f32) * 100.0
    }
}

impl Default for LlmGuidedMutator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MAIN MUTATOR
// ============================================================================

/// Core mutation engine orchestrator
pub struct Mutator {
    /// Next mutation candidate ID
    next_candidate_id: AtomicU64,
    /// Next batch ID
    next_batch_id: AtomicU64,
    /// Next refactoring op ID
    next_refactoring_op_id: AtomicU32,
    /// Next optimization op ID
    next_optimization_op_id: AtomicU32,
    /// Number of mutations generated
    pub mutations_generated: u64,
    /// Number of mutations passed testing
    pub mutations_passed: u64,
    /// Number of mutations applied
    pub mutations_applied: u64,
    /// Current mutation strategy
    pub strategy: MutationStrategy,
    /// LLM-guided mutation component
    llm_mutator: LlmGuidedMutator,
}

impl Mutator {
    /// Create a new mutator
    pub fn new() -> Self {
        Self {
            next_candidate_id: AtomicU64::new(1),
            next_batch_id: AtomicU64::new(1),
            next_refactoring_op_id: AtomicU32::new(1),
            next_optimization_op_id: AtomicU32::new(1),
            mutations_generated: 0,
            mutations_passed: 0,
            mutations_applied: 0,
            strategy: MutationStrategy::Balanced,
            llm_mutator: LlmGuidedMutator::new(),
        }
    }

    /// Set the mutation strategy
    pub fn set_strategy(&mut self, strategy: MutationStrategy) {
        self.strategy = strategy;
    }

    /// Generate a refactoring mutation
    pub fn generate_refactoring(
        &mut self,
        refactoring_type: RefactoringType,
    ) -> Result<(MutationCandidate, RefactoringOp), EvolutionResult> {
        let refactoring_op_id = self.next_refactoring_op_id.fetch_add(1, Ordering::Relaxed);
        let candidate_id = self.next_candidate_id.fetch_add(1, Ordering::Relaxed);

        let mut candidate = MutationCandidate::new(candidate_id, MutationType::Rename);
        candidate.refactoring_op_id = Some(refactoring_op_id);
        candidate.severity = RefactoringOp::new(refactoring_op_id, refactoring_type).severity();

        let mut refactoring_op = RefactoringOp::new(refactoring_op_id, refactoring_type);
        refactoring_op.estimated_benefit = 50; // Default benefit
        refactoring_op.risk_level = 25; // Refactoring is relatively safe

        self.mutations_generated = self.mutations_generated.saturating_add(1);
        Ok((candidate, refactoring_op))
    }

    /// Generate an optimization mutation
    pub fn generate_optimization(
        &mut self,
        optimization_type: OptimizationType,
    ) -> Result<(MutationCandidate, OptimizationOp), EvolutionResult> {
        let optimization_op_id = self.next_optimization_op_id.fetch_add(1, Ordering::Relaxed);
        let candidate_id = self.next_candidate_id.fetch_add(1, Ordering::Relaxed);

        let mut candidate = MutationCandidate::new(candidate_id, MutationType::AlgorithmReplacement);
        candidate.optimization_op_id = Some(optimization_op_id);
        candidate.severity = OptimizationOp::new(optimization_op_id, optimization_type).severity();

        let mut optimization_op = OptimizationOp::new(optimization_op_id, optimization_type);
        optimization_op.expected_speedup = 5.0; // Default speedup estimate
        optimization_op.risk_level = 100; // Optimizations have moderate risk

        self.mutations_generated = self.mutations_generated.saturating_add(1);
        Ok((candidate, optimization_op))
    }

    /// Generate LLM-guided mutation
    pub fn generate_llm_mutation(
        &mut self,
        mutation_type: MutationType,
    ) -> MutationCandidate {
        let candidate = self.llm_mutator.suggest_mutation(mutation_type);
        self.mutations_generated = self.mutations_generated.saturating_add(1);
        candidate
    }

    /// Create a new mutation batch
    pub fn create_batch(&mut self) -> MutationBatch {
        let batch_id = self.next_batch_id.fetch_add(1, Ordering::Relaxed);
        MutationBatch::new(batch_id)
    }

    /// Record mutation passed testing
    pub fn record_pass(&mut self) {
        self.mutations_passed = self.mutations_passed.saturating_add(1);
    }

    /// Record mutation applied
    pub fn record_applied(&mut self) {
        self.mutations_applied = self.mutations_applied.saturating_add(1);
    }

    /// Get mutation success rate
    pub fn success_rate(&self) -> f32 {
        if self.mutations_generated == 0 {
            return 0.0;
        }
        (self.mutations_passed as f32 / self.mutations_generated as f32) * 100.0
    }

    /// Get application rate
    pub fn application_rate(&self) -> f32 {
        if self.mutations_passed == 0 {
            return 0.0;
        }
        (self.mutations_applied as f32 / self.mutations_passed as f32) * 100.0
    }

    /// Get LLM acceptance rate
    pub fn llm_acceptance_rate(&self) -> f32 {
        self.llm_mutator.acceptance_rate()
    }

    /// Record LLM suggestion accepted
    pub fn record_llm_accepted(&mut self, confidence: u8) {
        self.llm_mutator.record_acceptance(confidence);
    }
}

impl Default for Mutator {
    fn default() -> Self {
        Self::new()
    }
}

impl Checkpointable for Mutator {
    fn checkpoint(&self) -> Result<Checkpoint, EvolutionResult> {
        let mut data = CheckpointData::new();

        let bytes_generated = (self.mutations_generated as u64).to_le_bytes();
        let bytes_passed = (self.mutations_passed as u64).to_le_bytes();
        let bytes_applied = (self.mutations_applied as u64).to_le_bytes();

        let mut combined = [0u8; 24];
        combined[0..8].copy_from_slice(&bytes_generated);
        combined[8..16].copy_from_slice(&bytes_passed);
        combined[16..24].copy_from_slice(&bytes_applied);

        data.set(&combined)?;

        Ok(Checkpoint {
            id: self.mutations_generated,
            timestamp: 0,
            data,
        })
    }

    fn restore(&mut self, _checkpoint: &Checkpoint) -> Result<(), EvolutionResult> {
        Ok(())
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refactoring_op_creation() {
        let op = RefactoringOp::new(1, RefactoringType::Rename);
        assert_eq!(op.id, 1);
        assert_eq!(op.refactoring_type, RefactoringType::Rename);
        assert_eq!(op.severity(), MutationSeverity::Minor);
    }

    #[test]
    fn test_refactoring_severity_levels() {
        let op1 = RefactoringOp::new(1, RefactoringType::Rename);
        let op2 = RefactoringOp::new(2, RefactoringType::InlineFunction);

        assert!(op1.severity() < op2.severity());
    }

    #[test]
    fn test_optimization_op_creation() {
        let op = OptimizationOp::new(1, OptimizationType::LoopUnroll);
        assert_eq!(op.id, 1);
        assert_eq!(op.optimization_type, OptimizationType::LoopUnroll);
        assert!(op.expected_speedup >= 0.0);
    }

    #[test]
    fn test_mutation_candidate_creation() {
        let candidate = MutationCandidate::new(1, MutationType::Extract);
        assert_eq!(candidate.id, 1);
        assert_eq!(candidate.status, MutationStatus::Proposed);
        assert!(!candidate.llm_suggested);
    }

    #[test]
    fn test_mutation_candidate_llm_marking() {
        let mut candidate = MutationCandidate::new(1, MutationType::Extract);
        candidate.mark_llm_suggested();
        assert!(candidate.llm_suggested);
        assert_eq!(candidate.confidence, 180);
    }

    #[test]
    fn test_mutation_batch_creation() {
        let batch = MutationBatch::new(1);
        assert_eq!(batch.id, 1);
        assert_eq!(batch.mutation_count, 0);
        assert_eq!(batch.status, BatchStatus::Created);
    }

    #[test]
    fn test_mutation_batch_status_transitions() {
        let mut batch = MutationBatch::new(1);
        assert_eq!(batch.status, BatchStatus::Created);

        batch.mutation_count = 1;
        batch.mark_ready();
        assert_eq!(batch.status, BatchStatus::Ready);

        batch.mark_testing();
        assert_eq!(batch.status, BatchStatus::Testing);

        batch.mark_passed();
        assert_eq!(batch.status, BatchStatus::Passed);
        assert!(batch.is_complete());
    }

    #[test]
    fn test_mutation_strategy_multipliers() {
        assert!((MutationStrategy::Conservative.rate_multiplier() - 0.5).abs() < 0.001);
        assert!((MutationStrategy::HotspotFocused.rate_multiplier() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_mutation_strategy_risk_tolerance() {
        assert!(MutationStrategy::Conservative.risk_tolerance() < MutationStrategy::LlmGuided.risk_tolerance());
    }

    #[test]
    fn test_llm_guided_mutator() {
        let mut llm = LlmGuidedMutator::new();
        let candidate = llm.suggest_mutation(MutationType::Inline);

        assert!(candidate.llm_suggested);
        assert_eq!(llm.suggestion_count, 1);

        llm.record_acceptance(200);
        assert_eq!(llm.acceptance_count, 1);
        assert!(llm.acceptance_rate() > 0.0);
    }

    #[test]
    fn test_mutator_basic_operations() {
        let mut mutator = Mutator::new();

        let (candidate, _op) = mutator
            .generate_refactoring(RefactoringType::Rename)
            .unwrap();
        assert_eq!(mutator.mutations_generated, 1);
        assert!(candidate.refactoring_op_id.is_some());

        mutator.record_pass();
        assert_eq!(mutator.mutations_passed, 1);

        mutator.record_applied();
        assert_eq!(mutator.mutations_applied, 1);
    }

    #[test]
    fn test_mutator_success_rate() {
        let mut mutator = Mutator::new();

        // Generate 10 mutations
        for _ in 0..10 {
            let _ = mutator.generate_refactoring(RefactoringType::Rename);
        }

        // 5 pass testing
        for _ in 0..5 {
            mutator.record_pass();
        }

        assert!((mutator.success_rate() - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_mutator_application_rate() {
        let mut mutator = Mutator::new();

        mutator.mutations_generated = 10;
        mutator.mutations_passed = 8;
        mutator.mutations_applied = 6;

        assert!((mutator.application_rate() - 75.0).abs() < 0.1);
    }

    #[test]
    fn test_mutator_llm_acceptance_rate() {
        let mut mutator = Mutator::new();

        let _ = mutator.generate_llm_mutation(MutationType::Inline);
        let _ = mutator.generate_llm_mutation(MutationType::Inline);

        mutator.record_llm_accepted(180);
        assert!((mutator.llm_acceptance_rate() - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_refactoring_op_set_name() {
        let mut op = RefactoringOp::new(1, RefactoringType::Rename);
        op.set_name(b"old_name").unwrap();
        assert_eq!(op.name_len, 8);
    }

    #[test]
    fn test_mutation_candidate_status_change() {
        let mut candidate = MutationCandidate::new(1, MutationType::Inline);
        assert_eq!(candidate.status, MutationStatus::Proposed);

        candidate.set_status(MutationStatus::Testing);
        assert_eq!(candidate.status, MutationStatus::Testing);
    }

    #[test]
    fn test_mutation_candidate_increment_counts() {
        let mut candidate = MutationCandidate::new(1, MutationType::Inline);
        assert_eq!(candidate.suggestion_count, 1);
        assert_eq!(candidate.test_count, 0);

        candidate.increment_suggestion_count();
        candidate.increment_test_count();

        assert_eq!(candidate.suggestion_count, 2);
        assert_eq!(candidate.test_count, 1);
    }
}
