#!/bin/bash
# Interactive "more complete" Linux environment (developer tool).
#
# Boots the Alpine virt ISO in a QEMU window so you can install packages
# (e.g., a compositor like weston) and experiment while we build the proper Wayland bridge.
#
# This is still under RayOS control (host decides devices/network).

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

# Default: networking off (matches contract). Set LINUX_NET=1 to enable user-mode networking.
LINUX_NET="${LINUX_NET:-0}"

# Optional: enable virgl/OpenGL for smoother graphics (requires host GL support).
LINUX_GL="${LINUX_GL:-0}"

# Optional: persistent disk (qcow2). If provided, it's attached as virtio-blk.
DISK_PATH="${LINUX_DISK:-$WORK_DIR/linux-guest/alpine-iso/alpine-persist.qcow2}"
DISK_GB="${LINUX_DISK_GB:-16}"
USE_DISK="${LINUX_USE_DISK:-1}"

ISO_PATH="$(WORK_DIR="$WORK_DIR" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/download_alpine_iso.py")"

echo "Launching Alpine ISO (interactive)..." >&2

echo "ISO: $ISO_PATH" >&2

echo "Tip: login is usually 'root' with no password on the live ISO." >&2
echo "Tip: run 'setup-alpine' to install to disk (optional)." >&2
echo "Tip (Wayland): after networking is up, try: apk add weston weston-terminal seatd && seatd -g video & weston" >&2

NET_ARGS=("-net" "none")
if [ "$LINUX_NET" != "0" ]; then
  NET_ARGS=(
    "-netdev" "user,id=n0"
    "-device" "virtio-net-pci,netdev=n0"
  )
  echo "NOTE: networking enabled (LINUX_NET=1)" >&2
fi

DISK_ARGS=()
if [ "$USE_DISK" != "0" ]; then
  mkdir -p "$(dirname "$DISK_PATH")"
  if [ ! -f "$DISK_PATH" ]; then
    echo "Creating persistent disk: $DISK_PATH (${DISK_GB}G)" >&2
    qemu-img create -f qcow2 "$DISK_PATH" "${DISK_GB}G" >/dev/null
  fi
  DISK_ARGS=(
    "-drive" "file=$DISK_PATH,if=none,format=qcow2,id=vd0"
    "-device" "virtio-blk-pci,drive=vd0"
  )
fi

GPU_DEV="virtio-gpu-pci"
DISPLAY_ARGS=("-display" "gtk")
if [ "$LINUX_GL" != "0" ]; then
  GPU_DEV="virtio-gpu-gl-pci"
  DISPLAY_ARGS=("-display" "gtk,gl=on")
  echo "NOTE: virgl/OpenGL enabled (LINUX_GL=1)" >&2
fi

exec "$QEMU_BIN" \
  -machine q35 \
  -m "${LINUX_MEM:-2048}" \
  -smp "${LINUX_SMP:-2}" \
  -cdrom "$ISO_PATH" \
  -boot d \
  "${DISK_ARGS[@]}" \
  -device "$GPU_DEV" \
  -device virtio-keyboard-pci \
  -device virtio-mouse-pci \
  "${DISPLAY_ARGS[@]}" \
  -serial stdio \
  -monitor none \
  -no-reboot \
  "${NET_ARGS[@]}"
