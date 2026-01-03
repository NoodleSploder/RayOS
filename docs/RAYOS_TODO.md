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
- Status: ✅ done (v0 architecture doc filled; includes dev-harness mapping)

41) Disk layout & persistence spec
- Doc: [DISK_LAYOUT_AND_PERSISTENCE.md](DISK_LAYOUT_AND_PERSISTENCE.md)
- Goal: partitions/filesystems, VM registry location, VM disk/state locations, log locations, invariants.
- Status: ✅ done (concrete v0 layout)

42) Installer + boot manager spec
- Doc: [INSTALLER_AND_BOOT_MANAGER_SPEC.md](INSTALLER_AND_BOOT_MANAGER_SPEC.md)
- Goal: USB boot installer wizard, partition selection/creation, boot entries, recovery entry, boot manager decision.
- Status: ✅ done (v0 spec filled)

43) Policy configuration schema
- Doc: [POLICY_CONFIGURATION_SCHEMA.md](POLICY_CONFIGURATION_SCHEMA.md)
- Goal: concrete policy format and controls (VM autoboot hidden, present gating, networking defaults, device exposure, resource caps).
- Status: ✅ done (concrete v0 schema)

44) Update + recovery strategy
- Doc: [UPDATE_AND_RECOVERY_STRATEGY.md](UPDATE_AND_RECOVERY_STRATEGY.md)
- Goal: update mechanism, rollback, recovery mode, compatibility/versioning rules.
- Status: ✅ done (v0 strategy filled)

45) Security & threat model
- Doc: [SECURITY_THREAT_MODEL.md](SECURITY_THREAT_MODEL.md)
- Goal: trust boundaries, invariants, secure/measured boot posture, auditing, key management.
- Status: ✅ done (v0 threat model filled)

46) Observability & crash recovery
- Doc: [OBSERVABILITY_AND_RECOVERY.md](OBSERVABILITY_AND_RECOVERY.md)
- Goal: persistent logs, health/readiness markers, crash artifacts, and recovery UX that does not require another machine.
- Status: ✅ done (v0 observability + recovery)

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
- Status: ✅ done (embedded-surface prototype done; desktop compositor bring-up is in progress).
	- Implemented: guest agent `SURFACE_TEST` emits a deterministic single surface (PPM) between `RAYOS_LINUX_EMBED_SURFACE_BEGIN/END` markers.
	- Verification: `./scripts/test-linux-subsystem-embedded-surface-headless.sh` (extracts frame and asserts sha256).
	- Remaining for “Wayland-first”: replace this transport with real Wayland surface forwarding (e.g., virtio-gpu/virtio-wayland style path) while keeping the single-surface embed semantics.
	- Developer bring-up helper: `./scripts/run-linux-subsystem-desktop-weston.sh` (boots Alpine ISO with networking so you can `apk add weston ...` and run a compositor manually while we implement the real bridge).
	- More automatic bring-up helper: `./scripts/run-linux-subsystem-desktop-auto.sh` (netboot + initramfs overlay auto-DHCP + persistent rootfs provisioning on first boot + auto-start `seatd`+`weston`, then prints `RAYOS_LINUX_DESKTOP_READY` only when the Wayland socket exists).
		- Pre-provision headlessly: `./scripts/tools/linux_subsystem/build_desktop_rootfs_image.sh` (builds the persistent ext4 rootfs image once).
	- ✅ Pointer/input bridge: RayOS prompt now accepts `mouse <x> <y>` (normalized 0..1) and `click left|right`, emitting `LINUX_MOUSE_ABS`/`LINUX_CLICK` host events; host injects via QEMU monitor and appends ACK markers.
	- ✅ Host ACK markers: `scripts/test-boot.sh` now writes `RAYOS_HOST_ACK:<op>:<ok|err>:<detail>` to `serial-boot-graphical.log` for desktop launch/control/mouse events (guest/UI consumption still pending).

