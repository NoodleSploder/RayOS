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

Installability implies an end-to-end install path on real machines:

1) Boot from a RayOS USB stick (UEFI)
2) Launch an **installer wizard** (text UI first is fine)
3) Partitioning step:
  - Select an existing partition, or
  - Create a new partition (guided)
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

## 9) Related Docs

- Linux design/contract: LINUX_SUBSYSTEM_DESIGN.md, LINUX_SUBSYSTEM_CONTRACT.md
- Windows design/contract: WINDOWS_SUBSYSTEM_DESIGN.md, WINDOWS_SUBSYSTEM_CONTRACT.md
- Consolidated TODOs: RAYOS_TODO.md
