#!/bin/bash
# Create proper UEFI-bootable RayOS ISO with xorriso

set -e

REPO_ROOT="/home/noodlesploder/repos/RayOS"
BUILD_DIR="$REPO_ROOT/build"
KERNEL_DIR="$REPO_ROOT/crates/kernel-bare"

echo "Building Phase 4 kernel boot ISO..."

# Step 1: Build kernel
echo "  [1/5] Building kernel..."
cd "$KERNEL_DIR"
cargo +nightly build --release --target x86_64-rayos-kernel.json \
  -Zbuild-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem > /dev/null 2>&1

# Step 2: Extract raw binary  
echo "  [2/5] Extracting binary..."
objcopy -O binary "$KERNEL_DIR/target/x86_64-rayos-kernel/release/kernel-bare" "$BUILD_DIR/kernel.bin"

# Step 3: Get bootloader
echo "  [3/5] Locating bootloader..."
BOOTLOADER="$REPO_ROOT/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
if [ ! -f "$BOOTLOADER" ]; then
    BOOTLOADER="$REPO_ROOT/crates/bootloader/target/x86_64-unknown-uefi/debug/uefi_boot.efi"
fi
if [ ! -f "$BOOTLOADER" ]; then
    echo "    ✗ Bootloader not found!"
    exit 1
fi
BOOT_SIZE=$(ls -lh "$BOOTLOADER" | awk '{print $5}')

# Step 4: Create ISO layout
echo "  [4/5] Laying out ISO..."
TMPISO="/tmp/rayos_iso_$$"
rm -rf "$TMPISO"
mkdir -p "$TMPISO/EFI/BOOT" "$TMPISO/EFI/RAYOS"

# Copy files
cp "$BOOTLOADER" "$TMPISO/EFI/BOOT/BOOTX64.EFI"
cp "$BUILD_DIR/kernel.bin" "$TMPISO/EFI/RAYOS/kernel.bin"

# Create registry
cat > "$TMPISO/EFI/RAYOS/registry.json" << 'EOFREGISTRY'
{"boot_mode":"kernel","installer_mode":false,"kernel_binary_path":"EFI/RAYOS/kernel.bin"}
EOFREGISTRY

# Create the ISO with proper UEFI support
echo "  [5/5] Creating ISO..."
xorriso -as mkisofs \
    -o "$BUILD_DIR/rayos-kernel-p4.iso" \
    -R -J \
    -V "RayOS-Kernel-P4" \
    -e EFI/BOOT/BOOTX64.EFI \
    -no-emul-boot \
    "$TMPISO" 2>&1 | grep -v "^$" || true

# Cleanup
rm -rf "$TMPISO"

# Check result
if [ -f "$BUILD_DIR/rayos-kernel-p4.iso" ]; then
    ISO_SIZE=$(ls -lh "$BUILD_DIR/rayos-kernel-p4.iso" | awk '{print $5}')
    echo ""
    echo "✓ Build complete!"
    echo "  ISO: $BUILD_DIR/rayos-kernel-p4.iso ($ISO_SIZE)"
    echo "  Kernel: 191K"
    echo "  Bootloader: $BOOT_SIZE"
    echo ""
    echo "To boot:"
    echo "  qemu-system-x86_64 -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on -drive if=pflash,format=raw,unit=1,file=/tmp/OVMF_VARS.fd -cdrom build/rayos-kernel-p4.iso -m 2G -serial file:serial.log -display none"
else
    echo "✗ Build failed!"
    exit 1
fi