23a) Linux desktop “show desktop” path (QEMU window shows a working Weston session)
- Success criteria: a visible desktop (Weston + terminal) appears in the QEMU display window when triggered; readiness is only signaled when Wayland is actually up.
	- **Important:** this path is currently **host-bridge driven**. If you boot RayOS with `./scripts/test-boot.sh` using defaults (`ENABLE_HOST_DESKTOP_BRIDGE=0`), the kernel will still emit the event marker, but **nothing will happen** because no host bridge is watching.
	- **Current dev-harness behavior:**
		- `show linux desktop` triggers a host action only when `ENABLE_HOST_DESKTOP_BRIDGE=1`.
		- If the Linux desktop was prelaunched hidden (`PRELAUNCH_HIDDEN_DESKTOPS=1`), `show linux desktop` opens a **VNC viewer** (currently `gvncviewer`) to the already-running hidden VM.
		- If not prelaunched, `show linux desktop` launches the Linux desktop VM on demand.
	- Status: ✅ done (on-demand launch + basic show/hide wiring exist in the dev harness)
	- ✅ Host QEMU display defaults fixed (portable `-vga virtio`; GL opt-in) and VGA console enabled (`console=tty0` alongside serial).
	- ✅ Guest readiness semantics tightened (declare READY only after `$XDG_RUNTIME_DIR/wayland-0` exists).
	- ✅ Persistent rootfs provisioning corruption fixed (previously produced 0-byte binaries like `/usr/bin/seatd`/`weston`; now detected and forces reprovision).
	- ✅ seatd bring-up stabilized (socket appears; `LIBSEAT_BACKEND=seatd` + `/run/seatd.sock`).
	- ✅ Desktop host-control bridge (v0): RayOS can ask host to `SHOW_LINUX_DESKTOP`, `LINUX_SENDTEXT:<text>`, `LINUX_SENDKEY:<spec>`, `LINUX_SHUTDOWN`.
	- Current RayOS commands (type into the RayOS prompt / chat input):
		- `show linux desktop` (or `show desktop`, `linux desktop`)
		- `hide linux desktop` (currently stops the desktop VM; future: hide without stopping)
		- `type <text>` (types text + Enter into the Linux desktop VM)
		- `press <key>` / `key <key>` (sends a single key or combo; examples: `press esc`, `press tab`, `press enter`, `press ctrl+l`, `press alt+f4`, `press up`)
		- `mouse <x> <y>` (normalized 0..1 absolute pointer injection)
		- `click left|right` (button injection)
		- `shutdown linux` / `stop linux` (best-effort ACPI powerdown; falls back to forcing QEMU quit)
		- Notes: this is currently host-level QEMU input injection; it is not yet a guest agent “launch app / list windows / click element” API.
	- Next tasks (make it usable, not just demo-able):
		- ✅ Host ACK channel to the guest UI: host injects `@ack <op> <ok|err> <detail>` into RayOS; `kernel-bare` displays it as a SYS message (disable with `INJECT_ACK_TO_GUEST=0` in `scripts/test-boot.sh`).
		- ✅ Focus & routing: ensure the Linux desktop VM can be focused/raised on request (host-side window focus is best-effort; still provide typing even when unfocused).
		- ✅ Graceful shutdown: prefer in-guest shutdown via agent/control-plane when available; keep ACPI powerdown fallback.
		- ✅ Hardening: debounce/queue host events so repeated commands don’t spam QEMU; add bounded timeouts per command.
		- ✅ Observability: write a single host log for desktop control actions (`build/linux-desktop-control.log`) alongside `linux-desktop-launch.log`.
		- ⏳ Make `show linux desktop` work out-of-the-box:
			- Document required host dependencies (VNC viewer) and provide an alternative (e.g., `remote-viewer`/`vncviewer`) when `gvncviewer` is missing.
			- Decide whether `./scripts/test-boot.sh` should default `ENABLE_HOST_DESKTOP_BRIDGE=1` for interactive dev.

23b) Desktop VM control plane (versioned, auditable) - ✅ done
- Goal: a minimal, versioned RayOS→host→guest command surface that’s deterministic and testable.
- Success criteria: RayOS can (1) show/hide desktop, (2) type, (3) press keys, (4) click, (5) confirm success via ACKs, (6) shut down cleanly.
- TODOs:
	- ✅ Define v0 wire format + version tag (`RAYOS_HOST_EVENT_V0:<op>:<payload>`, backward-compatible parsing).
	- ✅ Add host-side schema validation (reject oversized payloads; strict ASCII for now).
	- ✅ Add a single end-to-end smoke test: `./scripts/test-desktop-control-e2e-headless.sh` (boot RayOS headless → show desktop → type/press via host bridge → shutdown; asserts `RAYOS_HOST_ACK:*` markers).
	- ✅ Add a show→hide→show smoke test: `./scripts/test-desktop-show-hide-show-headless.sh`.
	- ✅ Add host-side “desktop running state” tracking to avoid acting on stale PID/sock (baseline PID/monitor gating + ACKs now exist in `scripts/test-boot.sh`; still need versioned state + reattach semantics).

