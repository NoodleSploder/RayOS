#!/bin/bash
# Headless smoke: boot RayOS with in-kernel VMM + Linux guest + virtio-gpu,
# then inject the interactive command "show linux desktop" via the QEMU monitor.
#
# This validates the *user-driven* presentation path (no autostart feature gate):
# - RayOS reaches the prompt
# - command injection triggers the in-OS Linux guest to run
# - virtio-gpu scanout publishes into RayOS
# - first-frame marker is emitted

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-linux-desktop-native-command-present.log}"
TIMEOUT_SECS="${TIMEOUT_SECS:-120}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-vmm-linux-desktop-native-command-present.sock}"
CMDLINE_FILE="$WORK_DIR/vmm-linux-desktop-native-command-present-cmdline.txt"
MON_LOG="${MON_LOG:-$WORK_DIR/monitor-vmm-linux-desktop-native-command-present.log}"

: > "$SERIAL_LOG"
: > "$MON_LOG"

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

BASE_CMDLINE="console=ttyS0,115200n8 earlycon=uart,io,0x3f8,115200n8 rdinit=/rayos_init ignore_loglevel loglevel=7 panic=-1"
# Note: address must match the in-kernel MMIO mapping.
echo "$BASE_CMDLINE virtio_mmio.device=0x1000@0x10001000:5" > "$CMDLINE_FILE"

export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_linux_guest,vmm_virtio_gpu}"

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

wait_for_log() {
  local needle="$1"
  local tmo="$2"
  local start
  start=$(date +%s)
  while true; do
    if grep -F -a "$needle" "$SERIAL_LOG" >/dev/null 2>&1; then
      return 0
    fi
    local now
    now=$(date +%s)
    if [ $((now - start)) -ge "$tmo" ]; then
      return 1
    fi
    sleep 0.1
  done
}

send_monitor_cmds_py() {
  # Reads commands from stdin (one per line) and sends them to the HMP monitor socket.
  python3 -c '
import socket
import sys
import time

sock_path = sys.argv[1]
cmds = [line.strip("\n") for line in sys.stdin.read().splitlines() if line.strip()]

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1.0)
s.connect(sock_path)

# Drain banner/prompt.
s.settimeout(0.2)
try:
  s.recv(4096)
except Exception:
  pass

def drain():
  out = []
  while True:
    try:
      s.settimeout(0.15)
      chunk = s.recv(4096)
      if not chunk:
        break
      out.append(chunk)
    except Exception:
      break
  if out:
    sys.stdout.write(b"".join(out).decode("utf-8", errors="replace"))

for cmd in cmds:
  s.sendall((cmd + "\r\n").encode("ascii"))
  time.sleep(0.05)
  drain()

drain()
s.close()
' "$MON_SOCK"
}

cleanup() {
  # Best-effort quit via monitor.
  if [ -S "$MON_SOCK" ]; then
    python3 - "$MON_SOCK" >/dev/null 2>&1 <<'PY' || true
import socket, sys
path = sys.argv[1]
try:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(path)
    s.sendall(b"quit\r\n")
    s.close()
except Exception:
    pass
PY
  fi
  kill "$BOOT_PID" 2>/dev/null || true
}
trap cleanup EXIT

"$ROOT_DIR/scripts/test-boot.sh" --headless &
BOOT_PID=$!

if ! wait_for_log "RayOS bicameral loop ready" "$TIMEOUT_SECS"; then
  echo "FAIL: RayOS did not reach prompt readiness" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

# Send the command. Use "linu" to tolerate the known 'x' key issue.
{
  echo "info version"
  echo "sendkey s"; echo "sendkey h"; echo "sendkey o"; echo "sendkey w"
  echo "sendkey spc"
  echo "sendkey l"; echo "sendkey i"; echo "sendkey n"; echo "sendkey u"
  echo "sendkey spc"
  echo "sendkey d"; echo "sendkey e"; echo "sendkey s"; echo "sendkey k"; echo "sendkey t"; echo "sendkey o"; echo "sendkey p"
  echo "sendkey ret"
} | send_monitor_cmds_py >> "$MON_LOG" 2>&1 || true

PRESENTED_MARKER="RAYOS_HOST_EVENT_V0:LINUX_PRESENTATION:PRESENTED"
if ! wait_for_log "$PRESENTED_MARKER" "$TIMEOUT_SECS"; then
  echo "FAIL: did not observe show command marker ($PRESENTED_MARKER)" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  echo "Monitor log: $MON_LOG" >&2
  tail -n 200 "$MON_LOG" 2>/dev/null || true
  exit 1
fi

NEED1="RAYOS_VMM:LINUX:READY"
NEED2="RAYOS_LINUX_DESKTOP_PRESENTED"
NEED3="RAYOS_LINUX_DESKTOP_FIRST_FRAME"

if ! wait_for_log "$NEED1" "$TIMEOUT_SECS"; then
  echo "FAIL: did not observe $NEED1" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_log "$NEED2" "$TIMEOUT_SECS"; then
  echo "FAIL: did not observe $NEED2" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_log "$NEED3" "$TIMEOUT_SECS"; then
  echo "FAIL: did not observe $NEED3" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "PASS: interactive show linux desktop triggers in-OS presentation"
echo "Serial log: $SERIAL_LOG"
echo "Monitor log: $MON_LOG"
