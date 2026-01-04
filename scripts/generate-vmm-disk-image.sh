#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/crates/kernel-bare/assets"
OUT_IMG="$OUT_DIR/vmm_disk.img"

# Keep this small and deterministic: 128 sectors × 512 bytes = 64 KiB.
SECTOR_SIZE=512
SECTORS=128
BYTES=$((SECTOR_SIZE * SECTORS))

mkdir -p "$OUT_DIR"

# Create a zeroed image.
dd if=/dev/zero of="$OUT_IMG" bs=1 count="$BYTES" status=none

# Write a simple ASCII marker at the start so reads can be asserted.
printf 'RAYOS_VMM_DISK_IMAGE\n' | dd of="$OUT_IMG" bs=1 seek=0 conv=notrunc status=none

echo "Wrote $OUT_IMG ($BYTES bytes)"
