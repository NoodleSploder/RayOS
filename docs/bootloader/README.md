# RayOS Bootloader

This directory contains the RayOS UEFI bootloader workspace.

## What it does
- Builds `uefi_boot` as a UEFI application (`.efi`) for **x86_64** and **aarch64**.
- Brings up console + GOP framebuffer, builds a `BootInfo`, then loads/jumps to a kernel entry.
- For Linux/QEMU smoke tests, it typically boots `kernel-bare` from `EFI/RAYOS/kernel.bin`.

Key entrypoint:
- `uefi_boot/src/main.rs`

## Toolchain and targets
- Toolchain is pinned in `rust-toolchain.toml` (currently `nightly-2024-11-01`).
- Targets:
	- `x86_64-unknown-uefi`
	- `aarch64-unknown-uefi`

## Build (from repo root)
Build x86_64 UEFI bootloader:
```bash
cd bootloader
cargo build -p uefi_boot --release --target x86_64-unknown-uefi
```

Build aarch64 UEFI bootloader:
```bash
cd bootloader
cargo build -p uefi_boot --release --target aarch64-unknown-uefi
```

Artifacts:
- `bootloader/target/<target>/release/uefi_boot.efi`

## Canonical end-to-end build
Use the repo scripts instead of hand-assembling images:
- `./scripts/build-iso.sh --arch universal`

## Notes / contracts
- Headless tests expect the boot marker `RayOS uefi_boot: start` on serial.
- If changing boot output or the `BootInfo` layout, update the corresponding consumer(s) and scripts.

