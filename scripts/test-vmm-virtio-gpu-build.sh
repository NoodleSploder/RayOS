#!/bin/bash
# Build-only smoke test for the in-kernel VMM virtio-gpu device-model scaffolding.
#
# This does NOT boot a guest; it only ensures the feature-gated code compiles
# with the same build-std settings used by other kernel-bare tests.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# This repo's kernel-bare build (for `x86_64-unknown-none`) typically relies on
# a nightly `rustc` with `-Z build-std`. Some CI/dev containers ship only a
# stable `rustc` without rustup.
if ! rustc --version | grep -qi "nightly"; then
  echo "SKIP: nightly rustc not available; cannot run -Z build-std build" >&2
  exit 0
fi

echo "Building kernel-bare with vmm_virtio_gpu feature (release)..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null

cargo build \
  --release \
  --features vmm_virtio_gpu \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  >/dev/null

popd >/dev/null

echo "PASS: kernel-bare builds with vmm_virtio_gpu" >&2
