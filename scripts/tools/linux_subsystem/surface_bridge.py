#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path


CREATE_RE = re.compile(r"^RAYOS_LINUX_SURFACE_CREATE\b(?P<kv>.*)$")
CONFIGURE_RE = re.compile(r"^RAYOS_LINUX_SURFACE_CONFIGURE\b(?P<kv>.*)$")
DESTROY_RE = re.compile(r"^RAYOS_LINUX_SURFACE_DESTROY\b(?P<kv>.*)$")
ROLE_RE = re.compile(r"^RAYOS_LINUX_SURFACE_ROLE\b(?P<kv>.*)$")
FOCUS_RE = re.compile(r"^RAYOS_LINUX_SURFACE_FOCUS\b(?P<kv>.*)$")
PARENT_RE = re.compile(r"^RAYOS_LINUX_SURFACE_PARENT\b(?P<kv>.*)$")
STATE_RE = re.compile(r"^RAYOS_LINUX_SURFACE_STATE\b(?P<kv>.*)$")
FRAME_BEGIN_RE = re.compile(r"^RAYOS_LINUX_SURFACE_FRAME_BEGIN\b(?P<kv>.*)$")
FRAME_END_RE = re.compile(r"^RAYOS_LINUX_SURFACE_FRAME_END\b(?P<kv>.*)$")


def _parse_kv_tail(tail: str) -> dict[str, str]:
    out: dict[str, str] = {}
    for part in tail.strip().split():
        if "=" not in part:
            continue
        k, v = part.split("=", 1)
        out[k.strip()] = v.strip()
    return out


@dataclass
class SurfaceInfo:
    surface_id: str
    role: str | None = None
    title: str | None = None
    format: str | None = None
    w: int | None = None
    h: int | None = None

    # Host-side window mapping scaffold.
    window_id: str | None = None

    # Window hierarchy.
    parent_surface_id: str | None = None

    # Window state flags (scaffolding only).
    states: list[str] | None = None

    # Geometry (host window model).
    x: int | None = None
    y: int | None = None

    # Updated as frames arrive.
    latest_seq: int | None = None
    latest_sha256: str | None = None
    latest_path: str | None = None


@dataclass(frozen=True)
class FrameKey:
    surface_id: str
    seq: int


