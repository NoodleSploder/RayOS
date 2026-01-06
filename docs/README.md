# RayOS Project Index

This file is a **documentation index**.

- For the current build/boot/test entrypoints, start with **[../README.MD](../README.MD)**.
- For the authoritative roadmap/status (Linux subsystem + in-OS VMM), see **[RAYOS_TODO.md](RAYOS_TODO.md)**.

## 📚 Documentation (Read In Order)

### Start Here

0. **[../README.MD](../README.MD)** - Current build/boot + VMM/hypervisor smoke entrypoints
1. **[RAYOS_TODO.md](RAYOS_TODO.md)** - Current roadmap/status (including Linux subsystem + hypervisor track)

2. **[QUICKSTART.md](QUICKSTART.md)** - Historical quick reference (some sections are out of date)
3. **[PHASE1_COMPLETE.md](PHASE1_COMPLETE.md)** - Complete Phase 1 architecture overview
4. **[PHASE2_PLAN.md](PHASE2_PLAN.md)** - Phase 2 implementation options and roadmap
5. **[LINUX_SUBSYSTEM_DESIGN.md](LINUX_SUBSYSTEM_DESIGN.md)** - Linux Subsystem high-level design + interface contract
6. **[WINDOWS_SUBSYSTEM_DESIGN.md](WINDOWS_SUBSYSTEM_DESIGN.md)** - Windows Subsystem design notes
7. **[INSTALLABLE_RAYOS_PLAN.md](INSTALLABLE_RAYOS_PLAN.md)** - Installability plan (USB boot + installer + boot manager tracking)


### Design Tracking (Draft Stubs)

- **[SYSTEM_ARCHITECTURE.md](SYSTEM_ARCHITECTURE.md)** - Unified top-level architecture map
- **[intent_primitives.md](intent_primitives.md)** - Intent primitives, envelope, and kernel contract
- **[gaze_ray_pipeline.md](gaze_ray_pipeline.md)** - Gaze → ray → scheduler pipeline (attention modeling)
- **[linux_projection.md](linux_projection.md)** - Linux compatibility projection (presented desktop)
- **[hardware_target.md](hardware_target.md)** - Minimum viable hardware target for MVP
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

## 🧭 Where to Look (by topic)

- **Boot/build entrypoints:** [../README.MD](../README.MD)
- **Linux subsystem (Option D):** [LINUX_SUBSYSTEM_DESIGN.md](LINUX_SUBSYSTEM_DESIGN.md), [LINUX_SUBSYSTEM_CONTRACT.md](LINUX_SUBSYSTEM_CONTRACT.md)
- **Hypervisor / in-OS VMM roadmap:** [RAYOS_TODO.md](RAYOS_TODO.md)
- **Installability plan:** [INSTALLABLE_RAYOS_PLAN.md](INSTALLABLE_RAYOS_PLAN.md)
- **Boot debugging:** [BOOT_TROUBLESHOOTING.md](BOOT_TROUBLESHOOTING.md)

## ✅ Key Headless Verification Scripts (x86_64)

These scripts boot under OVMF and validate behavior via deterministic serial markers.

```bash
# Fast “did it boot?” marker check
./scripts/test-boot-headless.sh

# Full boot verification sweep
./scripts/verify-boot.sh

# Hypervisor/VMM bring-up smoke (VMX bring-up + selftests)
./scripts/test-vmm-hypervisor-boot.sh

# Boot Linux headless under the in-OS VMM (guest-ready marker)
./scripts/test-vmm-linux-guest-headless.sh

# Virtio-input validation inside Linux guest
./scripts/test-vmm-linux-virtio-input-headless.sh
./scripts/test-vmm-linux-virtio-input-e2e-headless.sh

# Virtio-console guest-driven smoke (when VMX reaches VMCS_READY)
./scripts/test-vmm-virtio-console-headless.sh
```

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
