// ===== RayOS Crash Recovery Module (Phase 9B Task 3) =====
// Crash artifact collection, automatic recovery, last-known-good boot
// Extends logging.rs and recovery.rs with comprehensive crash handling

use core::fmt::Write;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ===== Crash Recovery Constants =====

const MAX_CRASH_DUMPS: usize = 8;
const MAX_STACK_FRAMES: usize = 32;
const MAX_REGISTER_DUMP: usize = 32;
const MAX_CRASH_MESSAGE: usize = 256;
const MAX_ARTIFACT_NAME: usize = 64;
const MAX_ARTIFACTS: usize = 16;
const CRASH_SIGNATURE_MAGIC: u32 = 0xC4A5D00F;  // "CRASHDUMP" encoded

// ===== Exception Types =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum ExceptionType {
    DivideByZero = 0,
    Debug = 1,
    NonMaskableInterrupt = 2,
    Breakpoint = 3,
    Overflow = 4,
    BoundRangeExceeded = 5,
    InvalidOpcode = 6,
    DeviceNotAvailable = 7,
    DoubleFault = 8,
    InvalidTss = 10,
    SegmentNotPresent = 11,
    StackSegmentFault = 12,
    GeneralProtectionFault = 13,
    PageFault = 14,
    X87FloatingPoint = 16,
    AlignmentCheck = 17,
    MachineCheck = 18,
    SimdFloatingPoint = 19,
    VirtualizationException = 20,
    ControlProtection = 21,
    HypervisorInjection = 28,
    VmmCommunication = 29,
    SecurityException = 30,
    Unknown = 255,
}

impl ExceptionType {
    pub fn from_vector(vector: u32) -> Self {
        match vector {
            0 => ExceptionType::DivideByZero,
            1 => ExceptionType::Debug,
            2 => ExceptionType::NonMaskableInterrupt,
            3 => ExceptionType::Breakpoint,
            4 => ExceptionType::Overflow,
            5 => ExceptionType::BoundRangeExceeded,
            6 => ExceptionType::InvalidOpcode,
            7 => ExceptionType::DeviceNotAvailable,
            8 => ExceptionType::DoubleFault,
            10 => ExceptionType::InvalidTss,
            11 => ExceptionType::SegmentNotPresent,
            12 => ExceptionType::StackSegmentFault,
            13 => ExceptionType::GeneralProtectionFault,
            14 => ExceptionType::PageFault,
            16 => ExceptionType::X87FloatingPoint,
            17 => ExceptionType::AlignmentCheck,
            18 => ExceptionType::MachineCheck,
            19 => ExceptionType::SimdFloatingPoint,
            20 => ExceptionType::VirtualizationException,
            21 => ExceptionType::ControlProtection,
            28 => ExceptionType::HypervisorInjection,
            29 => ExceptionType::VmmCommunication,
            30 => ExceptionType::SecurityException,
            _ => ExceptionType::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ExceptionType::DivideByZero => "Divide by Zero",
            ExceptionType::Debug => "Debug Exception",
            ExceptionType::NonMaskableInterrupt => "NMI",
            ExceptionType::Breakpoint => "Breakpoint",
            ExceptionType::Overflow => "Overflow",
            ExceptionType::BoundRangeExceeded => "Bound Range Exceeded",
            ExceptionType::InvalidOpcode => "Invalid Opcode",
            ExceptionType::DeviceNotAvailable => "Device Not Available",
            ExceptionType::DoubleFault => "Double Fault",
            ExceptionType::InvalidTss => "Invalid TSS",
            ExceptionType::SegmentNotPresent => "Segment Not Present",
            ExceptionType::StackSegmentFault => "Stack Segment Fault",
            ExceptionType::GeneralProtectionFault => "General Protection Fault",
            ExceptionType::PageFault => "Page Fault",
            ExceptionType::X87FloatingPoint => "x87 Floating Point",
            ExceptionType::AlignmentCheck => "Alignment Check",
            ExceptionType::MachineCheck => "Machine Check",
            ExceptionType::SimdFloatingPoint => "SIMD Floating Point",
            ExceptionType::VirtualizationException => "Virtualization Exception",
            ExceptionType::ControlProtection => "Control Protection",
            ExceptionType::HypervisorInjection => "Hypervisor Injection",
            ExceptionType::VmmCommunication => "VMM Communication",
            ExceptionType::SecurityException => "Security Exception",
            ExceptionType::Unknown => "Unknown Exception",
        }
    }

