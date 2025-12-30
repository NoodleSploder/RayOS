# RayOS System Architecture (Top-Level)

Status: **draft / tracking stub**

Purpose: unify the end-to-end architecture across bootloader, kernel/runtime, subsystems (Linux/Windows VMs), compositor/presentation, input routing, policy, storage, and AI components.

---

## 1) Goals

- Provide a single “how it all fits together” view.
- Define the long-running services/processes and their boundaries.
- Define the primary communication paths (events/IPC) and ownership.

---

## 2) Components (to formalize)

- Boot chain: UEFI → RayOS bootloader → RayOS runtime
- Core runtime services:
  - VM Supervisor (Linux + Windows lifecycle)
  - Display/Compositor (presentation gating)
  - Input Broker (routing + injection)
  - Policy Engine
  - Volume/Storage service
  - Optional AI runtime (local inference)

---

## 3) Contracts / Invariants

- RayOS is authoritative for display + input + lifecycle + policy.
- Linux/Windows are long-lived VMs: start/resume at RayOS boot (hidden), present on request.
- “Presented” gates interactive input routing.

---

## 4) Open Questions

- Event bus vs direct service calls?
- Where does policy live (file format + update mechanism)?
- What is the minimum “installable” hardware envelope?

---

## 5) Related

- Linux: LINUX_SUBSYSTEM_DESIGN.md, LINUX_SUBSYSTEM_CONTRACT.md
- Windows: WINDOWS_SUBSYSTEM_DESIGN.md, WINDOWS_SUBSYSTEM_CONTRACT.md
- Installability: INSTALLABLE_RAYOS_PLAN.md
