# How Windows 11 Runs “Under” RayOS (Subsystem Design Notes)

Authoritative invariants: see [WINDOWS_SUBSYSTEM_CONTRACT.md](WINDOWS_SUBSYSTEM_CONTRACT.md).

Goal: run Windows 11 as a **managed guest subsystem** where **RayOS is always the authority** for lifecycle, GPU, input, and presentation.

Additional core goal (persistence):

> RayOS manages a **long-lived Windows VM**. RayOS boots/reboots should **resume or reattach** to an existing Windows VM instance, not create a brand new Windows environment each time.

Interpretation:

- Minimum: stable VM identity + persistent disk(s) so the Windows environment persists across RayOS boots.
- Target: persist/restore execution state (RAM/device model) so Windows can resume the same session after a RayOS reboot.

## 1) Execution Model: Type-1 Hypervisor or Micro-VMM

RayOS must include (or embed) a minimal hypervisor/VMM layer, comparable to:

- A stripped-down KVM-like VMM
- A microkernel-style VMM (seL4-like philosophy)
- A custom Rust VMM (principles similar to Firecracker / crosvm)

Key requirement:
- Windows never runs in ring-0 relative to hardware. RayOS does.

Windows executes in:
- VT-x / AMD-V guest mode
- With synthetic devices only

## 2) GPU Control: RayOS Owns the GPU

This is the most important design decision.

- Do NOT passthrough the GPU directly.

Instead:
- RayOS runs a persistent GPU “megakernel”
- Windows sees a virtual GPU
- RayOS intercepts DXGI / WDDM calls
- RayOS schedules GPU work
- RayOS mirrors output to a RayOS-managed surface

This allows:
- RayOS to pause Windows visually
- Re-render Windows at different resolutions
- Inject overlays
- Replace mouse with gaze
- Freeze frames for reasoning

Windows thinks it has a GPU.
RayOS decides when and how it renders.

## 3) Display Model: Windows Is a Texture, Not a Screen

Windows does not own the display.

Instead:
- Windows desktop → offscreen framebuffer
- RayOS maps that framebuffer as:
  - A floating panel
  - A window
  - A virtual monitor
  - A spatial object in 3D space

RayOS can:
- Crop
- Scale
- Reproject
- Time-slice
- Ray-trace it into the compositor

This aligns with a ray-logic / BVH-driven compositor flow.

## 4) Input Model: No Direct HID Access

Windows never sees raw input.

RayOS captures:
- Keyboard
- Mouse
- Eye-tracking vectors
- Voice intent

RayOS translates them into synthetic HID events.

Examples:
- Eye gaze → virtual mouse delta
- Voice command → Win32 automation
- Intent → sequence of UI interactions

Windows believes a user is interacting normally.
In reality, RayOS is the user.

## 5) Autonomy: RayOS Can Suspend, Script, or Override Windows

Because Windows is sandboxed:

RayOS can:
- Freeze the VM
- Snapshot RAM + GPU state
- Roll back UI actions
- Script workflows deterministically
- Run Windows headless
- Resume only when needed

Windows becomes:
- A compatibility layer
- A legacy app substrate
- A tool—not an authority

## Boot Sequence (Concrete)

1) Firmware → RayOS bootloader
2) RayOS kernel initializes:
  - CPU topology
  - GPU megakernel
  - Input sensors
3) RayOS policy engine loads
4) RayOS locates the **existing Windows VM instance** (identity + config + backing storage)
5) If a saved-state exists and policy allows it, RayOS resumes the VM; otherwise it cold-boots the same persistent disk
6) Windows VM runs in the background after RayOS boot, but is **not visible** and **not user-interactive** by default
7) User (or RayOS) requests: “present my Windows desktop”
8) RayOS:
  - Attaches presentation to the running VM
  - Maps framebuffer
  - Enables synthetic input routing (interactive)
9) User leaves Windows
10) RayOS may hide the surface and optionally pause/suspend the VM (stateful), rather than destroying it

Note: “present desktop” should never imply “create a new Windows VM.” It only changes whether the already-managed VM is visible/interactive.

## Why This Is Fundamentally Different from WSL / Hyper-V

| Feature | WSL / Hyper-V | RayOS Subsystem |
|---|---|---|
| Host OS | Windows | RayOS |
| Control | Windows-centric | RayOS-centric |
| GPU ownership | Windows | RayOS |
| Input authority | Windows | RayOS |
| Autonomy | User-driven | Intent-driven |
| Presentation | Fixed desktop | Spatial / conditional |

Windows cannot escape RayOS.

## Hard Constraints (Honest Engineering Reality)

You must solve or accept:

- Secure Boot / TPM
  - Windows 11 requires TPM → virtual TPM required
- WDDM expectations
  - Virtual GPU must behave plausibly
- Driver surface area
  - Fewer devices = fewer headaches
- Licensing
  - Windows licensing still applies

None are blockers. All are solvable.

## Bottom Line

Yes—Windows 11 can run as a RayOS-controlled subsystem.

The correct mental model is:

Windows is a simulated behavioral environment rendered and governed by RayOS—not an operating system in charge of anything.

## Next Steps to Formalize

Pick what to formalize next:

- Selecting the initial target hardware (x86_64 vs ARM)
- Designing the RayOS hypervisor boundary
- Defining the virtual GPU contract
- Drafting the Windows subsystem policy schema

### TODO (Persistence)

- Define a persistent Windows VM registry (name/id → disk paths → firmware/TPM config → device model → policy).
- Define the RayOS reboot pathway: checkpoint/suspend the VM (or trigger guest hibernate) before reboot; restore on next boot.
- Define fallback behavior when no saved-state exists (cold-boot same disk, still the same VM instance).
