#!/usr/bin/env python3

import os
import shutil
import subprocess
import gzip
import tempfile
from pathlib import Path


def _pick_kernel_version(modules_dir: Path) -> str | None:
    if not modules_dir.is_dir():
        return None
    candidates = [p.name for p in sorted(modules_dir.iterdir()) if p.is_dir()]
    if not candidates:
        return None
    # Alpine netboot initramfs typically has exactly one.
    return candidates[0]


def _unsquashfs_cat(*, modloop: Path, inner_path: str, out_path: Path) -> None:
    if shutil.which("unsquashfs") is None:
        raise RuntimeError(
            "unsquashfs is required to extract modules from modloop (install 'squashfs-tools')"
        )
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("wb") as fp:
        subprocess.run(
            ["unsquashfs", "-cat", str(modloop), inner_path],
            check=True,
            stdout=fp,
            stderr=subprocess.DEVNULL,
        )


def _include_evdev_from_modloop(*, tmp_root: Path, modloop: Path) -> None:
    # We place the module under /lib/modules/<ver>/... because that's what modprobe expects.
    kernver = _pick_kernel_version(tmp_root / "lib" / "modules")
    if kernver is None:
        return

    # Alpine modloop stores modules under /modules/<ver>/...
    # Note: unsquashfs -cat takes paths relative to the squashfs root (no 'squashfs-root/' prefix).
    base = f"modules/{kernver}"

    # Copy the evdev module.
    _unsquashfs_cat(
        modloop=modloop,
        inner_path=f"{base}/kernel/drivers/input/evdev.ko",
        out_path=tmp_root / "lib" / "modules" / kernver / "kernel" / "drivers" / "input" / "evdev.ko",
    )

    # Copy module metadata so modprobe can resolve the module name.
    for name in ["modules.dep", "modules.dep.bin", "modules.alias", "modules.alias.bin"]:
        _unsquashfs_cat(
            modloop=modloop,
            inner_path=f"{base}/{name}",
            out_path=tmp_root / "lib" / "modules" / kernver / name,
        )


