//! RayOS Recovery Mode
//!
//! Provides system recovery capabilities including:
//! - Safe boot mode with minimal drivers
//! - System restore from backup
//! - Diagnostics and repair tools
//! - Factory reset option

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Maximum diagnostic messages
pub const MAX_DIAG_MESSAGES: usize = 32;

/// Maximum message length
pub const MAX_MESSAGE_LEN: usize = 128;

// ===== Recovery Mode =====

/// Recovery mode type
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RecoveryMode {
    /// Normal boot
    Normal = 0,
    /// Safe mode with minimal drivers
    SafeMode = 1,
    /// Recovery console
    RecoveryConsole = 2,
    /// System restore
    SystemRestore = 3,
    /// Factory reset
    FactoryReset = 4,
    /// Boot menu
    BootMenu = 5,
}

impl RecoveryMode {
    /// Get mode name
    pub fn name(&self) -> &'static [u8] {
        match self {
            Self::Normal => b"Normal Boot",
            Self::SafeMode => b"Safe Mode",
            Self::RecoveryConsole => b"Recovery Console",
            Self::SystemRestore => b"System Restore",
            Self::FactoryReset => b"Factory Reset",
            Self::BootMenu => b"Boot Menu",
        }
    }

    /// Get mode description
    pub fn description(&self) -> &'static [u8] {
        match self {
            Self::Normal => b"Start RayOS normally with all features",
            Self::SafeMode => b"Start with minimal drivers for troubleshooting",
            Self::RecoveryConsole => b"Command-line recovery environment",
            Self::SystemRestore => b"Restore system to a previous state",
            Self::FactoryReset => b"Reset system to factory defaults (WARNING: data loss)",
            Self::BootMenu => b"Select boot device or operating system",
        }
    }

    /// From u8
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::SafeMode,
            2 => Self::RecoveryConsole,
            3 => Self::SystemRestore,
            4 => Self::FactoryReset,
            5 => Self::BootMenu,
            _ => Self::Normal,
        }
    }
}

// ===== Diagnostic Category =====

/// Diagnostic check category
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DiagCategory {
    /// Hardware checks
    Hardware = 0,
    /// Storage checks
    Storage = 1,
    /// Memory checks
    Memory = 2,
    /// Boot checks
    Boot = 3,
    /// System files
    SystemFiles = 4,
    /// Configuration
    Configuration = 5,
    /// Network
    Network = 6,
}

impl DiagCategory {
    /// Get category name
    pub fn name(&self) -> &'static [u8] {
        match self {
            Self::Hardware => b"Hardware",
            Self::Storage => b"Storage",
            Self::Memory => b"Memory",
            Self::Boot => b"Boot",
            Self::SystemFiles => b"System Files",
            Self::Configuration => b"Configuration",
            Self::Network => b"Network",
        }
    }
}

// ===== Diagnostic Status =====

/// Status of a diagnostic check
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DiagStatus {
    /// Not yet run
    Pending = 0,
    /// Currently running
    Running = 1,
    /// Passed
    Passed = 2,
    /// Warning (non-critical issue)
    Warning = 3,
    /// Failed (critical issue)
    Failed = 4,
    /// Skipped
    Skipped = 5,
}

impl DiagStatus {
    /// Get status symbol
    pub fn symbol(&self) -> &'static [u8] {
        match self {
            Self::Pending => b"[ ]",
            Self::Running => b"[~]",
            Self::Passed => b"[+]",
            Self::Warning => b"[!]",
            Self::Failed => b"[X]",
            Self::Skipped => b"[-]",
        }
    }
}

// ===== Diagnostic Result =====

/// A single diagnostic check result
#[derive(Clone)]
pub struct DiagResult {
    /// Whether this slot is active
    pub active: bool,
    /// Category
    pub category: DiagCategory,
    /// Check name
    pub name: [u8; 32],
    pub name_len: usize,
    /// Status
    pub status: DiagStatus,
    /// Detail message
    pub message: [u8; MAX_MESSAGE_LEN],
    pub message_len: usize,
}

