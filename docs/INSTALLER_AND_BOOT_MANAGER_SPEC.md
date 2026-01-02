# RayOS Installer + Boot Manager Spec

Status: **draft (concrete v0 spec)**

Purpose: specify how RayOS installs from a USB and how boot selection works (Windows Boot Manager / GRUB-like).

---

## 1) Scope (v0)

v0 focuses on an installable single-machine workflow:

- UEFI boot from USB installer
- guided partition selection/creation
- install RayOS system payload + configure boot entries
- create/configure RayOS Data layout
- provide a recovery entry and rollback-friendly boot selection

Non-goals (v0):
- full disk encryption UX (document hooks only)
- Secure Boot signing flow (document posture and failure UX; implementation may be deferred)

---

## 2) Installer Wizard (USB Boot)

Minimum v0 flow:

1) Boot from USB (UEFI)
2) Installer wizard starts (text UI acceptable)
3) Select/create install partition for RayOS
4) Select RayOS data partition (or create)
5) Optional: choose VM storage locations (Linux/Windows) (partition/path)
6) Install RayOS + configure boot entries
7) Reboot into installed RayOS

---

## 3) Disk / Partition Requirements (v0)

RayOS requires a UEFI system with an ESP and two RayOS partitions (see `DISK_LAYOUT_AND_PERSISTENCE.md`):

- ESP (FAT32) mounted at `/boot/efi`
- RayOS System (read-only by default) mounted at `/sysroot`
- RayOS Data (persistent) mounted at `/var/rayos`

Installer behaviors:
- Prefer using an existing ESP if present; otherwise create one.
- Never format a partition without explicit confirmation (type-to-confirm is acceptable).
- Validate partitions are large enough:
  - ESP: ≥ 256MiB recommended (512MiB preferred)
  - System: depends on payload; v0 should enforce a minimum and show required space
  - Data: recommend at least 20GiB (larger if guests are used)

---

## 4) Boot Manager Requirement

We need a boot manager experience similar to Windows Boot Manager / GRUB.

Options to decide:

- Use an existing UEFI boot manager (preferred when feasible, e.g. rEFInd/systemd-boot)
- Build a minimal RayOS boot manager (if dependencies or UX constraints require it)

Minimum required behaviors:

- Enumerate RayOS installs (and recovery entry)
- Default selection + timeout
- Clear logging of selection and failure reasons
- Recovery mode entry (boot installer/rescue)

---

## 5) Boot Entries & Selection (v0)

Installer must register at least:

- `RayOS` (normal boot)
- `RayOS (recovery)` (always boots into recovery environment/tools)

Optional convenience:
- `RayOS (previous)` (one-step rollback)

Selection policy (v0):
- Default to `RayOS` with a short timeout (e.g. 3–5s).
- If the last boot failed repeatedly (boot counter), default to `RayOS (previous)` or prompt user to choose.

Logging requirements:
- Boot manager writes a selection marker to persistent logs when available:
  - `RAYOS_BOOT_SELECT:<entry>:<reason>`

---

## 6) Install Procedure (v0)

### 6.1 Preflight

- Confirm UEFI mode (fail if legacy-only).
- Discover disks and partitions; show model/size and current partition table.
- Confirm target is not the installer USB device (explicit “this will be erased” guard).
- Verify ESP is writable.

### 6.2 Partition selection/creation

Two supported flows:

1) **Use existing partitions**
   - choose existing ESP
   - choose an existing partition for RayOS System (may be formatted)
   - choose an existing partition for RayOS Data (may be formatted)

2) **Create partitions**
   - shrink or allocate free space (when supported)
   - create RayOS System + Data partitions
   - format filesystems

### 6.3 Install payload

Write:
- minimal boot artifacts to the ESP (UEFI loader + pointers/manifests)
- system payload under `/sysroot/releases/<version>/...` (see `UPDATE_AND_RECOVERY_STRATEGY.md`)
- set `/sysroot/current` pointer

Initialize:
- `/var/rayos/` directory structure (`DISK_LAYOUT_AND_PERSISTENCE.md`)
- default policy file `policy/rayos-policy.toml` (`POLICY_CONFIGURATION_SCHEMA.md`)
- initial VM registry scaffolding (empty registry is OK)

### 6.4 Register boot entries

- Create UEFI boot entries:
  - `RayOS` → loader + normal mode args
  - `RayOS (recovery)` → loader + recovery mode args
- Validate by re-reading the UEFI NVRAM entry list.

### 6.5 Reboot

- Emit install markers (serial + persistent):
  - `RAYOS_INSTALL_BEGIN:<version>`
  - `RAYOS_INSTALL_OK:<version>`
  - `RAYOS_INSTALL_ERR:<version>:<detail>`

---

## 7) Recovery Environment (v0)

Recovery must be bootable even when the normal system payload is broken.

Minimum recovery capabilities:
- View/edit policy at `/var/rayos/policy/rayos-policy.toml`
- Inspect logs (`/var/rayos/logs/`) and crash artifacts (`/var/rayos/crash/`)
- Repair boot pointers (`/sysroot/current`, `/sysroot/previous`) and boot entries
- Disable subsystems (“safe mode”) without losing VM disks

---

## 8) Failure Modes to Document

- Install target mistakes (safety confirmations)
- ESP missing/readonly
- Secure Boot enabled (expected UX)
- Boot entry missing/corrupt
- Data partition missing/unmountable → boot recovery and preserve crash artifacts
- Policy parse failure → safe mode + clear marker (fail closed)

---

## 9) Related

- INSTALLABLE_RAYOS_PLAN.md
- BOOT_TROUBLESHOOTING.md
- DISK_LAYOUT_AND_PERSISTENCE.md
- POLICY_CONFIGURATION_SCHEMA.md
- UPDATE_AND_RECOVERY_STRATEGY.md
