# RayOS Remaining Work (Tracked TODOs)

This document consolidates the remaining work for RayOS, compiled from the project’s phase/design documents and the explicit TODO markers in code.

Sources used:
- Phase roadmap: [PHASE1_COMPLETE.md](PHASE1_COMPLETE.md), [PHASE2_PLAN.md](PHASE2_PLAN.md), [QUICKSTART.md](QUICKSTART.md), [SESSION_SUMMARY.md](SESSION_SUMMARY.md)
- Kernel status/limitations: [kernel/IMPLEMENTATION.md](kernel/IMPLEMENTATION.md), [kernel/README_KERNEL.md](kernel/README_KERNEL.md)
- Memory bring-up notes: [MEMORY_MANAGEMENT_SESSION.md](MEMORY_MANAGEMENT_SESSION.md)
- Boot notes: [bootloader/README-uefi.md](bootloader/README-uefi.md), [BOOT_VERIFICATION.md](BOOT_VERIFICATION.md)
- Linux Subsystem design + contract: [LINUX_SUBSYSTEM_DESIGN.md](LINUX_SUBSYSTEM_DESIGN.md), [LINUX_SUBSYSTEM_CONTRACT.md](LINUX_SUBSYSTEM_CONTRACT.md)
- Windows Subsystem design + contract: [WINDOWS_SUBSYSTEM_DESIGN.md](WINDOWS_SUBSYSTEM_DESIGN.md), [WINDOWS_SUBSYSTEM_CONTRACT.md](WINDOWS_SUBSYSTEM_CONTRACT.md)
- Subsystems: [cortex/README.md](cortex/README.md), [volume/README.md](volume/README.md), [intent/PHASE5_SUMMARY.md](intent/PHASE5_SUMMARY.md)
- Installability plan: [INSTALLABLE_RAYOS_PLAN.md](INSTALLABLE_RAYOS_PLAN.md)
- Design tracking stubs: [SYSTEM_ARCHITECTURE.md](SYSTEM_ARCHITECTURE.md), [DISK_LAYOUT_AND_PERSISTENCE.md](DISK_LAYOUT_AND_PERSISTENCE.md), [INSTALLER_AND_BOOT_MANAGER_SPEC.md](INSTALLER_AND_BOOT_MANAGER_SPEC.md), [POLICY_CONFIGURATION_SCHEMA.md](POLICY_CONFIGURATION_SCHEMA.md), [UPDATE_AND_RECOVERY_STRATEGY.md](UPDATE_AND_RECOVERY_STRATEGY.md), [SECURITY_THREAT_MODEL.md](SECURITY_THREAT_MODEL.md)
- Observability/recovery stub: [OBSERVABILITY_AND_RECOVERY.md](OBSERVABILITY_AND_RECOVERY.md)

Status key:
- ✅ done
- ⏳ in progress
- ⛔ blocked (needs a prerequisite)
- 💤 deferred (not required for current phase)

---

## Design docs to write (tracked)

These are the missing project-level documents needed to make RayOS installable and operable as a standalone OS (beyond the existing boot/subsystem docs). Each is currently a stub and should be filled in over time.

40) System architecture (unified)
- Doc: [SYSTEM_ARCHITECTURE.md](SYSTEM_ARCHITECTURE.md)
- Goal: one end-to-end view of services, boundaries, and communication paths.

41) Disk layout & persistence spec
- Doc: [DISK_LAYOUT_AND_PERSISTENCE.md](DISK_LAYOUT_AND_PERSISTENCE.md)
- Goal: partitions/filesystems, VM registry location, VM disk/state locations, log locations, invariants.

42) Installer + boot manager spec
- Doc: [INSTALLER_AND_BOOT_MANAGER_SPEC.md](INSTALLER_AND_BOOT_MANAGER_SPEC.md)
- Goal: USB boot installer wizard, partition selection/creation, boot entries, recovery entry, boot manager decision.

43) Policy configuration schema
- Doc: [POLICY_CONFIGURATION_SCHEMA.md](POLICY_CONFIGURATION_SCHEMA.md)
- Goal: concrete policy format and controls (VM autoboot hidden, present gating, networking defaults, device exposure, resource caps).

44) Update + recovery strategy
- Doc: [UPDATE_AND_RECOVERY_STRATEGY.md](UPDATE_AND_RECOVERY_STRATEGY.md)
- Goal: update mechanism, rollback, recovery mode, compatibility/versioning rules.

45) Security & threat model
- Doc: [SECURITY_THREAT_MODEL.md](SECURITY_THREAT_MODEL.md)
- Goal: trust boundaries, invariants, secure/measured boot posture, auditing, key management.

