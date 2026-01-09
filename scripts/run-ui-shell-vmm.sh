#!/bin/bash
# Run RayOS with UI Shell + Full VMM Linux Guest
#
# This script runs RayOS with:
# - ui_shell: Native windowed UI framework
# - vmm_hypervisor: In-kernel hypervisor (VMX on x86_64)
# - vmm_linux_guest: Real Linux guest VM
# - vmm_virtio_gpu: GPU virtualization for guest display
# - vmm_virtio_input: Keyboard/mouse passthrough to guest
#
# When running, type "show linux desktop" to the AI to display
# the Linux VM's Wayland desktop in a RayOS window.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

# Prepare Linux guest artifacts (kernel + initrd) using the existing tooling.
echo "Preparing Linux guest kernel and initrd..." >&2
ARTS_ENV=(
  WORK_DIR="$WORK_DIR"
  PREPARE_ONLY=1
  USE_AGENT_INITRD=1
)

ARTS="$(env "${ARTS_ENV[@]}" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py" 2>/dev/null || true)"
KERNEL="$(printf "%s\n" "$ARTS" | sed -n 's/^KERNEL=//p' | head -n1)"
INITRD="$(printf "%s\n" "$ARTS" | sed -n 's/^INITRD=//p' | head -n1)"

if [ -z "$KERNEL" ] || [ ! -f "$KERNEL" ]; then
  echo "WARNING: Linux guest kernel not available. Using dev_scanout test surface instead." >&2
  USE_DEV_SCANOUT=1
else
  USE_DEV_SCANOUT=0
  echo "Found Linux kernel: $KERNEL" >&2
  echo "Found initrd: $INITRD" >&2
fi

# Build kernel command line for Linux guest
if [ "$USE_DEV_SCANOUT" = "0" ]; then
  CMDLINE_FILE="$WORK_DIR/vmm-linux-desktop-ui-shell-cmdline.txt"
  BASE_CMDLINE="console=ttyS0,115200n8 earlycon=uart,io,0x3f8,115200n8 rdinit=/rayos_init ignore_loglevel loglevel=7 panic=-1 RAYOS_INPUT_PROBE=1"
  # virtio-mmio device declaration; derive addresses/IRQs from hypervisor.rs.
  VIRTIO_MMIO_DEVICES="$(python3 "$ROOT_DIR/scripts/tools/vmm_mmio_map.py" --features "vmm_linux_guest,vmm_virtio_gpu,vmm_virtio_input" 2>/dev/null || true)"
  echo "$BASE_CMDLINE $VIRTIO_MMIO_DEVICES" > "$CMDLINE_FILE"

  # Set environment for the kernel build
  export RAYOS_LINUX_GUEST_KERNEL_SRC="$KERNEL"
  export RAYOS_LINUX_GUEST_INITRD_SRC="$INITRD"
  export RAYOS_LINUX_GUEST_CMDLINE_SRC="$CMDLINE_FILE"

  # Full VMM feature set + UI shell + autostart the Linux desktop
  RAYOS_KERNEL_FEATURES="ui_shell,serial_debug,vmm_hypervisor,vmm_linux_guest,vmm_linux_desktop_autostart,vmm_virtio_gpu,vmm_virtio_input"
else
  # Fallback to dev_scanout for testing when Linux guest isn't available
  RAYOS_KERNEL_FEATURES="ui_shell,serial_debug,dev_scanout"
fi

export RAYOS_KERNEL_FEATURES

# Disable headless to show the window
export HEADLESS=0

# Explicitly disable host desktop bridge; use RayOS-native presentation
export ENABLE_HOST_DESKTOP_BRIDGE=0
export PRELAUNCH_HIDDEN_DESKTOPS=0

# Prefer KVM when available; otherwise request a VMX-capable CPU model
if [ -z "${QEMU_EXTRA_ARGS:-}" ]; then
  if [ -e /dev/kvm ]; then
    export QEMU_EXTRA_ARGS="-enable-kvm -cpu host"
  else
    export QEMU_EXTRA_ARGS="-cpu qemu64,+vmx"
  fi
fi

# Build the kernel
echo "Building kernel-bare with features: $RAYOS_KERNEL_FEATURES" >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES"
popd >/dev/null

echo "" >&2
echo "Kernel built. Launching with graphical display..." >&2
echo "Type 'show linux desktop' to display the Linux VM." >&2
echo "" >&2

# Run test-boot.sh which handles ISO creation and QEMU launch
BUILD_KERNEL=0 "$ROOT_DIR/scripts/test-boot.sh" || true

echo ""
echo "=== Session ended ===" >&2
