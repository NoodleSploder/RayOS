# Phase 21: RayOS Foundation Completion - Three Critical Milestones

**Phase**: 21/25+ (High-Impact Infrastructure & Product Readiness)
**Status**: Planning & Specification
**Target Completion**: ~120 minutes (6 tasks, ~4,800 lines)
**Commit Target**: `Phase 21: RayOS Foundation Completion (3 Milestones, 6 Implementation Tasks)`

---

## Phase Overview

Phase 21 focuses on delivering the **three highest-impact loose ends** identified in RAYOS_TODO.md:

### Milestone 1: RayOS-Native Linux Desktop (Remove VNC Dependency)
**Goal**: Make `show linux desktop` work out-of-the-box without requiring a host VNC viewer.  
**Impact**: Enables standalone RayOS GUI without host dependencies.  
**Primary Loose Ends** (from RAYOS_TODO.md, section 23e):
- Presentation bridge: validate in-OS virtio-gpu scanout path is production-ready
- Remove VNC client detection step (detect `gvncviewer`/`remote-viewer` or provide fallback)
- Make dev harness use RayOS-native presentation by default
- Add automated tests for show→hide→show without host bridge

### Milestone 2: Installer & Boot Manager Foundation (Standalone OS Readiness)
**Goal**: Build the minimal installer/boot manager so RayOS can run on physical hardware without QEMU.  
**Impact**: Transforms RayOS from a research VM into an installable OS; enables product-level deployment.  
**Primary Loose Ends** (from RAYOS_TODO.md, section 11-34 + INSTALLABLE_RAYOS_PLAN.md):
- Create `crates/installer` with USB boot detection and partition management
- Implement minimal boot manager that detects and boots installed RayOS copies
- Add disk image creation for direct hardware deployment
- Wire installer into the build pipeline

### Milestone 3: Observability & Crash Recovery (Reliability Loop)
**Goal**: Add persistent logging, watchdog semantics, and "last known good" recovery.  
**Impact**: Prevents regressions during VMM/GUI evolution; enables safe experimentation.  
**Primary Loose Ends** (from RAYOS_TODO.md, section 3 + OBSERVABILITY_AND_RECOVERY.md):
- Implement persistent kernel log buffer to SSD/USB
- Add watchdog timer with automatic reboot on hang
- Define and implement "last known good" boot marker semantics
- Create recovery mode that boots from last working state

---

## Design Principles

- **Progressive Enhancement**: Each milestone improves RayOS toward production without blocking the others
- **Test-First**: Every feature has headless automation; interactive testing is optional
- **Authority Preservation**: RayOS maintains full control over subsystems and recovery (never depend on host)
- **Deterministic Markers**: All operations emit testable markers for CI/automation
- **Fallback Safety**: Always provide a safe fallback when preferred path is unavailable

---

## Task Breakdown

### Task 1: Native Linux Desktop Presentation Bridge (800 lines)
**File**: `crates/kernel-bare/src/linux_presentation.rs` (new module)  
**Goal**: Validate and harden the in-OS virtio-gpu scanout presentation path.

**Components**:
- `PresentationBridge` struct: manages guest surface → RayOS window mapping
- `SurfaceCache`: tracks active guest surfaces, frame buffers, and presented state
- `FrameBuffer`: guest scanout backing (CPU-visible shared memory)
- `PresentationEvent`: surface create/update/destroy lifecycle
- Surface lifecycle: create → first_frame → presented → hidden → destroy

**Methods**:
- `new()`: Initialize presentation bridge
- `create_surface()`: Register new guest surface
- `update_frame()`: Ingest new guest scanout frame
- `get_surface()`: Retrieve surface by ID
- `present_surface()`: Make surface visible in RayOS UI
- `hide_surface()`: Unpresent without destroying
- `destroy_surface()`: Clean up surface resources
- `get_presented_surfaces()`: List currently visible surfaces
- `frame_available()`: Non-blocking frame check (for UI loop integration)
- `get_surface_stats()`: Frame rate, latency, buffer consumption
- `validate_gpa_range()`: Safety check for guest-provided addresses

**Tests**:
- test_create_surface
- test_frame_ingest
- test_present_hide_present
- test_concurrent_surfaces
- test_safety_bounds_checking