46) Observability & crash recovery
- Doc: [OBSERVABILITY_AND_RECOVERY.md](OBSERVABILITY_AND_RECOVERY.md)
- Goal: persistent logs, health/readiness markers, crash artifacts, and recovery UX that does not require another machine.

## Production readiness (audit 2025-12-29)

This repo has strong, repeatable **headless smoke tests** and clear boot markers, but it is **not production-ready as an OS** in the conventional sense (secure boot, hardening, driver maturity, stability/perf testing, telemetry, update strategy, etc.).

### Installability (tracked plan)

- Track: [INSTALLABLE_RAYOS_PLAN.md](INSTALLABLE_RAYOS_PLAN.md) (goal: RayOS is installable and does not rely on another machine; host-side QEMU/conductor becomes dev/CI harness only).

- **x86_64:** Headless boot + RAG + Cortex protocol paths have automated QEMU scripts. “System 1 GPU” exists in the std `kernel` path, but production readiness would still require sustained soak testing, failure-mode handling, and a security story (secure boot/measured boot, signed artifacts, sandboxing).
- **aarch64:** UEFI bring-up + ELF-load+jump + post-`ExitBootServices` UART loop are automated. GPU work is currently **framebuffer/GOP discovery**, not compute adapter/device initialization, so “System 1 GPU reflex engine on aarch64” is not production-ready.

---

## P0 — Unblock “Real Kernel on aarch64”

1) Choose Phase 2 execution path (decision)
- Option A (fast PoC): embed GPU + LLM init into the UEFI bootloader
- Option B (proper OS): build a real bare-metal `aarch64-unknown-none` kernel + ELF loader
- Option C (Linux-based): aarch64 Linux kernel + RayOS modules
- Option D (Linux subsystem): RayOS is host; Linux is a managed guest runtime with Wayland-first graphics bridging
- Source: [PHASE2_PLAN.md](PHASE2_PLAN.md)
- Status: ✅ chosen (Option D is now the active focus: Linux runs as a managed subsystem/guest; Wayland-first UI). Audit note: Option A remains a valid bring-up path for firmware/kernel experiments, but Option D is the preferred compatibility + GUI direction.

---

## P1 — Linux Subsystem (Wayland-first, Linux is a guest)

21) Define the subsystem contract (RayOS host authority)
- Specify: lifecycle control, resource governance, filesystem authority (virtiofs), network policy, and a clear security boundary
- Explicitly: RayOS has full control over the Linux environment (keyboard/mouse, graphics presentation, terminal/serial, files, network, lifecycle). Linux is a managed guest runtime driven entirely by RayOS.
- Design + Contract: [LINUX_SUBSYSTEM_DESIGN.md](LINUX_SUBSYSTEM_DESIGN.md), [LINUX_SUBSYSTEM_CONTRACT.md](LINUX_SUBSYSTEM_CONTRACT.md)
- Status: ✅ done (design doc + interface contract are now the source-of-truth for Option D)

22) Bring up Linux VM with virtio devices (headless)
- KVM when available; deterministic fallback behavior when KVM is absent
- virtio-block/virtiofs + virtio-net + virtio-input baseline
- Status: ✅ done (headless Linux guest bring-up via QEMU is working and emits stable markers).
	- Verification (Step 2 baseline): `./scripts/test-linux-subsystem-headless.sh` (injects `RAYOS_LINUX_GUEST_READY` in initramfs shell mode)
	- Verification (Step 3 agent image): `./scripts/test-linux-subsystem-agent-headless.sh` (boots `rdinit=/rayos_init` from a gzip initramfs overlay and asserts `RAYOS_LINUX_AGENT_READY` + `RAYOS_LINUX_AGENT_PONG`)
	- Notes: default path uses pinned Alpine netboot artifacts (downloaded/cached under `build/linux-guest/`) to avoid host-kernel CPU-feature mismatches under TCG.

23) Wayland-first graphics bridge (embedded desktop surface)
- Linux desktop compositor produces a single embedded surface in RayOS as the first milestone
- Status: ⏳ in progress (embedded-surface prototype done; desktop compositor bring-up is in progress).
	- Implemented: guest agent `SURFACE_TEST` emits a deterministic single surface (PPM) between `RAYOS_LINUX_EMBED_SURFACE_BEGIN/END` markers.
	- Verification: `./scripts/test-linux-subsystem-embedded-surface-headless.sh` (extracts frame and asserts sha256).
	- Remaining for “Wayland-first”: replace this transport with real Wayland surface forwarding (e.g., virtio-gpu/virtio-wayland style path) while keeping the single-surface embed semantics.
	- Developer bring-up helper: `./scripts/run-linux-subsystem-desktop-weston.sh` (boots Alpine ISO with networking so you can `apk add weston ...` and run a compositor manually while we implement the real bridge).
	- More automatic bring-up helper: `./scripts/run-linux-subsystem-desktop-auto.sh` (netboot + initramfs overlay auto-DHCP + persistent rootfs provisioning on first boot + auto-start `seatd`+`weston`, then prints `RAYOS_LINUX_DESKTOP_READY` only when the Wayland socket exists).
		- Pre-provision headlessly: `./scripts/tools/linux_subsystem/build_desktop_rootfs_image.sh` (builds the persistent ext4 rootfs image once).

