#!/bin/bash
# Live multi-surface bridge bring-up helper.
#
# Boots the Alpine netboot kernel + RayOS agent overlay (rdinit=/rayos_init),
# forwards your stdin to the guest agent, and writes a live surface/window registry
# plus per-surface frame payloads into SURFACE_BRIDGE_OUT_DIR.
#
# Usage:
#   ./run-linux-subsystem-surface-bridge.sh
#   # then type commands like:
#   #   PING
#   #   SURFACE_TEST
#   #   SURFACE_MULTI_TEST
#   #   SURFACE_LIFECYCLE_TEST
#   #   SURFACE_FOCUS_ROLE_TEST
#   #   SURFACE_TREE_TEST
#
# Outputs:
#   build/surface-bridge-live/registry.json
#   build/surface-bridge-live/frames/surface-<id>-seq-<n>.ppm

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

export WORK_DIR
export USE_AGENT_INITRD=1
export READY_MARKER=RAYOS_LINUX_AGENT_READY
export INJECT_READY_MARKER=0

export SURFACE_BRIDGE_OUT_DIR="${SURFACE_BRIDGE_OUT_DIR:-$WORK_DIR/surface-bridge-live}"
export MIRROR_SERIAL=1
export PASS_STDIN=1

# 0 disables the global timeout (interactive session).
export TIMEOUT_SECS="${TIMEOUT_SECS:-0}"

export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Surface bridge out: $SURFACE_BRIDGE_OUT_DIR" >&2

echo "Tip: type SURFACE_LIFECYCLE_TEST then Enter" >&2

exec python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"
