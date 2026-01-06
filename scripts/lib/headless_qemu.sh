#!/bin/bash

# Shared helpers for headless QEMU runs driven by monitor socket + serial markers.
# Intended to be sourced by scripts under /scripts.

# This file is meant to be sourced; it must not mutate the caller's shell options.

_rayos_root_dir() {
  if [ -n "${ROOT_DIR:-}" ]; then
    printf '%s' "$ROOT_DIR"
    return 0
  fi
  # This file lives at scripts/lib/headless_qemu.sh
  (cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
}

quit_qemu() {
  local sock="$1"
  python3 - "$sock" <<'PY'
import socket, sys

path = sys.argv[1]
try:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(path)
    s.sendall(b"quit\r\n")
    s.close()
except (FileNotFoundError, ConnectionRefusedError, ConnectionError, OSError):
    # Best-effort shutdown. The VM may have already exited.
    pass
PY
}

run_headless_boot_until() {
  # Run scripts/test-boot.sh headless with a dedicated monitor socket and
  # stop it as soon as at least one of the provided marker regexes matches.
  #
  # Args:
  #   $1 serial_log
  #   $2 mon_sock
  #   $3 timeout_secs
  #   $4.. marker regexes (grep -E)
  local serial_log="$1"
  local mon_sock="$2"
  local timeout_secs="$3"
  shift 3

  : > "$serial_log"
  rm -f "$mon_sock" 2>/dev/null || true

  local root_dir
  root_dir="$(_rayos_root_dir)"

  (SERIAL_LOG="$serial_log" MON_SOCK="$mon_sock" QEMU_TIMEOUT_SECS="" BUILD_KERNEL=0 "$root_dir/scripts/test-boot.sh" --headless) &
  local boot_pid=$!

  local mon_wait_deadline=$(( $(date +%s) + 20 ))
  while [ ! -S "$mon_sock" ]; do
    if ! kill -0 "$boot_pid" 2>/dev/null; then
      break
    fi
    if [ "$(date +%s)" -ge "$mon_wait_deadline" ]; then
      break
    fi
    sleep 0.1
  done

  local deadline=$(( $(date +%s) + timeout_secs ))
  while true; do
    if [ -f "$serial_log" ]; then
      for pat in "$@"; do
        if grep -E -a -q "$pat" "$serial_log"; then
          if [ -S "$mon_sock" ]; then
            quit_qemu "$mon_sock" || true
          fi
          break 2
        fi
      done
    fi

    if [ "$(date +%s)" -ge "$deadline" ]; then
      if [ -S "$mon_sock" ]; then
        quit_qemu "$mon_sock" || true
      fi
      break
    fi
    if ! kill -0 "$boot_pid" 2>/dev/null; then
      break
    fi
    sleep 0.2
  done

  local wait_deadline=$(( $(date +%s) + 10 ))
  while kill -0 "$boot_pid" 2>/dev/null; do
    if [ "$(date +%s)" -ge "$wait_deadline" ]; then
      kill "$boot_pid" 2>/dev/null || true
      break
    fi
    sleep 0.1
  done

  wait "$boot_pid" 2>/dev/null || true
}

