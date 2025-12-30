#!/bin/bash
# Test boot script for RayOS

# NOTE:
# - This script boots RayOS *without* the host-side AI bridge.
# - By default, kernel-bare replies using the in-guest local AI responder.
#
# To boot with host-side replies (ai_bridge), run:
#   ./scripts/test-boot-ai.sh
#
# Or set:
#   BOOT_WITH_AI=1 ./test-boot.sh

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ "${BOOT_WITH_AI:-0}" = "1" ]; then
    exec "$ROOT_DIR/scripts/test-boot-ai.sh"
fi

# Optional Phase 2 bring-up gate: validate aarch64 post-ExitBootServices embedded loop.
# Opt-in because the main flow here is a blocking graphical x86_64 QEMU session.
if [ "${RUN_AARCH64_HEADLESS:-0}" = "1" ]; then
    echo "Running aarch64 headless bring-up test..."
    "$ROOT_DIR/scripts/test-boot-aarch64-headless.sh"
fi

# Optional: validate aarch64 embedded-mode + host AI bridge protocol.
if [ "${RUN_AARCH64_AI_HEADLESS:-0}" = "1" ]; then
    echo "Running aarch64 AI headless smoke test..."
    "$ROOT_DIR/scripts/test-boot-aarch64-ai-headless.sh"
fi

# Optional: validate aarch64 embedded-mode Volume staging + query.
if [ "${RUN_AARCH64_VOLUME_HEADLESS:-0}" = "1" ]; then
    echo "Running aarch64 Volume headless smoke test..."
    "$ROOT_DIR/scripts/test-boot-aarch64-volume-headless.sh"
fi

# Optional: validate aarch64 Option B kernel boot (loads EFI/RAYOS/kernel.bin).
if [ "${RUN_AARCH64_KERNEL_HEADLESS:-0}" = "1" ]; then
    echo "Running aarch64 kernel headless smoke test..."
    "$ROOT_DIR/scripts/test-boot-aarch64-kernel-headless.sh"
fi

# Optional: validate aarch64 Option B kernel + host AI bridge protocol.
if [ "${RUN_AARCH64_KERNEL_AI_HEADLESS:-0}" = "1" ]; then
    echo "Running aarch64 kernel AI headless smoke test..."
    "$ROOT_DIR/scripts/test-boot-aarch64-kernel-ai-headless.sh"
fi

# Optional: validate aarch64 Option B kernel can query staged Volume.
if [ "${RUN_AARCH64_KERNEL_VOLUME_HEADLESS:-0}" = "1" ]; then
    echo "Running aarch64 kernel Volume headless smoke test..."
    "$ROOT_DIR/scripts/test-boot-aarch64-kernel-volume-headless.sh"
fi

cd "$ROOT_DIR"

# Optionally build the kernel before staging so we always boot the latest code.
# Set BUILD_KERNEL=0 to skip.
BUILD_KERNEL="${BUILD_KERNEL:-1}"
if [ "$BUILD_KERNEL" != "0" ]; then
    echo "Building kernel-bare (release)..."
    pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
    # Force Cargo to use rustup's toolchain rustc so build-std doesn't accidentally
    # pick up a system rustc/sysroot.
    RUSTC="$(rustup which rustc)" cargo build \
        -Z build-std=core,alloc \
        -Z build-std-features=compiler-builtins-mem \
        --release \
        --target x86_64-unknown-none
    popd >/dev/null
fi

# Optionally build the UEFI bootloader so BootInfo ABI stays in sync.
BUILD_BOOTLOADER="${BUILD_BOOTLOADER:-1}"
if [ "$BUILD_BOOTLOADER" != "0" ]; then
    echo "Building rayos-bootloader (release)..."
    pushd "$ROOT_DIR/crates/bootloader" >/dev/null
    RUSTC="$(rustup which rustc)" cargo build \
        --release \
        --target x86_64-unknown-uefi \
        -p rayos-bootloader
    popd >/dev/null
fi

WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

