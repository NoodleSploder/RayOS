#!/bin/bash
# Headless dev scanout validation for RayOS native presentation bring-up.
#
# Boots kernel-bare under OVMF, injects "show linu desktop" via QEMU monitor
# sendkey, and verifies DEV_SCANOUT markers appear in the serial log.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TOOLCHAIN="${TOOLCHAIN:-nightly-2024-11-01-x86_64-unknown-linux-gnu}"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="${STAGE_DIR:-$WORK_DIR/dev-scanout-fat}"
BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi}"
KERNEL_BIN_SRC="${KERNEL_BIN_SRC:-$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare}"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-dev-scanout.log}"
QEMU_LOG="${QEMU_LOG:-$WORK_DIR/qemu-dev-scanout.log}"
MON_LOG="${MON_LOG:-$WORK_DIR/monitor-dev-scanout.log}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-dev-scanout.sock}"
PID_FILE="${PID_FILE:-$WORK_DIR/qemu-dev-scanout.pid}"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

TIMEOUT_SECS="${TIMEOUT_SECS:-20}"
AUTO_PRESENT_WAIT_SECS="${AUTO_PRESENT_WAIT_SECS:-2}"

# Extra validations (enabled by default for dev convenience).
TOGGLE_TEST="${TOGGLE_TEST:-1}"
AUTOHIDE_TEST="${AUTOHIDE_TEST:-0}"
AUTOHIDE_SECS="${AUTOHIDE_SECS:-2}"

cleanup() {
  if [ -f "$PID_FILE" ]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      sleep 0.2 || true
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
  rm -f "$PID_FILE" 2>/dev/null || true
  rm -f "$MON_SOCK" 2>/dev/null || true
}
trap cleanup EXIT

