#!/bin/bash
# RayOS ISO Build Script
# This script builds the bootloader, kernel, and creates a bootable UEFI ISO

set -e

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Ensure rustup-managed toolchain is preferred over any system Rust.
# (This repo uses Rust 2024 edition in the bootloader, which requires Rust >= 1.85.)
if [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

CLEAN=false
OUTPUT_DIR="$ROOT_DIR/build"
ARCHES="universal"  # x86_64 | aarch64 | both | universal (default: universal)

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --clean)
            CLEAN=true
            shift
            ;;
        --arch)
            ARCHES="$2"
            shift 2
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

cd "$ROOT_DIR"

# Bootloader uses a pinned nightly toolchain via bootloader/rust-toolchain.toml.
# Build explicitly with the same toolchain here to avoid accidentally invoking a
# system-installed Rust that lacks the UEFI target std/core libraries.
BOOTLOADER_TOOLCHAIN="nightly-2024-11-01"

arch_matches() {
    local want="$1"
    local arch="$2"
    if [ "$want" = "both" ]; then
        return 0
    fi
    [ "$want" = "$arch" ]
}

build_bootloader() {
    local target="$1"
    pushd "$ROOT_DIR/crates/bootloader" > /dev/null
    # Ensure the pinned toolchain + requested target are available.
    rustup toolchain install "$BOOTLOADER_TOOLCHAIN" >/dev/null 2>&1 || true
    rustup target add "$target" --toolchain "$BOOTLOADER_TOOLCHAIN" >/dev/null 2>&1 || true

    # Build using the pinned toolchain.
    cargo +"$BOOTLOADER_TOOLCHAIN" build -p rayos-bootloader --release --target "$target"
    popd > /dev/null
}

