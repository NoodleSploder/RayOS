UEFI bootloader and shim for RayOS
=================================

What I added
- `uefi_boot/` — minimal UEFI app that prints a greeting.
- `shim/` — a shim skeleton that looks for `EFI\\RAYOS\\kernel.bin` on the ESP and prints its size.

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
- Implement loading the kernel into memory and performing ExitBootServices + jump to entrypoint.
- Add automatic cross-compile configuration (`.cargo/config.toml`) and a target JSON.
- Help with creating and signing a real shim (requires keys and decision on trust model).