23c) Policy alignment (contract enforcement)
- Goal: keep Option D authority model true even during bring-up.
- TODOs:
	- ✅ Default networking OFF for desktop; auto-enable only for first-time provisioning when not explicitly configured.
	- ✅ Add a “network enabled” host marker so tests can assert policy behavior (`RAYOS_HOST_MARKER:LINUX_DESKTOP_NETWORK:<on|off>:<reason>` in the RayOS serial log).
	- ✅ Separate “provisioning VM run” vs “normal desktop run” (v0 pragmatic: provisioning inferred from a disk-ready marker; future: explicit provisioning mode + policy profile).
	- ✅ Document the security boundary: what host exposes (virtio devices, storage) and what is denied (host monitor sockets, passthrough by default): `docs/LINUX_SUBSYSTEM_CONTRACT.md`.
	- Verification: `./scripts/test-linux-desktop-network-marker-headless.sh` (deterministic marker emission without launching QEMU).

23d) Persistent Linux VM lifecycle (living VM across RayOS reboots) - ⏳ in progress
- Goal: Linux subsystem is a long-lived VM instance; RayOS reboot should resume/reattach rather than create a fresh environment.
- Minimum success criteria: stable VM identity + persistent disk(s) so the Linux environment persists across RayOS boots.
- Target success criteria: persist/restore guest execution state (RAM/device model) so the same session resumes after a RayOS reboot.
- TODOs:
	- ✅ Define a VM identity/registry record (name/id → disk paths → device config → policy). See `docs/VM_REGISTRY_SPEC.md`.
	- ✅ Create a default `registry.json` during the build/boot process.
	- ⏳ Default boot behavior (dev harness): start/resume the Linux VM during RayOS boot (background), keep it hidden, and make `show linux desktop` only **present** the already-running instance.
		- Today: this is available as an opt-in behavior in `./scripts/test-boot.sh` via `PRELAUNCH_HIDDEN_DESKTOPS=1` (plus `ENABLE_HOST_DESKTOP_BRIDGE=1`).
		- Goal: make this the default interactive developer experience (or provide a dedicated documented entrypoint).
		- Acceptance: after RayOS boots, Linux is already running (hidden) and `show linux desktop` reliably presents it without spawning a new VM.
		- Known current limitation: the “hidden” VM is presented via **VNC**; presenting requires a VNC client (currently `gvncviewer`).

	23e) RayOS-native desktop presentation (remove VNC dependency) - ⛔ blocked
	- Goal: `show linux desktop` presents Linux **inside RayOS** as a RayOS-owned surface/window.
	- Product constraint: do not rely on a host OS VNC client; RayOS is the hypervisor.
	- Prerequisites (blockers):
		- A real in-OS VM supervisor (or hypervisor layer) that can run the Linux VM on hardware (VMX/SVM + EPT/NPT) and expose virtio devices.
		- ✅ RayOS compositor/presentation can ingest guest scanout buffers (kernel-bare `guest_surface` publish/snapshot + native blit path exist; needs a real producer).

	**Hypervisor track: fastest path to “Linux Subsystem under RayOS VMM” (execution checklist)**

	This is the shortest, most testable sequence to replace the host-QEMU desktop bridge with an in-OS supervisor.
	Treat these as *hard gates* that unlock each next stage.

	P0 (Must-have): run *any* guest code under RayOS VMM
	- [✅] VMX/SVM bring-up is robust and repeatable (VM-entry succeeds; deterministic VM-exits; clean VMXOFF/teardown on failure).
	- [✅] EPT/NPT mapping is sufficient for a minimal guest RAM region (read/write; no panics; clear `EPT_VIOLATION` diagnostics).
	- [✅] Minimal interrupt/exit handling path exists for timer/interrupts needed by a real guest (even if initial guest is single-core + polling).
		- Note: Added VM-entry interrupt injection (writes VMCS `VM_ENTRY_INTERRUPTION_INFO`) when virtio-MMIO sets VRING interrupt status so guests observe IRQs (vector 0x20).
	- [✅] Add/keep a headless smoke test that proves: boot → enter guest → exit loop deterministically (existing `test-vmm-hypervisor-boot.sh` is the baseline; it now also validates virtio-gpu selftest + IRQ injection markers).
	- [✅] IRQ delivery: VM-entry injection implemented; **LAPIC fallback path** added and exercised via a forced-inject smoke run (`vmm_inject_force_fail` feature). MSI fallback implemented and exercised via forced-MSI run.
	- [✅] Bounded retry attempts added for pending interrupt injection (MAX=5). Exponential backoff implemented and a boot-time backoff selftest is available (`vmm_inject_backoff_selftest`).
	- [note] Unit-style tests were attempted; a host-runnable unit test exists (`crates/kernel-bare/tests/backoff_unit.rs`) but running `cargo test` for `kernel-bare` on the host was blocked by a Cargo lockfile / toolchain mismatch. To unblock testing on stable toolchains we added a dedicated host crate `crates/vmm-backoff-test` and added CI to run it (`.github/workflows/ci.yml`). Converting the in-kernel selftest to reuse the host crate's logic (or to make the kernel-side tests run under stable `cargo test`) remains a follow-up task.

	P1 (Must-have): boot Linux headless under RayOS VMM — status: **in progress**
	- [x] Virtqueue transport plumbing (descriptor chain logging, avail/used handling, queue notify handling) — basic plumbing exercised by deterministic guest driver blob.
	- [x] Virtio-blk minimal in-memory backing (READ/WRITE/GET_ID) — works with the deterministic guest blob; **persistent image backing remains TODO**.
	- [~] Virtio-console or minimal serial transport for guest logs/markers — **basic queue parsing implemented**: data/control queues handled, host logging emitted (markers: `RAYOS_VMM:VIRTIO_CONSOLE:COMPILED` / `ENABLED` / `CHAIN_HANDLED` / `RECV`, control markers `CTRLQ_HANDLED` present). Added a boot-time selftest feature (`vmm_virtio_console_selftest`) that exercises the handler during boot and emits `RAYOS_VMM:VIRTIO_CONSOLE:SELFTEST:INVOKE`. Remaining: richer control semantics, guest-driven end-to-end tests, and protocol conformance verification.
	- [ ] Linux boots to a deterministic “guest ready” marker under RayOS (analogue of `RAYOS_LINUX_GUEST_READY`) — planned after persistent disk + console visibility.
	- [ ] Add a headless test: boot RayOS → boot Linux headless under VMM → assert guest-ready marker → shutdown.

	**Notes / Implementation highlights:**
	- IRQ delivery robustness: VM-entry injection implemented using `VMCS_ENTRY_INTERRUPTION_INFO` with LAPIC and MSI fallbacks (forced-fail smoke features exercise each path).
	- Observability: deterministic markers added for lifecycle and device behavior (e.g., `RAYOS_VMM:VMX:*`, `RAYOS_VMM:VIRTIO_MMIO:*`, `RAYOS_LINUX_DESKTOP_PRESENTED`, `RAYOS_LINUX_DESKTOP_FIRST_FRAME`).
	- Testing: smoke script `scripts/test-vmm-hypervisor-boot.sh` now exercises VMX bring-up, virtio-gpu selftest, IRQ fallback modes, and the backoff selftest.
	- TODO: convert boot selftest to host unit tests (unblock by resolving Cargo lockfile/toolchain mismatch or extracting logic to a host-test crate).
	P2 (Must-have for desktop): single-surface scanout into RayOS compositor
	- [ ] Wire virtio-gpu device model to the *real* virtqueue transport (controlq + cursorq if needed; start with scanout-only).
	- [ ] Choose scanout backing (start CPU-visible shared backing: guest writes, RayOS blits).
	- [ ] Produce a `GuestSurface` from the virtio-gpu scanout and present it as a single RayOS-owned surface/window.
	- [ ] Add deterministic markers:
		- `RAYOS_LINUX_DESKTOP_PRESENTED` when the surface is visible
		- `RAYOS_LINUX_DESKTOP_FIRST_FRAME` when the first frame arrives
	- [ ] Add a headless test that validates the in-OS presentation path (no host VNC): boot → present → first-frame marker.

	P3 (Must-have for usability): input routing + lifecycle
	- [ ] Virtio-input device model + event injection from RayOS pointer/keyboard to the presented guest surface.
	- [ ] Present/Hide semantics do not kill the VM by default (presentation is UI-only; lifecycle is policy-controlled).
	- [ ] Reboot persistence test: boot → (Linux running hidden) → present → write marker on disk → reboot RayOS → marker persists.
	- TODOs (milestone 1: single full-desktop surface):
		- ⏳ Implement hypervisor runtime skeleton (VMX/SVM detection + enable + VMXON/VMCS + minimal VM-exit loop stub).
			- Milestone reached: VM-entry succeeds; deterministic VM-exits for `HLT` and port I/O; guest debug output via `out 0xE9` is trapped and printed (`RAYOS_GUEST_E9:<byte>`). See `scripts/test-vmm-hypervisor-boot.sh` markers `RAYOS_VMM:VMX:VMEXIT` + `RAYOS_GUEST_E9:`.
			- ✅ Handle `EPT_VIOLATION` as a first-class dispatch path so we can emulate MMIO/PCI BARs for virtio devices (MMIO counter region + register/decoder/handler chain exercises the new path in `scripts/test-vmm-hypervisor-boot.sh`).
		- ⏳ Implement virtqueue transport plumbing (virtio-pci modern or legacy) for in-OS virtio devices.
			- ✅ Exercise a minimal virtio-MMIO queue by logging descriptor/driver/used addresses, queue-notify events, the first avail/used entries, payload bytes from each descriptor chain, and by tracing descriptor chains into a used-ring completion while implementing a virtio-blk-like request path (READ fills pattern into writable data descriptors, WRITE logs buffers, GET_ID fills identity, and status byte is written; supports multiple data descriptors).
			- ✅ Virtio-blk now has a tiny in-memory disk backing: `VIRTIO_BLK_T_OUT` writes persist and subsequent `VIRTIO_BLK_T_IN` reads return the stored sector bytes (default disk initialized to the old `0xA5` pattern).
			- ✅ Virtio-MMIO interrupt registers are modeled (`InterruptStatus`/`InterruptAck`) and the VMM sets the VRING bit when publishing used-ring entries (see `INT_STATUS_SET` markers).
			- ✅ Virtio device dispatch scaffolding: MMIO state now tracks `device_id` and the descriptor-chain handler dispatches to either `handle_virtio_blk_chain` or `handle_virtio_net_chain` based on the active device. Enables future multi-device scenarios without recompiling.
			- ✅ Deterministic guest virtqueue setup now uses a **prebuilt guest driver blob** copied into guest RAM (instead of runtime instruction emission):
				- Blob: `crates/kernel-bare/src/guest_driver_template.bin` (loaded + patched by the VMM).
				- Generator: `scripts/generate_guest_driver.rs` (regenerates the blob deterministically; supports env vars like `RAYOS_GUEST_REQ0_TYPE`, `RAYOS_GUEST_REQ1_TYPE`, `RAYOS_GUEST_REQ0_SECTOR`, `RAYOS_GUEST_REQ1_SECTOR`).
				- Default scenario submits **two requests** in one notify: READ (desc chain starting at 0) + GET_ID (desc chain starting at 3), with completion verified via `STATUS_VERIFY`, `USED_ENTRY_READBACK`, and `USED_IDX_READBACK` markers in `scripts/test-vmm-hypervisor-boot.sh`.
			- ✅ Implemented basic virtio-net device model (TX+RX queues):
				- ✅ Scaffolding in place: TX queue logs packet lengths + Ethernet types; RX queue marks buffers ready.
				- ✅ MAC address assignment: device config space (offset 0x100+) exposes MAC address for virtio-net devices (fixed RAYOS MAC).
				- ✅ Multi-device dispatch: MMIO state tracks `device_id`; descriptor-chain handler dispatches to device-specific handlers based on configured device type.
				- ✅ Packet loopback infrastructure: TX handler gathers packet bytes, swaps src/dst MACs, and injects into RX descriptors.
				- ✅ Feature flags: `vmm_hypervisor_net_test` selects virtio-net device at startup (for testing); default remains virtio-blk.
				- ✅ Guest driver generator: `RAYOS_GUEST_NET_ENABLED=1` emits deterministic guest blob; descriptor layout and avail/used writes corrected.
				- ✅ End-to-end verified: headless smoke test `scripts/test-vmm-hypervisor-net.sh` now checks for `G:NET_RX` so guest RX completion is asserted.
				- Next steps:
					- Minor log cleanups and audit of remaining debug traces (done recently; additional reductions possible).
					- Add a deterministic guest-driven echo test or user-space test harness to broaden coverage.
					- Push branch and open a PR with changelog and serialized test evidence.
			- ✅ Implement guest memory mapping (GPA→HPA/EPT) + safe host accessors for device models (see [crates/kernel-bare/src/hypervisor.rs](crates/kernel-bare/src/hypervisor.rs#L1528-L1615)).
		- ✅ Implement scanout publication contract in the kernel (kernel-bare `GuestSurface` + `frame_seq` + Presented/Hidden gating).
		- ✅ Provide a synthetic scanout producer for end-to-end validation (`dev_scanout`).
		- ⏳ Implement virtio-gpu device model (scanout-focused) in the RayOS VM supervisor.
			- Status: ✅ protocol/model scaffolding exists in `kernel-bare` (feature `vmm_virtio_gpu`), but it is not yet wired to a real VMM/virtqueue transport.
			- Implemented subset: `GET_DISPLAY_INFO`, `RESOURCE_CREATE_2D`, single-entry `ATTACH_BACKING`, `SET_SCANOUT`, and `TRANSFER_TO_HOST_2D`/`RESOURCE_FLUSH` as "frame ready" signals.
		- Choose the host-side scanout buffer mechanism:
			- simplest: CPU-visible shared backing (guest writes, RayOS reads/blits)
			- later: GPU-resident / zero-copy where possible
		- ✅ Add a RayOS “GuestSurface” abstraction in the compositor: a surface backed by a VM scanout buffer.
		- Add input routing: when the Linux surface is presented, keyboard/mouse events are injected via a virtio-input device owned by RayOS.
		- Replace the dev-harness “VNC viewer” path with a RayOS-native presentation path in the installed architecture.
		- Add deterministic markers:
			- `RAYOS_LINUX_DESKTOP_PRESENTED` when the surface is visible
			- `RAYOS_LINUX_DESKTOP_FIRST_FRAME` when first frame arrives
		- Add a headless smoke test that validates the *in-OS* presentation path (no host VNC) once the supervisor exists.
	- ✅ Define “reboot semantics”: best-effort checkpoint/suspend (or guest hibernate) before reboot; restore on next boot. See `docs/REBOOT_SEMANTICS_SPEC.md`.
	- ✅ Add host tooling support: PID/state files and explicit `RESUME_LINUX_DESKTOP` vs `START_LINUX_DESKTOP` actions.
	- ⏳ Add a headless test: boot RayOS → assert Linux VM started/resumed but desktop NOT presented → present desktop → create a marker on disk → reboot RayOS → ensure marker persists and (if state restore exists) desktop resumes without re-provision.

	**Remaining TODO checklist for “Linux hidden-at-boot, show-only-presents”**
	- ⏳ Make the dev harness default match the contract:
		- Either (A) set `PRELAUNCH_HIDDEN_DESKTOPS=1` (and `ENABLE_HOST_DESKTOP_BRIDGE=1`) by default in `./scripts/test-boot.sh`, or (B) add a documented wrapper command (keep `test-boot.sh` minimal if desired).
	- ⏳ Make presentation not depend on one specific viewer:
		- If `gvncviewer` is absent, fall back to `remote-viewer` or print an explicit instruction marker telling the developer what command to run.
	- ✅ Emit a deterministic `RAYOS_HOST_MARKER:LINUX_DESKTOP_HIDDEN` for `running` once the VNC endpoint is ready and `stopped` when the hidden VM is torn down; the hidden launcher now routes these markers into the main serial log via `RAYOS_SERIAL_LOG`.
	- ✅ Emit `RAYOS_HOST_MARKER:LINUX_DESKTOP_PRESENTED:ok:<detail>` whenever the host actually opens (or already has) the Linux desktop viewer, giving automation a deterministic presentation signal alongside the existing ACK.
	- ⏳ Ensure `show linux desktop` never spawns duplicates:
		- If hidden VM exists, present it.
		- If not, launch once and record state, then present.
		- Add a regression test around this.

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
- Status: ✅ done (protocol + host-side extraction scaffolding done and smoke tests runnable; real Wayland forwarding still pending)

24a) Real Wayland/graphics forwarding (replace PPM-over-serial) - ✅ done
- Goal: replace the test transport with an actual Wayland-first surface forwarding path while keeping single-surface embedding semantics.
- TODOs:
	- ✅ Decide transport: virtio-gpu scanout capture vs virtio-wayland style bridge; keep it minimal for milestone 1. See `docs/WAYLAND_FORWARDING_SPEC.md`.
	- ✅ Implement `virtio-gpu` scanout capture on the host.
	- ✅ Implement “embedded desktop surface” as a real pixel buffer stream (not PPM text), with bounded bandwidth.
	- ✅ Keep deterministic readiness markers and add a stable “first frame presented” marker.
	- ✅ Add backpressure + frame dropping policy (host authoritative).

25) Subsystem command channel (Intent → Linux app launch)
- A controlled interface for launching apps and receiving window/surface metadata
- Status: ✅ done (RayOS emits `RAYOS_HOST_EVENT_V0:LINUX_LAUNCH_APP:<app>`; host bridge injects into the desktop VM and emits `RAYOS_HOST_ACK:LINUX_LAUNCH_APP:*`. The Linux desktop init path now hands off to a minimal command channel (`LAUNCH_APP:<name>`, `SHUTDOWN`) via the guest agent.)

