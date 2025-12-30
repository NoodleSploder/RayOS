# RayOS Disk Layout & Persistence Spec

Status: **draft / tracking stub**

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

## 2) Proposed Partitioning (UEFI)

- ESP (FAT32): UEFI boot artifacts + boot manager entries
- RayOS System: immutable/semi-immutable OS payload
- RayOS Data: persistent config + VM storage + logs

Optional:
- Dedicated Linux VM storage partition (optional)
- Dedicated Windows VM storage partition (optional)

Note: “Linux/Windows partitions” here mean **VM disk/state storage**, not dual-boot.

---

## 3) Required Persistent Data

- VM registry: name/id → disk paths → device model → policy
- VM disks:
  - linux.img
  - windows.img
- VM state (optional/target): suspend/resume saved-state, snapshots
- Logs and crash artifacts

---

## 4) Invariants

- Stable VM identity across RayOS reboots.
- Prefer resume-from-saved-state when available; fallback cold-boot same disk.

---

## 5) Related

- INSTALLABLE_RAYOS_PLAN.md
