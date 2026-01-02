#!/usr/bin/env python3
"""Send a text line to a running QEMU HMP monitor socket via `sendkey`.

Usage:
  ./qemu-sendtext.py --sock /path/to/monitor.sock --text "search boot" [--quit]

Notes:
- Designed for RayOS tests where the guest reads keyboard input.
- Uses a conservative US-layout mapping for common punctuation.
"""

import argparse
import os
import socket
import sys
import time


def wait_for_sock(path: str, timeout_s: float) -> None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if os.path.exists(path):
            return
        time.sleep(0.05)
    raise SystemExit(f"monitor socket not found: {path}")


SHIFTED = {
    "_": "shift-minus",
    "+": "shift-equal",
    ":": "shift-semicolon",
    "?": "shift-slash",
    "!": "shift-1",
    "@": "shift-2",
    "#": "shift-3",
    "$": "shift-4",
    "%": "shift-5",
    "^": "shift-6",
    "&": "shift-7",
    "*": "shift-8",
    "(": "shift-9",
    ")": "shift-0",
    '"': "shift-apostrophe",
    "<": "shift-comma",
    ">": "shift-dot",
    "{": "shift-leftbracket",
    "}": "shift-rightbracket",
}

PLAIN = {
    " ": "spc",
    "-": "minus",
    "=": "equal",
    ";": "semicolon",
    "'": "apostrophe",
    ",": "comma",
    ".": "dot",
    "/": "slash",
    "\\": "backslash",
    "[": "leftbracket",
    "]": "rightbracket",
}


def char_to_key(ch: str) -> str:
    if ch == "\n":
        return "ret"
    if ch in PLAIN:
        return PLAIN[ch]
    if ch in SHIFTED:
        return SHIFTED[ch]
    if "0" <= ch <= "9":
        return ch
    if "a" <= ch <= "z":
        return ch
    if "A" <= ch <= "Z":
        return f"shift-{ch.lower()}"
    raise SystemExit(f"unsupported character for sendkey: {ch!r}")


def to_sendkey(text: str, *, enter: bool = True) -> list[str]:
    cmds: list[str] = []
    for ch in text:
        key = char_to_key(ch)
        if key == "ret":
            cmds.append("sendkey ret")
        else:
            cmds.append(f"sendkey {key}")
    if enter:
        cmds.append("sendkey ret")
    return cmds


def send_cmds(sock_path: str, cmds: list[str]) -> None:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(1.0)
    s.connect(sock_path)

    # Drain banner/prompt.
    try:
        s.settimeout(0.2)
        s.recv(4096)
    except Exception:
        pass

    for cmd in cmds:
        s.sendall((cmd + "\r\n").encode("ascii"))
        time.sleep(0.03)

        # Drain output to keep socket happy and detect HMP errors.
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

        text = out.decode("utf-8", errors="ignore")
        lowered = text.lower()
        if "unknown command" in lowered or "error" in lowered:
            s.close()
            raise SystemExit(f"HMP command failed: {cmd}\n{text.strip()}")

    s.close()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--sock", required=True, help="Path to QEMU HMP monitor unix socket")
    ap.add_argument("--text", required=True, help="Text to type (ASCII letters/digits/spaces)")
    ap.add_argument("--wait", type=float, default=10.0, help="Seconds to wait for socket")
    ap.add_argument("--after", type=float, default=1.5, help="Seconds to wait after typing")
    ap.add_argument("--no-enter", action="store_true", help="Do not automatically press Enter after typing")
    ap.add_argument("--quit", action="store_true", help="Also send 'quit' to the monitor")
    args = ap.parse_args()

    wait_for_sock(args.sock, args.wait)
    cmds = to_sendkey(args.text, enter=(not args.no_enter))
    send_cmds(args.sock, cmds)

    time.sleep(args.after)
    if args.quit:
        send_cmds(args.sock, ["quit"])

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