23a) Linux desktop “show desktop” path (QEMU window shows a working Weston session)
- Success criteria: a visible desktop (Weston + terminal) appears in the QEMU display window when triggered; readiness is only signaled when Wayland is actually up.
- Status: ⏳ in progress
	- ✅ Host QEMU display defaults fixed (portable `-vga virtio`; GL opt-in) and VGA console enabled (`console=tty0` alongside serial).
	- ✅ Guest readiness semantics tightened (declare READY only after `$XDG_RUNTIME_DIR/wayland-0` exists).
	- ✅ Persistent rootfs provisioning corruption fixed (previously produced 0-byte binaries like `/usr/bin/seatd`/`weston`; now detected and forces reprovision).
	- ✅ seatd bring-up stabilized (socket appears; `LIBSEAT_BACKEND=seatd` + `/run/seatd.sock`).
	- ✅ Desktop host-control bridge (v0): RayOS can ask host to `SHOW_LINUX_DESKTOP`, `LINUX_SENDTEXT:<text>`, `LINUX_SENDKEY:<spec>`, `LINUX_SHUTDOWN`.
	- Current RayOS commands (type into the RayOS prompt / chat input):
		- `show linux desktop` (or `show desktop`, `linux desktop`)
		- `type <text>` (types text + Enter into the Linux desktop VM)
		- `press <key>` / `key <key>` (sends a single key or combo; examples: `press esc`, `press tab`, `press enter`, `press ctrl+l`, `press alt+f4`, `press up`)
		- `shutdown linux` / `stop linux` (best-effort ACPI powerdown; falls back to forcing QEMU quit)
		- Notes: this is currently host-level QEMU input injection; it is not yet a guest agent “launch app / list windows / click element” API.
	- Next tasks (make it usable, not just demo-able):
		- Host ACK channel: host replies over serial with `RAYOS_HOST_ACK:<op>:<ok|err>:<detail>` so RayOS can display “sent / failed / desktop not running”.
		- Mouse injection (minimum): add `LINUX_MOUSE_ABS:x:y` + `LINUX_CLICK:left|right` host events; implement via QEMU monitor (`mouse_move` / `mouse_button`) or switch to QMP input events.
		- Focus & routing: ensure the Linux desktop VM can be focused/raised on request (host-side window focus is best-effort; still provide typing even when unfocused).
		- Graceful shutdown: prefer in-guest shutdown via agent/control-plane when available; keep ACPI powerdown fallback.
		- Hardening: debounce/queue host events so repeated commands don’t spam QEMU; add bounded timeouts per command.
		- Observability: write a single host log for desktop control actions (timestamped) alongside `linux-desktop-launch.log`.

23b) Desktop VM control plane (versioned, auditable)
- Goal: a minimal, versioned RayOS→host→guest command surface that’s deterministic and testable.
- Success criteria: RayOS can (1) show/hide desktop, (2) type, (3) press keys, (4) click, (5) confirm success via ACKs, (6) shut down cleanly.
- TODOs:
	- Define v0 wire format + version tag (e.g. `RAYOS_HOST_EVENT_V0:<op>:<payload>`).
	- Add host-side schema validation (reject oversized payloads; strict ASCII for now).
	- Add a single end-to-end smoke test: boot RayOS → show desktop → type `echo ok` → press `enter` → observe visible effect marker or serial echo → shutdown.
	- Add host-side “desktop running state” tracking to avoid acting on stale PID/sock.

23c) Policy alignment (contract enforcement)
- Goal: keep Option D authority model true even during bring-up.
- TODOs:
	- Default networking OFF for desktop; enable only on explicit policy or first-time provisioning (with explicit log marker).
	- Add a “network enabled” host marker so tests can assert policy behavior.
	- Separate “provisioning VM run” vs “normal desktop run” to avoid accidental always-on net.
	- Document the security boundary: what host exposes (virtio devices, storage, monitor socket) and what is denied.

