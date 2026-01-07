# Phase 8: User Mode & IPC - PLANNING

**Target Start**: January 8, 2026
**Estimated Duration**: 2-3 sessions
**Priority**: HIGH - Final phase for core OS functionality

---

## Overview

Phase 8 is the final phase of core OS development. It focuses on executing user space code in Ring 3 privilege level and implementing inter-process communication (IPC) mechanisms. This phase transitions RayOS from a kernel-mode only system to a true multitasking operating system with process isolation.

---

## Tasks (4 Primary)

### Task 1: User Mode Execution & Ring 3 Transition

**Objective**: Enable execution of user code in Ring 3 privilege level with proper privilege transitions

**Deliverables**:
1. **Ring 3 Setup**
   - User mode GDT entries
   - User mode stack allocation
   - Privilege level management

2. **Syscall Entry Point**
   - SYSCALL/SYSRET instruction setup
   - INT 0x80 interrupt gate (alternative)
   - Argument passing from user mode
   - Return path to user mode

3. **Context Preservation**
   - User registers saved/restored
   - Stack switching (kernel ↔ user)
   - Flags preservation
   - Instruction pointer management

4. **Test User Program**
   - Simple user mode program
   - Syscall invocation
   - Return from syscall
   - Verification of execution

**Acceptance Criteria**:
- [ ] User code executes in Ring 3
- [ ] Syscalls from user mode work
- [ ] Context properly saved/restored
- [ ] No privilege escalation
- [ ] Stack switching correct

---

### Task 2: Virtual Memory & Process Isolation

**Objective**: Implement per-process virtual address spaces with memory protection

**Deliverables**:
1. **Per-Process Page Tables**
   - Root PML4 table per process
   - Virtual address space setup
   - Kernel space mapping
   - User space mapping

2. **Page Allocation**
   - Physical page allocator
   - Page frame allocation
   - Page table page allocation
   - Stack page allocation

3. **Address Space Isolation**
   - Separate virtual spaces per process
   - Kernel space shared
   - User space private
   - No cross-process access

4. **Memory Protection**
   - Page present/absent bits
   - Read/write permissions
   - User/supervisor bits
   - Execution disable (NX) bit

**Acceptance Criteria**:
- [ ] Each process has own page tables
- [ ] Page tables correctly set up
- [ ] Memory isolation working
- [ ] Kernel accessible from all processes
- [ ] User space private per process

---

### Task 3: Inter-Process Communication (IPC)

**Objective**: Implement mechanisms for processes to communicate

**Deliverables**:
1. **Pipe Mechanism**
   - Pipe creation (SYS_PIPE)
   - Pipe read/write
   - Buffer management
   - Blocking I/O

2. **Message Queues**
   - Message queue creation
   - Send message
   - Receive message
   - Blocking operations

3. **Signal Support**
   - Signal definition
   - Signal delivery
   - Signal handlers
   - Signal masks

4. **Shared Memory (Optional)**
   - Shared memory creation
   - Memory mapping between processes
   - Synchronization primitives
   - Access control

**Acceptance Criteria**:
- [ ] Pipes work between processes
- [ ] Messages can be sent/received
- [ ] Signals deliverable
- [ ] No data corruption
- [ ] Proper blocking behavior

---

### Task 4: Process Scheduling Refinement

**Objective**: Enhance scheduler with priority support and fairness

**Deliverables**:
1. **Priority Scheduling**
   - Priority levels (0-255)
   - Priority queue management
   - Priority inheritance (optional)
   - Dynamic priority adjustment

2. **Fairness Mechanisms**
   - Time slice allocation
   - Starvation prevention
   - Load balancing
   - Fair sharing

3. **Preemption**
   - Timer-based preemption
   - Preemption on I/O
   - Preemption safety
   - Critical sections

4. **Process Groups**
   - Process groups support
   - Group signal delivery
   - Job control
   - Session management

**Acceptance Criteria**:
- [ ] High priority runs first
- [ ] Fair scheduling working
- [ ] No process starvation
- [ ] Preemption safe
- [ ] Process groups functional

---

## Architecture Design

### Ring 3 Transition Model

