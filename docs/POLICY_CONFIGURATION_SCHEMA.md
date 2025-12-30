# RayOS Policy Configuration Schema

Status: **draft / tracking stub**

Purpose: define the concrete configuration format and policy controls RayOS uses for:

- VM lifecycle and persistence behavior
- device exposure
- networking defaults
- presentation gating (hidden vs presented)
- resource limits

---

## 1) Scope

Policies should cover at minimum:

- Linux VM:
  - autoboot/resume at RayOS boot (hidden)
  - present on request
  - networking default
  - storage mounts
  - CPU/memory caps
- Windows VM:
  - same as Linux plus vTPM/UEFI vars expectations

---

## 2) Decisions Needed

- Format: TOML vs JSON vs YAML
- Update mechanism: manual edit vs UI wizard vs signed policy bundles
- Where stored: RayOS data partition

---

## 3) Invariants

- “Presented” gates input routing.
- Default should be secure: networking off unless enabled.

---

## 4) Related

- LINUX_SUBSYSTEM_CONTRACT.md
- WINDOWS_SUBSYSTEM_CONTRACT.md
- DISK_LAYOUT_AND_PERSISTENCE.md
