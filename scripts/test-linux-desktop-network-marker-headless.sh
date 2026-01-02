#!/bin/bash
# Validate deterministic networking policy markers emitted by the Linux desktop launcher.
#
# This does not boot RayOS/QEMU; it verifies the launcher emits:
#   RAYOS_HOST_MARKER:LINUX_DESKTOP_NETWORK:<on|off>:<reason>
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

mk_work_dir() {
  if command -v mktemp >/dev/null 2>&1; then
    mktemp -d "$ROOT_DIR/build/e2e-linux-desktop-net-marker.XXXXXX"
  else
    local d="$ROOT_DIR/build/e2e-linux-desktop-net-marker.$(date +%s).$$"
    mkdir -p "$d"
    printf "%s\n" "$d"
  fi
}

assert_log_contains() {
  local log="$1"
  local needle="$2"
  if ! tr -d '\r' <"$log" | grep -F -a -q "$needle"; then
    echo "FAIL: expected marker not found: $needle" >&2
    echo "---- log ($log) ----" >&2
    tail -n 200 "$log" >&2 || true
    exit 1
  fi
}

echo "[net-marker] Case 1: explicit off (LINUX_NET=0)..." >&2
WORK_DIR="$(mk_work_dir)"
SERIAL_LOG="$WORK_DIR/serial-marker.log"
rm -f "$SERIAL_LOG" 2>/dev/null || true

WORK_DIR="$WORK_DIR" \
RAYOS_SERIAL_LOG="$SERIAL_LOG" \
LINUX_NET=0 \
EMIT_POLICY_MARKERS_ONLY=1 \
"$ROOT_DIR/scripts/run-linux-subsystem-desktop-auto.sh" \
  >/dev/null 2>&1 || true

assert_log_contains "$SERIAL_LOG" "RAYOS_HOST_MARKER:LINUX_DESKTOP_NETWORK:off:explicit"

echo "[net-marker] Case 2: auto provisioning enables net (no LINUX_NET)..." >&2
WORK_DIR="$(mk_work_dir)"
SERIAL_LOG="$WORK_DIR/serial-marker.log"
rm -f "$SERIAL_LOG" 2>/dev/null || true

WORK_DIR="$WORK_DIR" \
RAYOS_SERIAL_LOG="$SERIAL_LOG" \
EMIT_POLICY_MARKERS_ONLY=1 \
"$ROOT_DIR/scripts/run-linux-subsystem-desktop-auto.sh" \
  >/dev/null 2>&1 || true

assert_log_contains "$SERIAL_LOG" "RAYOS_HOST_MARKER:LINUX_DESKTOP_NETWORK:on:auto_provisioning"

echo "PASS: linux desktop network markers observed" >&2
exit 0
