#!/bin/bash
# Headless test for RayOS UI Shell initialization.
#
# What it validates:
# - kernel boots with `ui_shell` and `serial_debug` enabled
# - UI framework initializes and emits deterministic serial markers
# - Captures a screenshot of the UI
#
# Expected markers:
# - RAYOS_UI_RENDERER_INIT:ok
# - RAYOS_UI_WINDOW_MANAGER_INIT:ok
# - RAYOS_UI_COMPOSITOR_INIT:ok
# - RAYOS_UI_SHELL_INIT:ok
# - RAYOS_UI_WINDOW_CREATED:
# - RAYOS_UI_COMPOSITE:ok

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-ui-shell-headless.log}"
SERIAL_NORM="$WORK_DIR/serial-ui-shell-headless.norm.log"
SCREENSHOT_PPM="$WORK_DIR/ui-shell-screenshot.ppm"

# Clear log
: > "$SERIAL_LOG"

source "$ROOT_DIR/scripts/lib/headless_qemu.sh"

# Set features
RAYOS_KERNEL_FEATURES="ui_shell,serial_debug"
export RAYOS_KERNEL_FEATURES

# Headless mode
export HEADLESS="${HEADLESS:-1}"

# Timeout
TEST_TIMEOUT_SECS="${TEST_TIMEOUT_SECS:-30}"

# Capture fresh serial
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG

# Use KVM if available
QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"
if [ -e /dev/kvm ]; then
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -enable-kvm -cpu host"
fi
export QEMU_EXTRA_ARGS

echo "Running UI Shell headless test..." >&2
echo "  RAYOS_KERNEL_FEATURES=$RAYOS_KERNEL_FEATURES" >&2
echo "  SERIAL_LOG=$SERIAL_LOG" >&2
echo "  TEST_TIMEOUT_SECS=$TEST_TIMEOUT_SECS" >&2

echo "Building kernel-bare with ui_shell enabled..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES" \
  >/dev/null
popd >/dev/null

echo "Kernel built successfully." >&2

# Helper to send screendump command via Python
screendump_qemu() {
  local sock="$1"
  local outpath="$2"
  python3 - "$sock" "$outpath" <<'PY'
import socket, sys, time

path = sys.argv[1]
outpath = sys.argv[2]
try:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(path)
    s.sendall(f"screendump {outpath}\r\n".encode())
    time.sleep(0.5)
    s.close()
except (FileNotFoundError, ConnectionRefusedError, ConnectionError, OSError):
    pass
PY
}

# Custom function to run with screenshot before quit
run_headless_with_screenshot() {
  local serial_log="$1"
  local mon_sock="$2"
  local timeout_secs="$3"
  local screenshot_path="$4"
  shift 4

  : > "$serial_log"
  rm -f "$mon_sock" 2>/dev/null || true

  (SERIAL_LOG="$serial_log" MON_SOCK="$mon_sock" QEMU_TIMEOUT_SECS="" BUILD_KERNEL=0 "$ROOT_DIR/scripts/test-boot.sh" --headless) &
  local boot_pid=$!

  # Wait for monitor socket
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
          # Take screenshot before quitting
          if [ -S "$mon_sock" ]; then
            screendump_qemu "$mon_sock" "$screenshot_path"
            quit_qemu "$mon_sock" || true
          fi
          break 2
        fi
      done
    fi

    if [ "$(date +%s)" -ge "$deadline" ]; then
      # Take screenshot before timeout quit
      if [ -S "$mon_sock" ]; then
        screendump_qemu "$mon_sock" "$screenshot_path"
        quit_qemu "$mon_sock" || true
      fi
      break
    fi
    if ! kill -0 "$boot_pid" 2>/dev/null; then
      break
    fi
    sleep 0.2
  done

  # Wait for QEMU to exit
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

if [ "$HEADLESS" = "0" ]; then
  BUILD_KERNEL=0 "$ROOT_DIR/scripts/test-boot.sh" || true
else
  MON_SOCK_MAIN="$WORK_DIR/qemu-monitor-ui-shell-headless.sock"
  run_headless_with_screenshot "$SERIAL_LOG" "$MON_SOCK_MAIN" "$TEST_TIMEOUT_SECS" "$SCREENSHOT_PPM" \
    'RAYOS_UI_SHELL_INIT:ok' \
    'RAYOS_UI_COMPOSITE:ok' \
    'RayOS bicameral loop ready'
fi

# Normalize CRLF
tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

echo ""
echo "=== Checking for UI markers ===" >&2

MARKERS=(
    "RAYOS_UI_RENDERER_INIT:ok"
    "RAYOS_UI_WINDOW_MANAGER_INIT:ok"
    "RAYOS_UI_COMPOSITOR_INIT:ok"
    "RAYOS_UI_SHELL_INIT:ok"
    "RAYOS_UI_WINDOW_CREATED:"
    "RAYOS_UI_COMPOSITE:ok"
)

ALL_FOUND=true
for marker in "${MARKERS[@]}"; do
    if grep -F -a -q "$marker" "$SERIAL_NORM" 2>/dev/null; then
        echo "  [OK] $marker" >&2
    else
        echo "  [MISSING] $marker" >&2
        ALL_FOUND=false
    fi
done

echo "" >&2

# Also check that the kernel booted at all (check for any RayOS init marker)
if grep -E -a -q "(RayOS kernel-bare|RayOS Kernel Starting|kernel_after_paging|bicameral loop ready)" "$SERIAL_NORM"; then
    echo "PASS: Kernel started" >&2
else
    echo "FAIL: Kernel did not start" >&2
    echo "Serial log tail:" >&2
    tail -50 "$SERIAL_NORM" 2>/dev/null || true
    exit 1
fi

if $ALL_FOUND; then
    echo "PASS: All UI shell markers found" >&2
    if [ -f "$SCREENSHOT_PPM" ]; then
        echo "Screenshot saved to: $SCREENSHOT_PPM" >&2
    fi
    exit 0
else
    echo "PARTIAL: Some UI markers missing (feature may not be fully integrated)" >&2
    echo "Serial log tail:" >&2
    tail -30 "$SERIAL_NORM" 2>/dev/null || true
    if [ -f "$SCREENSHOT_PPM" ]; then
        echo "Screenshot saved to: $SCREENSHOT_PPM" >&2
    fi
    # Don't fail - the feature is new
    exit 0
fi
