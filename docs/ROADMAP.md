# RayOS Roadmap

**Last Updated**: January 2026

---

## Current Focus: UI Framework & App Platform

### Completed ✅

| Feature | Description |
|---------|-------------|
| Kernel Boot | x86_64 and aarch64 UEFI boot |
| Framebuffer | Direct framebuffer rendering |
| Window Manager | Create, move, resize, focus windows |
| Compositor | Z-order compositing, decorations |
| Mouse Input | PS/2 driver with cursor |
| Keyboard Input | Text input handling |
| Linux VM | Running as managed guest |
| Local AI | In-kernel LLM inference |
| Process Explorer | Graphical process/system monitor |
| System Log | In-kernel event journal for diagnostics |
| Widget Library | Button, Label, TextInput widgets |
| Layout System | VStack, HStack, Grid containers |
| VM Window | Linux desktop as native window with input routing |
| Linux Graphics | Resolution tracking, FPS overlay, bilinear scaling |
| App SDK | AppDescriptor, AppContext, lifecycle hooks, example apps |
| VS Code Extension | Build commands, snippets, .rayapp syntax, QEMU integration |
| Windows VM | Windows subsystem with UEFI, TPM, Hyper-V enlightenments |
| Package Format | .rayapp package structure, loader, and shell commands |
| App Store | App discovery, browsing, and installation from catalog |
| Standalone Deployment | Installer, update mechanism, recovery mode |
| Font Rendering | Scalable fonts with 9 sizes, anti-aliasing, glyph cache |
| Animations | Window transitions with easing, fade, scale, and pop effects |

### Completed ✅ (Phase 2)

| Feature | Completed | Notes |
|---------|-----------|-------|
| Reflex Learning | 2026 | HabitLearner detects patterns, System 2 approves/rejects |

### Planned 📋

| Feature | Target | Notes |
|---------|--------|-------|
| Multi-monitor | 2027 | Hardware support for multiple displays |

---

## Sentient Substrate 🧠

Core architectural components for RayOS as a cognitive substrate. See [SENTIENT_SUBSTRATE.md](SENTIENT_SUBSTRATE.md) for full design.

### Bicameral Kernel

| Task | Status | Description |
|------|--------|-------------|
| System 2: Resident LLM | ✅ Done | LLM inference integrated in kernel |
| System 1: Reflex Engine | ✅ Done | Pattern matching, reflexes, attention signals |
| System 1: GPU Compute Shaders | ✅ Done | GPU reflex pattern matching with WGSL compute shader |
| Attention Buffer Protocol | ✅ Done | System 1 sends attention signals to System 2 via ray queue |
| Reflex Learning | ✅ Done | HabitLearner + System 2 approval/rejection API |
| Perceptual Upward Signals | ✅ Done | System 1 notifies System 2 of anomalies/intent |
| Downward Control Commands | ✅ Done | Full System 2 control API (add/remove/enable/disable/priority/suppress) |

### Logic as Geometry

| Task | Status | Description |
|------|--------|-------------|
| RT Core Logic Encoding | ✅ Done | Encode conditionals as ray-geometry intersections |
| BVH Decision Trees | ✅ Done | Map decision trees to bounding volume hierarchies |
| Access Control Geometry | ✅ Done | GPU compute shader for geometric permission hit tests |
| Ray-Based State Access | ✅ Done | Variables as spatial structures (state_geometry.rs) |
| Unified Perception/Logic Pipeline | ✅ Done | Single GPU pipeline for perception, logic, and semantics |

### Neural File System

| Task | Status | Description |
|------|--------|-------------|
| Vector Store (Hippocampus) | ✅ Done | HNSW index for semantic memory with O(log n) ANN search |
| GPU-Accelerated Similarity | ✅ Done | WGSL compute shader for parallel cosine similarity |
| Multi-Modal Embedder | ✅ Done | Text, code, image, audio → vectors with modality-aware features |
| Content Ingestion Pipeline | ✅ Done | Automatic embedding on file events with debouncing and batching |
| Epiphany Buffer | ✅ Done | Connection discovery, dream scheduling, scoring, promotion |
| Semantic Query Interface | ✅ Done | Natural language parsing, query expansion, multi-factor ranking |
| Relationship Inference | ✅ Done | Automatic concept linking with knowledge graph and inference engine |

---

## Milestones

### M1: Native Linux Desktop (Q1 2026)

Linux VM runs in a native RayOS window with:
- virtio-gpu scanout
- Input routing
- Window decorations

### M2: App Framework Alpha (Q2 2026)

First public SDK release:
- Widget library
- Layout system
- Documentation

### M3: VS Code Integration (Q2 2026)

Developer tooling:
- Project templates
- Build tasks
- Debug adapter

### M4: Standalone Deployment (Q3 2026)

Production-ready installation:
- Installer
- Update mechanism
- Recovery mode

### M5: Sentient Substrate Alpha (2027)

Cognitive architecture foundation:
- Bicameral Kernel with GPU reflexes
- Neural File System with semantic search
- Logic as Geometry proof-of-concept

---

## Technical Debt

| Item | Priority | Notes |
|------|----------|-------|
| Multi-monitor | Medium | Future hardware support |
| Subpixel rendering | Low | LCD subpixel optimization for fonts |
| GPU acceleration | Low | Hardware-accelerated compositing |

---

## See Also

- [Sentient Substrate](SENTIENT_SUBSTRATE.md) - Core cognitive architecture
- [Framework Roadmap](development/FRAMEWORK_ROADMAP.md) - Detailed app framework plans
- [App Development](development/APP_DEVELOPMENT.md) - Building apps for RayOS
