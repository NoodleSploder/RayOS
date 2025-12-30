#!/bin/bash
# Headless Linux-guest bring-up (Option D Step 2).
# - Boots a Linux kernel+initrd under QEMU (no UEFI) in initramfs shell mode
# - Injects a deterministic marker over serial: RAYOS_LINUX_GUEST_READY
# - Verifies the marker appears in the captured log

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

TIMEOUT_SECS="${TIMEOUT_SECS:-45}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

LOG_FILE="${LOG_FILE:-$WORK_DIR/linux-subsystem-headless.log}"

rm -f "$LOG_FILE" 2>/dev/null || true

export WORK_DIR TIMEOUT_SECS QEMU_BIN LOG_FILE

echo "Starting Linux subsystem headless bring-up..." >&2

echo "QEMU: $QEMU_BIN" >&2

echo "Timeout: ${TIMEOUT_SECS}s" >&2

echo "Log: $LOG_FILE" >&2

python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"

# Normalize CRLF.
NORM="$WORK_DIR/linux-subsystem-headless.norm.log"
tr -d '\r' < "$LOG_FILE" > "$NORM" 2>/dev/null || true

MARKER="RAYOS_LINUX_GUEST_READY"

if grep -F -a -q "$MARKER" "$NORM"; then
  echo "PASS: observed $MARKER" >&2
  exit 0
fi

echo "FAIL: did not observe $MARKER" >&2

tail -n 200 "$NORM" 2>/dev/null || true

exit 1