23d) Persistent Linux VM lifecycle (living VM across RayOS reboots)
- Goal: Linux subsystem is a long-lived VM instance; RayOS reboot should resume/reattach rather than create a fresh environment.
- Minimum success criteria: stable VM identity + persistent disk(s) so the Linux environment persists across RayOS boots.
- Target success criteria: persist/restore guest execution state (RAM/device model) so the same session resumes after a RayOS reboot.
- TODOs:
	- Define a VM identity/registry record (name/id → disk paths → device config → policy).
	- Default boot behavior: start/resume the Linux VM during RayOS boot (background), but keep it hidden and non-interactive until explicitly presented.
	- Define “reboot semantics”: best-effort checkpoint/suspend (or guest hibernate) before reboot; restore on next boot.
	- Add host tooling support: PID/state files and explicit `RESUME_LINUX_DESKTOP` vs `START_LINUX_DESKTOP` actions.
	- Add a headless test: boot RayOS → assert Linux VM started/resumed but desktop NOT presented → present desktop → create a marker on disk → reboot RayOS → ensure marker persists and (if state restore exists) desktop resumes without re-provision.

24) Native-window mapping (Linux apps appear as RayOS windows)
- Map multiple guest Wayland surfaces into RayOS compositor as separate windows
- Focus: lifecycle, focus, input routing, DPI scaling, clipboard basics
	- Step 5 scaffolding: guest agent `SURFACE_MULTI_TEST` emits per-surface create + frame blocks over serial (`RAYOS_LINUX_SURFACE_CREATE`, `RAYOS_LINUX_SURFACE_FRAME_BEGIN/END`).
	- Step 5 scaffolding: guest agent `SURFACE_LIFECYCLE_TEST` emits configure + destroy events (`RAYOS_LINUX_SURFACE_CONFIGURE`, `RAYOS_LINUX_SURFACE_DESTROY`) so the host can model window geometry and lifecycle.
	- Step 5 scaffolding: guest agent `SURFACE_FOCUS_ROLE_TEST` emits role + focus events (`RAYOS_LINUX_SURFACE_ROLE`, `RAYOS_LINUX_SURFACE_FOCUS`) so the host can model focus and basic z-order.
	- Step 5 scaffolding: guest agent `SURFACE_TREE_TEST` emits parent + state events (`RAYOS_LINUX_SURFACE_PARENT`, `RAYOS_LINUX_SURFACE_STATE`) so the host can model popup-parent relationships and basic window states.
	- Verification: `./scripts/test-linux-subsystem-multi-surface-headless.sh` (extracts per-surface frames and asserts sha256).
	- Verification: `./scripts/test-linux-subsystem-surface-lifecycle-headless.sh` (asserts live `registry.json` window/surface mapping + geometry update + destroy).
	- Verification: `./scripts/test-linux-subsystem-surface-focus-role-headless.sh` (asserts `focused_window_id`, roles, and `z_order` in live `registry.json`).
	- Verification: `./scripts/test-linux-subsystem-surface-tree-headless.sh` (asserts parent/child tree + states in live `registry.json`).
- Status: ⏳ in progress (protocol + host-side extraction scaffolding done; real Wayland forwarding still pending)

24a) Real Wayland/graphics forwarding (replace PPM-over-serial)
- Goal: replace the test transport with an actual Wayland-first surface forwarding path while keeping single-surface embedding semantics.
- TODOs:
	- Decide transport: virtio-gpu scanout capture vs virtio-wayland style bridge; keep it minimal for milestone 1.
	- Implement “embedded desktop surface” as a real pixel buffer stream (not PPM text), with bounded bandwidth.
	- Keep deterministic readiness markers and add a stable “first frame presented” marker.
	- Add backpressure + frame dropping policy (host authoritative).

25) Subsystem command channel (Intent → Linux app launch)
- A controlled interface for launching apps and receiving window/surface metadata
- Status: 💤 deferred

26) Automated subsystem smoke tests
- Headless: start Linux guest, launch a Wayland client, assert a “surface created” marker, then shut down cleanly
- Status: 💤 deferred

---

## P1b — Windows 11 Subsystem (Windows is a guest)

Design notes + contract: [WINDOWS_SUBSYSTEM_DESIGN.md](WINDOWS_SUBSYSTEM_DESIGN.md), [WINDOWS_SUBSYSTEM_CONTRACT.md](WINDOWS_SUBSYSTEM_CONTRACT.md)