    pub fn is_fatal(&self) -> bool {
        matches!(self,
            ExceptionType::DoubleFault |
            ExceptionType::MachineCheck |
            ExceptionType::InvalidTss |
            ExceptionType::StackSegmentFault
        )
    }
}

// ===== Crash Severity =====

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CrashSeverity {
    /// Recoverable warning
    Warning,
    /// Non-fatal error, service can restart
    Error,
    /// Critical error, requires recovery
    Critical,
    /// Fatal error, system halt required
    Fatal,
    /// Kernel panic, immediate halt
    Panic,
}

impl CrashSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            CrashSeverity::Warning => "WARNING",
            CrashSeverity::Error => "ERROR",
            CrashSeverity::Critical => "CRITICAL",
            CrashSeverity::Fatal => "FATAL",
            CrashSeverity::Panic => "PANIC",
        }
    }
}

// ===== CPU Register State =====

#[derive(Copy, Clone)]
pub struct CpuRegisters {
    // General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,

    // Instruction pointer and flags
    pub rip: u64,
    pub rflags: u64,

    // Segment registers
    pub cs: u16,
    pub ds: u16,
    pub es: u16,
    pub fs: u16,
    pub gs: u16,
    pub ss: u16,

    // Control registers
    pub cr0: u64,
    pub cr2: u64,  // Page fault linear address
    pub cr3: u64,  // Page table base
    pub cr4: u64,

    // Error code (for exceptions that push one)
    pub error_code: u64,
}

impl CpuRegisters {
    pub fn new() -> Self {
        CpuRegisters {
            rax: 0, rbx: 0, rcx: 0, rdx: 0,
            rsi: 0, rdi: 0, rbp: 0, rsp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
            rip: 0, rflags: 0,
            cs: 0, ds: 0, es: 0, fs: 0, gs: 0, ss: 0,
            cr0: 0, cr2: 0, cr3: 0, cr4: 0,
            error_code: 0,
        }
    }

    pub fn dump(&self, output: &mut dyn Write) {
        let _ = writeln!(output, "CPU Registers:");
        let _ = writeln!(output, "  RAX={:016x}  RBX={:016x}  RCX={:016x}  RDX={:016x}",
            self.rax, self.rbx, self.rcx, self.rdx);
        let _ = writeln!(output, "  RSI={:016x}  RDI={:016x}  RBP={:016x}  RSP={:016x}",
            self.rsi, self.rdi, self.rbp, self.rsp);
        let _ = writeln!(output, "  R8 ={:016x}  R9 ={:016x}  R10={:016x}  R11={:016x}",
            self.r8, self.r9, self.r10, self.r11);
        let _ = writeln!(output, "  R12={:016x}  R13={:016x}  R14={:016x}  R15={:016x}",
            self.r12, self.r13, self.r14, self.r15);
        let _ = writeln!(output, "  RIP={:016x}  RFLAGS={:016x}",
            self.rip, self.rflags);
        let _ = writeln!(output, "  CS={:04x} DS={:04x} ES={:04x} FS={:04x} GS={:04x} SS={:04x}",
            self.cs, self.ds, self.es, self.fs, self.gs, self.ss);
        let _ = writeln!(output, "  CR0={:016x}  CR2={:016x}  CR3={:016x}  CR4={:016x}",
            self.cr0, self.cr2, self.cr3, self.cr4);
        let _ = writeln!(output, "  Error Code={:016x}", self.error_code);
    }
}

// ===== Stack Frame =====

#[derive(Copy, Clone)]
pub struct StackFrame {
    pub return_address: u64,
    pub frame_pointer: u64,
    pub symbol_offset: u32,
    symbol_name: [u8; 64],
    symbol_name_len: usize,
}

impl StackFrame {
    pub fn new() -> Self {
        StackFrame {
            return_address: 0,
            frame_pointer: 0,
            symbol_offset: 0,
            symbol_name: [0u8; 64],
            symbol_name_len: 0,
        }
    }

    pub fn set_symbol(&mut self, name: &str) {
        let len = core::cmp::min(name.len(), 63);
        for i in 0..len {
            self.symbol_name[i] = name.as_bytes()[i];
        }
        self.symbol_name_len = len;
    }