**Deterministic Markers**:
- `RAYOS_PRESENTATION:SURFACE_CREATE:<id>:<w>x<h>`
- `RAYOS_PRESENTATION:FIRST_FRAME:<id>`
- `RAYOS_PRESENTATION:PRESENTED:<id>`
- `RAYOS_PRESENTATION:HIDDEN:<id>`
- `RAYOS_PRESENTATION:DESTROY:<id>`

---

### Task 2: Installer Foundation & Partition Management (900 lines)
**Files**:
- `crates/installer/src/lib.rs` (new crate)
- `crates/kernel-bare/src/installer.rs` (new module integrating installer)
- `scripts/build-installer.sh` (new build helper)

**Goal**: Build core installer infrastructure for USB-bootable RayOS installation.

**Components**:
- `InstallerBoot`: USB detection and boot mode identification
- `PartitionManager`: partition enumeration, creation, formatting
- `DiskLayout`: defines target disk layout (EFI, RayOS system, recovery, user data partitions)
- `InstallationTarget`: selected disk + partition configuration
- `InstallationProgress`: progress tracking and persistent state
- `InstallerUI`: minimal text menu for disk/partition selection

**Methods**:
- `new()`: Initialize installer
- `detect_usb_boot()`: Identify if running from USB
- `enumerate_disks()`: List available block devices
- `enumerate_partitions()`: List partitions on target disk
- `validate_target()`: Safety checks before installation
- `create_partition()`: Create new partition with safety guards
- `format_partition()`: Format to FAT32/ext4
- `write_rayos_image()`: Copy system files to target
- `install_bootloader()`: Install UEFI bootloader
- `write_recovery_partition()`: Create recovery/snapshot partition
- `persist_installation_state()`: Save installation metadata
- `verify_installation()`: Post-install validation

**Deterministic Markers**:
- `RAYOS_INSTALLER:BOOT:USB`
- `RAYOS_INSTALLER:DISK_DETECTED:<device>:<size>`
- `RAYOS_INSTALLER:PARTITION_CREATED:<id>:<size>`
- `RAYOS_INSTALLER:INSTALL_BEGIN`
- `RAYOS_INSTALLER:INSTALL_COMPLETE:<device>`
- `RAYOS_INSTALLER:INSTALL_FAILED:<reason>`

**Safety Guards**:
- Confirm before writing to any disk (require 2-stage confirmation)
- Refuse to overwrite existing RayOS installations without explicit warning
- Validate target disk is not the boot device (prevent self-destruction)
- Create backups of existing partition table before modification

---

### Task 3: Boot Manager & Multi-Boot Support (900 lines)
**Files**:
- `crates/kernel-bare/src/boot_manager.rs` (new module)
- `scripts/install-bootloader.sh` (new helper)

**Goal**: Implement boot manager that detects and launches installed RayOS instances.

**Components**:
- `BootMenu` struct: discoverable boot entries
- `BootEntry`: installation target (disk + partition + config)
- `BootConfig`: UEFI variables, boot order, timeout, default selection
- `BootLoader`: chainload from UEFI firmware and select boot target
- `RecoveryEntry`: "last known good" snapshots and fallback paths
- `BootMetrics`: track boot attempts, failures, recovery triggers

**Methods**:
- `new()`: Initialize boot manager
- `enumerate_installations()`: Discover all RayOS instances
- `load_boot_config()`: Read UEFI boot variables
- `display_menu()`: Present boot options to user
- `boot_target()`: Validate and boot selected installation
- `boot_default()`: Auto-select and boot primary installation
- `boot_recovery()`: Boot into recovery/last-known-good
- `set_boot_order()`: Reorder boot entries
- `set_default_boot()`: Configure primary boot target
- `record_boot_attempt()`: Track successful/failed boots
- `select_recovery_target()`: Choose recovery snapshot

**Deterministic Markers**:
- `RAYOS_BOOTMGR:DISCOVERED:<n>_installations`
- `RAYOS_BOOTMGR:SELECTED:<installation_id>`
- `RAYOS_BOOTMGR:BOOT_ATTEMPT:<try>/<max>`
- `RAYOS_BOOTMGR:BOOT_SUCCESS`
- `RAYOS_BOOTMGR:RECOVERY_FALLBACK:<reason>`

**Recovery Semantics**:
- On failed boot (> 3 consecutive failures), automatically boot last-known-good snapshot
- On successful boot, record as "last known good"
- Provide explicit recovery entry in menu (e.g., `Boot Recovery (Snapshot 2026-01-06 14:30)`)
- Support atomic snapshot + rollback (copy-on-write semantics)