30) Persistent Windows VM lifecycle (living VM across RayOS reboots)
- Goal: manage the life of a Windows VM in perpetuity; RayOS reboot resumes/reattaches to an existing VM instance.
- Minimum success criteria: stable VM identity + persistent disk(s) so Windows is not “new” each RayOS boot.
- Target success criteria: persist/restore execution state (RAM/device model) so the same session resumes after a RayOS reboot.
- TODOs:
	- Define a Windows VM registry record (name/id → disk paths → OVMF vars → vTPM state → device model → policy).
	- Default boot behavior: start/resume the Windows VM during RayOS boot (background), but keep it hidden and non-interactive until explicitly presented.
	- Implement/choose a state mechanism: guest hibernate task, host-side save/restore, or VM snapshot + restore flow.
	- Add explicit “resume if present” behavior to the host launcher path; keep “fresh start” as an explicit user action.
	- Add a deterministic readiness/health marker strategy that works after resume (not just after cold boot).

31) “Show Windows desktop” v0 (host-launched QEMU window)
- Goal: the RayOS UI can trigger a host-launched Windows VM that visibly boots to a desktop in a separate QEMU window.
- Success criteria: typing `show windows desktop` in RayOS launches a Windows VM window; VM remains responsive; host can shut it down.
- Planned RayOS commands (once events are implemented):
	- `show windows desktop` (or `windows desktop`)
	- `windows type <text>` (types text + Enter into the Windows VM)
	- `windows press <key>` (sends a single key or combo; examples: `windows press esc`, `windows press tab`, `windows press ctrl+l`, `windows press alt+f4`)
	- `shutdown windows` / `stop windows` (best-effort ACPI powerdown; falls back to forcing QEMU quit)
- Milestones / steps:
	- Kernel event: add `RAYOS_HOST_EVENT:SHOW_WINDOWS_DESKTOP` emission for phrases like `show windows desktop` / `windows desktop`.
	- Host wiring: extend `test-boot.sh` to watch for `SHOW_WINDOWS_DESKTOP` and launch a Windows VM script (debounced, PID-file guarded).
	- Windows launcher script: add `./scripts/run-windows-subsystem-desktop.sh` that:
		- Uses UEFI (OVMF) and a Windows disk image (user-supplied path).
		- Creates an HMP monitor socket (for input injection + shutdown).
		- Adds robust input devices (`usb-kbd`, `usb-tablet`, plus virtio input if desired).
		- Runs with contract-safe defaults (network OFF unless explicitly enabled).
	- Preflight checks: detect missing prerequisites and fail with actionable errors:
		- `qemu-system-x86_64`
		- OVMF firmware path
		- `swtpm` (Windows 11 requires TPM → vTPM required)
		- user-provided Windows disk image path (and optional installer ISO path)

32) Windows control bridge v0 (sendtext/sendkey/shutdown)
- Goal: reuse the same host-event mechanism used for Linux to drive basic Windows UI interactions.
- Success criteria: RayOS can type text, press keys, and request shutdown of the Windows VM.
- Milestones / steps:
	- Add kernel events mirroring Linux:
		- `RAYOS_HOST_EVENT:WINDOWS_SENDTEXT:<text>` (RayOS command: `windows type <text>`)
		- `RAYOS_HOST_EVENT:WINDOWS_SENDKEY:<spec>` (RayOS command: `windows press <key>`)
		- `RAYOS_HOST_EVENT:WINDOWS_SHUTDOWN`
	- Host wiring in `test-boot.sh`:
		- Route `WINDOWS_SENDTEXT` via `scripts/qemu-sendtext.py` to the Windows monitor socket.
		- Route `WINDOWS_SENDKEY` via `scripts/qemu-sendkey.py` to the Windows monitor socket.
		- Implement `WINDOWS_SHUTDOWN` via `system_powerdown` (fallback `quit`).
	- Logging: write `build/windows-desktop-launch.log` and `build/windows-desktop-control.log`.

33) Windows readiness markers (deterministic automation hook)
- Goal: make “desktop ready” testable without manual observation.
- Success criteria: host can reliably determine when Windows is ready to accept input.
- Options (pick one to start):
	- QEMU Guest Agent (recommended): install QGA in the Windows guest; host polls guest state and emits `RAYOS_WINDOWS_READY`.
	- Screen-based readiness: simple OCR/template detection on the framebuffer (fallback, universal).
	- Guest-side marker: a startup task writes a marker to a serial channel (works but more brittle).

34) Policy contract (Windows)
- Goal: preserve the authority model: RayOS owns lifecycle, input, display, and policy.
- Milestones / steps:
	- Default networking OFF; explicit enable via host policy/env var.
	- Storage boundary: explicit disk image path(s) only; no accidental host mounts.
	- Resource caps: CPU/mem limits controllable from RayOS policy.
	- Snapshot strategy: define “pause/suspend” and snapshot hooks (RAM+device state), even if implementation is deferred.

