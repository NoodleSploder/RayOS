#!/usr/bin/env python3

import hashlib
import json
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path


CREATE_RE = re.compile(
    r"^RAYOS_LINUX_SURFACE_CREATE\b(?P<kv>.*)$"
)
FRAME_BEGIN_RE = re.compile(
    r"^RAYOS_LINUX_SURFACE_FRAME_BEGIN\b(?P<kv>.*)$"
)
FRAME_END_RE = re.compile(
    r"^RAYOS_LINUX_SURFACE_FRAME_END\b(?P<kv>.*)$"
)


def _parse_kv_tail(tail: str) -> dict[str, str]:
    out: dict[str, str] = {}
    for part in tail.strip().split():
        if "=" not in part:
            continue
        k, v = part.split("=", 1)
        out[k.strip()] = v.strip()
    return out


@dataclass
class SurfaceMeta:
    id: str
    role: str | None = None
    title: str | None = None
    format: str | None = None
    w: int | None = None
    h: int | None = None


@dataclass(frozen=True)
class FrameKey:
    id: str
    seq: str


def extract(*, log_path: Path, out_dir: Path) -> dict:
    surfaces: dict[str, SurfaceMeta] = {}
    frames: dict[FrameKey, list[str]] = {}

    current: FrameKey | None = None
    current_lines: list[str] = []

    def flush_current() -> None:
        nonlocal current, current_lines
        if current is None:
            return
        frames[current] = list(current_lines)
        current = None
        current_lines = []

    with log_path.open("r", encoding="utf-8", errors="replace") as f:
        for raw in f:
            line = raw.rstrip("\n")

            m = CREATE_RE.match(line)
            if m:
                kv = _parse_kv_tail(m.group("kv"))
                sid = kv.get("id")
                if sid:
                    surfaces[sid] = SurfaceMeta(
                        id=sid,
                        role=kv.get("role"),
                        title=kv.get("title"),
                        format=kv.get("format"),
                        w=int(kv["w"]) if "w" in kv else None,
                        h=int(kv["h"]) if "h" in kv else None,
                    )
                continue

            m = FRAME_BEGIN_RE.match(line)
            if m:
                flush_current()
                kv = _parse_kv_tail(m.group("kv"))
                sid = kv.get("id")
                seq = kv.get("seq", "0")
                if sid is None:
                    continue
                current = FrameKey(id=sid, seq=seq)
                current_lines = []
                continue

            m = FRAME_END_RE.match(line)
            if m:
                kv = _parse_kv_tail(m.group("kv"))
                sid = kv.get("id")
                seq = kv.get("seq", "0")
                if current is not None and current.id == (sid or current.id) and current.seq == seq:
                    flush_current()
                continue

            if current is not None:
                current_lines.append(line)

    flush_current()

    if not frames:
        raise RuntimeError("No multi-surface frames found (no FRAME_BEGIN/END pairs)")

    out_dir.mkdir(parents=True, exist_ok=True)

    sha_by_surface: dict[str, dict[str, str]] = {}
    for key, lines in sorted(frames.items(), key=lambda kv: (int(kv[0].id), int(kv[0].seq))):
        ppm_text = "\n".join(lines) + "\n"
        ppm_path = out_dir / f"surface-{key.id}-seq-{key.seq}.ppm"
        ppm_path.write_text(ppm_text, encoding="utf-8")
        sha = hashlib.sha256(ppm_text.encode("utf-8")).hexdigest()
        sha_by_surface.setdefault(key.id, {})[key.seq] = sha

    meta_out = {
        "surfaces": {
            sid: {
                "id": meta.id,
                "role": meta.role,
                "title": meta.title,
                "format": meta.format,
                "w": meta.w,
                "h": meta.h,
            }
            for sid, meta in sorted(surfaces.items(), key=lambda kv: int(kv[0]))
        },
        "frames": sha_by_surface,
    }
    (out_dir / "surfaces.json").write_text(json.dumps(meta_out, indent=2, sort_keys=True) + "\n")
    return meta_out


def main() -> int:
    log_file = os.environ.get("LOG_FILE", "").strip()
    out_dir = os.environ.get("OUT_DIR", "").strip()

    if not log_file or not out_dir:
        sys.stderr.write("Set LOG_FILE and OUT_DIR env vars\n")
        return 2

    log_path = Path(log_file)
    if not log_path.is_file():
        sys.stderr.write(f"Log file not found: {log_path}\n")
        return 2

    out_path = Path(out_dir)
    meta = extract(log_path=log_path, out_dir=out_path)
    print(json.dumps(meta, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
