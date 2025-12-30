#!/bin/bash
# Embedded-surface smoke test (Option D Step 4 milestone).
#
# This is the *first* embedded-surface transport prototype:
# - Linux guest boots the RayOS guest agent (rdinit=/rayos_init)
# - Host sends SURFACE_TEST
# - Guest emits a deterministic PPM block between BEGIN/END markers over serial
# - Host extracts the PPM and asserts a stable sha256
#
# Note: This proves the single-surface "embed" pipeline and logging/transport.
# Real Wayland surface forwarding is the next iteration.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

TIMEOUT_SECS="${TIMEOUT_SECS:-45}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

LOG_FILE="${LOG_FILE:-$WORK_DIR/linux-subsystem-embedded-surface.log}"
OUT_PPM="${OUT_PPM:-$WORK_DIR/linux-subsystem-embedded-surface.ppm}"  # text PPM (P3)

rm -f "$LOG_FILE" "$OUT_PPM" 2>/dev/null || true

export WORK_DIR TIMEOUT_SECS QEMU_BIN LOG_FILE

export USE_AGENT_INITRD=1
export READY_MARKER=RAYOS_LINUX_AGENT_READY
export INJECT_READY_MARKER=0
export POST_READY_SEND=$'SURFACE_TEST\n'
export POST_READY_EXPECT=RAYOS_LINUX_EMBED_SURFACE_END

export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Starting embedded surface headless test..." >&2

echo "QEMU: $QEMU_BIN" >&2

echo "Timeout: ${TIMEOUT_SECS}s" >&2

echo "Log: $LOG_FILE" >&2

echo "PPM out: $OUT_PPM" >&2

python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"

# Normalize CRLF for parsing.
NORM="$WORK_DIR/linux-subsystem-embedded-surface.norm.log"
tr -d '\r' < "$LOG_FILE" > "$NORM" 2>/dev/null || true

export LOG_FILE="$NORM"
export OUT_FILE="$OUT_PPM"

SHA="$(python3 "$ROOT_DIR/scripts/tools/linux_subsystem/extract_embedded_surface.py")"

# Deterministic sha256 of the extracted PPM text block.
EXPECTED_SHA="d971357e3ef5f1da102dee562e3cc278a3d9fce258ab815e4748f96fa238310f"

if [ "$SHA" = "$EXPECTED_SHA" ]; then
  echo "PASS: embedded surface sha256 matched ($SHA)" >&2
  exit 0
fi

echo "FAIL: embedded surface sha mismatch" >&2

echo "Expected: $EXPECTED_SHA" >&2

echo "Observed: $SHA" >&2

exit 1
