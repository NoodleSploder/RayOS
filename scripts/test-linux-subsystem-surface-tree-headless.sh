#!/bin/bash
# Surface parent/child tree + state flags headless test (Step 5 scaffolding).
#
# - Boots the RayOS guest agent (rdinit=/rayos_init)
# - Enables the live SurfaceBridge registry output
# - Sends SURFACE_TREE_TEST
# - Asserts:
#   - win-11 is a child of win-10
#   - win-11 parent_window_id is win-10
#   - win-10 states contains maximized
#   - win-11 states contains popup and modal

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

TIMEOUT_SECS="${TIMEOUT_SECS:-45}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

LOG_FILE="${LOG_FILE:-$WORK_DIR/linux-subsystem-surface-tree.log}"
BRIDGE_DIR="${BRIDGE_DIR:-$WORK_DIR/linux-subsystem-surface-tree-bridge}"

rm -f "$LOG_FILE" 2>/dev/null || true
rm -rf "$BRIDGE_DIR" 2>/dev/null || true

export WORK_DIR TIMEOUT_SECS QEMU_BIN LOG_FILE

export USE_AGENT_INITRD=1
export READY_MARKER=RAYOS_LINUX_AGENT_READY
export INJECT_READY_MARKER=0
export POST_READY_SEND=$'SURFACE_TREE_TEST\n'
export POST_READY_EXPECT=RAYOS_LINUX_SURFACE_TREE_END

export SURFACE_BRIDGE_OUT_DIR="$BRIDGE_DIR"
export MIRROR_SERIAL=0
export PASS_STDIN=0

export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Starting surface tree headless test..." >&2

echo "Log: $LOG_FILE" >&2

echo "Bridge: $BRIDGE_DIR" >&2

python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"

BRIDGE_DIR="$BRIDGE_DIR" python3 - <<'PY'
import json
import os
from pathlib import Path

bridge = Path(os.environ["BRIDGE_DIR"])
reg = json.loads((bridge / "registry.json").read_text(encoding="utf-8"))

windows = reg.get("windows", {})
assert "win-10" in windows and "win-11" in windows, windows.keys()

w10 = windows["win-10"]
w11 = windows["win-11"]

assert w11.get("parent_window_id") == "win-10", w11
assert "win-11" in (w10.get("children") or []), w10

s10 = set(w10.get("states") or [])
s11 = set(w11.get("states") or [])
assert "maximized" in s10, w10
assert "popup" in s11 and "modal" in s11, w11

print("OK")
PY

echo "PASS: window tree + states asserted" >&2
