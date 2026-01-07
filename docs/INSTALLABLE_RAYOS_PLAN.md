# Installable RayOS Plan (All-Encompassing, No External Host)

Purpose: track what it means for RayOS to be **installable on a machine** while preserving the project’s direction (Linux + Windows subsystems as long-lived VMs managed by RayOS).

This document is planning-only; it is **not** an implementation change.

---

## 1) Definition: “Installable” + “All Encompassing”

RayOS is **installable** when it can be installed onto a machine’s local storage and then:

- Boots on bare metal (UEFI → RayOS bootloader → RayOS runtime)
- Brings up essential hardware locally (storage, basic display/output, input)
- Runs its subsystems (Linux and Windows) **without requiring another computer** to:
  - launch VMs
  - inject input
  - parse serial logs
  - respond to the AI bridge protocol

“All encompassing” means:

- Any orchestration currently performed by host-side scripts/tools becomes a RayOS-native service (or is eliminated).
- Persistent state (VM disks, VM identity registry, policy config, snapshots/saved-states) is stored on the installed machine.

---

## 2) Current Reality vs Installable Reality

Today’s repo has powerful host-side tooling (QEMU scripts, conductor/ai_bridge) that is ideal for bring-up and automated tests.

Installable RayOS requires a production path where:

- QEMU/conductor are **development and CI harness only**.
- RayOS itself provides:
  - VM supervisor/lifecycle management
  - input routing
  - presentation gating (hide/present)
  - AI bridge (if the intent is to run “AI” locally)

---

## 3) Target Boot + Runtime Shape (High Level)

### Boot flow

1) UEFI firmware loads RayOS bootloader
2) RayOS bootloader loads RayOS kernel/runtime
3) RayOS mounts local persistent storage (system + data)
4) RayOS starts core services (policy, compositor, input, VM supervisor)
5) RayOS starts/resumes Linux + Windows VMs in the background (hidden)
6) Only on explicit request: RayOS presents a VM surface and enables input routing

### Installer flow (bootable USB)

Distribution artifact for the installer is a **bootable UEFI ISO** intended to be written to a USB thumb drive. Booting from this media enters the RayOS installer environment directly (no host OS dependency).

Installability implies an end-to-end install path on real machines:

1) Boot from a RayOS USB stick (UEFI)
2) Launch an **installer wizard** (text UI first is fine)
3) Partitioning step (interactive partition manager/creator/selector is required):
  - Enumerate existing disks/partitions and highlight RayOS-safe targets
  - Allow selecting an existing partition, or
  - Create a new partition (guided, with safety confirmations)
4) Install RayOS boot artifacts + system image
5) Configure persistence locations (RayOS data + VM storage)
6) Reboot into the installed RayOS

### VM lifecycle requirement (already adopted)

- Linux and Windows are **long-lived VMs** managed “in perpetuity”.
- RayOS reboot should reattach/resume rather than create a new VM.
- VMs can run **hidden** until presented; “present” gates input routing.

---

## 4) Persistence Model (Disk + Identity + State)

Minimum needed for installability:

- A RayOS-managed on-disk storage layout that can store:
  - VM identity registry (name/id → disk paths → device config → policy)
  - Linux VM disk image(s)
  - Windows VM disk image(s)
  - optional saved-state/snapshot artifacts
  - logs/telemetry

Installer requirements:

- The installer must support choosing where RayOS installs.
- The installer should optionally allow selecting where VM storage lives:
  - Linux VM storage partition/path (optional)
  - Windows VM storage partition/path (optional)

Notes:

- Treat “Linux partition” / “Windows partition” as **storage locations for VM disks/state**, not bare-metal dual-boot installs.
- Default can be “store VM disks under RayOS data volume” unless the user chooses dedicated partitions.

Design constraints:

- Keep VM identity stable across RayOS reboots.
- Make “resume if state exists” the default.
- Keep “fresh start” an explicit action.

---

## 5) Windows Reality Check (Redistribution + Provisioning)

Windows images/media generally cannot be redistributed as part of RayOS.

Therefore, the installable/all-encompassing plan should assume:

- RayOS is fully self-contained as an OS and VM manager.
- The user supplies Windows installation media / disk image once (first-boot provisioning).
- After provisioning, Windows runs as a persistent long-lived VM on the same machine.

This is a product/legal constraint, not a technical blocker.

---

## 6) What Must Move Into RayOS (No External Host Dependency)

If a feature requires a second machine today, it needs an in-RayOS equivalent.

Key items:

- VM lifecycle orchestration (start/resume/suspend/shutdown) must be RayOS-native.
- Input injection must be RayOS-native (not QEMU monitor injection).
- “Present/hide” must be RayOS compositor routing (not QEMU window focus tricks).
- AI bridge protocol (if still desired) must run locally:
  - either a local model/runtime,
  - or an explicit network service (but that violates “no other machine” unless optional).

---

## 7) Milestones (Planning)

M0 — Define target hardware scope
- Decide x86_64 vs aarch64 vs both
- Decide whether hardware virtualization (VT-x/AMD-V) is required