create_iso_for_arch() {
    local arch="$1"            # x86_64 | aarch64
    local target="$2"          # rust target triple
    local boot_efi_name="$3"   # BOOTX64.EFI | BOOTAA64.EFI
    local iso_name="$4"        # output filename
    local label="$5"           # volume label

    local ISO_CONTENT_DIR="$OUTPUT_DIR/iso-content-$arch"
    local BOOT_DIR="$ISO_CONTENT_DIR/EFI/BOOT"
    local RAYOS_DIR="$ISO_CONTENT_DIR/EFI/RAYOS"

    echo "  • Staging $arch ISO..."
    mkdir -p "$BOOT_DIR" "$RAYOS_DIR"

    local BOOTLOADER_SOURCE="$ROOT_DIR/crates/bootloader/target/$target/release/uefi_boot.efi"
    if [ ! -f "$BOOTLOADER_SOURCE" ]; then
        echo "  ✗ Bootloader EFI not found at $BOOTLOADER_SOURCE"
        exit 1
    fi
    cp "$BOOTLOADER_SOURCE" "$BOOT_DIR/$boot_efi_name"

    local KERNEL_SOURCE="$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare"
    if [ -f "$KERNEL_SOURCE" ]; then
        cp "$KERNEL_SOURCE" "$RAYOS_DIR/kernel.bin"
    fi

    # Create an EFI System Partition (ESP) FAT image for UEFI boot.
    local ESP_IMG="$BOOT_DIR/efiboot.img"
    # Keep < 32 MiB so El Torito load-size stays representable for picky firmware.
    dd if=/dev/zero of="$ESP_IMG" bs=1M count=31 status=none
    mkfs.vfat -F 32 "$ESP_IMG" > /dev/null

    # Create a startup.nsh for UEFI shell auto-boot
    cat > /tmp/startup.nsh << NSH
fs0:
\\EFI\\BOOT\\${boot_efi_name}
NSH

    if command -v mmd >/dev/null 2>&1 && command -v mcopy >/dev/null 2>&1; then
        mmd -i "$ESP_IMG" ::/EFI ::/EFI/BOOT ::/EFI/RAYOS 2>/dev/null || true
        mcopy -i "$ESP_IMG" "$BOOTLOADER_SOURCE" "::/EFI/BOOT/$boot_efi_name"
        mcopy -i "$ESP_IMG" /tmp/startup.nsh ::/startup.nsh
        if [ -f "$KERNEL_SOURCE" ]; then
            mcopy -i "$ESP_IMG" "$KERNEL_SOURCE" ::/EFI/RAYOS/kernel.bin
        fi
    else
        # Without mtools, we fall back to loop-mounting the FAT image.
        # In non-interactive contexts, a sudo password prompt would hang the build.
        if [ ! -t 0 ]; then
            if ! command -v sudo >/dev/null 2>&1 || ! sudo -n true >/dev/null 2>&1; then
                echo "  ✗ Need mtools (mcopy/mmd) OR passwordless sudo to populate the ESP image"
                echo "    Install mtools: sudo apt-get install mtools"
                echo "    Or rerun interactively so sudo can prompt for a password."
                exit 1
            fi
        fi

        if ! command -v sudo >/dev/null 2>&1; then
            echo "  ✗ Need either mtools (mcopy/mmd) or sudo+mount to populate the ESP image"
            echo "    Install mtools: sudo apt-get install mtools"
            exit 1
        fi
        local ESP_MNT
        ESP_MNT=$(mktemp -d)
        cleanup_esp_mount() {
            sudo umount "$ESP_MNT" >/dev/null 2>&1 || true
            rmdir "$ESP_MNT" >/dev/null 2>&1 || true
        }
        trap cleanup_esp_mount EXIT

        sudo mount -o loop,rw "$ESP_IMG" "$ESP_MNT"
        sudo mkdir -p "$ESP_MNT/EFI/BOOT" "$ESP_MNT/EFI/RAYOS"
        sudo cp "$BOOTLOADER_SOURCE" "$ESP_MNT/EFI/BOOT/$boot_efi_name"
        echo 'fs0:' | sudo tee "$ESP_MNT/startup.nsh" > /dev/null
        echo "\\EFI\\BOOT\\${boot_efi_name}" | sudo tee -a "$ESP_MNT/startup.nsh" > /dev/null
        if [ -f "$KERNEL_SOURCE" ]; then
            sudo cp "$KERNEL_SOURCE" "$ESP_MNT/EFI/RAYOS/kernel.bin"
        fi
        sudo sync
        sudo umount "$ESP_MNT"
        rmdir "$ESP_MNT"
        trap - EXIT
    fi

    cat > "$ISO_CONTENT_DIR/README.txt" << EOF
RayOS Boot Information
======================

Bootloader: UEFI $arch
Architecture: GPU-native, AI-centric OS
System: Bicameral Kernel (System 1 + System 2)

Files:
- EFI/BOOT/$boot_efi_name: UEFI bootloader
- EFI/BOOT/efiboot.img: EFI System Partition image (FAT)
- EFI/RAYOS/kernel.bin: RayOS kernel binary (if present)

Boot Method:
1. Insert USB or mount ISO
2. Boot from UEFI firmware (enable UEFI boot mode)
3. Select this device from boot menu
EOF

    local ISO_PATH="$OUTPUT_DIR/$iso_name"
    xorriso -as mkisofs \
    -R -J \
    -V "$label" \
        -c /boot.catalog \
    -e EFI/BOOT/efiboot.img \
    -no-emul-boot \
    -isohybrid-gpt-basdat \
    -o "$ISO_PATH" \
    "$ISO_CONTENT_DIR"

    if [ -f "$ISO_PATH" ]; then
        local ISO_SIZE
        ISO_SIZE=$(du -h "$ISO_PATH" | cut -f1)
        echo "  ✓ ISO created: $ISO_PATH ($ISO_SIZE)"
    else
        echo "  ✗ ISO creation failed: $ISO_PATH"
        exit 1
    fi
}

