#!/bin/bash
# Surface lifecycle + geometry headless test (Step 5 scaffolding).
#
# - Boots the RayOS guest agent (rdinit=/rayos_init)
# - Enables the live SurfaceBridge registry output
# - Sends SURFACE_LIFECYCLE_TEST
# - Asserts:
#   - surface 1 was destroyed (absent from registry)
#   - surface 2 exists with updated geometry (x=0,y=0,w=800,h=600)

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

TIMEOUT_SECS="${TIMEOUT_SECS:-60}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

LOG_FILE="${LOG_FILE:-$WORK_DIR/linux-subsystem-surface-lifecycle.log}"
BRIDGE_DIR="${BRIDGE_DIR:-$WORK_DIR/linux-subsystem-surface-lifecycle-bridge}"

rm -f "$LOG_FILE" 2>/dev/null || true
rm -rf "$BRIDGE_DIR" 2>/dev/null || true

export WORK_DIR TIMEOUT_SECS QEMU_BIN LOG_FILE

export USE_AGENT_INITRD=1
export READY_MARKER=RAYOS_LINUX_AGENT_READY
export INJECT_READY_MARKER=0
export POST_READY_SEND=$'SURFACE_LIFECYCLE_TEST\n'
export POST_READY_EXPECT=RAYOS_LINUX_SURFACE_LIFECYCLE_END

export SURFACE_BRIDGE_OUT_DIR="$BRIDGE_DIR"
export MIRROR_SERIAL=0
export PASS_STDIN=0

export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Starting surface lifecycle headless test..." >&2

echo "Log: $LOG_FILE" >&2

echo "Bridge: $BRIDGE_DIR" >&2

python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"

BRIDGE_DIR="$BRIDGE_DIR" python3 - <<'PY'
import json
import os
from pathlib import Path

bridge = Path(os.environ["BRIDGE_DIR"])
reg = json.loads((bridge / "registry.json").read_text(encoding="utf-8"))

surfaces = reg.get("surfaces", {})
windows = reg.get("windows", {})

assert "1" not in surfaces, f"surface 1 should be destroyed, found: {list(surfaces.keys())}"
assert "2" in surfaces, "surface 2 missing"

s2 = surfaces["2"]
assert s2.get("x") == 0, s2
assert s2.get("y") == 0, s2
assert s2.get("w") == 800, s2
assert s2.get("h") == 600, s2

assert "win-2" in windows, f"expected win-2 in windows, got: {list(windows.keys())}"
w2 = windows["win-2"]
assert w2.get("x") == 0 and w2.get("y") == 0 and w2.get("w") == 800 and w2.get("h") == 600, w2

print("OK")
PY

echo "PASS: lifecycle + geometry registry asserted" >&2
