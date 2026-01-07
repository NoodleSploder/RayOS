#!/bin/bash
set -e

# Create RayOS system image for installation
# This script packages the kernel and essential files into a format
# that the installer can copy to the target system partition

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$REPO_ROOT/build"
SYSTEM_IMAGE_DIR="$BUILD_DIR/rayos-system-image"

echo "=== Building RayOS System Image ==="
echo

# Clean previous build
rm -rf "$SYSTEM_IMAGE_DIR"
mkdir -p "$SYSTEM_IMAGE_DIR"

echo "[1] Copying kernel..."
cp "$BUILD_DIR/boot-fat/EFI/RAYOS/kernel.bin" "$SYSTEM_IMAGE_DIR/kernel.bin"
echo "  ✓ Kernel copied"

echo "[2] Copying initrd..."
mkdir -p "$SYSTEM_IMAGE_DIR/boot"
if [ -f "$BUILD_DIR/boot-fat/EFI/RAYOS/linux/initrd" ]; then
  cp "$BUILD_DIR/boot-fat/EFI/RAYOS/linux/initrd" "$SYSTEM_IMAGE_DIR/boot/initrd"
  echo "  ✓ Initrd copied"
else
  echo "  ⚠ Initrd not found, continuing without it"
fi

echo "[3] Copying bootloader..."
if [ -f "$BUILD_DIR/boot-fat/EFI/BOOT/BOOTX64.EFI" ]; then
  mkdir -p "$SYSTEM_IMAGE_DIR/EFI/BOOT"
  cp "$BUILD_DIR/boot-fat/EFI/BOOT/BOOTX64.EFI" "$SYSTEM_IMAGE_DIR/EFI/BOOT/BOOTX64.EFI"
  echo "  ✓ Bootloader copied"
else
  echo "  ⚠ Bootloader not found"
fi

echo "[4] Creating system metadata..."
cat > "$SYSTEM_IMAGE_DIR/VERSION" << EOF
RayOS System Image
Build Date: $(date -u)
Kernel: kernel.bin
Initrd: boot/initrd
EOF
echo "  ✓ Metadata created"

echo "[5] Creating install manifest..."
cat > "$SYSTEM_IMAGE_DIR/MANIFEST" << EOF
# RayOS System Installation Manifest
# This file lists all files to be copied during installation

kernel.bin                  /boot/kernel.bin
boot/initrd                 /boot/initrd
EFI/BOOT/BOOTX64.EFI        /EFI/BOOT/BOOTX64.EFI
EOF
echo "  ✓ Manifest created"

echo "[6] Calculating checksums..."
find "$SYSTEM_IMAGE_DIR" -type f ! -name "MANIFEST" ! -name "VERSION" -exec sh -c '
  for file; do
    if command -v sha256sum &> /dev/null; then
      sha256sum "$file"
    else
      md5sum "$file"
    fi
  done
' _ {} + > "$SYSTEM_IMAGE_DIR/.checksums"
echo "  ✓ Checksums calculated"

# Create tarball
echo "[7] Creating system image tarball..."
TARBALL="$BUILD_DIR/rayos-system-image.tar.gz"
tar -czf "$TARBALL" -C "$BUILD_DIR" rayos-system-image/
echo "  ✓ Tarball created: $TARBALL"

# Show summary
echo
echo "=== System Image Ready ==="
echo "Location: $SYSTEM_IMAGE_DIR"
echo "Tarball: $TARBALL"
echo "Size: $(du -sh "$SYSTEM_IMAGE_DIR" | cut -f1)"
echo
du -sh "$SYSTEM_IMAGE_DIR"/* 2>/dev/null | sed 's/^/  /'
