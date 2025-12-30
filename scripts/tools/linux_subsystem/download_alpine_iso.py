#!/usr/bin/env python3

import os
from pathlib import Path
from urllib.request import urlopen


def download(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.is_file() and dest.stat().st_size > 0:
        return
    with urlopen(url, timeout=120) as resp:
        if resp.status != 200:
            raise RuntimeError(f"download failed: HTTP {resp.status} for {url}")
        data = resp.read()
        if not data:
            raise RuntimeError(f"download returned empty body: {url}")
        dest.write_bytes(data)


def main() -> int:
    work_dir = Path(os.environ.get("WORK_DIR", "./build"))
    version = os.environ.get("ALPINE_ISO_VERSION", "3.20.3")
    base = os.environ.get(
        "ALPINE_ISO_BASE_URL",
        "https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/x86_64",
    ).rstrip("/")

    iso_name = f"alpine-virt-{version}-x86_64.iso"
    url = f"{base}/{iso_name}"

    dest = work_dir / "linux-guest" / "alpine-iso" / version / iso_name
    download(url, dest)
    print(dest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
