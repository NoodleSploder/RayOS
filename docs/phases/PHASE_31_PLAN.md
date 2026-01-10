# Phase 31: The Ouroboros Engine

**Status**: Planning
**Target**: Self-Evolving System Architecture
**Estimated Lines**: 8,000+

---

## Overview

The **Ouroboros Engine** is RayOS's metabolism—a built-in drive for constant self-improvement. Named after the ancient symbol of a serpent eating its own tail, this system embodies the principle that RayOS should never be static; it is a living system that continuously evolves into better versions of itself.

### Core Philosophy

> **"No Idle Principle"**: When RayOS is not actively serving the user, it does not sleep—it dreams. During dream mode, the Ouroboros Engine activates, mutating its own code, testing variations, and live-patching the winners.

### Requirements

- **Source Access**: RayOS's source code must be available to RayOS itself for introspection and mutation
- **Safe Mutation**: Changes occur in isolated sandboxes before affecting the live system
- **Reversibility**: All mutations are logged and can be instantly reverted
- **User Control**: Configurable thresholds for automatic evolution vs. user approval

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        OUROBOROS ENGINE                              │
│                   "The System That Evolves Itself"                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────────┐  │
│  │   GENOME         │  │    MUTATION      │  │    SELECTION      │  │
│  │   REPOSITORY     │  │    ENGINE        │  │    ARENA          │  │
│  │                  │  │                  │  │                   │  │
│  │  Source code     │  │  Code           │  │  Sandbox          │  │
│  │  as mutable      │──►  transformation  │──►  testing &        │  │
│  │  genome          │  │  & variation     │  │  fitness scoring  │  │
│  │                  │  │                  │  │                   │  │
│  │  • AST parsing   │  │  • Refactoring   │  │  • Performance    │  │
│  │  • Dependency    │  │  • Optimization  │  │  • Memory usage   │  │
│  │    graph         │  │  • LLM-guided    │  │  • Correctness    │  │
│  │  • Hot regions   │  │    rewrites      │  │  • Regression     │  │
│  └────────▲─────────┘  └─────────────────┘  └─────────┬─────────┘  │
│           │                                           │             │
│           │            ┌─────────────────┐            │             │
│           └────────────┤   LIVE PATCHER  │◄───────────┘             │
│                        │                 │                          │
│                        │  Hot-swap       │                          │
│                        │  winning        │                          │
│                        │  mutations      │                          │
│                        └────────┬────────┘                          │
│                                 │                                   │
│  ┌──────────────────────────────▼───────────────────────────────┐  │
│  │                    DREAM SCHEDULER                            │  │
│  │                                                               │  │
│  │  Monitors user activity → Triggers evolution during idle     │  │
│  │  Configurable idle threshold (default: 5 minutes)            │  │
│  │  Power-aware: More aggressive on AC, conservative on battery │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Tasks

### Task 1: Genome Repository (1,500+ lines)
**File**: `crates/kernel-bare/src/ouroboros/genome.rs`

The source code representation and introspection system.

**Components**:
- `SourceGenome` - Represents RayOS source as an evolvable genome
- `GenomeRegion` - Marked code regions eligible for mutation
- `AstNode` - Simplified AST representation for mutation
- `DependencyGraph` - Tracks dependencies between code units
- `HotspotTracker` - Identifies frequently executed / high-impact code
- `MutationPoint` - Valid points where mutations can occur
- `GenomeChecksum` - Integrity verification for source

**Markers**:
- `RAYOS_OUROBOROS:PARSED` - Source genome parsed
- `RAYOS_OUROBOROS:REGION_MARKED` - Mutation region identified
- `RAYOS_OUROBOROS:HOTSPOT_DETECTED` - Performance hotspot found

---

### Task 2: Mutation Engine (1,500+ lines)
**File**: `crates/kernel-bare/src/ouroboros/mutation.rs`

The code transformation and variation generation system.

