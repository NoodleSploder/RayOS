#!/usr/bin/env python3
"""Derive RayOS in-kernel VMM MMIO layout from hypervisor.rs.

Goal: keep host-side scripts from hardcoding addresses like 0x10202000.

This intentionally supports only the small expression subset used by the
`const MMIO_*` definitions (integer literals, identifiers, +, -, *, /,
parentheses, and simple `as u64` casts).

Usage (from bash):
  eval "$(python3 scripts/tools/vmm_mmio_map.py --print-env)"
  echo "$BASE_CMDLINE $CMDLINE_VIRTIO_MMIO_DEVICES" > cmdline.txt
"""

from __future__ import annotations

import argparse
import ast
import os
import re
import shlex
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


CONST_RE = re.compile(
    # Allow trailing line comments after the semicolon.
    r"^\s*const\s+(?P<name>[A-Z0-9_]+)\s*:\s*[^=]+?=\s*(?P<expr>[^;]+);\s*(?://.*)?$"
)


class SafeEvalError(Exception):
    pass


def _strip_rust_casts(expr: str) -> str:
    # Drop common `as u64`/`as usize` casts used in the consts.
    expr = re.sub(r"\bas\s+(u64|usize|u32|u16|u8)\b", "", expr)
    # Rust allows `_` separators in numeric literals; Python too, but keep as-is.
    return expr


def _safe_eval_int(expr: str, names: dict[str, int]) -> int:
    """Evaluate a restricted integer expression safely."""

    expr = _strip_rust_casts(expr).strip()

    # Replace Rust-style hex like 0xFFFF_FFFF with Python-compatible (it already is).
    # Just ensure identifiers remain identifiers.

    try:
        node = ast.parse(expr, mode="eval")
    except SyntaxError as e:
        raise SafeEvalError(f"failed to parse expr: {expr!r}: {e}")

    def eval_node(n: ast.AST) -> int:
        if isinstance(n, ast.Expression):
            return eval_node(n.body)
        if isinstance(n, ast.Constant):
            if isinstance(n.value, int):
                return int(n.value)
            raise SafeEvalError(f"non-int constant in expr: {expr!r}")
        if isinstance(n, ast.Name):
            if n.id in names:
                return int(names[n.id])
            raise SafeEvalError(f"unknown identifier {n.id!r} in expr: {expr!r}")
        if isinstance(n, ast.UnaryOp) and isinstance(n.op, (ast.UAdd, ast.USub)):
            v = eval_node(n.operand)
            return v if isinstance(n.op, ast.UAdd) else -v
        if isinstance(n, ast.BinOp) and isinstance(n.op, (ast.Add, ast.Sub, ast.Mult, ast.FloorDiv, ast.Div)):
            a = eval_node(n.left)
            b = eval_node(n.right)
            if isinstance(n.op, ast.Add):
                return a + b
            if isinstance(n.op, ast.Sub):
                return a - b
            if isinstance(n.op, ast.Mult):
                return a * b
            # Treat `/` as integer division for our const usage.
            if b == 0:
                raise SafeEvalError(f"division by zero in expr: {expr!r}")
            return a // b
        paren_expr = getattr(ast, "ParenExpr", None)  # py>=3.12
        if paren_expr is not None and isinstance(n, paren_expr):
            return eval_node(n.expression)
        raise SafeEvalError(f"unsupported syntax in expr: {expr!r}: {ast.dump(n, include_attributes=False)}")

    return eval_node(node)


@dataclass(frozen=True)
class Features:
    vmm_linux_guest: bool
    vmm_virtio_gpu: bool
    vmm_virtio_input: bool


def _features_from_env_or_args(features_csv: str | None) -> Features:
    if features_csv is None:
        features_csv = os.environ.get("RAYOS_KERNEL_FEATURES", "")
    parts = [p.strip() for p in features_csv.split(",") if p.strip()]
    s = set(parts)
    return Features(
        vmm_linux_guest=("vmm_linux_guest" in s),
        vmm_virtio_gpu=("vmm_virtio_gpu" in s),
        vmm_virtio_input=("vmm_virtio_input" in s),
    )


def _parse_const_defs(text: str) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for line in text.splitlines():
        m = CONST_RE.match(line)
        if not m:
            continue
        name = m.group("name")
        expr = m.group("expr").strip()
        out.setdefault(name, []).append(expr)
    return out


def _pick_const_expr(defs: dict[str, list[str]], name: str, *, prefer: int | None = None) -> str:
    if name not in defs or not defs[name]:
        raise KeyError(name)
    if prefer is None:
        # Use the last definition (often the cfg(all(...)) one is later).
        return defs[name][-1]
    # Pick the definition that contains the preferred literal if possible.
    # This is mainly for cfg-chosen constants like GUEST_RAM_SIZE_MB.
    for expr in defs[name]:
        if str(prefer) in expr:
            return expr
    return defs[name][-1]


def _hex(v: int) -> str:
    # Match the style used in scripts (lowercase hex, 0x prefix).
    return hex(v)


