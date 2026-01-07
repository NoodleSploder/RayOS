#!/bin/bash
# Create RayOS Boot Media with Custom Registry
# Allows creating ISO/USB with different boot modes

set -e

REPO_ROOT="/home/noodlesploder/repos/RayOS"
BUILD_DIR="$REPO_ROOT/build"

usage() {
    cat << 'EOF'
Usage: create-custom-boot-media.sh [OPTIONS]

Create RayOS boot media with custom registry configuration

Options:
  -m, --mode MODE         Boot mode: kernel (default) or installer
  -o, --output OUTPUT     Output filename (no extension, will add .iso)
  -c, --cdonly            Create ISO only (skip USB image)
  -u, --usbonly           Create USB only (skip ISO)
  -h, --help              Show this help message

Examples:
  # Create installer mode ISO
  ./create-custom-boot-media.sh --mode installer --output rayos-installer-mode

  # Create kernel mode USB (default)
  ./create-custom-boot-media.sh --output rayos-kernel --usbonly

EOF
    exit 1
}

# Defaults
MODE="kernel"
OUTPUT=""
CDONLY=0
USBONLY=0

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -m|--mode)
            MODE="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT="$2"
            shift 2
            ;;
        -c|--cdonly)
            CDONLY=1
            shift
            ;;
        -u|--usbonly)
            USBONLY=1
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

if [ -z "$OUTPUT" ]; then
    echo "Error: Output filename required"
    usage
fi

if [ "$CDONLY" = "1" ] && [ "$USBONLY" = "1" ]; then
    echo "Error: Cannot use both --cdonly and --usbonly"
    exit 1
fi

# Validate mode
if [ "$MODE" != "kernel" ] && [ "$MODE" != "installer" ]; then
    echo "Error: Mode must be 'kernel' or 'installer'"
    exit 1
fi

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║       RayOS Custom Boot Media Creator                          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo
echo "Configuration:"
echo "  Boot Mode: $MODE"
echo "  Output: $OUTPUT"
echo "  Create ISO: $([ $CDONLY -eq 0 ] && [ $USBONLY -eq 0 ] && echo 'Yes' || echo 'No')"
echo "  Create USB: $([ $USBONLY -eq 0 ] && [ $CDONLY -eq 0 ] && echo 'Yes' || echo 'No')"
echo

# Create temporary build directory
WORK_DIR="$BUILD_DIR/custom-media-build-$(date +%s)"
mkdir -p "$WORK_DIR"
cd "$WORK_DIR"

echo "[1/4] Setting up work directory..."
mkdir -p iso-content/EFI/RAYOS
echo "✓ Work directory ready"

# Create registry based on mode
echo "[2/4] Creating registry.json for $MODE mode..."
case $MODE in
    kernel)
        INSTALLER_FLAG="false"
        ;;
    installer)
        INSTALLER_FLAG="true"
        ;;
esac

cat > iso-content/EFI/RAYOS/registry.json << REGISTRY
[
  {
    "installer_mode": $INSTALLER_FLAG,
    "boot_config": "$MODE",
    "created": "$(date -u)",
    "rayos_version": "0.1"
  }
]
REGISTRY

echo "✓ Registry created with installer_mode=$INSTALLER_FLAG"
echo "Registry content:"
cat iso-content/EFI/RAYOS/registry.json | sed 's/^/  /'

echo

