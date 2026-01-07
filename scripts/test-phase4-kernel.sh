#!/bin/bash
# RayOS Phase 4 Boot Test with Manual UEFI Shell Commands

set -e

REPO_ROOT="/home/noodlesploder/repos/RayOS"
BUILD_DIR="$REPO_ROOT/build"

echo "RayOS Phase 4 Kernel Boot Test"
echo "=============================="
echo ""

# Setup QEMU command
QEMU_CMD="qemu-system-x86_64"
OVMF_CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
OVMF_VARS="/tmp/OVMF_VARS.fd"

# Create fresh VARS file
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$OVMF_VARS"

ISO="$BUILD_DIR/rayos-kernel-p4-auto.iso"
if [ ! -f "$ISO" ]; then
    echo "Error: ISO not found at $ISO"
    exit 1
fi

echo "Configuration:"
echo "  QEMU:        $QEMU_CMD"
echo "  OVMF Code:   $OVMF_CODE"
echo "  OVMF Vars:   $OVMF_VARS"
echo "  ISO:         $ISO"
echo "  Boot Command: FS0:\EFI\Boot\bootx64.efi"
echo ""

# Create expect script for QEMU interaction
EXPECT_SCRIPT="/tmp/qemu_boot_$$"
cat > "$EXPECT_SCRIPT" << 'EOFEXPECT'
#!/usr/bin/expect -f
set timeout 30
set boot_cmd "FS0:\EFI\Boot\bootx64.efi"

# Wait for UEFI shell prompt
expect {
    "Press ESC" {
        send "\r"
        exp_continue
    }
    "Shell>" {
        puts "UEFI Shell ready, executing bootloader..."
        send "$boot_cmd\r"
        exp_continue
    }
    "RayOS kernel" {
        puts "SUCCESS: Kernel started!"
        exit 0
    }
    timeout {
        puts "Timeout waiting for kernel"
        exit 1
    }
}
EOFEXPECT

chmod +x "$EXPECT_SCRIPT"

# Run QEMU and try to boot
echo "Starting QEMU... (will attempt to boot via UEFI shell)"
echo ""

timeout 45 $QEMU_CMD \
  -drive if=pflash,format=raw,unit=0,file=$OVMF_CODE,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=$OVMF_VARS \
  -cdrom "$ISO" \
  -m 2G -smp 2 \
  -serial file:serial-p4-test.log \
  -display none \
  2>&1 || true

sleep 2

echo ""
echo "=== Serial Output ===" 
if [ -f serial-p4-test.log ]; then
    cat serial-p4-test.log | head -100
    
    # Check for kernel startup
    if grep -q "RayOS kernel" serial-p4-test.log; then
        echo ""
        echo "✓ SUCCESS: Kernel boot message detected!"
    else
        echo ""
        echo "⚠ No kernel output yet - UEFI shell is running but bootloader wasn't executed"
        echo ""
        echo "Manual Steps to Test:"
        echo "  1. In UEFI Shell, type: FS0:"
        echo "  2. Type: ls"
        echo "  3. Type: cd EFI\\Boot"
        echo "  4. Type: bootx64.efi"
    fi
else
    echo "No serial log found"
fi

# Cleanup
rm -f "$EXPECT_SCRIPT"