def _emit_env(k: str, v: str) -> str:
    return f"{k}={shlex.quote(v)}"


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--hypervisor-rs",
        default=None,
        help="Path to crates/kernel-bare/src/hypervisor.rs (defaults to repo-relative)",
    )
    ap.add_argument(
        "--features",
        default=None,
        help="Comma-separated feature list (defaults to RAYOS_KERNEL_FEATURES env)",
    )
    ap.add_argument(
        "--print-env",
        action="store_true",
        help="Print shell env assignments (KEY='value')",
    )

    args = ap.parse_args(argv)

    repo_root = Path(__file__).resolve().parents[2]
    hypervisor_rs = Path(args.hypervisor_rs) if args.hypervisor_rs else repo_root / "crates/kernel-bare/src/hypervisor.rs"

    feats = _features_from_env_or_args(args.features)

    try:
        text = hypervisor_rs.read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        print(f"FAIL: could not read {hypervisor_rs}: {e}", file=sys.stderr)
        return 2

    defs = _parse_const_defs(text)

    # Resolve config-dependent consts.
    # hypervisor.rs defines GUEST_RAM_SIZE_MB twice (cfg vmm_linux_guest => 256, else 16).
    guest_ram_mb = 256 if feats.vmm_linux_guest else 16

    # Evaluate consts we care about.
    names: dict[str, int] = {}

    # Seed the feature-selected constant explicitly.
    names["GUEST_RAM_SIZE_MB"] = guest_ram_mb

    # Prefer reading PAGE_SIZE from file, but default to 4096.
    try:
        page_size_expr = _pick_const_expr(defs, "PAGE_SIZE")
        names["PAGE_SIZE"] = _safe_eval_int(page_size_expr, names)
    except Exception:
        names["PAGE_SIZE"] = 4096

    # Provide basic identifiers used by the MMIO chain.
    # We re-evaluate in a dependency loop to avoid relying on file order.
    wanted = [
        "GUEST_RAM_SIZE_BYTES",
        "MMIO_COUNTER_BASE",
        "MMIO_COUNTER_SIZE",
        "MMIO_VIRTIO_BASE",
        "MMIO_VIRTIO_SIZE",
        "MMIO_VIRTIO_GPU_SHM_BASE",
        "MMIO_VIRTIO_GPU_SHM_SIZE",
        "MMIO_VIRTIO_INPUT_BASE",
        "MMIO_VIRTIO_INPUT_SIZE",
        "VIRTIO_MMIO_IRQ_PIN",
        "VIRTIO_MMIO_INPUT_IRQ_PIN",
    ]

    remaining = set(wanted)
    for _ in range(64):
        progressed = False
        for name in list(remaining):
            if name in names:
                remaining.discard(name)
                continue
            if name not in defs:
                continue

            # Choose definition: for cfg-only values, last is fine.
            expr = _pick_const_expr(defs, name)
            try:
                names[name] = _safe_eval_int(expr, names)
                remaining.discard(name)
                progressed = True
            except SafeEvalError:
                # Dependencies not ready yet.
                continue
        if not remaining or not progressed:
            break

    # Derive cmdline pieces.
    virtio_mmio_size = names.get("MMIO_VIRTIO_SIZE", names.get("PAGE_SIZE", 4096))
    virtio_mmio_base = names.get("MMIO_VIRTIO_BASE")
    virtio_mmio_irq_pin = names.get("VIRTIO_MMIO_IRQ_PIN")

    if virtio_mmio_base is None or virtio_mmio_irq_pin is None:
        print("FAIL: could not derive MMIO_VIRTIO_BASE or VIRTIO_MMIO_IRQ_PIN from hypervisor.rs", file=sys.stderr)
        return 2

    cmdline_parts: list[str] = []

    # In our model, the *primary* virtio-mmio window always exists (GPU or input-only).
    cmdline_parts.append(
        f"virtio_mmio.device={_hex(virtio_mmio_size)}@{_hex(virtio_mmio_base)}:{virtio_mmio_irq_pin}"
    )

    # If both gpu+input are enabled, virtio-input is on a second window.
    if feats.vmm_virtio_gpu and feats.vmm_virtio_input:
        input_base = names.get("MMIO_VIRTIO_INPUT_BASE")
        input_size = names.get("MMIO_VIRTIO_INPUT_SIZE", virtio_mmio_size)
        input_irq = names.get("VIRTIO_MMIO_INPUT_IRQ_PIN")
        if input_base is None or input_irq is None:
            print(
                "FAIL: features imply virtio-input second window, but MMIO_VIRTIO_INPUT_BASE/IRQ could not be derived",
                file=sys.stderr,
            )
            return 2
        cmdline_parts.append(
            f"virtio_mmio.device={_hex(input_size)}@{_hex(input_base)}:{input_irq}"
        )

    out_env: dict[str, str] = {
        "VIRTIO_MMIO_SIZE_HEX": _hex(virtio_mmio_size),
        "VIRTIO_MMIO_BASE_HEX": _hex(virtio_mmio_base),
        "VIRTIO_MMIO_IRQ_PIN": str(virtio_mmio_irq_pin),
        "CMDLINE_VIRTIO_MMIO_DEVICES": " ".join(cmdline_parts),
    }

    if feats.vmm_virtio_gpu and feats.vmm_virtio_input:
        out_env["VIRTIO_MMIO_INPUT_BASE_HEX"] = _hex(names["MMIO_VIRTIO_INPUT_BASE"])
        out_env["VIRTIO_MMIO_INPUT_IRQ_PIN"] = str(names["VIRTIO_MMIO_INPUT_IRQ_PIN"])

    if args.print_env:
        for k in sorted(out_env.keys()):
            print(_emit_env(k, out_env[k]))
        return 0

    # Default: print the cmdline fragment only.
    print(out_env["CMDLINE_VIRTIO_MMIO_DEVICES"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
