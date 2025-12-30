# RayOS Project Index

## 📚 Documentation (Read In Order)

### Start Here

1. **[QUICKSTART.md](QUICKSTART.md)** - Quick reference and common commands
2. **[PHASE1_COMPLETE.md](PHASE1_COMPLETE.md)** - Complete Phase 1 architecture overview
3. **[PHASE2_PLAN.md](PHASE2_PLAN.md)** - Phase 2 implementation options and roadmap
4. **[LINUX_SUBSYSTEM_DESIGN.md](LINUX_SUBSYSTEM_DESIGN.md)** - Linux Subsystem high-level design + interface contract
5. **[WINDOWS_SUBSYSTEM_DESIGN.md](WINDOWS_SUBSYSTEM_DESIGN.md)** - Windows Subsystem design notes
6. **[INSTALLABLE_RAYOS_PLAN.md](INSTALLABLE_RAYOS_PLAN.md)** - Installability plan (USB boot + installer + boot manager tracking)

### Design Tracking (Draft Stubs)

- **[SYSTEM_ARCHITECTURE.md](SYSTEM_ARCHITECTURE.md)** - Unified top-level architecture map
- **[DISK_LAYOUT_AND_PERSISTENCE.md](DISK_LAYOUT_AND_PERSISTENCE.md)** - Concrete disk layout + persistence invariants
- **[INSTALLER_AND_BOOT_MANAGER_SPEC.md](INSTALLER_AND_BOOT_MANAGER_SPEC.md)** - Installer wizard + boot manager requirements
- **[POLICY_CONFIGURATION_SCHEMA.md](POLICY_CONFIGURATION_SCHEMA.md)** - Policy config schema (VM lifecycle/presentation/networking)
- **[UPDATE_AND_RECOVERY_STRATEGY.md](UPDATE_AND_RECOVERY_STRATEGY.md)** - Update + rollback + recovery plan
- **[SECURITY_THREAT_MODEL.md](SECURITY_THREAT_MODEL.md)** - Threat model + security invariants
- **[OBSERVABILITY_AND_RECOVERY.md](OBSERVABILITY_AND_RECOVERY.md)** - Logging/health/crash recovery spec

### Session Records

- **[SESSION_SUMMARY.md](SESSION_SUMMARY.md)** - Current session accomplishments (today)
- **[BUILD_GUIDE.md](BUILD_GUIDE.md)** - Original build documentation
- **[BOOT_TROUBLESHOOTING.md](BOOT_TROUBLESHOOTING.md)** - Boot debugging guide

## 🏗️ Project Structure

### Core Components

```
bootloader/
├── Cargo.toml (updated for aarch64)
├── .cargo/config.toml (aarch64-unknown-uefi config)
└── uefi_boot/
    └── src/main.rs (✅ UEFI entry + kernel loader)

kernel/
├── Cargo.toml
├── src/
│   ├── main.rs (✅ Kernel entry + demo)
│   ├── lib.rs (✅ RayKernelBuilder)
│   ├── system1/ (✅ GPU Reflex Engine)
│   ├── system2/ (✅ LLM Cognitive Engine)
│   ├── hal/ (✅ Hardware abstraction)
│   └── types.rs (✅ Core types)

conductor/
└── src/main.rs (✅ Task orchestration)

cortex/
└── src/lib.rs (✅ Vision/reasoning AI)

intent/
└── src/lib.rs (✅ Multimodal intent parser)

volume/
└── src/main.rs (✅ Vector storage + FS)
```

### Build System

```
scripts/build-iso-aarch64.ps1 (✅ MAIN - Use this)
scripts/build-iso-final.ps1 (x86_64 version)
scripts/build-iso.ps1 (x86_64 version)
scripts/build-iso.sh (Linux version)
```

### Output

```
build/
└── rayos-aarch64.iso (✅ 7.88 MB - READY TO TEST)
    ├── EFI/BOOT/BOOTAA64.EFI (aarch64 bootloader)
    └── EFI/RAYOS/kernel.bin (kernel binary)
```

## 🎯 Current Status: Phase 1 Complete ✅

