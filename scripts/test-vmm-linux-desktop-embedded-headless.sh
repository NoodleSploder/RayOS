#!/bin/bash
# Headless smoke test: run the Linux guest "embedded" under the in-kernel VMX VMM.
#
# This boots RayOS with:
# - vmm_hypervisor + vmm_linux_guest
# - vmm_linux_desktop_autostart (starts guest hidden and runs it via time-slices)
#
# The test asserts the Linux guest emits a readiness marker over COM1.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-linux-desktop-embedded.log}"
TIMEOUT_SECS="${TIMEOUT_SECS:-75}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-vmm-linux-desktop-embedded.sock}"

: > "$SERIAL_LOG"

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

CMDLINE_FILE="$WORK_DIR/vmm-linux-desktop-embedded-cmdline.txt"
cat >"$CMDLINE_FILE" <<'EOF'
console=ttyS0,115200n8 earlycon=uart,io,0x3f8,115200n8 rdinit=/rayos_init ignore_loglevel loglevel=7 panic=-1
EOF

export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_linux_guest,vmm_linux_desktop_autostart}"

export RAYOS_LINUX_GUEST_KERNEL_SRC="$KERNEL"
export RAYOS_LINUX_GUEST_INITRD_SRC="$INITRD"
export RAYOS_LINUX_GUEST_CMDLINE_SRC="$CMDLINE_FILE"

export HEADLESS=1
export QEMU_TIMEOUT_SECS="$TIMEOUT_SECS"
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG
export MON_SOCK

# Prefer KVM when available; otherwise request a VMX-capable CPU model.
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

NORM="$WORK_DIR/serial-vmm-linux-desktop-embedded.norm.log"
PRIMARY_MARKER="RAYOS_LINUX_AGENT_READY"
FALLBACK_MARKER="RAYOS_LINUX_GUEST_READY"

source "$ROOT_DIR/scripts/lib/headless_qemu.sh"

DEADLINE=$(( $(date +%s) + TIMEOUT_SECS ))
while true; do
  tr -d '\r' < "$SERIAL_LOG" > "$NORM" 2>/dev/null || true

  if grep -F -a -q "RAYOS_VMM:VMX:UNSUPPORTED" "$NORM"; then
    quit_qemu "$MON_SOCK" || true
    break
  fi

  if grep -F -a -q "$PRIMARY_MARKER" "$NORM" || grep -F -a -q "$FALLBACK_MARKER" "$NORM"; then
    quit_qemu "$MON_SOCK" || true
    break
  fi

  if [ "$(date +%s)" -ge "$DEADLINE" ]; then
    quit_qemu "$MON_SOCK" || true
    break
  fi
  sleep 0.2
done

WAIT_DEADLINE=$(( $(date +%s) + 10 ))
while kill -0 "$BOOT_PID" 2>/dev/null; do
  if [ "$(date +%s)" -ge "$WAIT_DEADLINE" ]; then
    echo "WARN: QEMU did not exit after quit; killing test-boot" >&2
    kill "$BOOT_PID" 2>/dev/null || true
    break
  fi
  sleep 0.1
done

wait "$BOOT_PID" 2>/dev/null || true
tr -d '\r' < "$SERIAL_LOG" > "$NORM" 2>/dev/null || true

if grep -F -a -q "$PRIMARY_MARKER" "$NORM"; then
  echo "PASS: observed $PRIMARY_MARKER" >&2
  exit 0
fi

if grep -F -a -q "$FALLBACK_MARKER" "$NORM"; then
  echo "PASS: observed $FALLBACK_MARKER" >&2
  exit 0
fi

if grep -F -a -q "RAYOS_VMM:VMX:UNSUPPORTED" "$NORM"; then
  echo "SKIP: VMX unsupported in this QEMU configuration" >&2
  exit 0
fi

echo "FAIL: did not observe $PRIMARY_MARKER or $FALLBACK_MARKER" >&2

tail -n 250 "$NORM" 2>/dev/null || true

exit 1