create_universal_iso() {
    local ISO_CONTENT_DIR="$OUTPUT_DIR/iso-content-universal"
    local BOOT_DIR="$ISO_CONTENT_DIR/EFI/BOOT"
    local RAYOS_DIR="$ISO_CONTENT_DIR/EFI/RAYOS"

    echo "  • Staging universal ISO..."
    mkdir -p "$BOOT_DIR" "$RAYOS_DIR"

    local BOOTLOADER_X64="$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
    local BOOTLOADER_A64="$ROOT_DIR/crates/bootloader/target/aarch64-unknown-uefi/release/uefi_boot.efi"
    if [ ! -f "$BOOTLOADER_X64" ]; then
        echo "  ✗ x86_64 bootloader EFI not found at $BOOTLOADER_X64"
        exit 1
    fi
    if [ ! -f "$BOOTLOADER_A64" ]; then
        echo "  ✗ aarch64 bootloader EFI not found at $BOOTLOADER_A64"
        exit 1
    fi

    cp "$BOOTLOADER_X64" "$BOOT_DIR/BOOTX64.EFI"
    cp "$BOOTLOADER_A64" "$BOOT_DIR/BOOTAA64.EFI"

    local KERNEL_SOURCE="$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare"
    if [ -f "$KERNEL_SOURCE" ]; then
        cp "$KERNEL_SOURCE" "$RAYOS_DIR/kernel.bin"
    fi

    local ESP_IMG="$BOOT_DIR/efiboot.img"
    # Keep < 32 MiB so El Torito load-size stays representable for picky firmware.
    dd if=/dev/zero of="$ESP_IMG" bs=1M count=31 status=none
    mkfs.vfat -F 32 "$ESP_IMG" > /dev/null

    # Create a startup.nsh for UEFI shell auto-boot (tries both arches).
    cat > /tmp/startup.nsh << 'NSH'
fs0:
\EFI\BOOT\BOOTX64.EFI
\EFI\BOOT\BOOTAA64.EFI
NSH

    if command -v mmd >/dev/null 2>&1 && command -v mcopy >/dev/null 2>&1; then
        mmd -i "$ESP_IMG" ::/EFI ::/EFI/BOOT ::/EFI/RAYOS 2>/dev/null || true
        mcopy -i "$ESP_IMG" "$BOOTLOADER_X64" ::/EFI/BOOT/BOOTX64.EFI
        mcopy -i "$ESP_IMG" "$BOOTLOADER_A64" ::/EFI/BOOT/BOOTAA64.EFI
        mcopy -i "$ESP_IMG" /tmp/startup.nsh ::/startup.nsh
        if [ -f "$KERNEL_SOURCE" ]; then
            mcopy -i "$ESP_IMG" "$KERNEL_SOURCE" ::/EFI/RAYOS/kernel.bin
        fi
    else
        # Without mtools, we fall back to loop-mounting the FAT image.
        # In non-interactive contexts, a sudo password prompt would hang the build.
        if [ ! -t 0 ]; then
            if ! command -v sudo >/dev/null 2>&1 || ! sudo -n true >/dev/null 2>&1; then
                echo "  ✗ Need mtools (mcopy/mmd) OR passwordless sudo to populate the ESP image"
                echo "    Install mtools: sudo apt-get install mtools"
                echo "    Or rerun interactively so sudo can prompt for a password."
                exit 1
            fi
        fi

        if ! command -v sudo >/dev/null 2>&1; then
            echo "  ✗ Need either mtools (mcopy/mmd) or sudo+mount to populate the ESP image"
            echo "    Install mtools: sudo apt-get install mtools"
            exit 1
        fi
        local ESP_MNT
        ESP_MNT=$(mktemp -d)
        cleanup_esp_mount() {
            sudo umount "$ESP_MNT" >/dev/null 2>&1 || true
            rmdir "$ESP_MNT" >/dev/null 2>&1 || true
        }
        trap cleanup_esp_mount EXIT

        sudo mount -o loop,rw "$ESP_IMG" "$ESP_MNT"
        sudo mkdir -p "$ESP_MNT/EFI/BOOT" "$ESP_MNT/EFI/RAYOS"
        sudo cp "$BOOTLOADER_X64" "$ESP_MNT/EFI/BOOT/BOOTX64.EFI"
        sudo cp "$BOOTLOADER_A64" "$ESP_MNT/EFI/BOOT/BOOTAA64.EFI"
        sudo cp /tmp/startup.nsh "$ESP_MNT/startup.nsh"
        if [ -f "$KERNEL_SOURCE" ]; then
            sudo cp "$KERNEL_SOURCE" "$ESP_MNT/EFI/RAYOS/kernel.bin"
        fi
        sudo sync
        sudo umount "$ESP_MNT"
        rmdir "$ESP_MNT"
        trap - EXIT
    fi

    cat > "$ISO_CONTENT_DIR/README.txt" << 'EOF'
RayOS Boot Information
======================

Bootloader: UEFI (universal)
Architecture: GPU-native, AI-centric OS
System: Bicameral Kernel (System 1 + System 2)

Files:
- EFI/BOOT/BOOTX64.EFI: UEFI bootloader (x86_64)
- EFI/BOOT/BOOTAA64.EFI: UEFI bootloader (aarch64)
- EFI/BOOT/efiboot.img: EFI System Partition image (FAT)
- EFI/RAYOS/kernel.bin: RayOS kernel binary (if present)

Boot Method:
1. Insert USB or mount ISO
2. Boot from UEFI firmware (enable UEFI boot mode)
3. Select this device from boot menu
EOF

    # Create a BIOS boot stub so the ISO shows as bootable in all firmware
    if command -v nasm >/dev/null 2>&1; then
        cat > /tmp/bios_stub_rayos.asm << 'ASMEOF'
BITS 16
ORG 0x7C00
start:
    xor ax, ax
    mov ds, ax
    mov si, msg
.print:
    lodsb
    or al, al
    jz .halt
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    jmp .print
.halt:
    hlt
    jmp .halt
msg db 'RayOS requires UEFI boot mode.', 13, 10
    db 'Enable UEFI in firmware settings.', 13, 10, 0
times 510-($-$$) db 0
dw 0xAA55
ASMEOF
        nasm -f bin /tmp/bios_stub_rayos.asm -o "$OUTPUT_DIR/bios_stub.bin" 2>/dev/null
    fi

    local ISO_PATH="$OUTPUT_DIR/rayos.iso"

    # Create UEFI-bootable ISO with El Torito
    # Standard approach: -c creates boot catalog, -e points to ESP image for UEFI boot
    xorriso -as mkisofs \
        -R -J -joliet-long \
        -V "RAYOS" \
        -c boot.catalog \
        -e EFI/BOOT/efiboot.img \
        -no-emul-boot \
        -o "$ISO_PATH" \
        "$ISO_CONTENT_DIR" 2>&1 | grep -v "^WARNING" || true

    if [ -f "$ISO_PATH" ]; then
        local ISO_SIZE
        ISO_SIZE=$(du -h "$ISO_PATH" | cut -f1)
        echo "  ✓ ISO created: $ISO_PATH ($ISO_SIZE)"
    else
        echo "  ✗ ISO creation failed: $ISO_PATH"
        exit 1
    fi
}

