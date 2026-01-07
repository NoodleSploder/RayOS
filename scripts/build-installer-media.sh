#!/bin/bash
# Build RayOS “installer” boot media artifacts.
#
# Today this produces installer-labeled media that boots the existing RayOS UEFI boot path.
# The actual installer runtime (partition manager + copy flow) will be integrated later.
#
# Outputs:
#   build/rayos-installer.iso
#   build/rayos-installer-usb.img

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

STAGE_OUT_DIR="${STAGE_OUT_DIR:-$ROOT_DIR/build/installer-media.stage}"
FINAL_OUT_DIR="${FINAL_OUT_DIR:-$ROOT_DIR/build}"

CLEAN="${CLEAN:-0}"

mkdir -p "$STAGE_OUT_DIR" "$FINAL_OUT_DIR"

args=("--arch" "universal" "--output" "$STAGE_OUT_DIR")
if [ "$CLEAN" = "1" ]; then
  args=("--clean" "${args[@]}")
fi

echo "[installer-media] Building universal UEFI media via scripts/build-iso.sh..." >&2
"$ROOT_DIR/scripts/build-iso.sh" "${args[@]}" >/dev/null

ISO_SRC="$STAGE_OUT_DIR/rayos.iso"
USB_SRC="$STAGE_OUT_DIR/rayos-universal-usb.img"

ISO_DST="$FINAL_OUT_DIR/rayos-installer.iso"
USB_DST="$FINAL_OUT_DIR/rayos-installer-usb.img"

if [ ! -f "$ISO_SRC" ]; then
  echo "ERROR: expected ISO not found at $ISO_SRC" >&2
  exit 1
fi
if [ ! -f "$USB_SRC" ]; then
  echo "ERROR: expected USB image not found at $USB_SRC" >&2
  exit 1
fi

cp -f "$ISO_SRC" "$ISO_DST"
cp -f "$USB_SRC" "$USB_DST"

ISO_SIZE=$(du -h "$ISO_DST" | cut -f1)
USB_SIZE=$(du -h "$USB_DST" | cut -f1)

echo "[installer-media] OK" >&2
echo "  ISO: $ISO_DST ($ISO_SIZE)" >&2
echo "  USB: $USB_DST ($USB_SIZE)" >&2
