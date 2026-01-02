#!/bin/bash
# Headless test for Linux desktop persistence across RayOS reboots.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$ROOT_DIR/build/test-linux-persistence"
mkdir -p "$WORK_DIR"

RAYOS_SERIAL_LOG="$WORK_DIR/rayos-serial.log"
RAYOS_MON_SOCK="$WORK_DIR/rayos-monitor.sock"
TEST_BOOT_PID_FILE="$WORK_DIR/test-boot.pid"

cleanup() {
    if [ -f "$TEST_BOOT_PID_FILE" ]; then
        pid=$(cat "$TEST_BOOT_PID_FILE")
        kill "$pid" 2>/dev/null || true
        rm -f "$TEST_BOOT_PID_FILE"
    fi
}
trap cleanup EXIT

echo "--- Starting first RayOS boot ---"
HEADLESS=1 \
SERIAL_LOG="$RAYOS_SERIAL_LOG" \
MON_SOCK="$RAYOS_MON_SOCK" \
WORK_DIR="$WORK_DIR" \
"$ROOT_DIR/scripts/test-boot.sh" > "$WORK_DIR/test-boot-1.log" 2>&1 &
echo "$!" > "$TEST_BOOT_PID_FILE"

echo "Waiting for hidden Linux VM to be ready..."
for _ in $(seq 1 60); do
    if grep -q "RAYOS_LINUX_DESKTOP_READY" "$WORK_DIR/linux-desktop-hidden-serial.log" 2>/dev/null; then
        echo "Hidden VM is ready."
        break
    fi
    sleep 1
done

# Give it a moment to settle
sleep 5

echo "--- Presenting desktop ---"
python3 "$ROOT_DIR/scripts/qemu-sendtext.py" --sock "$RAYOS_MON_SOCK" --text "show linux desktop"

echo "--- Creating marker file ---"
MARKER_TEXT="rayos-persistence-test-$(date +%s)"
MARKER_FILE="/home/rayos/persistence-marker.txt"
python3 "$ROOT_DIR/scripts/qemu-sendtext.py" --sock "$RAYOS_MON_SOCK" --text "type echo $MARKER_TEXT > $MARKER_FILE"

# Give it time to write the file
sleep 5

echo "--- Rebooting RayOS ---"
cleanup
sleep 5 # Make sure files are released

echo "--- Starting second RayOS boot ---"
HEADLESS=1 \
SERIAL_LOG="$RAYOS_SERIAL_LOG" \
MON_SOCK="$RAYOS_MON_SOCK" \
WORK_DIR="$WORK_DIR" \
"$ROOT_DIR/scripts/test-boot.sh" > "$WORK_DIR/test-boot-2.log" 2>&1 &
echo "$!" > "$TEST_BOOT_PID_FILE"

echo "Waiting for hidden Linux VM to be ready after reboot..."
for _ in $(seq 1 60); do
    if grep -q "RAYOS_LINUX_DESKTOP_READY" "$WORK_DIR/linux-desktop-hidden-serial.log" 2>/dev/null; then
        echo "Hidden VM is ready."
        break
    fi
    sleep 1
done

sleep 5

echo "--- Checking for marker file ---"
python3 "$ROOT_DIR/scripts/qemu-sendtext.py" --sock "$RAYOS_MON_SOCK" --text "type cat $MARKER_FILE"

sleep 5

if grep -q "$MARKER_TEXT" "$RAYOS_SERIAL_LOG"; then
    echo "--- Test successful ---"
    exit 0
else
    echo "--- Test failed: marker not found ---"
    exit 1
fi