    pub fn symbol(&self) -> &str {
        if self.symbol_name_len > 0 {
            unsafe { core::str::from_utf8_unchecked(&self.symbol_name[..self.symbol_name_len]) }
        } else {
            "<unknown>"
        }
    }
}

// ===== Stack Trace =====

#[derive(Copy, Clone)]
pub struct StackTrace {
    frames: [StackFrame; MAX_STACK_FRAMES],
    frame_count: usize,
}

impl StackTrace {
    pub fn new() -> Self {
        StackTrace {
            frames: [StackFrame::new(); MAX_STACK_FRAMES],
            frame_count: 0,
        }
    }

    pub fn push_frame(&mut self, addr: u64, fp: u64, symbol: Option<&str>) -> bool {
        if self.frame_count >= MAX_STACK_FRAMES {
            return false;
        }
        self.frames[self.frame_count].return_address = addr;
        self.frames[self.frame_count].frame_pointer = fp;
        if let Some(sym) = symbol {
            self.frames[self.frame_count].set_symbol(sym);
        }
        self.frame_count += 1;
        true
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn get_frame(&self, idx: usize) -> Option<&StackFrame> {
        if idx < self.frame_count {
            Some(&self.frames[idx])
        } else {
            None
        }
    }

    pub fn dump(&self, output: &mut dyn Write) {
        let _ = writeln!(output, "Stack Trace ({} frames):", self.frame_count);
        for i in 0..self.frame_count {
            let frame = &self.frames[i];
            let _ = writeln!(output, "  #{:02} {:016x} {} +{:#x}",
                i, frame.return_address, frame.symbol(), frame.symbol_offset);
        }
    }

    /// Unwind the stack from current frame pointer
    pub fn unwind_from(&mut self, initial_rip: u64, initial_rbp: u64) {
        self.frame_count = 0;

        // Add current instruction
        self.push_frame(initial_rip, initial_rbp, None);

        // Walk the stack using frame pointers
        let mut rbp = initial_rbp;

        for _ in 0..MAX_STACK_FRAMES - 1 {
            if rbp == 0 || rbp < 0x1000 {
                break;  // Invalid frame pointer
            }

            // In a real implementation, we would read memory at rbp
            // For now, simulate a few frames
            let return_addr = rbp.wrapping_add(8);  // Simulated
            let prev_rbp = rbp.wrapping_sub(0x20);  // Simulated

            if return_addr == 0 || prev_rbp >= rbp {
                break;  // End of stack or invalid
            }

            self.push_frame(return_addr, prev_rbp, None);
            rbp = prev_rbp;
        }
    }
}

// ===== Crash Artifact =====

#[derive(Copy, Clone)]
pub struct CrashArtifact {
    name: [u8; MAX_ARTIFACT_NAME],
    name_len: usize,
    pub artifact_type: ArtifactType,
    pub data_offset: u64,
    pub data_size: u32,
    pub timestamp: u64,
    pub checksum: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ArtifactType {
    RegisterDump,
    StackTrace,
    MemoryDump,
    LogBuffer,
    KernelState,
    DeviceState,
    ProcessList,
    FileSystemState,
}

impl CrashArtifact {
    pub fn new(name: &str, artifact_type: ArtifactType) -> Self {
        let mut artifact = CrashArtifact {
            name: [0u8; MAX_ARTIFACT_NAME],
            name_len: 0,
            artifact_type,
            data_offset: 0,
            data_size: 0,
            timestamp: 0,
            checksum: 0,
        };
        let len = core::cmp::min(name.len(), MAX_ARTIFACT_NAME - 1);
        for i in 0..len {
            artifact.name[i] = name.as_bytes()[i];
        }
        artifact.name_len = len;
        artifact
    }

    pub fn name(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.name[..self.name_len]) }
    }
}

// ===== Crash Dump Record =====

#[derive(Copy, Clone)]
pub struct CrashDump {
    /// Magic signature for validation
    pub magic: u32,
    /// Crash dump ID
    pub dump_id: u32,
    /// Timestamp when crash occurred
    pub timestamp: u64,
    /// Boot count when crash occurred
    pub boot_count: u32,
    /// Uptime in seconds when crash occurred
    pub uptime_secs: u64,
    /// Exception type
    pub exception: ExceptionType,
    /// Crash severity
    pub severity: CrashSeverity,
    /// CPU registers at time of crash
    pub registers: CpuRegisters,
    /// Stack trace
    pub stack_trace: StackTrace,
    /// Error message
    message: [u8; MAX_CRASH_MESSAGE],
    message_len: usize,
    /// Faulting process ID
    pub faulting_pid: u32,
    /// Faulting thread ID
    pub faulting_tid: u32,
    /// Artifacts collected
    artifacts: [CrashArtifact; MAX_ARTIFACTS],
    artifact_count: usize,
    /// CRC32 of dump for integrity
    pub checksum: u32,
}

