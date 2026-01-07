# Phase 6: Device Driver Framework & Storage Subsystem - PLANNING

**Target Start**: January 7, 2026  
**Estimated Duration**: 2-3 sessions  
**Priority**: HIGH - Foundation for storage, networking, and device management

---

## Overview

Phase 6 focuses on creating the device driver framework and implementing core storage functionality. This phase is critical for:
- Making RayOS installable (needs persistent storage access)
- Supporting hardware integration (PCI/PCIe devices)
- Enabling VM disk access
- Establishing the driver model for future subsystems

---

## Tasks (3 Primary)

### Task 1: Device Discovery & Enumeration Framework

**Objective**: Create mechanisms to discover and enumerate hardware devices

**Deliverables**:
1. **PCI Bus Enumeration**
   - PCI configuration space access (Type 0/1)
   - Device/vendor ID parsing
   - BAR (Base Address Register) reading
   - Class/subclass detection

2. **Device Registry**
   - Device database structure
   - Hot-plug support framework
   - Device identification strategy
   - Driver matching system

3. **Boot Integration**
   - Hardware enumeration at kernel startup
   - Device tree building
   - Logging and reporting

**Acceptance Criteria**:
- [ ] PCI enumeration working (at least on QEMU)
- [ ] 10+ test devices detected and logged
- [ ] Device structure properly abstracted
- [ ] Framework extensible for USB, ACPI, etc.

---

### Task 2: Block Device Abstraction & Virtual Disk Support

**Objective**: Implement block device interface and virtual disk handling

**Deliverables**:
1. **Block Device Interface**
   - read_blocks() / write_blocks() API
   - Block size (512B, 4K) handling
   - Sector-based addressing
   - DMA buffer management

2. **Virtual Disk (VirtIO)**
   - VirtIO block device detection
   - Queue management
   - Command submission
   - Response handling

3. **Simple Block Driver**
   - ATA/IDE support (optional)
   - Fallback disk detection
   - Error handling
   - Logging

**Acceptance Criteria**:
- [ ] VirtIO block device recognized
- [ ] Can read sectors from virtual disk
- [ ] Write operations functional
- [ ] Error cases handled

---

### Task 3: File System Bootstrap & Persistent Storage

**Objective**: Implement basic file system support for RayOS persistence

**Deliverables**:
1. **File System Abstraction**
   - File system trait/interface
   - Inode representation
   - Directory walking
   - Path resolution

2. **FAT32 Support** (minimal)
   - Boot sector parsing
   - FAT table reading
   - File lookup
   - Directory enumeration

3. **Configuration & Boot Storage**
   - Boot config file reading
   - System settings persistence
   - VM image path resolution
   - Log file writing

**Acceptance Criteria**:
- [ ] Can read files from FAT32 disk
- [ ] Directory traversal working
- [ ] Boot configuration loadable
- [ ] Persistence layer functional

---

## Architecture Design

### Device Driver Model

```
┌─────────────────────────────────────┐
│      High-Level Subsystems          │
│  (VM Manager, File System, etc)     │
└──────────┬──────────────────────────┘
           │
┌──────────┴──────────────────────────┐
│      Device Driver Framework        │
│  ├─ Device Registry                 │
│  ├─ Driver Matching                 │
│  └─ Lifecycle Management            │
└──────────┬──────────────────────────┘
           │
┌──────────┴──────────────────────────┐
│    Hardware Abstraction Drivers     │
│  ├─ PCI/PCIe Bus Driver            │
│  ├─ Block Device Drivers            │
│  ├─ VirtIO Device Driver            │
│  └─ Storage Controller Drivers       │
└──────────┬──────────────────────────┘
           │
┌──────────┴──────────────────────────┐
│      Hardware Interfaces            │
│  ├─ Configuration Space Access      │
│  ├─ MMIO Register Access            │
│  ├─ I/O Port Access                 │
│  └─ Interrupt Handling              │
└─────────────────────────────────────┘
```

### Kernel Integration Points

Phase 6 will integrate with:
- **Phase 5**: Module system (drivers as modules)
- **Phase 4**: Interrupt handlers (device IRQs)
- **Phase 3**: ACPI (device discovery)
- **Phase 2**: Memory management (DMA buffers)
- **Phase 1**: Bootloader (device info)

---

## Implementation Plan

### Session 1: Device Discovery

**Duration**: ~2 hours

**Steps**:
1. Create PCI configuration space reader
2. Implement device enumeration
3. Add device registry structure
4. Integrate into kernel boot
5. Add debug logging
6. Test with QEMU

