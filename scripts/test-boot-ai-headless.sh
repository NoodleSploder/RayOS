#!/bin/bash
# Headless AI bridge smoke test.
# - Boots kernel-bare under OVMF
# - Runs ai_bridge (host-side) which uses -serial stdio
# - Uses a UNIX monitor socket to inject keystrokes
# - Verifies we get RAYOS_INPUT + AI_END in the captured log

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="${STAGE_DIR:-$WORK_DIR/ai-headless-fat}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-ai-headless.sock}"
LOG_FILE="${LOG_FILE:-$WORK_DIR/ai-headless.log}"
PID_FILE="${PID_FILE:-$WORK_DIR/ai-headless.pid}"

INPUT_TEXT="${INPUT_TEXT:-hi}"
EXPECT_TASK="${EXPECT_TASK:-0}"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi}"
KERNEL_BIN_SRC="${KERNEL_BIN_SRC:-$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare}"

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

# Build artifacts if requested.
BUILD_KERNEL="${BUILD_KERNEL:-1}"
QUIET_BUILD="${QUIET_BUILD:-1}"
if [ "$BUILD_KERNEL" != "0" ]; then
  echo "Building kernel-bare (release)..." >&2

  # Optional: pass extra kernel Cargo features without editing the script.
  # Example:
  #   RAYOS_KERNEL_FEATURES=dev_scanout ./scripts/test-boot-ai-headless.sh
  RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-}"
  KERNEL_FEATURES="host_ai"
  if [ -n "$RAYOS_KERNEL_FEATURES" ]; then
    KERNEL_FEATURES="$KERNEL_FEATURES,$RAYOS_KERNEL_FEATURES"
  fi

  pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
  if [ "$QUIET_BUILD" = "1" ]; then
    RUSTC="$(rustup which rustc)" cargo build --quiet \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target x86_64-unknown-none \
      --no-default-features --features "$KERNEL_FEATURES" >/dev/null
  else
    RUSTC="$(rustup which rustc)" cargo build \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target x86_64-unknown-none \
      --no-default-features --features "$KERNEL_FEATURES"
  fi
  popd >/dev/null
fi

BUILD_BRIDGE="${BUILD_BRIDGE:-1}"
BRIDGE_PROFILE="${BRIDGE_PROFILE:-debug}"
BRIDGE_FEATURES="ai,ai_ollama"
if [ "$BUILD_BRIDGE" != "0" ]; then
  echo "Building ai_bridge ($BRIDGE_PROFILE)..." >&2
  pushd "$ROOT_DIR/crates/conductor" >/dev/null
  if [ "$BRIDGE_PROFILE" = "release" ]; then
    if [ "$QUIET_BUILD" = "1" ]; then
      cargo build --quiet --release --features "$BRIDGE_FEATURES" --bin ai_bridge >/dev/null
    else
      cargo build --release --features "$BRIDGE_FEATURES" --bin ai_bridge
    fi
  else
    if [ "$QUIET_BUILD" = "1" ]; then
      cargo build --quiet --features "$BRIDGE_FEATURES" --bin ai_bridge >/dev/null
    else
      cargo build --features "$BRIDGE_FEATURES" --bin ai_bridge
    fi
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

if [ "$BRIDGE_PROFILE" = "release" ]; then
  BRIDGE_BIN="$ROOT_DIR/crates/conductor/target/release/ai_bridge"
else
  BRIDGE_BIN="$ROOT_DIR/crates/conductor/target/debug/ai_bridge"
fi
if [ ! -x "$BRIDGE_BIN" ]; then
  echo "ERROR: ai_bridge binary not found/executable at $BRIDGE_BIN" >&2
  exit 1
fi

# Start ai_bridge in background, capturing combined output.
# ai_bridge will spawn QEMU; we keep QEMU headless but with -serial stdio.
(
  exec "$BRIDGE_BIN" \
    "$QEMU_BIN" \
      -machine q35,graphics=on,i8042=on \
      -m 2048 \
      -smp 2 \
      -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
      -drive file="fat:rw:$STAGE_DIR",format=raw \
      -serial stdio \
      -monitor "unix:$MON_SOCK,server,nowait" \
      -vga std \
      -display none \
      -no-reboot \
      -net none
) >"$LOG_FILE" 2>&1 &

PID=$!
echo "$PID" > "$PID_FILE"

# Wait for monitor socket.
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

# Wait for boot marker.
BOOT_MARKER='RayOS bicameral loop ready'
for _ in $(seq 1 1600); do
  if [ -f "$LOG_FILE" ] && grep -a -q "$BOOT_MARKER" "$LOG_FILE"; then
    break
  fi
  sleep 0.05
done

if ! grep -a -q "$BOOT_MARKER" "$LOG_FILE"; then
  echo "ERROR: Boot marker not found in log" >&2
  tail -n 200 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

# Send keystrokes via monitor.
python3 "$ROOT_DIR/scripts/qemu-sendtext.py" --sock "$MON_SOCK" --text "$INPUT_TEXT" --after 0.6 --quit

# Wait for bridge to exit.
for _ in $(seq 1 200); do
  if ! kill -0 "$PID" 2>/dev/null; then
    break
  fi
  sleep 0.05
done

# Verify log contains the correlated protocol.
if ! grep -a -q "RAYOS_INPUT:.*:${INPUT_TEXT}" "$LOG_FILE"; then
  echo "ERROR: did not observe RAYOS_INPUT for '${INPUT_TEXT}'" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

if ! grep -a -q 'AI_END:' "$LOG_FILE"; then
  echo "ERROR: did not observe AI_END" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

if [ "$EXPECT_TASK" = "1" ]; then
  if ! grep -a -q 'Action: queued task' "$LOG_FILE"; then
    echo "ERROR: expected a queued task but did not observe one" >&2
    tail -n 240 "$LOG_FILE" 2>/dev/null || true
    exit 1
  fi
  # Tasks can complete fast enough that we never see an intermediate Running state.
  # Accept either the generic completion line or the user-facing completion label.
  if ! (grep -a -q 'Task [0-9a-f]\{8\} completed' "$LOG_FILE" || grep -a -q 'Search results (' "$LOG_FILE"); then
    echo "ERROR: expected task completion output but did not observe it" >&2
    tail -n 240 "$LOG_FILE" 2>/dev/null || true
    exit 1
  fi
fi

echo "OK: AI headless smoke test passed. Log: $LOG_FILE" >&2
