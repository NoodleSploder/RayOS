#!/usr/bin/env python3
"""UDP gaze bridge for RayOS Cortex.

Sends gaze points to Cortex over UDP so hardware eye trackers can be integrated
without linking proprietary SDKs into the Rust crate.

Cortex side:
  export RAYOS_GAZE_UDP_ADDR=127.0.0.1:5555

This tool can emit JSON datagrams:
  {"x":0.5,"y":0.5,"confidence":1.0,"timestamp":123}

Sources:
  - stdin: read gaze lines from stdin and forward
  - mouse: poll X11 mouse cursor via xdotool and forward as gaze (useful for demo)

Note: The UDP payload format is intentionally simple; adapt your tracker bridge to
output JSON or tokens (x=.. y=.. conf=.. ts=..).
"""

from __future__ import annotations

import argparse
import json
import socket
import subprocess
import sys
import time
from typing import Optional, Tuple


def now_ms() -> int:
    return int(time.time() * 1000)


def parse_dest(dest: str) -> Tuple[str, int]:
    if ":" not in dest:
        raise ValueError("dest must be host:port")
    host, port_s = dest.rsplit(":", 1)
    return host, int(port_s)


def get_screen_size_fallback() -> Tuple[int, int]:
    # Try xrandr first
    try:
        out = subprocess.check_output(["xrandr", "--current"], text=True, stderr=subprocess.DEVNULL)
        for line in out.splitlines():
            if " current " in line:
                # Example: Screen 0: minimum 8 x 8, current 2560 x 1440, maximum ...
                parts = line.split(" current ", 1)[1]
                dims = parts.split(",", 1)[0].strip()
                w_s, _, h_s = dims.partition(" x ")
                return int(w_s.strip()), int(h_s.strip())
    except Exception:
        pass

    # Try xdpyinfo
    try:
        out = subprocess.check_output(["xdpyinfo"], text=True, stderr=subprocess.DEVNULL)
        for line in out.splitlines():
            line = line.strip()
            if line.startswith("dimensions:"):
                # dimensions:    2560x1440 pixels
                dims = line.split()[1]
                w_s, h_s = dims.split("x", 1)
                return int(w_s), int(h_s)
    except Exception:
        pass

    return 1920, 1080


def xdotool_mouse_xy() -> Optional[Tuple[int, int]]:
    try:
        out = subprocess.check_output(
            ["xdotool", "getmouselocation", "--shell"],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except Exception:
        return None

    x = None
    y = None
    for line in out.splitlines():
        if "=" not in line:
            continue
        k, v = line.split("=", 1)
        if k == "X":
            try:
                x = int(v)
            except ValueError:
                pass
        elif k == "Y":
            try:
                y = int(v)
            except ValueError:
                pass

    if x is None or y is None:
        return None
    return x, y


def send_json(sock: socket.socket, dest: Tuple[str, int], x: float, y: float, confidence: float) -> None:
    payload = {
        "x": max(0.0, min(1.0, x)),
        "y": max(0.0, min(1.0, y)),
        "confidence": max(0.0, min(1.0, confidence)),
        "timestamp": now_ms(),
    }
    data = json.dumps(payload, separators=(",", ":")).encode("utf-8")
    sock.sendto(data, dest)


def run_stdin(sock: socket.socket, dest: Tuple[str, int], *, echo: bool) -> int:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        if echo:
            print(line)
        sock.sendto(line.encode("utf-8"), dest)
    return 0


def run_mouse(sock: socket.socket, dest: Tuple[str, int], *, hz: float, echo: bool) -> int:
    w, h = get_screen_size_fallback()
    if echo:
        print(f"screen={w}x{h}", file=sys.stderr)

    period = 1.0 / max(1.0, hz)
    while True:
        xy = xdotool_mouse_xy()
        if xy is None:
            print("ERROR: xdotool not available or not running under X11", file=sys.stderr)
            return 2

        px, py = xy
        x = px / float(w)
        y = py / float(h)
        send_json(sock, dest, x, y, 1.0)
        if echo:
            print(f"{{\"x\":{x:.4f},\"y\":{y:.4f}}}")
        time.sleep(period)


def main() -> int:
    ap = argparse.ArgumentParser(description="RayOS UDP gaze bridge")
    ap.add_argument("--dest", default="127.0.0.1:5555", help="Destination host:port (Cortex UDP bind)")
    ap.add_argument("--source", choices=["stdin", "mouse"], default="stdin")
    ap.add_argument("--hz", type=float, default=60.0, help="Polling rate for --source mouse")
    ap.add_argument("--echo", action="store_true", help="Echo payloads to stdout")

    args = ap.parse_args()

    dest = parse_dest(args.dest)
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    if args.source == "stdin":
        return run_stdin(sock, dest, echo=args.echo)
    if args.source == "mouse":
        return run_mouse(sock, dest, hz=args.hz, echo=args.echo)

    return 1


if __name__ == "__main__":
    raise SystemExit(main())
