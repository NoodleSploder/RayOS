#!/bin/bash

echo "╔═══════════════════════════════════════════════════╗"
echo "║     RayOS Boot Verification Test                ║"
echo "╚═══════════════════════════════════════════════════╝"
echo ""
echo "This test will:"
echo "  1. Boot RayOS in QEMU with graphical output"
echo "  2. Display bootloader messages including:"
echo "     - Framebuffer base address (hex)"
echo "     - Resolution (width x height)"
echo "     - Stride value"
echo "  3. Boot into kernel with animated UI"
echo ""
echo "Expected Results:"
echo "  ✓ Bootloader shows blue screen with white/colored text"
echo "  ✓ Framebuffer parameters displayed (0x... address)"
echo "  ✓ Kernel displays RayOS banner and status indicators"
echo "  ✓ Green heartbeat box blinks in top-right"
echo "  ✓ Tick counter increases"
echo ""
echo "Automation:"
echo "  - Headless boot markers: ./scripts/test-boot-headless.sh"
echo "  - Headless local AI (no host bridge): ./scripts/test-boot-local-ai-headless.sh"
echo "  - Headless host AI bridge: ./scripts/test-boot-ai-headless.sh"
echo ""
echo "Press Ctrl+C to stop the test"
echo "Starting in 3 seconds..."
sleep 3

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

qemu-system-x86_64 \
    -machine q35 \
    -m 2048 \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive file="$ROOT_DIR/build/rayos-universal-usb.img",format=raw \
    -serial stdio \
    -vga std \
    -display gtk,zoom-to-fit=on

echo ""
echo "Test complete!"
