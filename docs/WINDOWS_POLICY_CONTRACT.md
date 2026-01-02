# RayOS Windows Policy Contract

Status: **draft (v0 spec)**

Purpose: Define the policy and security contract for the Windows subsystem, ensuring that it runs as a managed guest under RayOS's authority.

---

## 1) Core Principle: Host Authority

RayOS is the host and has ultimate authority over the Windows guest. The Windows subsystem is not a peer; it is a compatibility projection. RayOS controls the entire lifecycle, resource allocation, and security posture of the Windows VM.

---

## 2) Policy Enforcement Points

Policies for the Windows subsystem are defined in the main RayOS policy file (`/var/rayos/policy/rayos-policy.toml`) and are enforced by the VM Supervisor.

### 2.1 Networking

-   **Default:** Networking for the Windows VM is **OFF** by default.
-   **Explicit Enablement:** Networking must be explicitly enabled in the VM's policy profile.
-   **Network Profiles:** Different network profiles (e.g., "restricted", "unrestricted") can be defined to apply different firewall rules or network access controls.

### 2.2 Storage

-   **Storage Boundary:** The Windows VM is confined to its virtual disk image(s). There is no accidental or implicit access to the host filesystem.
-   **Explicit Mounts:** Any additional storage (e.g., shared folders) must be explicitly configured as a `virtio-fs` or similar device in the VM's registry record and allowed by policy.

### 2.3 Resource Caps

-   **CPU and Memory:** The number of vCPUs and the amount of memory allocated to the Windows VM are defined in the VM's registry record. These values are subject to global RayOS policy to prevent resource exhaustion.
-   **Policy Overrides:** The RayOS policy can override the registry values to enforce stricter limits.

### 2.4 Snapshot and State Management

-   **Host-owned State:** The state of the Windows VM (snapshots, saved states) is managed by the RayOS host. The guest does not have direct access to these state files.
-   **Snapshot Policy:** Policy can define how often snapshots are taken, how many are retained, and whether they can be initiated by the guest. For v0, all snapshot operations are host-initiated.

---

## 3) Security Invariants

-   **Guest is Untrusted:** The Windows guest environment is considered untrusted.
-   **Input Gating:** Input is only routed to the Windows VM when it is the actively "presented" application.
-   **Device Passthrough:** No hardware devices are passed through to the Windows VM by default. All device access is mediated by virtual devices (virtio) and governed by policy.

---

## 4) Related Documents

-   [WINDOWS_SUBSYSTEM_DESIGN.md](WINDOWS_SUBSYSTEM_DESIGN.md)
-   [POLICY_CONFIGURATION_SCHEMA.md](POLICY_CONFIGURATION_SCHEMA.md)
-   [VM_REGISTRY_SPEC.md](VM_REGISTRY_SPEC.md)
-   [SECURITY_THREAT_MODEL.md](SECURITY_THREAT_MODEL.md)