26) Automated subsystem smoke tests
- Headless: start Linux guest, launch a Wayland client, assert a “surface created” marker, then shut down cleanly
- Status: ✅ done (desktop auto headless test launches `weston-terminal` via `LAUNCH_APP` and asserts launch + shutdown markers; additional surface/window protocol smoke tests already exist for deterministic surface markers.)

### RayOS-native GUI (app container with embedded surfaces)
- Goal: deliver an integrated RayOS windowing surface that can host UI-heavy apps (e.g., a VNC client) without depending on host windows.
- Status: ⏳ in progress
	- ✅ Surface plumbing: `guest_surface` + compositor scaffolding already publishes RayOS-managed surfaces and receives deterministic `SURFACE_*` markers.
	- ⏳ Window manager layer: build a RayOS compositor view that can embed multiple `GuestSurface`s as resizable/focusable windows; tie z-order, titlebars, and close/shade controls to host-state.
	- ⏳ Input routing: route RayOS pointer/keyboard events into the selected window surface and expose a host command channel (e.g., parser for `mouse`, `click`, `type`, etc.) so apps can be manipulated from the RayOS prompt.
	- ⏳ RayApp abstraction: define `RayApp` surfaces (with lifecycle hooks) and ship a minimal UI framework so we can embed a VNC client as a RayOS app; the client renders into its `GuestSurface` and accepts host-provided input events.
	- ⏳ App launcher: add RayOS commands (such as `run vnc <address>`) that instantiate RayApps, manage their surfaces, and emit `RAYOS_GUI_APP_READY:<name>` once the UI is ready.
	- ⏳ Tests/observability: add headless smoke tests that start the VNC RayApp, assert `SURFACE_FRAME` + `RAYOS_GUI_APP_READY` markers, and verify input-handshake ACKs.
	- Next steps:
		- Flesh out `RayApp` service/registry + policy schema (window metadata, focus, z-order).
		- Build the VNC client RayApp and wire it to `guest_surface` subscriptions.
		- Document UI flow so host automation can launch/close/resume RayApps deterministically.

