# RayOS: The Sentient Substrate

**Status**: Core Architecture Specification
**Last Updated**: January 2026

---

## Overview

RayOS is not merely an operating system—it is a **sentient substrate**: a living computational foundation that perceives, reasons, and acts. Unlike traditional operating systems that passively await commands, RayOS continuously models its environment, anticipates user intent, and negotiates outcomes through semantic understanding.

This document defines the four foundational pillars of RayOS as a sentient substrate:

1. **The Bicameral Kernel** — Dual-process cognition architecture
2. **Logic as Geometry** — GPU ray tracing as the execution model
3. **The Neural File System** — Semantic memory and meaning-based retrieval
4. **The Ouroboros Engine** — Self-evolving metabolism for continuous improvement

---

## 1. The Bicameral Kernel

RayOS implements a dual-process cognitive architecture inspired by human cognition research. The kernel operates as two complementary systems working in continuous dialogue:

### System 2: The Conscious Mind

| Attribute | Description |
|-----------|-------------|
| **Role** | Reasoning, planning, and intent resolution |
| **Host** | Resident Large Language Model (LLM) |
| **Speed** | Slow and deliberate (100ms - seconds) |
| **Nature** | Probabilistic, contextual, explainable |

System 2 is the reflective, reasoning component of RayOS. It:

- Interprets ambiguous user intent through semantic analysis
- Plans multi-step actions across system resources
- Negotiates conflicts between competing goals
- Provides explanations for its decisions in natural language
- Learns from interactions to improve future responses

The resident LLM operates continuously, not as an application but as a core kernel service. It receives a constant stream of perceptual data, system state, and user signals, maintaining a persistent context that spans sessions.

### System 1: The Subconscious Mind

| Attribute | Description |
|-----------|-------------|
| **Role** | Reflex, pattern matching, and execution |
| **Host** | Persistent GPU Compute Shaders |
| **Speed** | Millisecond latency (~1-16ms) |
| **Nature** | Deterministic, parallel, reactive |

System 1 handles the fast, automatic responses that require no deliberation:

- Gaze tracking and attention modeling
- Input event routing and gesture recognition
- Window compositing and visual feedback
- Pattern-matched shortcuts (learned reflexes)
- Real-time sensor fusion

System 1 runs as persistent compute shaders on the GPU, eliminating kernel launch overhead and maintaining sub-frame latency for perceptual tasks.

### Bicameral Dialogue

The two systems communicate through a shared attention buffer:

```
┌─────────────────────────────────────────────────────────────┐
│                    Bicameral Kernel                         │
├─────────────────────────┬───────────────────────────────────┤
│      SYSTEM 2           │           SYSTEM 1                │
│   (The Conscious)       │       (The Subconscious)          │
│                         │                                   │
│  ┌─────────────────┐    │    ┌─────────────────────────┐   │
│  │  Resident LLM   │◄───┼────│  Persistent Compute     │   │
│  │                 │    │    │  Shaders (GPU)          │   │
│  │  • Reasoning    │    │    │                         │   │
│  │  • Planning     │────┼───►│  • Perception           │   │
│  │  • Intent       │    │    │  • Reflexes             │   │
│  │  • Explanation  │    │    │  • Compositing          │   │
│  └─────────────────┘    │    └─────────────────────────┘   │
│         ▲               │              ▲                    │
│         │               │              │                    │
│         └───────────────┴──────────────┘                    │
│                    Attention Buffer                         │
│              (Shared State & Signals)                       │
└─────────────────────────────────────────────────────────────┘
```

**Upward signals** (System 1 → System 2):
- "User is looking at the email icon with intent to click"
- "Anomalous pattern detected in file access"
- "Gesture sequence doesn't match known reflexes"

**Downward signals** (System 2 → System 1):
- "Install new reflex: triple-tap opens terminal"
- "Increase attention weight on calendar notifications"
- "Suppress visual notifications for next 30 minutes"

---

## 2. Logic as Geometry

Traditional computing represents logic as sequential instruction streams executed by CPUs. RayOS inverts this paradigm: **logic is geometry, executed through GPU ray tracing**.

### The Fundamental Insight

In ray tracing, rays traverse a spatial structure (BVH - Bounding Volume Hierarchy) and decisions occur at intersection points. This is functionally equivalent to decision trees, but executed in massively parallel hardware optimized for exactly this operation.

| Traditional CPU Logic | RayOS Geometric Logic |
|-----------------------|----------------------|
| Instructions (opcodes) | Rays (origin + direction) |
| Conditionals (if/else) | Intersections (hit tests) |
| Branch prediction | BVH traversal |
| Call stack | Ray recursion depth |
| Memory access | Texture/buffer sampling at hit points |

