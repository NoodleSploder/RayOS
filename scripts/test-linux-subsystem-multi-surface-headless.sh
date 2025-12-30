#!/bin/bash
# Multi-surface smoke test (Step 5 scaffolding).
#
# - Linux guest boots the RayOS guest agent (rdinit=/rayos_init)
# - Host sends SURFACE_MULTI_TEST
# - Guest emits multiple independent surface frame blocks over serial
# - Host extracts per-surface PPM outputs and asserts stable sha256s

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

TIMEOUT_SECS="${TIMEOUT_SECS:-60}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

LOG_FILE="${LOG_FILE:-$WORK_DIR/linux-subsystem-multi-surface.log}"
OUT_DIR="${OUT_DIR:-$WORK_DIR/linux-subsystem-multi-surface}"

rm -f "$LOG_FILE" 2>/dev/null || true
rm -rf "$OUT_DIR" 2>/dev/null || true

export WORK_DIR TIMEOUT_SECS QEMU_BIN LOG_FILE

export USE_AGENT_INITRD=1
export READY_MARKER=RAYOS_LINUX_AGENT_READY
export INJECT_READY_MARKER=0
export POST_READY_SEND=$'SURFACE_MULTI_TEST\n'
export POST_READY_EXPECT=RAYOS_LINUX_SURFACE_MULTI_END

export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Starting multi-surface headless test..." >&2

echo "QEMU: $QEMU_BIN" >&2

echo "Timeout: ${TIMEOUT_SECS}s" >&2

echo "Log: $LOG_FILE" >&2

echo "Out dir: $OUT_DIR" >&2

python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"

# Normalize CRLF for parsing.
NORM="$WORK_DIR/linux-subsystem-multi-surface.norm.log"
tr -d '\r' < "$LOG_FILE" > "$NORM" 2>/dev/null || true

export LOG_FILE="$NORM"
export OUT_DIR

JSON="$(python3 "$ROOT_DIR/scripts/tools/linux_subsystem/extract_multi_surfaces.py")"

# Deterministic shas for the extracted PPM blocks.
# These are computed from the exact extracted text (including trailing newline).
EXPECTED_SURFACE_1_SHA="${EXPECTED_SURFACE_1_SHA:-411ed3f2b627be225978a8282cba7ab1b18b4af3337591c3cb643130e052c5ce}"
EXPECTED_SURFACE_2_SHA="${EXPECTED_SURFACE_2_SHA:-bd1312ef394c47e8bea8126f1a0217bcc00d0fb170a147e902feb0d87838d4fc}"

SHA1="$(JSON="$JSON" python3 - <<'PY'
import json, os
m=json.loads(os.environ['JSON'])
print(m['frames']['1']['1'])
PY
)"

SHA2="$(JSON="$JSON" python3 - <<'PY'
import json, os
m=json.loads(os.environ['JSON'])
print(m['frames']['2']['1'])
PY
)"

if [ "$SHA1" != "$EXPECTED_SURFACE_1_SHA" ]; then
  echo "FAIL: surface 1 sha mismatch" >&2
  echo "Expected: $EXPECTED_SURFACE_1_SHA" >&2
  echo "Observed: $SHA1" >&2
  exit 1
fi

if [ "$SHA2" != "$EXPECTED_SURFACE_2_SHA" ]; then
  echo "FAIL: surface 2 sha mismatch" >&2
  echo "Expected: $EXPECTED_SURFACE_2_SHA" >&2
  echo "Observed: $SHA2" >&2
  exit 1
fi

echo "PASS: multi-surface extracted" >&2

echo "Surface1 sha: $SHA1" >&2

echo "Surface2 sha: $SHA2" >&2

exit 0