---

## P1b — Windows 11 Subsystem (Windows is a guest)

Design notes + contract: [WINDOWS_SUBSYSTEM_DESIGN.md](WINDOWS_SUBSYSTEM_DESIGN.md), [WINDOWS_SUBSYSTEM_CONTRACT.md](WINDOWS_SUBSYSTEM_CONTRACT.md)

30) Persistent Windows VM lifecycle (living VM across RayOS reboots) - ✅ done
- Goal: manage the life of a Windows VM in perpetuity; RayOS reboot resumes/reattaches to an existing VM instance.
- Minimum success criteria: stable VM identity + persistent disk(s) so Windows is not “new” each RayOS boot.
- Target success criteria: persist/restore execution state (RAM/device model) so the same session resumes after a RayOS reboot.
- TODOs:
	- ✅ Define a Windows VM registry record (name/id → disk paths → OVMF vars → vTPM state → device model → policy).
	- ✅ Default boot behavior: start/resume the Windows VM during RayOS boot (background), but keep it hidden and non-interactive until explicitly presented.
	- ✅ Implement/choose a state mechanism: guest hibernate task, host-side save/restore, or VM snapshot + restore flow.
	- ✅ Add explicit “resume if present” behavior to the host launcher path; keep “fresh start” as an explicit user action.
	- ✅ Add a deterministic readiness/health marker strategy that works after resume (not just after cold boot).