---

### Task 4: Persistent Logging & Watchdog Infrastructure (800 lines)
**Files**:
- `crates/kernel-bare/src/persistent_log.rs` (new module)
- `crates/kernel-bare/src/watchdog.rs` (new module)

**Goal**: Add durable logging and automatic recovery from hangs.

**Components**:
- `PersistentLog`: circular buffer backed to persistent storage (USB/SSD)
- `LogLevel` enum: FATAL, ERROR, WARN, INFO, DEBUG, TRACE
- `LogEntry`: timestamp, level, component, message (binary + text)
- `Watchdog`: timer-based hang detection and automatic reboot
- `WatchdogPolicy`: configurable timeout, reboot action, recovery trigger
- `CrashArtifacts`: preserved crash state (registers, backtrace, last N log entries)

**Methods (PersistentLog)**:
- `new()`: Initialize with backing store
- `write()`: Add log entry (blocking until persisted)
- `flush()`: Ensure all entries written to disk
- `get_last_n()`: Retrieve last N entries (for recovery UI)
- `clear()`: Wipe log (for fresh boots)
- `export()`: Export log as text for debugging
- `rotate()`: Move to next log file when full
- `validate()`: Integrity check on log structure

**Methods (Watchdog)**:
- `new()`: Initialize watchdog with timeout
- `start()`: Arm watchdog timer
- `kick()`: Reset timer (heartbeat from main loop)
- `on_timeout()`: Handler for expired timer
- `set_policy()`: Change timeout or reboot action
- `is_armed()`: Check if watchdog is active
- `get_last_reboot_reason()`: Why did we last reboot?

**Deterministic Markers**:
- `RAYOS_LOG:INIT:<backing_device>:<capacity>`
- `RAYOS_LOG:WRITTEN:<n>_entries`
- `RAYOS_LOG:ROTATED:<file_id>`
- `RAYOS_WATCHDOG:ARMED:<timeout_sec>`
- `RAYOS_WATCHDOG:KICK:<count>`
- `RAYOS_WATCHDOG:TIMEOUT:<hang_reason>`
- `RAYOS_WATCHDOG:REBOOT_RECOVERY`
- `RAYOS_CRASH:ARTIFACT_SAVED:<type>`

**Storage Layout**:
- Partition reserved: 128 MB (or USB endpoint with explicit path)
- Format: simple sequential binary entries with headers
- Rollover: when full, archive old log and start new file
- Integrity: CRC32 per entry, timestamp monotonicity checks
- Recovery: on boot, read log to identify last failure + trigger recovery if needed

---

### Task 5: Last-Known-Good Boot Marker System (700 lines)
**Files**:
- `crates/kernel-bare/src/boot_marker.rs` (new module)
- `crates/kernel-bare/src/recovery_policy.rs` (update existing)

**Goal**: Define and implement deterministic "last known good" semantics.

**Components**:
- `BootMarker` enum: states like `Booting`, `Kernel_Loaded`, `Subsystems_Ready`, `Shell_Ready`, `Golden`
- `MarkerTimestamp`: precise boot progress tracking
- `RecoverySnapshot`: checkpoint of known-good state
- `BootFailureRecord`: consecutive failure tracking
- `MarkerStorage`: persist markers to disk

**Methods**:
- `new()`: Initialize marker system
- `mark_stage()`: Record boot progress (Kernel → Subsystems → Shell → Golden)
- `mark_golden()`: Flag current state as "known good"
- `get_current_marker()`: What stage are we at?
- `load_last_golden()`: Get previous golden marker
- `record_failure()`: Increment failure counter
- `get_failure_count()`: How many consecutive failures?
- `should_trigger_recovery()`: Failover decision logic
- `trigger_recovery()`: Switch to recovery path
- `validate_marker_chain()`: Ensure markers are monotonic + valid

**Deterministic Markers**:
- `RAYOS_BOOT_MARKER:KERNEL_LOADED`
- `RAYOS_BOOT_MARKER:SUBSYSTEMS_READY`
- `RAYOS_BOOT_MARKER:SHELL_READY`
- `RAYOS_BOOT_MARKER:GOLDEN`
- `RAYOS_BOOT_MARKER:FAILURE_COUNT:<n>`
- `RAYOS_BOOT_MARKER:RECOVERY_TRIGGERED`

