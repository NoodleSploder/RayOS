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
#   BOOT_WITH_AI=1 ./scripts/test-boot.sh

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Kernel Cargo features can be provided via env var (also used later in the build step).
RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-}"

usage() {
    cat <<'USAGE'
Usage: ./scripts/test-boot.sh [--headless|--graphical] [--help]

Env vars:
  HEADLESS=1   Run QEMU without a GUI window

Notes:
  - CLI flags override the HEADLESS env var.
  - Unknown flags are treated as errors.
USAGE
}

# CLI flags (minimal; keep env-var defaults working).
# Examples:
#   ./scripts/test-boot.sh --headless
#   ./scripts/test-boot.sh --graphical
CLI_HEADLESS=""
while [ $# -gt 0 ]; do
    case "$1" in
        --headless)
            CLI_HEADLESS="1"
            shift
            ;;
        --graphical)
            CLI_HEADLESS="0"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "ERROR: unknown argument: $1" >&2
            echo "Run with --help for usage." >&2
            exit 2
            ;;
    esac
done

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

    # Optional: pass extra kernel Cargo features without editing the script.
    # Example:
    #   RAYOS_KERNEL_FEATURES=dev_scanout ./scripts/test-boot.sh
    RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-}"
    KERNEL_FEATURE_ARGS=()
    if [ -n "$RAYOS_KERNEL_FEATURES" ]; then
        KERNEL_FEATURE_ARGS=(--features "$RAYOS_KERNEL_FEATURES")
    fi

    pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
    # Force Cargo to use rustup's toolchain rustc so build-std doesn't accidentally
    # pick up a system rustc/sysroot.
    RUSTC="$(rustup which rustc)" cargo build \
        -Z build-std=core,alloc \
        -Z build-std-features=compiler-builtins-mem \
        --release \
        --target x86_64-unknown-none \
        "${KERNEL_FEATURE_ARGS[@]}"
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

# By default, start a fresh serial log each run.
# This avoids stale markers (e.g. "RayOS bicameral loop ready") causing host-side
# gating to trigger too early and inject keystrokes into firmware screens.
PRESERVE_SERIAL_LOG="${PRESERVE_SERIAL_LOG:-0}"
if [ "$PRESERVE_SERIAL_LOG" = "0" ]; then
    : > "$SERIAL_LOG" 2>/dev/null || true
fi

# Set HEADLESS=1 to run QEMU without a GUI window (useful for CI/smoke tests).
HEADLESS="${HEADLESS:-0}"
if [ -n "$CLI_HEADLESS" ]; then
    HEADLESS="$CLI_HEADLESS"
fi

# QEMU + firmware configuration.
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"
OVMF_CODE="${OVMF_CODE:-}"
QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"
QEMU_TIMEOUT_SECS="${QEMU_TIMEOUT_SECS:-}"

