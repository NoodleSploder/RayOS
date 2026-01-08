# RayOS: What We're Building

**Created**: January 8, 2026  
**Current Phase**: 21 (Foundation Completion)  
**Total Phases Planned**: 25+

---

## What is RayOS?

RayOS is an experimental **Rust-based, UEFI-bootable operating system** implementing a revolutionary **bicameral kernel architecture**:

### The Bicameral Brain

```
┌─────────────────────────────────────────────────────────────┐
│                   RayOS Kernel                              │
├────────────────────┬──────────────────────────────────────┤
│  System 1: Reflex  │   System 2: Cognitive                │
│  (GPU Engine)      │   (LLM Intent Engine)                │
│                    │                                       │
│  • Real-time       │   • Natural Language Processing      │
│    processing      │   • Intent understanding             │
│  • Low-latency     │   • Task reasoning + planning        │
│  • Ray-tracing     │   • Knowledge retrieval (RAG)        │
│  • Compute         │   • Model inference                  │
│                    │                                       │
└────────────────────┴──────────────────────────────────────┘
         ↓                              ↓
    GPU Hardware                   Inference Runtime
                         ↓
                  ┌─────────────────┐
                  │  Conductor      │
                  │ (Orchestration) │
                  └─────────────────┘
```

### Key Characteristics

1. **Managed Subsystems (Option D Architecture)**
   - Linux runs as a managed guest VM (Wayland-first graphics)
   - Windows runs as a managed guest VM (VNC/GPU bridging)
   - RayOS maintains full authority: lifecycle, input, display, policy

2. **Native RayOS GUI**
   - Compositor that displays guest surfaces as embedded windows
   - Desktop-class application framework (RayApps)
   - Input routing: keyboard/mouse injection into guest VMs

3. **Infrastructure-First Design**
   - Strong boot/subsystem testing (headless automation)
   - Deterministic markers for all operations (CI-ready)
   - Persistent observability: logs, watchdog, recovery
   - Installer + boot manager for standalone deployment

---

## What We've Built (Phases 1-20)

### Phase Progression

| Phase | Focus | Status |
|-------|-------|--------|
| 9-10 | Core kernel infrastructure | ✅ Complete |
| 11-12 | Consensus + distributed systems | ✅ Complete |
| 13-14 | Container orchestration + scheduling | ✅ Complete |
| 15-16 | Storage + NUMA optimization | ✅ Complete |
| 17-18 | Security + encryption | ✅ Complete |
| 19 | API Gateway & service integration | ✅ Complete |
| 20 | Rate limiting & API governance | ✅ Complete |
| **21** | **Foundation completion** | 🟡 **Planned** |

### Completed Subsystems

#### Boot Infrastructure (Phases 1-3)
- ✅ UEFI bootloader (x86_64 + aarch64)
- ✅ Kernel loading (ELF + embedded modes)
- ✅ Post-ExitBootServices output (serial + framebuffer)
- ✅ Boot media generation (ISO + USB)
- ✅ Automated QEMU testing framework

#### GPU & System 1 (Phases 5-8)
- ✅ GPU initialization (GOP + virtio-gpu)
- ✅ Ray-tracing via Vulkan RT cores
- ✅ Compute dispatch via WGSL
- ✅ Watchdog/timeout strategies
- ✅ Performance profiling

#### LLM & System 2 (Phase 9)
- ✅ Intent parsing pipeline
- ✅ Tokenization + embeddings
- ✅ Neural intent classification
- ✅ Entity extraction
- ✅ RAG (retrieval-augmented generation)

#### Subsystem Infrastructure (Phase 9-10)
- ✅ Linux guest VM management
- ✅ Windows guest VM management
- ✅ Virtio device models (blk, net, gpu, input, console)
- ✅ Wayland-first graphics forwarding
- ✅ Input routing + lifecycle control
- ✅ Policy enforcement (network OFF by default)

#### Distributed Systems (Phases 11-18)
- ✅ Raft consensus
- ✅ BFT (Byzantine Fault Tolerant) consensus
- ✅ Service mesh
- ✅ Distributed storage
- ✅ Distributed transactions
- ✅ Container orchestration
- ✅ Security hardening
- ✅ Network encryption (TLS/DTLS)

#### Hypervisor & Virtualization (Phases 9-14)
- ✅ VMX/SVM hardware virtualization
- ✅ EPT/NPT memory mapping
- ✅ MMIO device emulation
- ✅ Interrupt delivery (VM-entry injection + LAPIC/MSI fallbacks)
- ✅ Guest memory protection
- ✅ Performance tuning (time-slicing, preemption)

