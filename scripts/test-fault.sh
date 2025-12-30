#!/bin/bash
# Headless QEMU fault regression test for RayOS exceptions.
# Boots, triggers a page fault via the shell, and asserts the serial marker.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TOOLCHAIN="${TOOLCHAIN:-nightly-2024-11-01-x86_64-unknown-linux-gnu}"

WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="$WORK_DIR/fault-fat"
BOOT_EFI_SRC="$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
KERNEL_BIN_SRC="$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare"

SERIAL_LOG="$WORK_DIR/serial-fault.log"
QEMU_LOG="$WORK_DIR/qemu-fault.log"
MON_LOG="$WORK_DIR/monitor-fault.log"
MON_SOCK="$WORK_DIR/qemu-monitor-fault.sock"
PID_FILE="$WORK_DIR/qemu-fault.pid"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

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

echo "[1/3] Staging a FAT drive (bootloader + kernel)..."

# Build bootloader if needed.
if [ ! -f "$BOOT_EFI_SRC" ]; then
  (cd "$ROOT_DIR/crates/bootloader" && \
    RUSTC="$(rustup which rustc --toolchain "$TOOLCHAIN")" \
    rustup run "$TOOLCHAIN" cargo build -p rayos-bootloader --release --target x86_64-unknown-uefi >/dev/null)
fi

# Build kernel if needed.
if [ ! -f "$KERNEL_BIN_SRC" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
  (cd "$ROOT_DIR/crates/kernel-bare" && \
    RUSTC="$(rustup which rustc --toolchain "$TOOLCHAIN")" \
    rustup run "$TOOLCHAIN" cargo build --release --target x86_64-unknown-none \
      -Z build-std=core,alloc -Z build-std-features=compiler-builtins-mem >/dev/null)
fi

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"
cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"

rm -f "$SERIAL_LOG" "$QEMU_LOG" "$MON_LOG" "$MON_SOCK" 2>/dev/null || true

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

# Wait for monitor socket.
for _ in $(seq 1 400); do
  if [ -S "$MON_SOCK" ]; then
    break
  fi
  sleep 0.05
done

if [ ! -S "$MON_SOCK" ]; then
  echo "ERROR: QEMU monitor socket did not appear: $MON_SOCK"
  exit 1
fi

# Wait for kernel boot marker in serial output.
BOOT_MARKER='RayOS kernel-bare: _start'
for _ in $(seq 1 1200); do
  if [ -f "$SERIAL_LOG" ] && grep -a -q "$BOOT_MARKER" "$SERIAL_LOG"; then
    break
  fi
  sleep 0.05
done

if ! grep -a -q "$BOOT_MARKER" "$SERIAL_LOG"; then
  echo "ERROR: Kernel boot marker not found in serial log within timeout"
  tail -n 200 "$QEMU_LOG" 2>/dev/null || true
  tail -n 120 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

send_monitor_cmds_py() {
  python3 -c '
import socket
import sys
import time

sock_path = sys.argv[1]
cmds = [line.strip("\n") for line in sys.stdin.read().splitlines() if line.strip()]

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1.0)
s.connect(sock_path)

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

# Best-effort initial drain.
try:
  s.settimeout(0.2)
  s.recv(4096)
except Exception:
  pass

for cmd in cmds:
  s.sendall((cmd + "\r\n").encode("ascii"))
  time.sleep(0.05)
  drain()

drain()
s.close()
' "$MON_SOCK"
}

wait_for_qemu_exit() {
  local pid="$1"
  local timeout_secs="$2"
  local start
  start=$(date +%s)
  while kill -0 "$pid" 2>/dev/null; do
    local now
    now=$(date +%s)
    if [ $((now - start)) -ge "$timeout_secs" ]; then
      return 1
    fi
    sleep 0.1
  done
  return 0
}

echo "[3/3] Triggering page fault via shell..."
FAULT_KIND="${FAULT_KIND:-pf}"
case "$FAULT_KIND" in
  pf|gp|ud) ;;
  *)
    echo "ERROR: FAULT_KIND must be pf|gp|ud (got: $FAULT_KIND)" >&2
    exit 2
    ;;
esac

echo "[3/3] Triggering $FAULT_KIND via shell..."
{
  # Type: fault <kind><ret>
  echo "sendkey f"
  echo "sendkey a"
  echo "sendkey u"
  echo "sendkey l"
  echo "sendkey t"
  echo "sendkey spc"
  case "$FAULT_KIND" in
    pf)
      echo "sendkey p"; echo "sendkey f" ;;
    gp)
      echo "sendkey g"; echo "sendkey p" ;;
    ud)
      echo "sendkey u"; echo "sendkey d" ;;
  esac
  echo "sendkey ret"
} | send_monitor_cmds_py >> "$MON_LOG" 2>&1 || true

# Give the guest time to print the exception marker.
sleep 0.6

# Quit QEMU (guest will be halted in the exception handler).
echo "quit" | send_monitor_cmds_py >> "$MON_LOG" 2>&1 || true

if ! wait_for_qemu_exit "$QEMU_PID" 10; then
  kill "$QEMU_PID" 2>/dev/null || true
  sleep 0.2 || true
  kill -9 "$QEMU_PID" 2>/dev/null || true
fi

wait "$QEMU_PID" 2>/dev/null || true

SERIAL_LOG_NORM="$WORK_DIR/serial-fault.norm.log"
tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_LOG_NORM"

EXPECTED_MARKER="EXC $(echo "$FAULT_KIND" | tr '[:lower:]' '[:upper:]')"
if grep -F -a -q "$EXPECTED_MARKER" "$SERIAL_LOG_NORM"; then
  echo "PASS: Found $EXPECTED_MARKER marker in serial log"
  echo "Serial log: $SERIAL_LOG"
  exit 0
fi

echo "FAIL: Expected fault marker not found: $EXPECTED_MARKER"
echo "Serial log: $SERIAL_LOG"
echo "Monitor log: $MON_LOG"
tail -n 200 "$SERIAL_LOG_NORM" 2>/dev/null || true
exit 1
