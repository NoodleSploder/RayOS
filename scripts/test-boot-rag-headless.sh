#!/bin/bash
# Headless RAG smoke test for RayOS (x86_64 kernel-bare).
# - Boots kernel-bare under OVMF
# - Stages EFI/RAYOS/embeddings.bin + index.bin
# - Injects a :rag query via QEMU monitor
# - Verifies top hit contains the expected doc text

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

STAGE_DIR="${STAGE_DIR:-$WORK_DIR/rag-headless-fat}"
MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-rag-headless.sock}"
LOG_FILE="${LOG_FILE:-$WORK_DIR/rag-headless.log}"
PID_FILE="${PID_FILE:-$WORK_DIR/rag-headless.pid}"

export STAGE_DIR

QUERY_TEXT="${QUERY_TEXT:-greeting volume}"
EXPECT_DOC="${EXPECT_DOC:-doc: greeting from volume}"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"

BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi}"
KERNEL_BIN_SRC="${KERNEL_BIN_SRC:-$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare}"

cleanup() {
  if [ -f "$PID_FILE" ]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      sleep 0.2 || true
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
  rm -f "$PID_FILE" 2>/dev/null || true
  rm -f "$MON_SOCK" 2>/dev/null || true
}
trap cleanup EXIT

rm -f "$MON_SOCK" 2>/dev/null || true
rm -f "$LOG_FILE" 2>/dev/null || true

BUILD_KERNEL="${BUILD_KERNEL:-1}"
QUIET_BUILD="${QUIET_BUILD:-1}"

BUILD_BOOTLOADER="${BUILD_BOOTLOADER:-1}"
if [ "$BUILD_BOOTLOADER" != "0" ]; then
  echo "Building uefi_boot (release)..." >&2
  pushd "$ROOT_DIR/crates/bootloader" >/dev/null
  if [ "$QUIET_BUILD" = "1" ]; then
    RUSTC="$(rustup which rustc)" cargo build --quiet \
      --release \
      --target x86_64-unknown-uefi \
      -p rayos-bootloader >/dev/null
  else
    RUSTC="$(rustup which rustc)" cargo build \
      --release \
      --target x86_64-unknown-uefi \
      -p rayos-bootloader
  fi
  popd >/dev/null
fi

if [ "$BUILD_KERNEL" != "0" ]; then
  echo "Building kernel-bare (release)..." >&2
  pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
  if [ "$QUIET_BUILD" = "1" ]; then
    RUSTC="$(rustup which rustc)" cargo build --quiet \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target x86_64-unknown-none >/dev/null
  else
    RUSTC="$(rustup which rustc)" cargo build \
      -Z build-std=core,alloc \
      -Z build-std-features=compiler-builtins-mem \
      --release \
      --target x86_64-unknown-none
  fi
  popd >/dev/null
fi

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"

if [ -f "$BOOT_EFI_SRC" ]; then
  cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
else
  echo "ERROR: bootloader EFI not found at $BOOT_EFI_SRC" >&2
  exit 1
fi

if [ -f "$KERNEL_BIN_SRC" ]; then
  cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"
else
  echo "ERROR: kernel-bare not found at $KERNEL_BIN_SRC" >&2
  exit 1
fi

# Create embeddings.bin + index.bin deterministically.
python3 - <<'PY'
import math, struct, os

stage = os.environ["STAGE_DIR"]

docs = [
  "doc: greeting from volume",
  "doc: unrelated kernel note",
  "doc: how to use rag command",
]

def fnv1a64(bs: bytes) -> int:
  h = 0xcbf29ce484222325
  prime = 0x00000100000001B3
  for b in bs:
    h ^= b
    h = (h * prime) & 0xFFFFFFFFFFFFFFFF
  return h

def embed8(text: str):
  v = [0.0]*8
  for tok in text.split():
    tok = tok.encode("ascii", "ignore").lower()[:32]
    if not tok:
      continue
    h = fnv1a64(tok)
    idx = h % 8
    sign = -1.0 if (h >> 63) else 1.0
    v[idx] += sign
  ss = sum(x*x for x in v)
  if ss > 0:
    inv = 1.0 / math.sqrt(ss)
    v = [x*inv for x in v]
  return v

