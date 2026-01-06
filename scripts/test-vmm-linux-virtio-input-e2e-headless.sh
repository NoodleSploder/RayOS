#!/bin/bash
# Headless e2e test for Linux virtio-input under the in-kernel VMX VMM.
#
# What it validates:
# - boots a real Linux guest under RayOS VMM
# - instantiates a virtio-mmio device via Linux cmdline
# - guest reads one /dev/input/event0 event and prints RAYOS_LINUX_INPUT_EVENT_RX

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-linux-virtio-input-e2e.log}"
TIMEOUT_SECS="${TIMEOUT_SECS:-70}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-vmm-linux-virtio-input-e2e.sock}"

# Avoid mixing output from prior runs.
: > "$SERIAL_LOG"

# Prepare Alpine netboot kernel+initramfs (agent initrd).
ARTS_ENV=(
  WORK_DIR="$WORK_DIR"
  PREPARE_ONLY=1
  USE_AGENT_INITRD=1
)
ARTS_OUT="$(env "${ARTS_ENV[@]}" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py")"
KERNEL="$(printf "%s\n" "$ARTS_OUT" | sed -n 's/^KERNEL=//p' | head -n1)"
INITRD="$(printf "%s\n" "$ARTS_OUT" | sed -n 's/^INITRD=//p' | head -n1)"

if [ -z "$KERNEL" ] || [ -z "$INITRD" ]; then
  echo "ERROR: failed to prepare Linux artifacts" >&2
  echo "$ARTS_OUT" >&2
  exit 1
fi

# virtio-mmio base address in the Linux guest memory map:
# MMIO_VIRTIO_BASE = GUEST_RAM_SIZE_BYTES + 0x1000
# For vmm_linux_guest, guest RAM is 256MB => base 0x10000000 + 0x1000 = 0x10001000.
VIRTIO_MMIO_BASE_HEX="${VIRTIO_MMIO_BASE_HEX:-0x10001000}"
VIRTIO_MMIO_SIZE_HEX="${VIRTIO_MMIO_SIZE_HEX:-0x1000}"
# IRQ line is informational for Linux, but keep it in a PIC-compatible range.
VIRTIO_MMIO_IRQ_LINE="${VIRTIO_MMIO_IRQ_LINE:-5}"

CMDLINE_FILE="$WORK_DIR/vmm-linux-virtio-input-cmdline.txt"
cat >"$CMDLINE_FILE" <<EOF
console=ttyS0,115200n8 earlycon=uart,io,0x3f8,115200n8 rdinit=/rayos_init ignore_loglevel loglevel=7 panic=-1 virtio_mmio.device=${VIRTIO_MMIO_SIZE_HEX}@${VIRTIO_MMIO_BASE_HEX}:${VIRTIO_MMIO_IRQ_LINE} RAYOS_INPUT_PROBE=1
EOF

# Enable Linux guest under VMM + virtio-input device model.
export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_hypervisor_smoke,vmm_linux_guest,vmm_virtio_input}"

export RAYOS_LINUX_GUEST_KERNEL_SRC="$KERNEL"
export RAYOS_LINUX_GUEST_INITRD_SRC="$INITRD"
export RAYOS_LINUX_GUEST_CMDLINE_SRC="$CMDLINE_FILE"

export HEADLESS=1
export QEMU_TIMEOUT_SECS="$TIMEOUT_SECS"
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG
export MON_SOCK

# Prefer KVM when available.
if [ -z "${QEMU_EXTRA_ARGS:-}" ]; then
  if [ -e /dev/kvm ]; then
    export QEMU_EXTRA_ARGS="-enable-kvm -cpu host"
  else
    export QEMU_EXTRA_ARGS="-cpu qemu64,+vmx"
  fi
fi

export ENABLE_HOST_DESKTOP_BRIDGE=0
export PRELAUNCH_HIDDEN_DESKTOPS=0

"$ROOT_DIR/scripts/test-boot.sh" --headless &
BOOT_PID=$!

# Wait for monitor socket so we can quit early.
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

NORM="$WORK_DIR/serial-vmm-linux-virtio-input-e2e.norm.log"
MARKER="RAYOS_LINUX_INPUT_EVENT_RX"

quit_qemu() {
  local sock="$1"
  python3 - "$sock" <<'PY'
import socket, sys
path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(path)
s.sendall(b"quit\r\n")
s.close()
PY
}

DEADLINE=$(( $(date +%s) + TIMEOUT_SECS ))
while true; do
  tr -d '\r' < "$SERIAL_LOG" > "$NORM" 2>/dev/null || true

  if grep -F -a -q "$MARKER" "$NORM"; then
    quit_qemu "$MON_SOCK" || true
    break
  fi

  if grep -F -a -q "RAYOS_LINUX_INPUT_PROBE:SKIP no_event0" "$NORM"; then
    quit_qemu "$MON_SOCK" || true
    break
  fi

  if grep -F -a -q "RAYOS_VMM:VMX:UNSUPPORTED" "$NORM"; then
    quit_qemu "$MON_SOCK" || true
    break
  fi

  if [ "$(date +%s)" -ge "$DEADLINE" ]; then
    quit_qemu "$MON_SOCK" || true
    break
  fi
  sleep 0.2
done

wait "$BOOT_PID" 2>/dev/null || true
tr -d '\r' < "$SERIAL_LOG" > "$NORM" 2>/dev/null || true

if grep -F -a -q "$MARKER" "$NORM"; then
  echo "PASS: observed $MARKER" >&2
  exit 0
fi

if grep -F -a -q "RAYOS_LINUX_INPUT_PROBE:SKIP no_event0" "$NORM"; then
  echo "SKIP: Linux guest has no /dev/input/event0 (virtio-input driver not present/loaded)" >&2
  exit 0
fi

# If VMX isn't supported in this environment, we can't boot a guest; treat as skip.
if grep -F -a -q "RAYOS_VMM:VMX:UNSUPPORTED" "$NORM"; then
  echo "SKIP: VMX unsupported in this QEMU configuration" >&2
  exit 0
fi

echo "FAIL: did not observe $MARKER" >&2

tail -n 250 "$NORM" 2>/dev/null || true

exit 1