SERIAL_LOG="${SERIAL_LOG:-$WORK_DIR/serial-boot-graphical.log}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-graphical.sock}"
rm -f "$MON_SOCK" 2>/dev/null || true

# Optional: host-side bridge for "show linux desktop" typed inside RayOS.
# We watch the serial log for a marker emitted by the kernel and then launch
# the Linux guest desktop (separate QEMU window for now).
ENABLE_LINUX_DESKTOP_BRIDGE="${ENABLE_LINUX_DESKTOP_BRIDGE:-1}"
DESKTOP_BRIDGE_PID=""
DESKTOP_PID_FILE="$WORK_DIR/.linux-desktop-qemu.pid"

cleanup_bridge() {
    if [ -n "$DESKTOP_BRIDGE_PID" ]; then
        kill "$DESKTOP_BRIDGE_PID" 2>/dev/null || true
    fi

    if [ -f "$DESKTOP_PID_FILE" ]; then
        DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
        if [ -n "$DESKTOP_QEMU_PID" ] && kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
            echo "[host] Stopping Linux desktop (pid=$DESKTOP_QEMU_PID)" >&2
            kill "$DESKTOP_QEMU_PID" 2>/dev/null || true
        fi
        rm -f "$DESKTOP_PID_FILE" 2>/dev/null || true
    fi
}

if [ "$ENABLE_LINUX_DESKTOP_BRIDGE" != "0" ]; then
    (
        # Wait for the serial log to exist, then tail it.
        while [ ! -f "$SERIAL_LOG" ]; do
            sleep 0.05
        done

        # Prevent repeated/overlapping launches when RayOS emits the event multiple times
        # (or emits it quickly enough to race PID-file creation).
        DESKTOP_LAUNCH_LOCK_DIR="$WORK_DIR/.linux-desktop-launch.lock"
        DESKTOP_LAST_LAUNCH_TS_FILE="$WORK_DIR/.linux-desktop-last-launch.ts"

        tail -n0 -F "$SERIAL_LOG" 2>/dev/null | while IFS= read -r line; do
            case "$line" in
                *RAYOS_HOST_EVENT:SHOW_LINUX_DESKTOP*)
                    # Debounce bursts of identical events.
                    now_ts="$(date +%s 2>/dev/null || echo 0)"
                    last_ts="$(cat "$DESKTOP_LAST_LAUNCH_TS_FILE" 2>/dev/null || echo 0)"
                    if [ "$now_ts" -ne 0 ] && [ "$last_ts" -ne 0 ] && [ $((now_ts - last_ts)) -lt 2 ]; then
                        continue
                    fi

                    # Serialize launch attempts so we don't start multiple QEMUs concurrently.
                    if ! mkdir "$DESKTOP_LAUNCH_LOCK_DIR" 2>/dev/null; then
                        # Someone else is launching (or already launched) right now.
                        continue
                    fi

                    # If a desktop QEMU is already running, do not relaunch.
                    if [ -f "$DESKTOP_PID_FILE" ]; then
                        DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
                        if [ -n "$DESKTOP_QEMU_PID" ] && kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
                            echo "[host] Linux desktop already running (pid=$DESKTOP_QEMU_PID)" >&2
                            rmdir "$DESKTOP_LAUNCH_LOCK_DIR" 2>/dev/null || true
                            continue
                        fi
                    fi

                    printf '%s\n' "$now_ts" >"$DESKTOP_LAST_LAUNCH_TS_FILE" 2>/dev/null || true

                    echo "[host] RayOS requested Linux desktop; launching..." >&2
                    rm -f "$DESKTOP_PID_FILE" 2>/dev/null || true

                    DESKTOP_LAUNCH_LOG="$WORK_DIR/linux-desktop-launch.log"

                    # Launch the Linux desktop guest in a separate QEMU window.
                    # It uses a persistent ext4 disk under WORK_DIR for speed across runs.
                    WORK_DIR="$WORK_DIR" \
                    LINUX_DESKTOP_DISK="$WORK_DIR/linux-guest/desktop/desktop-rootfs.ext4" \
                    "$ROOT_DIR/scripts/run-linux-subsystem-desktop-auto.sh" \
                        >"$DESKTOP_LAUNCH_LOG" 2>&1 &

                    echo "$!" >"$DESKTOP_PID_FILE" 2>/dev/null || true
                    rmdir "$DESKTOP_LAUNCH_LOCK_DIR" 2>/dev/null || true
                    ;;

                *RAYOS_HOST_EVENT:LINUX_SENDTEXT:*)
                    # Inject a line of text into the Linux desktop VM via its HMP monitor socket.
                    # The kernel emits: RAYOS_HOST_EVENT:LINUX_SENDTEXT:<text>
                    # Note: qemu-sendtext has a conservative ASCII mapping.

                    # If the desktop isn't running, ignore.
                    if [ ! -f "$DESKTOP_PID_FILE" ]; then
                        continue
                    fi
                    DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
                    if [ -z "$DESKTOP_QEMU_PID" ] || ! kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
                        continue
                    fi

                    DESKTOP_MON_SOCK="$WORK_DIR/linux-desktop-monitor.sock"
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        continue
                    fi

                    text="${line#*RAYOS_HOST_EVENT:LINUX_SENDTEXT:}"
                    # Strip CR if present.
                    text="${text%$'\r'}"
                    if [ -z "$text" ]; then
                        continue
                    fi

                    python3 "$ROOT_DIR/scripts/qemu-sendtext.py" \
                        --sock "$DESKTOP_MON_SOCK" \
                        --text "$text" \
                        --wait 1.5 \
                        >/dev/null 2>&1 || true
                    ;;

                *RAYOS_HOST_EVENT:LINUX_SHUTDOWN*)
                    # Ask the Linux desktop VM to shut down.
                    # Best-effort graceful: ACPI power button, then quit if still running.
                    if [ ! -f "$DESKTOP_PID_FILE" ]; then
                        continue
                    fi
                    DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
                    if [ -z "$DESKTOP_QEMU_PID" ] || ! kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
                        continue
                    fi

                    DESKTOP_MON_SOCK="$WORK_DIR/linux-desktop-monitor.sock"
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        continue
                    fi

                    # Send ACPI powerdown.
                    python3 - <<'PY' "$DESKTOP_MON_SOCK" >/dev/null 2>&1 || true
