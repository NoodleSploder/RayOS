#!/bin/bash
# Headless smoke test for virtio-net packet loopback in the in-kernel hypervisor.
#
# What it validates:
# - kernel boots with `vmm_hypervisor_net_test` enabled
# - VMX bring-up path runs and emits deterministic serial markers
# - virtio-net device is initialized and loopback injection occurs
# - Network RX injection markers are observed
#
# Notes:
# - This script prefers KVM if /dev/kvm is present.
# - Without KVM, it will try TCG with a VMX-capable CPU model; VMX instructions
#   may still not be usable depending on QEMU build.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-hypervisor-net.log}"
SERIAL_NORM="$WORK_DIR/serial-vmm-hypervisor-net.norm.log"

# Ensure the features are enabled (append if user provided other features).
RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-}"
if [ -z "$RAYOS_KERNEL_FEATURES" ]; then
  RAYOS_KERNEL_FEATURES="vmm_hypervisor,vmm_hypervisor_net_test"
elif ! echo ",${RAYOS_KERNEL_FEATURES}," | grep -q ",vmm_hypervisor,"; then
  RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES},vmm_hypervisor"
fi

if ! echo ",${RAYOS_KERNEL_FEATURES}," | grep -q ",vmm_hypervisor_net_test,"; then
  RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES},vmm_hypervisor_net_test"
fi
export RAYOS_KERNEL_FEATURES

# Default to headless for CI/determinism, but allow `HEADLESS=0` to show a QEMU window.
export HEADLESS="${HEADLESS:-1}"

# Keep this a finite smoke test (only when headless, so interactive runs aren't killed).
if [ "$HEADLESS" != "0" ]; then
  export QEMU_TIMEOUT_SECS="${QEMU_TIMEOUT_SECS:-15}"
fi

# Make sure we capture a fresh serial.
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG

QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"
if [ -e /dev/kvm ]; then
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -enable-kvm -cpu host"
else
  # Best-effort: expose VMX in the emulated CPU.
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -cpu qemu64,+vmx"
fi
export QEMU_EXTRA_ARGS

echo "Running hypervisor virtio-net loopback smoke test..." >&2
echo "  RAYOS_KERNEL_FEATURES=$RAYOS_KERNEL_FEATURES" >&2
echo "  SERIAL_LOG=$SERIAL_LOG" >&2
echo "  QEMU_TIMEOUT_SECS=$QEMU_TIMEOUT_SECS" >&2
echo "  QEMU_EXTRA_ARGS=$QEMU_EXTRA_ARGS" >&2

echo "Building kernel-bare with vmm_hypervisor_net_test enabled..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES" \
  >/dev/null
popd >/dev/null

if [ "$HEADLESS" = "0" ]; then
  BUILD_KERNEL=0 "$ROOT_DIR/scripts/test-boot.sh" || true
else
  BUILD_KERNEL=0 "$ROOT_DIR/scripts/test-boot.sh" --headless || true
fi

# Normalize CRLF.
tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

NEED1="RAYOS_VMM:VMX:INIT_BEGIN"
NEED2="RAYOS_VMM:VMX:VMLAUNCH_ATTEMPT"
NEED3="RAYOS_VMM:VIRTIO_MMIO:NET_RX_INJECT"

if grep -F -a -q "$NEED1" "$SERIAL_NORM" && grep -F -a -q "$NEED2" "$SERIAL_NORM"; then
  echo "PASS: hypervisor init path executed (markers present)" >&2
  if grep -F -a -q "$NEED3" "$SERIAL_NORM"; then
    echo "PASS: virtio-net loopback injection observed" >&2
    echo "Serial log: $SERIAL_LOG" >&2
    exit 0
  else
    echo "FAIL: no virtio-net RX injection marker observed" >&2
    echo "NOTE: This may indicate the test packet injection didn't trigger" >&2
    echo "NOTE: Check if guest driver assembly needs debugging" >&2
  fi
else
  echo "FAIL: missing expected VMX markers" >&2
fi

echo "Serial log: $SERIAL_LOG" >&2
tail -n 200 "$SERIAL_NORM" 2>/dev/null || true
exit 1