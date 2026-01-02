#!/bin/bash
# Headless boot smoke test for RayOS.
# - Captures serial to a file (more reliable than piping stdio through timeout)
# - Uses a configurable image path + timeout
# - Verifies a couple of boot markers

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

IMAGE="${IMAGE:-$ROOT_DIR/build/rayos-universal-usb.img}"
TIMEOUT_SECS="${TIMEOUT_SECS:-15}"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

SERIAL_LOG="$WORK_DIR/serial-boot-headless.log"
QEMU_LOG="$WORK_DIR/qemu-boot-headless.log"

echo "Starting RayOS headless boot test..."
echo "Image: $IMAGE"
echo "Timeout: ${TIMEOUT_SECS}s"

if [ ! -f "$OVMF_CODE" ]; then
    echo "ERROR: OVMF_CODE not found at $OVMF_CODE"
    exit 1
fi

if [ ! -f "$IMAGE" ]; then
    echo "ERROR: Image not found at $IMAGE"
    exit 1
fi

rm -f "$SERIAL_LOG" "$QEMU_LOG" 2>/dev/null || true

set +e
timeout "$TIMEOUT_SECS" "$QEMU_BIN" \
    -machine q35 \
    -m 2048 \
    -device virtio-gpu-pci \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive file="$IMAGE",format=raw \
    -serial "file:$SERIAL_LOG" \
    -vga std \
    -display none \
    -monitor none \
    -no-reboot \
    -net none \
    >"$QEMU_LOG" 2>&1
QEMU_RC=$?
set -e

# Normalize CRLF (some firmware builds emit CR).
SERIAL_NORM="$WORK_DIR/serial-boot-headless.norm.log"
tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

BOOTLOADER_MARKER="RayOS uefi_boot: start"
KERNEL_MARKER="RayOS kernel-bare: _start"
BICAMERAL_MARKER="RayOS bicameral loop ready (':' for shell)"
GPU_MARKER="RAYOS_X86_64_VIRTIO_GPU:FEATURES_OK"

if grep -F -a -q "$BOOTLOADER_MARKER" "$SERIAL_NORM" \
    && grep -F -a -q "$KERNEL_MARKER" "$SERIAL_NORM" \
    && grep -F -a -q "$BICAMERAL_MARKER" "$SERIAL_NORM" \
    && grep -F -a -q "$GPU_MARKER" "$SERIAL_NORM"; then
    echo "PASS: Found boot markers in serial log"
    echo "Serial log: $SERIAL_LOG"
    echo "QEMU log: $QEMU_LOG"
    exit 0
fi

echo "FAIL: Missing expected boot markers"
echo "Serial log: $SERIAL_LOG"
echo "QEMU log: $QEMU_LOG"
echo "Exit code: $QEMU_RC"
tail -n 200 "$SERIAL_NORM" 2>/dev/null || true
exit 1
