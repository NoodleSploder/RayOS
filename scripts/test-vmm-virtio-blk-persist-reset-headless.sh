#!/bin/bash
# Headless smoke test: virtio-blk backing persists across a QEMU `system_reset`.
#
# This validates a core hypervisor milestone for “reboot persistence”:
# - The in-kernel virtio-blk backing store survives a reset
# - The kernel does NOT reinitialize/overwrite it on the next boot
#
# Implementation notes:
# - The kernel feature `vmm_virtio_blk_persist_selftest` writes a marker into the
#   virtio-blk backing if missing and emits `PERSIST_NEEDS_RESET`.
# - This script watches serial output, issues a QEMU `system_reset` once, then
#   waits for `PERSIST_OK` on the second boot.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-virtio-blk-persist.log}"
SERIAL_NORM="$WORK_DIR/serial-vmm-virtio-blk-persist.norm.log"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-vmm-virtio-blk-persist.sock}"
TIMEOUT_SECS="${TIMEOUT_SECS:-45}"

# Avoid mixing output from prior runs.
: > "$SERIAL_LOG"

export HEADLESS=1
export QEMU_TIMEOUT_SECS="$TIMEOUT_SECS"
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG
export MON_SOCK

# Avoid host-side desktop bridges for this test.
export ENABLE_HOST_DESKTOP_BRIDGE=0
export PRELAUNCH_HIDDEN_DESKTOPS=0

# Build kernel-bare with the persistence selftest enabled.
export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_hypervisor_smoke,vmm_virtio_blk_persist_selftest}"

QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"
if [ -e /dev/kvm ]; then
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -enable-kvm -cpu host"
else
  QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS} -cpu qemu64,+vmx"
fi
export QEMU_EXTRA_ARGS

echo "Running virtio-blk reboot persistence smoke test..." >&2
echo "  RAYOS_KERNEL_FEATURES=$RAYOS_KERNEL_FEATURES" >&2
echo "  SERIAL_LOG=$SERIAL_LOG" >&2
echo "  MON_SOCK=$MON_SOCK" >&2
echo "  TIMEOUT_SECS=$TIMEOUT_SECS" >&2

echo "Building kernel-bare (persist selftest enabled)..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES" \
  >/dev/null
popd >/dev/null

# Start QEMU/RayOS.
"$ROOT_DIR/scripts/test-boot.sh" --headless &
BOOT_PID=$!

source "$ROOT_DIR/scripts/lib/headless_qemu.sh"

system_reset_qemu() {
  local sock="$1"
  python3 - "$sock" <<'PY'
import socket, sys
path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(path)
s.sendall(b"system_reset\r\n")
s.close()
PY
}

# Wait for monitor socket so we can reset/quit deterministically.
MON_WAIT_DEADLINE=$(( $(date +%s) + 20 ))
while [ ! -S "$MON_SOCK" ]; do
  if [ "$(date +%s)" -ge "$MON_WAIT_DEADLINE" ]; then
    echo "FAIL: monitor socket not created: $MON_SOCK" >&2
    kill "$BOOT_PID" 2>/dev/null || true
    wait "$BOOT_PID" 2>/dev/null || true
    exit 1
  fi
  sleep 0.1
done

NEED_INIT="RAYOS_VMM:VMX:INIT_BEGIN"
NEED_RESET="RAYOS_VMM:VIRTIO_BLK:PERSIST_NEEDS_RESET"
NEED_OK="RAYOS_VMM:VIRTIO_BLK:PERSIST_OK"
NEED_UNSUPPORTED="RAYOS_VMM:VMX:UNSUPPORTED"

DEADLINE=$(( $(date +%s) + TIMEOUT_SECS ))
DID_RESET=0

while true; do
  tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

  # If VMX isn't supported in this environment, treat as a skip.
  if grep -F -a -q "$NEED_UNSUPPORTED" "$SERIAL_NORM"; then
    echo "SKIP: VMX unsupported in this QEMU configuration" >&2
    quit_qemu "$MON_SOCK" || true
    wait "$BOOT_PID" 2>/dev/null || true
    exit 0
  fi

  if grep -F -a -q "$NEED_OK" "$SERIAL_NORM"; then
    echo "PASS: observed $NEED_OK" >&2
    quit_qemu "$MON_SOCK" || true
    break
  fi

  if [ "$DID_RESET" = "0" ] && grep -F -a -q "$NEED_RESET" "$SERIAL_NORM"; then
    echo "INFO: persistence selftest requested reset; issuing system_reset" >&2
    system_reset_qemu "$MON_SOCK" || true
    DID_RESET=1
    # Give firmware/kernel time to restart.
    sleep 1
  fi

  if [ "$(date +%s)" -ge "$DEADLINE" ]; then
    echo "FAIL: timed out waiting for persistence markers" >&2
    if ! grep -F -a -q "$NEED_INIT" "$SERIAL_NORM"; then
      echo "  also missing init marker ($NEED_INIT)" >&2
    fi
    tail -n 200 "$SERIAL_NORM" >&2 || true
    quit_qemu "$MON_SOCK" || true
    break
  fi

  sleep 0.2
done

wait "$BOOT_PID" 2>/dev/null || true

# Final check.
tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true
if grep -F -a -q "$NEED_OK" "$SERIAL_NORM"; then
  exit 0
fi
exit 1
