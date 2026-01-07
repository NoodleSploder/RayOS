# Phase 4: System Initialization & Kernel Development - Planning

**Status:** Planning  
**Date:** January 7, 2026  
**Previous Phase:** Phase 3 (Boot Testing) - ✅ COMPLETE

---

## 📊 Current System Status

### What We Have ✅

**Bootloader (Phase 2)**
- 57 KB UEFI binary with chainloading support
- Registry-based mode detection (kernel vs installer)
- Installer and kernel binary loading
- Memory management and allocation
- Error handling with fallbacks

**Kernel (In Progress)**
- 3.6 MB ELF binary
- x86_64 target (x86_64-rayos-kernel)
- Bare-metal implementation
- Entry point prepared

**Installer (Available)**
- 5.3 MB flat binary
- Chainloaded via bootloader
- Ready for system initialization

**Boot Media (Phase 3)**
- Kernel-mode ISO (4.0 MB) - Direct kernel boot
- Installer-mode ISO (9.3 MB) - Installer chainloading
- Both verified with correct structure
- QEMU firmware testing complete

**Documentation (Phase 3)**
- 2,400+ lines of comprehensive guides
- Boot testing procedures documented
- Troubleshooting guides included
- Architecture documentation complete

---

## 🎯 Phase 4 Objectives

### Goal: Bring RayOS to Functional Boot with Basic I/O

Get the kernel executing beyond bootloader and performing basic system initialization.

---

## 📋 Phase 4 Tasks (Proposed Order)

### Task 1: Kernel Entry Point & CPU Initialization
**Duration:** 1-2 hours  
**What:** Get kernel executing, set up CPU environment

**Subtasks:**
- [ ] Verify kernel entry point is reached
  - Add serial output from kernel startup
  - Print "Kernel entry point" message
  
- [ ] Set up GDT (Global Descriptor Table)
  - Configure segment descriptors
  - Load GDT register
  
- [ ] Set up IDT (Interrupt Descriptor Table)
  - Prepare for interrupt handling
  - Install exception handlers (basic)
  
- [ ] Enable paging if not already enabled
  - Verify page tables from bootloader
  - Set up kernel page directory

**Success Criteria:**
- Kernel prints startup message via serial
- CPU is in protected mode with paging
- No triple faults or hangs
- Next phase can set up interrupts

**Files to Modify:**
- `crates/kernel-bare/src/main.rs` - Entry point
- `crates/kernel-bare/src/cpu.rs` - CPU setup (create if needed)

---

### Task 2: Serial Console Output (Early Logging)
**Duration:** 30 minutes - 1 hour  
**What:** Get kernel printing to serial console for debugging

**Subtasks:**
- [ ] Set up UART serial port (COM1: 0x3F8)
  - Initialize UART with correct baud rate (115200)
  - Configure data/stop/parity bits
  
- [ ] Implement early serial write function
  - Simple character output
  - String output helper
  
- [ ] Add kernel logging macros
  - `println!` equivalent for kernel
  - Different log levels (debug, info, warn)

**Success Criteria:**
- Kernel startup messages appear on serial console
- Can see CPU initialization progress
- Log messages are clear and helpful

**Files to Modify:**
- `crates/kernel-bare/src/serial.rs` - Serial driver (create if needed)
- `crates/kernel-bare/src/main.rs` - Add logging calls

---

### Task 3: Memory Management Setup
**Duration:** 1-2 hours  
**What:** Implement basic memory allocation and heap

**Subtasks:**
- [ ] Set up heap allocator
  - Choose allocator (linked-list, buddy, simple bump)
  - Initialize heap region
  
- [ ] Implement `alloc` support
  - Enable `alloc` crate for Rust collections
  - Use global allocator
  
- [ ] Add memory mapping utilities
  - Virtual to physical address conversion
  - Page allocation functions

**Success Criteria:**
- Can allocate memory (Vec, Box, etc.)
- Heap is functional
- No memory corruption

**Files to Modify:**
- `crates/kernel-bare/src/memory.rs` - Memory management
- `crates/kernel-bare/Cargo.toml` - Add alloc dependency

---

### Task 4: Interrupt & Exception Handling
**Duration:** 1-2 hours  
**What:** Handle CPU exceptions and interrupts safely

