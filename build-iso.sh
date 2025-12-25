#!/bin/bash
# RayOS ISO Build Script
# This script builds the bootloader, kernel, and creates a bootable UEFI ISO

set -e

CLEAN=false
OUTPUT_DIR="./iso-output"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --clean)
            CLEAN=true
            shift
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

ROOT_DIR=$(pwd)
ISO_CONTENT_DIR="$OUTPUT_DIR/iso-content"
BOOT_DIR="$ISO_CONTENT_DIR/EFI/BOOT"
RAYOS_DIR="$ISO_CONTENT_DIR/EFI/RAYOS"

echo "=== RayOS ISO Build Script ==="
echo "Root Directory: $ROOT_DIR"
echo ""

# Step 1: Clean previous builds if requested
if [ "$CLEAN" = true ]; then
    echo "[1/5] Cleaning previous builds..." 
    rm -rf "$OUTPUT_DIR"
    echo "  ✓ Cleaned output directory"
    
    pushd bootloader > /dev/null
    cargo clean
    popd > /dev/null
    
    pushd kernel > /dev/null
    cargo clean
    popd > /dev/null
    echo "  ✓ Cleaned cargo builds"
fi

# Step 2: Build bootloader
echo "[2/5] Building UEFI bootloader..."
pushd bootloader > /dev/null
if ! cargo build --release --target x86_64-unknown-uefi; then
    echo "  ✗ Bootloader build failed"
    popd > /dev/null
    exit 1
fi
BOOTLOADER_PATH="./target/x86_64-unknown-uefi/release/uefi_boot.efi"
if [ ! -f "$BOOTLOADER_PATH" ]; then
    echo "  ✗ Bootloader EFI not found at $BOOTLOADER_PATH"
    popd > /dev/null
    exit 1
fi
echo "  ✓ Bootloader built successfully"
popd > /dev/null

# Step 3: Build kernel
echo "[3/5] Building RayOS kernel..."
pushd kernel > /dev/null
if cargo build --release 2>&1; then
    KERNEL_PATH="./target/release/kernel"
    if [ -f "$KERNEL_PATH" ]; then
        echo "  ✓ Kernel built successfully"
    else
        echo "  ⚠ Warning: Kernel executable not found, continuing anyway..."
    fi
else
    echo "  ⚠ Warning: Kernel build had issues, continuing..."
fi
popd > /dev/null

# Step 4: Create ISO structure
echo "[4/5] Creating ISO directory structure..."
mkdir -p "$BOOT_DIR"
mkdir -p "$RAYOS_DIR"
echo "  ✓ Created EFI directory structure"

# Copy bootloader
BOOTLOADER_SOURCE="$ROOT_DIR/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
cp "$BOOTLOADER_SOURCE" "$BOOT_DIR/BOOTX64.EFI"
echo "  ✓ Copied UEFI bootloader"

# Copy kernel if it exists
KERNEL_SOURCE="$ROOT_DIR/kernel/target/release/kernel"
if [ -f "$KERNEL_SOURCE" ]; then
    cp "$KERNEL_SOURCE" "$RAYOS_DIR/kernel.bin"
    echo "  ✓ Copied kernel binary"
else
    echo "  ⚠ Kernel binary not found, ISO will have bootloader only"
fi

# Create boot information file
cat > "$ISO_CONTENT_DIR/README.txt" << 'EOF'
RayOS Boot Information
======================

Bootloader: UEFI x86_64
Architecture: GPU-native, AI-centric OS
System: Bicameral Kernel (System 1 + System 2)

Files:
- EFI/BOOT/BOOTX64.EFI: UEFI bootloader
- EFI/RAYOS/kernel.bin: RayOS kernel binary

Boot Method:
1. Insert USB or mount ISO
2. Boot from UEFI firmware (enable UEFI boot mode)
3. Select this device from boot menu
EOF
echo "  ✓ Created boot information file"

# Step 5: Create ISO image
echo "[5/5] Creating ISO image..."

# Check if xorriso is available
if ! command -v xorriso &> /dev/null; then
    echo "  ✗ xorriso is required but not installed"
    echo ""
    echo "  Installation options:"
    echo "    Ubuntu/Debian: sudo apt-get install xorriso"
    echo "    Fedora/RHEL: sudo dnf install xorriso"
    echo "    macOS: brew install xorriso"
    echo "    Windows (WSL): wsl sudo apt-get install xorriso"
    exit 1
fi

ISO_PATH="$OUTPUT_DIR/rayos.iso"
xorriso -as mkisofs -R -J -b EFI/BOOT/BOOTX64.EFI -eltorito-alt-boot -e EFI/BOOT/BOOTX64.EFI -no-emul-boot -isohybrid-gpt-basdat -o "$ISO_PATH" "$ISO_CONTENT_DIR"

if [ -f "$ISO_PATH" ]; then
    ISO_SIZE=$(du -h "$ISO_PATH" | cut -f1)
    echo "  ✓ ISO created successfully: $ISO_PATH ($ISO_SIZE)"
else
    echo "  ✗ ISO creation failed"
    exit 1
fi

echo ""
echo "=== Build Complete ==="
echo ""
echo "ISO Location: $ISO_PATH"
echo ""
echo "Next Steps:"
echo "1. Write to USB drive using one of these tools:"
echo "   - dd (Linux/Mac): sudo dd if=$ISO_PATH of=/dev/sdX bs=4M && sudo sync"
echo "   - Rufus (Windows): https://rufus.ie/"
echo "   - Balena Etcher (Cross-platform): https://www.balena.io/etcher/"
echo ""
echo "2. Or mount the ISO directly:"
echo "   - Linux: sudo mount -o loop $ISO_PATH /mnt"
echo "   - macOS: hdiutil attach $ISO_PATH"
echo "   - Windows: Mount-DiskImage -ImagePath '$ISO_PATH'"
echo ""
echo "3. Boot from UEFI firmware (may need to enable UEFI boot in BIOS)"