detect_ovmf_code() {
    local candidate=""
    for candidate in \
        /usr/share/OVMF/OVMF_CODE_4M.fd \
        /usr/share/OVMF/OVMF_CODE.fd \
        /usr/share/edk2/ovmf/OVMF_CODE.fd \
        /usr/share/edk2/x64/OVMF_CODE.fd \
        /usr/share/qemu/OVMF.fd \
        /usr/share/OVMF/OVMF.fd
    do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

if [ -z "$OVMF_CODE" ]; then
    if OVMF_CODE="$(detect_ovmf_code 2>/dev/null)"; then
        :
    else
        OVMF_CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
    fi
fi

# Optional: host-side bridge for "show linux/windows desktop" typed inside RayOS.
# We watch the serial log for markers emitted by the kernel and then launch/drive
# guest desktops (separate QEMU windows for now).
#
# Env var name is shared because this bridge handles both Linux and Windows.
# Back-compat: ENABLE_LINUX_DESKTOP_BRIDGE still works.
ENABLE_HOST_DESKTOP_BRIDGE_WANTED="${ENABLE_HOST_DESKTOP_BRIDGE:-${ENABLE_LINUX_DESKTOP_BRIDGE:-0}}"

# If dev_scanout is enabled, prefer RayOS-native presentation and avoid spinning up
# the host-side VNC-based desktop bridge unless explicitly requested.
if [ -n "$RAYOS_KERNEL_FEATURES" ] && echo ",$RAYOS_KERNEL_FEATURES," | grep -q ",dev_scanout,"; then
    if [ -z "${ENABLE_HOST_DESKTOP_BRIDGE+x}" ] && [ -z "${ENABLE_LINUX_DESKTOP_BRIDGE+x}" ]; then
        ENABLE_HOST_DESKTOP_BRIDGE_WANTED="0"
    fi
    if [ -z "${PRELAUNCH_HIDDEN_DESKTOPS+x}" ]; then
        PRELAUNCH_HIDDEN_DESKTOPS="0"
    fi
fi
ENABLE_HOST_DESKTOP_BRIDGE="$ENABLE_HOST_DESKTOP_BRIDGE_WANTED"
# Default behavior for interactive dev: prelaunch hidden desktops at boot.
PRELAUNCH_HIDDEN_DESKTOPS="${PRELAUNCH_HIDDEN_DESKTOPS:-1}"
DESKTOP_BRIDGE_PID=""
DESKTOP_PID_FILE="$WORK_DIR/.linux-desktop-qemu.pid"
DESKTOP_LAUNCH_LOG="$WORK_DIR/linux-desktop-launch.log"
DESKTOP_CONTROL_LOG="$WORK_DIR/linux-desktop-control.log"
DESKTOP_MON_SOCK="$WORK_DIR/linux-desktop-monitor.sock"
DESKTOP_LAUNCH_INFLIGHT="$WORK_DIR/.linux-desktop-launch.inflight"
DESKTOP_RUN_GUARD="$WORK_DIR/.linux-desktop-running.guard"
DESKTOP_STATE_FILE="$WORK_DIR/.linux-desktop.state"
DESKTOP_BRIDGE_LOCK_FILE="$WORK_DIR/.linux-desktop-bridge.lock"
DESKTOP_BRIDGE_LOCK_DIR="$WORK_DIR/.linux-desktop-bridge.lockdir"
DESKTOP_BRIDGE_LOCK_FD=""
HIDDEN_DESKTOP_PID_FILE="$WORK_DIR/.linux-desktop-hidden-qemu.pid"
HIDDEN_DESKTOP_MON_SOCK="$WORK_DIR/linux-desktop-hidden-monitor.sock"
HIDDEN_VNC_TARGET_FILE="$WORK_DIR/.linux-desktop-hidden.vnc_target"
VNC_VIEWER_PID_FILE="$WORK_DIR/.vnc-viewer.pid"
HIDDEN_WINDOWS_PID_FILE="$WORK_DIR/.windows-desktop-hidden-qemu.pid"
HIDDEN_WINDOWS_MON_SOCK="$WORK_DIR/windows-desktop-hidden-monitor.sock"
VNC_VIEWER_WIN_PID_FILE="$WORK_DIR/.vnc-viewer-win.pid"

# If enabled, the host will also inject ACKs back into the RayOS prompt as:
#   @ack <op> <ok|err> <detail>
# so the in-guest UI can show host action outcomes.
INJECT_ACK_TO_GUEST="${INJECT_ACK_TO_GUEST:-1}"

start_linux_desktop_hidden() {
    if [ -f "$HIDDEN_DESKTOP_PID_FILE" ]; then
        local existing_pid
        existing_pid="$(cat "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true)"
        if [ -n "$existing_pid" ] && kill -0 "$existing_pid" 2>/dev/null; then
            echo "[host] Hidden Linux desktop already running (pid=$existing_pid)" >&2
            return 0
        fi
        rm -f "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true
    fi

    echo "[host] Launching hidden Linux desktop..."
    # NOTE: Do not use QEMU savevm/loadvm for the Linux desktop.
    # The persistent disk is a raw ext4 image (format=raw), which cannot be used with
    # internal snapshots (-loadvm). Attempting to do so aborts QEMU with:
    #   "Device 'virtio0' is writable but does not support snapshots"
    rm -f "$WORK_DIR/.linux-desktop-hidden.state" 2>/dev/null || true

    # Use a TCP-bound VNC server on localhost so common viewers can attach.
    # Pick a free display to avoid collisions with other VNC users.
    local pick_disp=""
    for d in $(seq 0 9); do
        if python3 - "$d" <<'PY' >/dev/null 2>&1; then
import socket, sys
disp = int(sys.argv[1])
port = 5900 + disp
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    s.bind(("127.0.0.1", port))
    s.close()
    sys.exit(0)
except Exception:
    sys.exit(1)
PY
            pick_disp="$d"
            break
        fi
    done

    if [ -z "$pick_disp" ]; then
        pick_disp="0"
    fi

    local vnc_target="${LINUX_VNC_TARGET:-127.0.0.1:${pick_disp}}"
    printf '%s' "localhost:${pick_disp}" >"$HIDDEN_VNC_TARGET_FILE" 2>/dev/null || true

    # Background probe: wait for RayOS monitor socket, then announce starting/ready.
    # This is intentionally async so RayOS QEMU window appears immediately.
    (
        # Wait for RayOS to reach the interactive prompt before injecting any @ack keystrokes.
        # (Monitor socket alone isn't sufficient; early sendkey can land in firmware screens.)
        for _ in $(seq 1 600); do
            if [ -f "$SERIAL_LOG" ] && grep -F -a -q "RayOS bicameral loop ready" "$SERIAL_LOG" 2>/dev/null; then
                break
            fi
            sleep 0.1 2>/dev/null || true
        done

        # Wait (briefly) for the RayOS monitor socket so we can inject @ack lines.
        for _ in $(seq 1 200); do
            if [ -S "$MON_SOCK" ]; then
                emit_host_ack "LINUX_DESKTOP" "ok" "starting"
                break
            fi
            sleep 0.05 2>/dev/null || true
        done

        local vnc_probe_host="127.0.0.1"
        local vnc_probe_port="5900"
        if printf '%s' "$vnc_target" | grep -q ':'; then
            vnc_probe_host="${vnc_target%:*}"
            vnc_probe_tail="${vnc_target##*:}"
            if [ -z "$vnc_probe_host" ]; then vnc_probe_host="127.0.0.1"; fi
            if printf '%s' "$vnc_probe_tail" | grep -qE '^[0-9]+$'; then
                if [ "$vnc_probe_tail" -lt 100 ]; then
                    vnc_probe_port=$((5900 + vnc_probe_tail))
                else
                    vnc_probe_port="$vnc_probe_tail"
                fi
            fi
        fi

        echo "[host] (bg) waiting for hidden VM VNC endpoint ($vnc_probe_host:$vnc_probe_port)..." >&2
        local ready=0
        for i in $(seq 1 60); do
            if python3 - "$vnc_probe_host" "$vnc_probe_port" <<'PY' >/dev/null 2>&1; then
import socket, sys
host, port = sys.argv[1], int(sys.argv[2])
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
try:
    s.settimeout(1.0)
    s.connect((host, port))
    s.close()
    sys.exit(0)
except Exception:
    sys.exit(1)
PY
                ready=1
                break
            fi
            sleep 1
        done

        if [ "$ready" -eq 1 ]; then
            emit_host_ack "LINUX_DESKTOP_HIDDEN_READY" "ok" "vnc_ready"
            emit_host_marker "LINUX_DESKTOP_HIDDEN" "running" "vnc_ready"

            # Best-effort single-frame capture for determinism/diagnostics.
            local framebuffer_out="$WORK_DIR/framebuffer-hidden.raw"
            if python3 "$ROOT_DIR/scripts/read-framebuffer.py" --sock "$vnc_probe_host:$vnc_probe_port" --out "$framebuffer_out" --raw >/dev/null 2>&1; then
                local expected_size=$((1024 * 768 * 4))
                local actual_size
                actual_size=$(stat -c%s "$framebuffer_out" 2>/dev/null || echo 0)
                if [ "$actual_size" -eq "$expected_size" ]; then
                    emit_host_ack "FIRST_FRAME_PRESENTED" "ok" "initial_frame"
                fi
            fi
        else
            emit_host_ack "LINUX_DESKTOP_HIDDEN_READY" "err" "vnc_timeout"
            emit_host_marker "LINUX_DESKTOP_HIDDEN" "stopped" "vnc_timeout"
        fi
    ) >/dev/null 2>&1 &

    WORK_DIR="$WORK_DIR" \
    LINUX_DESKTOP_DISK="$WORK_DIR/linux-guest/desktop/desktop-rootfs.ext4" \
    LINUX_SERIAL_LOG="$WORK_DIR/linux-desktop-hidden-serial.log" \
    LINUX_DISPLAY_TYPE=vnc \
    LINUX_VNC_TARGET="$vnc_target" \
    LINUX_DESKTOP_GLOBAL_LOCK="$WORK_DIR/.linux-desktop-auto.global.lock" \
    LINUX_DESKTOP_GLOBAL_LOCK_DIR="$WORK_DIR/.linux-desktop-auto.global.lockdir" \
    LINUX_DESKTOP_MONITOR_SOCK="$HIDDEN_DESKTOP_MON_SOCK" \
    RAYOS_SERIAL_LOG="$SERIAL_LOG" \
    "$ROOT_DIR/scripts/run-linux-subsystem-desktop-auto.sh" >"$WORK_DIR/linux-desktop-hidden-launch.log" 2>&1 </dev/null &
    echo "$!" >"$HIDDEN_DESKTOP_PID_FILE"

    # If the launcher exited immediately (e.g., due to a stale lock), don't keep a dead PID around.
    sleep 0.05 2>/dev/null || true
    local hp
    hp="$(cat "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true)"
    if [ -n "$hp" ] && ! kill -0 "$hp" 2>/dev/null; then
        rm -f "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true
        echo "[host] Hidden Linux desktop launcher exited immediately; see $WORK_DIR/linux-desktop-hidden-launch.log" >&2
    fi

    echo "[host] Hidden Linux desktop launch initiated (pid=$(cat "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || echo '?'))" >&2
}

