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

## Documentation

Start here:
- `docs/QUICKSTART.md`
- `docs/BUILD_GUIDE.md`
- `docs/BOOT_TROUBLESHOOTING.md`
- `docs/SYSTEM_ARCHITECTURE.md`

## Contributing

Issues and PRs are welcome. If you’re changing project structure or scripts, please ensure:

- `./scripts/build-iso.sh` still succeeds
- `./scripts/test-boot-headless.sh` still passes
- Artifacts remain under `build/`