M1 — Installable storage + boot
- Installer flow writes RayOS boot artifacts and partitions
- RayOS can boot from local disk and mount its data volume

M1a — USB boot + installer wizard
- Bootable USB image exists
- Installer can select/create partition(s) and install RayOS
- Installer can optionally configure VM storage locations (Linux/Windows)

M2 — RayOS-native VM supervisor (Linux first)
- Start/resume Linux at RayOS boot (hidden)
- Present/hide gates surface visibility and input routing

M3 — Windows provisioning + persistent VM
- User-provided Windows media/image path
- Persistent VM registry + vTPM/UEFI vars state stored locally
- Start/resume Windows at RayOS boot (hidden)

M4 — Remove external-host requirements
- Features that relied on host-side scripts have in-OS replacements

---

## 8) Open Questions (To Answer Later)

- Target platforms: x86_64 only first? aarch64? both?
- Required acceleration: is “no-VT fallback” required or acceptable to omit?
- Where does “AI” run in the installable product: local inference vs optional network?
- How aggressive should background VM boot be (always-on vs policy-controlled)?

---

## 10) Installer Wizard Options (Track Now, Decide Later)

Keep the initial installer minimal, but these are plausible options worth tracking:

- Install target: disk/partition selection + safety confirmation
- Filesystem choice for RayOS data partition (pick one initially; keep it consistent)
- VM storage locations:
  - default (RayOS data volume)
  - optional dedicated partitions/paths for Linux/Windows VM disks + saved-state
- Networking during install:
  - offline install (default)
  - optional network enable for updates/provisioning
- Boot mode expectations:
  - UEFI-only (recommended)
  - Secure Boot story (defer if needed, but track)
- Boot manager:
  - Need a boot manager experience similar to Windows Boot Manager / GRUB.
  - Prefer using an existing UEFI boot manager when possible (simpler, proven).
  - If we build our own minimal boot manager, scope it to:
    - enumerating RayOS installs
    - selecting a default boot entry with timeout
    - recovery entry (boot into installer/rescue)
    - clear logging of boot selection and failure reasons
- Optional disk encryption for RayOS data + VM volumes (track; can be deferred)
- First-boot configuration:
  - machine name
  - time/locale
  - policy defaults (e.g., whether Linux/Windows should autoboot hidden)

---

## 11) Installability 2026 Execution Checklist (Jan 07 Update)

### 11.1 Blocker Audit

| Area | Current State | Gaps / Blockers |
| --- | --- | --- |
| Bootable installer artifacts | No end-to-end installer image produced; `build_iso` creates dev harness media only | Need scripted ISO/USB build that bundles the installer runtime and validates signatures |
| Installer wizard (partitioning + copy) | Spec exists (`INSTALLER_AND_BOOT_MANAGER_SPEC.md`); no runtime implementation | CLI/TUI flow to select target disk, partition safely, and copy RayOS system image |
| Disk layout + persistence | Spec finalized (`DISK_LAYOUT_AND_PERSISTENCE.md`); serialization partially implemented via `VM_REGISTRY_SPEC.md` | Need code that materializes the layout on disk, mounts at boot, and enforces invariants |
| Boot manager | Spec complete; current flows rely on UEFI fallback entries or dev harness | Implement minimal boot entry management + recovery entry provisioning |
| Update & recovery | Strategy doc in place; automated updater/recovery artifacts not implemented | Build update package format, recovery partition layout, and validation tests |
| Policy configuration | Schema defined; installer does not collect or apply defaults | Add policy bootstrap (networking, auto-boot, resource limits) during install |
| Security posture | Threat model captured; secure boot & artifact signing unimplemented | Decide signing pipeline, ensure boot artifacts are verifiable, plan secure boot story |
| Observability & crash recovery | Spec completed; tooling assumes dev harness | Implement persistent log rotation, crash dump capture, recovery shell integration |
| Automated validation | Numerous headless smokes exist for VMM features; none for installer/update flows | Add CI headless tests that exercise install-to-disk, recovery boot, and update rollback |

### 11.2 Near-Term Milestones

1. **M1 Bootstrap Installer Runtime**
  - Create a minimal installer binary (Rust) that can enumerate disks, detect existing RayOS installs, and emit a dry-run plan.
  - Add integration test harness using QEMU disks to validate dry-run output.

2. **M2 Guided Partition + Copy Flow**
  - Implement interactive CLI wizard (text/console UI acceptable) with a partition manager/selector capable of safe edits before filesystem creation and system image copy.
  - Integrate disk layout enforcement from `DISK_LAYOUT_AND_PERSISTENCE.md`.