launch_vnc_viewer_linux() {
    # Returns 0 if a viewer was launched.
    local target="${1:-localhost:0}"

    if command -v gvncviewer >/dev/null 2>&1; then
        gvncviewer "$target" >/dev/null 2>&1 &
        echo "$!" > "$VNC_VIEWER_PID_FILE"
        return 0
    fi

    # remote-viewer usually expects a URL.
    if command -v remote-viewer >/dev/null 2>&1; then
        # Convert localhost:N to vnc://127.0.0.1:59NN when possible.
        if printf '%s' "$target" | grep -qE '^[^:]+:[0-9]+$'; then
            local host="${target%:*}"
            local disp="${target##*:}"
            if [ "$disp" -lt 100 ]; then
                local port=$((5900 + disp))
                remote-viewer "vnc://${host}:${port}" >/dev/null 2>&1 &
                echo "$!" > "$VNC_VIEWER_PID_FILE"
                return 0
            fi
        fi
        remote-viewer "vnc://$target" >/dev/null 2>&1 &
        echo "$!" > "$VNC_VIEWER_PID_FILE"
        return 0
    fi

    if command -v vncviewer >/dev/null 2>&1; then
        vncviewer "$target" >/dev/null 2>&1 &
        echo "$!" > "$VNC_VIEWER_PID_FILE"
        return 0
    fi

    if command -v tigervncviewer >/dev/null 2>&1; then
        tigervncviewer "$target" >/dev/null 2>&1 &
        echo "$!" > "$VNC_VIEWER_PID_FILE"
        return 0
    fi

    return 1
}

start_windows_desktop_hidden() {
    echo "[host] Launching hidden Windows desktop..."
    local loadvm_tag=""
    local state_file="$WORK_DIR/.windows-desktop-hidden.state"
    if [ -f "$state_file" ]; then
        loadvm_tag=$(cat "$state_file")
    fi

    WORK_DIR="$WORK_DIR" \
    WINDOWS_DISK="$WINDOWS_DISK" \
    LINUX_DISPLAY_TYPE=vnc \
    LINUX_LOADVM_TAG="$loadvm_tag" \
    WINDOWS_MONITOR_SOCK="$HIDDEN_WINDOWS_MON_SOCK" \
    "$ROOT_DIR/scripts/run-windows-subsystem-desktop.sh" >"$WORK_DIR/windows-desktop-hidden-launch.log" 2>&1 </dev/null &
    echo "$!" >"$HIDDEN_WINDOWS_PID_FILE"

    echo "[host] Waiting for hidden Windows VM to be ready..."
    for _ in $(seq 1 60); do
        if python3 - "$HIDDEN_WINDOWS_MON_SOCK" <<'PY' >/dev/null 2>&1; then
import socket, sys, json, time
sock_path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1.0)
s.connect(sock_path)
s.settimeout(0.2)
try: s.recv(4096)
except Exception: pass
s.sendall(b'{"execute": "guest-ping"}\r\n')
time.sleep(0.5)
resp = s.recv(4096)
if b'return' in resp:
    sys.exit(0)
