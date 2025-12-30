# RayOS Security & Threat Model

Status: **draft / tracking stub**

Purpose: define the threat model and security invariants for RayOS as an installable host OS that runs Linux/Windows as managed VMs.

---

## 1) Trust Boundaries

- RayOS host: trusted computing base
- Guests (Linux/Windows): untrusted workloads confined by virtualization + policy
- Optional network services: treat as untrusted unless explicitly authenticated

---

## 2) Invariants (baseline)

- Guests never access raw host devices outside policy.
- Input routing is gated by “presented” state.
- Networking default off for guests unless enabled by policy.
- Persistence is auditable (clear logs for start/resume/suspend/shutdown).

---

## 3) Topics to Decide

- Secure Boot / measured boot posture
- Key management (signing keys, policy keys)
- Logging/auditing scope and retention
- Supply chain for dependencies

---

## 4) Related

- LINUX_SUBSYSTEM_CONTRACT.md
- WINDOWS_SUBSYSTEM_CONTRACT.md
- UPDATE_AND_RECOVERY_STRATEGY.md
