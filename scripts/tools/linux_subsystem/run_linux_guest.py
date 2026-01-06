#!/usr/bin/env python3

import glob
import os
import signal
import shlex
import subprocess
import sys
import time
import threading
from dataclasses import dataclass
from pathlib import Path
from urllib.request import urlopen

# When executed as a script, sys.path defaults to the script directory.
# Add scripts/ so we can import the tools/ package (scripts/tools/...).
_SCRIPTS_DIR = Path(__file__).resolve().parents[2]
if str(_SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS_DIR))

from tools.linux_subsystem.surface_bridge import SurfaceBridge


MARKER = "RAYOS_LINUX_GUEST_READY"

DEFAULT_ALPINE_VERSION = os.environ.get("ALPINE_NETBOOT_VERSION", "3.20.3")
DEFAULT_ALPINE_BASE = os.environ.get(
    "ALPINE_NETBOOT_BASE_URL",
    "https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/x86_64",
)


@dataclass(frozen=True)
class GuestArtifacts:
    kernel: Path
    initrd: Path
    modloop: Path | None = None


def _bool_env(name: str, default: bool) -> bool:
    v = os.environ.get(name)
    if v is None:
        return default
    return v.strip() not in ("", "0", "false", "False", "no", "NO")


def _run_checked(argv: list[str], *, cwd: Path | None = None) -> None:
    subprocess.run(argv, check=True, cwd=str(cwd) if cwd else None)


def _ensure_agent_initrd(*, base_initrd: Path, work_dir: Path) -> Path:
    # Keep separate initrd outputs for different agent modes so caching doesn't
    # accidentally reuse a mismatched overlay.
    persist_mode = _bool_env("RAYOS_AGENT_ENABLE_PERSIST_TEST", False)
    out_name = "initramfs-with-agent-persist" if persist_mode else "initramfs-with-agent"
    out_initrd = work_dir / "linux-guest" / "agent" / out_name
    out_initrd.parent.mkdir(parents=True, exist_ok=True)

    agent_src = Path(__file__).resolve().parent / "guest_agent"
    builder = Path(__file__).resolve().parent / "build_agent_initrd.py"

    src_paths = [
        agent_src / "rayos_init",
        agent_src / "rayos_agent.sh",
        agent_src / "rayos_desktop_init",
        builder,
    ]

    latest_src_mtime = 0.0
    for p in src_paths:
        if not p.is_file():
            raise FileNotFoundError(f"missing agent build input: {p}")
        latest_src_mtime = max(latest_src_mtime, p.stat().st_mtime)

    if out_initrd.is_file() and out_initrd.stat().st_size > 0:
        if out_initrd.stat().st_mtime >= latest_src_mtime:
            return out_initrd

    env = os.environ.copy()
    env["BASE_INITRD"] = str(base_initrd)
    env["OUT_INITRD"] = str(out_initrd)
    env["AGENT_SRC_DIR"] = str(agent_src)

    # Alpine netboot uses a separate squashfs (modloop-virt) for kernel modules. Since we bypass
    # Alpine init (rdinit=/rayos_init), we need to make critical modules available in the initramfs.
    modloop_candidate = base_initrd.parent / "modloop-virt"
    if modloop_candidate.is_file():
        env["MODLOOP"] = str(modloop_candidate)

    subprocess.run(["python3", str(builder)], check=True, env=env)

    if not out_initrd.is_file() or out_initrd.stat().st_size == 0:
        raise RuntimeError("failed to build agent initrd")
    return out_initrd


def _first_existing(candidates: list[str]) -> Path | None:
    for pattern in candidates:
        for p in sorted(glob.glob(pattern)):
            path = Path(p)
            if path.is_file():
                return path
    return None


