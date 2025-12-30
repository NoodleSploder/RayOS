#!/bin/bash
# Headless Cortex daemon -> Kernel protocol integration test (x86_64 kernel-bare).
#
# Boots kernel-bare under OVMF, then runs the host-side `rayos-cortex` binary in a
# "send one CORTEX line and exit" mode. The cortex binary injects the message
# via the QEMU monitor socket (sendkey), and we verify the kernel logs it.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="${STAGE_DIR:-$WORK_DIR/cortex-daemon-headless-fat}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-cortex-daemon-headless.sock}"
LOG_FILE="${LOG_FILE:-$WORK_DIR/cortex-daemon-headless.log}"
PID_FILE="${PID_FILE:-$WORK_DIR/cortex-daemon-headless.pid}"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi}"
KERNEL_BIN_SRC="${KERNEL_BIN_SRC:-$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare}"

CORTEX_LINE="${CORTEX_LINE:-CORTEX:INTENT kind=delete target=demo_file}"

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

rm -f "$MON_SOCK" 2>/dev/null || true
rm -f "$LOG_FILE" 2>/dev/null || true

BUILD_KERNEL="${BUILD_KERNEL:-1}"
QUIET_BUILD="${QUIET_BUILD:-1}"

BUILD_BOOTLOADER="${BUILD_BOOTLOADER:-1}"
if [ "$BUILD_BOOTLOADER" != "0" ]; then
  echo "Building uefi_boot (release)..." >&2
  pushd "$ROOT_DIR/crates/bootloader" >/dev/null
  if [ "$QUIET_BUILD" = "1" ]; then
    RUSTC="$(rustup which rustc)" cargo build --quiet \
      --release \
      --target x86_64-unknown-uefi \
      -p rayos-bootloader >/dev/null
  else
    RUSTC="$(rustup which rustc)" cargo build \
      --release \
      --target x86_64-unknown-uefi \
      -p rayos-bootloader
  fi
  popd >/dev/null
fi

if [ "$BUILD_KERNEL" != "0" ]; then
  echo "Building kernel-bare (release)..." >&2
  pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
  if [ "$QUIET_BUILD" = "1" ]; then
    RUSTC="$(rustup which rustc)" cargo build --quiet \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target x86_64-unknown-none >/dev/null
  else
    RUSTC="$(rustup which rustc)" cargo build \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target x86_64-unknown-none
  fi
  popd >/dev/null
fi

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"

if [ -f "$BOOT_EFI_SRC" ]; then
  cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
else
  echo "ERROR: bootloader EFI not found at $BOOT_EFI_SRC" >&2
  exit 1
fi

if [ -f "$KERNEL_BIN_SRC" ]; then
  cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"
else
  echo "ERROR: kernel-bare not found at $KERNEL_BIN_SRC" >&2
  exit 1
fi

if [ ! -f "$OVMF_CODE" ]; then
  echo "ERROR: OVMF_CODE not found at $OVMF_CODE" >&2
  exit 1
fi

"$QEMU_BIN" \
  -machine q35,graphics=on,i8042=on \
  -m 2048 \
  -smp 2 \
  -rtc base=utc,clock=host \
  -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
  -drive file="fat:rw:$STAGE_DIR",format=raw \
  -serial "file:$LOG_FILE" \
  -monitor "unix:$MON_SOCK,server,nowait" \
  -vga std \
  -display none \
  -no-reboot \
  -net none \
  >"$WORK_DIR/qemu-cortex-daemon-headless.log" 2>&1 &

PID=$!
echo "$PID" > "$PID_FILE"

for _ in $(seq 1 400); do
  if [ -S "$MON_SOCK" ]; then
    break
  fi
  sleep 0.05
done

if [ ! -S "$MON_SOCK" ]; then
  echo "ERROR: QEMU monitor socket did not appear: $MON_SOCK" >&2
  tail -n 200 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

BOOT_MARKER='RayOS bicameral loop ready'
for _ in $(seq 1 1600); do
  if [ -f "$LOG_FILE" ] && grep -a -q "$BOOT_MARKER" "$LOG_FILE"; then
    break
  fi
  sleep 0.05
done

if ! grep -a -q "$BOOT_MARKER" "$LOG_FILE"; then
  echo "ERROR: Boot marker not found in log" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

# Run the host cortex binary in a "send once" mode.
pushd "$ROOT_DIR/crates/cortex" >/dev/null
RAYOS_QEMU_MONITOR_SOCK="$MON_SOCK" RAYOS_CORTEX_TEST_LINE="$CORTEX_LINE" \
  cargo run --quiet
popd >/dev/null

# Give the guest some time to process and then quit QEMU.
sleep 0.6
python3 "$ROOT_DIR/scripts/qemu-sendtext.py" --sock "$MON_SOCK" --text ":echo done" --after 0.2 --quit

for _ in $(seq 1 200); do
  if ! kill -0 "$PID" 2>/dev/null; then
    break
  fi
  sleep 0.05
done

NORM="$WORK_DIR/cortex-daemon-headless.norm.log"
tr -d '\r' < "$LOG_FILE" > "$NORM" 2>/dev/null || true

if ! grep -a -q "cortex intent kind=" "$NORM"; then
  echo "ERROR: did not observe cortex intent log" >&2
  tail -n 260 "$NORM" 2>/dev/null || true
  exit 1
fi

if ! grep -a -q "cortex s2 rays=" "$NORM"; then
  echo "ERROR: did not observe cortex forwarding into s2" >&2
  tail -n 260 "$NORM" 2>/dev/null || true
  exit 1
fi

echo "OK: Cortex daemon -> Kernel headless test passed. Log: $LOG_FILE" >&2
