#!/bin/bash
# Run RayOS with UI Shell (graphical mode)
#
# This script builds and runs RayOS with the ui_shell feature enabled
# and displays the graphical output.
#
# For a real Linux VM (requires Linux guest artifacts), use:
#   ./scripts/run-ui-shell-vmm.sh
#
# This script uses dev_scanout to provide a test surface for development.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

# Set features - include ui_shell, serial_debug, and dev_scanout for testing
RAYOS_KERNEL_FEATURES="ui_shell,serial_debug,dev_scanout"
export RAYOS_KERNEL_FEATURES

# Disable headless to show the window
export HEADLESS=0

# Use PS/2 mouse for input (our kernel has PS/2 driver, not USB stack)
# The -show-cursor makes host cursor visible for reference
export QEMU_EXTRA_ARGS="${QEMU_EXTRA_ARGS:-}"

# Build the kernel with UI shell
echo "Building kernel-bare with ui_shell enabled..." >&2
pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
RUSTC="$(rustup which rustc)" cargo build \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release \
  --target x86_64-unknown-none \
  --features "$RAYOS_KERNEL_FEATURES"
popd >/dev/null

echo "Kernel built. Launching with graphical display..." >&2
echo ""

# Run test-boot.sh which handles ISO creation and QEMU launch
BUILD_KERNEL=0 "$ROOT_DIR/scripts/test-boot.sh" || true

echo ""
echo "=== Session ended ===" >&2