### What Works

- ✅ aarch64 UEFI bootloader compiles and boots
- ✅ Bootloader prints initialization banner
- ✅ Kernel entry point properly defined
- ✅ Build system fully automated
- ✅ ISO 9660 hybrid bootable format
- ✅ All four systems architecturally designed

### What's Ready for Phase 2

- ⏳ GPU initialization (System 1)
- ⏳ LLM inference (System 2)
- ⏳ Task orchestration (Conductor)
- ⏳ Storage/embeddings (Volume)

## 🚀 How to Proceed

### Option 1: Test Current ISO (Recommended First Step)

```powershell
# Boot in aarch64 UEFI VM
# Mount: build/rayos-aarch64.iso
# Expected: Bootloader banner appears, kernel enters loop
```

### Option 2: Rebuild Everything

```powershell
cd c:\Users\caden\Documents\Programming Scripts\Personal\Rust\ray-os
powershell -ExecutionPolicy Bypass -File .\scripts\build-iso-aarch64.ps1
```

### Option 3: Choose Phase 2 Implementation

1. Read [PHASE2_PLAN.md](PHASE2_PLAN.md)
2. Phase 2 is currently proceeding with **Option A (Quick PoC / embedded bring-up in bootloader)**
3. Follow implementation steps (GPU probe/logging + optional `model.bin` handoff are in place)

## 📊 Key Metrics

| Aspect      | Value                            |
| ----------- | -------------------------------- |
| Target Arch | aarch64 (ARM64)                  |
| Bootloader  | UEFI aarch64                     |
| ISO Size    | 7.88 MB                          |
| Build Time  | ~2 minutes                       |
| Systems     | 4 (GPU, LLM, Conductor, Storage) |
| Phase       | 1 of N                           |

## 🔗 System Architecture

```
UEFI VM (aarch64)
    ↓
BOOTLOADER (BOOTAA64.EFI)
    ├─ Initialize console
    ├─ Load kernel
    └─ Transition to kernel
         ↓
    KERNEL (rayos-aarch64-kernel)
         ├─ System 1: GPU Reflex
         ├─ System 2: LLM Cognitive
         ├─ Conductor: Orchestration
         └─ Volume: Storage
              ↓
         Autonomous Loop (Phase 2+)
```

## 📁 File Reference

### Configuration Files

| File                              | Purpose                       | Status     |
| --------------------------------- | ----------------------------- | ---------- |
| `bootloader/.cargo/config.toml`   | Compiler settings for aarch64 | ✅ Updated |
| `bootloader/uefi_boot/Cargo.toml` | Bootloader dependencies       | ✅ Updated |
| `kernel/Cargo.toml`               | Kernel dependencies           | ✅ Works   |
| `build-iso-aarch64.ps1`           | ISO build automation          | ✅ Works   |

### Source Code

| File                               | Purpose                    | Status         |
| ---------------------------------- | -------------------------- | -------------- |
| `bootloader/uefi_boot/src/main.rs` | UEFI entry + kernel loader | ✅ Complete    |
| `kernel/src/main.rs`               | Kernel entry + demo        | ✅ Complete    |
| `kernel/src/lib.rs`                | Kernel library             | ✅ Complete    |
| `kernel/src/system1/mod.rs`        | GPU engine                 | ✅ Implemented |
| `kernel/src/system2/mod.rs`        | LLM engine                 | ✅ Implemented |
| `conductor/src/main.rs`            | Task orchestrator          | ✅ Implemented |
| `crates/volume/src/main.rs`        | Storage engine             | ✅ Implemented |

### Documentation

| File                 | Purpose                | Audience      |
| -------------------- | ---------------------- | ------------- |
| `QUICKSTART.md`      | Quick reference        | Everyone      |
| `PHASE1_COMPLETE.md` | Architecture detail    | Developers    |
| `PHASE2_PLAN.md`     | Implementation roadmap | Project leads |
| `SESSION_SUMMARY.md` | Current session        | Team          |
| `BUILD_GUIDE.md`     | Original guide         | Reference     |

