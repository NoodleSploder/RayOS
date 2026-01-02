#!/bin/bash
# Windows desktop control preflight smoke test (headless).
#
# This does not require a real Windows disk image. It validates that:
# - RayOS emits the Windows host events
# - The host bridge responds with deterministic ACK error markers when prerequisites are missing
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${WORK_DIR+x}" ]; then
  if command -v mktemp >/dev/null 2>&1; then
    WORK_DIR="$(mktemp -d "$ROOT_DIR/build/e2e-windows-preflight.XXXXXX")"
  else
    WORK_DIR="$ROOT_DIR/build/e2e-windows-preflight.$(date +%s).$$"
    mkdir -p "$WORK_DIR"
  fi
else
  mkdir -p "$WORK_DIR"
fi

TIMEOUT_SECS="${TIMEOUT_SECS:-90}"

SERIAL_LOG="$WORK_DIR/serial-rayos-headless.log"
MON_SOCK="$WORK_DIR/qemu-monitor-rayos-headless.sock"

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

echo "[win-preflight] WORK_DIR=$WORK_DIR" >&2
(
  cd "$ROOT_DIR"
  WORK_DIR="$WORK_DIR" \
  SERIAL_LOG="$SERIAL_LOG" \
  MON_SOCK="$MON_SOCK" \
  HEADLESS=1 \
  ENABLE_HOST_DESKTOP_BRIDGE=1 \
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

echo "[win-preflight] Waiting for RayOS prompt..." >&2
if ! wait_for_log "RayOS bicameral loop ready" "$TIMEOUT_SECS"; then
  echo "FAIL: RayOS did not reach prompt readiness within ${TIMEOUT_SECS}s" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "[win-preflight] Requesting Windows desktop without WINDOWS_DISK..." >&2
send_to_rayos "show windows desktop"
if ! wait_for_log "RAYOS_HOST_ACK:SHOW_WINDOWS_DESKTOP:err:missing_WINDOWS_DISK_env" "$TIMEOUT_SECS"; then
  echo "FAIL: missing expected SHOW_WINDOWS_DESKTOP preflight ACK error" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "[win-preflight] Sending input without Windows running..." >&2
send_to_rayos "windows type echo ok"
if ! wait_for_log "RAYOS_HOST_ACK:WINDOWS_SENDTEXT:err:desktop_not_running" "$TIMEOUT_SECS"; then
  echo "FAIL: missing expected WINDOWS_SENDTEXT desktop_not_running ACK" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

send_to_rayos "windows press enter"
if ! wait_for_log "RAYOS_HOST_ACK:WINDOWS_SENDKEY:err:desktop_not_running" "$TIMEOUT_SECS"; then
  echo "FAIL: missing expected WINDOWS_SENDKEY desktop_not_running ACK" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "PASS: Windows desktop preflight ACKs observed" >&2
exit 0
