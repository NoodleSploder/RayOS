#!/usr/bin/env python3
"""Generate a tiny character-level bigram model.bin for RayOS local LLM.

This is intentionally small and fast to generate, and exists to prove the
"model inference" pipeline works end-to-end inside the guest.

Format (little-endian):
- TinyLmHeader (repr(C) in kernel):
  magic[8] = b"RAYTLM01"
  version  = u32 (1)
  vocab    = u32 (95)  # printable ASCII 0x20..0x7E
  ctx      = u32 (64)
  top_k    = u32 (8)
  rows     = u32 (95)
  cols     = u32 (95)
  reserved = 2*u32 (0)
- table: rows*cols u16 weights (transition weights)

Training:
- counts transitions between printable characters in the provided corpus files
- adds +1 smoothing everywhere so sampling never gets stuck

Usage:
    scripts/tools/gen_tinylm_model.py -o model.bin docs/README.md docs/QUICKSTART.md
"""

from __future__ import annotations

import argparse
import os
import struct
from pathlib import Path

MAGIC = b"RAYTLM01"
VOCAB = 95


def tok(ch: int) -> int | None:
    if 0x20 <= ch <= 0x7E:
        return ch - 0x20
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("-o", "--out", required=True, help="Output model.bin path")
    ap.add_argument("files", nargs="+", help="Corpus files")
    ap.add_argument("--ctx", type=int, default=64)
    ap.add_argument("--top-k", type=int, default=8)
    args = ap.parse_args()

    table = [[1 for _ in range(VOCAB)] for _ in range(VOCAB)]  # +1 smoothing

    for fp in args.files:
        p = Path(fp)
        if not p.exists():
            raise SystemExit(f"corpus file not found: {fp}")
        data = p.read_bytes()

        prev = None
        for b in data:
            t = tok(b)
            if t is None:
                prev = None
                continue
            if prev is not None:
                table[prev][t] = min(0xFFFF, table[prev][t] + 1)
            prev = t

    header = struct.pack(
        "<8sIIIIIIII",
        MAGIC,
        1,
        VOCAB,
        int(args.ctx),
        int(args.top_k),
        VOCAB,
        VOCAB,
        0,
        0,
    )

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    with out_path.open("wb") as f:
        f.write(header)
        for r in range(VOCAB):
            for c in range(VOCAB):
                f.write(struct.pack("<H", table[r][c]))

    size = out_path.stat().st_size
    print(f"wrote {out_path} ({size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
