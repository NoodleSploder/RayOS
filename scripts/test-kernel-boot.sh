#!/bin/bash
# RayOS Phase 4 Kernel Boot Test with Manual UEFI Shell Execution

set -e

REPO_ROOT="/home/noodlesploder/repos/RayOS"
BUILD_DIR="$REPO_ROOT/build"

echo "RayOS Phase 4: Kernel Boot Verification"
echo "========================================"
echo ""

# Create fresh VARS
cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/OVMF_VARS_test.fd

ISO="$BUILD_DIR/rayos-kernel-p4.iso"
LOG="$BUILD_DIR/kernel-boot-test.log"

echo "Starting QEMU and attempting kernel boot..."
echo "  ISO: $ISO"
echo "  Log: $LOG"
echo ""

rm -f "$LOG"

# Run QEMU with serial capture
# We'll use a timeout and let it run, capturing all output
timeout 25 qemu-system-x86_64 \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=/tmp/OVMF_VARS_test.fd \
  -cdrom "$ISO" \
  -m 2G -smp 2 \
  -serial file:"$LOG" \
  -display none \
  2>&1 || true

sleep 1

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "BOOT LOG ANALYSIS"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Check for key indicators
if [ -f "$LOG" ]; then
    echo "Log file exists: $(ls -lh "$LOG" | awk '{print $5}')"
    echo ""
    
    # Look for RayOS kernel messages
    echo "--- RayOS Kernel Messages ---"
    grep "RayOS\|INIT\|╔" "$LOG" 2>/dev/null | head -20 || echo "No RayOS kernel output detected"
    
    echo ""
    echo "--- Full Serial Output (last 50 lines) ---"
    tail -50 "$LOG"
    
    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    
    # Analyze results
    if grep -q "RayOS kernel\|INIT\|╔════" "$LOG"; then
        echo "✓ SUCCESS: Kernel startup messages detected!"
        echo ""
        echo "The kernel has booted and is producing output."
        exit 0
    else
        echo "⚠ WARNING: No kernel output detected"
        echo ""
        echo "The bootloader may not have been executed. This is expected if UEFI"
        echo "firmware defaults to the shell instead of the boot device."
        echo ""
        echo "To manually test:"
        echo "  1. Run: qemu-system-x86_64 -cdrom build/rayos-kernel-p4.iso ..."
        echo "  2. At UEFI Shell prompt, type: FS0:"
        echo "  3. Type: ls"
        echo "  4. Type: cd EFI\\\\Boot"
        echo "  5. Type: bootx64.efi"
        echo ""
        echo "The kernel should then print initialization messages to serial."
        exit 1
    fi
else
    echo "✗ ERROR: No log file created"
    exit 1
fi
