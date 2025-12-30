# RayOS Linux Subsystem — High-Level Design (Managed Guest)

This document is the high-level design and interface contract for running Linux as a subordinate subsystem under RayOS.

Goal: RayOS runs Linux as a managed guest VM while RayOS retains authoritative control over presentation, input, and automation (voice + eye tracking).

---

## 1) Core Principle: Linux as a “Managed Guest,” Not a Peer OS

To make RayOS fully autonomous over Linux, Linux must not “own” the hardware surfaces that matter:

- **Display ownership**: RayOS owns the physical display pipeline (or at minimum the final compositor).
- **Input ownership**: RayOS owns keyboard/mouse/touch injection.
- **Policy ownership**: Linux runs inside a sandbox where RayOS enforces resources, permissions, and automation.

That implies Linux should run as a guest VM (or multiple guests) controlled by a RayOS Virtual Machine Monitor (VMM) and a RayOS Host Compositor.

### Persistence principle: a “living VM,” not a fresh one

Linux is not a disposable demo VM that gets recreated at each RayOS boot.

- RayOS creates a **named Linux VM instance** once (identity + config + backing storage).
- On subsequent RayOS boots/reboots, RayOS **reattaches to the same instance** and **resumes it** if a saved-state exists.
- By default, RayOS **boots/resumes Linux during RayOS boot** (background), but Linux remains **hidden and non-interactive** until explicitly presented.
- “Present/hide Linux desktop” is a **presentation routing decision**, not a “boot Linux from scratch” decision.

Practical interpretation:

- Minimum: persistent VM disk + stable VM identity (so the Linux environment persists even if the VM cold-boots).
- Target: persist/restore guest execution state (RAM/device model) so a RayOS reboot results in the **same ongoing session** after resume.

---

## 2) High-Level Architecture

### Components

#### RayOS VMM / Hypervisor Layer

- Boots and runs Linux as a VM.
- Exposes paravirtual devices (virtio*) for:
  - GPU/display (virtio-gpu + Wayland/DRM plumbing)
  - Input (virtio-input)
  - Network (virtio-net)
  - Storage (virtio-blk / virtio-fs)

#### RayOS Display Broker

- Captures Linux desktop frames (shared memory or GPU surfaces).
- Composites them into a RayOS scene graph (your “ray-traced UI” or simpler compositor initially).

#### RayOS Input Broker

- Injects pointer/keyboard events into Linux via virtio-input.
- Provides high-level commands (“click that,” “type password,” “drag window left”) by translating intent → discrete input sequences.

#### RayOS Multimodal Control Plane

- **Voice pipeline**: speech → text → intent → action plan.
- **Eye tracking pipeline**: camera → gaze vector → screen coords → pointer control / targeting.
- **Automation**: a “desktop agent” that can operate Linux UI deterministically.

### Data Flow Summary

- Linux renders desktop → RayOS captures frames → RayOS displays it (optionally inside a RayOS window/space).
- User speaks → RayOS decides UI action → RayOS injects input to Linux.
- Camera sees eyes → RayOS estimates gaze → RayOS moves pointer (and/or uses gaze as a target selector).

---

## 3) Choose a Practical Virtualization Strategy (Recommended: Start Here)

You want “new OS,” but you also want a Linux desktop quickly. The shortest path:

### Phase A (Fast Path): RayOS as a thin host + VMM

- RayOS boots, initializes hardware, then runs Linux VM.
- RayOS doesn’t need full driver coverage immediately—just enough to:
  - Manage memory
  - Schedule CPU cores
  - Own display/input
  - Run the VMM

### Implementation options (pick one)

1) KVM-like acceleration (if RayOS runs on hardware that supports VT-x/AMD-V and you implement a minimal hypervisor layer)
2) Custom VMM using hardware virtualization (RayOS implements VMX/SVM directly)
3) Software emulation (too slow for desktop; use only as fallback)

For your goals, you want (2) long-term. For getting it working, implement a minimal VMM with:

- EPT/NPT mappings
- VM exits (CPUID, IO port traps, MMIO traps)
- Virtio device model
- Interrupt injection (APIC virtualization or emulated)

---

## 4) Display Strategy: “Present Linux Desktop On Demand”

The key: Linux renders to a virtual GPU; RayOS consumes the output and decides when/how to display it.

Important default: Linux may already be running.

- Linux should boot/resume during RayOS boot (background).
- “Present” means “attach Linux output to RayOS presentation + enable interaction routing,” not “start the VM.”

### Recommended: virtio-gpu + shared memory / scanout surfaces