class SurfaceBridge:
    """Incremental, line-based surface bridge.

    This is intentionally simple scaffolding:
    - Parses RAYOS_LINUX_SURFACE_CREATE + FRAME_BEGIN/END lines
    - Writes per-surface frame payloads to disk
    - Maintains a small window/surface registry JSON suitable for mapping into a compositor later

    It does not implement real Wayland, damage tracking, or binary pixel transport.
    """

    def __init__(self, *, out_dir: Path) -> None:
        self._out_dir = out_dir
        self._frames_dir = out_dir / "frames"
        self._frames_dir.mkdir(parents=True, exist_ok=True)

        self._surfaces: dict[str, SurfaceInfo] = {}

        # Simple window ordering + focus scaffolding.
        self._z_order: list[str] = []  # list of window_id
        self._focused_window_id: str | None = None

        self._current_key: FrameKey | None = None
        self._current_lines: list[str] = []

    def on_line(self, line: str) -> None:
        line = line.rstrip("\n")

        m = CREATE_RE.match(line)
        if m:
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            if not sid:
                return

            info = self._surfaces.get(sid)
            if info is None:
                info = SurfaceInfo(surface_id=sid)
                # For now, keep a stable 1:1 mapping.
                info.window_id = f"win-{sid}"
                self._surfaces[sid] = info
                if info.window_id not in self._z_order:
                    self._z_order.append(info.window_id)

            info.role = kv.get("role")
            info.title = kv.get("title")
            info.format = kv.get("format")
            info.w = int(kv["w"]) if "w" in kv else info.w
            info.h = int(kv["h"]) if "h" in kv else info.h

            self._write_registry()
            return

        m = ROLE_RE.match(line)
        if m:
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            if not sid:
                return
            info = self._surfaces.get(sid)
            if info is None:
                info = SurfaceInfo(surface_id=sid, window_id=f"win-{sid}")
                self._surfaces[sid] = info
                if info.window_id not in self._z_order:
                    self._z_order.append(info.window_id)
            info.role = kv.get("role") or info.role
            self._write_registry()
            return

        m = PARENT_RE.match(line)
        if m:
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            parent = kv.get("parent")
            if not sid or not parent:
                return

            child = self._surfaces.get(sid)
            if child is None:
                child = SurfaceInfo(surface_id=sid, window_id=f"win-{sid}")
                self._surfaces[sid] = child
                if child.window_id not in self._z_order:
                    self._z_order.append(child.window_id)

            parent_info = self._surfaces.get(parent)
            if parent_info is None:
                parent_info = SurfaceInfo(surface_id=parent, window_id=f"win-{parent}")
                self._surfaces[parent] = parent_info
                if parent_info.window_id not in self._z_order:
                    self._z_order.append(parent_info.window_id)

            child.parent_surface_id = parent
            self._write_registry()
            return

        m = STATE_RE.match(line)
        if m:
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            if not sid:
                return

            info = self._surfaces.get(sid)
            if info is None:
                info = SurfaceInfo(surface_id=sid, window_id=f"win-{sid}")
                self._surfaces[sid] = info
                if info.window_id not in self._z_order:
                    self._z_order.append(info.window_id)

            states_raw = kv.get("states", "")
            states = [s.strip() for s in states_raw.split(",") if s.strip()]
            info.states = states
            self._write_registry()
            return

        m = FOCUS_RE.match(line)
        if m:
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            if not sid:
                return

            info = self._surfaces.get(sid)
            if info is None:
                info = SurfaceInfo(surface_id=sid, window_id=f"win-{sid}")
                self._surfaces[sid] = info
            if info.window_id is None:
                info.window_id = f"win-{sid}"
            if info.window_id not in self._z_order:
                self._z_order.append(info.window_id)

            focused = kv.get("focused", "")
            is_focused = focused in ("1", "true", "True", "yes", "YES")
            if is_focused:
                self._focused_window_id = info.window_id
                # Bring to front.
                try:
                    self._z_order.remove(info.window_id)
                except ValueError:
                    pass
                self._z_order.append(info.window_id)
            else:
                if self._focused_window_id == info.window_id:
                    self._focused_window_id = None

            self._write_registry()
            return

        m = CONFIGURE_RE.match(line)
        if m:
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            if not sid:
                return

            info = self._surfaces.get(sid)
            if info is None:
                info = SurfaceInfo(surface_id=sid, window_id=f"win-{sid}")
                self._surfaces[sid] = info

            # Allow configure to override size/pos independent of CREATE.
            if "x" in kv:
                info.x = int(kv["x"])
            if "y" in kv:
                info.y = int(kv["y"])
            if "w" in kv:
                info.w = int(kv["w"])
            if "h" in kv:
                info.h = int(kv["h"])

            self._write_registry()
            return

        m = DESTROY_RE.match(line)
        if m:
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            if not sid:
                return

            # Drop any in-flight frame for this surface.
            if self._current_key is not None and self._current_key.surface_id == sid:
                self._current_key = None
                self._current_lines = []

            # Remove surface/window mapping.
            info = self._surfaces.pop(sid, None)
            if info is not None and info.window_id is not None:
                if self._focused_window_id == info.window_id:
                    self._focused_window_id = None
                try:
                    self._z_order.remove(info.window_id)
                except ValueError:
                    pass
            self._write_registry()
            return

        m = FRAME_BEGIN_RE.match(line)
        if m:
            self._flush_current(allow_partial=False)
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            if not sid:
                return
            seq = int(kv.get("seq", "0"))
            self._current_key = FrameKey(surface_id=sid, seq=seq)
            self._current_lines = []
            return

        m = FRAME_END_RE.match(line)
        if m:
            kv = _parse_kv_tail(m.group("kv"))
            sid = kv.get("id")
            seq = int(kv.get("seq", "0"))
            if self._current_key is None:
                return
            if self._current_key.surface_id != (sid or self._current_key.surface_id):
                return
            if self._current_key.seq != seq:
                return
            self._flush_current(allow_partial=False)
            return

        if self._current_key is not None:
            self._current_lines.append(line)

    def close(self) -> None:
        self._flush_current(allow_partial=True)
        self._write_registry()

    def _flush_current(self, *, allow_partial: bool) -> None:
        if self._current_key is None:
            return

        key = self._current_key
        lines = self._current_lines

        self._current_key = None
        self._current_lines = []

        if not lines and not allow_partial:
            return

        ppm_text = "\n".join(lines) + "\n"
        sha = hashlib.sha256(ppm_text.encode("utf-8")).hexdigest()
        out_path = self._frames_dir / f"surface-{key.surface_id}-seq-{key.seq}.ppm"
        out_path.write_text(ppm_text, encoding="utf-8")

        info = self._surfaces.get(key.surface_id)
        if info is None:
            info = SurfaceInfo(surface_id=key.surface_id, window_id=f"win-{key.surface_id}")
            self._surfaces[key.surface_id] = info

        info.latest_seq = key.seq
        info.latest_sha256 = sha
        info.latest_path = str(out_path.relative_to(self._out_dir))

        self._write_registry()

    def _write_registry(self) -> None:
        surfaces_out = {
            sid: {
                "surface_id": info.surface_id,
                "window_id": info.window_id,
                "role": info.role,
                "title": info.title,
                "format": info.format,
                "x": info.x,
                "y": info.y,
                "w": info.w,
                "h": info.h,
                "parent_surface_id": info.parent_surface_id,
                "states": info.states,
                "latest_seq": info.latest_seq,
                "latest_sha256": info.latest_sha256,
                "latest_path": info.latest_path,
            }
            for sid, info in sorted(self._surfaces.items(), key=lambda kv: int(kv[0]))
        }

        # Build window children list based on parent_surface_id.
        children_by_window: dict[str, list[str]] = {}
        for s in self._surfaces.values():
            if s.window_id is None:
                continue
            children_by_window.setdefault(s.window_id, [])
        for s in self._surfaces.values():
            if not s.parent_surface_id:
                continue
            parent = self._surfaces.get(s.parent_surface_id)
            if parent is None or parent.window_id is None or s.window_id is None:
                continue
            children_by_window.setdefault(parent.window_id, [])
            if s.window_id not in children_by_window[parent.window_id]:
                children_by_window[parent.window_id].append(s.window_id)

        window_map = {
            info.window_id: {
                "window_id": info.window_id,
                "surface_id": info.surface_id,
                "title": info.title,
                "role": info.role,
                "x": info.x,
                "y": info.y,
                "w": info.w,
                "h": info.h,
                "parent_window_id": (
                    self._surfaces[info.parent_surface_id].window_id
                    if info.parent_surface_id in self._surfaces
                    else None
                ),
                "states": info.states,
                "children": children_by_window.get(info.window_id, []),
            }
            for info in self._surfaces.values()
            if info.window_id is not None
        }

        out = {
            "surfaces": surfaces_out,
            "windows": dict(sorted(window_map.items())),
            "focused_window_id": self._focused_window_id,
            "z_order": list(self._z_order),
        }

        self._out_dir.mkdir(parents=True, exist_ok=True)
        (self._out_dir / "registry.json").write_text(
            json.dumps(out, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
