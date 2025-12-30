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

# Set HEADLESS=1 to run QEMU without a GUI window (useful for CI/smoke tests).
HEADLESS="${HEADLESS:-0}"

# Optional: host-side bridge for "show linux desktop" typed inside RayOS.
# We watch the serial log for a marker emitted by the kernel and then launch
# the Linux guest desktop (separate QEMU window for now).
ENABLE_LINUX_DESKTOP_BRIDGE="${ENABLE_LINUX_DESKTOP_BRIDGE:-1}"
DESKTOP_BRIDGE_PID=""
DESKTOP_PID_FILE="$WORK_DIR/.linux-desktop-qemu.pid"
DESKTOP_LAUNCH_LOG="$WORK_DIR/linux-desktop-launch.log"
DESKTOP_MON_SOCK="$WORK_DIR/linux-desktop-monitor.sock"
DESKTOP_LAUNCH_INFLIGHT="$WORK_DIR/.linux-desktop-launch.inflight"
DESKTOP_RUN_GUARD="$WORK_DIR/.linux-desktop-running.guard"
DESKTOP_STATE_FILE="$WORK_DIR/.linux-desktop.state"
DESKTOP_ONCE_GUARD="$WORK_DIR/.linux-desktop-once.guard"
DESKTOP_BRIDGE_LOCK_FILE="$WORK_DIR/.linux-desktop-bridge.lock"
DESKTOP_BRIDGE_LOCK_DIR="$WORK_DIR/.linux-desktop-bridge.lockdir"
DESKTOP_BRIDGE_LOCK_FD=""

validate_ascii_payload() {
    # Ensure payload is printable ASCII (space..~) and within a sane length.
    # Returns 0 if valid, 1 otherwise.
    local payload="$1"
    local max_len="${2:-512}"
    if [ "${#payload}" -eq 0 ] || [ "${#payload}" -gt "$max_len" ]; then
        return 1
    fi
    # Reject non-printable bytes.
    LC_ALL=C
    if printf '%s' "$payload" | tr -d '\040-\176' | grep -q .; then
        return 1
    fi
    return 0
}

validate_float_01() {
    # Validate a float in the range [0,1]. Accepts "0", "1", "0.5", ".5".
    # Returns 0 if valid, 1 otherwise.
    local v="$1"
    if ! printf '%s' "$v" | grep -E '^(0(\.[0-9]+)?|1(\.0*)?|\.?[0-9]+)$' >/dev/null 2>&1; then
        return 1
    fi
    awk "BEGIN { exit !($v >= 0 && $v <= 1) }" >/dev/null 2>&1 || return 1
    return 0
}

emit_host_ack() {
    # Append to the serial log so the marker is visible in the same stream as the
    # guest output. Note: this is host-side only; it does not round-trip into the guest.
    local op="$1"
    local status="$2"
    local detail="$3"
    if [ -n "$SERIAL_LOG" ]; then
        printf "RAYOS_HOST_ACK:%s:%s:%s\n" "$op" "$status" "$detail" >>"$SERIAL_LOG"
    fi
}

desktop_running() {
    # Returns 0 if the desktop VM is running and sets DESKTOP_QEMU_PID.
    # Returns 1 otherwise.
    if [ -f "$DESKTOP_LAUNCH_INFLIGHT" ] || [ -f "$DESKTOP_RUN_GUARD" ] || [ -f "$DESKTOP_STATE_FILE" ]; then
        # A launch is in progress or running state is asserted; treat as running to avoid duplicate starts.
        return 0
    fi
    if [ ! -f "$DESKTOP_PID_FILE" ]; then
        # Fallback: try to detect a live monitor socket even without a PID file.
        if [ -S "$DESKTOP_MON_SOCK" ]; then
            if python3 - "$DESKTOP_MON_SOCK" <<'PY' >/dev/null 2>&1
import socket, sys
sock_path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(0.5)
s.connect(sock_path)
try:
    s.settimeout(0.2)
    s.recv(4096)
except Exception:
    pass
try:
    s.sendall(b"info status\r\n")
except Exception:
    s.close()
    sys.exit(1)
s.close()
sys.exit(0)
PY
            then
                DESKTOP_QEMU_PID=""
                return 0
            fi
        fi
        return 1
    fi
    DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
    if [ -z "$DESKTOP_QEMU_PID" ] || ! kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
        return 1
    fi
    return 0
}

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

    rm -f "$DESKTOP_LAUNCH_INFLIGHT" "$DESKTOP_RUN_GUARD" "$DESKTOP_STATE_FILE" "$DESKTOP_ONCE_GUARD" 2>/dev/null || true
    if [ -n "$DESKTOP_LAUNCH_LOCK_DIR" ]; then
        rmdir "$DESKTOP_LAUNCH_LOCK_DIR" 2>/dev/null || true
    fi

    if [ -n "$DESKTOP_BRIDGE_LOCK_FD" ]; then
        eval "exec ${DESKTOP_BRIDGE_LOCK_FD}>&-" 2>/dev/null || true
    fi
    rmdir "$DESKTOP_BRIDGE_LOCK_DIR" 2>/dev/null || true
}

