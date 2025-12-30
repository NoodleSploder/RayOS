#!/bin/bash
# Interactive Linux subsystem bring-up (developer tool).
#
# What it does today:
# - Boots the pinned Alpine netboot kernel+initramfs under QEMU.
# - Layers the RayOS guest agent into initramfs (rdinit=/rayos_init).
# - Opens a QEMU display window (virtio-gpu) and attaches serial+monitor to this terminal.
#
# What it does NOT do yet:
# - This is not a Wayland desktop yet. The guest currently runs the minimal agent.
#   You can still interact with it via serial: type `PING` or `SURFACE_TEST`.
#
# Notes:
# - We keep RayOS-as-authority semantics: host controls input, display, and serial.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

# Ensure the agent init runs first.
export USE_AGENT_INITRD=1
export LINUX_GUEST_KIND="${LINUX_GUEST_KIND:-alpine-netboot}"
export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

# Prepare artifacts (downloads/caches Alpine netboot + builds agent initrd overlay).
ARTS="$(PREPARE_ONLY=1 WORK_DIR="$WORK_DIR" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py")"

KERNEL="$(printf "%s\n" "$ARTS" | sed -n 's/^KERNEL=//p' | head -n1)"
INITRD="$(printf "%s\n" "$ARTS" | sed -n 's/^INITRD=//p' | head -n1)"

if [ -z "$KERNEL" ] || [ -z "$INITRD" ]; then
  echo "ERROR: failed to prepare kernel/initrd" >&2
  echo "$ARTS" >&2
  exit 1
fi

echo "Launching Linux subsystem (interactive)..." >&2

echo "QEMU: $QEMU_BIN" >&2

echo "Kernel: $KERNEL" >&2

echo "Initrd: $INITRD" >&2

echo "Cmdline: $LINUX_CMDLINE" >&2

echo "Tip: type PING (then Enter)" >&2

echo "Tip: type SURFACE_TEST (then Enter)" >&2

exec "$QEMU_BIN" \
  -machine q35 \
  -m "${LINUX_MEM:-2048}" \
  -smp "${LINUX_SMP:-2}" \
  -kernel "$KERNEL" \
  -initrd "$INITRD" \
  -append "$LINUX_CMDLINE" \
  -device virtio-gpu-pci \
  -device virtio-keyboard-pci \
  -device virtio-mouse-pci \
  -display gtk \
  -serial stdio \
  -monitor none \
  -no-reboot \
  -net none
