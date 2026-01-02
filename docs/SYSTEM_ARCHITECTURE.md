
# RayOS System Architecture (Top-Level)

Status: **draft (concrete v0 architecture)**

Purpose: unify the end-to-end architecture across boot, kernel/runtime, subsystems (Linux/Windows VMs), presentation, input routing, policy, storage, and AI components.

This document describes the *installed* RayOS target, and also maps to the current developer harness (QEMU + host scripts) where applicable.

---

## 1) Architecture Goals (v0)

- One end-to-end view of services, boundaries, and primary data/control paths.
- Explicit ownership: who decides lifecycle, input routing, networking, and persistence.
- Deterministic markers for automation and recovery (see `OBSERVABILITY_AND_RECOVERY.md`).

Non-goals (v0):
- Not a full microkernel spec.
- Not a complete compositor protocol spec (see subsystem docs for today’s v0 control plane).

---

## 2) High-Level View

RayOS is a host OS that treats Linux/Windows as *managed guests*, not as peers:

- **RayOS is authoritative** over guest lifecycle, devices, persistence, and policy.
- Guests are **compatibility projections** used to run apps; their UI is presented as RayOS surfaces and is gated by policy.

See `docs/architecture_diagram.md` for an intent/perception-oriented view; the rest of this document focuses on concrete boot/runtime boundaries.

---

## 3) Boot Chain & Modes (v0)

### 3.1 Boot chain

UEFI firmware → RayOS bootloader → RayOS runtime:

- Bootloader responsibilities:
  - locate/load RayOS runtime artifacts
  - stage required data (policy, registry pointers, optional Volume artifacts)
  - emit boot markers and hand off with a minimal ABI (`BootInfo`)
- Runtime responsibilities:
  - bring up console/logging
  - mount RayOS Data (`/var/rayos`)
  - start core services (policy, storage, VM supervisor, presentation/input)

### 3.2 Boot entries

RayOS installs multiple boot entries (see `INSTALLER_AND_BOOT_MANAGER_SPEC.md`):

- `RayOS` (normal)
- `RayOS (recovery)` (always boots recovery UX/tools)
- `RayOS (previous)` (optional convenience rollback)

---

## 4) Core Services (v0)

These are logical services; early v0 may co-locate them in one process, but the boundaries should remain explicit.

### 4.1 Policy Engine

Source-of-truth: `RAYOS_DATA/policy/rayos-policy.toml` (`POLICY_CONFIGURATION_SCHEMA.md`)

Responsibilities:
- decide whether a guest may be started/resumed
- decide whether a guest may be presented (“presentation gating”)
- decide whether guest networking is allowed
- enforce resource limits (CPU/mem caps)
- log violations as stable markers: `RAYOS_POLICY_VIOLATION:<rule>:<detail>`

### 4.2 Storage / Volume Service

Responsibilities:
- mount/manage RayOS Data layout (`DISK_LAYOUT_AND_PERSISTENCE.md`)
- load/query Volume (vector store) for RAG paths when enabled
- provide a stable location for logs and crash artifacts

### 4.3 VM Supervisor

Responsibilities:
- maintain VM identity and configuration (registry)
- start/resume/stop Linux/Windows VMs
- ensure “hidden vs presented” state is respected
- manage guest devices (virtio input/gpu/net) according to policy
- maintain VM health/readiness markers

VM identity:
- stable VM IDs with disk/state paths under `RAYOS_DATA/vm/...`
- registry is the canonical mapping from ID → storage/device config

### 4.4 Presentation & Compositor

Responsibilities:
- own the display and the user-visible composition of surfaces
- gate guest “desktop” surfaces by policy and user intent
- map guest output into RayOS surfaces (single-surface embed in v0; multi-surface later)

v0 milestones:
- Linux: single “embedded desktop” surface, initially via a prototype transport, later via a real Wayland-first forwarding path (see `LINUX_SUBSYSTEM_DESIGN.md`)

Product constraint:
- The installed-RayOS path must present guest UI **inside RayOS** as RayOS-owned surfaces/windows.
- Network-based viewers (e.g. VNC clients) may be used in the developer harness only.

### 4.5 Input Broker

Responsibilities:
- accept input from physical devices + sensors (keyboard/mouse, gaze, voice, etc.)
- route inputs to the active RayOS surface/window
- inject inputs into guests only when presented and allowed by policy
- provide audit markers for sensitive actions (input injection and lifecycle control)

### 4.6 Intent / AI Runtime (optional in v0)

Responsibilities:
- interpret user signals (text/voice/gaze) into intents
- select targets (objects/surfaces) using attention models
- produce “intent envelopes” that are checked against policy before execution

In v0 bring-up, a text REPL can stand in for this layer; the architecture assumes it becomes a continuous model over time.

---

## 5) Primary Data/Control Paths (v0)

### 5.1 Intent → Policy → Effects

1) Inputs (text/voice/gaze) arrive at Intent resolution.
2) Intent resolution produces an **intent envelope** (action + target + confidence).
3) Policy Engine authorizes/denies (and may require explicit confirmation).
4) Executors perform effects:
   - native RayOS UI effects (future)
   - guest lifecycle actions (start/stop/present)
   - guest input injection (type/press/mouse/click)