31) “Show Windows desktop” v0 (host-launched QEMU window)
- Goal: the RayOS UI can trigger a host-launched Windows VM that visibly boots to a desktop in a separate QEMU window.
- Success criteria: typing `show windows desktop` in RayOS launches a Windows VM window; VM remains responsive; host can shut it down.
- Planned RayOS commands (once events are implemented):
	- `show windows desktop` (or `windows desktop`)
	- `windows type <text>` (types text + Enter into the Windows VM)
	- `windows press <key>` (sends a single key or combo; examples: `windows press esc`, `windows press tab`, `windows press ctrl+l`, `windows press alt+f4`)
	- `shutdown windows` / `stop windows` (best-effort ACPI powerdown; falls back to forcing QEMU quit)
- Milestones / steps:
	- ✅ Kernel event: add `RAYOS_HOST_EVENT_V0:SHOW_WINDOWS_DESKTOP` emission for phrases like `show windows desktop` / `windows desktop`.
	- ✅ Host wiring: extend `scripts/test-boot.sh` to watch for `SHOW_WINDOWS_DESKTOP` and launch a Windows VM script (PID-file guarded).
	- ✅ Windows launcher script: add `./scripts/run-windows-subsystem-desktop.sh` that:
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
	- ✅ Add kernel events mirroring Linux:
		- `RAYOS_HOST_EVENT_V0:WINDOWS_SENDTEXT:<text>` (RayOS command: `windows type <text>`)
		- `RAYOS_HOST_EVENT_V0:WINDOWS_SENDKEY:<spec>` (RayOS command: `windows press <key>`)
		- `RAYOS_HOST_EVENT_V0:WINDOWS_SHUTDOWN`
	- ✅ Host wiring in `scripts/test-boot.sh`:
		- Route `WINDOWS_SENDTEXT` via `scripts/qemu-sendtext.py` to the Windows monitor socket.
		- Route `WINDOWS_SENDKEY` via `scripts/qemu-sendkey.py` to the Windows monitor socket.
		- Implement `WINDOWS_SHUTDOWN` via `system_powerdown` (fallback `quit` is TODO).
	- Logging: write `build/windows-desktop-launch.log` and `build/windows-desktop-control.log`.