create_universal_usb_image() {
    local IMG_PATH="$OUTPUT_DIR/rayos-universal-usb.img"
    local IMG_SIZE_MB=128

    echo "  • Creating universal UEFI USB image..."

    # This path requires root (loop devices + mount). Fail fast if sudo would prompt.
    if ! command -v sudo >/dev/null 2>&1; then
        echo "  ✗ sudo not found; cannot create universal USB image"
        echo "    Install mtools (or run as root) to avoid mounting, or build ISO targets instead."
        exit 1
    fi
    if [ ! -t 0 ] && ! sudo -n true >/dev/null 2>&1; then
        echo "  ✗ Need passwordless sudo to create universal USB image in non-interactive mode"
        echo "    Re-run interactively (so sudo can prompt) or configure NOPASSWD."
        exit 1
    fi

    mkdir -p "$OUTPUT_DIR"
    rm -f "$IMG_PATH"
    dd if=/dev/zero of="$IMG_PATH" bs=1M count="$IMG_SIZE_MB" status=none

    # Create GPT with a single EFI System Partition.
    sgdisk -og "$IMG_PATH" > /dev/null
    # Start at 1 MiB, use most of the disk, type EF00 (EFI System).
    sgdisk -n 1:2048:0 -t 1:EF00 -c 1:"RAYOS-ESP" "$IMG_PATH" > /dev/null

    local loopdev
    loopdev=$(sudo losetup --find --partscan --show "$IMG_PATH")
    local part="${loopdev}p1"

    # Give udev a moment to create the partition device.
    for _ in 1 2 3 4 5; do
        if [ -b "$part" ]; then
            break
        fi
        sleep 0.1
    done
    if [ ! -b "$part" ]; then
        sudo losetup -d "$loopdev" || true
        echo "  ✗ Failed to create loop partition device for $IMG_PATH"
        exit 1
    fi

    sudo mkfs.vfat -F 32 -n RAYOSESP "$part" > /dev/null

    local mnt
    mnt=$(mktemp -d)
    sudo mount "$part" "$mnt"
    sudo mkdir -p "$mnt/EFI/BOOT" "$mnt/EFI/RAYOS"

    local BOOTLOADER_X64="$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
    local BOOTLOADER_A64="$ROOT_DIR/crates/bootloader/target/aarch64-unknown-uefi/release/uefi_boot.efi"
    sudo cp "$BOOTLOADER_X64" "$mnt/EFI/BOOT/BOOTX64.EFI"
    sudo cp "$BOOTLOADER_A64" "$mnt/EFI/BOOT/BOOTAA64.EFI"

    # Optional UEFI shell auto-boot convenience.
    cat > /tmp/startup.nsh << 'NSH'
fs0:
\EFI\BOOT\BOOTX64.EFI
\EFI\BOOT\BOOTAA64.EFI
NSH
    sudo cp /tmp/startup.nsh "$mnt/startup.nsh"

    local KERNEL_SOURCE="$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare"
    if [ -f "$KERNEL_SOURCE" ]; then
        sudo cp "$KERNEL_SOURCE" "$mnt/EFI/RAYOS/kernel.bin"
    fi

    sudo sync
    sudo umount "$mnt"
    rmdir "$mnt"
    sudo losetup -d "$loopdev"

    local IMG_SIZE
    IMG_SIZE=$(du -h "$IMG_PATH" | cut -f1)
    echo "  ✓ USB image created: $IMG_PATH ($IMG_SIZE)"
}

