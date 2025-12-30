#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_LOG="$ROOT_DIR/build/serial-boot-aarch64-headless.log"
STAGE_DIR="$ROOT_DIR/build/aarch64-headless-fat"

mkdir -p "$ROOT_DIR/build"

if [[ ! -d "$ROOT_DIR/build/iso-content-aarch64/EFI" ]]; then
  echo "iso-content-aarch64 not found; building aarch64 ISO staging..." >&2
  "$ROOT_DIR/scripts/build-iso.sh" --arch aarch64 >/dev/null
fi

# If we have a freshly built bootloader, prefer it (keeps the test loop fast).
if [[ -f "$ROOT_DIR/crates/bootloader/target/aarch64-unknown-uefi/release/uefi_boot.efi" ]]; then
  command cp -f "$ROOT_DIR/crates/bootloader/target/aarch64-unknown-uefi/release/uefi_boot.efi" \
    "$ROOT_DIR/build/iso-content-aarch64/EFI/BOOT/BOOTAA64.EFI" || true
fi

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"
command cp -a "$ROOT_DIR/build/iso-content-aarch64/." "$STAGE_DIR/"

# Force the bootloader into aarch64 embedded fallback by removing the (x86_64) kernel.
rm -f "$STAGE_DIR/EFI/RAYOS/kernel.bin"

# Optional: stage a model blob so the bootloader can prove it can consume model bytes.
if [[ -f "$ROOT_DIR/build/model.bin" ]]; then
  command cp -f "$ROOT_DIR/build/model.bin" "$STAGE_DIR/EFI/RAYOS/model.bin" || true
fi

# UEFI shell will run this automatically if it can't find a Boot#### entry.
cat >"$STAGE_DIR/startup.nsh" <<'EOF'
fs0:\EFI\BOOT\BOOTAA64.EFI
EOF

# Use a writable copy of VARS so UEFI can create Boot#### entries if desired.
VARS_FD="/tmp/rayos-aavmf-vars.fd"
command cp -f /usr/share/AAVMF/AAVMF_VARS.fd "$VARS_FD"

echo "Starting RayOS aarch64 headless boot test..." >&2

rm -f "$OUT_LOG"

timeout 25s qemu-system-aarch64 \
  -machine virt \
  -cpu cortex-a57 \
  -m 2048 \
  -device ramfb \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/AAVMF/AAVMF_CODE.no-secboot.fd \
  -drive if=pflash,format=raw,file="$VARS_FD" \
  -drive if=virtio,format=raw,file=fat:rw:"$STAGE_DIR" \
  -display none \
  -monitor none \
  -serial file:"$OUT_LOG" \
  >/dev/null 2>&1 || true

head -n 40 "$OUT_LOG" >&2 || true

if grep -q "RayOS uefi_boot: post-exit embedded loop" "$OUT_LOG"; then
  # Ensure we printed basic GPU/framebuffer probe info.
  if ! grep -q "RayOS: GPU probe: GOP handles" "$OUT_LOG"; then
    echo "FAIL: did not see GPU probe GOP handles marker" >&2
    echo "Serial log: $OUT_LOG" >&2
    exit 1
  fi
  if ! grep -q "RayOS: GPU probe: fb_base=" "$OUT_LOG"; then
    echo "FAIL: did not see GPU probe framebuffer base marker" >&2
    echo "Serial log: $OUT_LOG" >&2
    exit 1
  fi

  if [[ -f "$ROOT_DIR/build/model.bin" ]]; then
    if ! grep -q "model_fnv1a64_64k=0x" "$OUT_LOG"; then
      echo "FAIL: model.bin staged but checksum line missing" >&2
      echo "Serial log: $OUT_LOG" >&2
      exit 1
    fi
  fi
  echo "PASS: saw post-exit embedded UART banner" >&2
  exit 0
fi

echo "FAIL: did not see post-exit embedded UART banner" >&2
echo "Serial log: $OUT_LOG" >&2
exit 1
