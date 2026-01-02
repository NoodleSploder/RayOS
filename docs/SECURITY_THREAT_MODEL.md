# RayOS Security & Threat Model

Status: **draft (concrete v0 threat model)**

Purpose: define the threat model and security invariants for RayOS as an installable host OS that runs Linux/Windows as managed VMs.

---

## 1) Security Goals (v0)

- **Contain untrusted workloads** inside Linux/Windows guests.
- Ensure **policy is authoritative** (fail closed).
- Ensure **presentation gates input**: when not presented, guests should not receive user input.
- Default to **least privilege**: guest networking off, minimal device exposure.
- Provide **auditability**: lifecycle actions, policy violations, and sensitive operations emit stable markers to persistent logs.

Non-goals (v0):
- Perfect secrecy against a fully compromised host kernel.
- Protecting a user from themselves (developer mode can disable guardrails).
- Full supply-chain attestation (document posture and hooks; implementation may be incremental).

---

## 2) Trust Boundaries & Assets

- RayOS host: trusted computing base
- Guests (Linux/Windows): untrusted workloads confined by virtualization + policy
- Optional network services: treat as untrusted unless explicitly authenticated

Primary assets:
- integrity of the host (VM supervisor, policy engine, storage)
- confidentiality/integrity of RayOS Data (`/var/rayos`)
- correctness of policy enforcement (networking/device exposure/presentation gating)
- integrity of update/rollback pointers and boot entries

---

## 3) Invariants (baseline)

- Guests never access raw host devices outside policy.
- Input routing is gated by “presented” state.
- Networking default off for guests unless enabled by policy.
- Persistence is auditable (clear logs for start/resume/suspend/shutdown).

---

## 4) Attacker Model (v0)

Consider these threat classes:

- **Malicious guest software**: malware inside Linux/Windows trying to escape VM or exfiltrate data.
- **Network attacker**: if networking is enabled, remote attacker attempts to compromise guest and pivot.
- **Local attacker**: physical access to machine; attempts to boot alternate media, modify ESP, or read disks.
- **Supply chain**: compromised build artifacts, unsigned updates, or poisoned policy files.
- **Event spoofing (dev harness)**: in developer mode, a compromised host environment could inject fake markers into logs.

Assumptions (v0):
- Hardware virtualization is available for meaningful containment; without it, RayOS should degrade safely (policy disables guests or runs them with reduced expectations).
- The developer harness (`scripts/test-boot.sh`) is **not** a security boundary; it exists for bring-up/CI.

---

## 5) Attack Surfaces (v0)

- Virtualization boundary (QEMU/KVM, virtio devices, emulated devices).
- Storage boundary (VM disk images, virtiofs mounts, host file permissions).
- Networking boundary (virtio-net user-mode NAT today; real NIC in future).
- Policy loading/parsing (policy file integrity and validation).
- Update/boot chain (ESP contents, boot entries, release pointers).
- Observability surfaces (logs and markers must not be treated as authenticated signals).

---

## 6) Mitigations & Controls (v0)

### 6.1 Device exposure

- Prefer virtio devices with explicit allowlists.
- No “host passthrough” (USB/GPU) in v0 by default.
- Any additional device exposure must be policy-gated and logged.

### 6.2 Presentation gating

- Guests may run hidden (background), but:
  - input injection is disabled unless “presented”
  - presentation transitions are logged:
    - `RAYOS_PRESENTATION:on|off vm=<id> kind=<linux|windows>`

### 6.3 Networking default off

- Default guest networking is off (`POLICY_CONFIGURATION_SCHEMA.md`).
- If networking is enabled for provisioning-only, it must emit a clear marker and revert to off afterward.
- Tests should be able to assert networking enablement via deterministic host markers (see `OBSERVABILITY_AND_RECOVERY.md`).

### 6.4 Storage containment

- VM disks/state live under RayOS Data and are referenced by stable VM IDs (see `DISK_LAYOUT_AND_PERSISTENCE.md`).
- Prevent path traversal / escaping the data root when parsing config/registry (fail closed).
- Avoid implicit host mounts; any virtiofs mount must be explicit and policy-governed.

### 6.5 Policy validation (fail closed)

- Unknown keys rejected.
- Unsupported major version triggers safe mode/recovery.
- Policy violations emit `RAYOS_POLICY_VIOLATION:<rule>:<detail>` and should deny action by default.

### 6.6 Updates & boot chain (posture)

v0 posture:
- unsigned updates are allowed for developer/early adopter workflows
- update and rollback operations must emit stable markers

Forward path:
- signed releases (manifest signatures) and measured boot where available
- Secure Boot integration (shim + signed loader) for managed deployments

---

## 7) Topics to Decide / Future Work

- Secure Boot / measured boot posture
- Key management (signing keys, policy keys)
- Logging/auditing scope and retention
- Supply chain for dependencies

---

## 8) Related

- LINUX_SUBSYSTEM_CONTRACT.md
- WINDOWS_SUBSYSTEM_CONTRACT.md
- UPDATE_AND_RECOVERY_STRATEGY.md
- OBSERVABILITY_AND_RECOVERY.md
- DISK_LAYOUT_AND_PERSISTENCE.md
- POLICY_CONFIGURATION_SCHEMA.md