sys.exit(1)
PY
            echo "[host] Windows VM is ready."
            emit_host_ack "WINDOWS_READY" "ok" "guest_agent_ping"
            break
        fi
        sleep 1
    done
}

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

log_desktop_control() {
    local tag="$1"
    shift || true
    local ts
    ts="$(date '+%Y-%m-%dT%H:%M:%S')"
    printf "%s [%s] %s\n" "$ts" "$tag" "$*" >>"$DESKTOP_CONTROL_LOG"
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

    if [ "$INJECT_ACK_TO_GUEST" = "1" ] && [ -S "$MON_SOCK" ] && command -v python3 >/dev/null 2>&1; then
        # Best-effort: keep it short and ASCII; replace spaces to preserve token parsing.
        local safe_detail
        safe_detail="$(printf '%s' "$detail" | tr ' ' '_' | tr -cd '\040-\176' | cut -c1-64)"
        python3 "$ROOT_DIR/scripts/qemu-sendtext.py" \
            --sock "$MON_SOCK" \
            --text "@ack $op $status $safe_detail" \
            --wait 0.8 \
            >/dev/null 2>&1 || true
    fi
}

emit_host_marker() {
    local op="$1"
    local status="$2"
    local detail="${3:-}"
    if [ -n "$SERIAL_LOG" ]; then
        printf "RAYOS_HOST_MARKER:%s:%s:%s\n" "$op" "$status" "$detail" >>"$SERIAL_LOG"
    fi
}