#### API Platform (Phases 19-20)
- ✅ API Gateway (routing + mediation)
- ✅ Service authentication & authorization
- ✅ API monitoring & metrics
- ✅ Request prioritization & SLA enforcement
- ✅ Rate limiting (token bucket + leaky bucket)
- ✅ Quota management
- ✅ Cost tracking & attribution
- ✅ Policy engine + governance

---

## Phase 20 (Just Completed)

**Added**: Rate Limiting & API Governance Shell Commands

- 6 new shell commands: `ratelimit`, `quota`, `priority`, `cost`, `governance`, + policy
- 20+ subcommands for rate limiter control, quota allocation, cost tracking
- 1,815 lines of shell command infrastructure
- Fixed 5 compilation issues (borrow checker, types, attributes)
- Zero pre-existing test regressions

**Commits**:
```
bb43169 - Phase 20: Add Rate Limiting & API Governance Shell Commands
2c260ab - Add Phase 20 Final Report
```

---

## Phase 21: Foundation Completion (Planned)

**Goal**: Tie up the three highest-impact loose ends to make RayOS a production-ready OS.

### Three Critical Milestones

#### Milestone 1: RayOS-Native Linux Desktop
**Problem**: `show linux desktop` currently requires a host VNC viewer (e.g., `gvncviewer`)  
**Solution**: Validate + harden in-OS virtio-gpu scanout path; remove host dependencies  
**Impact**: Standalone RayOS GUI without external tools  
**Deliverable**: PresentationBridge module (800 lines)

#### Milestone 2: Installer & Boot Manager
**Problem**: RayOS runs in QEMU but isn't installable on bare hardware  
**Solution**: Build USB installer + boot menu for standalone deployment  
**Impact**: RayOS becomes a real installable OS, not a research VM  
**Deliverable**: InstallerBoot + BootManager modules (1,800 lines)

#### Milestone 3: Observability & Crash Recovery
**Problem**: No persistent logs, watchdog, or recovery from hangs  
**Solution**: Add durable logging, auto-reboot watchdog, "last known good" snapshots  
**Impact**: Prevents regressions; enables safe experimentation  
**Deliverable**: PersistentLog + Watchdog + BootMarker modules (2,200 lines)

### Phase 21 Task Breakdown

| Task | Module | Lines | Goal |
|------|--------|-------|------|
| 1 | `linux_presentation.rs` | 800 | Native scanout → RayOS window |
| 2 | `crates/installer` | 900 | USB boot + partition management |
| 3 | `boot_manager.rs` | 900 | Boot menu + installation discovery |
| 4 | `persistent_log.rs` + `watchdog.rs` | 800 | Durable logs + auto-recovery |
| 5 | `boot_marker.rs` | 700 | Golden state tracking |
| 6 | `shell.rs` (update) | 600 | CLI integration (20+ commands) |
| **Total** | **6 files** | **~4,800** | **Standalone OS** |

---

## Key Design Principles

### 1. **Authority Preservation**
RayOS maintains full control over:
- Subsystem lifecycle (start, stop, restart)
- Input routing (keyboard, mouse injection)
- Display management (present, hide, resize)
- Policy enforcement (CPU/memory/network limits)
- Recovery (reboot, recovery mode)

**Never depend on host tools**. Everything must work in-OS.

### 2. **Deterministic Markers**
Every operation emits structured markers for CI automation:
```
RAYOS_PRESENTATION:SURFACE_CREATE:42:1920x1080
RAYOS_BOOT_MARKER:KERNEL_LOADED
RAYOS_WATCHDOG:ARMED:30
RAYOS_INSTALLER:DISK_DETECTED:sda:1000GB
```

This enables **headless testing** without human observation.

### 3. **Safe Defaults**
```
• Networks OFF by default (enable explicitly)
• Watchdog armed by default (30 sec)
• Recovery enabled by default
• Installer requires 2-stage confirmation
• Boot manager timeouts at 30 seconds
```

### 4. **Fallback Pathways**
```
• Missing VNC viewer? → Print instructions
• No net boot? → Fall back to USB
• Boot fails 3×? → Auto-boot recovery
• Watchdog timeout? → Reboot to golden state
```

---