**Deliverable**: Working PCI enumeration with 10+ devices detected

---

### Session 2: Block Device Driver

**Duration**: ~3 hours

**Steps**:
1. Design block device interface
2. Implement VirtIO block driver
3. Add read/write operations
4. Handle queues and responses
5. Error handling
6. Integration testing

**Deliverable**: VirtIO block device can read/write sectors

---

### Session 3: File System & Persistence

**Duration**: ~2-3 hours

**Steps**:
1. Design file system abstraction
2. Implement FAT32 parser
3. Add file lookup
4. Create boot config reader
5. Integration testing
6. Persistence validation

**Deliverable**: Can load boot configuration and read VM image paths

---

## Code Structure (Planned)

### New Modules in kernel-bare/src/

```
main.rs
├── Device Discovery & Enumeration
│   ├── PCI configuration access
│   ├── Device enumeration
│   ├── Device registry
│   └── Driver matching
│
├── Block Device Interface
│   ├── Block device trait
│   ├── VirtIO implementation
│   ├── Generic block driver
│   └── DMA buffer management
│
└── File System
    ├── File system trait
    ├── FAT32 implementation
    ├── Inode/directory structures
    └── Path resolution
```

### Structures to Create

**Device Discovery**:
- `PciDevice` - PCI device info
- `DeviceRegistry` - Device database
- `DeviceDriver` - Driver interface

**Block Devices**:
- `BlockDeviceOps` - Read/write trait
- `VirtIOBlock` - VirtIO driver
- `BlockDevice` - Generic wrapper

**File System**:
- `FileSystem` - File system trait
- `Inode` - File/directory node
- `FAT32FS` - FAT32 implementation

---

## Testing Strategy

### Unit Tests
- PCI configuration parsing
- Device enumeration correctness
- FAT32 parsing accuracy

### Integration Tests
- Full device discovery on QEMU
- Block device I/O operations
- File system reads

### Real-World Tests
- Multiple QEMU architectures
- Different disk configurations
- Edge cases (corrupted FAT, etc)

---

## Success Criteria

### Minimum (Phase Success)
- [ ] PCI devices enumerable
- [ ] Block device reads working
- [ ] File system can load boot config
- [ ] Builds without errors
- [ ] Documented with examples

### Stretch (Phase Excellence)
- [ ] Write operations working
- [ ] Multiple disk support
- [ ] AHCI/IDE driver support
- [ ] File system write support
- [ ] Performance optimizations

---

## Dependencies & Constraints

### What We Have (from Phase 5)
- ✅ Module system for drivers
- ✅ Virtual memory management
- ✅ Exception handling
- ✅ Interrupt framework

### What We Need
- [ ] VirtIO device spec knowledge
- [ ] FAT32 file system spec
- [ ] PCI configuration space understanding
- [ ] QEMU block device configuration

### External Resources
- VirtIO specification (free, published)
- FAT32 standard (free, published)
- PCI Local Bus Specification (available)

---

## Risk Assessment

### High Priority Risks
1. **VirtIO Device Complexity**: Might need queue debugging
   - *Mitigation*: Start simple, use QEMU logs

2. **DMA Buffer Management**: Can cause system instability
   - *Mitigation*: Test thoroughly, add validation

3. **File System Data Corruption**: FAT32 bugs can destroy data
   - *Mitigation*: Read-only initially, test extensively

### Medium Priority Risks
1. **Device Enumeration Edge Cases**: Unusual hardware
   - *Mitigation*: Start with QEMU only

2. **Performance Issues**: Slow I/O
   - *Mitigation*: Profile, optimize iteratively

### Low Priority Risks
1. **API Changes**: Future phases might need different interface
   - *Mitigation*: Keep APIs flexible, modular

---

## Deliverables Summary

| Item | Lines | Status |
|------|-------|--------|
| PCI Bus Enumeration | ~300 | 🔄 Planned |
| Device Registry | ~200 | 🔄 Planned |
| Block Device Interface | ~250 | 🔄 Planned |
| VirtIO Block Driver | ~400 | 🔄 Planned |
| FAT32 File System | ~500 | 🔄 Planned |
| Integration & Docs | ~200 | 🔄 Planned |
| **TOTAL** | **~1,850** | **🔄 Planned** |

---

## Next Phase Preview (Phase 7)

After Phase 6, Phase 7 will focus on:
- Process management and task scheduling
- System calls interface
- User-mode execution
- Context switching

This phase enables running user applications and the VM subsystems.

---

**Status**: PLANNING COMPLETE - Ready for implementation
**Start Time**: January 7, 2026
**Estimated End**: January 10, 2026