emb_path = os.path.join(stage, "EFI", "RAYOS", "embeddings.bin")
idx_path = os.path.join(stage, "EFI", "RAYOS", "index.bin")

# embeddings.bin
out = bytearray()
out += b"EMB0"
out += struct.pack("<I", 1)      # version
out += struct.pack("<I", 8)      # dim
out += struct.pack("<I", len(docs))
for d in docs:
  db = d.encode("utf-8")
  out += struct.pack("<I", len(db))
  out += db
  vec = embed8(d)
  out += struct.pack("<" + "f"*8, *vec)

with open(emb_path, "wb") as f:
  f.write(out)

# index.bin: simple undirected ring (M=4) so every node is reachable.
count = len(docs)
M = 4
entry = 0
neighbors = []
for i in range(count):
  n1 = (i + 1) % count
  n2 = (i - 1 + count) % count
  # pad with UINT32_MAX
  neighbors.extend([n1, n2, 0xFFFFFFFF, 0xFFFFFFFF])

idx = bytearray()
idx += b"HNS0"
idx += struct.pack("<I", 1)          # version
idx += struct.pack("<I", count)
idx += struct.pack("<I", M)
idx += struct.pack("<I", entry)
idx += struct.pack("<" + "I"*(count*M), *neighbors)

with open(idx_path, "wb") as f:
  f.write(idx)

print("Wrote", emb_path, "and", idx_path)
PY

if [ ! -f "$OVMF_CODE" ]; then
  echo "ERROR: OVMF_CODE not found at $OVMF_CODE" >&2
  exit 1
fi

# Start QEMU headless, capture serial to a log file.
"$QEMU_BIN" \
  -machine q35,graphics=on,i8042=on \
  -m 2048 \
  -smp 2 \
  -rtc base=utc,clock=host \
  -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
  -drive file="fat:rw:$STAGE_DIR",format=raw \
  -serial "file:$LOG_FILE" \
  -monitor "unix:$MON_SOCK,server,nowait" \
  -vga std \
  -display none \
  -no-reboot \
  -net none \
  >"$WORK_DIR/qemu-rag-headless.log" 2>&1 &

PID=$!
echo "$PID" > "$PID_FILE"

# Wait for monitor socket.
for _ in $(seq 1 400); do
  if [ -S "$MON_SOCK" ]; then
    break
  fi
  sleep 0.05
done

if [ ! -S "$MON_SOCK" ]; then
  echo "ERROR: QEMU monitor socket did not appear: $MON_SOCK" >&2
  tail -n 200 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

# Wait for boot marker.
BOOT_MARKER='RayOS bicameral loop ready'
for _ in $(seq 1 1600); do
  if [ -f "$LOG_FILE" ] && grep -a -q "$BOOT_MARKER" "$LOG_FILE"; then
    break
  fi
  sleep 0.05
done

if ! grep -a -q "$BOOT_MARKER" "$LOG_FILE"; then
  echo "ERROR: Boot marker not found in log" >&2
  tail -n 240 "$LOG_FILE" 2>/dev/null || true
  exit 1
fi

# Run query and quit.
python3 "$ROOT_DIR/scripts/qemu-sendtext.py" --sock "$MON_SOCK" --text ":rag $QUERY_TEXT" --after 0.8 --quit

# Wait for QEMU to exit.
for _ in $(seq 1 200); do
  if ! kill -0 "$PID" 2>/dev/null; then
    break
  fi
  sleep 0.05
done

# Normalize CRLF.
NORM="$WORK_DIR/rag-headless.norm.log"
tr -d '\r' < "$LOG_FILE" > "$NORM" 2>/dev/null || true

if ! grep -a -q "RAG: top=" "$NORM"; then
  echo "ERROR: did not observe RAG output" >&2
  tail -n 240 "$NORM" 2>/dev/null || true
  exit 1
fi

if ! grep -a -q "$EXPECT_DOC" "$NORM"; then
  echo "ERROR: did not observe expected top doc: $EXPECT_DOC" >&2
  tail -n 240 "$NORM" 2>/dev/null || true
  exit 1
fi

echo "OK: RAG headless smoke test passed. Log: $LOG_FILE" >&2
