#!/bin/bash
# Automatic Linux desktop bring-up (temporary stepping stone).
#
# Boots Alpine netboot kernel+initramfs under QEMU, overlays RayOS init scripts,
# and uses rdinit=/rayos_desktop_init to automatically:
# - DHCP
# - apk add weston/seatd/weston-terminal
# - start weston + a terminal
#
# The guest prints RAYOS_LINUX_DESKTOP_READY on serial when ready.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

# Ensure only one desktop launcher touches the persistent disk at a time.
LOCK_FILE="$WORK_DIR/.linux-desktop-auto.lock"
if command -v flock >/dev/null 2>&1; then
  exec 9>"$LOCK_FILE"
  if ! flock -n 9; then
    echo "WARN: Linux desktop launcher already running (lock: $LOCK_FILE)" >&2
    exit 0
  fi
else
  # Best-effort fallback: atomic mkdir lock.
  LOCK_DIR="$WORK_DIR/.linux-desktop-auto.lockdir"
  if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    echo "WARN: Linux desktop launcher already running (lock: $LOCK_DIR)" >&2
    exit 0
  fi
  trap 'rmdir "$LOCK_DIR" 2>/dev/null || true' EXIT
fi

# Persistent desktop rootfs disk. The guest init will provision this disk on first boot
# (installing weston/seatd/weston-terminal into a chroot rootfs) and reuse it on later boots.
DESKTOP_DISK_PATH="${LINUX_DESKTOP_DISK:-$WORK_DIR/linux-guest/desktop/desktop-rootfs.ext4}"
DESKTOP_DISK_SIZE="${LINUX_DESKTOP_DISK_SIZE:-4G}"
mkdir -p "$(dirname "$DESKTOP_DISK_PATH")"

if [ ! -f "$DESKTOP_DISK_PATH" ]; then
  echo "Creating persistent desktop disk: $DESKTOP_DISK_PATH ($DESKTOP_DISK_SIZE)" >&2
  if ! command -v mkfs.ext4 >/dev/null 2>&1; then
    echo "ERROR: mkfs.ext4 not found. Install e2fsprogs." >&2
    exit 1
  fi
  truncate -s "$DESKTOP_DISK_SIZE" "$DESKTOP_DISK_PATH"
  mkfs.ext4 -F -L RAYOSDESK "$DESKTOP_DISK_PATH" >/dev/null
fi

# If the disk got corrupted by an abrupt QEMU kill, fix it on the host.
if command -v e2fsck >/dev/null 2>&1; then
  FSCK_LOG="$WORK_DIR/e2fsck-desktop-rootfs.log"
  set +e
  e2fsck -fy "$DESKTOP_DISK_PATH" >"$FSCK_LOG" 2>&1
  FSCK_RC=$?
  set -e

  # 0/1 are typically OK-ish (clean / fixed). However, we've seen cases where
  # the guest still reports ext4 superblock checksum errors. Treat those as
  # hard corruption and recreate the disk (it's just a cache).
  if [ "$FSCK_RC" -ge 4 ] || \
     rg -i -n --fixed-strings -- "invalid superblock" "$FSCK_LOG" >/dev/null 2>&1 || \
     rg -i -n --fixed-strings -- "superblock checksum" "$FSCK_LOG" >/dev/null 2>&1 || \
     rg -i -n --fixed-strings -- "bad checksum" "$FSCK_LOG" >/dev/null 2>&1 || \
     rg -i -n --fixed-strings -- "orphan" "$FSCK_LOG" >/dev/null 2>&1 || \
     rg -i -n --fixed-strings -- "UNEXPECTED INCONSISTENCY" "$FSCK_LOG" >/dev/null 2>&1; then
    echo "WARN: desktop rootfs disk appears corrupted (e2fsck rc=$FSCK_RC); recreating $DESKTOP_DISK_PATH" >&2
    echo "WARN: see $FSCK_LOG" >&2
    rm -f "$DESKTOP_DISK_PATH"
    truncate -s "$DESKTOP_DISK_SIZE" "$DESKTOP_DISK_PATH"
    mkfs.ext4 -F -L RAYOSDESK "$DESKTOP_DISK_PATH" >/dev/null
  fi
fi

QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

# Default: networking OFF (matches contract). We auto-enable it only for first-time
# provisioning when the caller did not explicitly set LINUX_NET.
LINUX_NET_DEFAULTED=0
if [ -z "${LINUX_NET+x}" ]; then
  LINUX_NET=0
  LINUX_NET_DEFAULTED=1