# Extract existing ISO content
echo "[3/4] Extracting base ISO content..."
if [ -f "$BUILD_DIR/rayos-installer.iso" ]; then
    # Extract ISO to temp location
    EXTRACT_DIR="$WORK_DIR/iso-extract"
    mkdir -p "$EXTRACT_DIR"
    
    # Use xorriso to extract if available, otherwise inform user
    if command -v xorriso &>/dev/null; then
        xorriso -indev "$BUILD_DIR/rayos-installer.iso" -extract / "$EXTRACT_DIR" 2>/dev/null || true
        cp -r "$EXTRACT_DIR"/* iso-content/ 2>/dev/null || true
        echo "✓ Extracted base ISO content"
    else
        echo "⚠ xorriso not available - using default content only"
        echo "  (For full extraction, install xorriso)"
    fi
else
    echo "⚠ Base ISO not found at $BUILD_DIR/rayos-installer.iso"
    echo "  Building media with registry only"
fi

# Ensure essential files exist
echo "[4/4] Creating custom boot media..."

# Check for required binaries
if [ ! -f "iso-content/EFI/BOOT/BOOTX64.EFI" ]; then
    echo "⚠ Bootloader not found - copying from build..."
    mkdir -p iso-content/EFI/BOOT
    if [ -f "$REPO_ROOT/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi" ]; then
        cp "$REPO_ROOT/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi" \
           iso-content/EFI/BOOT/BOOTX64.EFI
        echo "✓ Bootloader copied"
    fi
fi

if [ ! -f "iso-content/EFI/RAYOS/kernel.bin" ]; then
    echo "⚠ kernel.bin not found - searching..."
    KERNEL=$(find "$BUILD_DIR" -name "kernel.bin" -type f 2>/dev/null | head -1)
    if [ -n "$KERNEL" ]; then
        cp "$KERNEL" iso-content/EFI/RAYOS/kernel.bin
        echo "✓ kernel.bin found and copied"
    fi
fi

if [ ! -f "iso-content/EFI/RAYOS/installer.bin" ]; then
    echo "⚠ installer.bin not found - searching..."
    INSTALLER=$(find "$BUILD_DIR" -name "installer.bin" -type f 2>/dev/null | head -1)
    if [ -n "$INSTALLER" ]; then
        cp "$INSTALLER" iso-content/EFI/RAYOS/installer.bin
        echo "✓ installer.bin found and copied"
    fi
fi

# Create ISO if not usbonly
if [ $USBONLY -eq 0 ]; then
    echo "Creating ISO image..."
    if command -v xorriso &>/dev/null; then
        xorriso -as mkisofs -R -J -L \
            -boot-load-size 4 -boot-info-table \
            -isohybrid-mbr /usr/share/syslinux/isohdpfx.bin \
            -o "$BUILD_DIR/${OUTPUT}.iso" \
            iso-content/ 2>&1 | grep -v "^xorriso" || true
        if [ -f "$BUILD_DIR/${OUTPUT}.iso" ]; then
            SIZE=$(du -h "$BUILD_DIR/${OUTPUT}.iso" | cut -f1)
            echo "✓ ISO created: ${OUTPUT}.iso ($SIZE)"
        fi
    else
        echo "✗ xorriso not available - cannot create ISO"
        echo "  Install with: sudo apt install xorriso"
    fi
fi

# Create USB image if not cdonly
if [ $CDONLY -eq 0 ]; then
    echo "Creating USB image..."
    if [ -f "$BUILD_DIR/${OUTPUT}.iso" ]; then
        # Convert ISO to USB image format
        cp "$BUILD_DIR/${OUTPUT}.iso" "$BUILD_DIR/${OUTPUT}-usb.img"
        SIZE=$(du -h "$BUILD_DIR/${OUTPUT}-usb.img" | cut -f1)
        echo "✓ USB image created: ${OUTPUT}-usb.img ($SIZE)"
    else
        echo "⚠ Skipping USB image (ISO not created)"
    fi
fi

# Summary
echo
echo "═══════════════════════════════════════════════════════════════"
echo "✓ Custom Boot Media Created Successfully"
echo "═══════════════════════════════════════════════════════════════"
echo
echo "Output Files:"
[ $USBONLY -eq 0 ] && [ -f "$BUILD_DIR/${OUTPUT}.iso" ] && echo "  • $BUILD_DIR/${OUTPUT}.iso"
[ $CDONLY -eq 0 ] && [ -f "$BUILD_DIR/${OUTPUT}-usb.img" ] && echo "  • $BUILD_DIR/${OUTPUT}-usb.img"
echo
echo "Boot Configuration: $MODE mode"
echo "Registry: installer_mode=$INSTALLER_FLAG"
echo
echo "Usage:"
if [ $USBONLY -eq 0 ] && [ -f "$BUILD_DIR/${OUTPUT}.iso" ]; then
    echo "  QEMU boot:"
    echo "    qemu-system-x86_64 -bios /usr/share/qemu/OVMF.fd \\"
    echo "                        -cdrom $BUILD_DIR/${OUTPUT}.iso \\"
    echo "                        -m 2G -smp 2"
    echo
fi
if [ $CDONLY -eq 0 ] && [ -f "$BUILD_DIR/${OUTPUT}-usb.img" ]; then
    echo "  USB installation:"
    echo "    sudo dd if=$BUILD_DIR/${OUTPUT}-usb.img of=/dev/sdX bs=4M status=progress"
    echo
fi

# Cleanup
echo "Cleaning up temporary files..."
cd /
rm -rf "$WORK_DIR"
echo "✓ Done"
