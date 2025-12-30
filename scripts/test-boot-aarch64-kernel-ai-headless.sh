#!/usr/bin/env bash
# Headless aarch64 kernel + host AI bridge smoke test (Option B).
#
# Boots uefi_boot + kernel-aarch64-bare under AAVMF, using conductor's ai_bridge
# to respond to RAYOS_INPUT:<id>:... with AI:<id>:... and AI_END:<id>.
#
# The kernel emits a deterministic prompt "what time is it?" so ai_bridge can
# reply via its built-in UTC shortcut (no external LLM backend required).

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="${STAGE_DIR:-$WORK_DIR/aarch64-kernel-ai-headless-fat}"
LOG_FILE="${LOG_FILE:-$WORK_DIR/aarch64-kernel-ai-headless.log}"
PID_FILE="${PID_FILE:-$WORK_DIR/aarch64-kernel-ai-headless.pid}"

QEMU_BIN="${QEMU_BIN:-qemu-system-aarch64}"
AAVMF_CODE="${AAVMF_CODE:-/usr/share/AAVMF/AAVMF_CODE.no-secboot.fd}"
AAVMF_VARS_SRC="${AAVMF_VARS_SRC:-/usr/share/AAVMF/AAVMF_VARS.fd}"

BUILD_BOOTLOADER="${BUILD_BOOTLOADER:-1}"
BUILD_KERNEL="${BUILD_KERNEL:-1}"
BUILD_BRIDGE="${BUILD_BRIDGE:-1}"
QUIET_BUILD="${QUIET_BUILD:-1}"

BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/aarch64-unknown-uefi/release/uefi_boot.efi}"
KERNEL_BIN_SRC_DEFAULT="$ROOT_DIR/crates/kernel-aarch64-bare/target/aarch64-unknown-none-softfloat/release/kernel-aarch64-bare"
KERNEL_BIN_SRC="${KERNEL_BIN_SRC:-$KERNEL_BIN_SRC_DEFAULT}"

cleanup() {
  if [ -f "$PID_FILE" ]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      pkill -P "$pid" 2>/dev/null || true
      kill "$pid" 2>/dev/null || true
      sleep 0.2 || true
      pkill -P "$pid" 2>/dev/null || true
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
  rm -f "$PID_FILE" 2>/dev/null || true
}
trap cleanup EXIT

rm -f "$LOG_FILE" 2>/dev/null || true

if [ "$BUILD_BOOTLOADER" != "0" ]; then
  echo "Building bootloader (aarch64 UEFI, release)..." >&2
  pushd "$ROOT_DIR/crates/bootloader" >/dev/null
  if [ "$QUIET_BUILD" = "1" ]; then
    RUSTC="$(rustup which rustc)" cargo build --quiet --release --target aarch64-unknown-uefi >/dev/null
  else
    RUSTC="$(rustup which rustc)" cargo build --release --target aarch64-unknown-uefi
  fi
  popd >/dev/null
fi

if [ "$BUILD_KERNEL" != "0" ]; then
  echo "Building kernel-aarch64-bare (release)..." >&2
  pushd "$ROOT_DIR/crates/kernel-aarch64-bare" >/dev/null
  if [ "$QUIET_BUILD" = "1" ]; then
    PATH="$HOME/.cargo/bin:$PATH" RUSTC="$(rustup which rustc)" cargo build --quiet \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target aarch64-unknown-none-softfloat >/dev/null
  else
    PATH="$HOME/.cargo/bin:$PATH" RUSTC="$(rustup which rustc)" cargo build \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target aarch64-unknown-none-softfloat
  fi
  popd >/dev/null
fi

if [ "$BUILD_BRIDGE" != "0" ]; then
  echo "Building ai_bridge (debug)..." >&2
  pushd "$ROOT_DIR/crates/conductor" >/dev/null
  cargo build --quiet --features "ai,ai_ollama" --bin ai_bridge >/dev/null
  popd >/dev/null
fi

BRIDGE_BIN="$ROOT_DIR/crates/conductor/target/debug/ai_bridge"
if [ ! -x "$BRIDGE_BIN" ]; then
  echo "ERROR: ai_bridge binary not found/executable at $BRIDGE_BIN" >&2
  exit 1
fi

if [[ ! -d "$ROOT_DIR/build/iso-content-aarch64/EFI" ]]; then
  echo "iso-content-aarch64 not found; building aarch64 ISO staging..." >&2
  "$ROOT_DIR/scripts/build-iso.sh" --arch aarch64 >/dev/null
fi

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR"
cp -a "$ROOT_DIR/build/iso-content-aarch64/." "$STAGE_DIR/"
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"

if [ -f "$BOOT_EFI_SRC" ]; then
  cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTAA64.EFI"
else
  echo "ERROR: bootloader EFI not found at $BOOT_EFI_SRC" >&2
  exit 1
fi

if [ -f "$KERNEL_BIN_SRC" ]; then
  cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"
else
  echo "ERROR: kernel ELF not found at $KERNEL_BIN_SRC" >&2
  exit 1
fi

cat >"$STAGE_DIR/startup.nsh" <<'EOF'
fs0:\EFI\BOOT\BOOTAA64.EFI
EOF

if [ ! -f "$AAVMF_CODE" ]; then
  echo "ERROR: AAVMF_CODE not found at $AAVMF_CODE" >&2
  exit 1
fi
if [ ! -f "$AAVMF_VARS_SRC" ]; then
  echo "ERROR: AAVMF_VARS not found at $AAVMF_VARS_SRC" >&2
  exit 1
fi

VARS_FD="/tmp/rayos-aavmf-vars-kernel-ai-headless.fd"
cp -f "$AAVMF_VARS_SRC" "$VARS_FD"

echo "Starting aarch64 kernel AI headless boot test..." >&2
(
  exec "$BRIDGE_BIN" \
    "$QEMU_BIN" \
      -machine virt \
      -cpu cortex-a57 \
      -m 2048 \
      -device ramfb \
      -drive if=pflash,format=raw,readonly=on,file="$AAVMF_CODE" \
      -drive if=pflash,format=raw,file="$VARS_FD" \
      -drive if=virtio,format=raw,file=fat:rw:"$STAGE_DIR" \
      -serial stdio \
      -display none \
      -monitor none \
      -no-reboot \
      -net none
) >"$LOG_FILE" 2>&1 &

PID=$!
echo "$PID" > "$PID_FILE"

# Wait for protocol markers.
for _ in $(seq 1 1200); do
  if [ -f "$LOG_FILE" ] && grep -a -q "RAYOS_INPUT:" "$LOG_FILE"; then
    break
  fi
  sleep 0.05
done

for _ in $(seq 1 1600); do
  if [ -f "$LOG_FILE" ] && grep -a -q "AI_END:" "$LOG_FILE"; then
    break
  fi
  sleep 0.05
done

if ! grep -a -q "RayOS kernel-aarch64-bare: _start" "$LOG_FILE"; then
  echo "ERROR: did not observe kernel start marker" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

if ! grep -a -q "RAYOS_INPUT:1:what time is it\?" "$LOG_FILE"; then
  echo "ERROR: did not observe expected RAYOS_INPUT" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

if ! grep -a -q "AI_END:1" "$LOG_FILE"; then
  echo "ERROR: did not observe expected AI_END:1" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

if ! grep -a -q "UTC:" "$LOG_FILE"; then
  echo "ERROR: expected UTC time reply but did not observe it" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

echo "OK: aarch64 kernel AI headless smoke test passed. Log: $LOG_FILE" >&2