desktop_running() {
    # Returns 0 if the desktop VM is running and sets DESKTOP_QEMU_PID.
    # Returns 1 otherwise.
    if [ -f "$DESKTOP_LAUNCH_INFLIGHT" ] || [ -f "$DESKTOP_RUN_GUARD" ] || [ -f "$DESKTOP_STATE_FILE" ]; then
        # Guard files are helpful to debounce launches, but they can become stale
        # after a failed start. Only treat them as "running" when we can verify
        # a live monitor socket or live PID.

        # Monitor socket implies QEMU is up.
        if [ -S "$DESKTOP_MON_SOCK" ]; then
            DESKTOP_QEMU_PID=""
            return 0
        fi

        # Live PID implies QEMU is up.
        if [ -f "$DESKTOP_PID_FILE" ]; then
            DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
            if [ -n "$DESKTOP_QEMU_PID" ] && kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
                return 0
            fi
        fi

        # Clear stale guards (older than ~30s) so the next request can recover.
        local now_ts inflight_ts guard_ts age
        now_ts="$(date +%s 2>/dev/null || echo 0)"
        inflight_ts="$(cat "$DESKTOP_LAUNCH_INFLIGHT" 2>/dev/null || echo 0)"
        guard_ts="$(cat "$DESKTOP_RUN_GUARD" 2>/dev/null || echo 0)"

        if [ "$now_ts" -ne 0 ] && [ "$guard_ts" -ne 0 ]; then
            age=$((now_ts - guard_ts))
            if [ "$age" -gt 30 ]; then
                rm -f "$DESKTOP_LAUNCH_INFLIGHT" "$DESKTOP_RUN_GUARD" "$DESKTOP_STATE_FILE" 2>/dev/null || true
            fi
        elif [ "$now_ts" -ne 0 ] && [ "$inflight_ts" -ne 0 ]; then
            age=$((now_ts - inflight_ts))
            if [ "$age" -gt 30 ]; then
                rm -f "$DESKTOP_LAUNCH_INFLIGHT" "$DESKTOP_RUN_GUARD" "$DESKTOP_STATE_FILE" 2>/dev/null || true
            fi
        fi
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

stop_linux_desktop() {
    # Best-effort stop of the Linux desktop VM without killing the bridge.
    local reason="$1"
    log_desktop_control "stop" "$reason"

    if [ -S "$DESKTOP_MON_SOCK" ]; then
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

    if [ -f "$DESKTOP_PID_FILE" ]; then
        DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
        if [ -n "$DESKTOP_QEMU_PID" ] && kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
            kill "$DESKTOP_QEMU_PID" 2>/dev/null || true
        fi
    fi

    rm -f "$DESKTOP_PID_FILE" "$DESKTOP_MON_SOCK" 2>/dev/null || true
    rm -f "$DESKTOP_LAUNCH_INFLIGHT" "$DESKTOP_RUN_GUARD" "$DESKTOP_STATE_FILE" 2>/dev/null || true
}

cleanup_bridge() {
    if [ -n "$DESKTOP_BRIDGE_PID" ]; then
        kill "$DESKTOP_BRIDGE_PID" 2>/dev/null || true
    fi

    if [ -f "$HIDDEN_DESKTOP_PID_FILE" ]; then
        hidden_pid="$(cat "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true)"
        if [ -n "$hidden_pid" ] && kill -0 "$hidden_pid" 2>/dev/null; then
            echo "[host] Stopping hidden Linux desktop (pid=$hidden_pid)" >&2
            kill "$hidden_pid" 2>/dev/null || true
        fi
        rm -f "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true
        rm -f "$WORK_DIR/.linux-desktop-hidden.state" 2>/dev/null || true
        emit_host_marker "LINUX_DESKTOP_HIDDEN" "stopped" "cleanup"
    fi

    if [ -f "$VNC_VIEWER_PID_FILE" ]; then
        vnc_viewer_pid="$(cat "$VNC_VIEWER_PID_FILE" 2>/dev/null || true)"
        if [ -n "$vnc_viewer_pid" ] && kill -0 "$vnc_viewer_pid" 2>/dev/null; then
            kill "$vnc_viewer_pid" 2>/dev/null || true
        fi
        rm -f "$VNC_VIEWER_PID_FILE" 2>/dev/null || true
    fi

    if [ -f "$HIDDEN_WINDOWS_PID_FILE" ]; then
        hidden_pid="$(cat "$HIDDEN_WINDOWS_PID_FILE" 2>/dev/null || true)"
        if [ -n "$hidden_pid" ] && kill -0 "$hidden_pid" 2>/dev/null; then
            echo "[host] Saving and stopping hidden Windows desktop (pid=$hidden_pid)" >&2
            local state_file="$WORK_DIR/.windows-desktop-hidden.state"
            local tag="pre-exit-win-$(date +%s)"
            if [ -S "$HIDDEN_WINDOWS_MON_SOCK" ]; then
                python3 - "$HIDDEN_WINDOWS_MON_SOCK" "$tag" <<'PY' >/dev/null 2>&1
import socket, sys, time
sock_path, tag = sys.argv[1], sys.argv[2]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(2.0)
s.connect(sock_path)
s.settimeout(0.2)
try: s.recv(4096)
except Exception: pass
s.sendall(f"savevm {tag}\r\n".encode())
time.sleep(0.1)
s.close()
PY
                echo "$tag" > "$state_file"
            fi
            kill "$hidden_pid" 2>/dev/null || true
        fi
        rm -f "$HIDDEN_WINDOWS_PID_FILE" 2>/dev/null || true
    fi

    if [ -f "$VNC_VIEWER_WIN_PID_FILE" ]; then
        vnc_viewer_pid="$(cat "$VNC_VIEWER_WIN_PID_FILE" 2>/dev/null || true)"
        if [ -n "$vnc_viewer_pid" ] && kill -0 "$vnc_viewer_pid" 2>/dev/null; then
            kill "$vnc_viewer_pid" 2>/dev/null || true
        fi
        rm -f "$VNC_VIEWER_WIN_PID_FILE" 2>/dev/null || true
    fi

    if [ -f "$DESKTOP_PID_FILE" ]; then
        DESKTOP_QEMU_PID="$(cat "$DESKTOP_PID_FILE" 2>/dev/null || true)"
        if [ -n "$DESKTOP_QEMU_PID" ] && kill -0 "$DESKTOP_QEMU_PID" 2>/dev/null; then
            echo "[host] Stopping Linux desktop (pid=$DESKTOP_QEMU_PID)" >&2
            kill "$DESKTOP_QEMU_PID" 2>/dev/null || true
        fi
        rm -f "$DESKTOP_PID_FILE" 2>/dev/null || true
    fi

    rm -f "$DESKTOP_LAUNCH_INFLIGHT" "$DESKTOP_RUN_GUARD" "$DESKTOP_STATE_FILE" 2>/dev/null || true
    if [ -n "$DESKTOP_LAUNCH_LOCK_DIR" ]; then
        rmdir "$DESKTOP_LAUNCH_LOCK_DIR" 2>/dev/null || true
    fi

    if [ -n "$DESKTOP_BRIDGE_LOCK_FD" ]; then
        eval "exec ${DESKTOP_BRIDGE_LOCK_FD}>&-" 2>/dev/null || true
    fi
    rmdir "$DESKTOP_BRIDGE_LOCK_DIR" 2>/dev/null || true
}

if [ "$ENABLE_HOST_DESKTOP_BRIDGE" != "0" ]; then
    # Ensure only one bridge instance per WORK_DIR, even if multiple test-boot.sh
    # processes are running. Without this, multiple tailers can react to a single
    # SHOW_LINUX_DESKTOP event and launch multiple desktop VMs.
    if command -v flock >/dev/null 2>&1; then
        DESKTOP_BRIDGE_LOCK_FD="9"
        eval "exec ${DESKTOP_BRIDGE_LOCK_FD}>\"$DESKTOP_BRIDGE_LOCK_FILE\""
        if ! flock -n "$DESKTOP_BRIDGE_LOCK_FD"; then
            # Prefer correctness (show/hide works) over strict single-bridge enforcement.
            # Duplicate bridges are largely de-risked by per-WORK_DIR launch serialization
            # and the per-WORK_DIR Linux desktop launcher lock.
            echo "[host] Linux desktop bridge already active for WORK_DIR=$WORK_DIR; continuing without acquiring bridge lock (may duplicate)." >&2
            DESKTOP_BRIDGE_LOCK_FD=""
        fi
    else
        if ! mkdir "$DESKTOP_BRIDGE_LOCK_DIR" 2>/dev/null; then
            echo "[host] Linux desktop bridge already active for WORK_DIR=$WORK_DIR; continuing without acquiring bridge lock (may duplicate)." >&2
        fi
    fi
fi

