#!/bin/bash
# Graphical boot with host-side AI responses.
#
# This runs QEMU with `-serial stdio` so a host bridge can read what you type
# (via `RAYOS_INPUT:` lines printed by the kernel) and send back `AI:` replies
# that the kernel renders in the Response line.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

MON_SOCK="${MON_SOCK:-$WORK_DIR/qemu-monitor-ai.sock}"
rm -f "$MON_SOCK" 2>/dev/null || true

STAGE_DIR="${STAGE_DIR:-$WORK_DIR/boot-fat}"
BOOT_EFI_SRC="${BOOT_EFI_SRC:-$ROOT_DIR/crates/bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi}"
KERNEL_BIN_SRC="${KERNEL_BIN_SRC:-$ROOT_DIR/crates/kernel-bare/target/x86_64-unknown-none/release/kernel-bare}"

# Build kernel-bare first so we always boot the latest code.
# Set BUILD_KERNEL=0 to skip.
BUILD_KERNEL="${BUILD_KERNEL:-1}"
if [ "$BUILD_KERNEL" != "0" ]; then
  echo "Building kernel-bare (release)..."

  # Optional: pass extra kernel Cargo features without editing the script.
  # Example:
  #   RAYOS_KERNEL_FEATURES=dev_scanout ./scripts/test-boot-ai.sh
  RAYOS_KERNEL_FEATURES="${RAYOS_KERNEL_FEATURES:-}"
  EXTRA_FEATURE_ARGS=()
  if [ -n "$RAYOS_KERNEL_FEATURES" ]; then
    EXTRA_FEATURE_ARGS=(--features "$RAYOS_KERNEL_FEATURES")
  fi

  pushd "$ROOT_DIR/crates/kernel-bare" >/dev/null
  RUSTC="$(rustup which rustc)" cargo build \
    -Z build-std=core,alloc \
    -Z build-std-features=compiler-builtins-mem \
    --release \
    --target x86_64-unknown-none \
    --no-default-features --features host_ai \
    "${EXTRA_FEATURE_ARGS[@]}"
  popd >/dev/null
fi

rm -rf "$STAGE_DIR" 2>/dev/null || true
mkdir -p "$STAGE_DIR/EFI/BOOT" "$STAGE_DIR/EFI/RAYOS"
if [ -f "$BOOT_EFI_SRC" ]; then
  cp "$BOOT_EFI_SRC" "$STAGE_DIR/EFI/BOOT/BOOTX64.EFI"
else
  echo "warning: bootloader EFI not found at $BOOT_EFI_SRC" >&2
fi

if [ -f "$KERNEL_BIN_SRC" ]; then
  cp "$KERNEL_BIN_SRC" "$STAGE_DIR/EFI/RAYOS/kernel.bin"
else
  echo "warning: kernel-bare not found at $KERNEL_BIN_SRC" >&2
fi

echo "Starting RayOS (graphical) with AI bridge..."

# Build ai_bridge with Ollama support so we can use a real LLM by default.
# If Ollama isn't running (or no models are installed), the bridge falls back to
# the built-in template responder.
FEATURES="ai,ai_ollama"

# Run the bridge, which spawns QEMU. We must use -serial stdio so the bridge can reply.
cd "$ROOT_DIR/crates/conductor"
cargo run --features "$FEATURES" --bin ai_bridge -- \
  qemu-system-x86_64 \
    -machine q35 \
    -m 2048 \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive file="fat:rw:$STAGE_DIR",format=raw \
    -serial stdio \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -vga std \
    -display gtk,zoom-to-fit=on
