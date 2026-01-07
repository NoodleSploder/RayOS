#!/bin/bash
set -e

# Complete RayOS installer provisioning
# This script:
# 1. Builds the system image
# 2. Builds the installer binary
# 3. Builds the installer media (ISO/USB)
# 4. Runs all validation tests
# 5. Creates a deployment-ready package

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$REPO_ROOT/build"

echo "========================================="
echo "RayOS Installer Provisioning Pipeline"
echo "========================================="
echo

# Stage 1: Build system image
echo "[1/5] Building system image..."
if "$SCRIPT_DIR/build-system-image.sh" > /dev/null 2>&1; then
  SYSTEM_IMAGE_SIZE=$(du -sh "$BUILD_DIR/rayos-system-image.tar.gz" | cut -f1)
  echo "✓ System image built ($SYSTEM_IMAGE_SIZE)"
else
  echo "✗ Failed to build system image"
  exit 1
fi
echo

# Stage 2: Build installer binary
echo "[2/5] Building installer binary..."
if (cd "$REPO_ROOT/crates/installer" && cargo build --release 2>&1 | grep -q "Finished"); then
  INSTALLER_SIZE=$(du -sh "$REPO_ROOT/crates/installer/target/release/rayos-installer" | cut -f1)
  echo "✓ Installer binary built ($INSTALLER_SIZE)"
else
  echo "✗ Failed to build installer binary"
  exit 1
fi
echo

# Stage 3: Build installer media
echo "[3/5] Building installer media (ISO/USB)..."
if "$SCRIPT_DIR/build-installer-media.sh" > /dev/null 2>&1; then
  ISO_SIZE=$(du -sh "$BUILD_DIR/rayos-installer.iso" | cut -f1)
  USB_SIZE=$(du -sh "$BUILD_DIR/rayos-installer-usb.img" | cut -f1)
  echo "✓ Installer media built"
  echo "  ISO: $ISO_SIZE"
  echo "  USB: $USB_SIZE"
else
  echo "✗ Failed to build installer media"
  exit 1
fi
echo

# Stage 4: Run all validation tests
echo "[4/5] Running validation tests..."
TESTS_PASSED=0
TESTS_TOTAL=0

# Test 4a: Dry-run validation
echo "  [4a] Testing installer dry-run..."
TESTS_TOTAL=$((TESTS_TOTAL + 1))
if "$SCRIPT_DIR/test-installer-dry-run.sh" > /dev/null 2>&1; then
  echo "  ✓ Dry-run test PASSED"
  TESTS_PASSED=$((TESTS_PASSED + 1))
else
  echo "  ✗ Dry-run test FAILED"
fi

# Test 4b: Interactive mode
echo "  [4b] Testing interactive mode..."
TESTS_TOTAL=$((TESTS_TOTAL + 1))
if "$SCRIPT_DIR/test-installer-interactive.sh" > /dev/null 2>&1; then
  echo "  ✓ Interactive test PASSED"
  TESTS_PASSED=$((TESTS_PASSED + 1))
else
  echo "  ✗ Interactive test FAILED"
fi

# Test 4c: Full E2E
echo "  [4c] Testing full E2E flow..."
TESTS_TOTAL=$((TESTS_TOTAL + 1))
if "$SCRIPT_DIR/test-installer-full-e2e.sh" > /dev/null 2>&1; then
  echo "  ✓ E2E test PASSED"
  TESTS_PASSED=$((TESTS_PASSED + 1))
else
  echo "  ✗ E2E test FAILED"
fi

echo "  Test results: $TESTS_PASSED/$TESTS_TOTAL passed"
if [ "$TESTS_PASSED" -ne "$TESTS_TOTAL" ]; then
  echo "✗ Some tests failed"
  exit 1
fi
echo

# Stage 5: Create deployment package
echo "[5/5] Creating deployment package..."
PACKAGE_DIR="$BUILD_DIR/rayos-installer-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$PACKAGE_DIR"

