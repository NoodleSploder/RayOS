# RayOS Policy Configuration Schema

Status: **draft (concrete v0 schema)**

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

## 2) Format Choice (v0)

RayOS policy is **TOML** for v0:

- Human-editable, commentable, and stable enough for installers and recovery workflows.
- Strict typing (numbers vs strings) while staying ergonomic.
- Easily parsed on host tooling (bring-up) and later in-OS.

File location (v0): `RAYOS_DATA/policy/rayos-policy.toml` (see `DISK_LAYOUT_AND_PERSISTENCE.md`).

---

## 3) Invariants (must hold)

- “Presented” gates input routing.
- Default should be secure: networking off unless enabled.
- Host tooling must refuse to violate policy silently; violations emit `RAYOS_POLICY_VIOLATION:<rule>:<detail>` markers in logs.

---

## 4) v0 Schema (TOML)

This is the minimal schema needed to enforce the Option D authority model during bring-up.

```toml
version = 0

[defaults]
# Secure by default.
networking = "off"          # off | on
present_on_boot = false     # presented UI surface at boot
autoboot_vms = true         # start/resume VMs during RayOS boot (hidden)

[limits]
cpu_percent = 80            # 1..100
mem_mib = 12288             # overall cap for guest memory

[logging]
level = "info"              # error | warn | info | debug | trace
persist = true

# ---- Linux subsystem policy ----
[linux]
enabled = true
vm_id = "linux-desktop-001"
autoboot = true
present_on_boot = false
networking = "off"          # off | on | provisioning_only

[linux.limits]
cpus = 2
mem_mib = 4096

[linux.storage]
root_disk = "linux-guest/desktop/desktop-rootfs.ext4"
# Future: virtiofs mounts, volumes, read-only host shares.

[linux.devices]
input = true
gpu = "virtio"              # virtio | none

# ---- Windows subsystem policy ----
[windows]
enabled = false
vm_id = "windows-desktop-001"
autoboot = false
present_on_boot = false
networking = "off"

[windows.limits]
cpus = 4
mem_mib = 8192

[windows.storage]
disk = "windows-guest/windows-desktop-001/windows.qcow2"
ovmf_vars = "windows-guest/windows-desktop-001/OVMF_VARS.fd"
vtpm_state_dir = "windows-guest/windows-desktop-001/tpm"

[windows.devices]
input = true
gpu = "virtio"
vtpm = true
```

### Semantics

- `autoboot_*`: starts/resumes VMs in the background. Presentation and input are still gated.
- `present_on_boot`: if true, RayOS is allowed to show the desktop surface without an explicit user request (still subject to policy + “present gating”).
- `networking`:
  - `off`: no network device attached.
  - `on`: attach user-mode NAT (dev harness) or real NIC (future), subject to explicit policy.
  - `provisioning_only`: network allowed only for first-time provisioning; must log a marker and revert to `off` afterward.

### Validation rules (v0)

- `version` must be `0`.
- Unknown keys are rejected (fail closed).
- Any `*_disk` path must be relative to the RayOS data root and must not escape it (`..` forbidden).
- If `windows.enabled=true`, then `windows.devices.vtpm=true` is required for Windows 11.

---

## 5) Update + Signing (deferred)

v0 assumes a local, unsigned policy file (developer + installer workflows). Future:

- Signed policy bundles for managed deployments.
- Policy audit log and rollback hooks (see `UPDATE_AND_RECOVERY_STRATEGY.md`).

---

## 6) Related

- LINUX_SUBSYSTEM_CONTRACT.md
- WINDOWS_SUBSYSTEM_CONTRACT.md
- DISK_LAYOUT_AND_PERSISTENCE.md