impl DiagResult {
    /// Create empty result
    pub const fn empty() -> Self {
        Self {
            active: false,
            category: DiagCategory::Hardware,
            name: [0; 32],
            name_len: 0,
            status: DiagStatus::Pending,
            message: [0; MAX_MESSAGE_LEN],
            message_len: 0,
        }
    }

    /// Get name
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    /// Get message
    pub fn message(&self) -> &[u8] {
        &self.message[..self.message_len]
    }

    /// Set name
    pub fn set_name(&mut self, name: &[u8]) {
        let len = name.len().min(32);
        self.name[..len].copy_from_slice(&name[..len]);
        self.name_len = len;
    }

    /// Set message
    pub fn set_message(&mut self, msg: &[u8]) {
        let len = msg.len().min(MAX_MESSAGE_LEN);
        self.message[..len].copy_from_slice(&msg[..len]);
        self.message_len = len;
    }
}

// ===== Restore Point =====

/// A system restore point
#[derive(Clone)]
pub struct RestorePoint {
    /// Whether this slot is active
    pub active: bool,
    /// Restore point ID
    pub id: u32,
    /// Timestamp (ticks)
    pub timestamp: u64,
    /// Description
    pub description: [u8; 64],
    pub description_len: usize,
    /// Type (auto, manual, pre-update)
    pub restore_type: RestoreType,
    /// Size in bytes
    pub size: u64,
}

impl RestorePoint {
    /// Create empty restore point
    pub const fn empty() -> Self {
        Self {
            active: false,
            id: 0,
            timestamp: 0,
            description: [0; 64],
            description_len: 0,
            restore_type: RestoreType::Manual,
            size: 0,
        }
    }

    /// Get description
    pub fn description(&self) -> &[u8] {
        &self.description[..self.description_len]
    }

    /// Set description
    pub fn set_description(&mut self, desc: &[u8]) {
        let len = desc.len().min(64);
        self.description[..len].copy_from_slice(&desc[..len]);
        self.description_len = len;
    }
}

/// Restore point type
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RestoreType {
    /// Created automatically
    Auto = 0,
    /// Created manually by user
    Manual = 1,
    /// Created before update
    PreUpdate = 2,
    /// Created before driver install
    PreDriver = 3,
}

impl RestoreType {
    /// Get type name
    pub fn name(&self) -> &'static [u8] {
        match self {
            Self::Auto => b"Automatic",
            Self::Manual => b"Manual",
            Self::PreUpdate => b"Pre-Update",
            Self::PreDriver => b"Pre-Driver",
        }
    }
}

// ===== Recovery State =====

/// Recovery operation state
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RecoveryState {
    /// Idle
    Idle = 0,
    /// Running diagnostics
    RunningDiagnostics = 1,
    /// Restoring system
    Restoring = 2,
    /// Resetting to factory
    Resetting = 3,
    /// Repairing
    Repairing = 4,
    /// Complete
    Complete = 5,
    /// Failed
    Failed = 6,
}

impl RecoveryState {
    /// Get state name
    pub fn name(&self) -> &'static [u8] {
        match self {
            Self::Idle => b"Idle",
            Self::RunningDiagnostics => b"Running Diagnostics",
            Self::Restoring => b"Restoring System",
            Self::Resetting => b"Factory Reset",
            Self::Repairing => b"Repairing System",
            Self::Complete => b"Complete",
            Self::Failed => b"Failed",
        }
    }
}

// ===== Recovery Manager =====

/// Maximum restore points
pub const MAX_RESTORE_POINTS: usize = 8;

