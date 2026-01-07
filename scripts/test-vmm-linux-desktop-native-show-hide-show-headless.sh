#!/bin/bash
# Headless smoke: boot RayOS with in-kernel VMM + Linux guest + virtio-gpu (no host bridge),
# then inject:
#   1) "show linux desktop"
#   2) "hide linux desktop"
#   3) "show linux desktop"
# via QEMU monitor sendkey.
#
# This validates the interactive presentation toggling contract:
# - Hide/show is presentation-only (does not stop the VM)
# - After re-show, scanout is still publishable and first frame arrives

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-linux-desktop-native-show-hide-show.log}"
TIMEOUT_SECS="${TIMEOUT_SECS:-140}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-vmm-linux-desktop-native-show-hide-show.sock}"
CMDLINE_FILE="$WORK_DIR/vmm-linux-desktop-native-show-hide-show-cmdline.txt"
MON_LOG="${MON_LOG:-$WORK_DIR/monitor-vmm-linux-desktop-native-show-hide-show.log}"

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

BASE_CMDLINE="console=ttyS0,115200n8 earlycon=uart,io,0x3f8,115200n8 rdinit=/rayos_init ignore_loglevel loglevel=7 panic=-1 i8042.nokbd=1 i8042.noaux=1"
# virtio-mmio device declaration; address must match the in-kernel MMIO mapping.
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

wait_for_log_after_line() {
  local needle="$1"
  local tmo="$2"
  local after_line="$3"
  local start
  start=$(date +%s)
  while true; do
    if [ -f "$SERIAL_LOG" ]; then
      # Only search newly appended lines.
      if tail -n +"$((after_line + 1))" "$SERIAL_LOG" 2>/dev/null | grep -F -a "$needle" >/dev/null 2>&1; then
        return 0
      fi
    fi
    local now
    now=$(date +%s)
    if [ $((now - start)) -ge "$tmo" ]; then
      return 1
    fi
    sleep 0.1
  done
}

log_lines() {
  if [ -f "$SERIAL_LOG" ]; then
    wc -l < "$SERIAL_LOG" | tr -d ' '
  else
    echo 0
  fi
}

send_monitor_cmds_py() {
  # Reads commands from stdin (one per line) and sends them to the HMP monitor socket.
  python3 -c '
import socket
import sys
import time

sock_path = sys.argv[1]
cmds = [line.strip("\n") for line in sys.stdin.read().splitlines() if line.strip()]

deadline = time.time() + 5.0
while True:
  try:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(1.0)
    s.connect(sock_path)
    break
  except Exception:
    if time.time() >= deadline:
      raise
    time.sleep(0.05)

def drain(s):
  while True:
    try:
      s.settimeout(0.05)
      if not s.recv(4096):
        break
    except Exception:
      break

# Drain banner/prompt.
try:
  s.settimeout(0.2)
  s.recv(4096)
except Exception:
  pass

for cmd in cmds:
  s.sendall((cmd + "\r\n").encode("ascii"))
  sys.stdout.write("HMP_SENT:" + cmd + "\n")
  sys.stdout.flush()
  time.sleep(0.03)
  drain(s)

drain(s)
s.close()
' "$MON_SOCK"
}

send_show_linux_desktop() {
  # Use "linu" to tolerate the known 'x' key issue.
  {
    echo "sendkey s"; echo "sendkey h"; echo "sendkey o"; echo "sendkey w"
    echo "sendkey spc"
    echo "sendkey l"; echo "sendkey i"; echo "sendkey n"; echo "sendkey u"
    echo "sendkey spc"
    echo "sendkey d"; echo "sendkey e"; echo "sendkey s"; echo "sendkey k"; echo "sendkey t"; echo "sendkey o"; echo "sendkey p"
    echo "sendkey ret"
  } | send_monitor_cmds_py >> "$MON_LOG" 2>&1
}

send_hide_linux_desktop() {
  {
    echo "sendkey h"; echo "sendkey i"; echo "sendkey d"; echo "sendkey e"
    echo "sendkey spc"
    # Use "linu" to tolerate the known 'x' key issue.
    echo "sendkey l"; echo "sendkey i"; echo "sendkey n"; echo "sendkey u"
    echo "sendkey spc"
    echo "sendkey d"; echo "sendkey e"; echo "sendkey s"; echo "sendkey k"; echo "sendkey t"; echo "sendkey o"; echo "sendkey p"
    echo "sendkey ret"
  } | send_monitor_cmds_py >> "$MON_LOG" 2>&1
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

# SHOW #1
start1=$(log_lines)
send_show_linux_desktop

if ! wait_for_log_after_line "RAYOS_HOST_EVENT_V0:LINUX_PRESENTATION:PRESENTED" "$TIMEOUT_SECS" "$start1"; then
  echo "FAIL: missing Presented marker after first show" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_log "RAYOS_LINUX_DESKTOP_PRESENTED" "$TIMEOUT_SECS"; then
  echo "FAIL: missing RAYOS_LINUX_DESKTOP_PRESENTED" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_log "RAYOS_LINUX_DESKTOP_FIRST_FRAME" "$TIMEOUT_SECS"; then
  echo "FAIL: missing RAYOS_LINUX_DESKTOP_FIRST_FRAME" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

# HIDE
start2=$(log_lines)
send_hide_linux_desktop
if ! wait_for_log_after_line "RAYOS_HOST_EVENT_V0:LINUX_PRESENTATION:HIDDEN" "$TIMEOUT_SECS" "$start2"; then
  echo "FAIL: missing Hidden marker after hide" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

# SHOW #2
start3=$(log_lines)
send_show_linux_desktop
if ! wait_for_log_after_line "RAYOS_HOST_EVENT_V0:LINUX_PRESENTATION:PRESENTED" "$TIMEOUT_SECS" "$start3"; then
  echo "FAIL: missing Presented marker after second show" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

# Ensure we see a desktop-presented marker again after re-show.
if ! wait_for_log_after_line "RAYOS_LINUX_DESKTOP_PRESENTED" "$TIMEOUT_SECS" "$start3"; then
  echo "FAIL: missing RAYOS_LINUX_DESKTOP_PRESENTED after second show" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "PASS: show -> hide -> show toggles presentation and keeps VMM desktop alive"
echo "Serial log: $SERIAL_LOG"
echo "Monitor log: $MON_LOG"
