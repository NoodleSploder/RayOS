# RayOS Phase 2 Implementation Plan

## Overview

Phase 1 established the bootloader and architecture. Phase 2 focuses on making the kernel systems actually initialize and run on aarch64.

## The Challenge

The kernel is currently compiled for x86_64 (the build machine), but runs on aarch64 (the VM). We need to:

1. Either compile kernel for aarch64-bare-metal, or
2. Embed functional kernel code in the bootloader

## Recommended Approach: Hybrid Phase 2 (now focusing on Option D)

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

### Option D: Linux Subsystem (Wayland-first, Linux is a guest)

Goal: run Linux as a **subsystem of RayOS** (RayOS is the host OS and policy authority), while still getting a full graphical Linux userspace.

**Pros**: Real Linux app compatibility, strong isolation boundary (VM), Wayland-first performance, RayOS remains “the OS”
**Cons**: More moving parts (KVM/virtio/device model), graphics bridge complexity

Subsystem contract (non-negotiable for this option):
- **RayOS is the sole authority**: boot, orchestration, policy, storage/volume, intent, and *all I/O* (keyboard, mouse, graphics presentation, serial/terminal).
- Linux is a managed guest runtime (started/stopped by RayOS, resource-governed, device exposure is policy-gated).
- Linux does not “become the OS”; it is an app-compatibility environment fully driven by RayOS.
- Authoritative contract: [LINUX_SUBSYSTEM_CONTRACT.md](LINUX_SUBSYSTEM_CONTRACT.md)

Preferred graphics model:
- Wayland-first: Linux guests render to **Wayland surfaces**.
- RayOS compositor embeds those surfaces.

Chosen UX (from plan discussion):
1) Linux desktop appears **embedded** inside RayOS (one surface/window) for baseline compatibility.
2) Linux apps can appear as **native RayOS windows** (multiple surfaces) rather than a single monolithic desktop.

Implementation sketch (high-level):
- Run Linux in a VM (KVM when available), using virtio devices.
- Graphics via virtio-gpu + a Wayland bridge (e.g., virtio-wayland style plumbing).
- Input via virtio-input (RayOS routes events).
- Files via virtiofs (RayOS volume is authoritative).
- App lifecycle: RayOS launches Linux apps through a controlled command channel and maps their Wayland surfaces into the RayOS compositor.

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

### Option D Next Steps (Linux Subsystem track)

If focusing on Option D, the next steps shift from “boot-only bring-up” to “compatibility subsystem”:

1. Define the Linux subsystem contract (RayOS is host/policy authority)
2. Bring up a minimal Linux VM headless with virtio devices
3. Add Wayland-first graphics bridging (embedded desktop surface first)
4. Add native-window mapping (Linux apps as RayOS windows)
5. Add a command channel so Intent can launch Linux apps
6. Add an automated smoke test: start guest → spawn Wayland client → verify surface → shutdown

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
- **(TBD)**: Option D Linux subsystem (VM + Wayland bridge; expected multi-day effort)

## Decision Point

**For fastest progress**: Choose Option A (embedded in bootloader)

- Gets GPU + LLM running quickly
- Proves the architecture works
- Can refactor to proper kernel later

**Current focus (this repo)**: Option D is the active Phase 2 focus (Linux as a RayOS subsystem).

Notes:
- Option A remains a valuable bring-up path for firmware/boot/kernel experiments.
- Option D is the preferred path for “Wayland GUI + app compatibility while RayOS remains the OS”.

Progress notes:
- Bootloader System 1: GOP-based “GPU present” probe + device-path node logging (best-effort PCI node decode).
- Bootloader System 2: Optional `EFI\\RAYOS\\model.bin` blob is loaded pre-`ExitBootServices` and passed to the kernel via `BootInfo`.

**For production quality**: Choose Option B (proper aarch64 kernel)

- Scalable and proper OS design
- Separates concerns correctly
- Worth the extra complexity

**For app compatibility while keeping RayOS as host**: Choose Option D (Linux subsystem)

- Linux runs as a managed guest runtime (not the host OS)
- Wayland-first graphics bridge to RayOS compositor
- Enables embedded desktop + native-window mapping roadmap

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
# Mount build/rayos-aarch64.iso in aarch64 UEFI VM
# Boot and observe
```

---

**Status**: Phase 1 Complete ✅
**Next**: Phase 2 Implementation (choose Option A or B above)
**Blockers**: None - ready to proceed