## Current Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    RayOS Kernel                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ Shell & CLI (20+ commands)                          │ │
│  │ • install, bootmgr, recovery, log, watchdog         │ │
│  │ • apimetrics, quota, ratelimit, priority, cost      │ │
│  └─────────────────────────────────────────────────────┘ │
│                        ↓                                  │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ Core Services (APIs, Governance, Subsystems)        │ │
│  │ • API Gateway (routing + mediation)                 │ │
│  │ • Rate Limiter (token bucket + leaky bucket)        │ │
│  │ • Quota Manager (allocation + enforcement)          │ │
│  │ • Policy Engine (declarative rules)                 │ │
│  │ • Linux Guest (Wayland-first)                       │ │
│  │ • Windows Guest (VNC/GPU bridging)                  │ │
│  │ • PresentationBridge (surfaces → RayOS windows)     │ │
│  │ • BootManager (multi-install detection)             │ │
│  │ • PersistentLog (durable logging)                   │ │
│  │ • Watchdog (hang detection + recovery)              │ │
│  └─────────────────────────────────────────────────────┘ │
│                        ↓                                  │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ System 1 (GPU) + System 2 (LLM)                     │ │
│  │ • Ray-tracing via Vulkan RT cores                   │ │
│  │ • Compute dispatch via WGSL                         │ │
│  │ • Intent parsing + neural classification            │ │
│  │ • RAG (retrieval-augmented generation)              │ │
│  └─────────────────────────────────────────────────────┘ │
│                        ↓                                  │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ Distributed Systems (Consensus, Storage, Mesh)      │ │
│  │ • Raft + BFT consensus                              │ │
│  │ • Distributed storage (HNSW, embeddings, logs)      │ │
│  │ • Service mesh + networking                         │ │
│  │ • Container orchestration                           │ │
│  └─────────────────────────────────────────────────────┘ │
│                        ↓                                  │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ Hardware Abstraction & Drivers                       │ │
│  │ • GPU (virtio-gpu, RT cores)                        │ │
│  │ • Storage (virtio-blk, persistent log partition)    │ │
│  │ • Network (virtio-net, encrypted tunnels)           │ │
│  │ • Input (virtio-input, ACPI)                        │ │
│  │ • Hypervisor (VMX/SVM, EPT/NPT)                     │ │
│  └─────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

---

## What Makes RayOS Different

### 1. **Bicameral Design**
Most OSes implement a monolithic command interpreter. RayOS splits into:
- **System 1 (GPU Reflex)**: Sub-millisecond response, deterministic
- **System 2 (LLM Cognitive)**: High-latency reasoning, probabilistic

This mirrors human cognition: immediate intuition (System 1) paired with deliberate reasoning (System 2).

### 2. **Managed Guest Architecture (Option D)**
Unlike traditional hypervisors that give guests autonomy:
- **Linux/Windows are controlled subsystems**, not peers
- RayOS owns the display (composits as windows)
- RayOS owns the input (injects key/mouse)
- RayOS owns the network (policy-enforced, OFF by default)
- RayOS owns the filesystem (via virtiofs)

This inverts the typical relationship: **guests serve the OS, not vice versa**.

### 3. **Authority Through Intent**
Users don't command the kernel; they express **intent**:
```
> show linux desktop          # High-level intent
  ↓ (System 2 parses, System 1 executes)
> create surface 42, set scanout, mark presented
  ↓ (kernel handles details)
> desktop appears in RayOS window
```

The kernel understands **what you want**, not just **what to do**.

### 4. **Production Readiness from Day 1**
Every feature includes:
- ✅ Deterministic markers (testable)
- ✅ Headless automation (CI-ready)
- ✅ Observability (logs, watchdog, recovery)
- ✅ Safety guards (confirmations, bounds checking)
- ✅ Documentation (why + how)

---

## Loose Ends Being Tied (Phase 21)

From RAYOS_TODO.md, we identified these high-impact unfinished items:

### Linux Subsystem (23e)
```
❌ VNC client detection (gvncviewer missing? → fallback)
❌ RayOS-native presentation (remove host bridge)
❌ Deterministic readiness without manual observation
❌ Dev harness defaults (auto-launch hidden VM)
```

### Installability (11, INSTALLABLE_RAYOS_PLAN.md)
```
❌ USB bootable installer
❌ Partition management + disk selection
❌ Boot manager discovery + selection
❌ Multi-installation support
❌ Recovery/rollback semantics
```

### Observability (3, OBSERVABILITY_AND_RECOVERY.md)
```
❌ Persistent kernel logs to persistent storage
❌ Watchdog timer + auto-reboot on hang
❌ Crash artifacts + recovery UI
❌ "Last known good" boot marker
❌ Automatic recovery snapshots
```

**Phase 21 directly addresses all 13 loose ends above.**

---

## The Three Milestones Explained

### Milestone 1: Native Linux Desktop (Remove External Dependencies)

**Current State**:
```
RayOS boots with `show linux desktop` → host bridge watches serial log
→ extracts QEMU monitor socket path → launches VNC viewer → user sees desktop
```

**Problem**: Requires `gvncviewer` (or equivalent) on host, QEMU monitor access, external process management.

**Phase 21 Target**:
```
RayOS boots with `show linux desktop` → kernel publishes scanout via virtio-gpu
→ PresentationBridge ingests frames → RayOS compositor displays as embedded window
→ User sees desktop (no host tools needed)
```