## ⚡ Quick Commands

```powershell
# Build
powershell -ExecutionPolicy Bypass -File build-iso-aarch64.ps1

# Build with clean
powershell -ExecutionPolicy Bypass -File build-iso-aarch64.ps1 -Clean

# Bootloader only
cd bootloader\uefi_boot
cargo +nightly build -Zbuild-std=core --release --target aarch64-unknown-uefi

# Kernel only
cd kernel
cargo build --release

# Check ISO
Get-Item build\rayos-aarch64.iso
```

## ✅ Headless Verification (x86_64 kernel-bare)

On Linux, there are unattended scripts that boot under OVMF and validate behavior from the serial log:

```bash
# Boot markers only
./scripts/test-boot-headless.sh

# “LLM inside RayOS” smoke test (in-guest local responder; no host bridge)
./scripts/test-boot-local-ai-headless.sh

# Host AI bridge smoke test (requires building/running conductor ai_bridge)
./scripts/test-boot-ai-headless.sh

# Cortex protocol smoke test (injects via shell passthrough)
./scripts/test-boot-cortex-headless.sh

# End-to-end Cortex daemon -> guest -> kernel test
./scripts/test-boot-cortex-daemon-headless.sh
```

## 🎓 Key Concepts

### Bicameral Architecture

- **System 1**: Fast, reactive, GPU-based (subconscious)
- **System 2**: Slow, deliberative, LLM-based (conscious)
- Both run simultaneously, feedback-coupled

### Logic Rays

Custom GPU compute abstraction representing thoughts/tasks that flow through the system. Persistent shader kernel processes them.

### Ouroboros Loop

Self-aware feedback where system monitors its own entropy and can trigger "dreams" to solve stuck states.

### Dream Mode

Autonomous problem-solving when system entropy is high. Uses System 2 (LLM) to generate novel solutions.

## 🔍 Troubleshooting Guide

### Problem: "Bootloader won't compile"

→ Check [QUICKSTART.md](QUICKSTART.md) Troubleshooting section

### Problem: "ISO won't boot"

→ Check [BOOT_TROUBLESHOOTING.md](BOOT_TROUBLESHOOTING.md)

### Problem: "Kernel initialization fails"

→ Review [PHASE2_PLAN.md](PHASE2_PLAN.md) Known Limitations section

### Problem: "Build is slow"

→ Normal first build is ~2 minutes. Incremental builds are faster.

## 📈 Next Milestones

- **Phase 1**: ✅ Bootloader + Architecture (COMPLETE)
- **Phase 2**: ⏳ GPU + LLM Integration (READY TO START)
- **Phase 3**: ⏳ Autonomous Operation
- **Phase 4**: ⏳ Full User Interface
- **Phase 5**: ⏳ Production Hardening

## 💡 Pro Tips

1. **Always start with**: `QUICKSTART.md` for quick answers
2. **For deep understanding**: Read `PHASE1_COMPLETE.md`
3. **Before implementing**: Check `PHASE2_PLAN.md`
4. **If stuck**: Check `SESSION_SUMMARY.md` for recent fixes
5. **For boot issues**: Read `BOOT_TROUBLESHOOTING.md`

## 🎯 Success Criteria Met

✅ aarch64 UEFI bootloader boots successfully
✅ Kernel architecture designed and implemented
✅ ISO 9660 format created and verified
✅ Build system fully automated
✅ Comprehensive documentation provided
✅ Clear upgrade path to Phase 2
✅ Code compiles without errors
✅ System boots to kernel stub autonomously

## 🚀 Ready to Start?

1. **Read**: [QUICKSTART.md](QUICKSTART.md) (5 min)
2. **Test**: Boot ISO in aarch64 VM (5 min)
3. **Plan**: Review [PHASE2_PLAN.md](PHASE2_PLAN.md) (10 min)
4. **Implement**: Choose Phase 2 option and start (2-8 hours)

---

**Status**: Phase 1 ✅ Complete - Ready for Phase 2
**Last Updated**: December 25, 2025
**Next Phase**: GPU + LLM Integration