if [ "$ENABLE_LINUX_DESKTOP_BRIDGE" != "0" ]; then
    # Ensure only one bridge instance per WORK_DIR, even if multiple test-boot.sh
    # processes are running. Without this, multiple tailers can react to a single
    # SHOW_LINUX_DESKTOP event and launch multiple desktop VMs.
    if command -v flock >/dev/null 2>&1; then
        DESKTOP_BRIDGE_LOCK_FD="9"
        eval "exec ${DESKTOP_BRIDGE_LOCK_FD}>\"$DESKTOP_BRIDGE_LOCK_FILE\""
        if ! flock -n "$DESKTOP_BRIDGE_LOCK_FD"; then
            echo "[host] Linux desktop bridge already active for WORK_DIR=$WORK_DIR; disabling duplicate bridge in this session." >&2
            ENABLE_LINUX_DESKTOP_BRIDGE=0
        fi
    else
        if ! mkdir "$DESKTOP_BRIDGE_LOCK_DIR" 2>/dev/null; then
            echo "[host] Linux desktop bridge already active for WORK_DIR=$WORK_DIR; disabling duplicate bridge in this session." >&2
            ENABLE_LINUX_DESKTOP_BRIDGE=0
        fi
    fi
fi

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

        # Robust tail loop: use process substitution (blocking FD) and restart on transient errors.
        while true; do
            while IFS= read -r line; do
                norm_line="$line"
                case "$line" in
                    *RAYOS_HOST_EVENT_V0:*)
                        norm_line="RAYOS_HOST_EVENT:${line#*RAYOS_HOST_EVENT_V0:}"
                        ;;
                esac

                case "$norm_line" in
                *RAYOS_HOST_EVENT:SHOW_LINUX_DESKTOP*)
                    # If we've already launched once this host session, ignore further show requests.
                    if [ -f "$DESKTOP_ONCE_GUARD" ]; then
                        emit_host_ack "SHOW_LINUX_DESKTOP" "err" "already_launched"
                        continue
                    fi

                    # Debounce bursts of identical events.
                    now_ts="$(date +%s 2>/dev/null || echo 0)"
                    last_ts="$(cat "$DESKTOP_LAST_LAUNCH_TS_FILE" 2>/dev/null || echo 0)"
                    if [ "$now_ts" -ne 0 ] && [ "$last_ts" -ne 0 ] && [ $((now_ts - last_ts)) -lt 2 ]; then
                        emit_host_ack "SHOW_LINUX_DESKTOP" "err" "debounced"
                        continue
                    fi

                    # If a launch is already in-flight (pid not yet recorded), skip.
                    if [ -f "$DESKTOP_LAUNCH_INFLIGHT" ]; then
                        emit_host_ack "SHOW_LINUX_DESKTOP" "err" "launch_in_progress"
                        continue
                    fi

                    # Serialize launch attempts so we don't start multiple QEMUs concurrently.
                    if ! mkdir "$DESKTOP_LAUNCH_LOCK_DIR" 2>/dev/null; then
                        # Someone else is launching (or already launched) right now.
                        emit_host_ack "SHOW_LINUX_DESKTOP" "err" "launch_in_progress"
                        continue
                    fi

                    # If a desktop QEMU is already running, do not relaunch.
                    if desktop_running; then
                        echo "[host] Linux desktop already running (pid=$DESKTOP_QEMU_PID)" >&2
                        emit_host_ack "SHOW_LINUX_DESKTOP" "ok" "already_running"
                        rmdir "$DESKTOP_LAUNCH_LOCK_DIR" 2>/dev/null || true
                        continue
                    fi

                    printf '%s\n' "$now_ts" >"$DESKTOP_LAST_LAUNCH_TS_FILE" 2>/dev/null || true
                    date +%s >"$DESKTOP_ONCE_GUARD" 2>/dev/null || true

                    log_ts="$(date '+%Y-%m-%dT%H:%M:%S')"
                    echo "[host] RayOS requested Linux desktop; launching..." >&2
                    echo "$log_ts [show_linux_desktop] requested" >> "$DESKTOP_LAUNCH_LOG"
                    rm -f "$DESKTOP_PID_FILE" 2>/dev/null || true

                    # Guard files to prevent duplicate launches while QEMU spins up.
                    date +%s >"$DESKTOP_LAUNCH_INFLIGHT" 2>/dev/null || true
                    date +%s >"$DESKTOP_RUN_GUARD" 2>/dev/null || true
                    echo "running" >"$DESKTOP_STATE_FILE"

                    # Launch the Linux desktop guest in a separate QEMU window.
                    # It uses a persistent ext4 disk under WORK_DIR for speed across runs.
                    WORK_DIR="$WORK_DIR" \
                    LINUX_DESKTOP_DISK="$WORK_DIR/linux-guest/desktop/desktop-rootfs.ext4" \
                    LINUX_SERIAL_LOG="$WORK_DIR/linux-desktop-serial.log" \
                    "$ROOT_DIR/scripts/run-linux-subsystem-desktop-auto.sh" \
                        >"$DESKTOP_LAUNCH_LOG" 2>&1 </dev/null &

                    echo "$!" >"$DESKTOP_PID_FILE" 2>/dev/null || true
                    DESKTOP_CHILD_PID="$!"

                    # Wait briefly for monitor socket or early exit to avoid relaunch races.
                    for _ in $(seq 1 100); do
                        if [ -S "$DESKTOP_MON_SOCK" ]; then
                            break
                        fi
                        if ! kill -0 "$DESKTOP_CHILD_PID" 2>/dev/null; then
                            break
                        fi
                        sleep 0.1 2>/dev/null || true
                    done

                    rm -f "$DESKTOP_LAUNCH_INFLIGHT" 2>/dev/null || true
                    emit_host_ack "SHOW_LINUX_DESKTOP" "ok" "launching"
                    ;;

                *RAYOS_HOST_EVENT:SHOW_WINDOWS_DESKTOP*)
                    WINDOWS_PID_FILE="$WORK_DIR/.windows-desktop-qemu.pid"
                    WINDOWS_MON_SOCK="$WORK_DIR/windows-desktop-monitor.sock"
                    WINDOWS_LAUNCH_LOG="$WORK_DIR/windows-desktop-launch.log"

                    if [ -f "$WINDOWS_PID_FILE" ]; then
                        win_pid="$(cat "$WINDOWS_PID_FILE" 2>/dev/null || true)"
                        if [ -n "$win_pid" ] && kill -0 "$win_pid" 2>/dev/null; then
                            emit_host_ack "SHOW_WINDOWS_DESKTOP" "ok" "already_running"
                            continue
                        fi
                    fi

                    echo "[host] RayOS requested Windows desktop; launching..." >&2
                    log_ts="$(date '+%Y-%m-%dT%H:%M:%S')"
                    echo "$log_ts [show_windows_desktop] requested" >>"$WINDOWS_LAUNCH_LOG"

                    # WINDOWS_DISK is required (user-provided). Fail fast with ACK.
                    if [ -z "${WINDOWS_DISK:-}" ]; then
                        emit_host_ack "SHOW_WINDOWS_DESKTOP" "err" "missing_WINDOWS_DISK_env"
                        continue
                    fi

                    WORK_DIR="$WORK_DIR" \
                    WINDOWS_DISK="$WINDOWS_DISK" \
                    "$ROOT_DIR/scripts/run-windows-subsystem-desktop.sh" \
                        >"$WINDOWS_LAUNCH_LOG" 2>&1 &

                    echo "$!" >"$WINDOWS_PID_FILE" 2>/dev/null || true
                    emit_host_ack "SHOW_WINDOWS_DESKTOP" "ok" "launching"
                    ;;

                *RAYOS_HOST_EVENT:WINDOWS_SENDTEXT:*)
                    WINDOWS_PID_FILE="$WORK_DIR/.windows-desktop-qemu.pid"
                    WINDOWS_MON_SOCK="$WORK_DIR/windows-desktop-monitor.sock"
                    text="${norm_line#*RAYOS_HOST_EVENT:WINDOWS_SENDTEXT:}"
                    text="${text%$'\r'}"
                    if [ -z "$text" ]; then
                        emit_host_ack "WINDOWS_SENDTEXT" "err" "empty"
                        continue
                    fi
                    if ! validate_ascii_payload "$text" 512; then
                        emit_host_ack "WINDOWS_SENDTEXT" "err" "invalid_ascii_or_length"
                        continue
                    fi
                    if [ ! -f "$WINDOWS_PID_FILE" ]; then
                        emit_host_ack "WINDOWS_SENDTEXT" "err" "desktop_not_running"
                        continue
                    fi
                    win_pid="$(cat "$WINDOWS_PID_FILE" 2>/dev/null || true)"
                    if [ -z "$win_pid" ] || ! kill -0 "$win_pid" 2>/dev/null; then
                        emit_host_ack "WINDOWS_SENDTEXT" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$WINDOWS_MON_SOCK" ]; then
                        emit_host_ack "WINDOWS_SENDTEXT" "err" "no_monitor_sock"
                        continue
                    fi
                    python3 "$ROOT_DIR/scripts/qemu-sendtext.py" \
                        --sock "$WINDOWS_MON_SOCK" \
                        --text "$text" \
                        --wait 1.5 \
                        >/dev/null 2>&1 || true
                    emit_host_ack "WINDOWS_SENDTEXT" "ok" "sent"
                    ;;

                *RAYOS_HOST_EVENT:WINDOWS_SENDKEY:*)
                    WINDOWS_PID_FILE="$WORK_DIR/.windows-desktop-qemu.pid"
                    WINDOWS_MON_SOCK="$WORK_DIR/windows-desktop-monitor.sock"
                    key="${norm_line#*RAYOS_HOST_EVENT:WINDOWS_SENDKEY:}"
                    key="${key%$'\r'}"
                    if [ -z "$key" ]; then
                        emit_host_ack "WINDOWS_SENDKEY" "err" "empty"
                        continue
                    fi
                    if ! validate_ascii_payload "$key" 64; then
                        emit_host_ack "WINDOWS_SENDKEY" "err" "invalid_ascii_or_length"
                        continue
                    fi
                    if [ ! -f "$WINDOWS_PID_FILE" ]; then
                        emit_host_ack "WINDOWS_SENDKEY" "err" "desktop_not_running"
                        continue
                    fi
                    win_pid="$(cat "$WINDOWS_PID_FILE" 2>/dev/null || true)"
                    if [ -z "$win_pid" ] || ! kill -0 "$win_pid" 2>/dev/null; then
                        emit_host_ack "WINDOWS_SENDKEY" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$WINDOWS_MON_SOCK" ]; then
                        emit_host_ack "WINDOWS_SENDKEY" "err" "no_monitor_sock"
                        continue
                    fi
                    python3 "$ROOT_DIR/scripts/qemu-sendkey.py" \
                        --sock "$WINDOWS_MON_SOCK" \
                        --key "$key" \
                        --wait 1.5 \
                        >/dev/null 2>&1 || true
                    emit_host_ack "WINDOWS_SENDKEY" "ok" "sent"
                    ;;

                *RAYOS_HOST_EVENT:WINDOWS_SHUTDOWN*)
                    WINDOWS_PID_FILE="$WORK_DIR/.windows-desktop-qemu.pid"
                    WINDOWS_MON_SOCK="$WORK_DIR/windows-desktop-monitor.sock"
                    if [ ! -f "$WINDOWS_PID_FILE" ]; then
                        emit_host_ack "WINDOWS_SHUTDOWN" "err" "desktop_not_running"
                        continue
                    fi
                    win_pid="$(cat "$WINDOWS_PID_FILE" 2>/dev/null || true)"
                    if [ -z "$win_pid" ] || ! kill -0 "$win_pid" 2>/dev/null; then
                        emit_host_ack "WINDOWS_SHUTDOWN" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$WINDOWS_MON_SOCK" ]; then
                        emit_host_ack "WINDOWS_SHUTDOWN" "err" "no_monitor_sock"
                        continue
                    fi
                    python3 - <<'PY' "$WINDOWS_MON_SOCK" >/dev/null 2>&1 || true
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
s.close()
PY
                    emit_host_ack "WINDOWS_SHUTDOWN" "ok" "powerdown_sent"
                    ;;

                *RAYOS_HOST_EVENT:LINUX_SENDTEXT:*)
                    # Inject a line of text into the Linux desktop VM via its HMP monitor socket.
                    # The kernel emits: RAYOS_HOST_EVENT:LINUX_SENDTEXT:<text>
                    # Note: qemu-sendtext has a conservative ASCII mapping.

                    if ! desktop_running; then
                        emit_host_ack "LINUX_SENDTEXT" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        emit_host_ack "LINUX_SENDTEXT" "err" "no_monitor_sock"
                        continue
                    fi

                    text="${norm_line#*RAYOS_HOST_EVENT:LINUX_SENDTEXT:}"
                    # Strip CR if present.
                    text="${text%$'\r'}"
                    if [ -z "$text" ]; then
                        emit_host_ack "LINUX_SENDTEXT" "err" "empty"
                        continue
                    fi
                    if ! validate_ascii_payload "$text" 512; then
                        emit_host_ack "LINUX_SENDTEXT" "err" "invalid_ascii_or_length"
                        continue
                    fi

                    log_ts="$(date '+%Y-%m-%dT%H:%M:%S')"
                    echo "$log_ts [sendtext] $text" >> "$DESKTOP_LAUNCH_LOG"
                    python3 "$ROOT_DIR/scripts/qemu-sendtext.py" \
                        --sock "$DESKTOP_MON_SOCK" \
                        --text "$text" \
                        --wait 1.5 \
                        >/dev/null 2>&1 || true
                    emit_host_ack "LINUX_SENDTEXT" "ok" "sent"
                    ;;

                *RAYOS_HOST_EVENT:LINUX_SHUTDOWN*)
                    # Ask the Linux desktop VM to shut down.
                    # Best-effort graceful: ACPI power button, then quit if still running.
                    if ! desktop_running; then
                        emit_host_ack "LINUX_SHUTDOWN" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        emit_host_ack "LINUX_SHUTDOWN" "err" "no_monitor_sock"
                        continue
                    fi

                    # Send ACPI powerdown.
                    log_ts="$(date '+%Y-%m-%dT%H:%M:%S')"
                    echo "$log_ts [shutdown] requested" >> "$DESKTOP_LAUNCH_LOG"
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
                        emit_host_ack "LINUX_SHUTDOWN" "ok" "forced_quit"
                    else
                        emit_host_ack "LINUX_SHUTDOWN" "ok" "powerdown_sent"
                    fi

                    rm -f "$DESKTOP_RUN_GUARD" "$DESKTOP_STATE_FILE" 2>/dev/null || true
                    ;;

                *RAYOS_HOST_EVENT:LINUX_SENDKEY:*)
                    # Inject a single key (or key combo) into the Linux desktop VM via HMP sendkey.
                    # The kernel emits: RAYOS_HOST_EVENT:LINUX_SENDKEY:<spec>
                    if ! desktop_running; then
                        emit_host_ack "LINUX_SENDKEY" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        emit_host_ack "LINUX_SENDKEY" "err" "no_monitor_sock"
                        continue
                    fi

                    key="${norm_line#*RAYOS_HOST_EVENT:LINUX_SENDKEY:}"
                    key="${key%$'\r'}"
                    if [ -z "$key" ]; then
                        emit_host_ack "LINUX_SENDKEY" "err" "empty"
                        continue
                    fi
                    if ! validate_ascii_payload "$key" 64; then
                        emit_host_ack "LINUX_SENDKEY" "err" "invalid_ascii_or_length"
                        continue
                    fi

                    log_ts="$(date '+%Y-%m-%dT%H:%M:%S')"
                    echo "$log_ts [sendkey] $key" >> "$DESKTOP_LAUNCH_LOG"
                    python3 "$ROOT_DIR/scripts/qemu-sendkey.py" \
                        --sock "$DESKTOP_MON_SOCK" \
                        --key "$key" \
                        --wait 1.5 \
                        >/dev/null 2>&1 || true
                    emit_host_ack "LINUX_SENDKEY" "ok" "sent"
                    ;;
                *RAYOS_HOST_EVENT:LINUX_MOUSE_ABS:*)
                    # Inject absolute mouse movement into the Linux desktop VM via QEMU monitor socket.
                    # The kernel emits: RAYOS_HOST_EVENT:LINUX_MOUSE_ABS:x:y (normalized 0..1)
                    if ! desktop_running; then
                        emit_host_ack "LINUX_MOUSE_ABS" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        emit_host_ack "LINUX_MOUSE_ABS" "err" "no_monitor_sock"
                        continue
                    fi
                    coords="${norm_line#*RAYOS_HOST_EVENT:LINUX_MOUSE_ABS:}"
                    coords="${coords%$'\r'}"
                    x="$(echo "$coords" | cut -d: -f1)"
                    y="$(echo "$coords" | cut -d: -f2)"
                    if ! validate_float_01 "$x" || ! validate_float_01 "$y"; then
                        emit_host_ack "LINUX_MOUSE_ABS" "err" "invalid_coords"
                        continue
                    fi
                    # QEMU HMP: mouse_move <x> <y> [display] (pixels)
                    # For now, map 0..1 to 0..1023 (QEMU default tablet range)
                    px=$(awk "BEGIN { printf(\"%d\", $x * 1023) }")
                    py=$(awk "BEGIN { printf(\"%d\", $y * 1023) }")
                    python3 - <<'PY' "$DESKTOP_MON_SOCK" "$px" "$py" >/dev/null 2>&1 || true
