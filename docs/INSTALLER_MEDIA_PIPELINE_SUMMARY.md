# Installer Media Pipeline Implementation Summary (Jan 07, 2026)

## What Was Completed

### 1. **Installer Binary with Dry-Run Planning** ✓
- Created `crates/installer/` crate with disk enumeration and installation planning
- Emits structured JSON reports of available disks, partitions, and sizes
- Safe-by-default: sample layout without real disk enumeration; explicit `--enumerate-local-disks` opt-in
- Stderr markers for test automation: `RAYOS_INSTALLER:STARTED`, `SAMPLE_MODE`, `PLAN_GENERATED`, `JSON_EMITTED`, `COMPLETE`

### 2. **Bootable Installer Media** ✓
- `scripts/build-installer-media.sh`: Wraps existing universal UEFI build to produce installer-labeled ISO/USB
- Artifacts: `build/rayos-installer.iso` (44 MB) and `build/rayos-installer-usb.img` (128 MB)
- Installer binary automatically bundled into ESP (EFI/RAYOS/installer.bin) via updated `build-iso.sh`

### 3. **Validation Tests** ✓
- **`test-installer-boot-headless.sh`**: Boots installer USB under QEMU with disposable target disk; verifies boot markers and media integrity
- **`test-installer-dry-run.sh`**: Runs installer binary directly; validates marker sequence and JSON output validity
- Both tests are safe (no host disk interaction) and CI-ready

### 4. **Documentation** ✓
- Updated `docs/INSTALLABLE_RAYOS_PLAN.md` with complete pipeline overview
- Added Section 12 with exact build/test commands and expected outputs
- Included status table showing what's complete vs. pending

---

## Current Installer Media Pipeline

```
┌─────────────────────────────────────────────────────────────┐
│ Developer Machine (any OS)                                  │
├─────────────────────────────────────────────────────────────┤
│ $ scripts/build-installer-media.sh                          │
│   → build/rayos-installer.iso                               │
│   → build/rayos-installer-usb.img                           │
└─────────────────────────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────────────────────────┐
│ Physical USB Drive (user writes with dd/Balena/Rufus)       │
└─────────────────────────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────────────────────────┐
│ Target Machine (boot UEFI from USB)                         │
├─────────────────────────────────────────────────────────────┤
│ • UEFI firmware loads bootloader                            │
│ • Bootloader loads RayOS kernel (from ESP)                  │
│ • Kernel boots into RayOS runtime                           │
│ • Installer binary available at /EFI/RAYOS/installer.bin    │
│   (currently requires kernel integration to invoke)         │
└─────────────────────────────────────────────────────────────┘
```

---

## Testing Commands

```bash
# Build installer media
scripts/build-installer-media.sh

# Test installer binary directly (dry-run, no local disks touched)
scripts/test-installer-dry-run.sh

# Test installer media boots under QEMU (safe, uses disposable VM disk)
scripts/test-installer-boot-headless.sh

# All tests should print PASS and emit expected markers
```

---

## What's Next (Post-Jan 07)

1. **Bootloader Integration** (M1 milestone)
   - Bootloader needs to recognize a special registry flag or environment variable to invoke the installer
   - When invoked: bootloader extracts installer binary from ESP, loads it, and transfers control

2. **Interactive Partition Manager** (M2 milestone)
   - Replace dry-run JSON output with interactive CLI/TUI
   - Allow users to select existing partition or create new partition(s)
   - Partition safety confirmations before writing

3. **System Image Copy** (M2 milestone)
   - Implement `install()` flow to copy RayOS system image to selected partition
   - Write boot entries and recovery partition metadata

4. **Install-to-Disk Validation Test** (M3 milestone)
   - End-to-end QEMU test: installer → partition → copy → reboot → verify mounted
   - Validate persistence across reboot

5. **Boot Manager & Recovery** (M3 milestone)
   - UEFI Boot#### entry provisioning
   - Recovery boot entry pointing back to installer USB

---

## Key Design Decisions

- **Installer runs from USB, not embedded in RayOS**: Allows future updates to installer without reflashing firmware
- **Safe by default**: Dry-run planning mode is default; real disk enumeration requires explicit opt-in
- **Modular crate approach**: Installer is a separate Cargo crate; can be versioned/tested independently
- **Serial markers for CI**: All major installer steps emit markers to stderr; CI tests grep for them
- **Disposable VM disks for testing**: All validation tests use temporary disk images; no risk to dev machine

---

## Files Changed/Added

### New files:
- `crates/installer/` (full crate)
- `scripts/build-installer-media.sh`
- `scripts/test-installer-boot-headless.sh`
- `scripts/test-installer-dry-run.sh`
- `scripts/tools/run_installer_planner.sh`
- `scripts/tools/vmm_mmio_map.py`

### Modified files:
- `scripts/build-iso.sh` (added installer binary bundling)
- `docs/INSTALLABLE_RAYOS_PLAN.md` (added Section 12 and status table)
- `docs/RAYOS_TODO.md` (installer tracking)

---

## References

- Installer design: [INSTALLER_AND_BOOT_MANAGER_SPEC.md](./INSTALLER_AND_BOOT_MANAGER_SPEC.md)
- Disk layout: [DISK_LAYOUT_AND_PERSISTENCE.md](./DISK_LAYOUT_AND_PERSISTENCE.md)
- Full installability plan: [INSTALLABLE_RAYOS_PLAN.md](./INSTALLABLE_RAYOS_PLAN.md)
