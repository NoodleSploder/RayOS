# RayOS Disk Layout & Persistence Spec

Status: **draft (concrete v0 layout)**

Purpose: define the concrete on-disk layout needed for an installable RayOS:

- Partitions
- Filesystems
- Required directories/files
- Persistence rules for VM identity/disks/state

---

## 1) Goals

- Make install/recovery predictable.
- Make Linux/Windows VM continuity across RayOS reboots reliable.
- Define where logs and saved-state live.

---

## 2) Partitioning (UEFI, v0)

RayOS uses three partitions:

1) **ESP** (FAT32, ~512MiB)
- Mount: `/boot/efi`
- Contents:
  - `EFI/BOOT/BOOTX64.EFI` (fallback)
  - `EFI/RAYOS/` (RayOS loader + optional signed artifacts)
  - `EFI/BOOT/` entries created by installer

2) **RayOS System** (read-only by default, ext4 or squashfs-on-ext4, size variable)
- Mount: `/sysroot`
- Contents:
  - immutable OS payload (kernel/runtime, built-in services)
  - versioned release manifests (for rollback)

3) **RayOS Data** (ext4, size variable)
- Mount: `/var/rayos`
- Contents:
  - policy/config
  - VM registry + VM disks/state
  - logs + crash artifacts

Optional (future):
- Separate partitions for large VM storage, or encrypted data partition.

Note: “Linux/Windows partitions” here mean **VM disk/state storage**, not dual-boot.

---

## 3) Required On-Disk Layout (RayOS Data)

Base directories (under `/var/rayos/`):

- `policy/`
  - `rayos-policy.toml` (see `POLICY_CONFIGURATION_SCHEMA.md`)
- `vm/`
  - `registry.json` (VM identity → disks → device config → policy refs)
  - `linux/<vm_id>/`
    - `rootfs.ext4` (or qcow2 in future)
    - `state/` (optional; suspend/snapshot)
    - `logs/`
  - `windows/<vm_id>/`
    - `disk.qcow2`
    - `ovmf_vars.fd`
    - `tpm/` (swtpm state)
    - `state/` (optional)
    - `logs/`
- `logs/`
  - `rayos.log` (host/runtime)
  - `boot/` (bootloader markers, boot sessions)
  - `desktop/` (linux/windows desktop bridge actions)
- `crash/`
  - `panics/`
  - `core/` (when available)
  - `last-boot/` (collected artifacts from previous boot failure)

Development mapping (current repo tooling):
- Host scripts currently use `build/` as the data root; for installability this should migrate to `/var/rayos/` on the installed system while keeping host tooling compatible.

---

## 4) Invariants

- Stable VM identity across RayOS reboots.
- Prefer resume-from-saved-state when available; fallback cold-boot same disk.
- No implicit host mounts into guests; all storage exposures are explicit and policy-governed.
- Policy is the source of truth for networking enablement; default remains off.

---

## 5) VM Registry Record (v0)

`/var/rayos/vm/registry.json` is a JSON object with stable IDs and relative paths.

Minimum fields per VM:

- `vm_id` (stable string)
- `kind` (`linux` | `windows`)
- `storage` (relative paths under `/var/rayos/`)
- `devices` (virtio input/gpu, vtpm for Windows)
- `policy_ref` (optional; path to a policy file or a named profile)

---

## 6) Related

- INSTALLABLE_RAYOS_PLAN.md
