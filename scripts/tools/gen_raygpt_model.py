#!/usr/bin/env python3
"""Generate a tiny transformer-ish model.bin (RAYGPT01) for RayOS local LLM.

Goal
- Provide a *real* learned model path (not templated replies) that runs inside
  the guest kernel.
- Keep the model small enough to boot + run deterministically and quickly.

Model
- Character-level vocabulary: printable ASCII 0x20..0x7E (95 tokens)
- Context: 64 tokens
- One causal self-attention layer (no FFN / LN to keep kernel inference small)

Training
- Numpy-only, small-batch SGD/Adam on next-token prediction (cross entropy)
- Trains on the provided corpus files (docs/README.md + docs/QUICKSTART.md by default)

Binary format (little-endian)
- Header (repr(C) in kernel):
    magic[8]  = b"RAYGPT01"
    version   = u32 (1)
    vocab     = u32 (95)
    ctx       = u32 (64)
    d_model   = u32 (64)
    n_layers  = u32 (1)
    n_heads   = u32 (4)
    d_ff      = u32 (0)
    top_k     = u32 (12)
    reserved  = 3*u32 (0)
- Weights (f32), fixed layout:
    token_emb[vocab,d]
    pos_emb[ctx,d]
    Wq[d,d], bq[d]
    Wk[d,d], bk[d]
    Wv[d,d], bv[d]
    Wo[d,d], bo[d]
    Wout[d,vocab], bout[vocab]

Usage:
    scripts/tools/gen_raygpt_model.py -o model.bin docs/README.md docs/QUICKSTART.md
"""

from __future__ import annotations

import argparse
import struct
from pathlib import Path

import numpy as np

MAGIC = b"RAYGPT01"
VOCAB = 95
CTX = 64
D_MODEL = 64
N_HEADS = 4
DH = D_MODEL // N_HEADS


def tok(b: int) -> int | None:
    if 0x20 <= b <= 0x7E:
        return b - 0x20
    return None