else
  LINUX_NET="${LINUX_NET:-0}"
fi

# Default: virgl OFF (some QEMU builds don't support virtio-gl variants).
# Set LINUX_GL=1 to attempt virgl.
LINUX_GL="${LINUX_GL:-0}"

# Default: GUI ON (GTK window). Set LINUX_HEADLESS=1 to run without a GUI.
LINUX_HEADLESS="${LINUX_HEADLESS:-0}"

disk_ready_marker_exists() {
  # The guest marks provisioning completion by creating:
  #   /rootfs/.rayos_desktop_rootfs_ready
  # inside the ext4 image.
  command -v debugfs >/dev/null 2>&1 || return 1
  debugfs -R 'stat /rootfs/.rayos_desktop_rootfs_ready' "$DESKTOP_DISK_PATH" >/dev/null 2>&1
}

if [ "$LINUX_NET" = "0" ] && [ "$LINUX_NET_DEFAULTED" = "1" ]; then
  if ! disk_ready_marker_exists; then
    # First boot provisioning needs networking to fetch packages.
    LINUX_NET=1
    echo "NOTE: enabling networking for first-time desktop provisioning (set LINUX_NET=0 to forbid)." >&2
    echo "NOTE: to pre-provision headlessly, run: ./tools/linux_subsystem/build_desktop_rootfs_image.sh" >&2
  fi
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
  echo "WARN: MODLOOP not provided; persistent ext4 mount may fail" >&2
fi

VGA_KIND="${LINUX_VGA_KIND:-virtio}"
DISPLAY_ARGS=("-display" "gtk")
if [ "$LINUX_HEADLESS" != "0" ]; then
  DISPLAY_ARGS=("-display" "none")
fi

# If virgl is enabled, prefer an explicit virtio-vga with virgl=on.
# (Many QEMU builds support the device property even when `-vga virtio-gl` is not supported.)
GPU_DEV="${LINUX_GPU_DEV:-virtio-vga}"
GPU_ARGS=()
QEMU_GPU_ARGS=("-vga" "$VGA_KIND")
if [ "$LINUX_GL" != "0" ]; then
  DISPLAY_ARGS=("-display" "gtk,gl=on")
  GPU_DEV="${LINUX_GPU_DEV:-virtio-vga}"
  GPU_ARGS=("-vga" "none" "-device" "${GPU_DEV},virgl=on")
  QEMU_GPU_ARGS=("${GPU_ARGS[@]}")
fi

NET_ARGS=("-net" "none")
if [ "$LINUX_NET" != "0" ]; then
  NET_ARGS=(
    "-netdev" "user,id=n0"
    "-device" "virtio-net-pci,netdev=n0"
  )
fi

CMDLINE="${LINUX_CMDLINE:-console=tty0 console=ttyS0 rdinit=/rayos_desktop_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

echo "Launching auto desktop..." >&2

echo "QEMU: $QEMU_BIN" >&2

echo "VGA: $VGA_KIND" >&2
if [ "$LINUX_GL" != "0" ]; then
  echo "GPU: ${GPU_DEV} (virgl=on)" >&2
fi

echo "Kernel: $KERNEL" >&2

echo "Initrd: $INITRD" >&2

echo "Cmdline: $CMDLINE" >&2

echo "Waiting for: RAYOS_LINUX_DESKTOP_READY" >&2

MON_SOCK="${LINUX_DESKTOP_MONITOR_SOCK:-$WORK_DIR/linux-desktop-monitor.sock}"
rm -f "$MON_SOCK" 2>/dev/null || true

exec "$QEMU_BIN" \
  -machine q35,graphics=on,i8042=on \
  -m "${LINUX_MEM:-4096}" \
  -smp "${LINUX_SMP:-2}" \
  -kernel "$KERNEL" \
  -initrd "$INITRD" \
  -append "$CMDLINE" \
  -drive "file=$DESKTOP_DISK_PATH,format=raw,if=virtio,index=0" \
  ${MODLOOP:+-drive "file=$MODLOOP,format=raw,if=virtio,readonly=on,index=1"} \
  "${QEMU_GPU_ARGS[@]}" \
  -device virtio-keyboard-pci \
  -device virtio-mouse-pci \
  -device qemu-xhci \
  -device usb-kbd \
  -device usb-tablet \
  "${DISPLAY_ARGS[@]}" \
  -serial stdio \
  -monitor "unix:$MON_SOCK,server,nowait" \
  -no-reboot \
  "${NET_ARGS[@]}"
