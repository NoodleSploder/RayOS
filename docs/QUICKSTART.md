# RayOS Quick Reference - Phase 1 Complete

**Note:** This document is historically useful but **out of date** relative to the current repo layout and the current “Option D / Linux-subsystem-first” direction.

For current build/boot commands, start from:
- `README.MD`
- `docs/RAYOS_TODO.md`

Headless smoke tests (current behavior):
- Most headless scripts stop QEMU by sending `quit` to the QEMU monitor socket once a PASS/SKIP marker appears (instead of `timeout`-killing QEMU).
- Shared helper: `scripts/lib/headless_qemu.sh`.

Linux desktop (dev harness, current behavior):
- `show linux desktop` works only when the host bridge is enabled in `./scripts/test-boot.sh`.
- Recommended for the “Linux hidden at boot, show only presents” lifecycle:
      - `ENABLE_HOST_DESKTOP_BRIDGE=1 PRELAUNCH_HIDDEN_DESKTOPS=1 ./scripts/test-boot.sh`

## 🚀 Current Status
- ✅ aarch64 UEFI bootloader boots successfully
- ✅ Kernel architecture fully designed and implemented
- ✅ ISO ready for testing
- ⏳ Phase 2: GPU/LLM initialization pending

## 📁 Key Files

### Bootloader (aarch64-unknown-uefi)
- `bootloader/uefi_boot/src/main.rs` - Entry point and kernel loader
- `bootloader/uefi_boot/Cargo.toml` - Dependencies (uefi 0.13.0)
- `bootloader/.cargo/config.toml` - Compiler configuration

### Kernel (x86_64, needs aarch64 in Phase 2)
- `kernel/src/main.rs` - Kernel entry and demo
- `kernel/src/lib.rs` - Kernel builder and initialization
- `kernel/src/system1/` - GPU Reflex Engine
- `kernel/src/system2/` - LLM Cognitive Engine
- `conductor/src/` - Task orchestration
- `crates/volume/src/` - Persistent storage

### Build & ISO
- `scripts/build-iso-aarch64.ps1` - Automated build script
- `build/rayos-aarch64.iso` - Final bootable ISO (7.88 MB)

### Documentation
- `PHASE1_COMPLETE.md` - Full Phase 1 architecture and details
- `PHASE2_PLAN.md` - Implementation plan with 3 options
- `SESSION_SUMMARY.md` - This session's accomplishments

## 🔧 Common Commands

### aarch64 headless smoke tests (Linux)

```bash
# Bootloader embedded-mode (forces fallback by removing kernel.bin)
./scripts/test-boot-aarch64-headless.sh

# Boot real aarch64 bare kernel via bootloader ELF load+jump (Option B bring-up)
./scripts/test-boot-aarch64-kernel-headless.sh
```

### x86_64 (kernel-bare) smoke tests (Linux)
```bash
# Headless boot marker check
./scripts/test-boot-headless.sh

# Headless local AI check (no host bridge required)
./scripts/test-boot-local-ai-headless.sh

# Optional: customize what gets typed into the guest
INPUT_TEXT='hello there!' ./scripts/test-boot-local-ai-headless.sh

# Cortex protocol (kernel-side receiver via :cortex injection)
./scripts/test-boot-cortex-headless.sh

# Cortex daemon -> guest integration (host `rayos-cortex` injects via QEMU monitor)
./scripts/test-boot-cortex-daemon-headless.sh

```

### Cortex → kernel-bare (host injection)

The host-side `rayos-cortex` binary supports a minimal “send one line and exit” mode.

```bash
# Requires a running QEMU with a monitor socket configured
RAYOS_QEMU_MONITOR_SOCK=/path/to/qemu-monitor.sock \
RAYOS_CORTEX_TEST_LINE='CORTEX:INTENT kind=delete target=demo_file' \
      cargo run --quiet -p rayos-cortex
```

### Hardware gaze input (UDP bridge)

If you have a hardware eye tracker (or just want a demo input), you can feed Cortex gaze over UDP.

```bash
export RAYOS_GAZE_UDP_ADDR=127.0.0.1:5555

# Demo: use X11 mouse cursor as gaze
./scripts/tools/gaze-udp-bridge.py --dest 127.0.0.1:5555 --source mouse
```

### Build
```powershell
# Full build (bootloader + kernel + ISO)
powershell -ExecutionPolicy Bypass -File build-iso-aarch64.ps1

# Just bootloader
cd bootloader/uefi_boot
cargo +nightly build -Zbuild-std=core --release --target aarch64-unknown-uefi

# Just kernel
cd kernel
cargo build --release
```

### Test
```powershell
# Mount ISO in aarch64 UEFI VM and boot
# Should see bootloader banner and kernel entry
```

### Clean
```powershell
# Remove build artifacts
powershell -ExecutionPolicy Bypass -File build-iso-aarch64.ps1 -Clean
```

## 🎯 Architecture Overview

```
┌─────────────────────────────────────┐
│     aarch64 UEFI Firmware           │
├─────────────────────────────────────┤
│  BOOTLOADER (aarch64-unknown-uefi)  │
│  - Print banner                     │
│  - Load kernel                      │
│  - Exit boot services               │
│  - Jump to kernel entry             │
├─────────────────────────────────────┤
│      KERNEL STUB (Phase 1)          │
│  - Ready for System 1 init          │
│  - Ready for System 2 init          │
│  - Autonomous loop (tick counter)   │
└─────────────────────────────────────┘
         ↓
   PHASE 2: Add GPU/LLM
```

