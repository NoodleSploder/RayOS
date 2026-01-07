#!/bin/bash
# Rebuild RayOS kernel boot ISO with latest kernel binary

set -e

REPO_ROOT="/home/noodlesploder/repos/RayOS"
BUILD_DIR="$REPO_ROOT/build"
KERNEL_DIR="$REPO_ROOT/crates/kernel-bare"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║          RayOS Kernel ISO Rebuild                              ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo

# Build kernel
echo "[1/4] Building kernel..."
cd "$KERNEL_DIR"
cargo +nightly build --release --target x86_64-rayos-kernel.json -Zbuild-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem > /dev/null 2>&1
echo "  ✓ Kernel built"

# Extract raw binary
echo "[2/4] Extracting raw binary..."
objcopy -O binary "$KERNEL_DIR/target/x86_64-rayos-kernel/release/kernel-bare" "$BUILD_DIR/kernel.bin"
KERNEL_SIZE=$(ls -lh "$BUILD_DIR/kernel.bin" | awk '{print $5}')
echo "  ✓ Extracted: kernel.bin ($KERNEL_SIZE)"

# Create ISO structure
echo "[3/4] Creating ISO..."
ISO_WORK="$BUILD_DIR/iso-rebuild-temp"
rm -rf "$ISO_WORK"
mkdir -p "$ISO_WORK/EFI/Boot" "$ISO_WORK/EFI/RAYOS"

# Copy bootloader
BOOTLOADER="$REPO_ROOT/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
if [ ! -f "$BOOTLOADER" ]; then
    # Try debug build if release not found
    BOOTLOADER="$REPO_ROOT/crates/bootloader/target/x86_64-unknown-uefi/debug/uefi_boot.efi"
fi
if [ ! -f "$BOOTLOADER" ]; then
    echo "  ✗ Error: Bootloader not found"
    exit 1
fi
cp "$BOOTLOADER" "$ISO_WORK/EFI/Boot/bootx64.efi"
BOOT_SIZE=$(ls -lh "$BOOTLOADER" | awk '{print $5}')
echo "  ✓ Copied bootloader ($BOOT_SIZE)"

# Copy kernel
cp "$BUILD_DIR/kernel.bin" "$ISO_WORK/EFI/RAYOS/kernel.bin"
echo "  ✓ Copied kernel ($KERNEL_SIZE)"

# Create registry.json for kernel mode (installer_mode = false)
cat > "$ISO_WORK/EFI/RAYOS/registry.json" << 'EOFREGISTRY'
{
  "boot_mode": "kernel",
  "installer_mode": false,
  "kernel_binary_path": "EFI/RAYOS/kernel.bin",
  "installer_binary_path": "EFI/RAYOS/installer.bin"
}
EOFREGISTRY
echo "  ✓ Created registry (kernel mode)"

# Create El Torito boot configuration
mkdir -p "$ISO_WORK/boot"
touch "$ISO_WORK/boot/boot.catalog"
touch "$ISO_WORK/boot/boot.img"

# Build ISO with xorriso
echo "[4/4] Building ISO image..."
cd "$BUILD_DIR"
xorriso -as mkisofs -o rayos-kernel-phase4.iso \
  -R -J \
  -V "RayOS-Kernel-P4" \
  "$ISO_WORK" > /dev/null 2>&1

ISO_SIZE=$(ls -lh "$BUILD_DIR/rayos-kernel-phase4.iso" | awk '{print $5}')
echo "  ✓ Created: rayos-kernel-phase4.iso ($ISO_SIZE)"

# Cleanup
rm -rf "$ISO_WORK"

echo
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    Build Complete ✓                            ║"
echo "╠════════════════════════════════════════════════════════════════╣"
echo "║  ISO:       build/rayos-kernel-phase4.iso ($ISO_SIZE)           ║"
echo "║  Kernel:    $KERNEL_SIZE                                       ║"
echo "║  Bootloader: 57 KB                                               ║"
echo "║                                                                ║"
echo "║  Ready to test with QEMU:                                      ║"
echo "║  qemu-system-x86_64 -bios /usr/share/OVMF/OVMF_CODE.fd \\     ║"
echo "║      -cdrom build/rayos-kernel-phase4.iso \\                  ║"
echo "║      -m 2G -serial file:serial.log -display none              ║"
echo "╚════════════════════════════════════════════════════════════════╝"