echo "=== RayOS ISO Build Script ==="
echo "Root Directory: $ROOT_DIR"
echo "Architectures: $ARCHES"
echo ""

case "$ARCHES" in
    x86_64|aarch64|both|universal) ;;
    *)
        echo "Invalid --arch value: $ARCHES"
        echo "Valid values: x86_64 | aarch64 | both | universal"
        exit 1
        ;;
esac

# Step 1: Clean previous builds if requested
if [ "$CLEAN" = true ]; then
    echo "[1/5] Cleaning previous builds..."
    rm -rf "$OUTPUT_DIR"
    echo "  ✓ Cleaned output directory"

    pushd "$ROOT_DIR/crates/bootloader" > /dev/null
    cargo clean
    popd > /dev/null

    pushd "$ROOT_DIR/crates/kernel" > /dev/null
    cargo clean
    popd > /dev/null
    echo "  ✓ Cleaned cargo builds"
fi

echo "[2/5] Building UEFI bootloader(s)..."
if [ "$ARCHES" = "universal" ] || arch_matches "$ARCHES" "x86_64"; then
    echo "  • Building x86_64 bootloader..."
    if ! build_bootloader "x86_64-unknown-uefi"; then
        echo "  ✗ x86_64 bootloader build failed"
        exit 1
    fi
    echo "  ✓ x86_64 bootloader built"
