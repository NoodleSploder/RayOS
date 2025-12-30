#!/usr/bin/env python3

import hashlib
import os
import re
import sys
from pathlib import Path


BEGIN_RE = re.compile(r"^RAYOS_LINUX_EMBED_SURFACE_BEGIN\b")
END_RE = re.compile(r"^RAYOS_LINUX_EMBED_SURFACE_END\b")


def extract(*, log_path: Path, out_path: Path) -> str:
    data_lines: list[str] = []
    in_block = False

    with log_path.open("r", encoding="utf-8", errors="replace") as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not in_block:
                if BEGIN_RE.search(line):
                    in_block = True
                continue

            if END_RE.search(line):
                break

            data_lines.append(line)

    if not data_lines:
        raise RuntimeError("No embedded surface data found between BEGIN/END markers")

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_text = "\n".join(data_lines) + "\n"
    out_path.write_text(out_text, encoding="utf-8")

    sha = hashlib.sha256(out_text.encode("utf-8")).hexdigest()
    return sha


def main() -> int:
    log_file = os.environ.get("LOG_FILE", "").strip()
    out_file = os.environ.get("OUT_FILE", "").strip()

    if not log_file or not out_file:
        sys.stderr.write("Set LOG_FILE and OUT_FILE env vars\n")
        return 2

    log_path = Path(log_file)
    out_path = Path(out_file)

    if not log_path.is_file():
        sys.stderr.write(f"Log file not found: {log_path}\n")
        return 2

    sha = extract(log_path=log_path, out_path=out_path)
    print(sha)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
