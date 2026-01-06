#!/bin/bash
# Headless smoke test for virtio-input (P3 begin).
#
# What it validates:
# - kernel boots with `vmm_hypervisor` + `vmm_virtio_input`
# - guest blob posts writable buffers and notifies the virtqueue
# - VMM stashes buffers and writes at least one deterministic keepalive/event marker
#
# Notes:
# - VMX may not be available in all environments; when VMX bring-up doesn't reach
#   VMCS_READY, we only assert the init marker and skip the input marker.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-virtio-input.log}"
SERIAL_NORM="$WORK_DIR/serial-vmm-virtio-input.norm.log"

# Avoid mixing output from prior runs.
: > "$SERIAL_LOG"

export HEADLESS="${HEADLESS:-1}"
TEST_TIMEOUT_SECS="${TEST_TIMEOUT_SECS:-12}"

source "$ROOT_DIR/scripts/lib/headless_qemu.sh"

export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG

# Build a guest blob that posts a writeable event buffer descriptor.
# Also ensure we restore the default (blk-oriented) blob on exit.
pushd "$ROOT_DIR/scripts" >/dev/null
rustc generate_guest_driver.rs -O -o ./generate_guest_driver >/dev/null 2>&1 || true

cleanup() {
  # Best-effort restore of the default guest blob so running this test
  # doesn’t leave the working tree dirty.
  RAYOS_GUEST_INPUT_ENABLED=0 RAYOS_GUEST_NET_ENABLED=0 RAYOS_GUEST_CONSOLE_ENABLED=0 ./generate_guest_driver >/dev/null 2>&1 || true
  rm -f ./generate_guest_driver >/dev/null 2>&1 || true
}
trap cleanup EXIT

RAYOS_GUEST_INPUT_ENABLED=1 RAYOS_GUEST_NET_ENABLED=0 RAYOS_GUEST_CONSOLE_ENABLED=0 ./generate_guest_driver >/dev/null 2>&1 || true

popd >/dev/null

# Build kernel-bare with virtio-input enabled.
export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_hypervisor_smoke,vmm_virtio_input}"

QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"
if [ -e /dev/kvm ]; then
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -enable-kvm -cpu host"
else
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -cpu qemu64,+vmx"
fi
export QEMU_EXTRA_ARGS

echo "Running virtio-input smoke test..." >&2
echo "  RAYOS_KERNEL_FEATURES=$RAYOS_KERNEL_FEATURES" >&2
echo "  SERIAL_LOG=$SERIAL_LOG" >&2
echo "  TEST_TIMEOUT_SECS=$TEST_TIMEOUT_SECS" >&2

echo "Building kernel-bare (virtio-input enabled)..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES" \
  >/dev/null
popd >/dev/null

MON_SOCK="$WORK_DIR/qemu-monitor-vmm-virtio-input-headless.sock"

run_headless_boot_until \
  "$SERIAL_LOG" \
  "$MON_SOCK" \
  "$TEST_TIMEOUT_SECS" \
  'RAYOS_GUEST:VIRTIO_INPUT:EVENT_RX' \
  'RAYOS_GUEST:VIRTIO_INPUT:TIMEOUT' \
  'RAYOS_VMM:VIRTIO_INPUT:(KEEPALIVE_WRITTEN|EVENT_WRITTEN|BUF_STASHED)' \
  'RAYOS_VMM:VMX:VMCS_READY'

tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

NEED_INIT="RAYOS_VMM:VMX:INIT_BEGIN"
NEED_VMXON="RAYOS_VMM:VMX:VMXON_OK"
NEED_VMCS="RAYOS_VMM:VMX:VMCS_READY"
NEED_INPUT_A="RAYOS_VMM:VIRTIO_INPUT:KEEPALIVE_WRITTEN"
NEED_INPUT_B="RAYOS_VMM:VIRTIO_INPUT:EVENT_WRITTEN"
NEED_INPUT_C="RAYOS_VMM:VIRTIO_INPUT:BUF_STASHED"
NEED_GUEST_OK="RAYOS_GUEST:VIRTIO_INPUT:EVENT_RX"
NEED_GUEST_TIMEOUT="RAYOS_GUEST:VIRTIO_INPUT:TIMEOUT"

if ! grep -F -a -q "$NEED_INIT" "$SERIAL_NORM"; then
  echo "FAIL: missing hypervisor init marker ($NEED_INIT)" >&2
  exit 1
fi

# Gate strict assertions on VMX actually reaching VMCS_READY.
if grep -F -a -q "$NEED_VMXON" "$SERIAL_NORM" && grep -F -a -q "$NEED_VMCS" "$SERIAL_NORM"; then
  if grep -F -a -q "$NEED_GUEST_TIMEOUT" "$SERIAL_NORM"; then
    echo "FAIL: guest timed out waiting for virtio-input completion ($NEED_GUEST_TIMEOUT)" >&2
    tail -n 120 "$SERIAL_NORM" >&2 || true
    exit 1
  fi
  if grep -F -a -q "$NEED_GUEST_OK" "$SERIAL_NORM"; then
    echo "PASS: guest observed virtio-input event completion" >&2
    exit 0
  fi
  if grep -F -a -q "$NEED_INPUT_A" "$SERIAL_NORM" \
    || grep -F -a -q "$NEED_INPUT_B" "$SERIAL_NORM" \
    || grep -F -a -q "$NEED_INPUT_C" "$SERIAL_NORM"; then
    echo "PASS: virtio-input queue activity marker observed" >&2
    exit 0
  fi
  echo "FAIL: missing virtio-input markers ($NEED_GUEST_OK or $NEED_INPUT_A or $NEED_INPUT_B or $NEED_INPUT_C)" >&2
  tail -n 120 "$SERIAL_NORM" >&2 || true
  exit 1
else
  echo "NOTE: VMX did not reach VMXON/VMCS_READY; skipping strict virtio-input assertions" >&2
  exit 0
fi