**Subtasks:**
- [ ] Set up exception handlers
  - Double fault handler
  - Page fault handler
  - General protection fault handler
  
- [ ] Set up timer interrupt
  - Configure PIT (Programmable Interval Timer)
  - Handle periodic interrupts
  
- [ ] Add exception diagnostics
  - Print fault information
  - Register dump on crash

**Success Criteria:**
- Exceptions don't cause triple faults
- Can handle interrupts safely
- Useful error messages on crash

**Files to Modify:**
- `crates/kernel-bare/src/interrupts.rs` - Interrupt handling (create if needed)
- `crates/kernel-bare/src/exceptions.rs` - Exception handlers (create if needed)

---

### Task 5: Basic I/O Port Access
**Duration:** 30 minutes  
**What:** Safely access hardware I/O ports

**Subtasks:**
- [ ] Create I/O port abstraction
  - Read from I/O port
  - Write to I/O port
  
- [ ] Add device enumeration
  - Detect available hardware
  - Create device list

**Success Criteria:**
- Can read/write I/O ports safely
- Device discovery works
- No undefined behavior

**Files to Modify:**
- `crates/kernel-bare/src/io.rs` - I/O port handling (create if needed)

---

### Task 6: Testing & Validation
**Duration:** 1 hour  
**What:** Verify kernel initialization works

**Subtasks:**
- [ ] Create kernel initialization test script
  - Boot kernel in QEMU
  - Capture serial output
  - Verify all steps complete
  
- [ ] Test both boot paths
  - Kernel-mode direct boot
  - Installer-mode bootloader chainload
  
- [ ] Document boot sequence
  - Expected output at each stage
  - Troubleshooting guide

**Success Criteria:**
- Kernel boots in both modes
- Serial output shows all initialization steps
- QEMU and potential hardware both work

**Files to Create:**
- `scripts/test-kernel-init.sh` - Kernel initialization test

---

## 📈 Phase 4 Timeline

| Task | Est. Time | Priority | Status |
|------|-----------|----------|--------|
| Task 1: CPU Init | 1-2h | P0 | Not Started |
| Task 2: Serial Output | 0.5-1h | P0 | Not Started |
| Task 3: Memory Management | 1-2h | P1 | Not Started |
| Task 4: Interrupts | 1-2h | P1 | Not Started |
| Task 5: I/O Port Access | 0.5h | P2 | Not Started |
| Task 6: Testing | 1h | P0 | Not Started |
| **Total** | **5-8 hours** | — | — |

---

## 🔧 Architecture Overview for Phase 4

```
Phase 4 Kernel Initialization Flow:

Bootloader (Phase 2)
    ↓ (Transfers control)
    
Kernel Entry Point (kernel-bare/src/main.rs)
    ├─ CPU Initialization
    │  ├─ Set up GDT
    │  ├─ Set up IDT
    │  └─ Enable paging
    │
    ├─ Serial Console Setup
    │  └─ Print startup messages
    │
    ├─ Memory Management
    │  ├─ Initialize heap
    │  └─ Set up allocator
    │
    ├─ Exception Handlers
    │  ├─ Double fault
    │  ├─ Page fault
    │  └─ GPF handler
    │
    ├─ Interrupt Setup
    │  ├─ Timer interrupt
    │  └─ Exception handlers
    │
    └─ Device I/O
       ├─ Port access
       └─ Device detection
       
    ↓
    
Kernel Main Loop
    ├─ Idle loop
    ├─ Handle interrupts
    └─ Future: Process management
```

---

## 💾 Code Structure for Phase 4

**Proposed kernel directory structure:**
```
crates/kernel-bare/
├── src/
│   ├── main.rs              (kernel entry, initialization)
│   ├── cpu.rs               (CPU setup, GDT, IDT)
│   ├── serial.rs            (UART serial driver)
│   ├── memory.rs            (heap allocator)
│   ├── interrupts.rs        (interrupt handlers)
│   ├── exceptions.rs        (exception handlers)
│   ├── io.rs                (I/O port access)
│   ├── lib.rs               (library exports)
│   └── asm/
│       └── boot.s           (assembly entry point)
├── Cargo.toml               (with alloc dependency)
└── build.rs                 (conditional compilation)
```

