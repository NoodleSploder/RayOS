#!/bin/bash
# Headless persistence test for RayOS Volume.
#
# Proves: virtio-blk (legacy) is detected, Volume can format + append, and data
# survives a reboot when using the same disk image.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TOOLCHAIN="${TOOLCHAIN:-nightly-2024-11-01-x86_64-unknown-linux-gnu}"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="$WORK_DIR/volume-fat"
BOOT_EFI_SRC="$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi"
KERNEL_BIN_SRC="$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare"

SERIAL_LOG="$WORK_DIR/serial-volume.log"
QEMU_LOG="$WORK_DIR/qemu-volume.log"
MON_LOG="$WORK_DIR/monitor-volume.log"
MON_SOCK="$WORK_DIR/qemu-volume-monitor.sock"
PID_FILE="$WORK_DIR/qemu-volume.pid"

VOLUME_IMG="$WORK_DIR/volume-disk.img"
VOLUME_MB="${VOLUME_MB:-64}"

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

ensure_builds() {
  # Build bootloader if needed.
  if [ ! -f "$BOOT_EFI_SRC" ]; then
    (cd "$ROOT_DIR/crates/bootloader" && \
      rustup run "$TOOLCHAIN" cargo build -p rayos-bootloader --release --target x86_64-unknown-uefi >/dev/null)
  fi

  # Build kernel (always, to avoid stale artifacts during iteration).
  (cd "$ROOT_DIR/crates/kernel-bare" && \
    RUSTC="$(rustup which rustc --toolchain "$TOOLCHAIN")" \
    rustup run "$TOOLCHAIN" cargo build --release --target x86_64-unknown-none \
      -Z build-std=core,alloc -Z build-std-features=compiler-builtins-mem >/dev/null)

  # Build conductor bridge binary (always, to avoid stale subcommand tables).
  (cd "$ROOT_DIR/crates/conductor" && cargo build >/dev/null)
}

stage_fat() {
  rm -rf "$STAGE_DIR" 2>/dev/null || true
  mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"
  cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
  cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"
}

ensure_disk() {
  if [ ! -f "$VOLUME_IMG" ]; then
    dd if=/dev/zero of="$VOLUME_IMG" bs=1M count="$VOLUME_MB" status=none
  fi
}

qemu_boot() {
  rm -f "$SERIAL_LOG" "$QEMU_LOG" "$MON_LOG" 2>/dev/null || true
  rm -f "$MON_SOCK" 2>/dev/null || true

  "$QEMU_BIN" \
    -machine q35,graphics=on,i8042=on \
    -m 2048 \
    -smp 2 \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive file="fat:rw:$STAGE_DIR",format=raw \
    -drive if=none,id=vol0,file="$VOLUME_IMG",format=raw,cache=unsafe \
    -device virtio-blk-pci,drive=vol0,disable-modern=on \
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
}

qemu_quit() {
  python3 - "$MON_SOCK" >> "$MON_LOG" 2>&1 <<'PY' || true
import socket,sys,time
sock_path=sys.argv[1]
s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
s.connect(sock_path)
s.sendall(b"quit\r\n")
time.sleep(0.05)
s.close()
PY
}

run_shell() {
  local cmd="$1"
  local expect="$2"
  "$CONDUCTOR_BIN" qemu-shell --monitor-sock "$MON_SOCK" --serial-log "$SERIAL_LOG" --timeout-ms 4000 --expect "$expect" "$cmd" >/dev/null
}

# ---------------------------------------------------------------------------
# Test flow
# ---------------------------------------------------------------------------

ensure_builds
stage_fat
ensure_disk

TEST_TEXT="volume persistence hello"

# Boot #1: format and append a record via System 2 submit hook.
qemu_boot

run_shell "vol probe" "vol probe ok=0x"
run_shell "vol format" "vol format ok=0x0000000000000001"

"$CONDUCTOR_BIN" qemu-submit --monitor-sock "$MON_SOCK" --serial-log "$SERIAL_LOG" --timeout-ms 4000 "$TEST_TEXT" >/dev/null

run_shell "vol tail 1" "text=$TEST_TEXT"
qemu_quit
sleep 0.2

# Boot #2: verify the record is still present on the same disk image.
stage_fat
qemu_boot
run_shell "vol probe" "vol probe ok=0x"
run_shell "vol tail 4" "text=$TEST_TEXT"
qemu_quit

echo "PASS: Volume persisted record across reboot"