fi
if [ "$ARCHES" = "universal" ] || arch_matches "$ARCHES" "aarch64"; then
    echo "  • Building aarch64 bootloader..."
    if ! build_bootloader "aarch64-unknown-uefi"; then
        echo "  ✗ aarch64 bootloader build failed"
        exit 1
    fi
    echo "  ✓ aarch64 bootloader built"
fi

# Step 3: Build kernel
echo "[3/5] Building RayOS kernel..."
pushd "$ROOT_DIR/crates/kernel-bare" > /dev/null
# Build the bare-metal kernel binary for x86_64-unknown-none target.
# Pin the toolchain to avoid build-std breakage when rolling nightly's rust-src changes.
KERNEL_TOOLCHAIN="nightly-2024-11-01-x86_64-unknown-linux-gnu"
export PATH="$HOME/.cargo/bin:$PATH"
if RUSTC="$(rustup which rustc --toolchain "$KERNEL_TOOLCHAIN")" \
    rustup run "$KERNEL_TOOLCHAIN" cargo build --release --target x86_64-unknown-none \
    -Z build-std=core,alloc \
    -Z build-std-features=compiler-builtins-mem \
    2>&1; then
    # Binary name is kernel-bare
    KERNEL_PATH="./target/x86_64-unknown-none/release/kernel-bare"
    if [ -f "$KERNEL_PATH" ]; then
        echo "  ✓ Bare-metal kernel built successfully (ELF format)"
    else
        echo "  ⚠ Warning: Kernel executable not found, continuing anyway..."
    fi
else
    echo "  ⚠ Warning: Kernel build had issues, continuing..."
fi
popd > /dev/null

echo "[4/5] Creating ISO directory structure(s)..."

echo "[5/5] Creating ISO image(s)..."

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

if arch_matches "$ARCHES" "x86_64"; then
    create_iso_for_arch "x86_64" "x86_64-unknown-uefi" "BOOTX64.EFI" "rayos.iso" "RAYOSX64"
fi
if arch_matches "$ARCHES" "aarch64"; then
    create_iso_for_arch "aarch64" "aarch64-unknown-uefi" "BOOTAA64.EFI" "rayos-aarch64.iso" "RAYOSA64"
fi

if [ "$ARCHES" = "universal" ]; then
    create_universal_iso
    create_universal_usb_image
fi

echo ""
echo "=== Build Complete ==="
echo ""
echo "ISO Output Directory: $OUTPUT_DIR"
echo ""
echo "Next Steps:"
echo "1. Write to USB drive using one of these tools:"
echo "   - dd (Linux/Mac): sudo dd if=$OUTPUT_DIR/<iso>.iso of=/dev/sdX bs=4M && sudo sync"
echo "   - Rufus (Windows): https://rufus.ie/"
echo "   - Balena Etcher (Cross-platform): https://www.balena.io/etcher/"
echo ""
echo "2. Or mount the ISO directly:"
echo "   - Linux: sudo mount -o loop $OUTPUT_DIR/<iso>.iso /mnt"
echo "   - macOS: hdiutil attach $OUTPUT_DIR/<iso>.iso"
echo "   - Windows: Mount-DiskImage -ImagePath '$OUTPUT_DIR/<iso>.iso'"
echo ""
echo "3. Boot from UEFI firmware (may need to enable UEFI boot in BIOS)"