### How It Works

1. **Instructions become rays**: Each logical operation is encoded as a ray with an origin (current state) and direction (operation type).

2. **Decisions become intersections**: Conditional logic is represented as geometry. A ray either intersects an object (condition true) or misses (condition false).

3. **State lives in geometry**: Variables and state are encoded in the spatial structure. Accessing state is a ray-geometry intersection, not a memory load.

4. **Parallelism is inherent**: Millions of rays (operations) execute simultaneously across thousands of GPU cores.

### Geometry-Encoded Logic Example

Consider a simple access control check: "Can user X access file Y?"

**Traditional (CPU)**:
```
if user.role == "admin":
    return True
if file.owner == user.id:
    return True
if file.permissions & user.groups:
    return True
return False
```

**Geometric (GPU Ray Tracing)**:
```
Scene contains:
  - Admin sphere at origin (radius = admin_role_enabled)
  - Ownership cone from user to file
  - Permission mesh based on ACL geometry

Ray: origin=user_context, direction=toward_file

Result:
  - Hit admin sphere? → Access granted
  - Hit ownership cone? → Access granted
  - Hit permission mesh? → Access granted
  - No hit? → Access denied
```

The GPU's RT cores execute this in hardware, testing millions of access decisions per millisecond.

### Benefits

- **Massive parallelism**: RT cores are designed for billions of ray-geometry tests per second
- **Unified model**: Perception (ray casting for visibility) and logic (ray casting for decisions) use identical hardware paths
- **Spatial locality**: Related decisions cluster geometrically, improving cache efficiency
- **Hardware evolution**: As RT cores improve for graphics, RayOS logic automatically accelerates

---

## 3. The Neural File System

Traditional file systems organize data by **location** (paths, inodes, sectors). The Neural File System organizes data by **meaning** (semantic vectors, conceptual relationships).

### Design Philosophy

> "Find that presentation about Q3 sales numbers"

A user should be able to retrieve files by describing their content, not remembering their name or location. The Neural File System understands the *concept* of "Q3 sales presentation" and locates matching files instantly.

### Architecture

The Neural File System comprises three integrated components:

```
┌─────────────────────────────────────────────────────────────────┐
│                    NEURAL FILE SYSTEM                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐ │
│  │   VECTOR STORE   │  │    EMBEDDER      │  │   EPIPHANY    │ │
│  │   (Hippocampus)  │  │    (Encoder)     │  │    BUFFER     │ │
│  │                  │  │                  │  │   (Dreams)    │ │
│  │  Long-term       │  │  Semantic        │  │               │ │
│  │  semantic        │◄─┤  transformation  │  │  Sandbox for  │ │
│  │  memory          │  │  engine          │  │  speculative  │ │
│  │                  │  │                  │  │  connections  │ │
│  │  • File vectors  │  │  • Text → Vector │  │               │ │
│  │  • Relationships │  │  • Image → Vec   │  │  • Hypotheses │ │
│  │  • Access logs   │  │  • Audio → Vec   │  │  • Patterns   │ │
│  │  • Concepts      │  │  • Code → Vec    │  │  • Insights   │ │
│  └────────▲─────────┘  └────────┬─────────┘  └───────┬───────┘ │
│           │                     │                    │         │
│           │              ┌──────▼──────┐             │         │
│           └──────────────┤   FUSION    ├─────────────┘         │
│                          │   LAYER     │                       │
│                          └─────────────┘                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Component Details

#### The Vector Store (Hippocampus)

The Vector Store is the long-term semantic memory of RayOS, analogous to the brain's hippocampus.

**Responsibilities**:
- Store high-dimensional embedding vectors for all system content
- Index vectors for fast approximate nearest-neighbor search
- Maintain conceptual hierarchies and relationships
- Track access patterns for relevance weighting
- Persist across sessions and reboots

**Implementation**:
- HNSW (Hierarchical Navigable Small World) index for O(log n) search
- GPU-accelerated similarity computation
- Incremental updates without full re-indexing
- Configurable vector dimensions (512, 768, 1024)

#### The Embedder (Semantic Encoder)

The Embedder transforms all content types into unified semantic vectors.

**Supported modalities**:
| Content Type | Embedding Model | Vector Dimension |
|--------------|-----------------|------------------|
| Text documents | Sentence transformers | 768 |
| Source code | Code-specific encoder | 768 |
| Images | CLIP visual encoder | 512 |
| Audio/speech | Whisper + text embed | 768 |
| Structured data | Schema-aware encoder | 512 |

**Processing pipeline**:
1. Content ingestion (file create/modify events)
2. Type detection and preprocessing
3. Chunking (for large documents)
4. Embedding generation
5. Vector storage with metadata
6. Relationship inference

#### The Epiphany Buffer (Dream Sandbox)

The Epiphany Buffer is a safe, isolated space where the system can explore speculative connections without affecting the main knowledge graph.

**Purpose**:
- Test hypothetical relationships between concepts
- Explore "what if" scenarios during idle time
- Incubate insights that may prove valuable
- Isolate experimental reasoning from production state

**Operation**:
- Runs during low-activity periods ("dreaming")
- Generates candidate connections between distant concepts
- Scores candidates by coherence and utility
- Promotes high-confidence discoveries to the Vector Store
- Discards or archives low-confidence speculation

**Example epiphanies**:
- "The error patterns in project A resemble those fixed in project B six months ago"
- "Your meeting notes from March mention the same client as this new email"
- "This code refactoring approach appears in three successful PRs"

### Query Resolution

When a user requests "that presentation about Q3 sales numbers":

1. **Query embedding**: The Embedder converts the query to a vector
2. **Similarity search**: The Vector Store finds nearest neighbors
3. **Ranking**: Results are ranked by:
   - Semantic similarity score
   - Recency of access
   - User context (current project, role)
   - Relationship strength to current focus
4. **Presentation**: Top results returned with confidence scores

---

## 4. The Ouroboros Engine

RayOS embodies a radical design principle: **the system should constantly evolve into a better version of itself**. Named after the ancient symbol of a serpent consuming its own tail, the Ouroboros Engine is RayOS's metabolism—a built-in drive for perpetual self-improvement.

### The No Idle Principle

Traditional operating systems enter sleep or low-power states when the user is away. RayOS instead enters **Dream Mode**—a productive state where the system introspects, experiments, and evolves.

> When RayOS detects user absence (configurable, default 5 minutes), it doesn't sleep. It dreams. The Ouroboros Engine activates, mutating RayOS's own code, testing variations in sandboxes, and live-patching the winners.

### How It Works

The Ouroboros Engine operates as a continuous evolutionary loop:

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
│  │  Source code     │  │  Code            │  │  Sandbox          │  │
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

### Core Components

#### Genome Repository

RayOS's source code is represented as a mutable genome:

| Component | Purpose |
|-----------|---------|
| **AST Parser** | Parses source into abstract syntax tree for manipulation |
| **Dependency Graph** | Tracks relationships between code units |
| **Hotspot Tracker** | Identifies high-impact code regions for targeted evolution |
| **Mutation Points** | Valid locations where changes can safely occur |

**Requirement**: RayOS's source code must be accessible to RayOS itself. The system introspects its own codebase as the genome for evolution.

#### Mutation Engine

The Mutation Engine generates code variations:

| Mutation Type | Description |
|---------------|-------------|
| **Refactoring** | Extract functions, inline code, rename for clarity |
| **Optimization** | Loop unrolling, caching, vectorization |
| **LLM-Guided** | System 2 proposes intelligent rewrites |
| **Simplification** | Remove dead code, simplify conditionals |
| **Parallelization** | Convert sequential to parallel execution |

System 2 (the resident LLM) plays a crucial role, suggesting mutations that require semantic understanding rather than mechanical transformation.

#### Selection Arena

Mutations compete in sandboxed environments:

| Fitness Metric | Weight | Description |
|----------------|--------|-------------|
| Execution Time | 40% | Faster is better (weighted by call frequency) |
| Memory Usage | 25% | Lower allocation, smaller footprint |
| Test Pass Rate | **Must be 100%** | Correctness is non-negotiable |
| Code Size | 15% | Smaller often means more elegant |
| Energy Consumption | 20% | Battery-friendly for mobile use |

Only mutations that pass all tests AND improve the fitness score advance.

#### Live Patcher

Winning mutations are hot-swapped into the running system:

- **Safe Points**: Patches applied only at quiescent points (between syscalls)
- **Atomic Swap**: Lock-free code replacement for hot paths
- **Rollback Log**: Complete history enables instant reversion
- **Health Checks**: Automatic rollback if crash detected post-patch

#### Dream Scheduler

The Dream Scheduler monitors activity and triggers evolution:

| State | Trigger | Action |
|-------|---------|--------|
| **Active** | User input within threshold | Evolution paused |
| **Idle** | 5 min no activity (configurable) | Evolution begins |
| **Deep Idle** | 15 min no activity | Aggressive evolution |
| **Power Save** | Battery < 20% | Evolution suspended |

### User Control

Users retain full control over evolution:

| Mode | Behavior |
|------|----------|
| **Automatic** | All passing mutations applied immediately |
| **Notify** | Applied automatically, user notified |
| **Approve Major** | Minor refactors auto, major changes need approval |
| **Approve All** | Every mutation requires explicit consent |
| **Disabled** | Ouroboros Engine completely off |

### Safety Guarantees

1. **Isolation**: All mutations tested in sandboxes before affecting live system
2. **Reversibility**: Every change logged and instantly revertible
3. **Correctness**: 100% test pass rate required for acceptance
4. **Grace Period**: Automatic rollback if crash within configurable window
5. **User Consent**: Approval modes let users control evolution scope

---

## Integration Points

### Bicameral ↔ Logic as Geometry

System 1 (GPU compute shaders) naturally interfaces with the geometric logic engine:
- Perceptual ray casting feeds into logical ray casting
- Reflex patterns are encoded as geometric acceleration structures
- The same RT cores serve both vision and decision

### Bicameral ↔ Neural File System

- System 2 (LLM) queries the Vector Store for context during reasoning
- System 1 routes file access events to the Embedder
- The Epiphany Buffer feeds insights to System 2 for evaluation
- System 2 can instruct the Embedder to create new relationship types

### Logic as Geometry ↔ Neural File System

- Vector similarity can be computed as geometric distance
- The Vector Store index (HNSW) is itself a navigable geometry
- Access control decisions (geometric logic) gate file retrieval

### Ouroboros ↔ Bicameral Kernel

- **System 2** proposes intelligent mutations based on semantic understanding
- **System 2** evaluates whether mutations align with user intent and system goals
- **System 1** monitors real-time performance metrics for fitness scoring
- **System 1** triggers evolution when detecting performance degradation patterns

### Ouroboros ↔ Neural File System

- **Epiphany Buffer** may suggest code improvements based on pattern analysis across history
- **Vector Store** enables semantic search through mutation history
- **Relationship Inference** connects related code regions for coordinated batch mutations
- Mutation outcomes are embedded and stored for future learning

### Ouroboros ↔ Dream Mode (Shared Infrastructure)

The Ouroboros Engine and Epiphany Buffer share dream mode cycles:
1. Both activate during user idle periods
2. Resource budget split based on priority and pending work
3. Epiphany discoveries can trigger Ouroboros mutations
4. Ouroboros improvements inform Epiphany pattern recognition

---

## Development Status

| Component | Status | Notes |
|-----------|--------|-------|
| System 2 (LLM) | ✅ Implemented | Resident inference in kernel |
| System 1 (Reflexes) | ✅ Implemented | GPU compute shader pattern matching, reflexes, attention signals |
| Bicameral Bridge | ✅ Implemented | Attention buffer protocol with upward/downward signals |
| Logic as Geometry | ✅ Implemented | Conditionals as ray-geometry intersections with GPU compute |
| Access Control Geometry | ✅ Implemented | Permissions as GPU geometric hit tests |
| Unified Pipeline | ✅ Implemented | Single dispatch for perception, logic, and semantic stages |
| Vector Store | ✅ Implemented | HNSW index with O(log n) ANN search |
| GPU Similarity Search | ✅ Implemented | WGSL compute shader for parallel cosine similarity |
| Multi-Modal Embedder | ✅ Implemented | Text, code, image, audio embedding with modality features |
| Content Ingestion | ✅ Implemented | File watching, debouncing, batching, filtering |
| Epiphany Buffer | ✅ Implemented | Connection discovery, dream scheduling, scoring, promotion |
| Semantic Query | ✅ Implemented | Natural language parsing, expansion, multi-factor ranking |
| Relationship Inference | ✅ Implemented | Knowledge graph, concept linking, inference engine |
| GPU Reflex Engine | ✅ Implemented | Pattern matching on GPU with WGSL compute shaders |
| Ouroboros Genome Repository | 📋 Planned | Source code as mutable genome (Phase 31) |
| Ouroboros Mutation Engine | 📋 Planned | Code transformation and variation (Phase 31) |
| Ouroboros Selection Arena | 📋 Planned | Sandbox testing and fitness scoring (Phase 31) |
| Ouroboros Live Patcher | 📋 Planned | Hot-swap winning mutations (Phase 31) |
| Ouroboros Dream Scheduler | 📋 Planned | Idle detection and evolution triggers (Phase 31) |

---

## See Also

- [Whitepaper](whitepaper.md) — Academic framing of RayOS concepts
- [Intent Primitives](intent_primitives.md) — Semantic intent envelope specification
- [Cortex README](cortex/README.md) — Sensory processing layer
- [Gaze Ray Pipeline](gaze_ray_pipeline.md) — Eye tracking integration
- [ROADMAP](ROADMAP.md) — Development milestones and tasks
- [Phase 31 Plan](phases/PHASE_31_PLAN.md) — Ouroboros Engine implementation details