if [ "$ENABLE_HOST_DESKTOP_BRIDGE" != "0" ]; then
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
                    # If a viewer is already up, treat show as idempotent.
                    if [ -f "$VNC_VIEWER_PID_FILE" ]; then
                        vnc_viewer_pid="$(cat "$VNC_VIEWER_PID_FILE" 2>/dev/null || true)"
                        if [ -n "$vnc_viewer_pid" ] && kill -0 "$vnc_viewer_pid" 2>/dev/null; then
                            emit_host_ack "SHOW_LINUX_DESKTOP" "ok" "already_showing_vnc"
                            emit_host_marker "LINUX_DESKTOP_PRESENTED" "ok" "already_showing_vnc"
                            continue
                        fi
                    fi

                        # Prefer the hidden prelaunched desktop when available.
                        if [ -f "$HIDDEN_DESKTOP_PID_FILE" ]; then
                        hidden_pid="$(cat "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true)"
                        if [ -n "$hidden_pid" ] && kill -0 "$hidden_pid" 2>/dev/null; then
                            target="localhost:0"
                            if [ -f "$HIDDEN_VNC_TARGET_FILE" ]; then
                                target="$(cat "$HIDDEN_VNC_TARGET_FILE" 2>/dev/null || echo "localhost:0")"
                            fi
                            if launch_vnc_viewer_linux "$target"; then
                                emit_host_ack "SHOW_LINUX_DESKTOP" "ok" "showing_vnc"
                                emit_host_marker "LINUX_DESKTOP_PRESENTED" "ok" "showing_vnc"
                                continue
                            else
                                # Last-resort fallback: stop the hidden VM and launch a visible desktop.
                                # This preserves disk state but not RAM state.
                                echo "[host] No VNC viewer found; falling back to launching a visible Linux desktop VM." >&2
                                emit_host_ack "SHOW_LINUX_DESKTOP" "err" "no_vnc_viewer_fallback_visible"
                                kill "$hidden_pid" 2>/dev/null || true
                                emit_host_marker "LINUX_DESKTOP_HIDDEN" "stopped" "viewer_missing"
                                # Give the process a moment to exit so per-WORK_DIR locks release.
                                for _ in $(seq 1 50); do
                                    if ! kill -0 "$hidden_pid" 2>/dev/null; then
                                        break
                                    fi
                                    sleep 0.05 2>/dev/null || true
                                done
                                rm -f "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true
                                # Continue into normal launch path below.
                            fi
                        fi
                    fi

                    # If the hidden VM isn't running (or wasn't prelaunched), try starting it now.
                    if [ ! -f "$HIDDEN_DESKTOP_PID_FILE" ] || ! kill -0 "$(cat "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || echo 0)" 2>/dev/null; then
                        start_linux_desktop_hidden || true
                        if [ -f "$HIDDEN_DESKTOP_PID_FILE" ]; then
                            hidden_pid="$(cat "$HIDDEN_DESKTOP_PID_FILE" 2>/dev/null || true)"
                            if [ -n "$hidden_pid" ] && kill -0 "$hidden_pid" 2>/dev/null; then
                                target="localhost:0"
                                if [ -f "$HIDDEN_VNC_TARGET_FILE" ]; then
                                    target="$(cat "$HIDDEN_VNC_TARGET_FILE" 2>/dev/null || echo "localhost:0")"
                                fi
                                if launch_vnc_viewer_linux "$target"; then
                                    emit_host_ack "SHOW_LINUX_DESKTOP" "ok" "showing_vnc"
                                    emit_host_marker "LINUX_DESKTOP_PRESENTED" "ok" "showing_vnc"
                                    continue
                                fi
                            fi
                        fi
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
                    RAYOS_SERIAL_LOG="$SERIAL_LOG" \
                    LINUX_DESKTOP_GLOBAL_LOCK="$WORK_DIR/.linux-desktop-auto.global.lock" \
                    LINUX_DESKTOP_GLOBAL_LOCK_DIR="$WORK_DIR/.linux-desktop-auto.global.lockdir" \
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
                    if [ -S "$DESKTOP_MON_SOCK" ]; then
                        emit_host_ack "SHOW_LINUX_DESKTOP" "ok" "launching"
                    else
                        emit_host_ack "SHOW_LINUX_DESKTOP" "err" "launch_failed_no_monitor"
                    fi
                    rmdir "$DESKTOP_LAUNCH_LOCK_DIR" 2>/dev/null || true
                    ;;

                *RAYOS_HOST_EVENT:HIDE_LINUX_DESKTOP*)
                    if [ -f "$VNC_VIEWER_PID_FILE" ]; then
                        vnc_viewer_pid="$(cat "$VNC_VIEWER_PID_FILE" 2>/dev/null || true)"
                        if [ -n "$vnc_viewer_pid" ] && kill -0 "$vnc_viewer_pid" 2>/dev/null; then
                            kill "$vnc_viewer_pid" 2>/dev/null || true
                            rm -f "$VNC_VIEWER_PID_FILE"
                            emit_host_ack "HIDE_LINUX_DESKTOP" "ok" "hiding_vnc"
                            continue
                        fi
                    fi

                    if ! desktop_running; then
                        emit_host_ack "HIDE_LINUX_DESKTOP" "err" "desktop_not_running"
                        continue
                    fi
                    stop_linux_desktop "hide_requested"
                    emit_host_ack "HIDE_LINUX_DESKTOP" "ok" "stopped"
                    ;;

                *RAYOS_HOST_EVENT:HIDE_WINDOWS_DESKTOP*)
                    if [ -f "$VNC_VIEWER_WIN_PID_FILE" ]; then
                        vnc_viewer_pid="$(cat "$VNC_VIEWER_WIN_PID_FILE" 2>/dev/null || true)"
                        if [ -n "$vnc_viewer_pid" ] && kill -0 "$vnc_viewer_pid" 2>/dev/null; then
                            kill "$vnc_viewer_pid" 2>/dev/null || true
                            rm -f "$VNC_VIEWER_WIN_PID_FILE"
                            emit_host_ack "HIDE_WINDOWS_DESKTOP" "ok" "hiding_vnc"
                            continue
                        fi
                    fi
                    ;;

                *RAYOS_HOST_EVENT:SHOW_WINDOWS_DESKTOP*)
                    if [ -f "$HIDDEN_WINDOWS_PID_FILE" ]; then
                        hidden_pid="$(cat "$HIDDEN_WINDOWS_PID_FILE" 2>/dev/null || true)"
                        if [ -n "$hidden_pid" ] && kill -0 "$hidden_pid" 2>/dev/null; then
                            if command -v gvncviewer >/dev/null 2>&1; then
                                gvncviewer localhost:1 >/dev/null 2>&1 &
                                echo "$!" > "$VNC_VIEWER_WIN_PID_FILE"
                                emit_host_ack "SHOW_WINDOWS_DESKTOP" "ok" "showing_vnc"
                            else
                                emit_host_ack "SHOW_WINDOWS_DESKTOP" "err" "gvncviewer_not_found"
                            fi
                            continue
                        fi
                    fi

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
                    log_desktop_control "sendtext" "$text"
                    python3 "$ROOT_DIR/scripts/qemu-sendtext.py" \
                        --sock "$DESKTOP_MON_SOCK" \
                        --text "$text" \
                        --wait 1.5 \
                        >/dev/null 2>&1
                    rc=$?
                    if [ "$rc" -eq 0 ]; then
                        emit_host_ack "LINUX_SENDTEXT" "ok" "sent"
                    else
                        emit_host_ack "LINUX_SENDTEXT" "err" "send_failed"
                    fi
                    ;;

                *RAYOS_HOST_EVENT:LINUX_LAUNCH_APP:*)
                    # Controlled-ish app launch: inject the app name into the Linux desktop VM.
                    # This assumes a terminal is already open (rayos_desktop_init starts weston-terminal).
                    # The kernel emits: RAYOS_HOST_EVENT:LINUX_LAUNCH_APP:<app>

                    if ! desktop_running; then
                        emit_host_ack "LINUX_LAUNCH_APP" "err" "desktop_not_running"
                        continue
                    fi
                    if [ ! -S "$DESKTOP_MON_SOCK" ]; then
                        emit_host_ack "LINUX_LAUNCH_APP" "err" "no_monitor_sock"
                        continue
                    fi

                    app="${norm_line#*RAYOS_HOST_EVENT:LINUX_LAUNCH_APP:}"
                    app="${app%$'\r'}"
                    if [ -z "$app" ]; then
                        emit_host_ack "LINUX_LAUNCH_APP" "err" "empty"
                        continue
                    fi
                    if ! validate_ascii_payload "$app" 64; then
                        emit_host_ack "LINUX_LAUNCH_APP" "err" "invalid_ascii_or_length"
                        continue
                    fi

                    log_desktop_control "launch_app" "$app"
                    python3 "$ROOT_DIR/scripts/qemu-sendtext.py" \
                        --sock "$DESKTOP_MON_SOCK" \
                        --text "$app" \
                        --wait 1.5 \
                        >/dev/null 2>&1
                    rc=$?
                    if [ "$rc" -ne 0 ]; then
                        emit_host_ack "LINUX_LAUNCH_APP" "err" "send_failed"
                        continue
                    fi

                    python3 "$ROOT_DIR/scripts/qemu-sendkey.py" \
                        --sock "$DESKTOP_MON_SOCK" \
                        --key "ret" \
                        --wait 1.5 \
                        >/dev/null 2>&1 || true

                    emit_host_ack "LINUX_LAUNCH_APP" "ok" "sent"
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

                    stop_linux_desktop "shutdown_requested"
                    emit_host_ack "LINUX_SHUTDOWN" "ok" "stopped"
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
                    log_desktop_control "sendkey" "$key"
                    python3 "$ROOT_DIR/scripts/qemu-sendkey.py" \
                        --sock "$DESKTOP_MON_SOCK" \
                        --key "$key" \
                        --wait 1.5 \
                        >/dev/null 2>&1
                    rc=$?
                    if [ "$rc" -eq 0 ]; then
                        emit_host_ack "LINUX_SENDKEY" "ok" "sent"
                    else
                        emit_host_ack "LINUX_SENDKEY" "err" "send_failed"
                    fi
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
                    log_desktop_control "mouse_abs" "${x}:${y}"
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
                    log_desktop_control "click" "${btn}"
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