- Linux guest uses a standard driver for virtio-gpu.
- Guest renders into buffers that are shared with host (RayOS).
- RayOS composites those buffers.

### “Present Linux Desktop”

This becomes an API call inside RayOS:

- Bring Linux VM’s primary scanout surface into focus
- Possibly map it as a “panel/window” in RayOS
- Continue to receive frames continuously while visible

### “Hide Linux Desktop”

- Stop compositing it (but VM can keep running)
- Optionally reduce resource allocation (CPU/GPU time slice)

When hidden, RayOS should treat Linux as **not user-interactive** by default:

- Do not route keyboard/mouse/gaze input to Linux unless explicitly requested.
- Continue background execution only as permitted by policy.

---

## 5) Input Strategy: RayOS Owns Input, Linux Receives Virtual Input Only

You want:

- Voice controlling Linux UI
- Eye tracking controlling mouse

So Linux should never receive raw hardware input directly.

Instead:

- RayOS reads camera + mic + keyboard
- RayOS injects synthesized events into Linux

### Recommended: virtio-input

RayOS exposes a virtio-input device:

- pointer move
- button clicks
- scroll
- keyboard events

RayOS maintains a unified pointer state and sends deltas/absolute coordinates into Linux.

---

## 6) Eye Tracking Pipeline (Camera → Gaze → Cursor)

This is best treated as a RayOS-native service that outputs a stable screen-space target.

### Steps

- Camera capture
  - V4L2-like driver or USB UVC camera driver in RayOS
- Face/eye landmark detection
  - Start with a simple model (2D landmarks)
- Gaze estimation
  - Map landmarks → gaze vector
- Calibration
  - 5–9 point calibration to map gaze vector → screen coords
- Smoothing
  - Kalman filter / exponential smoothing (reduce jitter)
- Output
  - Publish (x, y) at ~60–120 Hz to the Input Broker

### Cursor control modes

- Direct gaze cursor: cursor follows gaze continuously
- Gaze target + confirm: gaze selects target, voice says “click” (far more usable)
- Dwell click: hover for N ms triggers click (can be fatiguing)

Given your “verbal commands” requirement, **gaze-target + voice-confirm** is the most robust.

---

## 7) Voice Control → Deterministic Desktop Actions

You do not want “LLM hallucinated input spam.” You want a constrained action DSL.

### Voice Flow

Speech → Text → Intent parse → Action plan → Execute → Verify

### Verification (important)

RayOS should confirm state using one of:

- UI introspection (best if Linux apps provide accessibility tree; harder in VM)
- Computer vision on the Linux frame buffer (works universally)
- Hybrid: OCR + icon/template matching

You can start with:

- OCR for text/buttons
- Simple template matching for common UI controls
- Cursor location feedback + window bounds detection

---

## 8) “RayOS Has Full Autonomy Over Linux” Means Enforceable Policy

Make Linux a managed workload with explicit limits:

- CPU quota / pinning
- Memory cap + ballooning
- I/O rate limits
- Network policy (deny/allow domains, proxies, MITM if you want)
- Filesystem boundaries (virtio-fs with explicit export set)
- “Kill switch” + snapshot/rollback

Also: RayOS should own Linux lifecycle:

- start / stop / suspend / snapshot / revert

“Present desktop” is just a display routing decision, not booting Linux.

---

## 9) Concrete Interface Contract (Copy/Paste for VS Code Agent)

### Project goal

RayOS runs Linux as a subordinate VM (“Linux Subsystem”). RayOS owns display+input and can:

1) present/hide the Linux desktop on demand,
2) inject input based on voice commands and eye tracking,
3) enforce resource and security policy on Linux.

### Repo layout (Rust)

```text
/kernel
  /vmm
    mod.rs
    vm.rs                // VM lifecycle (create/start/stop/snapshot)
    vmexit.rs            // VM exit handlers
    ept.rs               // EPT/NPT mapping layer
    virtio
      mod.rs
      virtio_gpu.rs      // virtio-gpu device model (scanout buffers)
      virtio_input.rs    // virtio-input device model (mouse/keyboard injection)
      virtio_blk.rs
      virtio_net.rs
  /display
    compositor.rs        // RayOS compositor / scene graph
    linux_surface.rs     // subscribes to virtio-gpu scanout buffers
  /input
    broker.rs            // unified pointer/keyboard state, routes to guests
    gaze.rs              // gaze->cursor policy + smoothing hooks
  /control_plane
    intent.rs            // intent parsing (voice text -> DSL)
    dsl.rs               // action DSL definitions + executor
    vision.rs            // OCR/template matching on Linux frames
    supervisor.rs        // policy enforcement, VM lifecycle authority
/services
  /speech
  /eye_tracking
  /desktop_agent
```