---

## 🧪 Testing Strategy for Phase 4

### Automated Tests
- QEMU kernel initialization test
- Serial output verification
- Memory allocation test
- Exception handling test

### Manual Verification
- Boot from USB on real hardware
- Observe serial console output
- Verify no crashes on startup

### Success Metrics
- Kernel boots successfully
- Serial output appears within 1 second
- No exceptions during initialization
- Can allocate memory from kernel

---

## 📚 Resources & References

**x86_64 Architecture:**
- Intel 64 and IA-32 Architectures Software Developer's Manual
- OSDev.org x86_64 documentation
- Bootloader specs (UEFI, multiboot)

**Rust Bare Metal:**
- `x86_64` crate documentation
- `uefi` crate for bootloader interaction
- `cortex-m` (similar embedded patterns)

**Previous RayOS Docs:**
- BOOTLOADER_CHAINLOADING.md - How bootloader works
- PHASE_3_BOOT_TESTING_GUIDE.md - How to test
- bootloader source in crates/bootloader/

---

## 🚀 Getting Started with Phase 4

### Prerequisites
- [ ] Phase 3 complete (boot media created)
- [ ] QEMU environment working
- [ ] Rust toolchain configured
- [ ] Serial console ready for capture

### First Steps
1. Review kernel entry point in `crates/kernel-bare/src/main.rs`
2. Set up CPU initialization (Task 1)
3. Add serial output (Task 2)
4. Run test script to verify
5. Continue with remaining tasks

### Quick Commands
```bash
# Build kernel
cd crates/kernel-bare
cargo build --release --target x86_64-rayos-kernel

# Test kernel boot
bash scripts/test-kernel-init.sh

# Check serial output
cat qemu-kernel-boot-output.txt
```

---

## 📋 Dependencies & Prerequisites

**Build Tools:**
- Rust nightly (already configured)
- x86_64 target (already configured)
- QEMU with OVMF (already available)

**Crates to Add:**
- `x86_64` - CPU operations
- `volatile` - Volatile register access
- `spin` - Spinlock for simple sync

**Hardware Knowledge:**
- x86_64 CPU architecture
- Memory paging basics
- Interrupt/exception handling
- UART serial communication

---

## ⚠️ Known Challenges

1. **Bootloader to Kernel Handoff**
   - Ensure bootloader leaves CPU in compatible state
   - Handle memory layout transition
   - Preserve boot information

2. **Stack Initialization**
   - Kernel needs valid stack from bootloader
   - May need to relocate stack
   - Handle stack overflow

3. **Memory Layout**
   - Bootloader uses lower memory
   - Kernel typically at higher addresses
   - Manage address spaces carefully

4. **Interrupt Handling**
   - Must be set up before any interrupts
   - Exceptions can occur during setup
   - Need safe early exception handlers

---

## ✅ Success Criteria for Phase 4

**All the Following Must Be True:**

1. **Kernel Boots Successfully**
   - Bootloader transfers control to kernel
   - No triple faults or immediate crashes
   - CPU is in protected mode with paging

2. **Serial Output Works**
   - Kernel prints startup messages
   - Messages appear on serial console
   - Can see initialization progress

3. **No Memory Corruption**
   - Heap allocation works
   - Allocated memory is accessible
   - No undefined behavior

4. **Exception Handling**
   - Double faults don't crash system
   - Page faults are handled gracefully
   - Can see error messages

5. **Code Quality**
   - 0 compilation errors
   - Unsafe code justified and marked
   - Comments explain critical sections

6. **Tests Pass**
   - QEMU boot test passes
   - Serial output matches expectations
   - Both boot paths (kernel & installer) work

---

## 📝 Notes

- Phase 4 is foundational for everything after (processes, virtual memory, drivers)
- Focus on stability and clear error messages
- Test thoroughly on both QEMU and real hardware if possible
- Document all assumptions about bootloader state
- Keep code modular for future expansion

---

**Status:** Ready to begin Phase 4  
**Next Action:** Start Task 1 - Kernel Entry Point & CPU Initialization

For questions, see DOCUMENTATION_INDEX.md or check previous phase documentation.
