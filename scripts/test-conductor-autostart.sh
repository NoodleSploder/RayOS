#!/bin/bash
# Headless QEMU test: Conductor auto-starts inside the kernel and submits at least
# one task without any host-side submit.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TOOLCHAIN="${TOOLCHAIN:-nightly-2024-11-01-x86_64-unknown-linux-gnu}"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="$WORK_DIR/conductor-autostart-fat"
BOOT_EFI_SRC="$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
KERNEL_BIN_SRC="$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare"

SERIAL_LOG="$WORK_DIR/serial-conductor-autostart.log"
QEMU_LOG="$WORK_DIR/qemu-conductor-autostart.log"
MON_SOCK="$WORK_DIR/qemu-conductor-autostart-monitor.sock"
PID_FILE="$WORK_DIR/qemu-conductor-autostart.pid"

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

rm -f "$SERIAL_LOG" "$QEMU_LOG" 2>/dev/null || true
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

# Snapshot via host bridge and assert Conductor is running and has enqueued rays.
SNAPSHOT_LINE="$($CONDUCTOR_BIN qemu-snapshot --monitor-sock "$MON_SOCK" --serial-log "$SERIAL_LOG" --timeout-ms 2500)"
echo "$SNAPSHOT_LINE"

echo "$SNAPSHOT_LINE" | grep -F -q " conductor_running=0x0000000000000001"
if echo "$SNAPSHOT_LINE" | grep -F -q " s2_enq=0x0000000000000000"; then
  echo "FAIL: expected Conductor to submit at least one task automatically"
  exit 1
fi

echo "PASS: conductor autostart"