# Copy all artifacts
cp "$BUILD_DIR/rayos-installer.iso" "$PACKAGE_DIR/"
cp "$BUILD_DIR/rayos-installer-usb.img" "$PACKAGE_DIR/"
cp "$BUILD_DIR/rayos-system-image.tar.gz" "$PACKAGE_DIR/"
cp "$REPO_ROOT/crates/installer/target/release/rayos-installer" "$PACKAGE_DIR/rayos-installer.bin"

# Create README
cat > "$PACKAGE_DIR/README.md" << 'PACKAGE_README'
# RayOS Installer Package

This package contains everything needed to install RayOS on a machine.

## Contents

- `rayos-installer.iso` - Bootable ISO image for UEFI systems
- `rayos-installer-usb.img` - Bootable USB image (dd-able)
- `rayos-system-image.tar.gz` - System image with kernel and rootfs
- `rayos-installer.bin` - Standalone installer binary
- `DEPLOYMENT_GUIDE.md` - Installation instructions

## Quick Start

### USB Installation
```bash
# Write USB image to a USB device
sudo dd if=rayos-installer-usb.img of=/dev/sdX bs=4M status=progress
sudo sync
```

### ISO Installation
```bash
# Burn ISO to DVD or mount in virtual machine
# Then boot from the media
```

## Features

- Interactive partition selection
- Safe dry-run mode (sample disks by default)
- Automatic partition creation (GPT)
- Filesystem formatting (FAT32/ext4)
- System image installation
- Full error handling and recovery

## Safety

- No automatic disk writes
- Explicit confirmation required before installation
- Safe by default (sample mode without enumeration flag)
- All operations can be cancelled

## Support

For issues or questions, see:
- INSTALLABLE_RAYOS_PLAN.md - Overall architecture
- BOOTLOADER_INSTALLER_INTEGRATION.md - Bootloader integration
- INSTALLER_MILESTONE_JAN_07_2026.md - Current status

PACKAGE_README

# Create installation guide
cat > "$PACKAGE_DIR/DEPLOYMENT_GUIDE.md" << 'DEPLOY_GUIDE'
# RayOS Installation Guide

## Prerequisites

- USB drive (8 GB minimum) or DVD burner
- Target machine with UEFI boot capability
- No data on target disk (will be overwritten)

## Installation Steps

### 1. Prepare Installation Media

**Option A: USB Installation (Recommended)**
```bash
# Insert USB drive
lsblk  # Identify your USB device (e.g., /dev/sdb)

# Write installer to USB
sudo dd if=rayos-installer-usb.img of=/dev/sdX bs=4M status=progress
sudo sync
```

**Option B: ISO Installation**
```bash
# Burn to DVD
sudo cdrecord -v -dev=/dev/sr0 rayos-installer.iso

# Or use in virtual machine
qemu-system-x86_64 -m 4G -drive file=rayos-installer.iso,format=raw,media=cdrom ...
```

### 2. Boot Installation Media

1. Insert USB drive or boot from ISO
2. Enter UEFI boot menu (usually F12, ESC, or DEL during POST)
3. Select RayOS installer from boot options
4. System will boot into the installer

### 3. Run Installation

The installer will display:
```
=== RayOS Installer ===
Installation Target Selection

Available disks:
  [1] sda - 500 GiB
  [2] sdb - 1000 GiB (removable)
```

**Select target disk:**
- Type the disk number (1, 2, etc.)
- Review the partition layout
- Type "yes" to confirm (or anything else to cancel)

**What happens next:**
1. Target disk is wiped (GPT table created)
2. Three partitions are created:
   - ESP: 512 MiB (FAT32)
   - System: 40 GiB (ext4)
   - VM Pool: Remaining space (ext4)
3. System image is copied
4. Installation completes

### 4. Post-Installation

After installation completes:
1. Installer prompts you to reboot
2. Remove installation media
3. Press Enter to reboot
4. System boots from installed disk

