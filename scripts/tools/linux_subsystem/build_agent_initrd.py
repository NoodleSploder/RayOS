#!/usr/bin/env python3

import os
import shutil
import subprocess
import gzip
from pathlib import Path


def build_agent_overlay_cpio(*, src_dir: Path, out_cpio: Path) -> None:
    if shutil.which("cpio") is None:
        raise RuntimeError("cpio is required to build the agent overlay (install 'cpio')")

    # Create a newc cpio archive containing the overlay files.
    out_cpio.parent.mkdir(parents=True, exist_ok=True)

    # Ensure executable bits are preserved.
    for rel in ["rayos_init", "rayos_agent.sh", "rayos_desktop_init"]:
        p = src_dir / rel
        if not p.is_file():
            raise FileNotFoundError(f"missing overlay file: {p}")

    with out_cpio.open("wb") as fp:
        # Put files at initramfs root by archiving them without a directory prefix.
        subprocess.run(
            [
                "bash",
                "-lc",
                "printf 'rayos_init\nrayos_agent.sh\nrayos_desktop_init\n' | cpio -o -H newc",
            ],
            check=True,
            stdout=fp,
            stderr=subprocess.DEVNULL,
            cwd=str(src_dir),
        )


def build_combined_initrd(*, base_initrd: Path, overlay_cpio: Path, out_initrd: Path) -> None:
    if not base_initrd.is_file():
        raise FileNotFoundError(f"base initrd not found: {base_initrd}")
    if not overlay_cpio.is_file():
        raise FileNotFoundError(f"overlay cpio not found: {overlay_cpio}")

    out_initrd.parent.mkdir(parents=True, exist_ok=True)

    base_bytes = base_initrd.read_bytes()
    overlay_bytes = overlay_cpio.read_bytes()

    # Match the base initrd compression format.
    # Alpine netboot initramfs is gzip; gzip supports concatenated members.
    if base_bytes.startswith(b"\x1f\x8b\x08"):
        overlay_comp = gzip.compress(overlay_bytes, compresslevel=9)
    # Uncompressed 'newc' cpio magic (070701/070702) can be concatenated.
    elif base_bytes.startswith(b"070701") or base_bytes.startswith(b"070702"):
        overlay_comp = overlay_bytes
    else:
        raise RuntimeError(
            "Unsupported base initrd format for overlaying. "
            "Set LINUX_GUEST_KIND=host or provide a gzip initrd."
        )

    with out_initrd.open("wb") as out:
        out.write(base_bytes)
        out.write(overlay_comp)


def main() -> int:
    base = os.environ.get("BASE_INITRD", "").strip()
    out = os.environ.get("OUT_INITRD", "").strip()
    src = os.environ.get("AGENT_SRC_DIR", "").strip()

    if not base or not out or not src:
        raise SystemExit(
            "Set BASE_INITRD, OUT_INITRD, and AGENT_SRC_DIR env vars. "
            "Example: BASE_INITRD=... OUT_INITRD=... AGENT_SRC_DIR=..."
        )

    base_initrd = Path(base)
    out_initrd = Path(out)
    src_dir = Path(src)

    overlay_cpio = out_initrd.with_suffix(out_initrd.suffix + ".overlay.cpio")

    # Make sure scripts are executable inside the overlay.
    (src_dir / "rayos_init").chmod(0o755)
    (src_dir / "rayos_agent.sh").chmod(0o755)

    build_agent_overlay_cpio(src_dir=src_dir, out_cpio=overlay_cpio)
    build_combined_initrd(base_initrd=base_initrd, overlay_cpio=overlay_cpio, out_initrd=out_initrd)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
