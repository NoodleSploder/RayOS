#!/usr/bin/env python3
"""Send a single QEMU HMP `sendkey` to a monitor socket.

Usage:
  ./qemu-sendkey.py --sock /path/to/monitor.sock --key "ctrl-l"

Notes:
- `--key` uses QEMU's sendkey syntax (e.g. esc, tab, ret, up, down, left, right,
  ctrl-l, alt-f4, shift-tab).
"""

import argparse
import os
import socket
import sys
import time


ALIASES = {
    "enter": "ret",
    "return": "ret",
    "escape": "esc",
    "esc": "esc",
    "tab": "tab",
    "space": "spc",
    "spc": "spc",
    "backspace": "backspace",
    "bksp": "backspace",
    "del": "delete",
    "delete": "delete",
    "pgup": "pgup",
    "pageup": "pgup",
    "pgdn": "pgdn",
    "pagedown": "pgdn",
    "home": "home",
    "end": "end",
    "up": "up",
    "down": "down",
    "left": "left",
    "right": "right",
}


def wait_for_sock(path: str, timeout_s: float) -> None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if os.path.exists(path):
            return
        time.sleep(0.05)
    raise SystemExit(f"monitor socket not found: {path}")


def normalize_key(key: str) -> str:
    k = key.strip().lower()
    if not k:
        raise SystemExit("empty key")

    # Allow common separators for combos.
    k = k.replace("+", "-")
    k = "-".join([p for p in k.split() if p])

    parts = [p for p in k.split("-") if p]
    if not parts:
        raise SystemExit("invalid key")

    norm_parts = []
    for p in parts:
        if p in ("ctrl", "alt", "shift", "meta", "win", "super"):
            # QEMU uses ctrl/alt/shift; 'meta/win/super' are sometimes accepted as 'meta'.
            if p in ("win", "super"):
                norm_parts.append("meta")
            else:
                norm_parts.append(p)
            continue
        if p in ALIASES:
            norm_parts.append(ALIASES[p])
            continue
        if len(p) == 1 and ("a" <= p <= "z" or "0" <= p <= "9"):
            norm_parts.append(p)
            continue
        if p.startswith("f") and p[1:].isdigit():
            n = int(p[1:])
            if 1 <= n <= 12:
                norm_parts.append(f"f{n}")
                continue
        # Pass through unknown parts as-is; QEMU will error if unsupported.
        norm_parts.append(p)

    return "-".join(norm_parts)


def send_cmd(sock_path: str, cmd: str) -> None:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(1.0)
    s.connect(sock_path)

    # Drain banner/prompt.
    try:
        s.settimeout(0.2)
        s.recv(4096)
    except Exception:
        pass

    s.sendall((cmd + "\r\n").encode("ascii"))
    time.sleep(0.03)

    # Drain output and attempt to detect HMP errors (unknown command, etc).
    out = b""
    try:
        s.settimeout(0.2)
        while True:
            data = s.recv(4096)
            if not data:
                break
            out += data
            if b"(qemu)" in out:
                break
    except Exception:
        pass
    finally:
        s.close()

    text = out.decode("utf-8", errors="ignore")
    lowered = text.lower()
    if "unknown command" in lowered or "error" in lowered:
        raise SystemExit(f"HMP command failed: {cmd}\n{text.strip()}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--sock", required=True)
    ap.add_argument("--key", required=True, help="Key spec, e.g. esc, tab, ret, ctrl-l")
    ap.add_argument("--wait", type=float, default=10.0)
    ap.add_argument("--after", type=float, default=0.05)
    args = ap.parse_args()

    wait_for_sock(args.sock, args.wait)
    key = normalize_key(args.key)
    send_cmd(args.sock, f"sendkey {key}")
    time.sleep(args.after)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
