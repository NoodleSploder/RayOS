# RayOS VM Registry Specification

Status: **draft (v0 spec)**

Purpose: Define the structure and management of the VM registry, which stores the configuration and state of all guest VMs managed by RayOS.

---

## 1) Scope (v0)

This specification defines a minimal, file-based VM registry sufficient for the v0 Linux and Windows subsystem lifecycles.

- **Registry Scope:** The registry is the single source of truth for all VMs known to RayOS.
- **VM Identity:** Each VM has a stable, unique identifier.
- **Persistence:** The registry is stored persistently on the RayOS data partition.
- **Atomicity:** Changes to the registry should be atomic to prevent corruption.

Non-goals (v0):
- A complex database-backed registry.
- Real-time synchronization across multiple RayOS nodes.
- Per-user VM registries.

---

## 2) Registry Structure and Location

### 2.1 Location

The VM registry is stored as a single JSON file on the RayOS data partition:

- **Path:** `/var/rayos/vm/registry.json`

This location ensures that the registry persists across RayOS reboots and is available to the VM Supervisor and other core services.

### 2.2 Format

The `registry.json` file contains a JSON object where each key is a unique VM ID (UUID). The value is a VM record object.

```json
{
  "vms": {
    "<vm-uuid-1>": {
      "id": "<vm-uuid-1>",
      "name": "Linux-Desktop",
      "type": "linux",
      "state": "stopped",
      "storage": {
        "disks": {
          "root": "/var/rayos/vm/linux-desktop/root.ext4",
          "data": "/var/rayos/vm/linux-desktop/data.ext4"
        }
      },
      "devices": {
        "vcpu": 4,
        "memory_mb": 4096,
        "network": { "enabled": false },
        "tpm": { "enabled": false }
      },
      "policy_profile": "default-linux"
    },
    "<vm-uuid-2>": {
      "id": "<vm-uuid-2>",
      "name": "Windows-11",
      "type": "windows",
      "state": "suspended",
      "storage": {
        "disks": {
          "system": "/var/rayos/vm/win11/system.qcow2"
        },
        "ovmf_vars": "/var/rayos/vm/win11/ovmf_vars.fd",
        "tpm_state": "/var/rayos/vm/win11/tpm_state"
      },
      "devices": {
        "vcpu": 8,
        "memory_mb": 8192,
        "network": { "enabled": true, "profile": "unrestricted" },
        "tpm": { "enabled": true }
      },
      "policy_profile": "default-windows"
    }
  }
}
```

---

## 3) VM Record Schema

Each VM record contains the following fields:

- **`id`** (string, required): A unique UUID for the VM. This ID is immutable.
- **`name`** (string, required): A human-readable name for the VM (e.g., "Linux-Desktop").
- **`type`** (string, required): The type of guest OS. Allowed values: `"linux"`, `"windows"`.
- **`state`** (string, required): The last known state of the VM. Allowed values:
    - `"stopped"`: The VM is not running.
    - `"running"`: The VM is running.
    - `"suspended"`: The VM is suspended to disk/RAM.
    - `"hibernated"`: The VM is hibernated.
- **`storage`** (object, required): Paths to storage-related files.
    - **`disks`** (object, required): A map of disk names to their paths on the data partition.
    - **`ovmf_vars`** (string, optional): Path to the OVMF variables file (for UEFI VMs).
    - **`tpm_state`** (string, optional): Path to the TPM state directory (for vTPM).
- **`devices`** (object, required): Configuration for virtual devices.
    - **`vcpu`** (number, required): The number of virtual CPUs.
    - **`memory_mb`** (number, required): The amount of memory in megabytes.
    - **`network`** (object, required): Network configuration.
        - **`enabled`** (boolean, required): Whether networking is enabled.
        - **`profile`** (string, optional): The name of the network policy profile to apply.
    - **`tpm`** (object, optional): TPM configuration.
        - **`enabled`** (boolean, required): Whether a vTPM is enabled.
- **`policy_profile`** (string, required): The name of the policy profile to apply to this VM, as defined in `rayos-policy.toml`.

---

## 4) Registry Management

### 4.1 Atomicity

To prevent corruption, updates to `registry.json` must be atomic. This can be achieved using a "write-and-rename" pattern:

1.  Read the existing `registry.json`.
2.  Modify the registry in memory.
3.  Write the new registry to a temporary file (e.g., `registry.json.tmp`).
4.  Atomically rename the temporary file to `registry.json`.

### 4.2 Concurrency

The VM Supervisor is the primary owner and writer of the registry. Other services should treat it as read-only. If other services need to request changes, they should do so through a well-defined API provided by the VM Supervisor.

---

## 5) Related Documents

- [DISK_LAYOUT_AND_PERSISTENCE.md](DISK_LAYOUT_AND_PERSISTENCE.md)
- [LINUX_SUBSYSTEM_DESIGN.md](LINUX_SUBSYSTEM_DESIGN.md)
- [WINDOWS_SUBSYSTEM_DESIGN.md](WINDOWS_SUBSYSTEM_DESIGN.md)
- [POLICY_CONFIGURATION_SCHEMA.md](POLICY_CONFIGURATION_SCHEMA.md)
