# Phase 7: File System Implementation & Process Management - PLANNING

**Target Start**: January 8, 2026
**Estimated Duration**: 2-3 sessions
**Priority**: HIGH - Critical for system functionality

---

## Overview

Phase 7 focuses on implementing actual file system operations and process management. This phase transitions RayOS from a bare kernel to a functional system capable of:
- Loading and executing files
- Managing multiple processes/tasks
- Context switching
- Basic scheduling

---

## Tasks (3 Primary)

### Task 1: File System Operations - FAT32 Implementation

**Objective**: Implement actual file system operations for FAT32

**Deliverables**:
1. **Directory Walking**
   - Parse directory entries
   - Navigate file hierarchy
   - Find files by name
   - List directory contents

2. **File Reading**
   - FAT chain following
   - Cluster-to-block mapping
   - File data loading into memory
   - Read operation completion

3. **Configuration Loading**
   - Load boot.cfg from FAT32
   - Parse VM image paths
   - Load VM disk metadata
   - Validate file presence

**Acceptance Criteria**:
- [ ] Can read directory entries
- [ ] Can locate files in directory tree
- [ ] Can load file into memory
- [ ] Can read boot configuration
- [ ] Can enumerate VM disk paths

---

### Task 2: Process/Task Management

**Objective**: Implement process structures and management

**Deliverables**:
1. **Process Structure**
   - Process Control Block (PCB)
   - State tracking (ready, running, blocked)
   - Context (registers, stack pointer)
   - Memory address space

2. **Process Manager**
   - Create process from file
   - Track active processes
   - Manage process lifecycle
   - Handle process termination

3. **Task Scheduling**
   - Ready queue management
   - Round-robin scheduling
   - Context save/restore
   - Time-slice management

**Acceptance Criteria**:
- [ ] PCB structure defined and functional
- [ ] Can create processes
- [ ] Can switch between processes
- [ ] Scheduler runs round-robin
- [ ] Context switching works

---

### Task 3: System Calls & User Mode

**Objective**: Implement basic system call interface

**Deliverables**:
1. **System Call Dispatcher**
   - Syscall table definition
   - Argument passing
   - Return value handling
   - Error codes

2. **Basic System Calls**
   - exit() - Process termination
   - write() - Console output
   - read() - Console input
   - open() - File opening (stub)
   - close() - File closing (stub)

3. **User Mode Transition**
   - Ring 3 execution
   - Interrupt gate setup
   - Stack switching
   - Privilege level management

**Acceptance Criteria**:
- [ ] Syscall dispatcher functional
- [ ] At least 3 syscalls implemented
- [ ] Syscalls callable from kernel
- [ ] Return values correct
- [ ] Error handling in place

---

## Architecture Design

### Process/Task Model

```
┌──────────────────────────────────┐
│    Process Control Block (PCB)   │
├──────────────────────────────────┤
│ ├─ Process ID (PID)              │
│ ├─ Process State                 │
│ │  (Ready, Running, Blocked)     │
│ ├─ Context (Registers)           │
│ │  (RIP, RSP, RBP, etc.)        │
│ ├─ Stack Pointer                 │
│ ├─ Memory Space                  │
│ │  (Virtual address range)       │
│ ├─ Parent PID                    │
│ └─ Resource Handles              │
└──────────────────────────────────┘
```

### Scheduler Architecture

```
Ready Queue (FIFO):
  [Process 1] [Process 2] [Process 3] ... [Process N]

Current Process: Process 1 (running)

Timer Interrupt:
  1. Save context of Process 1
  2. Move Process 1 to back of queue
  3. Get Process 2 from front of queue
  4. Restore context of Process 2
  5. Resume execution
```

### System Call Interface

```
Application (Ring 3)
       │
       ├─ System Call via SYSCALL/SYSRET
       │  (or INT 0x80)
       │
       ▼
Kernel (Ring 0)
       │
       ├─ System Call Dispatcher
       │
       ├─ Check syscall number
       │
       ├─ Validate arguments
       │
       ├─ Execute handler
       │
       └─ Return to userspace

User Space (Ring 3)
       │
       └─ Continue execution
```

---

## Implementation Plan

### Session 1: File System Operations

**Duration**: ~2.5 hours

**Steps**:
1. Implement directory entry parsing
2. Add directory walking functions
3. Implement file search
4. Add file data reading
5. Implement FAT chain following
6. Integration with block device
7. Test file loading

**Deliverable**: Can load files from FAT32 disk image

---

### Session 2: Process Management

**Duration**: ~2.5 hours

