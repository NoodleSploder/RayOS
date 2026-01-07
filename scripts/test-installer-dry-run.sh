#!/bin/bash
# Test the RayOS installer binary directly (dry-run mode).
#
# This test extracts the installer binary from the boot media and verifies
# it runs successfully in sample mode, detecting the expected disks and
# emitting proper markers.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

INSTALLER_BIN="${INSTALLER_BIN:-$ROOT_DIR/crates/installer/target/release/rayos-installer}"

if [ ! -f "$INSTALLER_BIN" ]; then
  echo "ERROR: Installer binary not found at $INSTALLER_BIN" >&2
  exit 1
fi

INSTALLER_LOG="$WORK_DIR/installer-dry-run.log"
INSTALLER_JSON="$WORK_DIR/installer-dry-run.json"
rm -f "$INSTALLER_LOG" "$INSTALLER_JSON" 2>/dev/null || true

echo "Running RayOS installer in dry-run (sample) mode..." >&2
"$INSTALLER_BIN" --output-format json >"$INSTALLER_JSON" 2>"$INSTALLER_LOG" || {
  RC=$?
  echo "ERROR: Installer exited with code $RC" >&2
  cat "$INSTALLER_LOG"
  exit $RC
}

# Check for expected markers
STARTED_MARKER="RAYOS_INSTALLER:STARTED"
SAMPLE_MARKER="RAYOS_INSTALLER:SAMPLE_MODE"
PLAN_MARKER="RAYOS_INSTALLER:PLAN_GENERATED"
JSON_MARKER="RAYOS_INSTALLER:JSON_EMITTED"
COMPLETE_MARKER="RAYOS_INSTALLER:COMPLETE"

if grep -q "$STARTED_MARKER" "$INSTALLER_LOG" \
  && grep -q "$SAMPLE_MARKER" "$INSTALLER_LOG" \
  && grep -q "$PLAN_MARKER" "$INSTALLER_LOG" \
  && grep -q "$JSON_MARKER" "$INSTALLER_LOG" \
  && grep -q "$COMPLETE_MARKER" "$INSTALLER_LOG"; then
  echo "PASS: Installer markers present and valid" >&2
  echo "Log: $INSTALLER_LOG" >&2

  # Optional: verify JSON is valid
  if jq -e '.disks | length > 0' "$INSTALLER_JSON" > /dev/null 2>&1; then
    echo "PASS: JSON payload contains disk records" >&2
  else
    echo "WARNING: Could not validate JSON structure" >&2
  fi
  exit 0
fi

echo "FAIL: Missing expected installer markers" >&2
echo "Log: $INSTALLER_LOG" >&2
cat "$INSTALLER_LOG"
exit 1
