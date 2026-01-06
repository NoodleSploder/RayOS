#!/bin/bash
# Headless smoke test (Hypervisor P3): Linux guest disk marker persists across RayOS reboot.
#
# Flow:
# - Boot RayOS with in-kernel VMM Linux guest (rdinit=/rayos_init)
# - Guest writes a raw-disk marker (gated by RAYOS_PERSIST_TEST=1 on cmdline)
# - Host issues QEMU `system_reset` (simulates a RayOS reboot)
# - On next boot, guest observes marker already present

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-vmm-linux-persist-marker.log}"
SERIAL_NORM="$WORK_DIR/serial-vmm-linux-persist-marker.norm.log"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-vmm-linux-persist-marker.sock}"
TIMEOUT_SECS="${TIMEOUT_SECS:-180}"

# Avoid mixing output from prior runs.
: > "$SERIAL_LOG"

export HEADLESS=1
export PRESERVE_SERIAL_LOG=0
export SERIAL_LOG
export MON_SOCK

# Avoid host-side desktop bridges for this test.
export ENABLE_HOST_DESKTOP_BRIDGE=0
export PRELAUNCH_HIDDEN_DESKTOPS=0

# Ensure we have a kernel/initrd pair. We reuse the existing Linux subsystem tooling
# to download/cached Alpine netboot artifacts and build the agent initrd.
ARTS_ENV=(
  WORK_DIR="$WORK_DIR"
  PREPARE_ONLY=1
  USE_AGENT_INITRD=1
  RAYOS_AGENT_ENABLE_PERSIST_TEST=1
)
ARTS_OUT="$(env "${ARTS_ENV[@]}" python3 "$ROOT_DIR/scripts/tools/linux_subsystem/run_linux_guest.py")"
KERNEL="$(printf "%s\n" "$ARTS_OUT" | sed -n 's/^KERNEL=//p' | head -n1)"
INITRD="$(printf "%s\n" "$ARTS_OUT" | sed -n 's/^INITRD=//p' | head -n1)"

if [ -z "$KERNEL" ] || [ -z "$INITRD" ]; then
  echo "ERROR: failed to prepare Linux artifacts" >&2
  echo "$ARTS_OUT" >&2
  exit 1
fi

CMDLINE_FILE="$WORK_DIR/vmm-linux-persist-marker-cmdline.txt"
cat >"$CMDLINE_FILE" <<'EOF'
console=ttyS0,115200n8 earlycon=uart,io,0x3f8,115200n8 rdinit=/rayos_init ignore_loglevel loglevel=7 panic=-1 virtio_mmio.device=4K@0x10001000:5
EOF

export RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-vmm_hypervisor,vmm_hypervisor_smoke,vmm_linux_guest}"

# Stage Linux artifacts into the RayOS boot FAT.
export RAYOS_LINUX_GUEST_KERNEL_SRC="$KERNEL"
export RAYOS_LINUX_GUEST_INITRD_SRC="$INITRD"
export RAYOS_LINUX_GUEST_CMDLINE_SRC="$CMDLINE_FILE"

# VMX guest execution requires a VMX-capable CPU model in QEMU.
# Prefer KVM when available.
if [ -z "${QEMU_EXTRA_ARGS:-}" ]; then
  if [ -e /dev/kvm ]; then
    export QEMU_EXTRA_ARGS="-enable-kvm -cpu host"
  else
    export QEMU_EXTRA_ARGS="-cpu qemu64,+vmx"
  fi
fi

echo "Running Linux disk persistence marker smoke test..." >&2
echo "  RAYOS_KERNEL_FEATURES=$RAYOS_KERNEL_FEATURES" >&2
echo "  SERIAL_LOG=$SERIAL_LOG" >&2
echo "  MON_SOCK=$MON_SOCK" >&2
echo "  TIMEOUT_SECS=$TIMEOUT_SECS" >&2

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

# Start QEMU/RayOS.
QEMU_TIMEOUT_SECS="" "$ROOT_DIR/scripts/test-boot.sh" --headless &
BOOT_PID=$!

# Wait for monitor socket.
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

NEED_UNSUPPORTED="RAYOS_VMM:VMX:UNSUPPORTED"
NEED_WROTE="RAYOS_LINUX_DISK_MARKER_WROTE"
NEED_RESET="RAYOS_LINUX_DISK_MARKER_NEEDS_REBOOT"
NEED_OK="RAYOS_LINUX_DISK_MARKER_PRESENT"
NEED_NO_DEV="RAYOS_LINUX_DISK_MARKER_NO_DEV"

DEADLINE=$(( $(date +%s) + TIMEOUT_SECS ))
DID_RESET=0

while true; do
  tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true

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

  if grep -F -a -q "$NEED_NO_DEV" "$SERIAL_NORM"; then
    echo "FAIL: Linux persist test could not find /dev/vda ($NEED_NO_DEV)" >&2
    tail -n 250 "$SERIAL_NORM" >&2 || true
    quit_qemu "$MON_SOCK" || true
    break
  fi

  if [ "$DID_RESET" = "0" ] && grep -F -a -q "$NEED_RESET" "$SERIAL_NORM"; then
    if ! grep -F -a -q "$NEED_WROTE" "$SERIAL_NORM"; then
      echo "WARN: saw reset marker without wrote marker; continuing" >&2
    fi
    echo "INFO: guest requested reboot; issuing system_reset" >&2
    system_reset_qemu "$MON_SOCK" || true
    DID_RESET=1
    # Treat TIMEOUT_SECS as a per-phase budget so slower hosts don't
    # accidentally spend the whole timeout budget before the post-reset boot.
    DEADLINE=$(( $(date +%s) + TIMEOUT_SECS ))
    sleep 1
  fi

  if [ "$(date +%s)" -ge "$DEADLINE" ]; then
    echo "FAIL: timed out waiting for Linux disk persistence markers" >&2
    tail -n 250 "$SERIAL_NORM" >&2 || true
    quit_qemu "$MON_SOCK" || true
    break
  fi

  sleep 0.2
done

wait "$BOOT_PID" 2>/dev/null || true

tr -d '\r' < "$SERIAL_LOG" > "$SERIAL_NORM" 2>/dev/null || true
if grep -F -a -q "$NEED_OK" "$SERIAL_NORM"; then
  exit 0
fi
exit 1
