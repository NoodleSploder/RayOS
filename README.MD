# RayOS - Operating System Project

**Status: Phase 3 - Boot Testing Infrastructure Complete**  
**Last Update: January 7, 2026**  
**Code Quality: 0 compilation errors, 6/6 tests passing**

---

## 🚀 Latest: Phase 3 Boot Testing Infrastructure

Phase 3 is now complete with comprehensive boot testing capabilities:

- **Boot Media:** Two test ISOs created
  - `build/rayos-kernel-test.iso` (4.0 MB) - Direct kernel boot
  - `build/rayos-installer-test.iso` (9.3 MB) - Installer chainloading
  
- **Testing Tools:** QEMU automation and hardware procedures
  - `scripts/test-qemu-kernel-boot.sh` - Automated QEMU testing
  - `PHASE_3_BOOT_TESTING_GUIDE.md` - 500+ line testing guide

- **Documentation:** 2,386 lines across 8 comprehensive files
  - **Start Here:** [DOCUMENTATION_INDEX.md](DOCUMENTATION_INDEX.md)
  - **Quick Start:** [PHASE_3_BOOT_MEDIA_README.md](PHASE_3_BOOT_MEDIA_README.md)
  - **Full Status:** [PROJECT_STATUS_JAN_07_2026.md](PROJECT_STATUS_JAN_07_2026.md)

**Ready for:** QEMU boot testing, UEFI hardware validation, installer verification

---

# RayOS

RayOS is an experimental, Rust-based, UEFI-bootable operating system project focused on a “bicameral” kernel architecture:

- **System 1 (Reflex Engine)**: low-latency, real-time processing
- **System 2 (Cognitive Engine)**: higher-level reasoning / orchestration

The repo includes a UEFI bootloader, bare-metal kernels (x86_64 + aarch64), and host-side tooling to build images and boot them in QEMU.

> RayOS is a research/prototype OS. Expect breaking changes.

## Repository layout

- `crates/` — Rust crates (bootloader, kernels, host tools)
- `scripts/` — build + test + QEMU launcher scripts
- `docs/` — design docs, architecture notes, boot troubleshooting
- `build/` — generated artifacts (ignored by `.gitignore`)

## Prerequisites

### Linux (recommended)

- Rust via `rustup`
- QEMU + UEFI firmware (OVMF)
- ISO/FAT tooling used by the build scripts

On Debian/Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y \
  qemu-system-x86 ovmf \
  xorriso dosfstools mtools \
  python3
```

Notes:
- The ISO builder uses `mkfs.vfat` (from `dosfstools`).
- If `mtools` isn’t installed, the ISO builder can fall back to loop-mounting a FAT image and may prompt for `sudo`.
- The bootloader build is pinned to a specific nightly toolchain; the scripts will attempt to install it automatically.

### Windows

Windows is supported primarily via **WSL2** (recommended). You can run the Linux scripts from WSL and use QEMU/OVMF inside WSL.
PowerShell helper scripts are also available under `scripts/`.

## Quick start (build + boot)

### 1) Build an ISO (and USB image)

```bash
./scripts/build-iso.sh
```

Artifacts are written to `build/`, typically including:
- `build/rayos.iso`
- `build/rayos-universal-usb.img`

To build a specific architecture:

```bash
./scripts/build-iso.sh --arch x86_64
./scripts/build-iso.sh --arch aarch64
./scripts/build-iso.sh --arch both
./scripts/build-iso.sh --arch universal
```

### 2) Boot in QEMU (fastest checks)

Headless boot marker check (fast):

```bash
./scripts/test-boot-headless.sh
```

Graphical boot (interactive):

```bash
./scripts/test-boot.sh
```

### Linux desktop (dev harness)

The `show linux desktop` / `hide linux desktop` flow is currently implemented via the **host-side bridge** inside `./scripts/test-boot.sh`.

- With defaults, the bridge is **off**, so `show linux desktop` will not present anything.
- To prelaunch a Linux desktop VM hidden at boot and then present it instantly on demand (no new VM spawn), run:

```bash
ENABLE_HOST_DESKTOP_BRIDGE=1 PRELAUNCH_HIDDEN_DESKTOPS=1 ./scripts/test-boot.sh
```

Presentation uses a VNC client (currently `gvncviewer`, often provided by the `virt-viewer` package).

If your OVMF firmware path differs, override it:

```bash
OVMF_CODE=/path/to/OVMF_CODE.fd ./scripts/test-boot-headless.sh
```

If your QEMU binary name differs:

```bash
QEMU_BIN=qemu-system-x86_64 ./scripts/test-boot-headless.sh
```

### RayOS-native guest presentation (dev_scanout)

RayOS is being brought up to present guest desktops *natively* (guest scanout pixels blitted into the RayOS framebuffer UI), without relying on an external host VNC client.

To validate the presentation pipeline before a real virtio-gpu/VMM scanout producer exists, there is a **dev-only synthetic scanout** producer feature in the kernel.

Headless validation (checks for serial markers):

```bash
./scripts/test-dev-scanout-headless.sh
```

Graphical demo (opens a QEMU window and injects `show linux desktop` automatically):

```bash
./scripts/test-dev-scanout.sh
```

While the guest panel is presented, press ` (backtick) to toggle hide/show.