def _download(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.is_file() and dest.stat().st_size > 0:
        return

    with urlopen(url, timeout=60) as resp:
        if resp.status != 200:
            raise RuntimeError(f"download failed: HTTP {resp.status} for {url}")
        data = resp.read()
        if not data:
            raise RuntimeError(f"download returned empty body: {url}")
        dest.write_bytes(data)


def resolve_artifacts(*, work_dir: Path) -> GuestArtifacts:
    # We default to Alpine netboot artifacts because:
    # - they are small,
    # - they boot reliably under QEMU TCG,
    # - they avoid host-kernel CPU-feature mismatches (e.g., x86-64-v3 kernels).
    guest_kind = os.environ.get("LINUX_GUEST_KIND", "alpine-netboot").strip().lower()

    if guest_kind == "alpine-netboot":
        version = DEFAULT_ALPINE_VERSION
        rel_dir = f"netboot-{version}"
        base = DEFAULT_ALPINE_BASE.rstrip("/")

        cache_dir = work_dir / "linux-guest" / "alpine-netboot" / version
        kernel = cache_dir / "vmlinuz-virt"
        initrd = cache_dir / "initramfs-virt"
        modloop = cache_dir / "modloop-virt"

        _download(f"{base}/{rel_dir}/vmlinuz-virt", kernel)
        _download(f"{base}/{rel_dir}/initramfs-virt", initrd)
        # Needed when using rdinit=/rayos_desktop_init (we bypass Alpine init which would mount modloop).
        _download(f"{base}/{rel_dir}/modloop-virt", modloop)

        if _bool_env("USE_AGENT_INITRD", False):
            initrd = _ensure_agent_initrd(base_initrd=initrd, work_dir=work_dir)

        return GuestArtifacts(kernel=kernel, initrd=initrd, modloop=modloop)

    if guest_kind != "host":
        raise ValueError(
            f"Unknown LINUX_GUEST_KIND={guest_kind!r}. Use 'alpine-netboot' or 'host'."
        )

    kernel_env = os.environ.get("LINUX_KERNEL", "").strip()
    initrd_env = os.environ.get("LINUX_INITRD", "").strip()

    if kernel_env and initrd_env:
        kernel = Path(kernel_env)
        initrd = Path(initrd_env)
        if not kernel.is_file():
            raise FileNotFoundError(f"LINUX_KERNEL not found: {kernel}")
        if not initrd.is_file():
            raise FileNotFoundError(f"LINUX_INITRD not found: {initrd}")
        return GuestArtifacts(kernel=kernel, initrd=initrd)

    uname_r = os.uname().release

    kernel = _first_existing(
        [
            f"/boot/vmlinuz-{uname_r}",
            "/boot/vmlinuz-*",
        ]
    )
    if kernel is None:
        raise FileNotFoundError(
            "Could not find a Linux kernel in /boot. Set LINUX_KERNEL and LINUX_INITRD explicitly."
        )

    initrd = _first_existing(
        [
            f"/boot/initrd.img-{uname_r}",
            f"/boot/initramfs-{uname_r}*",
            "/boot/initrd.img-*",
            "/boot/initramfs-*",
        ]
    )
    if initrd is None:
        raise FileNotFoundError(
            "Could not find an initrd/initramfs in /boot. Set LINUX_KERNEL and LINUX_INITRD explicitly."
        )

    return GuestArtifacts(kernel=kernel, initrd=initrd)


def run_guest(*, log_path: Path, timeout_secs: float) -> int:
    qemu_bin = os.environ.get("QEMU_BIN", "qemu-system-x86_64")

    work_dir = log_path.parent

    artifacts = resolve_artifacts(work_dir=work_dir)

    # This is Linux-as-guest bring-up plumbing for Option D.
    # - Step 2: boot into initramfs shell and inject a deterministic marker.
    # - Step 3: boot into a minimal guest agent (rdinit=/rayos_init) and wait for agent markers.
    cmdline = os.environ.get(
        "LINUX_CMDLINE",
        "console=ttyS0 rdinit=/bin/sh loglevel=7 earlyprintk=serial,ttyS0,115200 panic=-1",
    )

    ready_marker = os.environ.get("READY_MARKER", MARKER)
    inject_ready_marker = _bool_env(
        "INJECT_READY_MARKER",
        default=(ready_marker == MARKER),
    )
    post_ready_send = os.environ.get("POST_READY_SEND", "")
    post_ready_expect = os.environ.get("POST_READY_EXPECT", "")

    qemu_extra_args: list[str] = []
    extra = os.environ.get("QEMU_EXTRA_ARGS", "").strip()
    if extra:
        qemu_extra_args = shlex.split(extra)

    enable_net = _bool_env("QEMU_NET", False) or _bool_env("LINUX_NET", False)
    net_args: list[str]
    if enable_net:
        net_args = [
            "-netdev",
            "user,id=n0",
            "-device",
            "virtio-net-pci,netdev=n0",
        ]
    else:
        net_args = ["-net", "none"]

    enable_gpu = _bool_env("QEMU_GPU", False)
    gpu_dev = os.environ.get("QEMU_GPU_DEV", "virtio-gpu-pci").strip() or "virtio-gpu-pci"
    gpu_args: list[str] = []
    if enable_gpu:
        gpu_args = ["-device", gpu_dev]

    qemu_args = [
        qemu_bin,
        "-machine",
        "q35",
        "-m",
        os.environ.get("LINUX_MEM", "1024"),
        "-smp",
        os.environ.get("LINUX_SMP", "2"),
        "-kernel",
        str(artifacts.kernel),
        "-initrd",
        str(artifacts.initrd),
        "-append",
        cmdline,
        # Capture only serial output (avoid VGA/SeaBIOS escape sequences).
        "-display",
        "none",
        "-vga",
        "none",
        "-serial",
        "stdio",
        "-no-reboot",
        "-monitor",
        "none",
        *net_args,
        *gpu_args,
        *qemu_extra_args,
    ]

    log_path.parent.mkdir(parents=True, exist_ok=True)

    mirror_serial = _bool_env("MIRROR_SERIAL", False)
    pass_stdin = _bool_env("PASS_STDIN", False)

    # Note: -nographic routes serial to stdio. We keep everything in one stream.
    with log_path.open("wb") as log_fp:
        proc = subprocess.Popen(
            qemu_args,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )

        if pass_stdin and proc.stdin is not None:
            def _pump_stdin() -> None:
                try:
                    while True:
                        data = sys.stdin.buffer.readline()
                        if not data:
                            break
                        try:
                            proc.stdin.write(data)
                            proc.stdin.flush()
                        except Exception:
                            break
                except Exception:
                    return

            t = threading.Thread(target=_pump_stdin, daemon=True)
            t.start()

        def shutdown():
            if proc.poll() is not None:
                return
            try:
                proc.send_signal(signal.SIGTERM)
            except Exception:
                pass
            for _ in range(30):
                if proc.poll() is not None:
                    return
                time.sleep(0.05)
            try:
                proc.kill()
            except Exception:
                pass

        start = time.time()
        saw_shell_hint = False
        injected_ready = False
        observed_ready = False
        sent_post_ready = False
        observed_post_ready = False

        bridge: SurfaceBridge | None = None
        bridge_out = os.environ.get("SURFACE_BRIDGE_OUT_DIR", "").strip()
        if bridge_out:
            bridge = SurfaceBridge(out_dir=Path(bridge_out))

        try:
            while True:
                if timeout_secs > 0 and time.time() - start > timeout_secs:
                    raise TimeoutError(
                        f"Timed out after {timeout_secs}s waiting for Linux guest marker {MARKER}"
                    )

                chunk = proc.stdout.readline() if proc.stdout is not None else b""
                if chunk:
                    log_fp.write(chunk)
                    log_fp.flush()
                    line = chunk.decode("utf-8", errors="replace")

                    if mirror_serial:
                        sys.stdout.write(line)
                        sys.stdout.flush()

                    if bridge is not None:
                        bridge.on_line(line.rstrip("\n"))

                    # Common initramfs shell hints across busybox ash variants.
                    if (
                        "job control turned off" in line
                        or "can't access tty" in line
                        or "can't access tty; job control turned off" in line
                    ):
                        saw_shell_hint = True

                    if ready_marker in line:
                        observed_ready = True
                        if not post_ready_send:
                            return 0

                    if post_ready_expect and post_ready_expect in line:
                        observed_post_ready = True
                        return 0

                # If the guest shell is likely running, inject ready marker once (Step 2 mode).
                if inject_ready_marker and saw_shell_hint and not injected_ready:
                    if proc.stdin is not None:
                        proc.stdin.write(f"echo {ready_marker}\n".encode("utf-8"))
                        proc.stdin.flush()
                    injected_ready = True

                # After readiness, optionally send a control command and wait for a response.
                if observed_ready and post_ready_send and not sent_post_ready:
                    if proc.stdin is not None:
                        proc.stdin.write(post_ready_send.encode("utf-8"))
                        proc.stdin.flush()
                    sent_post_ready = True

                if proc.poll() is not None:
                    # Guest exited early.
                    return proc.returncode or 1

        except Exception:
            shutdown()
            raise
        finally:
            if bridge is not None:
                try:
                    bridge.close()
                except Exception:
                    pass
            shutdown()


def main() -> int:
    work_dir = Path(os.environ.get("WORK_DIR", str(Path(__file__).resolve().parents[2] / "build")))
    log_path = Path(os.environ.get("LOG_FILE", str(work_dir / "linux-subsystem-headless.log")))

    if _bool_env("PREPARE_ONLY", False):
        artifacts = resolve_artifacts(work_dir=work_dir)
        # Stable, parseable output.
        print(f"KERNEL={artifacts.kernel}")
        print(f"INITRD={artifacts.initrd}")
        if artifacts.modloop is not None:
            print(f"MODLOOP={artifacts.modloop}")
        return 0

    timeout_secs = float(os.environ.get("TIMEOUT_SECS", "45"))

    try:
        rc = run_guest(log_path=log_path, timeout_secs=timeout_secs)
    except Exception as e:
        sys.stderr.write(f"ERROR: {e}\n")
        sys.stderr.write(f"Log: {log_path}\n")
        return 1

    return rc


if __name__ == "__main__":
    raise SystemExit(main())