```
User Mode (Ring 3)          Kernel Mode (Ring 0)
┌──────────────────┐       ┌──────────────────┐
│  User Program    │       │  Kernel Code     │
│  - Running code  │       │  - Handler funcs │
│  - User stack    │  ────→│  - Page tables   │
│  - Execute user  │       │  - Interrupts    │
└──────────────────┘       └──────────────────┘
         │                         ▲
         │ SYSCALL               │ Return
         │                         │
         └───→ Privilege Gate ────┘
              (SYSCALL instr)
```

### Virtual Memory Layout

```
Per-Process Virtual Address Space (64-bit):
┌────────────────────────────────────────────┐
│                                            │ 0xFFFF_FFFF_FFFF_FFFF
│        Kernel Space (Shared)               │ Accessible from Ring 0
│  - Kernel code, data, heap                 │ Not accessible from Ring 3
│                                            │
├────────────────────────────────────────────┤ 0xFFFF_8000_0000_0000
│                                            │
│        (Unmapped - Guard Page)             │
│                                            │
├────────────────────────────────────────────┤ 0x0000_8000_0000_0000
│                                            │
│        User Space (Private per process)    │ Accessible from Ring 3
│  - User code, data, heap, stack            │
│                                            │
├────────────────────────────────────────────┤ 0x0000_0000_0001_0000
│  Kernel-mapped page (VDSO)                 │ Syscall fast path
└────────────────────────────────────────────┘ 0x0000_0000_0000_0000
```

### IPC Architecture

```
Process A                          Process B
┌─────────────┐                  ┌─────────────┐
│  User Code  │                  │  User Code  │
└──────┬──────┘                  └──────┬──────┘
       │ write(pipe_fd)                 │ read(pipe_fd)
       │                                 │
       ▼                                 ▼
    ┌────────────────────────────────────┐
    │     Kernel Pipe Buffer             │
    │  - Circular buffer (4KB)           │
    │  - Read pointer                    │
    │  - Write pointer                   │
    │  - Waitqueue (sleeping processes)  │
    └────────────────────────────────────┘
```

---

## Implementation Plan

### Session 1: User Mode Execution (2.5 hours)

**Objectives**:
1. Set up Ring 3 execution environment
2. Implement SYSCALL/SYSRET path
3. Create test user program
4. Verify privilege transitions

**Steps**:
1. Create user mode GDT entries
2. Allocate user space memory
3. Implement SYSCALL entry (MSR setup)
4. Create return path (SYSRET)
5. Build test user program (simple assembly)
6. Test syscall invocation
7. Verify no privilege escalation

**Deliverable**: User code running in Ring 3, syscalls working

---

### Session 2: Virtual Memory (2.5 hours)

**Objectives**:
1. Implement per-process page tables
2. Set up address space isolation
3. Implement page allocator
4. Test memory protection

**Steps**:
1. Create page frame allocator
2. Implement page table creation
3. Set up kernel/user split in each process
4. Test page table switching (CR3)
5. Verify memory isolation
6. Test access violations (page fault)
7. Implement page fault handler

**Deliverable**: Each process has isolated virtual memory

---

### Session 3: IPC & Scheduler (2 hours)

**Objectives**:
1. Implement pipe mechanism
2. Add message queues
3. Enhance scheduler
4. Integration testing

**Steps**:
1. Create pipe data structure
2. Implement pipe read/write
3. Add blocking I/O support
4. Implement message queues
5. Add priority scheduling
6. Test IPC between processes
7. Verify scheduler fairness

**Deliverable**: Processes can communicate via pipes and messages

---

## Code Structure (Planned)

### New Sections in kernel-bare/src/main.rs

```
User Mode Support:
├── GDT entry setup for Ring 3
├── SYSCALL/SYSRET instruction setup
├── MSR (Model-Specific Register) setup
├── User space memory allocation
├── Privilege transition code
└── Return path from syscall

Virtual Memory:
├── Page frame allocator
├── Page table structures
├── Page table creation/destruction
├── Address space setup
├── Memory isolation verification
├── Page fault handler
└── TLB management

IPC Mechanisms:
├── Pipe structure
├── Pipe read/write operations
├── Message queue structure
├── Signal definitions
├── Signal delivery
└── Blocking queue management

Enhanced Scheduler:
├── Priority queue implementation
├── Priority-based scheduling
├── Fair scheduling algorithm
├── Preemption safety
└── Process group management
```

### New Structures

**User Mode**:
- `UserSpaceContext` - User mode register state
- `PrivilegeTransition` - Syscall entry setup