Optional: auto-hide after N seconds (compile-time env var used only for `dev_scanout` builds):

```bash
RAYOS_DEV_SCANOUT_AUTOHIDE_SECS=10 ./scripts/test-dev-scanout.sh
```

If you need a different QEMU UI backend, override `QEMU_DISPLAY`:

```bash
QEMU_DISPLAY=sdl ./scripts/test-dev-scanout.sh
```

### 3) Run the broader boot verification

```bash
./scripts/verify-boot.sh
```

## Useful scripts

- `./scripts/build-iso.sh` — build UEFI bootloader + kernel and produce bootable images in `build/`
- `./scripts/test-boot-headless.sh` — quickest “did it boot?” smoke test
- `./scripts/test-boot.sh` — interactive boot
- `./scripts/test-volume-persistence.sh` — validates Volume persistence using a QEMU disk

## Hypervisor / In-OS VMM (experimental)

RayOS includes an experimental in-kernel VMM (VMX-first on x86_64) so we can run a headless Linux guest inside RayOS without relying on an external QEMU instance for production-like validation. This work is intentionally incremental; see `docs/RAYOS_TODO.md` for the current P0–P* roadmap/status (including the Linux guest persistence smokes).

Key implemented pieces (high level):

- VMX bring-up and VMCS setup with EPT/GPA ↔ HPA mapping for guest memory.
- Guest VM entry injection of virtio IRQs via the VMCS `VM_ENTRY_INTERRUPTION_INFO` field.
- IRQ delivery fallbacks: LAPIC MMIO write fallback and MSI-style APIC write fallback when VM-entry injection fails (test hooks exercise these paths).
- Bounded retry + exponential backoff for pending interrupt injection, plus a boot-time selftest and metrics counters.
- Virtio-MMIO plumbing: descriptor chain logging, avail/used handling, queue-notify, and basic virtio-blk handler (in-memory backing).
- Virtio-gpu: control-path stub + selftest that publishes scanout/presentation markers (`RAYOS_LINUX_DESKTOP_PRESENTED`, `RAYOS_LINUX_DESKTOP_FIRST_FRAME`).
- Virtio-console: scaffolded device model + basic dispatch and host logging (markers: `RAYOS_VMM:VIRTIO_CONSOLE:*`); queue handling & I/O remain to be implemented.

How to exercise (dev / smoke tests):

- Hypervisor boot smoke test (headless):

```bash
# Run the hypervisor boot smoke test (headless) which validates VMX bring-up and device selftests
RAYOS_KERNEL_FEATURES="vmm_hypervisor,vmm_hypervisor_smoke" ./scripts/test-vmm-hypervisor-boot.sh

# Add device features / forced-failure tests:
# - virtio-gpu selftest: add vmm_virtio_gpu
# - virtio-console presence: add vmm_virtio_console
# - force VMWRITE injection failures: add vmm_inject_force_fail
# - force MSI fallback: add vmm_inject_force_msi_fail
# - exercise backoff selftest: add vmm_inject_force_all_fail,vmm_inject_backoff_selftest
```

- Linux guest smokes (headless):

```bash
# Boot the Linux guest and wait for the guest agent ready marker
./scripts/test-vmm-linux-guest-headless.sh

# Persist-marker reset smoke: boot → write marker → QEMU monitor `system_reset` → boot → assert marker present
./scripts/test-vmm-linux-persist-marker-reset-headless.sh

# Virtio-blk persistence smoke using the VMM's persistent backing
./scripts/test-vmm-virtio-blk-persist-reset-headless.sh
```

Headless harness note:

- Most headless smokes prefer to stop QEMU by sending `quit` to the QEMU monitor socket once a PASS/SKIP marker appears, rather than relying on `timeout` to kill QEMU.
- Some smokes use `system_reset` to simulate a RayOS reboot within the same QEMU process.
- Shared helper used by multiple scripts: `scripts/lib/headless_qemu.sh`.

Current work & next steps:

- Convert the backoff boot selftest to a stable host-runnable unit test (blocked by a Cargo lockfile / toolchain mismatch; issue tracked in `docs/RAYOS_TODO.md`).
- Implement virtio-console queue parsing/control/data handling and end-to-end guest console testcases.
- Add persistent virtio-blk backing and a headless Linux boot test that asserts a deterministic guest-ready marker.

See `docs/RAYOS_TODO.md` for the hypervisor roadmap and more details.

## Documentation

Start here:
- `docs/README.md`
- `docs/RAYOS_TODO.md`
- `docs/QUICKSTART.md` (historical; some sections are out of date)
- `docs/BUILD_GUIDE.md`
- `docs/BOOT_TROUBLESHOOTING.md`
- `docs/SYSTEM_ARCHITECTURE.md`

## Contributing

Issues and PRs are welcome. If you’re changing project structure or scripts, please ensure:

- `./scripts/build-iso.sh` still succeeds
- `./scripts/test-boot-headless.sh` still passes
- Artifacts remain under `build/`