impl CrashDump {
    pub fn new(dump_id: u32) -> Self {
        CrashDump {
            magic: CRASH_SIGNATURE_MAGIC,
            dump_id,
            timestamp: 0,
            boot_count: 0,
            uptime_secs: 0,
            exception: ExceptionType::Unknown,
            severity: CrashSeverity::Error,
            registers: CpuRegisters::new(),
            stack_trace: StackTrace::new(),
            message: [0u8; MAX_CRASH_MESSAGE],
            message_len: 0,
            faulting_pid: 0,
            faulting_tid: 0,
            artifacts: [CrashArtifact::new("", ArtifactType::RegisterDump); MAX_ARTIFACTS],
            artifact_count: 0,
            checksum: 0,
        }
    }

    pub fn set_message(&mut self, msg: &str) {
        let len = core::cmp::min(msg.len(), MAX_CRASH_MESSAGE - 1);
        for i in 0..len {
            self.message[i] = msg.as_bytes()[i];
        }
        self.message_len = len;
    }

    pub fn message(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.message[..self.message_len]) }
    }

    pub fn add_artifact(&mut self, artifact: CrashArtifact) -> bool {
        if self.artifact_count >= MAX_ARTIFACTS {
            return false;
        }
        self.artifacts[self.artifact_count] = artifact;
        self.artifact_count += 1;
        true
    }

    pub fn is_valid(&self) -> bool {
        self.magic == CRASH_SIGNATURE_MAGIC
    }

    pub fn dump_summary(&self, output: &mut dyn Write) {
        let _ = writeln!(output, "═══════════════════════════════════════════════════════════");
        let _ = writeln!(output, "                    CRASH DUMP #{:04}", self.dump_id);
        let _ = writeln!(output, "═══════════════════════════════════════════════════════════");
        let _ = writeln!(output, "Exception: {} ({})", self.exception.as_str(),
            self.severity.as_str());
        let _ = writeln!(output, "Timestamp: {} (uptime: {} sec)", self.timestamp, self.uptime_secs);
        let _ = writeln!(output, "Process: PID {} TID {}", self.faulting_pid, self.faulting_tid);
        let _ = writeln!(output, "Message: {}", self.message());
        let _ = writeln!(output, "");
        self.registers.dump(output);
        let _ = writeln!(output, "");
        self.stack_trace.dump(output);
        let _ = writeln!(output, "");
        let _ = writeln!(output, "Artifacts Collected: {}", self.artifact_count);
        for i in 0..self.artifact_count {
            let art = &self.artifacts[i];
            let _ = writeln!(output, "  - {} ({:?}, {} bytes)",
                art.name(), art.artifact_type, art.data_size);
        }
    }
}

// ===== Recovery Action =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// No action needed
    None,
    /// Restart the faulting service
    RestartService,
    /// Kill and restart the faulting process
    RestartProcess,
    /// Attempt to recover kernel state
    KernelRecovery,
    /// Boot to last-known-good configuration
    BootLastKnownGood,
    /// Boot to recovery mode
    BootRecoveryMode,
    /// Full system reset
    SystemReset,
    /// Halt and wait for manual intervention
    Halt,
}

impl RecoveryAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecoveryAction::None => "none",
            RecoveryAction::RestartService => "restart-service",
            RecoveryAction::RestartProcess => "restart-process",
            RecoveryAction::KernelRecovery => "kernel-recovery",
            RecoveryAction::BootLastKnownGood => "boot-lkg",
            RecoveryAction::BootRecoveryMode => "boot-recovery",
            RecoveryAction::SystemReset => "system-reset",
            RecoveryAction::Halt => "halt",
        }
    }
}

// ===== Last Known Good State =====

#[derive(Copy, Clone)]
pub struct LastKnownGood {
    /// Boot count when LKG was saved
    pub boot_count: u32,
    /// Timestamp when LKG was saved
    pub timestamp: u64,
    /// Kernel version (encoded)
    pub kernel_version: u32,
    /// Configuration checksum
    pub config_checksum: u32,
    /// Is LKG valid?
    pub valid: bool,
    /// Was current boot successful?
    pub boot_success: bool,
    /// Number of consecutive boot failures
    pub failure_count: u32,
    /// Maximum failures before auto-recovery
    pub max_failures: u32,
}