import socket, sys, time
sock_path = sys.argv[1]
x = int(sys.argv[2])
y = int(sys.argv[3])
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1.0)
s.connect(sock_path)
try:
    s.settimeout(0.2)
    s.recv(4096)
except Exception:
    pass
s.sendall(f"mouse_move {x} {y}\r\n".encode("ascii"))
time.sleep(0.05)
s.close()
PY
                    emit_host_ack "LINUX_MOUSE_ABS" "ok" "${x}:${y}"
                    ;;
                *RAYOS_HOST_EVENT:LINUX_CLICK:*)
                    # Inject mouse button click into the Linux desktop VM via QEMU monitor socket.
                    # The kernel emits: RAYOS_HOST_EVENT:LINUX_CLICK:left|right
                    if ! desktop_running; then
                        emit_host_ack "LINUX_CLICK" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        emit_host_ack "LINUX_CLICK" "err" "no_monitor_sock"
                        continue
                    fi
                    btn="${norm_line#*RAYOS_HOST_EVENT:LINUX_CLICK:}"
                    btn="${btn%$'\r'}"
                    case "$btn" in
                        left)
                            btn_num=0
                            ;;
                        right)
                            btn_num=2
                            ;;
                        *)
                            emit_host_ack "LINUX_CLICK" "err" "unknown_button"
                            continue
                            ;;
                    esac
                    # QEMU HMP: mouse_button <button> <down>
                    python3 - <<'PY' "$DESKTOP_MON_SOCK" "$btn_num" >/dev/null 2>&1 || true
