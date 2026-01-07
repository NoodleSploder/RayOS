#!/bin/bash
# Headless milestone smoke: run the Linux guest under the in-kernel VMX VMM with virtio-gpu
# enabled, and assert the RayOS-native scanout publication markers.
#
# This is the first “no host bridge” gate:
# - Linux runs under RayOS VMM (embedded/time-sliced)
# - virtio-gpu device model publishes a scanout
# - a first-frame marker is emitted when the guest flushes

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-linux-desktop-native-present.log}"
TIMEOUT_SECS="${TIMEOUT_SECS:-90}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-vmm-linux-desktop-native-present.sock}"
CMDLINE_FILE="$WORK_DIR/vmm-linux-desktop-native-present-cmdline.txt"

: > "$SERIAL_LOG"

ARTS_ENV=(
  WORK_DIR="$WORK_DIR"
  PREPARE_ONLY=1
  USE_AGENT_INITRD=1
)

ARTS="$(env "${ARTS_ENV[@]}" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py")"
KERNEL="$(printf "%s\n" "$ARTS" | sed -n 's/^KERNEL=//p' | head -n1)"
INITRD="$(printf "%s\n" "$ARTS" | sed -n 's/^INITRD=//p' | head -n1)"

if [ -z "$KERNEL" ] || [ ! -f "$KERNEL" ]; then
  echo "FAIL: missing KERNEL from run_linux_guest.py" >&2
  exit 1
fi
if [ -z "$INITRD" ] || [ ! -f "$INITRD" ]; then
  echo "FAIL: missing INITRD from run_linux_guest.py" >&2
  exit 1
fi

# Base cmdline + virtio-mmio device declaration.
# Note: size is 0x1000 (one page) and the address matches the in-kernel MMIO mapping.
BASE_CMDLINE="console=ttyS0,115200n8 earlycon=uart,io,0x3f8,115200n8 rdinit=/rayos_init ignore_loglevel loglevel=7 panic=-1"
echo "$BASE_CMDLINE virtio_mmio.device=0x1000@0x10001000:5" > "$CMDLINE_FILE"

export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_linux_guest,vmm_linux_desktop_autostart,vmm_virtio_gpu}"

export RAYOS_LINUX_GUEST_KERNEL_SRC="$KERNEL"
export RAYOS_LINUX_GUEST_INITRD_SRC="$INITRD"
export RAYOS_LINUX_GUEST_CMDLINE_SRC="$CMDLINE_FILE"

export HEADLESS=1
export QEMU_TIMEOUT_SECS="$TIMEOUT_SECS"
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG
export MON_SOCK

# Prefer KVM when available; otherwise request a VMX-capable CPU model.
if [ -z "${QEMU_EXTRA_ARGS:-}" ]; then
  if [ -e /dev/kvm ]; then
    export QEMU_EXTRA_ARGS="-enable-kvm -cpu host"
  else
    export QEMU_EXTRA_ARGS="-cpu qemu64,+vmx"
  fi
fi

export ENABLE_HOST_DESKTOP_BRIDGE=0
export PRELAUNCH_HIDDEN_DESKTOPS=0

"$ROOT_DIR/scripts/test-boot.sh" --headless &
BOOT_PID=$!

cleanup() {
  if [ -S "$MON_SOCK" ]; then
    python3 - <<'PY' "$MON_SOCK" >/dev/null 2>&1 || true
import socket, sys
sock_path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(0.5)
s.connect(sock_path)
try:
    s.settimeout(0.1)
    s.recv(4096)
except Exception:
    pass
s.sendall(b"quit\r\n")
s.close()
PY
  fi
  kill "$BOOT_PID" 2>/dev/null || true
}
trap cleanup EXIT

wait_for_log() {
  local needle="$1"
  local tmo="$2"
  local start
  start=$(date +%s)
  while true; do
    if grep -F "$needle" "$SERIAL_LOG" >/dev/null 2>&1; then
      return 0
    fi
    now=$(date +%s)
    if [ $((now - start)) -ge "$tmo" ]; then
      return 1
    fi
    sleep 0.2
  done
}

if ! wait_for_log "RayOS bicameral loop ready" "$TIMEOUT_SECS"; then
  echo "FAIL: RayOS did not reach prompt readiness" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

NEED1="RAYOS_VMM:LINUX:READY"
NEED2="RAYOS_LINUX_DESKTOP_PRESENTED"
NEED3="RAYOS_LINUX_DESKTOP_FIRST_FRAME"

if ! wait_for_log "$NEED1" "$TIMEOUT_SECS"; then
  echo "FAIL: did not observe $NEED1" >&2
  tail -n 250 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

# Let Linux run; virtio-gpu publish/first-frame markers are emitted by the in-kernel model.
if ! wait_for_log "$NEED2" "$TIMEOUT_SECS"; then
  echo "FAIL: did not observe $NEED2" >&2
  tail -n 250 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

if ! wait_for_log "$NEED3" "$TIMEOUT_SECS"; then
  echo "FAIL: did not observe $NEED3" >&2
  tail -n 300 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "PASS: observed $NEED2 and $NEED3" >&2
exit 0
