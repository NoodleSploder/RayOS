#!/bin/bash
# Run the RayOS installer dry-run planner and print JSON output.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST_PATH="$ROOT_DIR/crates/installer/Cargo.toml"

if [ ! -f "$MANIFEST_PATH" ]; then
  echo "FAIL: installer crate manifest not found at $MANIFEST_PATH" >&2
  exit 1
fi

if [[ " $* " != *" --enumerate-local-disks "* ]]; then
  echo "INFO: running installer planner in SAMPLE mode (no local disk enumeration)." >&2
  echo "      Pass --enumerate-local-disks inside a disposable installer VM if you need real hardware data." >&2
fi

cargo run --manifest-path "$MANIFEST_PATH" -- "$@"
