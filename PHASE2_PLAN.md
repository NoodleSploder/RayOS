# RayOS Phase 2 Implementation Plan

## Overview

Phase 1 established the bootloader and architecture. Phase 2 focuses on making the kernel systems actually initialize and run on aarch64.

## The Challenge

The kernel is currently compiled for x86_64 (the build machine), but runs on aarch64 (the VM). We need to:

1. Either compile kernel for aarch64-bare-metal, or
2. Embed functional kernel code in the bootloader

## Recommended Approach: Hybrid Phase 2

### Option A: Quick PoC (Embedded Kernel in Bootloader)

**Pros**: Fast, proven architecture
**Cons**: Limited to bootloader constraints (no_std, limited deps)

Steps:

1. Move core kernel logic into bootloader
2. Compile for aarch64-unknown-uefi
3. Initialize System 1 GPU through wgpu-hal
4. Test on VM

### Option B: Full aarch64 Kernel

**Pros**: Proper separation, scalable, real OS
**Cons**: More complex, requires bare-metal dev

Steps:

1. Create `aarch64-unknown-none` kernel target
2. Implement minimal ELF loader in bootloader
3. Compile kernel separately for bare-metal aarch64
4. Load via bootloader

### Option C: Linux Host (Practical for VM)

**Pros**: Simplest, uses Linux kernel
**Cons**: Not custom RayOS kernel

For testing purposes, could use aarch64 Linux kernel + RayOS modules.

## Recommended Next Steps

1. **Test Boot Loop** (5 min)

   - Verify current ISO boots and enters infinite loop
   - Confirm bootloader → kernel transition works
   - No crashes, clean state

2. **GPU Detection** (30 min) - Option A

   - Move GPU initialization code to bootloader
   - Test wgpu-hal initialization on aarch64
   - Log GPU info to console

3. **LLM Integration** (60 min) - Option A

   - Add LLM inference to bootloader context
   - Load a small quantized model
   - Test inference on sample text

4. **Task Orchestration** (30 min)

   - Wire System 1 & System 2 together
   - Implement basic task queue
   - Test task submission and processing

5. **Storage** (20 min)
   - Mount ISO filesystem
   - Load vector embeddings
   - Test RAG retrieval

## Compilation Targets Needed

```
aarch64-unknown-uefi      ✅ (Bootloader) - WORKS
aarch64-unknown-none      ⏳ (Kernel - bare metal) - TO DO
x86_64-pc-windows-msvc    ✅ (Build host) - WORKS
```

## Key Considerations

### Memory Layout

```
0x0000_0000 - Bootloader region
0x0040_0000 - Kernel base (1 MB)
0x0050_0000 - Heap (growing)
0x8000_0000 - UEFI Runtime Services
0xFFFF_XXXX - Stack (high memory)
```

### Exit Boot Services

Currently called in bootloader after kernel load. In Phase 2:

- [ ] Preserve memory map
- [ ] Preserve runtime services pointer
- [ ] Handle page tables properly
- [ ] Set up exception handlers

### No STD Constraints

Bootloader is `#![no_std]` - we can add deps carefully:

- ✅ `wgpu-hal` (low-level GPU, no_std compatible)
- ✅ `glam` (math, no_std)
- ✅ `parking_lot` (sync, no_std)
- ❌ `tokio` (requires std/alloc)

### Solution: Async Without Tokio

Use `embassy` crate (no_std async) or pure event loop.

## Updated Build Process

```powershell
# Phase 2 Build
1. cargo build --release --target aarch64-unknown-uefi  # Bootloader
2. cargo build --release --target aarch64-unknown-none  # Kernel (NEW)
3. Create ISO with both binaries
4. Test on aarch64 VM
```

## Testing Strategy

### Quick Tests

```bash
# Bootloader alone
- Boot ISO, see banner, enters loop
- Bootloader → kernel transition works

# GPU init (Phase 2)
- Boot ISO, see GPU info printed
- System 1 initialized

# LLM init (Phase 2)
- Boot ISO, see LLM model loading
- Test inference on text
- System 2 initialized
```

### Full Integration Test

```bash
# All systems
- Boot ISO
- All 4 systems initialize
- Task submission works
- Metrics printed
- Autonomous loop running
```

## Success Criteria for Phase 2

- [ ] Kernel boots and initializes
- [ ] System 1 GPU detects and initializes
- [ ] System 2 LLM loads and runs inference
- [ ] Conductor task queue operational
- [ ] Volume filesystem mounted
- [ ] All 4 systems running simultaneously
- [ ] Metrics displayed every N seconds
- [ ] Autonomous loop stable >1 minute

## Timeline

- **2 hours**: Option A quick PoC (GPU + LLM in bootloader)
- **4 hours**: Option B full kernel (if choosing bare-metal)
- **8 hours**: Option C with Linux kernel + RayOS modules

## Decision Point

**For fastest progress**: Choose Option A (embedded in bootloader)

- Gets GPU + LLM running quickly
- Proves the architecture works
- Can refactor to proper kernel later

**For production quality**: Choose Option B (proper aarch64 kernel)

- Scalable and proper OS design
- Separates concerns correctly
- Worth the extra complexity

## Files to Modify (Phase 2)

### Option A (Quick)

- `bootloader/uefi_boot/src/main.rs` - Add GPU + LLM init
- `bootloader/uefi_boot/Cargo.toml` - Add wgpu-hal, glam
- `bootloader/.cargo/config.toml` - aarch64 settings (already done)

### Option B (Proper)

- Create `kernel/Cargo.toml` - new `[package.metadata]`
- Create `kernel/.cargo/config.toml` - for aarch64-unknown-none
- Create `kernel/src/start.s` - aarch64 assembly entry
- `bootloader/uefi_boot/src/main.rs` - ELF loader
- `build-iso-aarch64.ps1` - compile both targets

## Next Command

When ready to proceed:

```powershell
# Rebuild Phase 2 (Option A - quick PoC)
cd bootloader/uefi_boot
cargo build --release --target aarch64-unknown-uefi

# Test on VM
# Mount iso-output/rayos-aarch64.iso in aarch64 UEFI VM
# Boot and observe
```

---

**Status**: Phase 1 Complete ✅
**Next**: Phase 2 Implementation (choose Option A or B above)
**Blockers**: None - ready to proceed
