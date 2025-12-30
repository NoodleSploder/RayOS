UEFI bootloader and shim for RayOS
=================================

What it does
- `uefi_boot/` is the RayOS UEFI boot application.
	- Prints early logs
	- Initializes framebuffer (GOP when available)
	- Builds a `BootInfo`
	- Loads and transfers control to a kernel entry

Kernel loading modes
- **aarch64 embedded fallback**: if `EFI\\RAYOS\\kernel.bin` is missing/invalid, the bootloader enters a post-`ExitBootServices` embedded runtime (UART + simple loop).
- **aarch64 ELF load+jump**: when `EFI\\RAYOS\\kernel.bin` is an ELF, the bootloader loads PT_LOAD segments (reserving pages as `LOADER_CODE` pre-exit to avoid AAVMF NX faults) and jumps to the ELF entry.
- **x86_64 headless tests**: the repo’s smoke tests boot `kernel-bare` from `EFI\\RAYOS\\kernel.bin` under OVMF.

Build notes
- These are UEFI apps (PE/COFF `.efi` binaries). Building requires a suitable UEFI target and a linker that produces PE/COFF.
- Typical approach:

1. Install the Rust toolchain and add a UEFI target (you may need a custom target JSON):

```bash
# Example (may need adjustment):
rustup target add x86_64-unknown-uefi
```

2. Build the crate you want:

```bash
cd uefi_boot
cargo build --release --target x86_64-unknown-uefi
```

3. The produced `.efi` will be in `target/x86_64-unknown-uefi/release/uefi_boot` (rename to `.efi` if necessary) and can be copied to the EFI System Partition under `EFI/RAYOS/`.

Signing / Secure Boot shim notes
- A production "shim" used for Secure Boot requires signing with a key trusted by the platform (e.g., Microsoft or your own enrolled key). Building and signing a shim is a non-trivial process involving creating or using a signing key, embedding a small loader, and possibly interacting with sbctl/sbkeys or Microsoft's WHQL.
- The `shim/` crate added here is a functional skeleton that demonstrates locating and opening a kernel file. It does not perform any signing, verification, or machine-specific key handling.

Next steps you may want me to take

Verification (Linux)

From repo root:

```bash
# aarch64 embedded fallback (forces missing kernel.bin)
./scripts/test-boot-aarch64-headless.sh

# aarch64 ELF load+jump to kernel-aarch64-bare
./scripts/test-boot-aarch64-kernel-headless.sh

# x86_64 kernel-bare under OVMF
./scripts/test-boot-headless.sh
```
