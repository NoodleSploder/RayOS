
# RayOS System Architecture (Top-Level)

Status: **draft / tracking stub**

Purpose: unify the end-to-end architecture across bootloader, kernel/runtime, subsystems (Linux/Windows VMs), compositor/presentation, input routing, policy, storage, and AI components.

---

## 1) Goals

- Provide a single “how it all fits together” view.
- Define the long-running services/processes and their boundaries.
- Define the primary communication paths (events/IPC) and ownership.

---

## 2) Components (to formalize)

- Boot chain: UEFI → RayOS bootloader → RayOS runtime
- Core runtime services:
  - VM Supervisor (Linux + Windows lifecycle)
  - Display/Compositor (presentation gating)
  - Input Broker (routing + injection)
  - Policy Engine
  - Volume/Storage service
  - Optional AI runtime (local inference)

---

## 3) Human–Computer Interaction Without Traditional UI

### 3.1 The Core Shift: Interaction → Interpretation

Traditional operating systems assume:

- A discrete user
- Issuing explicit commands
- Through mechanical interfaces (keyboard, mouse, touch)

RayOS assumes:

- A continuous human presence
- Emitting signals, not commands
- That must be interpreted probabilistically

RayOS does not wait for events. It continuously models user intent.

### 3.2 Input Is a Sensor Stream, Not an Event

In RayOS, inputs are not interrupts. They are fields.

| Input      | Linux Interpretation | RayOS Interpretation   |
|------------|---------------------|------------------------|
| Eye gaze   | Pointer movement    | Attention vector       |
| Voice      | Text command        | Intent + urgency       |
| Head pose  | Ignored             | Spatial orientation    |
| Silence    | Idle                | Cognitive pause        |
| Hesitation | Ignored             | Uncertainty signal     |

Each input modality becomes a ray-emitting sensor, contributing to a probabilistic model of what the user is trying to do.

### 3.3 Gaze as a First-Class System Primitive

Instead of:

  mousemove(x, y)

RayOS reasons in terms of:

  focus_probability(object_id, duration, context)

**Example:**

If the user:
- Looks at a window
- Pauses for 600ms
- Then says “open this”

RayOS resolves:

“This” = the object with the highest gaze-confidence score

No pointer, no click, no ambiguity. This is selection by inference, not by action.

### 3.4 Voice Is Not a Command Line

In RayOS, voice input is not parsed into verbs and flags. It is embedded into an intent lattice.

**Example:**

“Can we take a look at that again?”

This resolves into:
- Reference resolution (“that”)
- Temporal context (“again”)
- Action ambiguity (inspect? restore? replay?)

RayOS does not error. It asks follow-up questions only when confidence is below threshold.

### 3.5 UI as a Projection, Not the Control Surface

In RayOS:
- UI is rendered output, not control input
- The OS state exists independently of windows or widgets
- UI adapts to attention, not vice versa

This allows:
- UI to disappear entirely
- AR/VR overlays instead of desktops
- Accessibility-first computing without special modes

The “desktop” becomes a lens, not a workspace.

### 3.6 Why Linux Cannot Be Extended to Do This

Linux fails here because:
- Inputs are interrupt-driven
- Meaning is resolved in applications, not the OS
- There is no global attention model
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
