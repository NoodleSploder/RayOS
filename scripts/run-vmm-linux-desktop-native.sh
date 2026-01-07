#!/bin/bash
# Boot RayOS with the in-kernel VMM Linux guest + virtio-gpu enabled (no host bridge).
# This is intended for interactive development: type `show linux desktop` in RayOS to present.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

# Prepare Linux guest artifacts (kernel + initrd) using the existing tooling.
ARTS_ENV=(
  WORK_DIR="$WORK_DIR"
  PREPARE_ONLY=1
  USE_AGENT_INITRD=1
)

ARTS="$(env "${ARTS_ENV[@]}" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py")"
KERNEL="$(printf "%s\n" "$ARTS" | sed -n 's/^KERNEL=//p' | head -n1)"
INITRD="$(printf "%s\n" "$ARTS" | sed -n 's/^INITRD=//p' | head -n1)"

if [ -z "$KERNEL" ] || [ ! -f "$KERNEL" ]; then
  echo "FAIL: missing KERNEL from run_linux_guest.py" >&2
  exit 1
fi
if [ -z "$INITRD" ] || [ ! -f "$INITRD" ]; then
  echo "FAIL: missing INITRD from run_linux_guest.py" >&2
  exit 1
fi

CMDLINE_FILE="$WORK_DIR/vmm-linux-desktop-native-cmdline.txt"
BASE_CMDLINE="console=ttyS0,115200n8 earlycon=uart,io,0x3f8,115200n8 rdinit=/rayos_init ignore_loglevel loglevel=7 panic=-1"
# virtio-mmio device declaration; address must match the in-kernel MMIO mapping.
echo "$BASE_CMDLINE virtio_mmio.device=0x1000@0x10001000:5" > "$CMDLINE_FILE"

# Enable the in-kernel VMM + Linux guest + virtio-gpu (+ virtio-input for interactive pointer/keys).
export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_linux_guest,vmm_virtio_gpu,vmm_virtio_input}"

export RAYOS_LINUX_GUEST_KERNEL_SRC="$KERNEL"
export RAYOS_LINUX_GUEST_INITRD_SRC="$INITRD"
export RAYOS_LINUX_GUEST_CMDLINE_SRC="$CMDLINE_FILE"

# Explicitly disable host desktop bridge; this script is for RayOS-native presentation.
export ENABLE_HOST_DESKTOP_BRIDGE=0
export PRELAUNCH_HIDDEN_DESKTOPS=0

# Prefer KVM when available; otherwise request a VMX-capable CPU model.
if [ -z "${QEMU_EXTRA_ARGS:-}" ]; then
  if [ -e /dev/kvm ]; then
    export QEMU_EXTRA_ARGS="-enable-kvm -cpu host"
  else
    export QEMU_EXTRA_ARGS="-cpu qemu64,+vmx"
  fi
fi

exec "$ROOT_DIR/scripts/test-boot.sh" "$@"