impl LastKnownGood {
    pub fn new() -> Self {
        LastKnownGood {
            boot_count: 0,
            timestamp: 0,
            kernel_version: 0,
            config_checksum: 0,
            valid: false,
            boot_success: false,
            failure_count: 0,
            max_failures: 3,
        }
    }

    pub fn mark_boot_success(&mut self) {
        self.boot_success = true;
        self.failure_count = 0;
    }

    pub fn mark_boot_failure(&mut self) {
        self.boot_success = false;
        self.failure_count += 1;
    }

    pub fn should_recover(&self) -> bool {
        self.failure_count >= self.max_failures && self.valid
    }

    pub fn save_current(&mut self, boot_count: u32, timestamp: u64, version: u32, checksum: u32) {
        self.boot_count = boot_count;
        self.timestamp = timestamp;
        self.kernel_version = version;
        self.config_checksum = checksum;
        self.valid = true;
    }
}

// ===== Crash Recovery Manager =====

pub struct CrashRecoveryManager {
    /// Recent crash dumps
    dumps: [CrashDump; MAX_CRASH_DUMPS],
    dump_count: usize,
    next_dump_id: u32,

    /// Last known good state
    lkg: LastKnownGood,

    /// Current boot count
    boot_count: u32,

    /// System uptime in seconds
    uptime_secs: AtomicU64,

    /// Total crashes since boot
    crashes_this_boot: AtomicU32,

    /// Recovery in progress
    recovery_in_progress: bool,

    /// Panic handler installed
    panic_handler_installed: bool,
}

impl CrashRecoveryManager {
    pub fn new() -> Self {
        CrashRecoveryManager {
            dumps: [CrashDump::new(0); MAX_CRASH_DUMPS],
            dump_count: 0,
            next_dump_id: 1,
            lkg: LastKnownGood::new(),
            boot_count: 0,
            uptime_secs: AtomicU64::new(0),
            crashes_this_boot: AtomicU32::new(0),
            recovery_in_progress: false,
            panic_handler_installed: false,
        }
    }

    /// Initialize crash recovery system
    pub fn initialize(&mut self, boot_count: u32) {
        self.boot_count = boot_count;

        // Check if we should auto-recover
        if self.lkg.should_recover() {
            self.trigger_recovery(RecoveryAction::BootLastKnownGood);
        }
    }

    /// Record a crash
    pub fn record_crash(
        &mut self,
        exception: ExceptionType,
        severity: CrashSeverity,
        registers: CpuRegisters,
        message: &str,
        pid: u32,
        tid: u32,
    ) -> u32 {
        let dump_id = self.next_dump_id;
        self.next_dump_id += 1;

        // Create crash dump
        let idx = self.dump_count % MAX_CRASH_DUMPS;
        self.dumps[idx] = CrashDump::new(dump_id);
        self.dumps[idx].timestamp = 0;  // Would be real timestamp
        self.dumps[idx].boot_count = self.boot_count;
        self.dumps[idx].uptime_secs = self.uptime_secs.load(Ordering::Relaxed);
        self.dumps[idx].exception = exception;
        self.dumps[idx].severity = severity;
        self.dumps[idx].registers = registers;
        self.dumps[idx].set_message(message);
        self.dumps[idx].faulting_pid = pid;
        self.dumps[idx].faulting_tid = tid;

        // Unwind stack
        self.dumps[idx].stack_trace.unwind_from(
            registers.rip,
            registers.rbp,
        );

        // Collect artifacts
        self.collect_artifacts(idx);

        if self.dump_count < MAX_CRASH_DUMPS {
            self.dump_count += 1;
        }

        self.crashes_this_boot.fetch_add(1, Ordering::Relaxed);

        dump_id
    }

