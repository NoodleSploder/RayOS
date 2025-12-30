#!/bin/bash
# Headless QEMU conductor-bridge smoke test for RayOS.
# - Boots RayOS under OVMF with a QEMU monitor socket
# - Injects `:conductor snapshot` via the host-side conductor CLI
# - Verifies the snapshot line appears in the serial log

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TOOLCHAIN="${TOOLCHAIN:-nightly-2024-11-01-x86_64-unknown-linux-gnu}"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="$WORK_DIR/conductor-fat"
BOOT_EFI_SRC="$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
KERNEL_BIN_SRC="$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare"

SERIAL_LOG="$WORK_DIR/serial-conductor.log"
QEMU_LOG="$WORK_DIR/qemu-conductor.log"
MON_LOG="$WORK_DIR/monitor-conductor.log"
MON_SOCK="$WORK_DIR/qemu-conductor-monitor.sock"
PID_FILE="$WORK_DIR/qemu-conductor.pid"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

CONDUCTOR_BIN="$ROOT_DIR/crates/conductor/target/debug/rayos-conductor"

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

# Build bootloader if needed.
if [ ! -f "$BOOT_EFI_SRC" ]; then
  (cd "$ROOT_DIR/crates/bootloader" && \
    rustup run "$TOOLCHAIN" cargo build -p rayos-bootloader --release --target x86_64-unknown-uefi >/dev/null)
fi

# Build kernel if needed.
if [ ! -f "$KERNEL_BIN_SRC" ]; then
  (cd "$ROOT_DIR/crates/kernel-bare" && \
    RUSTC="$(rustup which rustc --toolchain "$TOOLCHAIN")" \
    rustup run "$TOOLCHAIN" cargo build --release --target x86_64-unknown-none \
      -Z build-std=core,alloc -Z build-std-features=compiler-builtins-mem >/dev/null)
fi

# Build conductor bridge binary if needed.
if [ ! -f "$CONDUCTOR_BIN" ]; then
  (cd "$ROOT_DIR/crates/conductor" && cargo build >/dev/null)
fi

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"
cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"

rm -f "$SERIAL_LOG" "$QEMU_LOG" "$MON_LOG" 2>/dev/null || true
rm -f "$MON_SOCK" 2>/dev/null || true

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

# Wait until the kernel reaches the bicameral loop.
READY_MARKER="RayOS bicameral loop ready (':' for shell)"
for _ in $(seq 1 2000); do
  if [ -f "$SERIAL_LOG" ] && tr -d '\r' < "$SERIAL_LOG" | grep -F -a -q "$READY_MARKER"; then
    break
  fi
  sleep 0.05
done

if ! tr -d '\r' < "$SERIAL_LOG" | grep -F -a -q "$READY_MARKER"; then
  echo "ERROR: Kernel did not reach bicameral loop within timeout"
  tail -n 200 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi

# Query snapshot via the conductor bridge.
SNAPSHOT_LINE_0="$($CONDUCTOR_BIN qemu-snapshot --monitor-sock "$MON_SOCK" --serial-log "$SERIAL_LOG" --timeout-ms 2500)"
echo "$SNAPSHOT_LINE_0"

# Submit deterministic text to System 2 via structured command.
SUBMIT_LINE="$($CONDUCTOR_BIN qemu-submit --monitor-sock "$MON_SOCK" --serial-log "$SERIAL_LOG" --timeout-ms 2500 "find now")"
echo "$SUBMIT_LINE"

# Re-snapshot and ensure S2 enqueue count moved off 0.
SNAPSHOT_LINE_1="$($CONDUCTOR_BIN qemu-snapshot --monitor-sock "$MON_SOCK" --serial-log "$SERIAL_LOG" --timeout-ms 2500)"
echo "$SNAPSHOT_LINE_1"

# Quit QEMU.
python3 - "$MON_SOCK" >> "$MON_LOG" 2>&1 <<'PY' || true
import socket,sys,time
sock_path=sys.argv[1]
s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
s.connect(sock_path)
s.sendall(b"quit\r\n")
time.sleep(0.05)
s.close()
PY

# Verify snapshot line made it into the serial log.
SERIAL_NORM="$WORK_DIR/serial-conductor.norm.log"
tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM"

grep -F -a -q "conductor snapshot " "$SERIAL_NORM"
if echo "$SNAPSHOT_LINE_1" | grep -F -q " s2_enq=0x0000000000000000"; then
  echo "FAIL: expected s2_enq to be non-zero after submit"
  exit 1
fi
echo "PASS: conductor snapshot found"
