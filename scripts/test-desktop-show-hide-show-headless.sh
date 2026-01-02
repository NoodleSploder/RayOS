#!/bin/bash
# Desktop show→hide→show smoke test (headless).
#
# Validates v0 "presentation gating" semantics for the Linux desktop bridge:
# - show linux desktop (starts VM)
# - hide linux desktop (stops VM, keeps persistent disk)
# - show linux desktop again (restarts)
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${WORK_DIR+x}" ]; then
  if command -v mktemp >/dev/null 2>&1; then
    WORK_DIR="$(mktemp -d "$ROOT_DIR/build/e2e-desktop-show-hide-show.XXXXXX")"
  else
    WORK_DIR="$ROOT_DIR/build/e2e-desktop-show-hide-show.$(date +%s).$$"
    mkdir -p "$WORK_DIR"
  fi
else
  mkdir -p "$WORK_DIR"
fi

TIMEOUT_SECS="${TIMEOUT_SECS:-120}"

SERIAL_LOG="$WORK_DIR/serial-rayos-headless.log"
MON_SOCK="$WORK_DIR/qemu-monitor-rayos-headless.sock"
DESKTOP_MON_SOCK="$WORK_DIR/linux-desktop-monitor.sock"

rm -f "$SERIAL_LOG" "$MON_SOCK" 2>/dev/null || true

wait_for_log() {
  local needle="$1"
  local timeout="$2"
  local start
  start="$(date +%s)"
  while true; do
    if [ -f "$SERIAL_LOG" ] && tr -d '\r' <"$SERIAL_LOG" | grep -F -a -q "$needle"; then
      return 0
    fi
    local now
    now="$(date +%s)"
    if [ $((now - start)) -ge "$timeout" ]; then
      return 1
    fi
    sleep 0.1
  done
}

wait_for_sock() {
  local sock="$1"
  local timeout="$2"
  local start
  start="$(date +%s)"
  while true; do
    if [ -S "$sock" ]; then
      return 0
    fi
    local now
    now="$(date +%s)"
    if [ $((now - start)) -ge "$timeout" ]; then
      return 1
    fi
    sleep 0.1
  done
}

wait_for_sock_gone() {
  local sock="$1"
  local timeout="$2"
  local start
  start="$(date +%s)"
  while true; do
    if [ ! -S "$sock" ]; then
      return 0
    fi
    local now
    now="$(date +%s)"
    if [ $((now - start)) -ge "$timeout" ]; then
      return 1
    fi
    sleep 0.1
  done
}

send_to_rayos() {
  local text="$1"
  python3 "$ROOT_DIR/scripts/qemu-sendtext.py" \
    --sock "$MON_SOCK" \
    --text "$text" \
    --wait 1.5 \
    >/dev/null 2>&1 || true
}

quit_rayos_qemu() {
  if [ ! -S "$MON_SOCK" ]; then
    return 0
  fi
  python3 - <<'PY' "$MON_SOCK" >/dev/null 2>&1 || true
import socket, sys, time
sock_path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1.0)
s.connect(sock_path)
try:
    s.settimeout(0.2)
    s.recv(4096)
except Exception:
    pass
s.sendall(b"quit\r\n")
time.sleep(0.05)
s.close()
PY
}

echo "[show-hide-show] WORK_DIR=$WORK_DIR" >&2
(
  cd "$ROOT_DIR"
  WORK_DIR="$WORK_DIR" \
  SERIAL_LOG="$SERIAL_LOG" \
  MON_SOCK="$MON_SOCK" \
  HEADLESS=1 \
  ENABLE_HOST_DESKTOP_BRIDGE=1 \
  LINUX_DESKTOP_GLOBAL_LOCK="$WORK_DIR/rayos-linux-desktop-auto.lock" \
  AUTO_GENERATE_MODEL_BIN=0 \
  INJECT_ACK_TO_GUEST=0 \
  ./scripts/test-boot.sh
) &
RAYOS_PID="$!"

cleanup() {
  quit_rayos_qemu
  kill "$RAYOS_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo "[show-hide-show] Waiting for RayOS prompt..." >&2
if ! wait_for_log "RayOS bicameral loop ready" "$TIMEOUT_SECS"; then
  echo "FAIL: RayOS did not reach prompt readiness within ${TIMEOUT_SECS}s" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "[show-hide-show] show linux desktop..." >&2
send_to_rayos "show linux desktop"
if ! wait_for_log "RAYOS_HOST_ACK:SHOW_LINUX_DESKTOP:ok" "$TIMEOUT_SECS"; then
  echo "FAIL: missing SHOW_LINUX_DESKTOP ok ACK" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_sock "$DESKTOP_MON_SOCK" "$TIMEOUT_SECS"; then
  echo "FAIL: desktop monitor sock not created: $DESKTOP_MON_SOCK" >&2
  exit 1
fi

echo "[show-hide-show] hide linux desktop..." >&2
send_to_rayos "hide linux desktop"
if ! wait_for_log "RAYOS_HOST_ACK:HIDE_LINUX_DESKTOP:ok:stopped" "$TIMEOUT_SECS"; then
  echo "FAIL: missing HIDE_LINUX_DESKTOP ok ACK" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_sock_gone "$DESKTOP_MON_SOCK" "$TIMEOUT_SECS"; then
  echo "FAIL: desktop monitor sock did not disappear after hide" >&2
  exit 1
fi

echo "[show-hide-show] show linux desktop again..." >&2
send_to_rayos "show linux desktop"
if ! wait_for_log "RAYOS_HOST_ACK:SHOW_LINUX_DESKTOP:ok" "$TIMEOUT_SECS"; then
  echo "FAIL: missing SHOW_LINUX_DESKTOP ok ACK (second)" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_sock "$DESKTOP_MON_SOCK" "$TIMEOUT_SECS"; then
  echo "FAIL: desktop monitor sock not created on second show" >&2
  exit 1
fi

echo "PASS: show→hide→show worked" >&2
exit 0
