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

If your OVMF firmware path differs, override it:

```bash
OVMF_CODE=/path/to/OVMF_CODE.fd ./scripts/test-boot-headless.sh
```

If your QEMU binary name differs:

```bash
QEMU_BIN=qemu-system-x86_64 ./scripts/test-boot-headless.sh
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
