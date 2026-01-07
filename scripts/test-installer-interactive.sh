#!/bin/bash
set -e

# Test installer interactive mode
# This script validates that the installer correctly handles interactive disk selection

BUILD_DIR="$(cd "$(dirname "$0")/../build" && pwd)"
INSTALLER_BIN="$(cd "$(dirname "$0")/../crates/installer/target/release" && pwd)/rayos-installer"

if [[ ! -f "$INSTALLER_BIN" ]]; then
  echo "ERROR: Installer binary not found at $INSTALLER_BIN"
  echo "Run: cargo build -p rayos-installer --release"
  exit 1
fi

echo "=== Testing Installer Interactive Mode ==="
echo

# Test 1: Cancel operation
echo "[1] Testing cancel flow..."
OUTPUT=$(printf "0\n" | "$INSTALLER_BIN" --interactive 2>&1)

if echo "$OUTPUT" | grep -q "RAYOS_INSTALLER:INTERACTIVE_CANCELLED"; then
  echo "  ✓ Cancel flow works"
else
  echo "  ✗ Cancel flow failed"
  echo "$OUTPUT"
  exit 1
fi

# Test 2: Decline confirmation
echo "[2] Testing decline confirmation..."
OUTPUT=$(printf "1\nno\n" | "$INSTALLER_BIN" --interactive 2>&1)

if echo "$OUTPUT" | grep -q "RAYOS_INSTALLER:INTERACTIVE_CANCELLED"; then
  echo "  ✓ Decline confirmation works"
else
  echo "  ✗ Decline confirmation failed"
  echo "$OUTPUT"
  exit 1
fi

# Test 3: Affirm installation plan
echo "[3] Testing installation plan validation..."
OUTPUT=$(printf "1\nyes\n" | "$INSTALLER_BIN" --interactive 2>&1)

if echo "$OUTPUT" | grep -q "RAYOS_INSTALLER:INSTALLATION_PLAN_VALIDATED:disk=sample0"; then
  echo "  ✓ Installation plan validated"
else
  echo "  ✗ Installation plan validation failed"
  echo "$OUTPUT"
  exit 1
fi

# Verify all expected markers are present
echo "[4] Verifying marker sequence..."
MARKERS="RAYOS_INSTALLER:STARTED RAYOS_INSTALLER:SAMPLE_MODE RAYOS_INSTALLER:INTERACTIVE_MODE RAYOS_INSTALLER:INSTALLATION_PLAN_VALIDATED RAYOS_INSTALLER:INTERACTIVE_COMPLETE"

for marker in $MARKERS; do
  if echo "$OUTPUT" | grep -q "$marker"; then
    echo "  ✓ Marker present: $marker"
  else
    echo "  ✗ Marker missing: $marker"
    exit 1
  fi
done

# Verify disk info is displayed
echo "[5] Verifying disk enumeration display..."
if echo "$OUTPUT" | grep -q "Available disks:"; then
  echo "  ✓ Disk enumeration displayed"
else
  echo "  ✗ Disk enumeration not displayed"
  exit 1
fi

# Verify partition configuration is shown
if echo "$OUTPUT" | grep -q "Partition configuration:"; then
  echo "  ✓ Partition configuration displayed"
else
  echo "  ✗ Partition configuration not displayed"
  exit 1
fi

echo
echo "=== All Interactive Mode Tests PASSED ==="