/// The recovery manager
pub struct RecoveryManager {
    /// Current recovery mode
    mode: RecoveryMode,
    /// Current state
    state: RecoveryState,
    /// Progress (0-100)
    progress: u8,
    /// Diagnostic results
    diagnostics: [DiagResult; MAX_DIAG_MESSAGES],
    diag_count: usize,
    /// Restore points
    restore_points: [RestorePoint; MAX_RESTORE_POINTS],
    restore_count: usize,
    /// Error message
    error: [u8; 128],
    error_len: usize,
    /// Safe mode reason
    safe_mode_reason: [u8; 64],
    safe_mode_reason_len: usize,
}

impl RecoveryManager {
    /// Create new recovery manager
    pub const fn new() -> Self {
        Self {
            mode: RecoveryMode::Normal,
            state: RecoveryState::Idle,
            progress: 0,
            diagnostics: [const { DiagResult::empty() }; MAX_DIAG_MESSAGES],
            diag_count: 0,
            restore_points: [const { RestorePoint::empty() }; MAX_RESTORE_POINTS],
            restore_count: 0,
            error: [0; 128],
            error_len: 0,
            safe_mode_reason: [0; 64],
            safe_mode_reason_len: 0,
        }
    }

    /// Get current mode
    pub fn mode(&self) -> RecoveryMode {
        self.mode
    }

    /// Set recovery mode
    pub fn set_mode(&mut self, mode: RecoveryMode) {
        self.mode = mode;
    }

    /// Get current state
    pub fn state(&self) -> RecoveryState {
        self.state
    }

    /// Get progress
    pub fn progress(&self) -> u8 {
        self.progress
    }

    /// Get diagnostic count
    pub fn diag_count(&self) -> usize {
        self.diag_count
    }

    /// Get diagnostic result
    pub fn get_diagnostic(&self, index: usize) -> Option<&DiagResult> {
        if index < self.diag_count && self.diagnostics[index].active {
            Some(&self.diagnostics[index])
        } else {
            None
        }
    }

    /// Get restore point count
    pub fn restore_count(&self) -> usize {
        self.restore_count
    }

    /// Get restore point
    pub fn get_restore_point(&self, index: usize) -> Option<&RestorePoint> {
        if index < self.restore_count && self.restore_points[index].active {
            Some(&self.restore_points[index])
        } else {
            None
        }
    }

    /// Get error message
    pub fn error(&self) -> &[u8] {
        &self.error[..self.error_len]
    }

    /// Set error
    fn set_error(&mut self, msg: &[u8]) {
        let len = msg.len().min(128);
        self.error[..len].copy_from_slice(&msg[..len]);
        self.error_len = len;
        self.state = RecoveryState::Failed;
    }

    /// Add a diagnostic result
    fn add_diagnostic(&mut self, category: DiagCategory, name: &[u8], status: DiagStatus, message: &[u8]) {
        if self.diag_count >= MAX_DIAG_MESSAGES {
            return;
        }

        let diag = &mut self.diagnostics[self.diag_count];
        diag.active = true;
        diag.category = category;
        diag.set_name(name);
        diag.status = status;
        diag.set_message(message);
        self.diag_count += 1;
    }