def build_agent_overlay_cpio(*, src_dir: Path, out_cpio: Path) -> None:
    if shutil.which("cpio") is None:
        raise RuntimeError("cpio is required to build the agent overlay (install 'cpio')")

    # Create a newc cpio archive containing the overlay files.
    out_cpio.parent.mkdir(parents=True, exist_ok=True)

    persist_mode = os.environ.get("RAYOS_AGENT_ENABLE_PERSIST_TEST", "").strip() not in (
        "",
        "0",
        "false",
        "False",
        "no",
        "NO",
    )

    required = ["rayos_init", "rayos_agent.sh", "rayos_desktop_init"]
    for rel in required:
        p = src_dir / rel
        if not p.is_file():
            raise FileNotFoundError(f"missing overlay file: {p}")

    # Stage into a temp dir so we can add optional marker files without modifying src_dir.
    with tempfile.TemporaryDirectory(prefix="rayos-agent-overlay-") as td:
        stage = Path(td)
        for rel in required:
            shutil.copy2(src_dir / rel, stage / rel)

        extra = []
        if persist_mode:
            # Marker file to enable the persistence test in rayos_init without requiring
            # unknown kernel cmdline flags (which would get passed to init argv).
            (stage / "rayos_enable_persist_test").write_text("1\n")
            extra.append("rayos_enable_persist_test")

        names = required + extra
        with out_cpio.open("wb") as fp:
            subprocess.run(
                [
                    "bash",
                    "-lc",
                    "printf '%s' | cpio -o -H newc"
                    % "".join(f"{n}\\n" for n in names),
                ],
                check=True,
                stdout=fp,
                stderr=subprocess.DEVNULL,
                cwd=str(stage),
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
    # IMPORTANT: Many cpio readers (and the kernel initramfs unpacker) stop at the
    # first TRAILER!!! record. So concatenating cpio archives (or gzip members)
    # is not a reliable way to apply overlays. Instead, merge by unpacking both
    # archives into a temp dir and repacking a single cpio.
    if base_bytes.startswith(b"\x1f\x8b\x08"):
        base_cpio = gzip.decompress(base_bytes)
        base_is_gzip = True
    elif base_bytes.startswith(b"070701") or base_bytes.startswith(b"070702"):
        base_cpio = base_bytes
        base_is_gzip = False
    else:
        raise RuntimeError(
            "Unsupported base initrd format for overlaying. "
            "Set LINUX_GUEST_KIND=host or provide a gzip initrd."
        )

    if shutil.which("cpio") is None:
        raise RuntimeError("cpio is required to build the agent overlay (install 'cpio')")

    with tempfile.TemporaryDirectory(prefix="rayos-initrd-") as td:
        tmp = Path(td)
        base_cpio_path = tmp / "base.cpio"
        base_cpio_path.write_bytes(base_cpio)

        # Extract base then overlay into the same tree.
        subprocess.run(
            ["cpio", "-idmu"],
            check=True,
            stdin=base_cpio_path.open("rb"),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            cwd=str(tmp),
        )
        subprocess.run(
            ["cpio", "-idmu"],
            check=True,
            stdin=overlay_cpio.open("rb"),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            cwd=str(tmp),
        )

        # Repack a single newc archive.
        merged_cpio_path = tmp / "merged.cpio"
        with merged_cpio_path.open("wb") as fp:
            subprocess.run(
                [
                    "bash",
                    "-lc",
                    # Archive relative paths without a leading './'.
                    "find . -mindepth 1 -print | sed 's|^\\./||' | cpio -o -H newc",
                ],
                check=True,
                stdout=fp,
                stderr=subprocess.DEVNULL,
                cwd=str(tmp),
            )

        merged_bytes = merged_cpio_path.read_bytes()
        out_bytes = gzip.compress(merged_bytes, compresslevel=9) if base_is_gzip else merged_bytes
        out_initrd.write_bytes(out_bytes)


def main() -> int:
    base = os.environ.get("BASE_INITRD", "").strip()
    out = os.environ.get("OUT_INITRD", "").strip()
    src = os.environ.get("AGENT_SRC_DIR", "").strip()
    modloop = os.environ.get("MODLOOP", "").strip()

    if not base or not out or not src:
        raise SystemExit(
            "Set BASE_INITRD, OUT_INITRD, and AGENT_SRC_DIR env vars. "
            "Example: BASE_INITRD=... OUT_INITRD=... AGENT_SRC_DIR=..."
        )

    base_initrd = Path(base)
    out_initrd = Path(out)
    src_dir = Path(src)
    modloop_path = Path(modloop) if modloop else None

    overlay_cpio = out_initrd.with_suffix(out_initrd.suffix + ".overlay.cpio")

    # Make sure scripts are executable inside the overlay.
    (src_dir / "rayos_init").chmod(0o755)
    (src_dir / "rayos_agent.sh").chmod(0o755)

    build_agent_overlay_cpio(src_dir=src_dir, out_cpio=overlay_cpio)

    # Build the merged initrd, then (optionally) inject critical modules from modloop.
    # We do this because rdinit=/rayos_init bypasses Alpine init, which would normally mount modloop.
    if modloop_path is None or not modloop_path.is_file():
        build_combined_initrd(base_initrd=base_initrd, overlay_cpio=overlay_cpio, out_initrd=out_initrd)
        return 0

    # Reuse build_combined_initrd's unpack/merge logic to keep compression consistent.
    if not base_initrd.is_file():
        raise FileNotFoundError(f"base initrd not found: {base_initrd}")
    if not overlay_cpio.is_file():
        raise FileNotFoundError(f"overlay cpio not found: {overlay_cpio}")

    base_bytes = base_initrd.read_bytes()
    overlay_bytes = overlay_cpio.read_bytes()

    if base_bytes.startswith(b"\x1f\x8b\x08"):
        base_cpio = gzip.decompress(base_bytes)
        base_is_gzip = True
    elif base_bytes.startswith(b"070701") or base_bytes.startswith(b"070702"):
        base_cpio = base_bytes
        base_is_gzip = False
    else:
        raise RuntimeError(
            "Unsupported base initrd format for overlaying. "
            "Set LINUX_GUEST_KIND=host or provide a gzip initrd."
        )

    if shutil.which("cpio") is None:
        raise RuntimeError("cpio is required to build the agent overlay (install 'cpio')")

    with tempfile.TemporaryDirectory(prefix="rayos-initrd-") as td:
        tmp = Path(td)
        base_cpio_path = tmp / "base.cpio"
        overlay_cpio_path = tmp / "overlay.cpio"
        base_cpio_path.write_bytes(base_cpio)
        overlay_cpio_path.write_bytes(overlay_bytes)

        subprocess.run(
            ["cpio", "-idmu"],
            check=True,
            stdin=base_cpio_path.open("rb"),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            cwd=str(tmp),
        )
        subprocess.run(
            ["cpio", "-idmu"],
            check=True,
            stdin=overlay_cpio_path.open("rb"),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            cwd=str(tmp),
        )

        _include_evdev_from_modloop(tmp_root=tmp, modloop=modloop_path)

        merged_cpio_path = tmp / "merged.cpio"
        with merged_cpio_path.open("wb") as fp:
            subprocess.run(
                [
                    "bash",
                    "-lc",
                    "find . -mindepth 1 -print | sed 's|^\\./||' | cpio -o -H newc",
                ],
                check=True,
                stdout=fp,
                stderr=subprocess.DEVNULL,
                cwd=str(tmp),
            )

        merged_bytes = merged_cpio_path.read_bytes()
        out_bytes = gzip.compress(merged_bytes, compresslevel=9) if base_is_gzip else merged_bytes
        out_initrd.parent.mkdir(parents=True, exist_ok=True)
        out_initrd.write_bytes(out_bytes)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
