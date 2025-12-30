#!/bin/bash
# Headless QEMU sendkey regression test for RayOS keyboard typed input.
# - Boots the universal USB image under OVMF
# - Uses a UNIX monitor socket to inject keystrokes
# - Captures serial output to a log file and verifies typed text appears

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TOOLCHAIN="${TOOLCHAIN:-nightly-2024-11-01-x86_64-unknown-linux-gnu}"

WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="$WORK_DIR/sendkey-fat"
BOOT_EFI_SRC="$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
KERNEL_BIN_SRC="$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare"

SERIAL_LOG="$WORK_DIR/serial-sendkey.log"
QEMU_LOG="$WORK_DIR/qemu-sendkey.log"
MON_LOG="$WORK_DIR/monitor-sendkey.log"
MON_SOCK="$WORK_DIR/qemu-monitor.sock"
PID_FILE="$WORK_DIR/qemu-sendkey.pid"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

cleanup() {
  if [ -f "$PID_FILE" ]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      # Give QEMU a moment to exit.
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

if [ ! -f "$BOOT_EFI_SRC" ]; then
  echo "ERROR: Bootloader EFI not found at $BOOT_EFI_SRC"
  exit 1
fi
if [ ! -f "$KERNEL_BIN_SRC" ]; then
  echo "ERROR: Kernel binary not found at $KERNEL_BIN_SRC"
  exit 1
fi

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"
cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"

if [ ! -f "$OVMF_CODE" ]; then
  echo "ERROR: OVMF_CODE not found at $OVMF_CODE"
  echo "Set OVMF_CODE=/path/to/OVMF_CODE_4M.fd"
  exit 1
fi

rm -f "$SERIAL_LOG" 2>/dev/null || true
rm -f "$QEMU_LOG" 2>/dev/null || true
rm -f "$MON_LOG" 2>/dev/null || true
rm -f "$MON_SOCK" 2>/dev/null || true

# Launch headless QEMU; keep serial in a file so the script can control the monitor.
# Use readonly + snapshot to reduce disk locking and prevent writes.

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

# Confirm QEMU is still alive.
sleep 0.1
if ! kill -0 "$QEMU_PID" 2>/dev/null; then
  echo "ERROR: QEMU exited immediately"
  echo "QEMU log: $QEMU_LOG"
  tail -n 200 "$QEMU_LOG" 2>/dev/null || true
  exit 1
fi

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
  echo "Serial log: $SERIAL_LOG"
  echo "QEMU log: $QEMU_LOG"
  tail -n 200 "$QEMU_LOG" 2>/dev/null || true
  tail -n 120 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

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

# Inject a known keystroke sequence.
# QEMU key names: letters, spc, ret, backspace.
# We verify the serial shell prompt and command outputs appear in the serial log.

echo "[3/3] Injecting keys via QEMU monitor (sendkey)..."
echo "--- monitor probe ---" >> "$MON_LOG"
echo "info version" | send_monitor_cmds_py >> "$MON_LOG" 2>&1 || true
echo "--- sendkey sequence ---" >> "$MON_LOG"
{
  # NOTE: RayOS uses a bicameral input loop:
  # - Lines prefixed with ':' go to the debug shell.
  # - All other lines go to System 2 (ray submission).
  #
  # QEMU monitor sendkey for ':' is typically "shift-semicolon".

  # :help
  echo "sendkey shift-semicolon"
  echo "sendkey h"
  echo "sendkey e"
  echo "sendkey l"
  echo "sendkey p"
  echo "sendkey ret"

  # :irq
  echo "sendkey shift-semicolon"
  echo "sendkey i"
  echo "sendkey r"
  echo "sendkey q"
  echo "sendkey ret"

  # :mmap
  echo "sendkey shift-semicolon"
  echo "sendkey m"
  echo "sendkey m"
  echo "sendkey a"
  echo "sendkey p"
  echo "sendkey ret"

  # :mmap raw
  echo "sendkey shift-semicolon"
  echo "sendkey m"
  echo "sendkey m"
  echo "sendkey a"
  echo "sendkey p"
  echo "sendkey spc"
  echo "sendkey r"
  echo "sendkey a"
  echo "sendkey w"
  echo "sendkey ret"

  # :echo hello
  echo "sendkey shift-semicolon"
  echo "sendkey e"
  echo "sendkey c"
  echo "sendkey h"
  echo "sendkey o"
  echo "sendkey spc"
  echo "sendkey h"
  echo "sendkey e"
  echo "sendkey l"
  echo "sendkey l"
  echo "sendkey o"
  echo "sendkey ret"

  # :s1 start
  echo "sendkey shift-semicolon"
  echo "sendkey s"
  echo "sendkey 1"
  echo "sendkey spc"
  echo "sendkey s"
  echo "sendkey t"
  echo "sendkey a"
  echo "sendkey r"
  echo "sendkey t"
  echo "sendkey ret"

  # :s2 find now
  echo "sendkey shift-semicolon"
  echo "sendkey s"
  echo "sendkey 2"
  echo "sendkey spc"
  echo "sendkey f"
  echo "sendkey i"
  echo "sendkey n"
  echo "sendkey d"
  echo "sendkey spc"
  echo "sendkey n"
  echo "sendkey o"
  echo "sendkey w"
  echo "sendkey ret"

  # plain text (System 2 path): find now
  echo "sendkey f"
  echo "sendkey i"
  echo "sendkey n"
  echo "sendkey d"
  echo "sendkey spc"
  echo "sendkey n"
  echo "sendkey o"
  echo "sendkey w"
  echo "sendkey ret"

  # :s1 stats
  echo "sendkey shift-semicolon"
  echo "sendkey s"
  echo "sendkey 1"
  echo "sendkey spc"
  echo "sendkey s"
  echo "sendkey t"
  echo "sendkey a"
  echo "sendkey t"
  echo "sendkey s"
  echo "sendkey ret"
} | send_monitor_cmds_py >> "$MON_LOG" 2>&1 || true

# Give the guest time to process the full command sequence and print to serial.
sleep 2.5

# Quit QEMU.
echo "--- quit ---" >> "$MON_LOG"
echo "quit" | send_monitor_cmds_py >> "$MON_LOG" 2>&1 || true

# Wait for QEMU to exit (bounded), then hard-kill if needed.
if ! wait_for_qemu_exit "$QEMU_PID" 10; then
  echo "WARN: QEMU did not exit after 'quit'; killing..."
  kill "$QEMU_PID" 2>/dev/null || true
  sleep 0.2 || true
  kill -9 "$QEMU_PID" 2>/dev/null || true
fi

wait "$QEMU_PID" 2>/dev/null || true

# Verify expected shell output exists (strip CR so grep matches on either style).
EXPECTED_READY="RayOS bicameral loop ready (':' for shell)"
EXPECTED_HELP="Commands: help, mem, ticks, irq, mmap [raw], fault <pf|gp|ud>, echo <text>, s1 <start|stop|stats>, s2 <text>"
EXPECTED_ECHO="echo: hello"
EXPECTED_IRQ="irqs timer=0x"
EXPECTED_MMAP="mmap regions=0x"
EXPECTED_MMAPRAW="mmapraw count=0x"
EXPECTED_S1_START="s1 running=1"
EXPECTED_S2="s2 rays=0x"
EXPECTED_S1_STATS="s1 running=0x"
EXPECTED_S2_SUBMIT="s2 submit count=0x"

# NOTE: With `set -o pipefail`, a pipeline like `tr ... | grep -q ...` can fail
# even when the match is found, because `grep -q` exits early and `tr` may die
# with SIGPIPE. To keep this robust, normalize the log once and then grep it.
SERIAL_LOG_NORM="$WORK_DIR/serial-sendkey.norm.log"
tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_LOG_NORM"

if grep -F -a -q "$EXPECTED_READY" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_HELP" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_IRQ" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_MMAP" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_MMAPRAW" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_ECHO" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_S1_START" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_S2" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_S2_SUBMIT" "$SERIAL_LOG_NORM" \
  && grep -F -a -q "$EXPECTED_S1_STATS" "$SERIAL_LOG_NORM"; then
  echo "PASS: Found shell prompt and command outputs in serial log"
  echo "Serial log: $SERIAL_LOG"
  exit 0
fi

echo "FAIL: Expected shell output not found in serial log"
echo "Serial log: $SERIAL_LOG"
echo "Monitor log: $MON_LOG"
# Show the tail to aid debugging.
tr -d '\r' < "$SERIAL_LOG" | tail -n 200
exit 1