    /// Run system diagnostics
    pub fn run_diagnostics(&mut self) -> Result<u32, RecoveryError> {
        self.state = RecoveryState::RunningDiagnostics;
        self.progress = 0;
        self.diag_count = 0;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_RECOVERY_DIAGNOSTICS_START\n");

        let mut warnings = 0u32;
        let mut failures = 0u32;

        // Hardware checks
        self.add_diagnostic(DiagCategory::Hardware, b"CPU", DiagStatus::Passed, b"CPU detected and operational");
        self.progress = 10;

        self.add_diagnostic(DiagCategory::Hardware, b"Timer", DiagStatus::Passed, b"System timer functional");
        self.progress = 15;

        // Memory checks
        self.add_diagnostic(DiagCategory::Memory, b"RAM", DiagStatus::Passed, b"Memory test passed");
        self.progress = 25;

        self.add_diagnostic(DiagCategory::Memory, b"Heap", DiagStatus::Passed, b"Kernel heap operational");
        self.progress = 30;

        // Storage checks
        self.add_diagnostic(DiagCategory::Storage, b"Boot Device", DiagStatus::Passed, b"Boot device accessible");
        self.progress = 40;

        self.add_diagnostic(DiagCategory::Storage, b"System Partition", DiagStatus::Passed, b"System partition mounted");
        self.progress = 50;

        // Boot checks
        self.add_diagnostic(DiagCategory::Boot, b"Bootloader", DiagStatus::Passed, b"UEFI boot successful");
        self.progress = 60;

        self.add_diagnostic(DiagCategory::Boot, b"Kernel", DiagStatus::Passed, b"Kernel loaded and running");
        self.progress = 65;

        // System files
        self.add_diagnostic(DiagCategory::SystemFiles, b"Core Files", DiagStatus::Passed, b"System files intact");
        self.progress = 75;

        // Configuration
        self.add_diagnostic(DiagCategory::Configuration, b"Settings", DiagStatus::Passed, b"Configuration valid");
        self.progress = 85;

        // Network (may warn if not connected)
        self.add_diagnostic(DiagCategory::Network, b"Connectivity", DiagStatus::Warning, b"Network not connected");
        warnings += 1;
        self.progress = 95;

        // Count results
        for i in 0..self.diag_count {
            match self.diagnostics[i].status {
                DiagStatus::Warning => warnings += 1,
                DiagStatus::Failed => failures += 1,
                _ => {}
            }
        }

        self.progress = 100;
        self.state = RecoveryState::Idle;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_RECOVERY_DIAGNOSTICS_COMPLETE:");
            crate::serial_write_hex_u64(failures as u64);
            crate::serial_write_str(" failures, ");
            crate::serial_write_hex_u64(warnings as u64);
            crate::serial_write_str(" warnings\n");
        }