**Components**:
- `MutationType` - Types of mutations (refactor, optimize, rewrite, inline, etc.)
- `MutationStrategy` - Selection of which mutations to attempt
- `Mutator` - Core mutation executor
- `RefactoringOps` - Standard refactoring operations (extract, inline, rename)
- `OptimizationOps` - Performance-focused mutations (loop unroll, cache, vectorize)
- `LlmGuidedMutator` - System 2 LLM suggests intelligent mutations
- `MutationCandidate` - A proposed code change with metadata
- `MutationBatch` - Group of related mutations for atomic testing

**Markers**:
- `RAYOS_OUROBOROS:MUTATED` - Mutation generated
- `RAYOS_OUROBOROS:LLM_SUGGESTED` - LLM proposed mutation
- `RAYOS_OUROBOROS:BATCH_CREATED` - Mutation batch ready for testing

---

### Task 3: Selection Arena (1,500+ lines)
**File**: `crates/kernel-bare/src/ouroboros/selection.rs`

The sandbox testing and fitness scoring system.

**Components**:
- `Sandbox` - Isolated execution environment for testing mutations
- `FitnessMetric` - Individual performance/quality metric
- `FitnessScore` - Composite score from multiple metrics
- `TestSuite` - Regression tests to verify correctness
- `BenchmarkSuite` - Performance benchmarks for comparison
- `MemoryProfiler` - Track memory usage changes
- `SelectionResult` - Outcome of testing (accept, reject, needs_review)
- `TournamentSelector` - Compares mutations against baseline and each other

**Fitness Metrics**:
- Execution time (weighted by call frequency)
- Memory allocation (peak and average)
- Code size (smaller is often better)
- Test pass rate (must be 100% for acceptance)
- Energy consumption (for battery-aware evolution)

**Markers**:
- `RAYOS_OUROBOROS:TESTED` - Mutation tested in sandbox
- `RAYOS_OUROBOROS:SCORED` - Fitness score calculated
- `RAYOS_OUROBOROS:SELECTED` - Mutation selected as winner

---

### Task 4: Live Patcher (1,200+ lines)
**File**: `crates/kernel-bare/src/ouroboros/patcher.rs`

The hot-swap system for applying winning mutations.

**Components**:
- `PatchOperation` - Single code patch operation
- `PatchBundle` - Collection of patches for atomic application
- `LivePatcher` - Hot-swap engine that patches running code
- `PatchPoint` - Safe points where patches can be applied
- `RollbackLog` - Full history for instant reversion
- `AtomicSwap` - Lock-free swap for hot code paths
- `VersionRegistry` - Tracks active code versions

**Safety Guarantees**:
- Patches only applied at safe points (between syscalls, not mid-critical-section)
- Full state preservation across patch boundary
- Instant rollback if post-patch health check fails
- Automatic rollback if crash detected within grace period

**Markers**:
- `RAYOS_OUROBOROS:PATCHED` - Live patch applied
- `RAYOS_OUROBOROS:ROLLBACK` - Patch reverted
- `RAYOS_OUROBOROS:VERSION_UPDATED` - Code version incremented

---

### Task 5: Dream Scheduler (1,000+ lines)
**File**: `crates/kernel-bare/src/ouroboros/scheduler.rs`

The idle detection and evolution scheduling system.

**Components**:
- `ActivityMonitor` - Tracks user/system activity
- `IdleState` - Current idle classification (active, idle, deep_idle, dreaming)
- `DreamTrigger` - Conditions that activate Ouroboros Engine
- `DreamSession` - Active evolution session
- `PowerPolicy` - Adjust aggressiveness based on power source
- `SchedulerConfig` - User-configurable thresholds
- `DreamBudget` - Resource limits for evolution work

**Idle Thresholds** (configurable):
- `IDLE_THRESHOLD_DEFAULT`: 5 minutes
- `IDLE_THRESHOLD_AGGRESSIVE`: 2 minutes
- `IDLE_THRESHOLD_CONSERVATIVE`: 15 minutes
- `DEEP_IDLE_MULTIPLIER`: 3x (more aggressive after 15 min default)

**Power Policies**:
- **AC Power**: Full evolution budget, aggressive mutations
- **Battery >50%**: Moderate budget, targeted mutations
- **Battery <20%**: Minimal budget, critical hotspots only
- **Battery <5%**: Evolution suspended