**Recovery Logic**:
```
Boot sequence:
1. Check failure count from previous boot
2. If count >= 3, load "last golden" snapshot and boot from there
3. As boot progresses, mark stages
4. On successful completion of Shell, mark as "golden" for next boot
5. On failure (panic/watchdog), record failure and trigger reboot with counter
```

---

### Task 6: Integration & CLI Commands (600 lines)
**Files**:
- `crates/kernel-bare/src/shell.rs` (update: add installer/recovery commands)

**Goal**: Wire installer, boot manager, and recovery into RayOS shell.

**New Commands**:

#### Installer Commands
```bash
install                     # Start interactive installer
install --help             # Show installer options
install --list-disks       # Enumerate available disks
install --to <disk>        # Install to specific disk (requires confirmation)
install --recovery         # Install recovery partition only
install --verify           # Verify installed RayOS on target disk
```

#### Boot Manager Commands
```bash
bootmgr                     # Show current boot configuration
bootmgr list               # List available installations
bootmgr set-default <id>   # Set primary boot target
bootmgr set-timeout <sec>  # Configure boot menu timeout
bootmgr recovery           # Boot recovery/last-known-good
bootmgr status             # Show boot metrics and failure counts
```

#### Recovery Commands
```bash
recovery status            # Show recovery state
recovery snapshot          # Create manual recovery snapshot
recovery list              # List available recovery points
recovery restore <id>      # Restore from specific snapshot
recovery last-known-good   # Boot from last working state
```

#### Logging Commands
```bash
log show                    # Display recent kernel log
log export                  # Export log to USB/file
log level <TRACE|DEBUG|INFO|WARN|ERROR|FATAL>  # Set log level
log clear                   # Wipe log (with confirmation)
```

#### Watchdog Commands
```bash
watchdog status            # Show watchdog state + timeout
watchdog arm <sec>         # Enable watchdog with timeout
watchdog disarm            # Disable watchdog
watchdog kick              # Manual heartbeat
watchdog policy            # Show reboot policy on timeout
```

**Implementation Pattern**:
- Each command follows existing `cmd_*` naming convention
- All critical operations (install, recovery, watchdog) require 2-stage confirmation
- Emit deterministic markers for every operation
- Provide `--help` subcommand for discoverability
- Safe defaults: recovery is opt-in, watchdog defaults to safe timeout

---

## Acceptance Criteria

### Milestone 1: Native Linux Desktop
- [ ] `show linux desktop` works without VNC client dependency
- [ ] Fallback detection: gracefully handle missing `gvncviewer`
- [ ] Presentation framework validated: scanout → RayOS surface → user sees desktop
- [ ] Headless test passes: `test-vmm-linux-desktop-native-show-hide-show-headless.sh`
- [ ] Interactive test available: `show linux desktop` opens desktop in QEMU window

### Milestone 2: Installer & Boot Manager
- [ ] `install` command successfully installs RayOS to USB
- [ ] `bootmgr` menu appears when booting from installed image
- [ ] Successful boot → automatic golden marker
- [ ] Failed boot (3×) → automatic fallback to recovery
- [ ] Headless test: install → verify → boot → confirm success markers

### Milestone 3: Observability & Recovery
- [ ] Kernel logs persisted to USB/SSD
- [ ] Watchdog armed by default (30 sec timeout)
- [ ] Hang detected → reboot → recovery snapshot boots
- [ ] `log show` displays recent entries
- [ ] `recovery restore <id>` recovers from snapshot

### Integration
- [ ] All 6 tasks compile with 0 errors
- [ ] New commands visible in `help`
- [ ] Deterministic markers emitted for all operations
- [ ] CI smoke tests pass for each feature
- [ ] No pre-existing tests regressed

---

## Testing Strategy

### Headless Automation (CI-ready)
- `test-linux-presentation-bridge-headless.sh` - Validate scanout path
- `test-installer-usb-boot-headless.sh` - Installer USB detection
- `test-bootmgr-install-verify-headless.sh` - Install → boot → verify
- `test-recovery-golden-fallback-headless.sh` - Failure → golden recovery
- `test-watchdog-hang-recovery-headless.sh` - Watchdog timeout → reboot
- `test-persistent-log-write-headless.sh` - Log persistence across reboots

