#!/bin/bash
# Debug script to see what's happening during boot

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

echo "=== Starting QEMU with full debugging ==="
echo "OVMF firmware will output to $WORK_DIR/debug.log"
echo "Press Ctrl+C to stop"
echo ""

qemu-system-x86_64 \
    -machine q35 \
    -m 2048 \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive file="$WORK_DIR/rayos-universal-usb.img",format=raw \
    -debugcon file:"$WORK_DIR/debug.log" \
    -global isa-debugcon.iobase=0x402 \
    -serial stdio \
    -d cpu_reset,int \
    -D "$WORK_DIR/qemu.log" \
    -no-reboot \
    -no-shutdown
