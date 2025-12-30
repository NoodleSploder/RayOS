#!/bin/bash
# Headless local-AI prompt matrix for RayOS.
# Runs a suite of prompts against the in-guest local AI responder and asserts
# each case contains an expected substring.
#
# This is a thin runner around ./scripts/test-boot-local-ai-headless.sh.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/build}"
mkdir -p "$WORK_DIR"

QUIET_BUILD="${QUIET_BUILD:-1}"
BUILD_KERNEL_MATRIX="${BUILD_KERNEL_MATRIX:-1}"
BUILD_BOOTLOADER_MATRIX="${BUILD_BOOTLOADER_MATRIX:-1}"

# Optional filter: run only cases whose label contains this substring (case-insensitive).
ONLY="${ONLY:-}"

if [ "$BUILD_KERNEL_MATRIX" != "0" ]; then
  echo "Building kernel-bare once (release)..." >&2
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

if [ "$BUILD_BOOTLOADER_MATRIX" != "0" ]; then
  echo "Building uefi_boot once (release)..." >&2
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

# Matrix format: label|input_text|expect_contains
# Notes:
# - input_text is injected via QEMU monitor, so keep it short.
# - expect_contains is a literal grep pattern; avoid regex special chars if unsure.
CASES=(
  "greeting|hi|AI_LOCAL:"
  "help|help|Local AI:"
  "uptime|how old are you|Uptime"
  "uptime_phrase|what is your up time|Uptime"
  "version|who are you|RayOS"
  "status|status|Status:"
  "memory|memory|Memory: heap_used"
  "devices|what devices do we have|Devices:"
  "day_of_week|what day of the week is it|Weekday"
  "volume|why is volume not found|Volume"
  "files_phrase|how many files do i have|Files:"
  "system1|system 1|System 1:"
  "system2|system 2|System 2:"
)


pass_count=0
skip_count=0
fail_count=0

run_case() {
  local idx="$1"
  local label="$2"
  local input_text="$3"
  local expect_contains="$4"

  local stage_dir="$WORK_DIR/local-ai-matrix-${idx}-fat"
  local mon_sock="$WORK_DIR/qemu-monitor-local-ai-matrix-${idx}.sock"
  local pid_file="$WORK_DIR/local-ai-matrix-${idx}.pid"
  local log_file="$WORK_DIR/local-ai-matrix-${idx}.log"

  echo "[${idx}] ${label}: expecting '${expect_contains}'" >&2

  WORK_DIR="$WORK_DIR" \
  STAGE_DIR="$stage_dir" \
  MON_SOCK="$mon_sock" \
  PID_FILE="$pid_file" \
  LOG_FILE="$log_file" \
  INPUT_TEXT="$input_text" \
  EXPECT_CONTAINS="$expect_contains" \
  BUILD_KERNEL=0 \
  BUILD_BOOTLOADER=0 \
  QUIET_BUILD="$QUIET_BUILD" \
  "$ROOT_DIR/scripts/test-boot-local-ai-headless.sh"
}

idx=0
for row in "${CASES[@]}"; do
  idx=$((idx + 1))

  IFS='|' read -r label input_text expect_contains <<< "$row"

  if [ -n "$ONLY" ]; then
    if ! echo "$label" | tr '[:upper:]' '[:lower:]' | grep -q "$(echo "$ONLY" | tr '[:upper:]' '[:lower:]')"; then
      skip_count=$((skip_count + 1))
      continue
    fi
  fi

  if run_case "$idx" "$label" "$input_text" "$expect_contains"; then
    pass_count=$((pass_count + 1))
  else
    fail_count=$((fail_count + 1))
    echo "FAIL: case '${label}' (log: $WORK_DIR/local-ai-matrix-${idx}.log)" >&2
    exit 1
  fi

done

echo "OK: local-AI matrix passed (${pass_count} passed, ${skip_count} skipped)." >&2