### Interactive Tests (Developer-friendly)
- `./scripts/run-installer-interactive.sh` - Manual install walkthrough
- `./scripts/run-bootmgr-interactive.sh` - Boot menu with keyboard nav
- `./scripts/run-recovery-interactive.sh` - Recovery console
- `show linux desktop` - Visual confirmation of native presentation

---

## Risk Mitigation

| Risk | Severity | Mitigation |
|------|----------|-----------|
| Installer overwrites wrong disk | Critical | 2-stage confirmation + refuse to overwrite existing RayOS |
| Boot manager fails to find installations | High | Enumerate all disks during boot; provide manual selection menu |
| Persistent log fills up partition | Medium | Rotate logs; warn at 80% capacity |
| Watchdog false-positive reboot | Medium | Kick from main loop + idle detection (don't reboot if idle) |
| VNC client absent on host | Medium | Graceful fallback: try `gvncviewer`, then `remote-viewer`, then print instructions |
| Recovery snapshot corrupted | High | Validate checksum on boot; keep 3 rolling snapshots |

---

## Dependencies

### External
- UEFI firmware (OVMF) for boot manager
- USB libraries for disk enumeration (pure Rust)
- Timer hardware for watchdog (interrupt-driven)

### Internal
- Phase 20: Rate Limiting & API Governance (shell command infrastructure)
- Phase 19: API Gateway (service routing for installer/recovery operations)
- Existing: Hypervisor + virtio-gpu (for presentation bridge)

---

## Execution Plan

1. **Week 1 (Days 1-2): Task 1 — Presentation Bridge**
   - Implement `linux_presentation.rs` module
   - Add surface lifecycle tracking
   - Validate existing virtio-gpu scanout path
   - Add headless smoke test

2. **Week 1 (Days 2-3): Task 2 — Installer Foundation**
   - Create `crates/installer` crate
   - Implement partition management
   - Add USB boot detection
   - Wire into kernel-bare

3. **Week 1 (Days 3-4): Task 3 — Boot Manager**
   - Implement boot discovery + UEFI integration
   - Build interactive boot menu
   - Add multi-installation support
   - Test boot target selection

4. **Week 1 (Days 4-5): Task 4 — Logging & Watchdog**
   - Implement persistent log buffer
   - Add watchdog timer with auto-reboot
   - Create log storage format
   - Add heartbeat mechanism

5. **Week 1 (Days 5-6): Task 5 — Boot Markers & Recovery**
   - Implement boot marker system
   - Define recovery trigger logic
   - Create snapshot management
   - Add golden marker tracking

6. **Week 2 (Day 1): Task 6 — CLI Integration**
   - Add shell commands for all features
   - Wire commands into command dispatcher
   - Add deterministic markers
   - Test command help + discoverability

7. **Week 2 (Day 1): Testing & Validation**
   - Run all 6 headless smoke tests
   - Verify no regression in existing tests
   - Commit Phase 21 work
   - Create Phase 21 Final Report

---

## Metrics

| Metric | Target |
|--------|--------|
| Total lines added | ~4,800 |
| New modules | 7 (presentation, installer, boot_mgr, persistent_log, watchdog, boot_marker, recovery_policy) |
| New shell commands | 20+ (install*, bootmgr*, recovery*, log*, watchdog*) |
| Headless tests | 6 |
| Compilation errors | 0 |
| Pre-existing test regressions | 0 |
| CI pass rate | 100% |

---

## Next Phase (Phase 22 Preview)

After Phase 21, priorities shift to:

1. **RayApp Framework**: Build native RayOS application container
2. **Wayland Integration**: Replace VNC with true Wayland surface forwarding
3. **Multi-App GUI**: Support multiple guest surfaces as RayOS windows
4. **Disk I/O Performance**: Optimize virtio-blk and persistent storage throughput
5. **System Integration Testing**: Soak runs + stress tests under continuous operation

---

## References

- RAYOS_TODO.md: Sections 1-3 (milestones), 23e (presentation), 11 (installer), 3 (observability)
- INSTALLABLE_RAYOS_PLAN.md: Installer architecture + rollout strategy
- OBSERVABILITY_AND_RECOVERY.md: Logging + watchdog design
- LINUX_SUBSYSTEM_DESIGN.md: Subsystem authority model (preservation of RayOS control)
- BOOT_VERIFICATION.md: Boot flow documentation
- Existing: `crates/kernel-bare/src/shell.rs` (command dispatcher)

---

**Phase 21 Planning Complete** ✅

Ready to proceed with implementation on next session.
