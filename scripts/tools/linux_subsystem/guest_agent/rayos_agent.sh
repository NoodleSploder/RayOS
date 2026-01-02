#!/bin/sh

# RayOS guest agent (Stage 0)
# - Prints a stable readiness marker over serial
# - Accepts simple line-based commands over serial

set -u

exec </dev/console >/dev/console 2>/dev/console

echo "RAYOS_LINUX_AGENT_READY"

MODE="${RAYOS_AGENT_MODE:-}"

validate_ascii_payload() {
  # Usage: validate_ascii_payload <s> <maxlen>
  s="$1"
  maxlen="$2"
  # Length check.
  if [ "${#s}" -gt "$maxlen" ]; then
    return 1
  fi
  # Strict ASCII printable plus space.
  # shellcheck disable=SC2018,SC2019
  printf '%s' "$s" | LC_ALL=C /bin/busybox grep -Eq '^[ -~]+$'
}

desktop_launch_app() {
  app="$1"

  # Minimal allowlist for now.
  case "$app" in
    weston-terminal)
      ;;
    *)
      echo "RAYOS_LINUX_APP_LAUNCH_ERR reason=not_allowed name=$app"
      return 1
      ;;
  esac

  rootfs="${RAYOS_DESKTOP_ROOTFS:-}"
  xdg="${RAYOS_DESKTOP_XDG_RUNTIME_DIR:-/tmp/xdg}"
  wdisp="${RAYOS_DESKTOP_WAYLAND_DISPLAY:-wayland-0}"

  if [ -z "$rootfs" ] || [ ! -d "$rootfs" ]; then
    echo "RAYOS_LINUX_APP_LAUNCH_ERR reason=no_rootfs name=$app"
    return 1
  fi
  if [ ! -d "$rootfs/tmp" ]; then
    echo "RAYOS_LINUX_APP_LAUNCH_ERR reason=rootfs_tmp_missing name=$app"
    return 1
  fi

  # Launch inside the chroot so the app sees the real rootfs.
  # Use a tiny log file for debugging.
  ts="$(date +%s 2>/dev/null || echo 0)"
  /bin/busybox chroot "$rootfs" /bin/sh -lc "export XDG_RUNTIME_DIR=$xdg; export WAYLAND_DISPLAY=$wdisp; $app >/tmp/rayos-launch-$app-$ts.log 2>&1 &" >/dev/null 2>&1
  rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "RAYOS_LINUX_APP_LAUNCH_OK name=$app"
    return 0
  fi
  echo "RAYOS_LINUX_APP_LAUNCH_ERR reason=launch_failed name=$app rc=$rc"
  return 1
}

emit_ppm_p3() {
  # Usage: emit_ppm_p3 <w> <h> <seed>
  w="$1"
  h="$2"
  seed="$3"

  echo "P3"
  echo "${w} ${h}"
  echo "255"

  y=0
  while [ "$y" -lt "$h" ]; do
    x=0
    while [ "$x" -lt "$w" ]; do
      # Simple deterministic gradient.
      r=$(( 255 * x / (w - 1) ))
      g=$(( 255 * y / (h - 1) ))
      b=$(( 255 * (x + y + seed) / (w + h - 1) ))
      printf '%d %d %d ' "$r" "$g" "$b"
      x=$((x + 1))
    done
    printf '\n'
    y=$((y + 1))
  done
}