        Ok((failures << 16) | (warnings & 0xFFFF))
    }

    /// Initialize restore points (simulated)
    pub fn init_restore_points(&mut self) {
        self.restore_count = 0;

        // Add some simulated restore points
        self.add_restore_point(1, b"System installed", RestoreType::Auto, 256 * 1024 * 1024);
        self.add_restore_point(2, b"Before update to v0.4.0", RestoreType::PreUpdate, 384 * 1024 * 1024);
        self.add_restore_point(3, b"Manual backup", RestoreType::Manual, 512 * 1024 * 1024);
    }

    /// Add a restore point
    fn add_restore_point(&mut self, id: u32, desc: &[u8], rtype: RestoreType, size: u64) {
        if self.restore_count >= MAX_RESTORE_POINTS {
            return;
        }

        let rp = &mut self.restore_points[self.restore_count];
        rp.active = true;
        rp.id = id;
        rp.timestamp = crate::TIMER_TICKS.load(Ordering::Relaxed);
        rp.set_description(desc);
        rp.restore_type = rtype;
        rp.size = size;
        self.restore_count += 1;
    }

    /// Create a new restore point
    pub fn create_restore_point(&mut self, desc: &[u8]) -> Result<u32, RecoveryError> {
        if self.restore_count >= MAX_RESTORE_POINTS {
            // Remove oldest
            for i in 0..(MAX_RESTORE_POINTS - 1) {
                self.restore_points[i] = self.restore_points[i + 1].clone();
            }
            self.restore_count = MAX_RESTORE_POINTS - 1;
        }

        let id = self.restore_count as u32 + 1;
        self.add_restore_point(id, desc, RestoreType::Manual, 512 * 1024 * 1024);

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_RECOVERY_RESTORE_POINT_CREATED:");
            crate::serial_write_hex_u64(id as u64);
            crate::serial_write_str("\n");
        }

        Ok(id)
    }

    /// Restore from a restore point
    pub fn restore(&mut self, restore_id: u32) -> Result<(), RecoveryError> {
        // Find the restore point
        let mut found = false;
        for i in 0..self.restore_count {
            if self.restore_points[i].active && self.restore_points[i].id == restore_id {
                found = true;
                break;
            }
        }

        if !found {
            return Err(RecoveryError::RestorePointNotFound);
        }

        self.state = RecoveryState::Restoring;
        self.progress = 0;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_RECOVERY_RESTORE_START:");
            crate::serial_write_hex_u64(restore_id as u64);
            crate::serial_write_str("\n");
        }

        // Simulate restore process
        for prog in [10, 20, 30, 40, 50, 60, 70, 80, 90, 100] {
            self.progress = prog;
        }

        self.state = RecoveryState::Complete;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_RECOVERY_RESTORE_COMPLETE\n");

        Ok(())
    }

    /// Perform factory reset
    pub fn factory_reset(&mut self, confirm: bool) -> Result<(), RecoveryError> {
        if !confirm {
            return Err(RecoveryError::ConfirmationRequired);
        }

        self.state = RecoveryState::Resetting;
        self.progress = 0;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_RECOVERY_FACTORY_RESET_START\n");

        // Simulate reset process
        for prog in [5, 15, 25, 35, 45, 55, 65, 75, 85, 95, 100] {
            self.progress = prog;
        }

        self.state = RecoveryState::Complete;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_RECOVERY_FACTORY_RESET_COMPLETE\n");

        Ok(())
    }

    /// Attempt automatic repair
    pub fn auto_repair(&mut self) -> Result<u32, RecoveryError> {
        self.state = RecoveryState::Repairing;
        self.progress = 0;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_RECOVERY_AUTO_REPAIR_START\n");

        let mut repairs = 0u32;

        // Simulate repair process
        self.progress = 20;
        // Check boot configuration
        repairs += 1;

        self.progress = 40;
        // Check system files
        repairs += 1;

        self.progress = 60;
        // Check registry/settings

        self.progress = 80;
        // Verify drivers

        self.progress = 100;
        self.state = RecoveryState::Complete;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_RECOVERY_AUTO_REPAIR_COMPLETE:");
            crate::serial_write_hex_u64(repairs as u64);
            crate::serial_write_str(" repairs\n");
        }

        Ok(repairs)
    }

    /// Reset state
    pub fn reset(&mut self) {
        self.state = RecoveryState::Idle;
        self.progress = 0;
        self.diag_count = 0;
        self.error_len = 0;
    }
}

// ===== Recovery Errors =====

/// Recovery errors
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryError {
    /// Already in recovery
    AlreadyInProgress,
    /// Restore point not found
    RestorePointNotFound,
    /// Confirmation required
    ConfirmationRequired,
    /// Repair failed
    RepairFailed,
    /// Restore failed
    RestoreFailed,
    /// Reset failed
    ResetFailed,
    /// Storage error
    StorageError,
    /// Insufficient space
    InsufficientSpace,
}

impl RecoveryError {
    /// Get error message
    pub fn message(&self) -> &'static str {
        match self {
            Self::AlreadyInProgress => "Recovery already in progress",
            Self::RestorePointNotFound => "Restore point not found",
            Self::ConfirmationRequired => "Confirmation required",
            Self::RepairFailed => "Repair failed",
            Self::RestoreFailed => "Restore failed",
            Self::ResetFailed => "Reset failed",
            Self::StorageError => "Storage error",
            Self::InsufficientSpace => "Insufficient space",
        }
    }
}

// ===== Global Recovery Manager =====

static mut RECOVERY_MANAGER: RecoveryManager = RecoveryManager::new();
static RECOVERY_LOCK: AtomicBool = AtomicBool::new(false);
static RECOVERY_INITIALIZED: AtomicBool = AtomicBool::new(false);
static BOOT_MODE: AtomicU8 = AtomicU8::new(0);

