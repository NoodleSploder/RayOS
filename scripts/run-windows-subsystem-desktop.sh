#!/bin/bash
# Windows 11 desktop bring-up (host-launched QEMU window).
#
# Usage:
#   WINDOWS_DISK=/path/to/windows.qcow2 ./scripts/run-windows-subsystem-desktop.sh
#
# Notes:
# - Requires OVMF + swtpm (Windows 11).
# - Network defaults OFF; set WINDOWS_NET=1 to enable.
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

WINDOWS_DISK="${WINDOWS_DISK:-}"
if [ -z "$WINDOWS_DISK" ]; then
  echo "ERROR: WINDOWS_DISK is required (path to Windows disk image)." >&2
  exit 2
fi
if [ ! -f "$WINDOWS_DISK" ]; then
  echo "ERROR: WINDOWS_DISK not found: $WINDOWS_DISK" >&2
  exit 2
fi

QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"
if ! command -v "$QEMU_BIN" >/dev/null 2>&1; then
  echo "ERROR: qemu-system-x86_64 not found (QEMU_BIN=$QEMU_BIN)" >&2
  exit 2
fi

SWTPM_BIN="${SWTPM_BIN:-swtpm}"
if ! command -v "$SWTPM_BIN" >/dev/null 2>&1; then
  echo "ERROR: swtpm not found; required for Windows 11 vTPM." >&2
  exit 2
fi

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
if [ ! -f "$OVMF_CODE" ]; then
  # Fallback common path.
  OVMF_CODE="${OVMF_CODE_FALLBACK:-/usr/share/OVMF/OVMF_CODE.fd}"
fi
if [ ! -f "$OVMF_CODE" ]; then
  echo "ERROR: OVMF_CODE not found (tried /usr/share/OVMF/OVMF_CODE_4M.fd and /usr/share/OVMF/OVMF_CODE.fd)." >&2
  exit 2
fi

WINDOWS_ID="${WINDOWS_ID:-windows-desktop-001}"
WIN_DIR="$WORK_DIR/windows-guest/$WINDOWS_ID"
mkdir -p "$WIN_DIR"

OVMF_VARS="$WIN_DIR/OVMF_VARS.fd"
if [ ! -f "$OVMF_VARS" ]; then
  if [ -f /usr/share/OVMF/OVMF_VARS_4M.fd ]; then
    cp /usr/share/OVMF/OVMF_VARS_4M.fd "$OVMF_VARS"
  elif [ -f /usr/share/OVMF/OVMF_VARS.fd ]; then
    cp /usr/share/OVMF/OVMF_VARS.fd "$OVMF_VARS"
  else
    echo "ERROR: OVMF_VARS template not found under /usr/share/OVMF." >&2
    exit 2
  fi
fi

MON_SOCK="${WINDOWS_MONITOR_SOCK:-$WORK_DIR/windows-desktop-monitor.sock}"
PID_FILE="${WINDOWS_PID_FILE:-$WORK_DIR/.windows-desktop-qemu.pid}"
TPM_DIR="$WIN_DIR/tpm"
TPM_SOCK="$TPM_DIR/swtpm.sock"
rm -f "$MON_SOCK" "$TPM_SOCK" 2>/dev/null || true
mkdir -p "$TPM_DIR"

LOCK_FILE="$WORK_DIR/.windows-desktop.lock"
if command -v flock >/dev/null 2>&1; then
  exec 9>"$LOCK_FILE"
  if ! flock -n 9; then
    echo "WARN: Windows desktop launcher already running (lock: $LOCK_FILE)" >&2
    exit 0
  fi
else
  LOCK_DIR="$WORK_DIR/.windows-desktop.lockdir"
  if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    echo "WARN: Windows desktop launcher already running (lock: $LOCK_DIR)" >&2
    exit 0
  fi
  trap 'rmdir "$LOCK_DIR" 2>/dev/null || true' EXIT
fi

if [ -f "$PID_FILE" ]; then
  old_pid="$(cat "$PID_FILE" 2>/dev/null || true)"
  if [ -n "$old_pid" ] && kill -0 "$old_pid" 2>/dev/null; then
    echo "WARN: Windows desktop already running (pid=$old_pid)" >&2
    exit 0
  fi
fi

WINDOWS_NET="${WINDOWS_NET:-0}"
NET_ARGS=()
if [ "$WINDOWS_NET" != "0" ]; then
  NET_ARGS=(-netdev user,id=net0 -device virtio-net-pci,netdev=net0)
fi

echo "Starting swtpm..." >&2
"$SWTPM_BIN" socket \
  --tpm2 \
  --tpmstate dir="$TPM_DIR" \
  --ctrl type=unixio,path="$TPM_SOCK" \
  --log file="$WIN_DIR/swtpm.log" \
  --daemon

echo "Launching Windows desktop..." >&2
echo "Monitor: $MON_SOCK" >&2
echo "Disk: $WINDOWS_DISK" >&2

exec "$QEMU_BIN" \
  -machine q35,accel=kvm:tcg \
  -cpu host \
  -m "${WINDOWS_MEM:-8192}" \
  -smp "${WINDOWS_SMP:-4}" \
  -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
  -drive if=pflash,format=raw,file="$OVMF_VARS" \
  -chardev socket,id=chrtpm,path="$TPM_SOCK" \
  -tpmdev emulator,id=tpm0,chardev=chrtpm \
  -device tpm-tis,tpmdev=tpm0 \
  -drive file="$WINDOWS_DISK",if=virtio,format=qcow2 \
  -device qemu-xhci \
  -device usb-kbd \
  -device usb-tablet \
  -vga virtio \
  -display gtk,zoom-to-fit=on \
  -serial "file:$WIN_DIR/windows-serial.log" \
  -monitor "unix:$MON_SOCK,server,nowait" \
  -no-reboot \
  "${NET_ARGS[@]}"