## 📊 Four Core Systems

### System 1: Reflex Engine (GPU)
- Hardware abstraction layer (wgpu)
- Persistent shader kernel
- Logic ray processing
- Multi-GPU hive coordination
- Status: ✅ Code implemented, ⏳ Needs GPU init

### System 2: Cognitive Engine (LLM)
- Intent parser (multimodal: text + gaze)
- Policy arbiter
- Context manager
- LLM inference pipeline
- Status: ✅ Code implemented, ⏳ Needs model loading

### Conductor: Orchestration
- Task queue management
- Entropy monitoring
- Ouroboros feedback loop (self-aware)
- Dream mode trigger
- Load balancing
- Status: ✅ Code implemented, ⏳ Needs task wiring

### Volume: Storage
- Vector embeddings (semantic memory)
- Filesystem integration
- RAG (Retrieval-Augmented Generation)
- Persistent state
- Status: ✅ Code implemented, ⏳ Needs FS mount

## 🔄 Boot Sequence

1. **UEFI Firmware** loads BOOTAA64.EFI from ISO
2. **Bootloader** prints initialization banner
3. **Bootloader** calls `load_kernel_binary()`
4. **Bootloader** loads kernel.bin from ISO
5. **Bootloader** calls `system_table.exit_boot_services()`
6. **Bootloader** jumps to `kernel_entry_stub()`
7. **Kernel** enters infinite loop (autonomous)

## 📈 Performance Targets (Phase 2+)

| Metric | Target |
|--------|--------|
| GPU throughput | 1000+ rays/frame @ 60 FPS |
| LLM latency | <100ms inference |
| Task overhead | <1ms per task |
| Dream trigger | Entropy > 0.7 |
| Feedback loop | <1s detection |

## ⚡ Phase 2 Decision Points

Choose ONE:

**Option A: Quick PoC (2-4 hours)**
- Embed GPU/LLM in bootloader
- Compile for aarch64-unknown-uefi
- Test immediately on VM

**Option B: Proper Kernel (4-8 hours)**
- Create aarch64-unknown-none kernel
- Implement ELF loader in bootloader
- Proper OS separation

**Option C: Linux-based (8+ hours)**
- Use aarch64 Linux kernel + RayOS modules
- Simplest but less custom

**Recommended**: Option A for fastest proof, then B for production.

## 🔍 Troubleshooting

### Bootloader won't compile
```
→ Check .cargo/config.toml has aarch64 section
→ Verify uefi_boot/Cargo.toml has harness = false
→ Try: cargo clean && rebuild
```

### Kernel compilation fails
```
→ Check metrics field names match types.rs
→ Verify all dependencies are in Cargo.toml
→ Look for unused variable/import warnings
```

### ISO won't boot
```
→ Verify file format: file rayos-aarch64.iso
→ Should be: ISO 9660 ... isohybrid
→ Check VM is aarch64 UEFI capable
→ Verify BOOTAA64.EFI exists in EFI/BOOT/
```

## 📚 Documentation Map

| Document | Purpose | Audience |
|----------|---------|----------|
| PHASE1_COMPLETE.md | Architecture deep-dive | Developers |
| PHASE2_PLAN.md | Implementation roadmap | Project leads |
| SESSION_SUMMARY.md | Session accomplishments | Everyone |
| This file | Quick reference | Quick lookup |

## 🎓 Learning Resources

**aarch64 UEFI**:
- UEFI Specification (www.uefi.org)
- ARM64 Architecture Reference Manual
- wgpu-hal documentation

**Rust OS Development**:
- "Writing an OS in Rust" (philipjohn.me)
- OSDev.org forums
- Rust Embedded Book

**GPU Compute**:
- wgpu documentation
- SPIR-V shader language
- Logic ray algorithms (custom)

## ✅ Validation Checklist

Before proceeding to Phase 2, verify:
- [ ] ISO boots on aarch64 UEFI VM
- [ ] Bootloader banner prints to console
- [ ] System enters kernel without crash
- [ ] No build errors or warnings (except harmless)
- [ ] All code compiles cleanly
- [ ] Documentation is up-to-date

## 🚀 Next Steps

1. **Test ISO** (5 min) - Boot and verify output
2. **Choose Phase 2 option** (A/B/C) - From PHASE2_PLAN.md
3. **Implement GPU init** (Option A: 30 min, Option B: 90 min)
4. **Test GPU detection** - Verify wgpu-hal works
5. **Implement LLM** - Load model, test inference
6. **Wire orchestration** - Connect System 1 & 2
7. **Test autonomy** - Run full loop, check metrics

## 📞 Support

For issues or questions:
1. Check relevant documentation file
2. Review comments in source code
3. Check build-iso-aarch64.ps1 output for errors
4. Verify architecture matches (aarch64 vs x86_64)

## 🏁 Summary

**Phase 1 ✅**: Bootloader boots, architecture defined, build automated
**Phase 2 ⏳**: GPU + LLM integration (ready to start)
**Phase 3+**: Full autonomous operation with all subsystems

---

**Ready to proceed?** Start with ISO test, then choose Phase 2 option from PHASE2_PLAN.md