while true; do
  if ! IFS= read -r line; then
    sleep 0.05
    continue
  fi

  case "$line" in
    PING)
      echo "RAYOS_LINUX_AGENT_PONG"
      ;;
    SHUTDOWN)
      echo "RAYOS_LINUX_AGENT_SHUTDOWN_ACK"
      poweroff -f 2>/dev/null || reboot -f 2>/dev/null || true
      ;;
    SHELL)
      # Escape hatch for manual debugging.
      echo "RAYOS_LINUX_AGENT_SHELL"
      exec /bin/sh
      ;;
    LAUNCH_APP:*)
      if [ "$MODE" != "desktop" ]; then
        echo "RAYOS_LINUX_APP_LAUNCH_ERR reason=not_in_desktop_mode"
        continue
      fi
      app="${line#LAUNCH_APP:}"
      if [ -z "$app" ]; then
        echo "RAYOS_LINUX_APP_LAUNCH_ERR reason=empty"
        continue
      fi
      if ! validate_ascii_payload "$app" 64; then
        echo "RAYOS_LINUX_APP_LAUNCH_ERR reason=invalid_ascii_or_length"
        continue
      fi
      desktop_launch_app "$app"
      ;;
    SURFACE_TEST)
      # Deterministic single-surface output (PPM P3) over serial.
      # This is the initial "embedded desktop surface" transport prototype.
      # Host side can extract between BEGIN/END markers.
      W=32
      H=32
      echo "RAYOS_LINUX_EMBED_SURFACE_BEGIN id=1 format=ppm_p3 w=${W} h=${H}"
      emit_ppm_p3 "$W" "$H" 0
      echo "RAYOS_LINUX_EMBED_SURFACE_END id=1"
      ;;
    SURFACE_MULTI_TEST)
      # Deterministic multi-surface output (two independent surfaces) over serial.
      # This is Step 5 scaffolding: multi-window/multi-surface mapping without real Wayland forwarding yet.
      emit_surface() {
        sid="$1"
        title="$2"
        w="$3"
        h="$4"
        echo "RAYOS_LINUX_SURFACE_CREATE id=${sid} role=toplevel title=${title} format=ppm_p3 w=${w} h=${h}"
        echo "RAYOS_LINUX_SURFACE_FRAME_BEGIN id=${sid} seq=1"
        emit_ppm_p3 "$w" "$h" "$sid"
        echo "RAYOS_LINUX_SURFACE_FRAME_END id=${sid} seq=1"
      }

      emit_surface 1 SurfaceA 32 24
      emit_surface 2 SurfaceB 24 32
      echo "RAYOS_LINUX_SURFACE_MULTI_END"
      ;;
    SURFACE_LIFECYCLE_TEST)
      # Surface lifecycle + geometry test.
      # - create two surfaces
      # - configure geometry
      # - emit a frame for each
      # - destroy one surface
      # - reconfigure the remaining surface
      emit_surface() {
        sid="$1"
        title="$2"
        w="$3"
        h="$4"
        echo "RAYOS_LINUX_SURFACE_CREATE id=${sid} role=toplevel title=${title} format=ppm_p3 w=${w} h=${h}"
      }
      emit_configure() {
        sid="$1"
        x="$2"
        y="$3"
        w="$4"
        h="$5"
        echo "RAYOS_LINUX_SURFACE_CONFIGURE id=${sid} x=${x} y=${y} w=${w} h=${h}"
      }
      emit_frame() {
        sid="$1"
        w="$2"
        h="$3"
        seq="$4"
        echo "RAYOS_LINUX_SURFACE_FRAME_BEGIN id=${sid} seq=${seq}"
        emit_ppm_p3 "$w" "$h" "$sid"
        echo "RAYOS_LINUX_SURFACE_FRAME_END id=${sid} seq=${seq}"
      }

      emit_surface 1 SurfaceA 32 24
      emit_surface 2 SurfaceB 24 32

      emit_configure 1 10 20 320 240
      emit_configure 2 40 60 240 320

      emit_frame 1 32 24 1
      emit_frame 2 24 32 1

      echo "RAYOS_LINUX_SURFACE_DESTROY id=1 reason=test"

      emit_configure 2 0 0 800 600
      echo "RAYOS_LINUX_SURFACE_LIFECYCLE_END"
      ;;
    SURFACE_FOCUS_ROLE_TEST)
      # Focus + role test.
      # - create two surfaces
      # - change roles
      # - toggle focus between them
      # - end marker used by host tests
      echo "RAYOS_LINUX_SURFACE_CREATE id=1 role=toplevel title=Alpha format=ppm_p3 w=16 h=16"
      echo "RAYOS_LINUX_SURFACE_CREATE id=2 role=toplevel title=Beta format=ppm_p3 w=16 h=16"

      echo "RAYOS_LINUX_SURFACE_ROLE id=2 role=popup"
      echo "RAYOS_LINUX_SURFACE_ROLE id=1 role=toplevel"

      echo "RAYOS_LINUX_SURFACE_FOCUS id=2 focused=1"
      echo "RAYOS_LINUX_SURFACE_FOCUS id=1 focused=1"
      echo "RAYOS_LINUX_SURFACE_FOCUS id=1 focused=0"
      echo "RAYOS_LINUX_SURFACE_FOCUS id=2 focused=1"

      echo "RAYOS_LINUX_SURFACE_FOCUS_ROLE_END"
      ;;
    SURFACE_TREE_TEST)
      # Parent/child window tree + state flags test.
      # - create a toplevel window
      # - create a popup window
      # - declare popup parent
      # - set states (modal/popup/maximized)
      # - focus popup then toplevel
      echo "RAYOS_LINUX_SURFACE_CREATE id=10 role=toplevel title=Main format=ppm_p3 w=20 h=12"
      echo "RAYOS_LINUX_SURFACE_CREATE id=11 role=popup title=Menu format=ppm_p3 w=12 h=8"

      echo "RAYOS_LINUX_SURFACE_PARENT id=11 parent=10"
      echo "RAYOS_LINUX_SURFACE_STATE id=10 states=maximized"
      echo "RAYOS_LINUX_SURFACE_STATE id=11 states=popup,modal"

      echo "RAYOS_LINUX_SURFACE_FOCUS id=11 focused=1"
      echo "RAYOS_LINUX_SURFACE_FOCUS id=10 focused=1"

      echo "RAYOS_LINUX_SURFACE_TREE_END"
      ;;
    *)
      # Keep it intentionally minimal; unknown commands are reported.
      echo "RAYOS_LINUX_AGENT_ERR unknown_command"
      ;;
  esac

done