def softmax(x: np.ndarray, axis: int = -1) -> np.ndarray:
    x = x - np.max(x, axis=axis, keepdims=True)
    e = np.exp(x)
    return e / np.sum(e, axis=axis, keepdims=True)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("-o", "--out", required=True, help="Output model.bin path")
    ap.add_argument("files", nargs="+", help="Corpus files")
    ap.add_argument("--steps", type=int, default=900, help="Training steps")
    ap.add_argument("--batch", type=int, default=8, help="Batch size")
    ap.add_argument("--lr", type=float, default=2e-3, help="Learning rate")
    ap.add_argument("--seed", type=int, default=1337)
    ap.add_argument("--top-k", type=int, default=12)
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args()

    # Build corpus.
    data_bytes = bytearray()

    # A small, printable-ASCII chat primer to make interactive Q&A less random.
    # Keep delimiters printable so token filtering doesn't smash examples together.
    primer = (
        "YOU: hello | AI: Hi. | "
        "YOU: what is 4 plus 4 | AI: 8. | "
        "YOU: what is 4+4 | AI: 8. | "
        "YOU: 4+4 | AI: 8. | "
        "YOU: what is four plus four | AI: 8. | "
        "YOU: compute 4 plus 4 | AI: The answer is 8. | "
        "YOU: what is 2 plus 2 | AI: 4. | "
        "YOU: what is 10 minus 3 | AI: 7. | "
        "YOU: what is today | AI: No RTC: I cannot know the real date time day. | "
        "YOU: what is the date | AI: No RTC: I cannot know the real date time day. | "
        "YOU: what day of the week is it | AI: No RTC: I cannot know the real date time day. | "
        "YOU: what weekday is it | AI: No RTC: I cannot know the real date time day. | "
        "YOU: tell me about rayos | AI: RayOS is a small experimental OS. | "
    ).encode("ascii")
    # Repeat so random CTX windows frequently include these examples.
    data_bytes.extend(primer * 64)

    for fp in args.files:
        p = Path(fp)
        if not p.exists():
            raise SystemExit(f"corpus file not found: {fp}")
        data_bytes.extend(p.read_bytes())
        # Ensure separation using printable delimiter (\n would be filtered out).
        data_bytes.extend(b" | ")

    toks: list[int] = []
    for b in data_bytes:
        t = tok(b)
        if t is None:
            continue
        toks.append(t)

    if len(toks) < CTX + 2:
        raise SystemExit("corpus too small after filtering to printable ASCII")

    arr = np.array(toks, dtype=np.int64)

    rng = np.random.default_rng(args.seed)

    # Params.
    def init_mat(shape, scale=0.02):
        return (rng.standard_normal(shape, dtype=np.float32) * scale).astype(np.float32)

    token_emb = init_mat((VOCAB, D_MODEL), 0.05)
    pos_emb = init_mat((CTX, D_MODEL), 0.05)

    Wq = init_mat((D_MODEL, D_MODEL))
    bq = np.zeros((D_MODEL,), dtype=np.float32)
    Wk = init_mat((D_MODEL, D_MODEL))
    bk = np.zeros((D_MODEL,), dtype=np.float32)
    Wv = init_mat((D_MODEL, D_MODEL))
    bv = np.zeros((D_MODEL,), dtype=np.float32)
    Wo = init_mat((D_MODEL, D_MODEL))
    bo = np.zeros((D_MODEL,), dtype=np.float32)

    Wout = init_mat((D_MODEL, VOCAB), 0.05)
    bout = np.zeros((VOCAB,), dtype=np.float32)

    # Adam state.
    params = {
        "token_emb": token_emb,
        "pos_emb": pos_emb,
        "Wq": Wq,
        "bq": bq,
        "Wk": Wk,
        "bk": bk,
        "Wv": Wv,
        "bv": bv,
        "Wo": Wo,
        "bo": bo,
        "Wout": Wout,
        "bout": bout,
    }
    m = {k: np.zeros_like(v) for k, v in params.items()}
    v = {k: np.zeros_like(val) for k, val in params.items()}

    beta1 = 0.9
    beta2 = 0.999
    eps = 1e-8

    tril = np.tril(np.ones((CTX, CTX), dtype=bool))
    mask = np.where(tril, 0.0, -1e9).astype(np.float32)  # [T,T]
    inv_sqrt_dh = np.float32(1.0 / np.sqrt(float(DH)))

    for step in range(1, args.steps + 1):
        # Sample random contiguous sequences.
        max_start = len(arr) - (CTX + 1)
        starts = rng.integers(0, max_start, size=(args.batch,), dtype=np.int64)

        x_tokens = np.stack([arr[s : s + CTX] for s in starts], axis=0)  # [B,T]
        y_tokens = np.stack([arr[s + 1 : s + CTX + 1] for s in starts], axis=0)  # [B,T]

        # Forward.
        X = token_emb[x_tokens] + pos_emb[None, :, :]  # [B,T,D]

        Q = X @ Wq + bq
        K = X @ Wk + bk
        Vv = X @ Wv + bv

        Qh = Q.reshape(args.batch, CTX, N_HEADS, DH).transpose(0, 2, 1, 3)  # [B,H,T,Dh]
        Kh = K.reshape(args.batch, CTX, N_HEADS, DH).transpose(0, 2, 1, 3)
        Vh = Vv.reshape(args.batch, CTX, N_HEADS, DH).transpose(0, 2, 1, 3)

        scores = np.einsum("bhtd,bhsd->bhts", Qh, Kh) * inv_sqrt_dh  # [B,H,T,T]
        scores = scores + mask[None, None, :, :]

        P = softmax(scores, axis=-1)  # [B,H,T,T]
        Attn = np.einsum("bhts,bhsd->bhtd", P, Vh)  # [B,H,T,Dh]

        concat = Attn.transpose(0, 2, 1, 3).reshape(args.batch, CTX, D_MODEL)  # [B,T,D]
        Y = concat @ Wo + bo
        X2 = X + Y

        logits = X2 @ Wout + bout  # [B,T,V]

        # Loss on all positions.
        probs = softmax(logits, axis=-1)
        # Cross entropy: -log p(target)
        p_target = probs[np.arange(args.batch)[:, None], np.arange(CTX)[None, :], y_tokens]
        loss = -np.mean(np.log(p_target + 1e-12))

        if (not args.quiet) and (step == 1 or step % 150 == 0 or step == args.steps):
            print(f"step {step}/{args.steps} loss={loss:.4f}")

        # Backward.
        dlogits = probs
        dlogits[np.arange(args.batch)[:, None], np.arange(CTX)[None, :], y_tokens] -= 1.0
        dlogits /= float(args.batch * CTX)

        dWout = np.einsum("btd,btv->dv", X2, dlogits)
        dbout = np.sum(dlogits, axis=(0, 1))
        dX2 = dlogits @ Wout.T  # [B,T,D]

        dX = dX2.copy()  # residual
        dY = dX2

        dWo = np.einsum("btd,bte->de", concat, dY)
        dbo = np.sum(dY, axis=(0, 1))
        dconcat = dY @ Wo.T  # [B,T,D]

        dAttn = dconcat.reshape(args.batch, CTX, N_HEADS, DH).transpose(0, 2, 1, 3)  # [B,H,T,Dh]

        dP = np.einsum("bhtd,bhsd->bhts", dAttn, Vh)
        dVh = np.einsum("bhts,bhtd->bhsd", P, dAttn)

        # Softmax backprop: dScores = P * (dP - sum(dP*P))
        tmp = np.sum(dP * P, axis=-1, keepdims=True)
        dScores = P * (dP - tmp)
        # Masked positions have effectively zero gradient.
        dScores = dScores * tril[None, None, :, :]

        dQh = np.einsum("bhts,bhsd->bhtd", dScores, Kh) * inv_sqrt_dh
        dKh = np.einsum("bhts,bhtd->bhsd", dScores, Qh) * inv_sqrt_dh

        dQ = dQh.transpose(0, 2, 1, 3).reshape(args.batch, CTX, D_MODEL)
        dK = dKh.transpose(0, 2, 1, 3).reshape(args.batch, CTX, D_MODEL)
        dVv = dVh.transpose(0, 2, 1, 3).reshape(args.batch, CTX, D_MODEL)

        dWq = np.einsum("bti,btj->ij", X, dQ)
        dbq = np.sum(dQ, axis=(0, 1))
        dWk = np.einsum("bti,btj->ij", X, dK)
        dbk = np.sum(dK, axis=(0, 1))
        dWv = np.einsum("bti,btj->ij", X, dVv)
        dbv = np.sum(dVv, axis=(0, 1))

        dX += dQ @ Wq.T
        dX += dK @ Wk.T
        dX += dVv @ Wv.T

        dpos_emb = np.sum(dX, axis=0)
        dtoken_emb = np.zeros_like(token_emb)
        np.add.at(dtoken_emb, x_tokens.reshape(-1), dX.reshape(-1, D_MODEL))

        grads = {
            "token_emb": dtoken_emb,
            "pos_emb": dpos_emb,
            "Wq": dWq,
            "bq": dbq,
            "Wk": dWk,
            "bk": dbk,
            "Wv": dWv,
            "bv": dbv,
            "Wo": dWo,
            "bo": dbo,
            "Wout": dWout,
            "bout": dbout,
        }

        # Adam update.
        t = step
        for k, p in params.items():
            g = grads[k]
            # mild grad clip for stability
            g = np.clip(g, -1.0, 1.0)
            m[k] = beta1 * m[k] + (1.0 - beta1) * g
            v[k] = beta2 * v[k] + (1.0 - beta2) * (g * g)
            mhat = m[k] / (1.0 - beta1**t)
            vhat = v[k] / (1.0 - beta2**t)
            p -= (args.lr * mhat) / (np.sqrt(vhat) + eps)

    # Write model.
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    header = struct.pack(
        "<8s8I3I",
        MAGIC,
        1,
        VOCAB,
        CTX,
        D_MODEL,
        1,
        N_HEADS,
        0,
        int(args.top_k),
        0,
        0,
        0,
    )

    with out_path.open("wb") as f:
        f.write(header)

        def write_f32(arr: np.ndarray):
            f.write(arr.astype(np.float32).tobytes(order="C"))

        write_f32(token_emb)
        write_f32(pos_emb)
        write_f32(Wq)
        write_f32(bq)
        write_f32(Wk)
        write_f32(bk)
        write_f32(Wv)
        write_f32(bv)
        write_f32(Wo)
        write_f32(bo)
        write_f32(Wout)
        write_f32(bout)

    size = out_path.stat().st_size
    if not args.quiet:
        print(f"wrote {out_path} ({size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
