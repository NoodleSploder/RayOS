# RayOS AI Agent Progress Log

Last updated: 2025-12-28

This file is the crash-safe “source of truth” for what the agent has done and what to do next.

## Current focus (highest value / least risk)
- Improve developer + agent onboarding and repeatability.
- Capture correct build/test workflows for Linux QEMU headless tests.
- Keep Phase 2 bring-up tasks tracked (aarch64 runtime, GPU init, LLM init, task wiring).

## What I learned from the plans/TODOs
- The repo’s active Phase 2 path is **Option A**: do quick bring-up in the UEFI bootloader on aarch64.
- Linux smoke testing focuses on **x86_64 `kernel-bare` under OVMF** via scripts:
  - `./scripts/test-boot-headless.sh` (boot markers)
  - `./scripts/test-boot-local-ai-headless.sh` (in-guest local AI)
  - `./scripts/test-boot-ai-headless.sh` (host AI bridge via `conductor` `ai_bridge`)
- Bootloader toolchain is pinned to **nightly-2024-11-01** via `bootloader/rust-toolchain.toml`.

## Completed
- 2025-12-28: Read Phase 1/2 docs and consolidated TODOs from `RAYOS_TODO.md`.
- 2025-12-28: Added AI agent guidance in `.github/copilot-instructions.md`.
- 2025-12-28: Replaced placeholder READMEs in `bootloader/` and `conductor/`.
- 2025-12-28: Verified smoke tests: `test-boot-headless.sh`, `test-boot-local-ai-headless.sh`, `test-boot-ai-headless.sh`.
- 2025-12-28: Verified ISO build: `./scripts/build-iso.sh --arch aarch64` produced `build/rayos-aarch64.iso`.
- 2025-12-28: Bootloader Phase 2 Option A bring-up: added aarch64 “embedded mode” post-`ExitBootServices` fallback when kernel load fails.
- 2025-12-28: Fixed missing framebuffer helper `draw_hex_u64` used by the embedded loop.
- 2025-12-28: Added aarch64 PL011 UART post-exit logging + a serial-only bring-up path when GOP/framebuffer is missing (headless QEMU).
- 2025-12-28: Added `test-boot-aarch64-headless.sh` to validate reaching the post-exit embedded loop under QEMU/aarch64.
- 2025-12-28: Embedded mode now includes a minimal UART command loop + tiny task queue and can checksum `model.bin` bytes post-exit (FNV-1a over first 64KiB).
- 2025-12-28: `test-boot-aarch64-headless.sh` now stages `build/model.bin` (if present) and asserts the checksum line is printed.
- 2025-12-28: Moved `panic = "abort"` into the bootloader workspace root profiles so `uefi_boot` builds correctly (member profile settings are ignored in a workspace).
- 2025-12-28: Verified `uefi_boot` builds for `aarch64-unknown-uefi` and `x86_64-unknown-uefi`; re-ran `./scripts/test-boot-headless.sh` (PASS).

## Quick test runner notes
- `RUN_AARCH64_HEADLESS=1 ./scripts/test-boot.sh` runs the aarch64 headless bring-up test first.

## In progress
- (Optional) Reduce warning noise; not required for functionality.

## Next actions (do these in order)
1) If you want a fresh ISO build:
   - `./scripts/build-iso.sh --arch universal`
   - Tip: install `mtools` to avoid `sudo` mount prompts (`sudo apt-get install mtools`).
2) Re-run validation anytime:
   - `./scripts/test-boot-headless.sh`
   - `./scripts/test-boot-aarch64-headless.sh`
3) Bootloader-only compile checks (when iterating on `uefi_boot/src/main.rs`):
   - `cd bootloader && PATH="$HOME/.cargo/bin:$PATH" RUSTC="$HOME/.cargo/bin/rustc" cargo +nightly-2024-11-01 build -p uefi_boot --target aarch64-unknown-uefi`
   - `cd bootloader && PATH="$HOME/.cargo/bin:$PATH" RUSTC="$HOME/.cargo/bin/rustc" cargo +nightly-2024-11-01 build -p uefi_boot --target x86_64-unknown-uefi`

## Commands to resume (after crash)
From repo root:
1) Check state:
   - `git status`
   - `git diff`
2) Re-run the quick validation:
   - `./scripts/test-boot-headless.sh`
3) If you want the AI bridge flow:
   - `./scripts/test-boot-ai-headless.sh`

## Where to look
- Linux ISO/image build: `scripts/build-iso.sh`
- Headless QEMU tests: `test-boot-*-headless.sh`, `scripts/qemu-sendtext.py`
- Bootloader bring-up: `bootloader/uefi_boot/src/main.rs`
- Bare-metal kernel used for headless tests: `kernel-bare/src/main.rs`
- Host bridge binary: `conductor/src/bin/ai_bridge.rs`