**Markers**:
- `RAYOS_OUROBOROS:IDLE_DETECTED` - User idle detected
- `RAYOS_OUROBOROS:DREAM_STARTED` - Evolution session begun
- `RAYOS_OUROBOROS:DREAM_ENDED` - Evolution session complete

---

### Task 6: Evolution Coordinator (1,300+ lines)
**File**: `crates/kernel-bare/src/ouroboros/coordinator.rs`

The central orchestrator that ties all components together.

**Components**:
- `OuroborosEngine` - Main engine struct
- `EvolutionConfig` - Global configuration
- `EvolutionStats` - Metrics and history
- `MutationHistory` - Log of all attempted mutations
- `WinnerRegistry` - Successful mutations applied
- `ApprovalQueue` - Mutations requiring user approval
- `EvolutionReport` - Human-readable summary of evolution activity

**User Approval Modes**:
- **Automatic**: All passing mutations applied (for power users)
- **Notify**: Applied automatically, user notified of changes
- **Approve Major**: Minor refactors automatic, major changes need approval
- **Approve All**: Every mutation requires explicit user approval

**Markers**:
- `RAYOS_OUROBOROS:CYCLE_COMPLETE` - Full evolution cycle finished
- `RAYOS_OUROBOROS:APPROVED` - User approved mutation
- `RAYOS_OUROBOROS:REJECTED` - User rejected mutation

---

## Integration Points

### With Bicameral Kernel

- **System 2 (LLM)** provides intelligent mutation suggestions
- **System 2** evaluates whether mutations align with user intent
- **System 1** monitors performance metrics in real-time
- **System 1** triggers evolution when detecting performance degradation

### With Neural File System

- **Epiphany Buffer** may suggest code improvements based on pattern analysis
- **Vector Store** enables semantic search through mutation history
- **Relationship Inference** connects related code regions for batch mutations

### With Dream Mode

The Ouroboros Engine is the primary consumer of dream mode cycles:
1. Epiphany Buffer processes connections (existing)
2. **Ouroboros Engine processes code evolution** (this phase)
3. Both share the idle budget based on priority

---

## Configuration Schema

```rust
/// Ouroboros Engine configuration
pub struct OuroborosConfig {
    /// Enable/disable the engine entirely
    pub enabled: bool,

    /// Idle threshold before entering dream mode (seconds)
    pub idle_threshold_secs: u32,

    /// Maximum evolution cycles per dream session
    pub max_cycles_per_session: u32,

    /// Approval mode
    pub approval_mode: ApprovalMode,

    /// Mutation aggressiveness (0.0 - 1.0)
    pub mutation_aggressiveness: f32,

    /// Whether to evolve on battery power
    pub evolve_on_battery: bool,

    /// Minimum battery level for evolution (percentage)
    pub min_battery_level: u8,

    /// Maximum memory budget for sandboxes (MB)
    pub sandbox_memory_mb: u32,

    /// Rollback grace period (seconds)
    pub rollback_grace_period_secs: u32,
}
```

---

## Success Criteria

- [ ] Source code parsed into mutable genome representation
- [ ] At least 10 mutation types implemented
- [ ] Sandboxed testing with correctness verification
- [ ] Live patching with <10ms downtime
- [ ] Dream scheduler with configurable idle threshold
- [ ] Full rollback capability with instant recovery
- [ ] LLM-guided mutations producing meaningful improvements
- [ ] User approval workflow for major changes
- [ ] 25+ RAYOS_OUROBOROS markers in output

---

## Future Enhancements (Post-Phase 31)

- **Population Evolution**: Multiple mutation lineages competing
- **Cross-Module Optimization**: Mutations spanning crate boundaries
- **User Behavior Learning**: Evolve based on how user actually uses the system
- **Distributed Evolution**: Share winning mutations across RayOS installations
- **Formal Verification**: Prove mutations preserve correctness invariants

---

## See Also

- [SENTIENT_SUBSTRATE.md](../SENTIENT_SUBSTRATE.md) — The four pillars including Ouroboros
- [ROADMAP.md](../ROADMAP.md) — Development milestones
- [PHASE_30_PLAN.md](PHASE_30_PLAN.md) — Previous phase (Drag-Drop & Clipboard)