3. **M3 Boot Manager & Recovery Entry**
  - Author boot entry provisioning code (UEFI Boot#### entries) and create a recovery boot option.
  - Provide fallback instructions and automated verification via headless tests.

4. **M4 Persistent Policy & Registry Bootstrap**
  - During install, capture policy defaults and seed the VM registry with placeholders for Linux/Windows subsystems.
  - Ensure first boot consumes the seeded policy/registry without relying on host tooling.

5. **M5 Update/Recovery Pipeline Prototype**
  - Define update image format, implement staged updates, and create recovery media build script.
  - Add smoke tests for rollback and recovery boot.

### 11.3 First Implementation Targets

- Initialize a new crate/binary `crates/installer` with disk enumeration + dry-run planning (priority).
- Provide a helper wrapper (`scripts/tools/run_installer_planner.sh`) so CI and developers can surface the JSON dry-run report quickly.
- Default tooling emits a **sample layout**; require explicit `--enumerate-local-disks` opt-in (within installer/test VMs only) before touching `/sys/block`.
- Produce a developer-facing checklist in `docs/RAYOS_TODO.md` referencing the new milestones so day-to-day work tracks against installability goals.
- Schedule CI work item to prototype a headless install test (QEMU disk → install → reboot) once installer dry-run exists.

---

## 12) How to Build and Test Installer Media Locally (Jan 07, 2026)

### Build installer ISO/USB artifacts

Run this from the RayOS repo root:

```bash
scripts/build-installer-media.sh
```

Outputs:
  - build/rayos-installer.iso (UEFI bootable ISO)
  - build/rayos-installer-usb.img (raw USB image)

### Run installer binary dry-run test

This test runs the installer binary directly in sample mode (no local disk enumeration) and verifies it detects the sample disk layout and emits proper markers:

```bash
scripts/test-installer-dry-run.sh
```

Expected output:
  - "PASS: Installer markers present and valid"
  - "PASS: JSON payload contains disk records"
  - Stderr markers: `RAYOS_INSTALLER:STARTED`, `RAYOS_INSTALLER:SAMPLE_MODE`, `RAYOS_INSTALLER:PLAN_GENERATED`, `RAYOS_INSTALLER:JSON_EMITTED`, `RAYOS_INSTALLER:COMPLETE`

### Run headless QEMU smoke test for installer media

This boots the installer USB image with a disposable virtual disk attached and checks for expected boot markers and installer binary presence:

```bash
scripts/test-installer-boot-headless.sh
```

Expected output:
  - "PASS: installer media boots (markers present)"
  - Serial and QEMU logs in build/

This test does not touch any host disks and is safe to run on a dev machine.

### Summary of installer media pipeline (Jan 07, 2026)

| Component | Status | Location | Notes |
| --- | --- | --- | --- |
| Build script | ✓ Complete | [scripts/build-installer-media.sh](../../scripts/build-installer-media.sh) | Wraps `build-iso.sh` to produce installer-labeled ISO/USB artifacts |
| Installer binary | ✓ Complete | [crates/installer/](../../crates/installer/) | Bundled into boot media ESP; emits markers for test verification |
| Boot media smoke test | ✓ Complete | [scripts/test-installer-boot-headless.sh](../../scripts/test-installer-boot-headless.sh) | Verifies media boots under QEMU with attached target disk |
| Installer dry-run test | ✓ Complete | [scripts/test-installer-dry-run.sh](../../scripts/test-installer-dry-run.sh) | Runs installer binary directly and validates marker sequence and JSON output |
| Bootloader registry check | ✓ Complete | [crates/bootloader/uefi_boot/src/installer.rs](../../crates/bootloader/uefi_boot/src/installer.rs) | Detects `installer_mode` flag in registry.json; logs detection |
| Bootloader integration docs | ✓ Complete | [docs/BOOTLOADER_INSTALLER_INTEGRATION.md](./BOOTLOADER_INSTALLER_INTEGRATION.md) | Architecture, design decisions, next steps |
| Kernel/bootloader integration | ⏳ Pending | — | Bootloader needs to know when to invoke installer (via flag or special registry entry) |
| Partition manager flow | ⏳ Pending | — | Interactive partition selection/creation; currently placeholder in installer planner |
| Install-to-disk validation test | ⏳ Pending | — | Test that verifies installer can write to the target disk safely (dry-run only initially) |

---

## 13) Bootloader Installer Integration (Jan 07, 2026)

The bootloader now has infrastructure to detect and invoke the installer when requested. See [BOOTLOADER_INSTALLER_INTEGRATION.md](./BOOTLOADER_INSTALLER_INTEGRATION.md) for full details.

Key changes:
- `crates/bootloader/uefi_boot/src/installer.rs`: Registry-based installer mode detection
- `crates/bootloader/uefi_boot/src/main.rs`: Integrated check before kernel load
- Installer can be activated by setting `"installer_mode": true` in /EFI/RAYOS/registry.json

Current status:
- ✓ Registry flag detection implemented
- ✓ Installer binary loading logic designed
- ⏳ Actual ELF chainloading not yet implemented (placeholder)
- ⏳ Kernel-subprocess model being evaluated as cleaner alternative

---

## 9) Related Docs

- Linux design/contract: LINUX_SUBSYSTEM_DESIGN.md, LINUX_SUBSYSTEM_CONTRACT.md
- Windows design/contract: WINDOWS_SUBSYSTEM_DESIGN.md, WINDOWS_SUBSYSTEM_CONTRACT.md
- Consolidated TODOs: RAYOS_TODO.md
