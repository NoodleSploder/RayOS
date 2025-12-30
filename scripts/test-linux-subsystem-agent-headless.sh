#!/bin/bash
# Headless Linux guest-agent smoke test (Option D Step 3).
# - Boots Alpine netboot kernel+initramfs under QEMU
# - Layers a tiny RayOS guest agent into initramfs
# - Verifies the guest prints RAYOS_LINUX_AGENT_READY and responds to PING

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

TIMEOUT_SECS="${TIMEOUT_SECS:-45}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

LOG_FILE="${LOG_FILE:-$WORK_DIR/linux-subsystem-agent-headless.log}"

rm -f "$LOG_FILE" 2>/dev/null || true

export WORK_DIR TIMEOUT_SECS QEMU_BIN LOG_FILE

# Force the runner into agent-initrd mode.
export USE_AGENT_INITRD=1
export READY_MARKER=RAYOS_LINUX_AGENT_READY
export INJECT_READY_MARKER=0
export POST_READY_SEND=$'PING\n'
export POST_READY_EXPECT=RAYOS_LINUX_AGENT_PONG

# Ensure the agent init runs first.
export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Starting Linux subsystem guest-agent bring-up..." >&2

echo "QEMU: $QEMU_BIN" >&2

echo "Timeout: ${TIMEOUT_SECS}s" >&2

echo "Log: $LOG_FILE" >&2

python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"

NORM="$WORK_DIR/linux-subsystem-agent-headless.norm.log"
tr -d '\r' < "$LOG_FILE" > "$NORM" 2>/dev/null || true

if grep -F -a -q "RAYOS_LINUX_AGENT_READY" "$NORM" && grep -F -a -q "RAYOS_LINUX_AGENT_PONG" "$NORM"; then
  echo "PASS: observed agent ready + pong" >&2
  exit 0
fi

echo "FAIL: missing agent markers" >&2

tail -n 200 "$NORM" 2>/dev/null || true

exit 1
