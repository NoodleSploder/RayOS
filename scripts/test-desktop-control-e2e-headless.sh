#!/bin/bash
# End-to-end desktop control smoke test (headless).
#
# Boots RayOS kernel-bare (no GUI), triggers the host Linux desktop bridge via
# "show linux desktop", injects text+key via the RayOS prompt, then requests a
# shutdown and quits the RayOS QEMU.
#
# This validates:
# - RayOS -> host event emission (SHOW_LINUX_DESKTOP / LINUX_SENDTEXT / LINUX_SENDKEY / LINUX_SHUTDOWN)
# - host bridge parsing + ACK markers
# - linux desktop VM monitor socket creation (basic liveness)
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${WORK_DIR+x}" ]; then
  # Use a unique WORK_DIR to avoid lock collisions with other running test-boot.sh sessions.
  if command -v mktemp >/dev/null 2>&1; then
    WORK_DIR="$(mktemp -d "$ROOT_DIR/build/e2e-desktop-control.XXXXXX")"
  else
    WORK_DIR="$ROOT_DIR/build/e2e-desktop-control.$(date +%s).$$"
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

echo "[e2e] Starting RayOS headless QEMU..." >&2
echo "[e2e] WORK_DIR=$WORK_DIR" >&2
(
  cd "$ROOT_DIR"
  WORK_DIR="$WORK_DIR" \
  SERIAL_LOG="$SERIAL_LOG" \
  MON_SOCK="$MON_SOCK" \
  HEADLESS=1 \
  ENABLE_LINUX_DESKTOP_BRIDGE=1 \
  AUTO_GENERATE_MODEL_BIN=0 \
  ./scripts/test-boot.sh
) &
RAYOS_TEST_PID="$!"

cleanup() {
  quit_rayos_qemu
  kill "$RAYOS_TEST_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo "[e2e] Waiting for RayOS prompt..." >&2
if ! wait_for_log "RayOS bicameral loop ready" "$TIMEOUT_SECS"; then
  echo "FAIL: RayOS did not reach prompt readiness within ${TIMEOUT_SECS}s" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "[e2e] Requesting Linux desktop..." >&2
send_to_rayos "show linux desktop"
if ! wait_for_log "RAYOS_HOST_ACK:SHOW_LINUX_DESKTOP:ok" "$TIMEOUT_SECS"; then
  echo "FAIL: missing SHOW_LINUX_DESKTOP ACK" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "[e2e] Waiting for Linux desktop monitor socket..." >&2
start="$(date +%s)"
while [ ! -S "$DESKTOP_MON_SOCK" ]; do
  now="$(date +%s)"
  if [ $((now - start)) -ge "$TIMEOUT_SECS" ]; then
    echo "FAIL: linux desktop monitor socket not created: $DESKTOP_MON_SOCK" >&2
    tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
  fi
  sleep 0.1
done

echo "[e2e] Typing into Linux desktop via RayOS bridge..." >&2
send_to_rayos "type echo ok"
if ! wait_for_log "RAYOS_HOST_ACK:LINUX_SENDTEXT:ok:sent" "$TIMEOUT_SECS"; then
  echo "FAIL: missing LINUX_SENDTEXT ACK" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

send_to_rayos "press enter"
if ! wait_for_log "RAYOS_HOST_ACK:LINUX_SENDKEY:ok:sent" "$TIMEOUT_SECS"; then
  echo "FAIL: missing LINUX_SENDKEY ACK" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "[e2e] Shutting down Linux desktop..." >&2
send_to_rayos "shutdown linux"
if ! wait_for_log "RAYOS_HOST_ACK:LINUX_SHUTDOWN:ok" "$TIMEOUT_SECS"; then
  echo "FAIL: missing LINUX_SHUTDOWN ACK" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

echo "[e2e] Quitting RayOS QEMU..." >&2
quit_rayos_qemu

set +e
wait "$RAYOS_TEST_PID"
rc=$?
set -e

if [ "$rc" -ne 0 ]; then
  echo "WARN: RayOS test process exited with rc=$rc (continuing)" >&2
fi

echo "PASS: desktop control e2e markers observed" >&2
exit 0