# Optional: stage Linux guest artifacts for the in-kernel VMM.
# If provided, these are copied into:
#   EFI/RAYOS/linux/vmlinuz
#   EFI/RAYOS/linux/initrd
#   EFI/RAYOS/linux/cmdline.txt
RAYOS_LINUX_GUEST_KERNEL_SRC="${RAYOS_LINUX_GUEST_KERNEL_SRC:-}"
RAYOS_LINUX_GUEST_INITRD_SRC="${RAYOS_LINUX_GUEST_INITRD_SRC:-}"
RAYOS_LINUX_GUEST_CMDLINE_SRC="${RAYOS_LINUX_GUEST_CMDLINE_SRC:-}"

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

if [ -n "$RAYOS_LINUX_GUEST_KERNEL_SRC" ] || [ -n "$RAYOS_LINUX_GUEST_INITRD_SRC" ] || [ -n "$RAYOS_LINUX_GUEST_CMDLINE_SRC" ]; then
    mkdir -p "$STAGE_DIR/EFI/RAYOS/linux"

    if [ -n "$RAYOS_LINUX_GUEST_KERNEL_SRC" ]; then
        if [ -f "$RAYOS_LINUX_GUEST_KERNEL_SRC" ]; then
            cp "$RAYOS_LINUX_GUEST_KERNEL_SRC" "$STAGE_DIR/EFI/RAYOS/linux/vmlinuz"
        else
            echo "Warning: RAYOS_LINUX_GUEST_KERNEL_SRC set but not found: $RAYOS_LINUX_GUEST_KERNEL_SRC" >&2
        fi
    fi

    if [ -n "$RAYOS_LINUX_GUEST_INITRD_SRC" ]; then
        if [ -f "$RAYOS_LINUX_GUEST_INITRD_SRC" ]; then
            cp "$RAYOS_LINUX_GUEST_INITRD_SRC" "$STAGE_DIR/EFI/RAYOS/linux/initrd"
        else
            echo "Warning: RAYOS_LINUX_GUEST_INITRD_SRC set but not found: $RAYOS_LINUX_GUEST_INITRD_SRC" >&2
        fi
    fi

    if [ -n "$RAYOS_LINUX_GUEST_CMDLINE_SRC" ]; then
        if [ -f "$RAYOS_LINUX_GUEST_CMDLINE_SRC" ]; then
            cp "$RAYOS_LINUX_GUEST_CMDLINE_SRC" "$STAGE_DIR/EFI/RAYOS/linux/cmdline.txt"
        else
            echo "Warning: RAYOS_LINUX_GUEST_CMDLINE_SRC set but not found: $RAYOS_LINUX_GUEST_CMDLINE_SRC" >&2
        fi
    fi
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

