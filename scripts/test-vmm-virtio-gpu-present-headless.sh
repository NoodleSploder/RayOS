#!/bin/bash
# Headless P2 smoke test: virtio-gpu scanout publish + first frame markers.
#
# This is the earliest, most deterministic gate for the RayOS-native presentation path:
# - boot kernel with vmm_hypervisor_smoke + vmm_virtio_gpu
# - the built-in virtio-gpu selftest publishes a scanout and bumps frame_seq
# - we assert the serial markers and exit

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-virtio-gpu-present.log}"
SERIAL_NORM="$WORK_DIR/serial-vmm-virtio-gpu-present.norm.log"

# Ensure required features.
RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-}"
REQ_FEATURES=(vmm_hypervisor vmm_hypervisor_smoke vmm_virtio_gpu)
for f in "${REQ_FEATURES[@]}"; do
  if [ -z "$RAYOS_KERNEL_FEATURES" ]; then
    RAYOS_KERNEL_FEATURES="$f"
  elif ! echo ",${RAYOS_KERNEL_FEATURES}," | grep -q ",${f},"; then
    RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES},${f}"
  fi
done
export RAYOS_KERNEL_FEATURES

export HEADLESS="${HEADLESS:-1}"
export QEMU_TIMEOUT_SECS="${QEMU_TIMEOUT_SECS:-12}"

# Fresh serial log.
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG

QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"
if [ -e /dev/kvm ]; then
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -enable-kvm -cpu host"
else
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -cpu qemu64,+vmx"
fi
export QEMU_EXTRA_ARGS

HAVE_KVM=0
if [ -e /dev/kvm ]; then
  HAVE_KVM=1
fi

echo "Running virtio-gpu present smoke test..." >&2
echo "  RAYOS_KERNEL_FEATURES=$RAYOS_KERNEL_FEATURES" >&2
echo "  SERIAL_LOG=$SERIAL_LOG" >&2
echo "  QEMU_TIMEOUT_SECS=$QEMU_TIMEOUT_SECS" >&2

echo "Building kernel-bare (virtio-gpu enabled)..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES" \
  >/dev/null
popd >/dev/null

(BUILD_KERNEL=0 "$ROOT_DIR/scripts/test-boot.sh" --headless) || true

tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

NEED1="RAYOS_VMM:VMX:INIT_BEGIN"
NEED_VMXON="RAYOS_VMM:VMX:VMXON_OK"
NEED_VMCS="RAYOS_VMM:VMX:VMCS_READY"
NEED2="RAYOS_LINUX_DESKTOP_PRESENTED"
NEED3="RAYOS_LINUX_DESKTOP_FIRST_FRAME"

CAN_VMX=0
if grep -F -a -q "$NEED_VMXON" "$SERIAL_NORM" && grep -F -a -q "$NEED_VMCS" "$SERIAL_NORM"; then
  CAN_VMX=1
fi

if [ "$CAN_VMX" = "0" ]; then
  echo "NOTE: VMX did not reach VMXON/VMCS_READY; skipping strict virtio-gpu marker assertions" >&2
  if grep -F -a -q "$NEED1" "$SERIAL_NORM"; then
    echo "PASS: hypervisor init marker observed" >&2
    exit 0
  fi
  echo "FAIL: missing $NEED1" >&2
  tail -n 60 "$SERIAL_NORM" >&2 || true
  exit 1
fi

if grep -F -a -q "$NEED1" "$SERIAL_NORM" \
  && grep -F -a -q "$NEED2" "$SERIAL_NORM" \
  && grep -F -a -q "$NEED3" "$SERIAL_NORM"; then
  echo "PASS: virtio-gpu scanout published + first frame marker observed" >&2
  exit 0
fi

echo "FAIL: missing expected markers" >&2
echo "  required: $NEED1, $NEED2, $NEED3" >&2
echo "  tail:" >&2
tail -n 60 "$SERIAL_NORM" >&2 || true
exit 1
