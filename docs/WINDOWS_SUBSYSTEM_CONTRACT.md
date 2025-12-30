# RayOS Windows Subsystem Contract

This document defines the **non-negotiable contract** for running Windows as a **subsystem of RayOS**.

Core principle:

> **RayOS has full control over the Windows subsystem environment.**
> Windows is a managed guest runtime completely driven by RayOS policy.

---

## 1) Goals

- Run Windows for **app compatibility** while RayOS remains the host OS.
- RayOS owns **display**, **input**, **lifecycle**, and **policy**.
- Enable a baseline UX milestone:
  - **Embedded Windows desktop surface**: Windows appears as a RayOS surface/window (a texture), not as the owner of the physical display.

Additional goal (persistence across RayOS reboots):

- The Windows subsystem is a **long-lived VM instance** managed by RayOS. RayOS reboot must **reattach to the same instance** (stable identity + persistent storage) and should **resume the same session** when a saved-state exists.

Additional goal (boot behavior + invisibility by default):

- By default, RayOS should **boot/resume the Windows VM during RayOS boot** (background), but Windows is **not presented** and **not user-interactive** until RayOS explicitly enables presentation and input routing.

---

## 2) Non-goals

- Windows becoming the host OS.
- Granting Windows direct ownership of host hardware surfaces (GPU/display/input).
- Silent implicit recreation of the VM on each RayOS boot.

---

## 3) Authority model (who controls what)

RayOS is responsible for and controls:

- **Lifecycle**: create/start/stop/restart/suspend/snapshot/destroy the subsystem.
- **Continuity**: stable VM identity across RayOS reboots; resume/restore behavior.
- **Resources**: CPU/memory caps, storage quotas, device exposure.
- **Input**: keyboard/mouse/touch events. Windows receives input only through RayOS routing.
- **Graphics**: what is rendered, how it is presented, and what appears as a RayOS surface/window.
- **Filesystem**: what persistent volumes/disks exist and how they are mounted.
- **Networking**: whether networking exists at all; egress/ingress policy.

Windows:

- Sees only the synthetic devices RayOS exposes.
- Never bypasses RayOS policy to reach host devices.

---

## 4) Persistence & continuity (non-negotiable)

- RayOS must not treat the Windows subsystem as “new every boot.”
- There must be a stable VM identity concept (name/id) selecting:
  - backing disk image(s)
  - firmware/variables (UEFI vars)
  - vTPM state (if used)
  - device model and policy
- On RayOS boot:
  - If a saved-state exists and policy allows, RayOS should **resume from saved-state**.
  - Otherwise, RayOS must cold-boot the **same** persistent VM disk(s) for that identity.

---

## 5) Device model (baseline)

- Windows runs as a VM.
- Windows must not require direct passthrough of the host GPU/input for baseline operation.
- If Windows 11 is targeted, RayOS must provide required platform expectations via virtualization (e.g., vTPM when needed), under RayOS policy.

---

## 6) Presentation model

- Windows output is treated as a RayOS-managed surface (texture).
- “Show Windows desktop” means: attach presentation + input routing to the managed VM.
- “Hide Windows desktop” means: detach presentation (VM may continue running or be suspended per policy).

Hidden-by-default interaction requirement:

- When Windows is not presented, RayOS must not route user input to Windows unless explicitly requested.
- “Presented” is the gate that enables interactive input routing.

---

## 7) Security invariants (must always hold)

- Windows cannot escape RayOS policy to access host devices.
- All integration paths are mediated by RayOS-controlled virtual devices and channels.
- The lifecycle and persistence mechanisms must be auditable (clear logs for start/resume/suspend/shutdown).
