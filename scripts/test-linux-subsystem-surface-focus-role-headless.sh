#!/bin/bash
# Surface focus + role headless test (Step 5 scaffolding).
#
# - Boots the RayOS guest agent (rdinit=/rayos_init)
# - Enables the live SurfaceBridge registry output
# - Sends SURFACE_FOCUS_ROLE_TEST
# - Asserts:
#   - focused_window_id is win-2
#   - surface 2 role is popup
#   - z_order ends with win-2

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

TIMEOUT_SECS="${TIMEOUT_SECS:-45}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

LOG_FILE="${LOG_FILE:-$WORK_DIR/linux-subsystem-surface-focus-role.log}"
BRIDGE_DIR="${BRIDGE_DIR:-$WORK_DIR/linux-subsystem-surface-focus-role-bridge}"

rm -f "$LOG_FILE" 2>/dev/null || true
rm -rf "$BRIDGE_DIR" 2>/dev/null || true

export WORK_DIR TIMEOUT_SECS QEMU_BIN LOG_FILE

export USE_AGENT_INITRD=1
export READY_MARKER=RAYOS_LINUX_AGENT_READY
export INJECT_READY_MARKER=0
export POST_READY_SEND=$'SURFACE_FOCUS_ROLE_TEST\n'
export POST_READY_EXPECT=RAYOS_LINUX_SURFACE_FOCUS_ROLE_END

export SURFACE_BRIDGE_OUT_DIR="$BRIDGE_DIR"
export MIRROR_SERIAL=0
export PASS_STDIN=0

export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Starting surface focus+role headless test..." >&2

echo "Log: $LOG_FILE" >&2

echo "Bridge: $BRIDGE_DIR" >&2

python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"

BRIDGE_DIR="$BRIDGE_DIR" python3 - <<'PY'
import json
import os
from pathlib import Path

bridge = Path(os.environ["BRIDGE_DIR"])
reg = json.loads((bridge / "registry.json").read_text(encoding="utf-8"))

assert reg.get("focused_window_id") == "win-2", reg
assert reg["surfaces"]["2"]["role"] == "popup", reg["surfaces"]["2"]

z = reg.get("z_order", [])
assert z and z[-1] == "win-2", z

print("OK")
PY

echo "PASS: focus + role asserted" >&2