**Virtual Memory**:
- `PageTable` - Page table structure
- `PageFrameAllocator` - Physical page management
- `AddressSpace` - Per-process virtual space
- `PageTableEntry` - PTE structure

**IPC**:
- `Pipe` - Pipe buffer and state
- `MessageQueue` - Message queue structure
- `Signal` - Signal definition
- `WaitQueue` - Process waiting queue

**Scheduler**:
- `PriorityQueue` - Priority-based ready queue
- `ProcessGroup` - Group of related processes
- `SchedulerStats` - Scheduler statistics

---

## Success Criteria

### Minimum (Phase Success)
- [ ] User mode execution working
- [ ] Per-process page tables functional
- [ ] Syscalls from Ring 3 working
- [ ] Basic pipe communication working
- [ ] Builds without errors
- [ ] Fully documented

### Stretch (Phase Excellence)
- [ ] Priority scheduling working
- [ ] Message queues implemented
- [ ] Signal support complete
- [ ] Memory protection verified
- [ ] Performance optimized
- [ ] Comprehensive testing

---

## Risk Assessment

### High Priority Risks
1. **Privilege Escalation**: Ring 3 code escaping to Ring 0
   - *Mitigation*: Careful MSR setup, validation in syscall entry

2. **Page Table Corruption**: Incorrect virtual address mapping
   - *Mitigation*: Thorough testing, validation before CR3 load

3. **IPC Deadlock**: Processes waiting for each other
   - *Mitigation*: Timeout mechanisms, deadlock detection

### Medium Priority Risks
1. **TLB Invalidation**: Stale cache entries after page table change
   - *Mitigation*: INVLPG for specific entries, INVPCID

2. **Performance**: Virtual memory lookup overhead
   - *Mitigation*: TLB, multi-level hierarchy

### Low Priority Risks
1. **Compatibility**: Non-standard syscall entry
   - *Mitigation*: Support both SYSCALL and INT 0x80

---

## Dependencies

### From Phase 7
- ✅ Process management structures
- ✅ Syscall dispatcher framework
- ✅ Process state management

### New Requirements
- [ ] Page allocator implementation
- [ ] GDT modification (user segments)
- [ ] MSR setup capability
- [ ] Inline assembly for SYSCALL/SYSRET
- [ ] TLB invalidation code

### External Knowledge
- x86-64 privilege levels (Ring 0-3)
- Page table structures (PML4, PDPT, PD, PT)
- Syscall instruction semantics
- SWAPGS for user/kernel separation

---

## Testing Strategy

### Unit Tests
- Ring 3 instruction execution
- Page table correctness
- Pipe buffer operations
- Priority queue ordering

### Integration Tests
- User to kernel transition
- Multiple processes in Ring 3
- Memory isolation verification
- IPC between processes
- Scheduler fairness

### System Tests
- Long-running user programs
- Stress test with 100+ processes
- IPC under load
- Memory protection violations
- Signal delivery accuracy

---

## Deliverables Summary

| Item | Lines | Status |
|------|-------|--------|
| User Mode Support | ~400 | 🔄 Planned |
| Virtual Memory | ~500 | 🔄 Planned |
| IPC Mechanisms | ~400 | 🔄 Planned |
| Scheduler Enhancement | ~300 | 🔄 Planned |
| Integration & Docs | ~200 | 🔄 Planned |
| **TOTAL** | **~1,800** | **🔄 Planned** |

---

## Next Phase Preview (Phase 9+)

After Phase 8 (if needed):
- Advanced IPC (Unix domain sockets)
- File system operations (read/write files)
- Device drivers (proper initialization)
- Networking stack
- Shell and utilities

---

## Timeline & Milestones

| Date | Milestone | Status |
|------|-----------|--------|
| Jan 8 | Phase 8 Planning | 🔄 In Progress |
| Jan 9 | User Mode Implementation | 🔄 Ready |
| Jan 10 | Virtual Memory + IPC | 🔄 Ready |
| Jan 11 | Testing & Documentation | 🔄 Ready |
| Jan 12 | Phase 8 Complete | 🎯 Goal |

---

**Status**: PLANNING COMPLETE - Ready for implementation
**Phase**: 8 of 8 (Final core OS phase)
**Project Completion After Phase 8**: 100%
