#!/usr/bin/env bash
# Headless aarch64 Volume smoke test (bootloader embedded mode).
#
# Stages an ESP FAT dir and forces embedded-mode fallback by removing kernel.bin.
# Stages EFI\\RAYOS\\volume.bin and EFI\\RAYOS\\auto_prompt.txt containing "x" so the
# embedded runtime prints volume checksum to serial.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="${STAGE_DIR:-$WORK_DIR/aarch64-volume-headless-fat}"
OUT_LOG="${OUT_LOG:-$WORK_DIR/serial-boot-aarch64-volume-headless.log}"

# Make a deterministic KV volume if none exists.
VOLUME_SRC="${VOLUME_SRC:-$WORK_DIR/volume.bin}"

EMBEDDINGS_SRC="${EMBEDDINGS_SRC:-$WORK_DIR/embeddings.bin}"
INDEX_SRC="${INDEX_SRC:-$WORK_DIR/index.bin}"

BUILD_BOOTLOADER="${BUILD_BOOTLOADER:-1}"
QUIET_BUILD="${QUIET_BUILD:-1}"
BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/aarch64-unknown-uefi/release/uefi_boot.efi}"

AAVMF_CODE="${AAVMF_CODE:-/usr/share/AAVMF/AAVMF_CODE.no-secboot.fd}"
AAVMF_VARS_SRC="${AAVMF_VARS_SRC:-/usr/share/AAVMF/AAVMF_VARS.fd}"

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

echo "Creating $VOLUME_SRC (RAYOSVOL v1 KV volume)" >&2
python3 "$ROOT_DIR/scripts/tools/make_volume_kv.py" "$VOLUME_SRC" \
  greeting=hello-from-volume \
  build=rayos-aarch64

printf 'RAYOS_EMBEDDINGS_V1\n' >"$EMBEDDINGS_SRC"
printf 'RAYOS_INDEX_V1\n' >"$INDEX_SRC"

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

# Stage volume blob.
cp -f "$VOLUME_SRC" "$STAGE_DIR/EFI/RAYOS/volume.bin"

# Stage embeddings + index blobs.
cp -f "$EMBEDDINGS_SRC" "$STAGE_DIR/EFI/RAYOS/embeddings.bin"
cp -f "$INDEX_SRC" "$STAGE_DIR/EFI/RAYOS/index.bin"

# Autorun "vq greeting" to trigger a real volume query print.
cat >"$STAGE_DIR/EFI/RAYOS/auto_prompt.txt" <<'EOF'
vq greeting
EOF

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

VARS_FD="/tmp/rayos-aavmf-vars-volume-headless.fd"
cp -f "$AAVMF_VARS_SRC" "$VARS_FD"

rm -f "$OUT_LOG" 2>/dev/null || true

echo "Starting RayOS aarch64 volume headless test..." >&2

timeout 25s qemu-system-aarch64 \
  -machine virt \
  -cpu cortex-a57 \
  -m 2048 \
  -device ramfb \
  -drive if=pflash,format=raw,readonly=on,file="$AAVMF_CODE" \
  -drive if=pflash,format=raw,file="$VARS_FD" \
  -drive if=virtio,format=raw,file=fat:rw:"$STAGE_DIR" \
  -display none \
  -monitor none \
  -serial file:"$OUT_LOG" \
  >/dev/null 2>&1 || true

head -n 60 "$OUT_LOG" >&2 || true

if ! grep -q "RayOS uefi_boot: post-exit embedded loop" "$OUT_LOG"; then
  echo "FAIL: did not see embedded loop banner" >&2
  echo "Serial log: $OUT_LOG" >&2
  exit 1
fi

if ! grep -q "volume.bin loaded" "$OUT_LOG"; then
  echo "FAIL: did not see volume.bin loaded log" >&2
  echo "Serial log: $OUT_LOG" >&2
  exit 1
fi

if ! grep -q "embeddings.bin loaded" "$OUT_LOG"; then
  echo "FAIL: did not see embeddings.bin loaded log" >&2
  echo "Serial log: $OUT_LOG" >&2
  exit 1
fi

if ! grep -q "index.bin loaded" "$OUT_LOG"; then
  echo "FAIL: did not see index.bin loaded log" >&2
  echo "Serial log: $OUT_LOG" >&2
  exit 1
fi

if ! grep -q "volume: greeting = hello-from-volume" "$OUT_LOG"; then
  echo "FAIL: did not see volume query result line" >&2
  echo "Serial log: $OUT_LOG" >&2
  exit 1
fi

echo "PASS: saw volume query result in embedded UART output" >&2
