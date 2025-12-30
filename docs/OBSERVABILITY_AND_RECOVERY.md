# RayOS Observability, Logging & Crash Recovery

Status: **draft (v0 observability + recovery)**

Purpose: define how RayOS records logs/telemetry, captures crash information, and provides recovery tools once it is installable.

---

## 1) Goals

- Make failures diagnosable on a standalone installed machine.
- Provide a predictable place for logs and crash artifacts.
- Provide recovery entry points that don’t require another computer.

---

## 2) Logging (v0: what/where)

v0 defines concrete sinks and paths (see `DISK_LAYOUT_AND_PERSISTENCE.md`):

### 2.1 Sinks

- **Serial console**: always on, used for bring-up + deterministic markers.
- **Persistent logs**: stored under `/var/rayos/logs/`.
- **In-memory ring buffer** (early boot): flushed to persistent logs when the data partition is mounted.

### 2.2 Structure

All log lines should follow a simple, grep-friendly structure:

`<ts> <level> <component> <event> <k=v ...>`

Examples:

- `2025-12-30T15:12:01Z INFO bootloader RAYOS_BOOTLOADER_START version=0.1`
- `2025-12-30T15:12:03Z INFO kernel RAYOS_READY mode=bicameral`
- `2025-12-30T15:12:10Z WARN policy RAYOS_POLICY_VIOLATION rule=network_default_off detail=requested_on`

Timestamps:

- Prefer UTC wall-clock once available.
- Before wall-clock is known: emit `ts=mono:<nanos>` and later emit a `RAYOS_TIME_SYNC` marker.

### 2.3 Retention

- Default rotation: keep last 10 files per log category, cap each at 10MiB.
- Privacy: input/AI logs default to **redacted** unless explicitly enabled by policy.

---

## 3) Health/Readiness

v0 requires stable markers (serial + persistent):

- `RAYOS_BOOTLOADER_START`
- `RAYOS_KERNEL_START`
- `RAYOS_READY`
- `RAYOS_LINUX_VM_READY` (hidden/background)
- `RAYOS_WINDOWS_VM_READY` (hidden/background)
- `RAYOS_PRESENTATION:<on|off>`

Desktop bridge (host harness) markers:

- `RAYOS_HOST_EVENT_V0:<...>`
- `RAYOS_HOST_ACK:<op>:<ok|err>:<detail>`

These are harness-level observability hooks and must not be treated as a security boundary.

---

## 4) Crash Handling

v0 crash handling is “minimum useful”:

### 4.1 Panic/Exception Strategy

- Always print a concise crash banner to serial + framebuffer (if available).
- Persist a crash report to `/var/rayos/crash/panics/<boot_id>.log` once the data partition is available.
- If the data partition is not available, cache to the in-memory ring buffer and flush on next boot (recovery scans and preserves it).

### 4.2 Crash Report Contents (v0)

- boot id + version + build id
- registers (arch-dependent)
- fault address + error code (when available)
- last N log lines from in-memory buffer

Memory dumps are deferred.

### 4.3 Watchdogs (v0 minimal)

- Boot watchdog: if `RAYOS_READY` not reached within N seconds, reboot into recovery after M failures.
- Desktop harness watchdog: if desktop QEMU is unresponsive, force quit and emit a marker.

---

## 5) Recovery UX

Minimum recovery entry points (v0):

- Boot manager recovery entry (boot installer/rescue)
- “Safe mode” boot (disable subsystems; minimal drivers)
- Tools:
  - view logs
  - repair disk layout / boot entries
  - reset VM saved-state without deleting VM disks

Recovery UX requirements:

- Works headless (serial-only) and with a local display.
- Never requires a second machine to read logs.
- Offers a “factory reset (preserve VM disks)” action and a “factory reset (wipe all)” action.

---

## 6) Boot IDs (v0)

Every boot generates a stable boot id:

- Stored as `/var/rayos/boot/current_boot_id`
- Archived as `/var/rayos/boot/history/<boot_id>.json`

Used to correlate logs/crash artifacts:

- `/var/rayos/logs/boot/<boot_id>.log`
- `/var/rayos/crash/panics/<boot_id>.log`

---

## 7) Related

- INSTALLER_AND_BOOT_MANAGER_SPEC.md
- DISK_LAYOUT_AND_PERSISTENCE.md
- UPDATE_AND_RECOVERY_STRATEGY.md
- BOOT_TROUBLESHOOTING.md