32a) Windows desktop preflight smoke test (headless, no Windows disk required)
- Goal: validate the RayOS→host event path and deterministic ACK errors without requiring a Windows image.
- Verification: `./scripts/test-windows-desktop-preflight-headless.sh` (asserts `missing_WINDOWS_DISK_env` and `desktop_not_running` ACKs).
- Status: ✅ done

33) Windows readiness markers (deterministic automation hook) - ✅ done
- Goal: make “desktop ready” testable without manual observation.
- Success criteria: host can reliably determine when Windows is ready to accept input.
- Options (pick one to start):
	- ✅ QEMU Guest Agent (recommended): install QGA in the Windows guest; host polls guest state and emits `RAYOS_WINDOWS_READY`.
	- Screen-based readiness: simple OCR/template detection on the framebuffer (fallback, universal).
	- Guest-side marker: a startup task writes a marker to a serial channel (works but more brittle).

34) Policy contract (Windows) - ✅ done
- Goal: preserve the authority model: RayOS owns lifecycle, input, display, and policy.
- Milestones / steps:
	- ✅ Default networking OFF; explicit enable via host policy/env var.
	- ✅ Storage boundary: explicit disk image path(s) only; no accidental host mounts.
	- ✅ Resource caps: CPU/mem limits controllable from RayOS policy.
	- ✅ Snapshot strategy: define “pause/suspend” and snapshot hooks (RAM+device state), even if implementation is deferred.

35) GPU path (defer correctness, define interface now) - ✅ done
- Goal: define the long-term direction: Windows output is a RayOS surface (texture), not a direct display owner.
- Milestones / steps:
	- ✅ Start: virtual GPU with basic display for bring-up (no passthrough).
	- ✅ Next: define the virtual GPU contract needed for WDDM plausibility.
	- ✅ Later: RayOS-owned GPU scheduling and composited presentation.

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
- TODOs:
	- ✅ Add `virtio-gpu-pci` device to the `aarch64` QEMU VM.
	- ✅ Implement ACPI and PCI discovery logic in the `aarch64` kernel (MCFG + MMCONFIG scan).
	- ✅ Initialize the `virtio-gpu` device enough to read common config/features and complete a minimal `FEATURES_OK` handshake (markers asserted in `./scripts/test-boot-aarch64-kernel-headless.sh`).

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
- Status: ✅ done (virtio-gpu init attempt now succeeds/fails deterministically and the x86_64 headless smoke test asserts `RAYOS_X86_64_VIRTIO_GPU:FEATURES_OK` when `-device virtio-gpu-pci` is present; early-boot page fault fixed by removing premature HHDM assumptions.)

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
