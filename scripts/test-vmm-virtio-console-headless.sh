#!/bin/bash
# Headless smoke test for virtio-console (P1 console transport).
#
# What it validates:
# - kernel boots with `vmm_hypervisor` + `vmm_virtio_console`
# - guest blob submits a virtio-console dataq message
# - VMM prints the received payload to the RayOS serial log
#
# Notes:
# - VMX may not be available in all environments; without VMX/VMCS_READY the guest may not run.
#   In that case, we only assert hypervisor init markers and skip the guest-driven assertion.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-virtio-console.log}"
SERIAL_NORM="$WORK_DIR/serial-vmm-virtio-console.norm.log"

: > "$SERIAL_LOG"

export HEADLESS="${HEADLESS:-1}"
TEST_TIMEOUT_SECS="${TEST_TIMEOUT_SECS:-14}"

source "$ROOT_DIR/scripts/lib/headless_qemu.sh"

export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG

# Build a guest blob that writes a console message.
# Also ensure we restore the default (blk-oriented) blob on exit.
pushd "$ROOT_DIR/scripts" >/dev/null
rustc generate_guest_driver.rs -O -o ./generate_guest_driver >/dev/null 2>&1 || true

cleanup() {
  RAYOS_GUEST_INPUT_ENABLED=0 RAYOS_GUEST_NET_ENABLED=0 RAYOS_GUEST_CONSOLE_ENABLED=0 ./generate_guest_driver >/dev/null 2>&1 || true
  rm -f ./generate_guest_driver >/dev/null 2>&1 || true
}
trap cleanup EXIT

RAYOS_GUEST_INPUT_ENABLED=0 RAYOS_GUEST_NET_ENABLED=0 RAYOS_GUEST_CONSOLE_ENABLED=1 ./generate_guest_driver >/dev/null 2>&1 || true
popd >/dev/null

# Build kernel-bare with virtio-console enabled.
export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_hypervisor_smoke,vmm_virtio_console}"

QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"
if [ -e /dev/kvm ]; then
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -enable-kvm -cpu host"
else
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -cpu qemu64,+vmx"
fi
export QEMU_EXTRA_ARGS

echo "Running virtio-console smoke test..." >&2
echo "  RAYOS_KERNEL_FEATURES=$RAYOS_KERNEL_FEATURES" >&2
echo "  SERIAL_LOG=$SERIAL_LOG" >&2
echo "  TEST_TIMEOUT_SECS=$TEST_TIMEOUT_SECS" >&2

echo "Building kernel-bare (virtio-console enabled)..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES" \
  >/dev/null
popd >/dev/null

MON_SOCK="$WORK_DIR/qemu-monitor-vmm-virtio-console-headless.sock"

run_headless_boot_until \
  "$SERIAL_LOG" \
  "$MON_SOCK" \
  "$TEST_TIMEOUT_SECS" \
  'RAYOS_VMM:VMX:UNSUPPORTED' \
  'RAYOS_VMM:VIRTIO_CONSOLE:RECV|G:CONSOLE' \
  'RAYOS_VMM:VMX:VMCS_READY|RAYOS_VMM:VMX:VMEXIT'

tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

if grep -F -a -q "RAYOS_VMM:VMX:UNSUPPORTED" "$SERIAL_NORM"; then
  echo "SKIP: VMX unsupported in this QEMU configuration" >&2
  exit 0
fi

NEED_INIT="RAYOS_VMM:VMX:INIT_BEGIN"
NEED_VMXON="RAYOS_VMM:VMX:VMXON_OK"
NEED_VMCS="RAYOS_VMM:VMX:VMCS_READY"
NEED_RECV="RAYOS_VMM:VIRTIO_CONSOLE:RECV"
NEED_GUEST_ALT="G:CONSOLE"

if ! grep -F -a -q "$NEED_INIT" "$SERIAL_NORM"; then
  echo "FAIL: missing hypervisor init marker ($NEED_INIT)" >&2
  exit 1
fi

# Gate strict assertions on VMX reaching VMCS_READY.
if grep -F -a -q "$NEED_VMXON" "$SERIAL_NORM" && grep -F -a -q "$NEED_VMCS" "$SERIAL_NORM"; then
  if grep -F -a -q "$NEED_RECV" "$SERIAL_NORM" || grep -F -a -q "$NEED_GUEST_ALT" "$SERIAL_NORM"; then
    echo "PASS: guest-driven virtio-console message observed" >&2
    exit 0
  fi
  echo "FAIL: missing guest-driven virtio-console markers ($NEED_RECV or $NEED_GUEST_ALT)" >&2
  tail -n 120 "$SERIAL_NORM" >&2 || true
  exit 1
else
  echo "NOTE: VMX did not reach VMXON/VMCS_READY; skipping strict virtio-console assertions" >&2
  exit 0
fi
