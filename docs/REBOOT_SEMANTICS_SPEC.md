# RayOS Reboot Semantics Specification

Status: **draft (v0 spec)**

Purpose: Define how RayOS manages the lifecycle of guest VMs across host reboots, with a focus on preserving guest state.

---

## 1) Scope (v0)

This specification defines the "best-effort" reboot semantics for guest VMs, particularly the Linux and Windows subsystems. The primary goal is to make the guest environment feel persistent and long-lived, even when the underlying RayOS is updated or rebooted.

- **Goal:** Preserve guest VM state across RayOS reboots.
- **Mechanism:** Leverage host-based VM state save/restore capabilities.
- **Guest Agnostic:** The chosen mechanism should be as guest-agnostic as possible.

Non-goals (v0):
-   Guaranteed transactional state preservation.
-   Live migration of VMs between different hosts.
-   Complex multi-VM orchestration during reboot.

---

## 2) Proposed Reboot and Shutdown Procedure

When RayOS is about to shut down or reboot, it will instruct the VM Supervisor to gracefully suspend all running guest VMs.

### 2.1 Suspension Mechanism

The proposed mechanism is to use QEMU's state-saving capabilities, specifically the `savevm` monitor command. This command saves the entire VM state (RAM, device state, etc.) to a file on the host.

-   **Command:** `savevm <tag>`
-   **State File Location:** `/var/rayos/vm/<vm-id>/<tag>.state`

The VM Supervisor will be responsible for:
1.  Generating a unique tag for the saved state (e.g., a timestamp or a sequential ID).
2.  Executing the `savevm` command on the VM's QEMU monitor.
3.  Recording the tag of the saved state in the VM's registry record.

### 2.2 Shutdown/Reboot Sequence

1.  RayOS initiates shutdown/reboot.
2.  The system sends a "prepare-to-suspend" signal to the VM Supervisor.
3.  For each running VM, the VM Supervisor:
    a.  Generates a save state tag (e.g., `pre-reboot-<timestamp>`).
    b.  Executes `savevm` on the VM's monitor.
    c.  Waits for the save to complete.
    d.  Updates the VM's record in `registry.json` with the new saved state tag.
4.  Once all VMs are suspended, the VM Supervisor signals "ready-to-shutdown" to the system.
5.  RayOS proceeds with the shutdown/reboot.

---

## 3) Proposed Boot and Resume Procedure

On boot, the VM Supervisor will check the registry for VMs that have a saved state and automatically resume them.

### 3.1 Resumption Mechanism

The proposed mechanism is to use QEMU's `loadvm` monitor command, or to launch QEMU with the `-loadvm` flag.

-   **Command:** `loadvm <tag>`
-   **QEMU Flag:** `-loadvm <tag>`

The `-loadvm` flag is preferred as it restores the VM state on launch.

### 3.2 Boot/Resume Sequence

1.  RayOS boots and starts the VM Supervisor.
2.  The VM Supervisor reads `registry.json`.
3.  For each VM, it checks if a saved state tag is present.
4.  If a saved state exists, the supervisor launches the VM with the `-loadvm <tag>` flag, along with the other VM configuration options.
5.  The VM is resumed in the background, in a hidden state (e.g., with a VNC display but no active client).
6.  After a successful resume, the saved state tag may be removed from the registry to prevent accidental re-use.

---

## 4) Host Tooling Modifications

The host-side scripts (`test-boot.sh`, etc.) will need to be modified to support this new flow.

-   **`test-boot.sh`:**
    -   On exit (e.g., via `Ctrl+C`), the `cleanup_bridge` function should be modified to save the state of the hidden Linux VM before killing it.
    -   The `start_linux_desktop_hidden` function should be modified to check for a saved state and use `-loadvm` if available.

-   **`run-linux-subsystem-desktop-auto.sh`:**
    -   This script needs to be able to accept a `-loadvm <tag>` argument and pass it to QEMU.

---

## 5) Related Documents

-   [VM_REGISTRY_SPEC.md](VM_REGISTRY_SPEC.md)
-   [SYSTEM_ARCHITECTURE.md](SYSTEM_ARCHITECTURE.md)