wait_for_log() {
  local needle="$1"
  local timeout_secs="$2"
  local start
  start=$(date +%s)
  while true; do
    if [ -f "$SERIAL_LOG" ] && grep -F -a -q "$needle" "$SERIAL_LOG" 2>/dev/null; then
      return 0
    fi
    local now
    now=$(date +%s)
    if [ $((now - start)) -ge "$timeout_secs" ]; then
      return 1
    fi
    sleep 0.05
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

# Read initial banner/prompt if any.
s.settimeout(0.2)
try:
  data = s.recv(4096)
  if data:
    sys.stdout.write(data.decode("utf-8", errors="replace"))
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

echo "[1/3] Staging a FAT drive (bootloader + kernel)..."

# Build bootloader if needed.
if [ ! -f "$BOOT_EFI_SRC" ]; then
  (cd "$ROOT_DIR/crates/bootloader" && \
    RUSTC="$(rustup which rustc --toolchain "$TOOLCHAIN")" \
    rustup run "$TOOLCHAIN" cargo build -p rayos-bootloader --release --target x86_64-unknown-uefi >/dev/null)
fi

# Build kernel with dev_scanout enabled.
# NOTE: AUTOHIDE is compile-time (option_env!); if requested, set env var before cargo build.
if [ "$AUTOHIDE_TEST" = "1" ]; then
  export RAYOS_DEV_SCANOUT_AUTOHIDE_SECS="$AUTOHIDE_SECS"
else
  unset RAYOS_DEV_SCANOUT_AUTOHIDE_SECS 2>/dev/null || true
fi

export PATH="$HOME/.cargo/bin:$PATH"
(cd "$ROOT_DIR/crates/kernel-bare" && \
  RUSTC="$(rustup which rustc --toolchain "$TOOLCHAIN")" \
  rustup run "$TOOLCHAIN" cargo build --release --target x86_64-unknown-none \
    -Z build-std=core,alloc -Z build-std-features=compiler-builtins-mem \
    --features dev_scanout >/dev/null)

if [ ! -f "$BOOT_EFI_SRC" ]; then
  echo "ERROR: Bootloader EFI not found at $BOOT_EFI_SRC" >&2
  exit 1
fi
if [ ! -f "$KERNEL_BIN_SRC" ]; then
  echo "ERROR: Kernel binary not found at $KERNEL_BIN_SRC" >&2
  exit 1
fi
if [ ! -f "$OVMF_CODE" ]; then
  echo "ERROR: OVMF_CODE not found at $OVMF_CODE" >&2
  exit 1
fi

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"
cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"

rm -f "$SERIAL_LOG" "$QEMU_LOG" "$MON_LOG" 2>/dev/null || true
rm -f "$MON_SOCK" 2>/dev/null || true

echo "[2/3] Starting headless QEMU..."
"$QEMU_BIN" \
  -machine q35,graphics=on,i8042=on \
  -m 2048 \
  -smp 2 \
  -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
  -drive file="fat:rw:$STAGE_DIR",format=raw \
  -vga std \
  -display none \
  -serial "file:$SERIAL_LOG" \
  -monitor "unix:$MON_SOCK,server,nowait" \
  -no-reboot \
  -net none \
  >"$QEMU_LOG" 2>&1 &

QEMU_PID=$!
echo "$QEMU_PID" > "$PID_FILE"

sleep 0.1
if ! kill -0 "$QEMU_PID" 2>/dev/null; then
  echo "ERROR: QEMU exited immediately" >&2
  tail -n 200 "$QEMU_LOG" 2>/dev/null || true
  exit 1
fi

# Wait for monitor socket.
for _ in $(seq 1 400); do
  if [ -S "$MON_SOCK" ]; then
    break
  fi
  sleep 0.01
done
if [ ! -S "$MON_SOCK" ]; then
  echo "ERROR: monitor socket not created" >&2
  tail -n 200 "$QEMU_LOG" 2>/dev/null || true
  exit 1
fi

# Wait for kernel to reach interactive loop.
BICAMERAL_MARKER="RayOS bicameral loop ready (':' for shell)"
if ! wait_for_log "$BICAMERAL_MARKER" "$TIMEOUT_SECS"; then
  echo "FAIL: did not reach bicameral loop" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

PRESENTED_MARKER="RAYOS_HOST_EVENT_V0:LINUX_PRESENTATION:PRESENTED"
DEV1="DEV_SCANOUT: enabled"
DEV2="DEV_SCANOUT: publish surface"

if wait_for_log "$PRESENTED_MARKER" "$AUTO_PRESENT_WAIT_SECS"; then
  echo "[3/3] Presentation already active (autopresent); skipping sendkey injection."
else
  echo "[3/3] Injecting 'show linu desktop' via QEMU monitor (sendkey)..."
  {
    echo "info version"
    echo "sendkey s"; echo "sendkey h"; echo "sendkey o"; echo "sendkey w"
    echo "sendkey spc"
    echo "sendkey l"; echo "sendkey i"; echo "sendkey n"; echo "sendkey u"
    echo "sendkey spc"
    echo "sendkey d"; echo "sendkey e"; echo "sendkey s"; echo "sendkey k"; echo "sendkey t"; echo "sendkey o"; echo "sendkey p"
    echo "sendkey ret"
  } | send_monitor_cmds_py >> "$MON_LOG" 2>&1 || true
fi

if ! wait_for_log "$PRESENTED_MARKER" "$TIMEOUT_SECS"; then
  echo "FAIL: missing Presented marker" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_log "$DEV1" "$TIMEOUT_SECS"; then
  echo "FAIL: missing DEV_SCANOUT enabled marker" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! wait_for_log "$DEV2" "$TIMEOUT_SECS"; then
  echo "FAIL: missing DEV_SCANOUT publish marker" >&2
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

if [ "$TOGGLE_TEST" = "1" ]; then
  echo "[extra] Toggling Presented -> Hidden -> Presented (backtick)..."

  TOGGLE_HIDDEN="DEV_SCANOUT: toggle hidden"
  TOGGLE_PRESENTED="DEV_SCANOUT: toggle presented"

  try_toggle_key() {
    local key_name="$1"
    local expect="$2"
    {
      echo "sendkey $key_name"
    } | send_monitor_cmds_py >> "$MON_LOG" 2>&1 || true
    wait_for_log "$expect" 2
  }

  # Toggle to Hidden.
  if ! try_toggle_key "grave_accent" "$TOGGLE_HIDDEN"; then
    if ! try_toggle_key "grave" "$TOGGLE_HIDDEN"; then
      echo "FAIL: did not observe toggle hidden marker (tried grave_accent and grave)" >&2
      tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
      exit 1
    fi
  fi

  # Toggle back to Presented.
  if ! try_toggle_key "grave_accent" "$TOGGLE_PRESENTED"; then
    if ! try_toggle_key "grave" "$TOGGLE_PRESENTED"; then
      echo "FAIL: did not observe toggle presented marker (tried grave_accent and grave)" >&2
      tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
      exit 1
    fi
  fi
fi

if [ "$AUTOHIDE_TEST" = "1" ]; then
  echo "[extra] Waiting for auto-hide marker (AUTOHIDE_SECS=$AUTOHIDE_SECS)..."
  if ! wait_for_log "DEV_SCANOUT: auto-hide" "$TIMEOUT_SECS"; then
    echo "FAIL: missing DEV_SCANOUT auto-hide marker" >&2
    tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
  fi
  if ! wait_for_log "RAYOS_HOST_EVENT_V0:LINUX_PRESENTATION:HIDDEN" "$TIMEOUT_SECS"; then
    echo "FAIL: missing HIDDEN marker after auto-hide" >&2
    tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
  fi
fi

echo "PASS: dev scanout markers observed"
echo "Serial log: $SERIAL_LOG"
echo "QEMU log: $QEMU_LOG"
echo "Monitor log: $MON_LOG"

exit 0