35) GPU path (defer correctness, define interface now)
- Goal: define the long-term direction: Windows output is a RayOS surface (texture), not a direct display owner.
- Milestones / steps:
	- Start: virtual GPU with basic display for bring-up (no passthrough).
	- Next: define the virtual GPU contract needed for WDDM plausibility.
	- Later: RayOS-owned GPU scheduling and composited presentation.

2) Make the kernel actually run on the aarch64 VM
- If Option A: move/port enough of System 1 + System 2 init into the bootloader
- If Option B: introduce a dedicated aarch64 bare-metal kernel target and build artifacts
- Source: [PHASE1_COMPLETE.md](PHASE1_COMPLETE.md), [SESSION_SUMMARY.md](SESSION_SUMMARY.md)
- Status: ✅ done (Option A path: bootloader provides a post-`ExitBootServices` embedded runtime on aarch64, including UART I/O and a line-based REPL; validated by `test-boot-aarch64-headless.sh`. Option B path: `kernel-aarch64-bare` boots under AAVMF/QEMU and prints over PL011 UART; validated by `test-boot-aarch64-kernel-headless.sh`. Audit note: for production, add soak/long-run boot stability tests and define crash/recovery strategy.)

3) Implement kernel loading mechanism appropriate to the chosen path
- Option A: bootloader directly runs embedded kernel logic
- Option B: minimal ELF loader in the bootloader and jump to kernel entry
- Source: [PHASE2_PLAN.md](PHASE2_PLAN.md), [bootloader/README-uefi.md](bootloader/README-uefi.md)
- Status: ✅ done (Option A supports aarch64 embedded-mode when `kernel.bin` is missing/invalid; verified by `test-boot-aarch64-headless.sh`. Option B includes a minimal ELF PT_LOAD loader and jump to entry; verified by `test-boot-aarch64-kernel-headless.sh`. x86_64 kernel-bare boot continues to work via `test-boot-headless.sh`. Audit note: production follow-up would include signature verification / measured boot and stricter validation of ELF inputs.)

4) Post-`ExitBootServices` output path (so the kernel can print)
- Minimal serial OR framebuffer output after boot services are gone
- Source: [PHASE1_COMPLETE.md](PHASE1_COMPLETE.md), [BOOT_VERIFICATION.md](BOOT_VERIFICATION.md)
- Status: ✅ done (aarch64: bootloader has post-exit PL011 UART output + framebuffer loop when GOP exists; verified by `test-boot-aarch64-headless.sh`. Option B: `kernel-aarch64-bare` prints over PL011 UART post-exit and can round-trip the host AI bridge protocol (`RAYOS_INPUT`/`AI`/`AI_END`); verified by `test-boot-aarch64-kernel-headless.sh`. x86_64 headless markers validated by `test-boot-headless.sh`, and host bridge flows are exercised by `test-boot-ai-headless.sh` / `test-boot-ai.sh`.)

---

## P1 — System 1 (GPU Reflex Engine)

5) GPU initialization on the actual runtime target (aarch64 VM)
- Device discovery / adapter init
- Print GPU info to console
- Source: [PHASE2_PLAN.md](PHASE2_PLAN.md), [PHASE1_COMPLETE.md](PHASE1_COMPLETE.md)
- Status: ⏳ in progress (implemented **GOP/framebuffer probe + logging** on aarch64 and assert it in `./scripts/test-boot-aarch64-headless.sh` and `./scripts/test-boot-aarch64-kernel-headless.sh`. Remaining for “adapter init”: bring up a real compute-capable GPU backend (wgpu/Vulkan/Metal equivalent for the target) on aarch64 and print adapter/device properties, not just framebuffer mode/base/size. Note: the default QEMU `-device ramfb` provides GOP but is not a PCI display device, so PCI display count may be 0.)

6) Dispatch the megakernel (not just compile WGSL)
- Ensure `MegakernelExecutor` actually submits work to the GPU queue
- Source: [kernel/IMPLEMENTATION.md](kernel/IMPLEMENTATION.md)
- Status: ✅ done (std-kernel dispatches compute via `queue.submit(...)`. Remaining for production on aarch64: once (5) has a real compute adapter/device, validate end-to-end dispatch on aarch64 with a headless test that asserts a real GPU queue submission + completion marker.)

7) Watchdog/timeout strategy for “persistent” GPU workloads
- Avoid OS/driver watchdog killing long kernels; design a chunked or cooperative scheme
- Source: [kernel/IMPLEMENTATION.md](kernel/IMPLEMENTATION.md)
- Status: ✅ done (WGSL dispatch is bounded per call via `iteration_budget` and exits early when the queue empties; host re-dispatches in a loop. Audit note: production follow-up is a sustained-load soak test + explicit GPU timeout telemetry and backoff policy.)