MODE_LABEL="Graphical Mode"
if [ "$HEADLESS" != "0" ]; then
    MODE_LABEL="Headless Mode"
fi

echo "╔═══════════════════════════════════════════════════╗"
printf "║        RayOS Boot Test (%-14s)        ║\n" "$MODE_LABEL"
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
if [ -n "$RAYOS_KERNEL_FEATURES" ] && echo ",$RAYOS_KERNEL_FEATURES," | grep -q ",dev_scanout,"; then
    echo "Hint: dev_scanout enabled -> RayOS auto-presents the synthetic guest panel."
    echo "      Press \` (backtick) to toggle hide/show."
    echo "      Optional: RAYOS_DEV_SCANOUT_AUTOHIDE_SECS=N to auto-hide."
fi
echo ""

echo "Serial log: $SERIAL_LOG"
echo "Monitor sock: $MON_SOCK"
echo "FAT stage: $STAGE_DIR"
echo ""

# If we're running without a GUI environment, auto-fallback to headless.
if [ "$HEADLESS" = "0" ] && [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo "Warning: no DISPLAY/WAYLAND_DISPLAY detected; forcing HEADLESS=1." >&2
    HEADLESS="1"
fi

DISPLAY_ARGS=()
if [ "$HEADLESS" != "0" ]; then
    DISPLAY_ARGS=(-display none)
else
    # Prefer GTK (for zoom-to-fit), but gracefully fall back if unavailable.
    if "$QEMU_BIN" -display help >/dev/null 2>&1; then
        if "$QEMU_BIN" -display help 2>/dev/null | grep -qE '^gtk$'; then
            DISPLAY_ARGS=(-display gtk,zoom-to-fit=on)
        elif "$QEMU_BIN" -display help 2>/dev/null | grep -qE '^sdl$'; then
            DISPLAY_ARGS=(-display sdl)
        else
            DISPLAY_ARGS=()
        fi
    fi
fi

if [ ! -f "$OVMF_CODE" ]; then
    echo "ERROR: OVMF_CODE not found at $OVMF_CODE" >&2
    echo "Hint: set OVMF_CODE=/path/to/OVMF_CODE.fd" >&2
    exit 1
fi

if ! command -v "$QEMU_BIN" >/dev/null 2>&1; then
    echo "ERROR: QEMU binary not found: $QEMU_BIN" >&2
    echo "Hint: set QEMU_BIN=qemu-system-x86_64 (or install qemu-system-x86)" >&2
    exit 1
fi

if [ "$ENABLE_HOST_DESKTOP_BRIDGE_WANTED" != "0" ]; then
    if [ "$PRELAUNCH_HIDDEN_DESKTOPS" != "0" ]; then
        start_linux_desktop_hidden
        if [ -n "$WINDOWS_DISK" ]; then
            start_windows_desktop_hidden
        fi
    fi
fi

QEMU_PREFIX=()
if [ -n "$QEMU_TIMEOUT_SECS" ]; then
    QEMU_PREFIX=(timeout "$QEMU_TIMEOUT_SECS")
fi

"${QEMU_PREFIX[@]}" "$QEMU_BIN" \
    -machine q35 \
    -m 2048 \
    -rtc base=utc,clock=host \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive file="fat:rw:$STAGE_DIR",format=raw \
    -chardev "file,id=rayos-serial0,path=$SERIAL_LOG,append=on" \
    -serial "chardev:rayos-serial0" \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -vga std \
    ${QEMU_EXTRA_ARGS} \
    "${DISPLAY_ARGS[@]}"

echo ""
echo "QEMU exited."