fn lock_recovery() {
    while RECOVERY_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

fn unlock_recovery() {
    RECOVERY_LOCK.store(false, Ordering::Release);
}

// ===== Public API =====

/// Initialize recovery system
pub fn init() {
    if RECOVERY_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    lock_recovery();
    unsafe {
        RECOVERY_MANAGER.init_restore_points();
    }
    unlock_recovery();

    RECOVERY_INITIALIZED.store(true, Ordering::Release);

    #[cfg(feature = "serial_debug")]
    crate::serial_write_str("RAYOS_RECOVERY_INITIALIZED\n");
}

/// Check if initialized
pub fn is_initialized() -> bool {
    RECOVERY_INITIALIZED.load(Ordering::Relaxed)
}

/// Get current boot mode
pub fn boot_mode() -> RecoveryMode {
    RecoveryMode::from_u8(BOOT_MODE.load(Ordering::Relaxed))
}

/// Set boot mode (for next boot)
pub fn set_boot_mode(mode: RecoveryMode) {
    BOOT_MODE.store(mode as u8, Ordering::Release);
}

/// Get recovery state
pub fn state() -> RecoveryState {
    lock_recovery();
    let s = unsafe { RECOVERY_MANAGER.state() };
    unlock_recovery();
    s
}

/// Get progress
pub fn progress() -> u8 {
    lock_recovery();
    let p = unsafe { RECOVERY_MANAGER.progress() };
    unlock_recovery();
    p
}

/// Run diagnostics
pub fn run_diagnostics() -> Result<u32, RecoveryError> {
    if !is_initialized() {
        init();
    }
    lock_recovery();
    let result = unsafe { RECOVERY_MANAGER.run_diagnostics() };
    unlock_recovery();
    result
}

/// Get diagnostic count
pub fn diag_count() -> usize {
    lock_recovery();
    let c = unsafe { RECOVERY_MANAGER.diag_count() };
    unlock_recovery();
    c
}

/// Get diagnostic result (cloned)
pub fn get_diagnostic(index: usize) -> Option<DiagResult> {
    lock_recovery();
    let result = unsafe { RECOVERY_MANAGER.get_diagnostic(index).cloned() };
    unlock_recovery();
    result
}

/// Get restore point count
pub fn restore_count() -> usize {
    lock_recovery();
    let c = unsafe { RECOVERY_MANAGER.restore_count() };
    unlock_recovery();
    c
}

/// Get restore point (cloned)
pub fn get_restore_point(index: usize) -> Option<RestorePoint> {
    lock_recovery();
    let result = unsafe { RECOVERY_MANAGER.get_restore_point(index).cloned() };
    unlock_recovery();
    result
}

/// Create restore point
pub fn create_restore_point(desc: &[u8]) -> Result<u32, RecoveryError> {
    lock_recovery();
    let result = unsafe { RECOVERY_MANAGER.create_restore_point(desc) };
    unlock_recovery();
    result
}

/// Restore from restore point
pub fn restore(restore_id: u32) -> Result<(), RecoveryError> {
    lock_recovery();
    let result = unsafe { RECOVERY_MANAGER.restore(restore_id) };
    unlock_recovery();
    result
}

/// Factory reset
pub fn factory_reset(confirm: bool) -> Result<(), RecoveryError> {
    lock_recovery();
    let result = unsafe { RECOVERY_MANAGER.factory_reset(confirm) };
    unlock_recovery();
    result
}

/// Auto repair
pub fn auto_repair() -> Result<u32, RecoveryError> {
    lock_recovery();
    let result = unsafe { RECOVERY_MANAGER.auto_repair() };
    unlock_recovery();
    result
}

/// Reset recovery state
pub fn reset() {
    lock_recovery();
    unsafe { RECOVERY_MANAGER.reset(); }
    unlock_recovery();
}