8) RT Core integration (true hardware traversal)
- Replace/supplement simulated BVH traversal with RT hardware paths where available
- Source: [kernel/README_KERNEL.md](kernel/README_KERNEL.md), [kernel/IMPLEMENTATION.md](kernel/IMPLEMENTATION.md)
- Status: ✅ done (kernel: feature-gated Vulkan RT backend `rt-vulkan` provides a rayQuery + AS path and `LogicBVH::trace` can traverse the full logic tree via RT-core-backed branch evaluation when `RAYOS_RT_CORE=1` is set; safe fallback to software traversal when unsupported. Verification: `cd kernel && cargo run --bin rayos-rt-smoke --features rt-vulkan`. Remaining for production: run the smoke test with `RAYOS_RT_REQUIRED=1` on at least one Vulkan-RT-capable system/driver and record the expected pass criteria.)

---

## P1 — System 2 (LLM Cognitive Engine)

9) Replace keyword intent parsing with real inference (kernel-side)
- Integrate `candle` or an external inference runtime where appropriate
- Source: [kernel/IMPLEMENTATION.md](kernel/IMPLEMENTATION.md)
- Status: ✅ done (kernel-side `IntentParser` no longer uses `str::contains(...)` keyword intent parsing. Default path uses a small statistical classifier (tokenize → vectorize → score) and `candle-infer` optionally accelerates the same classifier with Candle tensor matmul. Verification: `cd kernel && cargo test` and `cd kernel && cargo test --features candle-infer`. Audit note: production follow-up is evaluation on a real dataset (precision/recall), adversarial inputs, and bounded-latency guarantees under load.)

10) Intent crate “LLM Mode” tasks
- Actual model integration
- Tokenization pipeline
- Embeddings generation
- Neural intent classification
- Entity extraction
- Source: [intent/PHASE5_SUMMARY.md](intent/PHASE5_SUMMARY.md)
- Status: ✅ done (intent crate: implemented a feature-gated LLM-mode pipeline in `intent/src/llm_connector.rs` including best-effort external tokenizer loading (`tokenizer.json`), deterministic hashed embeddings (feature-hashed bag-of-words), neural intent classification (optional Candle weight file `intent_classifier.json`), and entity extraction (quoted strings, filenames, paths, `k=v`, numbers). Verification: `cd intent && cargo test` and `cd intent && cargo test --features llm`. Audit note: production follow-up is shipping/standardizing real model artifacts + versioning/compat checks + prompt/security policy.)

---

## P1 — Conductor (Orchestration)

11) Wire System 1 + System 2 together with a real task queue
- Task submission → scheduling → completion → metrics
- Source: [PHASE2_PLAN.md](PHASE2_PLAN.md), [PHASE1_COMPLETE.md](PHASE1_COMPLETE.md)
- Status: ✅ complete (kernel-side: added a real `TaskQueue` with submission → priority scheduling → completion; System 1 now records completions + end-to-end latency for both CPU fallback and GPU megakernel path, and exposes completions via `RayKernel::drain_completions`; metrics are derived from real completion/latency rather than stubs. Audit note: production follow-up is backpressure + persistence across reboot + well-defined cancellation semantics.)

12) Entropy monitoring → Dream Mode trigger → Optimization loop
- Ensure metrics flow is connected to actual workload, not stubbed
- Source: [PHASE1_COMPLETE.md](PHASE1_COMPLETE.md)
- Status: ✅ complete (entropy is updated from real completion-derived latency; dream trigger uses real idleness + metrics; `enter_dream_mode()` runs a bounded optimization loop that applies real System 1 tuning changes. Audit note: production follow-up is reproducibility (seeded runs), safety constraints on tuning, and a way to disable/limit it in secure environments.)

13) Async “Send” constraints cleanup (if moving to multithreaded tokio)
- Current daemon uses `spawn_local` + `LocalSet`; if moving to `tokio::spawn`, replace async-shared `parking_lot::RwLock` with `tokio::sync` locks or restructure
- Source: [conductor/README_CLAUDE_RESULTS.md](conductor/README_CLAUDE_RESULTS.md)
- Status: ✅ complete (conductor now runs on a multi-thread tokio runtime and no longer uses `spawn_local`/`LocalSet`; orchestrator work-stealing queues were restructured to keep non-`Sync` crossbeam `Worker` thread-local and share only `Stealer` handles. Next step for production confidence: re-run `cd conductor && cargo test --features "daemon ai"` on the intended toolchain/CI image and record the expected pass output.)

---

## P1 — Volume (Storage / Semantic Memory)

