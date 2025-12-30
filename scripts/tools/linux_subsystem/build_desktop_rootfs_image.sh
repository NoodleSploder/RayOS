#!/bin/bash
# Build/provision the persistent Linux desktop rootfs image used by run-linux-subsystem-desktop-auto.sh
#
# This runs a headless QEMU netboot once, installs weston/seatd/weston-terminal into a chroot rootfs
# stored on a persistent ext4 disk image, and powers off.
#
# After this finishes, subsequent boots via run-linux-subsystem-desktop-auto.sh skip provisioning.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )/../.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

DESKTOP_DISK_PATH="${LINUX_DESKTOP_DISK:-$WORK_DIR/linux-guest/desktop/desktop-rootfs.ext4}"
DESKTOP_DISK_SIZE="${LINUX_DESKTOP_DISK_SIZE:-4G}"
mkdir -p "$(dirname "$DESKTOP_DISK_PATH")"

if [ ! -f "$DESKTOP_DISK_PATH" ]; then
  echo "Creating desktop disk: $DESKTOP_DISK_PATH ($DESKTOP_DISK_SIZE)" >&2
  if ! command -v mkfs.ext4 >/dev/null 2>&1; then
    echo "ERROR: mkfs.ext4 not found. Install e2fsprogs." >&2
    exit 1
  fi
  truncate -s "$DESKTOP_DISK_SIZE" "$DESKTOP_DISK_PATH"
  mkfs.ext4 -F -L RAYOSDESK "$DESKTOP_DISK_PATH" >/dev/null
fi

# Prepare artifacts and agent overlay.
export USE_AGENT_INITRD=1
ARTS="$(PREPARE_ONLY=1 WORK_DIR="$WORK_DIR" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py")"
KERNEL="$(printf "%s\n" "$ARTS" | sed -n 's/^KERNEL=//p' | head -n1)"
INITRD="$(printf "%s\n" "$ARTS" | sed -n 's/^INITRD=//p' | head -n1)"
MODLOOP="$(printf "%s\n" "$ARTS" | sed -n 's/^MODLOOP=//p' | head -n1)"

if [ -z "$KERNEL" ] || [ -z "$INITRD" ]; then
  echo "ERROR: failed to prepare kernel/initrd" >&2
  echo "$ARTS" >&2
  exit 1
fi

if [ -z "$MODLOOP" ]; then
  echo "ERROR: MODLOOP not provided; cannot mount ext4 in initramfs reliably" >&2
  echo "$ARTS" >&2
  exit 1
fi

CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_desktop_init rayos_desktop_provision_only=1 loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Provisioning desktop rootfs image (headless)..." >&2

echo "Disk: $DESKTOP_DISK_PATH" >&2

echo "Kernel: $KERNEL" >&2

echo "Initrd: $INITRD" >&2

echo "Cmdline: $CMDLINE" >&2

exec "$QEMU_BIN" \
  -machine q35 \
  -m "${LINUX_MEM:-1024}" \
  -smp "${LINUX_SMP:-2}" \
  -kernel "$KERNEL" \
  -initrd "$INITRD" \
  -append "$CMDLINE" \
  -drive "file=$DESKTOP_DISK_PATH,format=raw,if=virtio" \
  -drive "file=$MODLOOP,format=raw,if=virtio,readonly=on" \
  -display none \
  -vga none \
  -serial stdio \
  -monitor none \
  -no-reboot \
  -netdev user,id=n0 \
  -device virtio-net-pci,netdev=n0
