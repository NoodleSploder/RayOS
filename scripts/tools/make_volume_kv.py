#!/usr/bin/env python3
import struct
import sys
from pathlib import Path

MAGIC = b"RAYOSVOL"
VERSION = 1


def align_up4(n: int) -> int:
    return (n + 3) & ~3


def encode_ascii(s: str) -> bytes:
    # Keep it simple/deterministic for early boot parsing.
    try:
        b = s.encode("ascii")
    except UnicodeEncodeError:
        raise SystemExit(f"Non-ASCII input not supported: {s!r}")
    if any(c < 0x20 or c > 0x7E for c in b):
        raise SystemExit(f"Non-printable ASCII not supported: {s!r}")
    return b


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        print(
            "Usage: make_volume_kv.py OUT.bin key=value [key=value ...]\n"
            "Format: RAYOSVOL v1 (minimal KV table for bring-up)",
            file=sys.stderr,
        )
        return 2

    out_path = Path(argv[1])

    pairs: list[tuple[bytes, bytes]] = []
    for item in argv[2:]:
        if "=" not in item:
            raise SystemExit(f"Bad pair (expected key=value): {item!r}")
        k, v = item.split("=", 1)
        kb = encode_ascii(k)
        vb = encode_ascii(v)
        if len(kb) > 0xFFFF or len(vb) > 0xFFFF:
            raise SystemExit("key/value too large (max 65535 bytes)")
        pairs.append((kb, vb))

    buf = bytearray()
    buf += MAGIC
    buf += struct.pack("<I", VERSION)
    buf += struct.pack("<I", len(pairs))

    for kb, vb in pairs:
        buf += struct.pack("<HHI", len(kb), len(vb), 0)
        buf += kb
        buf += vb
        while len(buf) % 4 != 0:
            buf += b"\x00"

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(buf)
    print(f"wrote {out_path} ({len(buf)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
