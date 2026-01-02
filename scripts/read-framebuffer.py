#!/usr/bin/env python3

import socket
import struct
import argparse
import os

def read_exact(s, n):
    data = b''
    while len(data) < n:
        chunk = s.recv(n - len(data))
        if not chunk:
            raise ConnectionError("Socket closed unexpectedly")
        data += chunk
    return data

def rfb_handshake(s):
    # ProtocolVersion
    version = read_exact(s, 12)
    print(f"Server version: {version.decode().strip()}")
    s.sendall(version)

    # Security
    num_sec_types = struct.unpack('>B', read_exact(s, 1))[0]
    sec_types = read_exact(s, num_sec_types)
    print(f"Supported security types: {sec_types}")

    # We only support the "None" security type (1)
    if 1 not in sec_types:
        raise ValueError("Server does not support 'None' security type")

    s.sendall(struct.pack('>B', 1))

    # SecurityResult
    result = struct.unpack('>I', read_exact(s, 4))[0]
    if result != 0:
        raise ConnectionError("Security handshake failed")

    # ClientInit
    s.sendall(struct.pack('>B', 1)) # Share desktop

    # ServerInit
    width, height, bpp, depth, big_endian, true_color, rmax, gmax, bmax, rshift, gshift, bshift, _, _, _ = struct.unpack('>HHBBBBHHHBBBxxx', read_exact(s, 24))
    print(f"Framebuffer dimensions: {width}x{height}")
    print(f"BPP: {bpp}, Depth: {depth}")
    print(f"Big endian: {big_endian}, True color: {true_color}")
    print(f"Red mask: {rmax}<<{rshift}, Green mask: {gmax}<<{gshift}, Blue mask: {bmax}<<{bshift}")

    name_len = struct.unpack('>I', read_exact(s, 4))[0]
    name = read_exact(s, name_len).decode()
    print(f"Desktop name: {name}")

    return width, height, bpp

def _connect_vnc(target: str) -> socket.socket:
    # Accept either:
    # - Unix socket path (existing file)
    # - TCP endpoint: host:port
    # - VNC display: host:display (port = 5900 + display)
    if os.path.exists(target):
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.connect(target)
        return s

    if ':' not in target:
        raise ValueError(f"VNC target not found as file and not in host:port form: {target}")

    host, tail = target.rsplit(':', 1)
    host = host.strip() or '127.0.0.1'
    tail = tail.strip()
    if not tail.isdigit():
        raise ValueError(f"Invalid VNC target port/display: {target}")
    n = int(tail)
    if n < 100:
        port = 5900 + n
    else:
        port = n

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2.0)
    s.connect((host, port))
    return s


def read_framebuffer(sock_target, output_path, raw):
    s = _connect_vnc(sock_target)

    width, height, bpp = rfb_handshake(s)

    # For now, assume raw encoding and 32bpp
    if bpp != 32:
        raise NotImplementedError("Only 32bpp is supported for now")

    # Send FramebufferUpdateRequest
    s.sendall(struct.pack('>BBHHHH', 3, 0, 0, 0, width, height))

    # Receive FramebufferUpdate
    msg_type = struct.unpack('>B', read_exact(s, 1))[0]
    if msg_type != 0:
        raise ValueError(f"Expected FramebufferUpdate message (0), got {msg_type}")

    _ = read_exact(s, 1) # padding

    num_rects = struct.unpack('>H', read_exact(s, 2))[0]
    print(f"Number of rectangles: {num_rects}")

    if num_rects != 1:
        raise NotImplementedError("Only single rectangle updates are supported")

    x, y, w, h, encoding = struct.unpack('>HHHHi', read_exact(s, 12))
    print(f"Rectangle: at ({x},{y}), size {w}x{h}, encoding {encoding}")

    if encoding != 0:
        raise NotImplementedError(f"Only raw encoding (0) is supported, got {encoding}")

    pixel_data = read_exact(s, w * h * (bpp // 8))

    if raw:
        with open(output_path, 'wb') as f:
            f.write(pixel_data)
    else:
        # Save as PPM
        with open(output_path, 'w') as f:
            f.write(f"P6\n{w} {h}\n255\n")
            for i in range(w * h):
                pixel = struct.unpack_from('>I', pixel_data, i * 4)[0]
                r = (pixel >> 24) & 0xFF
                g = (pixel >> 16) & 0xFF
                b = (pixel >> 8) & 0xFF
                f.write(f"{chr(r)}{chr(g)}{chr(b)}")

    print(f"Framebuffer saved to {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Read a framebuffer from a VNC server.")
    parser.add_argument(
        "--sock",
        required=True,
        help="VNC target: unix socket path OR host:port OR host:display (e.g. 127.0.0.1:0).",
    )
    parser.add_argument("--out", required=True, help="Path to save the image.")
    parser.add_argument("--raw", action="store_true", help="Output raw pixel data.")
    args = parser.parse_args()

    read_framebuffer(args.sock, args.out, args.raw)