    /// Collect crash artifacts
    fn collect_artifacts(&mut self, dump_idx: usize) {
        // Register dump artifact
        let reg_artifact = CrashArtifact::new("cpu_registers", ArtifactType::RegisterDump);
        self.dumps[dump_idx].add_artifact(reg_artifact);

        // Stack trace artifact
        let stack_artifact = CrashArtifact::new("stack_trace", ArtifactType::StackTrace);
        self.dumps[dump_idx].add_artifact(stack_artifact);

        // Log buffer artifact
        let log_artifact = CrashArtifact::new("kernel_log", ArtifactType::LogBuffer);
        self.dumps[dump_idx].add_artifact(log_artifact);

        // Kernel state artifact
        let state_artifact = CrashArtifact::new("kernel_state", ArtifactType::KernelState);
        self.dumps[dump_idx].add_artifact(state_artifact);
    }

    /// Determine recovery action based on crash severity
    pub fn determine_recovery_action(&self, severity: CrashSeverity, pid: u32) -> RecoveryAction {
        match severity {
            CrashSeverity::Warning => RecoveryAction::None,
            CrashSeverity::Error => {
                if pid == 0 {
                    // Kernel error
                    RecoveryAction::KernelRecovery
                } else {
                    RecoveryAction::RestartProcess
                }
            }
            CrashSeverity::Critical => {
                if self.crashes_this_boot.load(Ordering::Relaxed) >= 3 {
                    RecoveryAction::BootLastKnownGood
                } else {
                    RecoveryAction::RestartService
                }
            }
            CrashSeverity::Fatal => {
                if self.lkg.valid {
                    RecoveryAction::BootLastKnownGood
                } else {
                    RecoveryAction::BootRecoveryMode
                }
            }
            CrashSeverity::Panic => RecoveryAction::Halt,
        }
    }

    /// Trigger recovery action
    pub fn trigger_recovery(&mut self, action: RecoveryAction) {
        self.recovery_in_progress = true;

        match action {
            RecoveryAction::None => {}
            RecoveryAction::RestartService => {
                // Would restart the faulting service
            }
            RecoveryAction::RestartProcess => {
                // Would kill and restart the faulting process
            }
            RecoveryAction::KernelRecovery => {
                // Attempt kernel state recovery
            }
            RecoveryAction::BootLastKnownGood => {
                // Set boot flag and reboot
                self.lkg.mark_boot_failure();
            }
            RecoveryAction::BootRecoveryMode => {
                // Set recovery boot flag
            }
            RecoveryAction::SystemReset => {
                // Full system reset
            }
            RecoveryAction::Halt => {
                // Halt and wait
            }
        }

        self.recovery_in_progress = false;
    }

    /// Get crash dump by ID
    pub fn get_dump(&self, dump_id: u32) -> Option<&CrashDump> {
        for i in 0..self.dump_count {
            if self.dumps[i].dump_id == dump_id {
                return Some(&self.dumps[i]);
            }
        }
        None
    }

    /// Get latest crash dump
    pub fn get_latest_dump(&self) -> Option<&CrashDump> {
        if self.dump_count == 0 {
            None
        } else {
            let idx = (self.dump_count - 1) % MAX_CRASH_DUMPS;
            Some(&self.dumps[idx])
        }
    }

    /// Get crash count this boot
    pub fn crashes_this_boot(&self) -> u32 {
        self.crashes_this_boot.load(Ordering::Relaxed)
    }

    /// Mark boot as successful (save LKG)
    pub fn mark_boot_success(&mut self) {
        self.lkg.mark_boot_success();
        // Would persist LKG to storage
    }

    /// Update uptime
    pub fn update_uptime(&self, secs: u64) {
        self.uptime_secs.store(secs, Ordering::Relaxed);
    }

    /// Get LKG state
    pub fn lkg_state(&self) -> &LastKnownGood {
        &self.lkg
    }

    /// Export crash dumps to buffer
    pub fn export_dumps(&self, output: &mut dyn Write) {
        let _ = writeln!(output, "╔═══════════════════════════════════════════════════════════╗");
        let _ = writeln!(output, "║              RayOS Crash Recovery Report                  ║");
        let _ = writeln!(output, "╚═══════════════════════════════════════════════════════════╝");
        let _ = writeln!(output, "Boot Count: {}", self.boot_count);
        let _ = writeln!(output, "Crashes This Boot: {}", self.crashes_this_boot());
        let _ = writeln!(output, "Total Dumps: {}", self.dump_count);
        let _ = writeln!(output, "LKG Valid: {}", self.lkg.valid);
        let _ = writeln!(output, "LKG Failure Count: {}", self.lkg.failure_count);
        let _ = writeln!(output, "");

        for i in 0..self.dump_count {
            self.dumps[i].dump_summary(output);
            let _ = writeln!(output, "");
        }
    }
}