### 5. First Boot

On first boot, RayOS will:
- Mount system and data partitions
- Initialize virtual machine subsystems
- Set up networking
- Start essential services

## Troubleshooting

### Installation won't start
- Ensure media boots (try on another machine)
- Check BIOS settings (UEFI mode enabled, CSM disabled)
- Try different boot order

### Disk not recognized
- Check disk connection
- Try another USB port or disk
- Reboot system and try again

### Installation fails
- Check disk space (minimum 40 GB recommended)
- Ensure disk is not in use by another OS
- Try formatting disk first with `sgdisk`

## Advanced Options

### Installer Flags

```bash
# Interactive mode (default)
rayos-installer --interactive

# Enumerate real disks instead of sample
rayos-installer --interactive --enumerate-local-disks

# JSON disk plan output
rayos-installer --output-format json
```

### Manual Partition Setup

If automatic partitioning fails, manually create partitions:
```bash
sudo sgdisk -Z /dev/sdX          # Clear existing table
sudo sgdisk -o /dev/sdX          # Create GPT
sudo sgdisk -n 1:2048:+512M -t 1:EF00 /dev/sdX  # ESP
sudo sgdisk -n 2:0:+40G -t 2:8300 /dev/sdX      # System
sudo sgdisk -n 3:0:0 -t 3:8300 /dev/sdX         # VM Pool
sudo partprobe /dev/sdX
```

DEPLOY_GUIDE

# Copy documentation
cp "$REPO_ROOT/docs/INSTALLABLE_RAYOS_PLAN.md" "$PACKAGE_DIR/"
cp "$REPO_ROOT/docs/BOOTLOADER_INSTALLER_INTEGRATION.md" "$PACKAGE_DIR/"
cp "$REPO_ROOT/docs/INSTALLER_MILESTONE_JAN_07_2026.md" "$PACKAGE_DIR/"

# Create manifest
cat > "$PACKAGE_DIR/MANIFEST.txt" << MANIFEST
RayOS Installer Package Manifest
Generated: $(date -u)

=== Media ===
rayos-installer.iso ($(du -h "$PACKAGE_DIR/rayos-installer.iso" | cut -f1))
rayos-installer-usb.img ($(du -h "$PACKAGE_DIR/rayos-installer-usb.img" | cut -f1))

=== Binaries ===
rayos-installer.bin ($(du -h "$PACKAGE_DIR/rayos-installer.bin" | cut -f1))

=== System Image ===
rayos-system-image.tar.gz ($(du -h "$PACKAGE_DIR/rayos-system-image.tar.gz" | cut -f1))

=== Documentation ===
README.md
DEPLOYMENT_GUIDE.md
INSTALLABLE_RAYOS_PLAN.md
BOOTLOADER_INSTALLER_INTEGRATION.md
INSTALLER_MILESTONE_JAN_07_2026.md

=== Testing ===
All validation tests: PASSED
- Dry-run: PASSED
- Interactive: PASSED
- Full E2E: PASSED

Total Package Size: $(du -sh "$PACKAGE_DIR" | cut -f1)
MANIFEST

echo "✓ Deployment package created"
echo "  Location: $PACKAGE_DIR"
echo "  Size: $(du -sh "$PACKAGE_DIR" | cut -f1)"
echo

# Final summary
echo "========================================="
echo "✓ RayOS Installer Ready for Deployment"
echo "========================================="
echo
echo "Package location: $PACKAGE_DIR"
echo
echo "Contents:"
ls -lh "$PACKAGE_DIR" | awk 'NR>1 {printf "  %-30s %6s\n", $9, $5}'
echo
echo "Next steps:"
echo "  1. Write ISO/USB to installation media"
echo "  2. Boot target machine from installer media"
echo "  3. Select target disk and confirm installation"
echo "  4. System will reboot into installed RayOS"
echo
echo "Documentation: See README.md and DEPLOYMENT_GUIDE.md"
echo