### 5.2 Linux subsystem (today’s v0 control plane)

Control actions:
- start/present/hide desktop
- type / press key / mouse / click
- shutdown

Determinism requirements:
- emit versioned host events (`RAYOS_HOST_EVENT_V0:<op>:<payload>`) for automation
- emit ACK markers (`RAYOS_HOST_ACK:<op>:<ok|err>:<detail>`)

This “host bridge” exists today in the developer harness, but the same event/ACK semantics should be preserved when the supervisor becomes an in-OS service.

### 5.3 Persistence and continuity

Authoritative state lives under `/var/rayos`:
- policy file
- VM registry
- VM disks/state
- logs and crash artifacts

VM continuity goals:
- minimum: persistent disks (cold-boot resume)
- target: saved-state restore (RAM/device) where supported, guarded by version compatibility

---

## 6) Security Invariants (summary)

See `SECURITY_THREAT_MODEL.md` for full detail. The architecture assumes:

- Guests are untrusted; device exposure is explicitly policy-governed.
- “Presented” gates input routing into guests.
- Guest networking is off by default and must be explicitly enabled by policy.
- All lifecycle and policy decisions are logged with stable markers.

---

## 7) Developer Harness Mapping (current repo)

Today’s repo provides a “host harness” that simulates missing in-OS services:

- `scripts/test-boot.sh` boots RayOS in QEMU and watches serial logs.
- On specific markers, it launches/controls guest VMs (Linux/Windows) as separate QEMU instances.
- This is *not* a security boundary and must not be treated as one; it is a developer/CI tool.

Installability work (see `INSTALLABLE_RAYOS_PLAN.md`) migrates these responsibilities into the installed RayOS runtime.

- The kernel cannot reason semantically

RayOS embeds interpretation below applications, at the kernel cognition layer.

---

## 4) GPU-First High-Performance Compute Control Planes

### 4.1 The Fundamental Reversal

Traditional OS model:

  CPU schedules → GPU accelerates

RayOS model:

  GPU simulates → CPU assists

The GPU is not a peripheral. It is the primary execution substrate.

### 4.2 Persistent GPU Residency

RayOS runs:
- Persistent kernels
- Continuous execution loops
- No launch/teardown cycles

This eliminates:
- Kernel launch latency
- PCIe round-trips
- CPU-mediated scheduling overhead

The GPU never goes idle unless the system sleeps.

### 4.3 Scheduling as Spatial Allocation, Not Time Slicing

Linux scheduling asks:

  “Who runs next?”

RayOS scheduling asks:

  “Where does this computation live in the simulation space?”

Compute is allocated by:
- Spatial locality
- Data affinity
- Thermal constraints
- Energy cost
- Priority rays

This is geometry-based scheduling, not queue-based scheduling.

### 4.4 Multi-GPU as a Single Cognitive Surface

Multiple GPUs are treated as:
- A single distributed simulation fabric
- With work stealing as ray diffusion
- With locality encoded in BVHs or spatial graphs

There is:
- No “primary” GPU
- No rigid master/slave topology
- No fixed NUMA boundaries

The system dynamically reshapes itself.

### 4.5 Zero-Copy, Zero-Context Switching

RayOS minimizes:
- Memory copies
- Context switches
- Driver mediation

Key principles:
- Shared memory pools
- Explicit ownership transfer
- Predictive data placement

This is essential for:
- Real-time perception
- Autonomous decision loops
- Large-scale simulations

### 4.6 Control Plane, Not Just Compute Plane

RayOS is not merely running workloads. It is reasoning about them.

The control plane:
- Observes performance continuously
- Adjusts execution strategies
- Reallocates resources proactively
- Explains why decisions were made

This enables:
- Self-optimizing systems
- Explainable scheduling
- Policy-based compute governance

### 4.7 Why This Matters for RayOS Specifically

This architecture enables RayOS to:
- Host Linux as a compatibility projection
- Drive robotics and autonomy stacks directly
- Run AI workloads without orchestration layers
- Treat computation as an evolving world state

Linux manages processes. RayOS manages reality models.

---

## 5) Synthesis: Why 3 and 4 Must Exist Together

These two areas reinforce each other:

| HCI Without UI      | GPU Control Plane      |
|---------------------|-----------------------|
| Continuous perception | Continuous execution |
| Probabilistic intent  | Probabilistic scheduling |
| Attention modeling    | Resource modeling    |
| No interrupts        | No context switches  |

Together, they form:

> A perceptual operating system that both understands and acts continuously.

---

## 6) Contracts / Invariants

- RayOS is authoritative for display + input + lifecycle + policy.
- Linux/Windows are long-lived VMs: start/resume at RayOS boot (hidden), present on request.
- “Presented” gates interactive input routing.

---

## 7) Open Questions

- Event bus vs direct service calls?
- Where does policy live (file format + update mechanism)?
- What is the minimum “installable” hardware envelope?

---

## 8) Related

- Linux: LINUX_SUBSYSTEM_DESIGN.md, LINUX_SUBSYSTEM_CONTRACT.md
- Windows: WINDOWS_SUBSYSTEM_DESIGN.md, WINDOWS_SUBSYSTEM_CONTRACT.md
- Installability: INSTALLABLE_RAYOS_PLAN.md
