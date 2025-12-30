# RayOS Observability, Logging & Crash Recovery

Status: **draft / tracking stub**

Purpose: define how RayOS records logs/telemetry, captures crash information, and provides recovery tools once it is installable.

---

## 1) Goals

- Make failures diagnosable on a standalone installed machine.
- Provide a predictable place for logs and crash artifacts.
- Provide recovery entry points that don’t require another computer.

---

## 2) Logging (What/Where)

Topics to decide:

- Log sinks:
  - serial console (debug)
  - persistent log files on RayOS data partition
  - ring buffer in memory for early boot
- Log structure:
  - timestamps (host time vs monotonic)
  - severity levels
  - component tags (bootloader, kernel, vmm, compositor, input, policy, volume)
- Retention:
  - size caps + rotation
  - privacy posture (especially for input/AI)

---

## 3) Health/Readiness

- Stable readiness markers for:
  - RayOS core services
  - Linux VM started/resumed (hidden)
  - Windows VM started/resumed (hidden)
  - presentation gate enabled/disabled

---

## 4) Crash Handling

Topics to decide:

- Panic/exception strategy:
  - what gets printed to screen/serial
  - what gets persisted
- Crash dump formats:
  - minimal backtrace + registers
  - optional memory dump (likely deferred)
- Watchdogs:
  - detect hung subsystems (VM supervisor, compositor)

---

## 5) Recovery UX

Minimum recovery entry points:

- Boot manager recovery entry (boot installer/rescue)
- “Safe mode” boot (disable subsystems; minimal drivers)
- Tools:
  - view logs
  - repair disk layout / boot entries
  - reset VM saved-state without deleting VM disks

---

## 6) Related

- INSTALLER_AND_BOOT_MANAGER_SPEC.md
- DISK_LAYOUT_AND_PERSISTENCE.md
- UPDATE_AND_RECOVERY_STRATEGY.md
- BOOT_TROUBLESHOOTING.md