import socket, sys, time
sock_path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1.0)
s.connect(sock_path)
try:
    s.settimeout(0.2)
    s.recv(4096)
except Exception:
    pass
s.sendall(b"system_powerdown\r\n")
time.sleep(0.05)
try:
    s.settimeout(0.1)
    s.recv(4096)
except Exception:
    pass
s.close()
PY

                    # Give the guest a moment to exit.
                    sleep 1.0 2>/dev/null || true
                    if kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
                        # Fallback: force quit QEMU.
                        python3 - <<'PY' "$DESKTOP_MON_SOCK" >/dev/null 2>&1 || true
import socket, sys, time
sock_path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1.0)
s.connect(sock_path)
try:
    s.settimeout(0.2)
    s.recv(4096)
except Exception:
    pass
s.sendall(b"quit\r\n")
time.sleep(0.05)
s.close()
PY
                    fi
                    ;;

                *RAYOS_HOST_EVENT:LINUX_SENDKEY:*)
                    # Inject a single key (or key combo) into the Linux desktop VM via HMP sendkey.
                    # The kernel emits: RAYOS_HOST_EVENT:LINUX_SENDKEY:<spec>
                    if [ ! -f "$DESKTOP_PID_FILE" ]; then
                        continue
                    fi
                    DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
                    if [ -z "$DESKTOP_QEMU_PID" ] || ! kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
                        continue
                    fi

                    DESKTOP_MON_SOCK="$WORK_DIR/linux-desktop-monitor.sock"
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        continue
                    fi

                    key="${line#*RAYOS_HOST_EVENT:LINUX_SENDKEY:}"
                    key="${key%$'\r'}"
                    if [ -z "$key" ]; then
                        continue
                    fi

                    python3 "$ROOT_DIR/scripts/qemu-sendkey.py" \
                        --sock "$DESKTOP_MON_SOCK" \
                        --key "$key" \
                        --wait 1.5 \
                        >/dev/null 2>&1 || true
                    ;;
            esac
        done
    ) &
    DESKTOP_BRIDGE_PID="$!"
    trap cleanup_bridge EXIT