import socket, sys, time
sock_path = sys.argv[1]
btn = int(sys.argv[2])
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1.0)
s.connect(sock_path)
try:
    s.settimeout(0.2)
    s.recv(4096)
except Exception:
    pass
# Press
s.sendall(f"mouse_button {btn} 1\r\n".encode("ascii"))
time.sleep(0.05)
# Release
s.sendall(f"mouse_button {btn} 0\r\n".encode("ascii"))
time.sleep(0.05)
s.close()
PY
                    emit_host_ack "LINUX_CLICK" "ok" "${btn}"
                    ;;
            esac
            done < <(tail -n0 -F "$SERIAL_LOG" 2>/dev/null) || true
            sleep 0.05
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
DISPLAY_ARGS=(-display gtk,zoom-to-fit=on)
if [ "$HEADLESS" != "0" ]; then
    DISPLAY_ARGS=(-display none)
fi

qemu-system-x86_64 \
    -machine q35 \
    -m 2048 \
    -rtc base=utc,clock=host \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive file="fat:rw:$STAGE_DIR",format=raw \
    -chardev "file,id=rayos-serial0,path=$SERIAL_LOG,append=on" \
    -serial "chardev:rayos-serial0" \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -vga std \
    "${DISPLAY_ARGS[@]}"

echo ""
echo "QEMU exited."
