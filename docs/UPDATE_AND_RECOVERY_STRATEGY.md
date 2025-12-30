# RayOS Update + Recovery Strategy

Status: **draft (v0 strategy)**

Purpose: define how RayOS updates safely (and recovers) once it is installable.

---

## 1) Goals

- Safe updates with rollback.
- Preserve user data and VM continuity.
- Provide recovery entry points.

---

## 2) v0 Model (pragmatic, installable)

v0 uses:

- **Immutable system payload** on the RayOS System partition (mounted read-only by default).
- **In-place replacement** of the system payload driven by an installer/update tool (host or recovery environment).
- **No background auto-update** in v0.
- **Unsigned update bundles** in v0 (developer + early adopter), with a clear path to signed bundles later.

This balances simplicity with rollback capability using versioned system directories.

---

## 3) System Layout for Updates (RayOS System)

RayOS System (`/sysroot`) stores versioned releases:

- `/sysroot/releases/<version>/` (e.g. `2025.12.30+dev.1`)
  - `efi/` (optional: loader copies or manifests)
  - `kernel/` (kernel/runtime artifacts)
  - `services/` (userland services when present)
  - `manifest.json` (hashes, compatibility metadata)
- `/sysroot/current -> /sysroot/releases/<version>` (symlink or small text file pointer)
- `/sysroot/previous -> ...` (one-step rollback pointer)

The bootloader loads artifacts from the ESP for now; installability work should converge on:

- ESP contains only minimal loader + pointers.
- System payload lives on RayOS System and is selected via `current/previous`.

---

## 4) Compatibility + Versioning Rules (v0)

Versioned formats and compatibility checks:

- Policy: `docs/POLICY_CONFIGURATION_SCHEMA.md` (`version = 0`)
- VM registry: `docs/DISK_LAYOUT_AND_PERSISTENCE.md` (registry JSON version field recommended)
- VM saved state: **disabled by default** in v0 (cold boot + persistent disks only)

Rules:

- If policy major version is unsupported, boot into recovery (do not guess).
- If VM registry is unsupported, boot with subsystems disabled and emit a clear marker.
- If VM saved state exists but is unsupported, ignore state and cold boot (do not delete).

---

## 5) Update Procedure (v0)

An update is a new release directory + pointer flip:

1) Preflight:
   - ensure ESP + System + Data partitions mounted and writable where needed
   - verify free space
2) Stage:
   - write new `/sysroot/releases/<new_version>/...`
   - write `/sysroot/releases/<new_version>/manifest.json`
3) Activate:
   - set `/sysroot/previous` to old `/sysroot/current`
   - set `/sysroot/current` to the new version
4) Reboot:
   - boot into `current`

Rollback:

- Manual: choose “RayOS (previous)” in boot manager or flip `current`/`previous` from recovery.
- Automatic (v0 minimal): if `current` boot fails N times (boot counter), revert to `previous`.

Boot counters live in RayOS Data (`/var/rayos/boot/`) so they persist across reboots.

---

## 6) Recovery Entry Points (v0)

Installer should register:

- `RayOS` (normal)
- `RayOS (recovery)` (always boots into recovery environment)
- `RayOS (previous)` (optional convenience)

Recovery environment capabilities:

- View/edit policy: `/var/rayos/policy/rayos-policy.toml`
- Validate disk layout, repair ESP entries, rebuild `current/previous` pointers
- Inspect logs + crash artifacts (`/var/rayos/logs`, `/var/rayos/crash`)
- Manage VM disks (list, rename, detach), without deleting by default

---

## 7) Observability Hooks (required)

Update and recovery operations must emit stable markers for automation:

- `RAYOS_UPDATE_BEGIN:<version>`
- `RAYOS_UPDATE_OK:<version>`
- `RAYOS_UPDATE_ERR:<version>:<detail>`
- `RAYOS_ROLLBACK:<from>:<to>:<reason>`
- `RAYOS_RECOVERY_ENTERED`

---

## 8) Related

- INSTALLER_AND_BOOT_MANAGER_SPEC.md
- DISK_LAYOUT_AND_PERSISTENCE.md