### Key runtime invariants

- Linux guest receives ONLY virtual devices for display+input.
- RayOS compositor is the ONLY thing that can present Linux frames to user.
- Input broker is the ONLY path to deliver pointer/keyboard to Linux.
- Supervisor can throttle, pause, snapshot, or kill Linux at any time.
- Linux VM instance identity (disk/config) is stable across RayOS reboots; resume-from-saved-state is preferred when available.

### TODO (Persistence)

- Define the persistent identity schema for Linux VM instances (name/id → disk paths → device config → policy).
- Define the “RayOS reboot path”: when possible, checkpoint/suspend the VM (or trigger guest hibernate) before reboot; on next boot, restore.
- Define fallback behavior when no saved-state exists (cold-boot same disk, still the same VM instance).

### Public kernel traits/APIs

```rust
trait GuestVm {
  fn start(&mut self) -> Result<()>;
  fn stop(&mut self) -> Result<()>;
  fn pause(&mut self) -> Result<()>;
  fn resume(&mut self) -> Result<()>;
  fn snapshot(&mut self, name: &str) -> Result<()>;
  fn revert(&mut self, name: &str) -> Result<()>;

  // Display surface handle produced by virtio-gpu scanout
  fn primary_surface(&self) -> Option<SurfaceHandle>;

  // Inject input
  fn inject_input(&mut self, ev: InputEvent) -> Result<()>;
}

enum InputEvent {
  PointerAbs { x: f32, y: f32 },   // normalized 0..1
  PointerRel { dx: i32, dy: i32 },
  Button { button: MouseButton, down: bool },
  Scroll { dx: i32, dy: i32 },
  Key { keycode: u16, down: bool },
  TextUtf8(String),
}

struct SurfaceHandle {
  // references a shared buffer (shmem) or GPU resource
  id: u64,
  width: u32,
  height: u32,
  format: PixelFormat,
}
```

### Display workflow

- virtio_gpu publishes scanout buffers → linux_surface converts to compositor texture
- compositor decides when to render it (“present linux desktop”)

### Control workflow

- speech service → control_plane::intent → DSL plan → executor
- executor may:
  - focus linux surface
  - move pointer to gaze target
  - click / type
  - verify with vision (OCR) from linux frames

### Eye tracking workflow

- eye_tracking service publishes GazePoint {x,y,confidence}
- input::gaze smooths + applies mode:
  - gaze_target_only: updates target reticle, no cursor move
  - gaze_controls_cursor: emits PointerAbs continuously

### Voice commands examples mapped to DSL

- "show linux desktop" → PresentSurface(LinuxPrimary)
- "hide linux desktop" → HideSurface(LinuxPrimary)
- "click" → ClickAt(CurrentGazeTarget)
- "type hello" → TextUtf8("hello")
- "open terminal" → (vision find "Terminal") → move/click → verify

### Minimum deliverable

- Boot RayOS
- Start Linux VM with virtio-gpu + virtio-input
- Capture Linux scanout into RayOS compositor
- Inject pointer events into Linux from RayOS
- Implement "present/hide linux desktop" command

---

## 10) Practical Execution Plan (Milestones)

### Milestone 1: Linux VM boots under RayOS

- Minimal VMM: memory map + CPU bring-up + basic devices
- virtio-blk for root disk
- virtio-net optional
- serial console for debug

### Milestone 2: Linux desktop frames visible in RayOS

- virtio-gpu scanout surfaces
- RayOS compositor draws Linux surface to screen
- Simple “present/hide” toggle

### Milestone 3: Inject mouse/keyboard

- virtio-input device
- absolute pointer mapping (normalized 0..1 to guest coords)
- basic clicking

### Milestone 4: Eye tracking moves cursor / target reticle

- camera capture + gaze estimation service
- smoothing + calibration
- gaze target mode + voice “click”

### Milestone 5: Voice-driven automation with verification

- intent → DSL
- executor injects input sequences
- OCR/vision verifies expected UI state

---

## 11) Key Design Choice You Should Make Early

Decide whether Linux is presented as:

- A RayOS “window” (Linux desktop inside a RayOS-managed rectangle), or
- A full-screen takeover mode (RayOS still owns it, but dedicates the whole display)

Start with the full-screen takeover mode for simplicity; the windowed mode is the natural evolution.