fi

# Stage a FAT drive (bootloader + kernel) so we boot the latest artifacts
# without requiring an ISO/USB image rebuild.
STAGE_DIR="${STAGE_DIR:-$WORK_DIR/boot-fat}"
BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi}"
KERNEL_BIN_SRC="${KERNEL_BIN_SRC:-$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare}"
MODEL_BIN_SRC="${MODEL_BIN_SRC:-}"

# Local LLM model staging:
# - If MODEL_BIN_SRC is set, that file is copied into EFI/RAYOS/model.bin
# - Otherwise, AUTO_GENERATE_MODEL_BIN=1 will generate a small model into $WORK_DIR/model.bin
#   from repo docs (default corpus: README.md QUICKSTART.md) and stage it.
AUTO_GENERATE_MODEL_BIN="${AUTO_GENERATE_MODEL_BIN:-1}"
MODEL_OUT_DEFAULT="$WORK_DIR/model.bin"
MODEL_CORPUS_FILES_DEFAULT=("$ROOT_DIR/docs/README.md" "$ROOT_DIR/docs/QUICKSTART.md")

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"
if [ -f "$BOOT_EFI_SRC" ]; then
    cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
fi
if [ -f "$KERNEL_BIN_SRC" ]; then
    cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"
fi

if [ -z "${MODEL_BIN_SRC}" ] && [ "$AUTO_GENERATE_MODEL_BIN" != "0" ]; then
    MODEL_BIN_SRC="$MODEL_OUT_DEFAULT"
    if [ ! -f "$MODEL_BIN_SRC" ]; then
        if command -v python3 >/dev/null 2>&1; then
            echo "Generating local LLM model: $MODEL_BIN_SRC"
            python3 "$ROOT_DIR/scripts/tools/gen_raygpt_model.py" \
                   -o "$MODEL_BIN_SRC" --quiet --steps 2500 --top-k 1 \
                "${MODEL_CORPUS_FILES_DEFAULT[@]}" \
                >/dev/null 2>&1 || true
        fi
    fi
fi

if [ -n "${MODEL_BIN_SRC}" ]; then
    if [ -f "$MODEL_BIN_SRC" ]; then
        cp "$MODEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/model.bin"
    else
        echo "Warning: MODEL_BIN_SRC set but not found: $MODEL_BIN_SRC"
    fi
fi

echo "╔═══════════════════════════════════════════════════╗"
echo "║        RayOS Boot Test (Graphical Mode)         ║"
echo "╚═══════════════════════════════════════════════════╝"
echo ""
echo "Testing: build/rayos-universal-usb.img"
echo ""
echo "Controls:"
echo "  - Ctrl+Alt+G: Release mouse from QEMU window"
echo "  - Ctrl+Alt+Q: Quit QEMU"
echo "  - Ctrl+C: Stop in terminal"
echo ""
echo "Starting QEMU..."
echo ""

echo "Hint: local AI replies are built-in. For host AI bridge replies, use ./scripts/test-boot-ai.sh (or BOOT_WITH_AI=1)."
echo "Hint: local LLM model is staged by default. Disable with AUTO_GENERATE_MODEL_BIN=0."
echo ""

echo "Serial log: $SERIAL_LOG"
echo "Monitor sock: $MON_SOCK"
echo "FAT stage: $STAGE_DIR"
echo ""

# Use USB image for better compatibility
qemu-system-x86_64 \
    -machine q35 \
    -m 2048 \
    -rtc base=utc,clock=host \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive file="fat:rw:$STAGE_DIR",format=raw \
    -serial "file:$SERIAL_LOG" \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -vga std \
    -display gtk,zoom-to-fit=on

echo ""
echo "QEMU exited."
