#!/usr/bin/env bash
# Headless aarch64 host-AI bridge smoke test (bootloader embedded mode).
#
# Stages an ESP FAT dir and forces embedded-mode fallback by removing kernel.bin.
# If EFI\\RAYOS\\auto_prompt.txt is present, the bootloader will emit:
#   RAYOS_INPUT:<id>:<text>
# The host-side conductor ai_bridge watches for that and replies with:
#   AI:<id>:...
#   AI_END:<id>
#
# This test uses an autorun prompt that triggers the host's time shortcut,
# so it doesn't depend on any external LLM backend.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="${STAGE_DIR:-$WORK_DIR/aarch64-ai-headless-fat}"
LOG_FILE="${LOG_FILE:-$WORK_DIR/aarch64-ai-headless.log}"
PID_FILE="${PID_FILE:-$WORK_DIR/aarch64-ai-headless.pid}"

QEMU_BIN="${QEMU_BIN:-qemu-system-aarch64}"
AAVMF_CODE="${AAVMF_CODE:-/usr/share/AAVMF/AAVMF_CODE.no-secboot.fd}"
AAVMF_VARS_SRC="${AAVMF_VARS_SRC:-/usr/share/AAVMF/AAVMF_VARS.fd}"

BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/aarch64-unknown-uefi/release/uefi_boot.efi}"

BUILD_BOOTLOADER="${BUILD_BOOTLOADER:-1}"
QUIET_BUILD="${QUIET_BUILD:-1}"

cleanup() {
  if [ -f "$PID_FILE" ]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      # Kill the QEMU child first, then the bridge.
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

# Force embedded fallback by removing the kernel.
rm -f "$STAGE_DIR/EFI/RAYOS/kernel.bin"

# Autorun prompt that triggers ai_bridge's time shortcut.
cat >"$STAGE_DIR/EFI/RAYOS/auto_prompt.txt" <<'EOF'
what time is it?
EOF

# UEFI shell will run this automatically if it can't find a Boot#### entry.
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

VARS_FD="/tmp/rayos-aavmf-vars-ai-headless.fd"
cp -f "$AAVMF_VARS_SRC" "$VARS_FD"

echo "Building ai_bridge (debug)..." >&2
pushd "$ROOT_DIR/crates/conductor" >/dev/null
cargo build --quiet --features "ai,ai_ollama" --bin ai_bridge >/dev/null
popd >/dev/null

BRIDGE_BIN="$ROOT_DIR/crates/conductor/target/debug/ai_bridge"
if [ ! -x "$BRIDGE_BIN" ]; then
  echo "ERROR: ai_bridge binary not found/executable at $BRIDGE_BIN" >&2
  exit 1
fi

echo "Starting aarch64 AI headless boot test..." >&2
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
for _ in $(seq 1 800); do
  if [ -f "$LOG_FILE" ] && grep -a -q "RAYOS_INPUT:" "$LOG_FILE"; then
    break
  fi
  sleep 0.05
done

for _ in $(seq 1 1200); do
  if [ -f "$LOG_FILE" ] && grep -a -q "AI_END:" "$LOG_FILE"; then
    break
  fi
  sleep 0.05
done

if ! grep -a -q "RAYOS_INPUT:" "$LOG_FILE"; then
  echo "ERROR: did not observe RAYOS_INPUT" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

if ! grep -a -q "AI_END:" "$LOG_FILE"; then
  echo "ERROR: did not observe AI_END" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

if ! grep -a -q "UTC:" "$LOG_FILE"; then
  echo "ERROR: expected UTC time reply but did not observe it" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

echo "OK: aarch64 AI headless smoke test passed. Log: $LOG_FILE" >&2
