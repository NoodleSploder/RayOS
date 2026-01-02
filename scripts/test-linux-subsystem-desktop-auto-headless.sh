#!/bin/bash
# Headless Linux desktop auto-bringup smoke test.
#
# Boots the Alpine netboot kernel+initramfs with the RayOS desktop init (rdinit=/rayos_desktop_init),
# attaches the persistent desktop rootfs disk, and asserts we see RAYOS_LINUX_DESKTOP_READY.
#
# If the persistent disk has not been provisioned yet, this script provisions it once (slow).

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

TIMEOUT_SECS="${TIMEOUT_SECS:-120}"
PROVISION_TIMEOUT_SECS="${PROVISION_TIMEOUT_SECS:-1200}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

LOG_FILE="${LOG_FILE:-$WORK_DIR/linux-subsystem-desktop-auto-headless.log}"
PROVISION_LOG_FILE="${PROVISION_LOG_FILE:-$WORK_DIR/linux-subsystem-desktop-provision.log}"

DESKTOP_DISK_PATH="${LINUX_DESKTOP_DISK:-$WORK_DIR/linux-guest/desktop/desktop-rootfs.ext4}"

rm -f "$LOG_FILE" "$PROVISION_LOG_FILE" 2>/dev/null || true

need_provision=1
if command -v debugfs >/dev/null 2>&1 && [ -f "$DESKTOP_DISK_PATH" ]; then
  if debugfs -R 'stat /rootfs/.rayos_desktop_rootfs_ready' "$DESKTOP_DISK_PATH" >/dev/null 2>&1; then
    need_provision=0
  fi
fi

if [ "$need_provision" = "1" ]; then
  echo "Desktop rootfs not provisioned; provisioning once (this can take a while)..." >&2
  echo "QEMU: $QEMU_BIN" >&2
  echo "Log: $PROVISION_LOG_FILE" >&2

  # Provision in a separate process (headless, networking on inside QEMU user-net).
  set +e
  (cd "$ROOT_DIR" && TIMEOUT_SECS="$PROVISION_TIMEOUT_SECS" QEMU_BIN="$QEMU_BIN" \
    ./tools/linux_subsystem/build_desktop_rootfs_image.sh) >"$PROVISION_LOG_FILE" 2>&1
  prov_rc=$?
  set -e

  if [ "$prov_rc" -ne 0 ]; then
    echo "FAIL: provisioning failed (rc=$prov_rc)" >&2
    tail -n 200 "$PROVISION_LOG_FILE" 2>/dev/null || true
    exit 1
  fi
fi

# Now boot with rdinit=/rayos_desktop_init and assert READY.
export WORK_DIR TIMEOUT_SECS QEMU_BIN LOG_FILE
export USE_AGENT_INITRD=1
export READY_MARKER=RAYOS_LINUX_DESKTOP_READY
export INJECT_READY_MARKER=0

# Desktop init needs modloop (kernel modules), networking is off in this phase.
export LINUX_CMDLINE="${LINUX_CMDLINE:-console=ttyS0 rdinit=/rayos_desktop_init loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1}"

# Ask the runner to attach the virtio GPU + the persistent disk + modloop.
export QEMU_GPU=1


# Check for VM registry and log lifecycle state
VM_REGISTRY_PATH="$WORK_DIR/linux-guest/desktop/vm_registry.json"
if [ -f "$VM_REGISTRY_PATH" ]; then
  echo "[test] VM registry found: resuming persistent VM" >&2
else
  echo "[test] No VM registry: fresh start" >&2
fi

ARTS="$(PREPARE_ONLY=1 WORK_DIR="$WORK_DIR" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py")"
KERNEL="$(printf "%s\n" "$ARTS" | sed -n 's/^KERNEL=//p' | head -n1)"
INITRD="$(printf "%s\n" "$ARTS" | sed -n 's/^INITRD=//p' | head -n1)"
MODLOOP="$(printf "%s\n" "$ARTS" | sed -n 's/^MODLOOP=//p' | head -n1)"

if [ -z "$KERNEL" ] || [ -z "$INITRD" ] || [ -z "$MODLOOP" ]; then
  echo "FAIL: could not resolve Alpine netboot artifacts" >&2
  echo "$ARTS" >&2
  exit 1
fi

# Note: indexes match rayos_desktop_init expectations (vda=persist, vdb=modloop).
export QEMU_EXTRA_ARGS="-drive file=$DESKTOP_DISK_PATH,format=raw,if=virtio,index=0 -drive file=$MODLOOP,format=raw,if=virtio,readonly=on,index=1 -device virtio-keyboard-pci -device virtio-mouse-pci"

# After the desktop reports READY, launch a Wayland client and request a clean shutdown.
# The guest command channel is provided by /rayos_agent.sh (desktop mode).
export POST_READY_SEND=$'LAUNCH_APP:weston-terminal\nSHUTDOWN\n'
export POST_READY_EXPECT=RAYOS_LINUX_AGENT_SHUTDOWN_ACK

echo "Starting Linux desktop auto bring-up (headless)..." >&2

echo "QEMU: $QEMU_BIN" >&2

echo "Timeout: ${TIMEOUT_SECS}s" >&2

echo "Log: $LOG_FILE" >&2

python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py"

NORM="$WORK_DIR/linux-subsystem-desktop-auto-headless.norm.log"
tr -d '\r' < "$LOG_FILE" > "$NORM" 2>/dev/null || true

if ! grep -F -a -q "RAYOS_LINUX_DESKTOP_READY" "$NORM"; then
  echo "FAIL: missing desktop ready marker" >&2
  tail -n 250 "$NORM" 2>/dev/null || true
  exit 1
fi

if ! grep -F -a -q "RAYOS_LINUX_APP_LAUNCH_OK name=weston-terminal" "$NORM"; then
  echo "FAIL: missing Wayland client launch marker" >&2
  tail -n 250 "$NORM" 2>/dev/null || true
  exit 1
fi

if ! grep -F -a -q "RAYOS_LINUX_AGENT_SHUTDOWN_ACK" "$NORM"; then
  echo "FAIL: missing clean shutdown ack" >&2
  tail -n 250 "$NORM" 2>/dev/null || true
  exit 1
fi

echo "PASS: desktop ready + client launched + shutdown ack" >&2
exit 0
