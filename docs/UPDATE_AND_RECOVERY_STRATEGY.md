# RayOS Update + Recovery Strategy

Status: **draft / tracking stub**

Purpose: define how RayOS updates safely (and recovers) once it is installable.

---

## 1) Goals

- Safe updates with rollback.
- Preserve user data and VM continuity.
- Provide recovery entry points.

---

## 2) Topics to Decide

- Update mechanism:
  - A/B system partitions vs single-partition in-place
  - Signed update bundles (later) vs manual (v0)
- Rollback triggers and UX
- Compatibility/versioning for:
  - policy schema
  - VM registry format
  - VM saved-state format

---

## 3) Recovery

- Boot manager recovery entry
- Repair disk layout / rebuild boot entries
- Reset/repair VM state without deleting persistent disks

---

## 4) Related

- INSTALLER_AND_BOOT_MANAGER_SPEC.md
- DISK_LAYOUT_AND_PERSISTENCE.md
