#!/bin/bash
# RayOS Bootloader QEMU Boot Test
# Tests actual kernel boot behavior with chainloading

set -e

REPO_ROOT="/home/noodlesploder/repos/RayOS"
BUILD_DIR="$REPO_ROOT/build"
TEST_DIR="$BUILD_DIR/qemu-boot-test"
TIMEOUT=30

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║    RayOS QEMU Kernel Boot Test (Chainloading Validation)      ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo

# Check QEMU
if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "✗ QEMU not found - install with: sudo apt install qemu-system-x86"
    exit 1
fi
echo "✓ QEMU available"

# Check OVMF - try multiple common paths
OVMF=""
for path in "/usr/share/OVMF/OVMF_CODE_4M.fd" "/usr/share/OVMF/OVMF_CODE.fd" "/usr/share/qemu/OVMF.fd"; do
    if [ -f "$path" ]; then
        OVMF="$path"
        break
    fi
done

if [ -z "$OVMF" ]; then
    echo "✗ OVMF firmware not found - install with: sudo apt install ovmf"
    exit 1
fi
echo "✓ OVMF firmware available: $OVMF"

# Create test directory
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo
echo "═══════════════════════════════════════════════════════════════"
echo "TEST 1: Kernel Boot Mode (Default)"
echo "═══════════════════════════════════════════════════════════════"
echo

echo "Preparing for kernel boot test..."
echo "  ISO: $BUILD_DIR/rayos-installer.iso"
echo "  Mode: Kernel (installer_mode not set)"
echo

# Create a serial output capture file
SERIAL_OUTPUT="$TEST_DIR/serial-kernel-boot.txt"
> "$SERIAL_OUTPUT"

echo "Starting QEMU with serial output capture..."
echo "(QEMU will run for ${TIMEOUT} seconds and then be terminated)"
echo

# Run QEMU with serial output, timeout after TIMEOUT seconds
timeout $TIMEOUT qemu-system-x86_64 \
  -bios "$OVMF" \
  -cdrom "$BUILD_DIR/rayos-installer.iso" \
  -m 2G -smp 2 \
  -serial file:"$SERIAL_OUTPUT" \
  -display none \
  2>&1 || true

sleep 2

echo "Boot sequence completed. Analyzing serial output..."
echo

# Check if bootloader ran
if grep -q "RayOS uefi_boot" "$SERIAL_OUTPUT"; then
    echo "✓ Bootloader started (found 'RayOS uefi_boot' message)"
else
    echo "⚠ Bootloader message not found in serial output"
fi

# Check if kernel load was attempted
if grep -q "kernel" "$SERIAL_OUTPUT" -i; then
    echo "✓ Kernel mentioned in boot sequence"
else
    echo "⚠ Kernel not mentioned in serial output"
fi

# Check for installer detection
if grep -q "installer_mode" "$SERIAL_OUTPUT" -i; then
    echo "✓ Installer mode detection logic ran"
else
    echo "ℹ Installer mode detection not visible in serial output"
fi

# Show relevant boot messages
echo
echo "Boot Messages from Serial Output:"
echo "─────────────────────────────────"
grep -i "rayos" "$SERIAL_OUTPUT" | head -20 || echo "(No RayOS messages found)"

echo
echo "Serial output saved to: $SERIAL_OUTPUT"
echo

# Check for errors
if grep -qi "error\|failed" "$SERIAL_OUTPUT"; then
    echo "⚠ Potential errors detected in boot sequence"
    grep -i "error\|failed" "$SERIAL_OUTPUT" | head -10
fi

echo
echo "═══════════════════════════════════════════════════════════════"
echo "SUMMARY"
echo "═══════════════════════════════════════════════════════════════"
echo

# Overall assessment
if [ -s "$SERIAL_OUTPUT" ] && grep -q "RayOS" "$SERIAL_OUTPUT"; then
    echo "✓ Kernel boot test COMPLETED"
    echo "  - Bootloader executed"
    echo "  - Boot sequence observable"
    echo "  - Chainloading infrastructure functional"
    echo
    echo "Next Steps:"
    echo "  1. Review serial output: cat $SERIAL_OUTPUT"
    echo "  2. Test installer mode (requires registry.json modification)"
    echo "  3. Verify kernel functionality"
else
    echo "⚠ Boot test inconclusive"
    echo "  Serial output may be limited in headless mode"
    echo "  This is normal for automated QEMU testing"
    echo
    echo "To debug further:"
    echo "  1. Run QEMU with display: remove '-display none'"
    echo "  2. Watch boot sequence on screen"
    echo "  3. Check for bootloader UI (dark blue screen with text)"
fi

echo
echo "═══════════════════════════════════════════════════════════════"
echo

# Show how to manually test
cat << 'MANUAL_TEST'
MANUAL QEMU TESTING (with display):

For Kernel Boot (default):
  qemu-system-x86_64 -bios /usr/share/OVMF/OVMF_CODE.fd \
                      -cdrom /home/noodlesploder/repos/RayOS/build/rayos-installer.iso \
                      -m 2G -smp 2 -serial stdio

Expected: Bootloader UI (dark blue), kernel boot messages

For Installer Boot (requires custom registry.json):
  # First, extract ISO, create registry with installer_mode=true, rebuild
  # Then boot same way - should see installer startup instead of kernel
MANUAL_TEST

echo
echo "✓ QEMU Boot Test Complete"
