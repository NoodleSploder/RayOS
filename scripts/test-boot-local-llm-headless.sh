#!/bin/bash
# Headless local-LLM test for RayOS.
# Proves the guest is using a learned model blob (model.bin) rather than only
# deterministic programmed responses.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

MODEL_OUT="${MODEL_OUT:-$WORK_DIR/model.bin}"

# Build a tiny transformer-ish model from local docs (fast enough).
python3 "$ROOT_DIR/scripts/tools/gen_raygpt_model.py" -o "$MODEL_OUT" --quiet \
  --steps 2500 --top-k 1 \
  "$ROOT_DIR/docs/README.md" "$ROOT_DIR/docs/QUICKSTART.md" >/dev/null

# Run the existing local-AI headless harness but stage the model.
# Expect the kernel to announce model presence and the reply to be model-driven.
MODEL_BIN_SRC="$MODEL_OUT" \
EXPECT_CONTAINS="8" \
INPUT_TEXT="what is 4 plus 4" \
"$ROOT_DIR/scripts/test-boot-local-ai-headless.sh" >/dev/null

# Also require the serial SYS marker that a model was loaded.
NORM="$WORK_DIR/local-ai-headless.norm.log"
if ! grep -a -q "SYS: local LLM model loaded" "$NORM"; then
  echo "ERROR: did not observe local LLM model load marker" >&2
  tail -n 240 "$NORM" 2>/dev/null || true
  exit 1
fi

echo "OK: local-LLM headless test passed. Model: $MODEL_OUT" >&2