**Steps**:
1. Define PCB structure
2. Create process manager
3. Implement process creation
4. Add context switching
5. Implement round-robin scheduler
6. Add timer interrupt handler
7. Test process switching

**Deliverable**: Multiple processes can run and context switch

---

### Session 3: System Calls

**Duration**: ~2 hours

**Steps**:
1. Define syscall interface
2. Create syscall dispatcher
3. Implement basic syscalls (exit, write, read)
4. Add argument passing
5. Add error handling
6. Ring 3 setup (optional)
7. Integration testing

**Deliverable**: Syscalls callable from kernel code

---

## Code Structure (Planned)

### New Sections in kernel-bare/src/main.rs

```
File System Operations:
├── Directory entry structures
├── FAT chain traversal
├── File search functions
├── File reading functions
└── Configuration loading

Process Management:
├── PCB (Process Control Block)
├── ProcessManager structure
├── Process creation/termination
├── Context switching
└── Scheduler implementation

System Calls:
├── Syscall dispatcher
├── Syscall handlers table
├── Individual syscall implementations
└── Error handling
```

### Structures to Create

**File System**:
- `DirectoryEntry` - FAT32 directory entry
- `FileHandle` - Open file descriptor
- `FileInfo` - File metadata

**Processes**:
- `ProcessControlBlock` - PCB
- `ProcessContext` - Saved registers
- `ProcessManager` - Global process manager
- `ProcessState` - State enum

**System Calls**:
- `SyscallHandler` - Function pointer type
- `SyscallDispatcher` - Syscall routing
- `SyscallArgs` - Argument structure

---

## Testing Strategy

### Unit Tests
- FAT32 parsing correctness
- Directory walking logic
- Process creation/termination
- Scheduler fairness
- Context switching accuracy

### Integration Tests
- Load file from disk
- Create and run process
- Multiple process context switches
- Syscall invocation
- Process termination handling

### Stress Tests
- 100+ processes created/destroyed
- Rapid context switching
- Large file reading
- Deep directory structures

---

## Success Criteria

### Minimum (Phase Success)
- [ ] Can read files from FAT32
- [ ] Process creation working
- [ ] Context switching functional
- [ ] 3+ syscalls implemented
- [ ] Builds without errors
- [ ] Fully documented

### Stretch (Phase Excellence)
- [ ] Ring 3 user mode working
- [ ] 10+ syscalls implemented
- [ ] Priority scheduling
- [ ] Process pipes/IPC
- [ ] File permissions support

---

## Risk Assessment

### High Priority Risks
1. **FAT32 Complexity**: Chain following can be error-prone
   - *Mitigation*: Extensive testing, simple test cases first

2. **Context Switching**: Incorrect stack handling = crashes
   - *Mitigation*: Careful assembly, minimal inline asm

3. **Scheduler Timing**: Timer interrupts while switching
   - *Mitigation*: Disable interrupts during critical sections

### Medium Priority Risks
1. **Memory Safety**: Process isolation not yet implemented
   - *Mitigation*: Keep in kernel mode initially

2. **File System State**: FAT modifications not handled
   - *Mitigation*: Read-only initially

### Low Priority Risks
1. **Performance**: Naive algorithms sufficient for now
   - *Mitigation*: Optimize later if needed

---

## Dependencies

### What We Have (From Phase 6)
- ✅ Block device interface
- ✅ FAT32 boot sector parsing
- ✅ PCI enumeration
- ✅ VirtIO block device structure

### What We Need
- [ ] Block device read implementation
- [ ] FAT table reading
- [ ] Sector-level I/O
- [ ] Memory layout for processes

### External Resources
- FAT32 specification (free)
- x86-64 context switching guides
- Linux kernel scheduling algorithms

---

## Deliverables Summary

| Item | Lines | Status |
|------|-------|--------|
| FAT32 Operations | ~400 | 🔄 Planned |
| Process Management | ~350 | 🔄 Planned |
| System Call Interface | ~200 | 🔄 Planned |
| Scheduler | ~300 | 🔄 Planned |
| Integration & Docs | ~150 | 🔄 Planned |
| **TOTAL** | **~1,400** | **🔄 Planned** |

---

## Next Phase Preview (Phase 8)

After Phase 7, Phase 8 will focus on:
- User space execution (Ring 3)
- Memory protection per process
- IPC mechanisms
- Virtual memory per process
- Permission model

---

**Status**: PLANNING COMPLETE - Ready for implementation
**Start Date**: January 8, 2026
**Estimated Completion**: January 10, 2026