**Why It Matters**: RayOS becomes usable on **any Linux host** without tool dependencies. The desktop is truly part of RayOS, not borrowed from the host.

---

### Milestone 2: Installer & Boot Manager (Standalone OS)

**Current State**:
```
RayOS only runs under QEMU. To use RayOS "for real," you:
1. Build a QEMU image
2. Run QEMU from your host
3. Host machine runs everything else (BIOS, power, network)
```

**Problem**: RayOS is a research VM, not an OS. You can't install it on a USB or laptop.

**Phase 21 Target**:
```
1. Boot RayOS ISO on USB
2. Run `install` command
3. Select target disk + partitions
4. Installer copies RayOS to target, installs bootloader
5. Reboot from target disk
6. Boot manager appears, selects installation
7. RayOS launches natively without QEMU
```

**Why It Matters**: RayOS becomes a **real operating system**. You can ship it on USB, install it on laptops, demonstrate it at conferences. The host QEMU tooling becomes a dev/CI harness, not a dependency.

---

### Milestone 3: Observability & Crash Recovery (Reliability)

**Current State**:
```
RayOS boots. If a subsystem crashes:
- Serial log is lost (it's in RAM, not on disk)
- No watchdog (hang = dead kernel, needs manual reboot)
- No recovery (reboot = fresh start, no way to get back to last working state)
```

**Problem**: RayOS isn't reliable enough for production. A single bug can hang the system permanently.

**Phase 21 Target**:
```
1. Persistent logging: every log entry written to USB/disk (survives reboot)
2. Watchdog armed: if kernel hangs > 30 sec, auto-reboot
3. Golden marker: on successful boot, mark as "known good"
4. Recovery fallback: on hang/reboot, auto-boot from last golden state
5. Crash artifacts: save registers + backtrace for post-mortem analysis
```

**Why It Matters**: RayOS can fail gracefully. You don't lose work. You can debug crashes after the fact. The system self-heals.

---

## Success Criteria for Phase 21

### Technical
- [ ] 0 compilation errors
- [ ] 0 pre-existing test regressions
- [ ] 6 new modules, 4,800+ lines
- [ ] 20+ new shell commands
- [ ] 6 headless smoke tests (all passing)
- [ ] Deterministic markers for every operation

### Product
- [ ] `show linux desktop` works without VNC client
- [ ] `install` command installs RayOS to USB successfully
- [ ] `bootmgr` discovers installations and boots them
- [ ] `log show` displays persistent kernel logs
- [ ] Watchdog armed by default, reboots on timeout
- [ ] Recovery snapshots restore from failure

### Reliability
- [ ] 3× boot failure → auto-fallback to recovery
- [ ] Persistent logging survives reboot
- [ ] Crash artifacts saved for debugging
- [ ] Golden marker prevents boot loops
- [ ] No lost data on system reboot

---

## Next Phases (22-25)

After Phase 21 establishes foundation:

### Phase 22: RayApp Framework
- Native RayOS application container
- Window lifecycle + focus management
- Clipboard + file I/O sandbox
- Multi-app coordination

### Phase 23: Wayland-First GUI
- Real Wayland surface forwarding (replace PPM)
- Multi-app windows with titles/decorations
- Drag + drop support
- DPI scaling + zoom

### Phase 24: System Integration Testing
- Soak tests (7+ day runs)
- Stress tests (CPU/memory saturation)
- Failure injection (disk errors, network partition)
- Reproducible crash artifact collection

### Phase 25: Production Hardening
- Secure boot + measured boot (TPM 2.0)
- Signed binaries + module verification
- Encryption at rest (LUKS)
- Audit logging + compliance

---

## Summary

**RayOS is a research prototype of an AI-centric OS where the kernel speaks human intent.**

We've built:
- ✅ Bicameral kernel (GPU reflex + LLM cognition)
- ✅ Managed subsystems (Linux + Windows as controlled guests)
- ✅ API platform (gateway, rate limiting, governance)
- ✅ Distributed systems (consensus, storage, orchestration)
- ✅ Hypervisor (hardware virtualization, device models)
- ✅ Boot infrastructure (UEFI, kernel loading, testing)

**Phase 21 will complete the foundation by:**
1. Removing external host dependencies (native presentation)
2. Making RayOS installable on real hardware (USB installer)
3. Adding reliability features for production use (logging, watchdog, recovery)

This transforms RayOS from a "cool research VM" into a **feasible standalone operating system**.

---

**Document created**: January 8, 2026  
**Current status**: Phase 20 complete, Phase 21 planned  
**Next session**: Begin Phase 21 implementation