14) Mount a filesystem from the ISO / volume backing store in the boot environment
- Read embeddings / index data during boot
- Source: [PHASE1_COMPLETE.md](PHASE1_COMPLETE.md), [PHASE2_PLAN.md](PHASE2_PLAN.md)
- Status: ✅ complete (bootloader uses the UEFI filesystem to stage `EFI\\RAYOS\\volume.bin`, `embeddings.bin`, and `index.bin` into memory before `ExitBootServices` and passes physical pointers/sizes via `BootInfo`; validated by `test-boot-aarch64-volume-headless.sh` + `test-boot-aarch64-kernel-volume-headless.sh`. Audit note: production follow-up is integrity checking (hash/signature) and memory-pressure handling for large volumes.)

15) Hook Volume (vector store + HNSW) into kernel queries
- RAG retrieval path (query → retrieve → feed System 2)
- Source: [volume/README.md](volume/README.md)
- Status: ✅ complete (kernel-bare now supports `:rag <text>` retrieval over staged `embeddings.bin` with optional `index.bin` neighbor-graph acceleration, and `:s2 <text>` prints retrieved context before parsing; validated by `test-boot-rag-headless.sh`. Audit note: production follow-up is persistence format versioning and retrieval quality benchmarks.)

---

## P2 — Cortex (Eyes / Sensory)

16) Communication protocol Cortex → kernel/System 2
- Define message schema + transport (shared memory / virtio / host bridge)
- Source: [cortex/README.md](cortex/README.md)
- Status: ✅ complete (defined a minimal line-based `CORTEX:<TYPE> k=v ...` schema and implemented a kernel-bare receiver over serial + a `:cortex <CORTEX:...>` injection command; host-side `rayos-cortex` can inject `CORTEX:` lines into a running guest via the QEMU monitor (sendkey); validated by `test-boot-cortex-headless.sh` and `test-boot-cortex-daemon-headless.sh`. Audit note: production follow-up is authentication/anti-spoofing for inbound events and schema versioning.)

17) Real object recognition + hardware eye tracking integration
- Tobii integration
- YOLO/MobileNet integration
- Source: [cortex/README.md](cortex/README.md)
- Status: ✅ complete (hardware gaze input supported via UDP bridge `RAYOS_GAZE_UDP_ADDR`; object recognition supports optional OpenCV DNN MobileNet-SSD Caffe models via `RAYOS_DNN_SSD_*` env vars and falls back safely when not configured. Next step for production confidence: test on real Tobii hardware + a known-good DNN model bundle and document expected calibration/latency and failure modes.)

---

## Code-Level TODOs (directly in repo)

18) Bare-metal entry stubs: interrupts/GDT/IDT bring-up
- File: [kernel/src/bare_metal_entry.rs](kernel/src/bare_metal_entry.rs)
- Notes: placeholders existed for IDT/GDT during bring-up
- Status: ✅ addressed for Phase 1 bring-up (explicitly keeps interrupts disabled). Audit note: for production, this is not sufficient—needs real exception/interrupt handling, proper IDT/GDT, and fault isolation.

19) Bare-metal decimal rendering must avoid division/modulo intrinsics
- File: [kernel/src/bare_metal_entry.rs](kernel/src/bare_metal_entry.rs)
- Notes: division can fault before IDT/handlers exist
- Status: ✅ addressed (division-free implementation). Audit note: production follow-up is broader early-boot safety checks (no unexpected panics, bounded logging, and verified exception handlers once interrupts are enabled).

20) Re-enable GPU detection in x86_64 `kernel-bare` once a real GPU init exists
- File: [kernel-bare/src/main.rs](kernel-bare/src/main.rs)
- Status: ⏳ in progress (PCI display-controller probe re-enabled, but the "once a real GPU init exists" prerequisite is still not met for `kernel-bare`. Next step: introduce a real GPU init path (or a well-defined stub that can succeed/fail deterministically) and then gate/validate the detection output against it.)

---

## Suggested execution order (minimal critical path)

- Option D focus (Linux subsystem):
	- Do Linux Subsystem (21→26) to get a Wayland GUI subsystem and app compatibility under RayOS control
	- Then (optionally) wire Intent → Linux app launch (25) into the broader orchestration path
	- Keep P0 (2→4) and the existing headless boot tests as regression coverage for bootloader/kernel bring-up

- Option A/B focus (bare-metal paths):
	- Do P0 (1→4) to get a real aarch64 kernel loop + output
	- Then P1 (5→6) to prove GPU init + dispatch
	- Then P1 System 2 (9) for real inference
	- Then P1 conductor + volume (11→15)
	- Then Cortex (16→17)
