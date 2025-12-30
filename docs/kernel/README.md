# RayOS Kernel

**Version:** 0.1.0-alpha
**Phase:** Phase 1 - The Skeleton (Proof of Concept)
**Status:** ✓ Compiles Successfully

A GPU-native, AI-centric operating system kernel implementing the **Bicameral Architecture**: a dual-system design inspired by human consciousness.

```
┌─────────────────────────────────────────┐
│          RayOS Kernel v0.1.0            │
│   GPU-Native AI-Centric OS Kernel       │
│    "Logic as Geometry, Thoughts as Rays"│
└─────────────────────────────────────────┘
```

## 🧠 Core Thesis

RayOS replaces the traditional Von Neumann "Interrupt Model" with a **Continuous Simulation Model**, where:

- **Logic is Geometry**: Control flow (`if/else`) is compiled into BVH (Bounding Volume Hierarchies) traversed by RT Cores
- **Threads are Rays**: Each task is a ray with an origin (current state) and direction (intent)
- **Consciousness is Bicameral**: Fast reflexes (System 1) + Slow reasoning (System 2)

## 🏗️ Architecture

### The Four Pillars

#### 1. **The Brain (Kernel)**
- **System 1 (Reflex Engine)**: Persistent GPU compute shader running an infinite megakernel loop
- **System 2 (Cognitive Engine)**: LLM-based intent parser and policy arbiter
- **HAL (Hardware Abstraction)**: Zero-copy allocator and hive manager for multi-GPU coordination

#### 2. **The Memory (Storage)** ⏳ *Coming in Phase 3*
- Vector Store: Semantic file system using embeddings
- HNSW Indexer: Spatial organization of concepts

#### 3. **The Senses (Input)** ⏳ *Coming in Phase 2*
- Vision Pathway: Gaze tracking and object recognition
- Auditory Pathway: Continuous speech-to-text with context fusion

#### 4. **The Metabolism (Autonomy)** ⏳ *Coming in Phase 4*
- Entropy Monitor: Detects inefficiency and idle states
- Ouroboros Engine: Self-refactoring and genetic optimization

## 📦 Project Structure

```
src/
├── lib.rs                  # Main kernel API
├── main.rs                 # Boot sequence and demo
├── types.rs                # Core data structures (LogicRay, etc.)
├── hal/                    # Hardware Abstraction Layer
│   ├── mod.rs              # HAL manager
│   ├── allocator.rs        # Zero-copy unified memory
│   └── hive.rs             # Multi-GPU work stealing
├── system1/                # The Reflex Engine (Subconscious)
│   ├── mod.rs              # Main loop controller
│   ├── megakernel.rs       # Persistent compute shader (WGSL)
│   └── ray_logic.rs        # BVH logic trees
└── system2/                # The Cognitive Engine (Conscious)
    ├── mod.rs              # LLM integration stub
    ├── intent.rs           # Natural language parsing
    └── policy.rs           # Resource allocation arbiter
```

## 🚀 Getting Started

### Prerequisites

- **Rust** 1.70+ (2021 edition)
- **GPU** with Vulkan/Metal/DX12 support
- **wgpu** compatible drivers

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run --release
```

### Run Tests

```bash
cargo test --all
```

## 🎯 Phase 1 Goals (Current)

- ✅ Zero-Copy Allocator (Unified Memory)
- ✅ Megakernel Loop (Persistent GPU Compute)
- ✅ LogicRay Data Structure
- ✅ Hive Manager (Multi-GPU Coordination)
- ✅ Basic Intent Parser (Placeholder)
- ⏳ RT Core Integration (Next)

## 📊 Example Output

```
[INFO] === Initializing RayOS Kernel ===
[INFO] ✓ Zero-Copy Allocator initialized
[INFO] ✓ System 1 (Reflex Engine) initialized
[INFO] ✓ System 2 (Cognitive Engine) initialized
[INFO] ✓ Autonomy Watcher initialized
[INFO] === RayOS Kernel Ready ===

[DEMO] Submitting test tasks...
  ✓ Submitted NL task: 'optimize the rendering pipeline'
  ✓ Submitted gaze task at (512, 384)
  ✓ Submitted multimodal task: 'delete that' + gaze

[METRICS] T+1s: Queue=0, Entropy=0.10, User=true
[METRICS] T+2s: Queue=0, Entropy=0.10, User=true
...
```

## 🔬 Key Concepts

### The LogicRay

The fundamental unit of execution:

```rust
pub struct LogicRay {
    pub origin: Vec3,        // Current state vector
    pub direction: Vec3,     // Intent vector
    pub task_id: u64,        // Unique identifier
    pub priority: u8,        // 0 = Dream, 255 = Immediate
    pub data_ptr: u64,       // Unified memory pointer
    pub logic_tree_id: u32,  // Which BVH to traverse
}
```

### The Megakernel Loop

Instead of CPU interrupts, we have an infinite GPU loop:

```rust
while self.running.load(Ordering::Relaxed) {
    // Pop rays from queue
    // Execute via RT Core traversal
    // Maintain 60 FPS target
    // Balance load across GPUs
}
```

### Logic as Geometry

Traditional code:
```rust
if condition_a {
    action_1();
} else {
    action_2();
}
```

RayOS equivalent:
```rust
let bvh = LogicBVH::from_simple_branch(0, 0, 100, 200);
let result = bvh.trace(&state); // Uses RT Cores!
```

## 🛠️ Technology Stack

- **Language**: Rust 2021
- **GPU Compute**: wgpu + WGSL shaders
- **Math**: glam (SIMD-optimized)
- **Concurrency**: tokio + crossbeam
- **Memory**: bytemuck (zero-copy)

## 🗺️ Roadmap

### Phase 1: The Skeleton ✓ (Current)
- Prove CPU-GPU unified memory works
- Get megakernel running without crashing
- Basic task queue and execution

### Phase 2: The Eyes (Months 4-6)
- Integrate gaze tracking (eye control)
- Connect local LLM for intent parsing
- Multimodal input fusion

### Phase 3: The Memory (Months 7-9)
- Vector store file system
- Semantic search and retrieval
- Dream state idea validation

### Phase 4: The Life (Months 10+)
- Ouroboros self-optimization
- Genetic algorithm mutations
- Hot-patching live kernel

## 📚 Documentation

For detailed design specifications, see:
- [Ray Outline](docs/ray-outline.md) - Component hierarchy
- [Ray Summary](docs/ray-summary.md) - Full system design

## 🤝 Contributing

This is an experimental research kernel. Key areas for contribution:
- RT Core BVH traversal optimization
- LLM integration patterns
- Multi-GPU load balancing algorithms
- Dream mode heuristics

## ⚠️ Current Limitations

- No actual RT Core invocation yet (simulated)
- LLM integration is stubbed
- Single-threaded CPU-side coordination
- No persistent storage
- No visual output

## 🧪 Testing Philosophy

> "The kernel tests itself by running."

Since this is a continuous simulation, traditional unit tests are supplemented with:
- Entropy monitoring
- Latency watchdogs
- Self-validation during dream mode

## 📄 License

MIT License - See LICENSE file

## 🔥 Vision

*"An operating system that thinks, dreams, and evolves. A computational organism, not just a resource manager."*

---

**Built with 🧠 by the RayOS Team**
*Making the GPU the new CPU, one ray at a time.*