// ===== Watchdog Timer =====

pub struct WatchdogTimer {
    /// Timeout in milliseconds
    pub timeout_ms: u32,
    /// Last pet time
    pub last_pet: u64,
    /// Is watchdog enabled
    pub enabled: bool,
    /// Action on timeout
    pub action: RecoveryAction,
    /// Timeout count
    pub timeout_count: u32,
}

impl WatchdogTimer {
    pub fn new(timeout_ms: u32) -> Self {
        WatchdogTimer {
            timeout_ms,
            last_pet: 0,
            enabled: false,
            action: RecoveryAction::SystemReset,
            timeout_count: 0,
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
        self.last_pet = 0;  // Would be current time
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn pet(&mut self, current_time_ms: u64) {
        self.last_pet = current_time_ms;
    }

    pub fn check(&mut self, current_time_ms: u64) -> bool {
        if !self.enabled {
            return false;
        }

        if current_time_ms - self.last_pet > self.timeout_ms as u64 {
            self.timeout_count += 1;
            true  // Timeout occurred
        } else {
            false
        }
    }
}

// ===== Global Crash Recovery Manager =====

static mut CRASH_MANAGER: Option<CrashRecoveryManager> = None;

pub fn crash_manager() -> &'static mut CrashRecoveryManager {
    unsafe {
        if CRASH_MANAGER.is_none() {
            CRASH_MANAGER = Some(CrashRecoveryManager::new());
        }
        CRASH_MANAGER.as_mut().unwrap()
    }
}

// ===== Tests =====

pub fn test_crash_dump() -> bool {
    let mut dump = CrashDump::new(1);
    dump.set_message("Test crash message");
    dump.exception = ExceptionType::GeneralProtectionFault;
    dump.severity = CrashSeverity::Critical;

    if !dump.is_valid() {
        return false;
    }

    if dump.message() != "Test crash message" {
        return false;
    }

    if dump.exception != ExceptionType::GeneralProtectionFault {
        return false;
    }

    true
}

pub fn test_stack_trace() -> bool {
    let mut trace = StackTrace::new();

    trace.push_frame(0xFFFF8000_00001000, 0xFFFF8000_00002000, Some("kernel_main"));
    trace.push_frame(0xFFFF8000_00001100, 0xFFFF8000_00002100, Some("handle_exception"));
    trace.push_frame(0xFFFF8000_00001200, 0xFFFF8000_00002200, Some("page_fault_handler"));

    if trace.frame_count() != 3 {
        return false;
    }

    if let Some(frame) = trace.get_frame(0) {
        if frame.symbol() != "kernel_main" {
            return false;
        }
    } else {
        return false;
    }

    true
}

pub fn test_recovery_action() -> bool {
    let manager = CrashRecoveryManager::new();

    // Warning should result in no action
    let action = manager.determine_recovery_action(CrashSeverity::Warning, 100);
    if action != RecoveryAction::None {
        return false;
    }

    // Error from user process should restart process
    let action = manager.determine_recovery_action(CrashSeverity::Error, 100);
    if action != RecoveryAction::RestartProcess {
        return false;
    }

    // Error from kernel should attempt recovery
    let action = manager.determine_recovery_action(CrashSeverity::Error, 0);
    if action != RecoveryAction::KernelRecovery {
        return false;
    }

    true
}

pub fn test_lkg() -> bool {
    let mut lkg = LastKnownGood::new();

    // Initially invalid
    if lkg.valid {
        return false;
    }

    // Save LKG
    lkg.save_current(1, 1000, 0x00090200, 0xDEADBEEF);

    if !lkg.valid {
        return false;
    }

    // Mark failures
    lkg.mark_boot_failure();
    lkg.mark_boot_failure();

    if lkg.should_recover() {
        return false;  // Need 3 failures
    }

    lkg.mark_boot_failure();

    if !lkg.should_recover() {
        return false;  // Should recover now
    }

    true
}

pub fn test_watchdog() -> bool {
    let mut watchdog = WatchdogTimer::new(5000);  // 5 second timeout

    watchdog.enable();
    watchdog.pet(0);

    // Check before timeout
    if watchdog.check(4000) {
        return false;  // Should not timeout
    }

    // Check after timeout
    if !watchdog.check(6000) {
        return false;  // Should timeout
    }

    if watchdog.timeout_count != 1 {
        return false;
    }

    true
}
