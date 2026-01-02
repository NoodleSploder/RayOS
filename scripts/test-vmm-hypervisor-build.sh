#!/bin/bash
# Build-only smoke test for the in-kernel hypervisor skeleton (kernel-bare feature: vmm_hypervisor).
#
# This script compiles the kernel-bare crate with the feature enabled using
# -Z build-std (same approach as other kernel-bare tests).
#
# It will SKIP (exit 0) when a nightly rustc is not available in the environment.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! rustc --version | grep -qi "nightly"; then
  echo "SKIP: nightly rustc not available; cannot run -Z build-std build" >&2
  exit 0
fi

echo "Building kernel-bare with vmm_hypervisor feature (release)..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null

cargo build \
  --release \
  --features vmm_hypervisor \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  >/dev/null

popd >/dev/null

echo "PASS: kernel-bare builds with vmm_hypervisor" >&2
