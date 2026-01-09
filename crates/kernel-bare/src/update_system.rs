//! RayOS Update System
//!
//! Provides system update capabilities including:
//! - Version tracking and comparison
//! - Update checking and download
//! - Atomic updates with rollback support
//! - Update verification

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Maximum update source URL length
pub const MAX_URL_LEN: usize = 256;

/// Maximum changelog length
pub const MAX_CHANGELOG_LEN: usize = 1024;

/// Maximum number of rollback slots
pub const MAX_ROLLBACK_SLOTS: usize = 2;

// ===== Version Info =====

/// System version information
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// Major version number
    pub major: u16,
    /// Minor version number
    pub minor: u16,
    /// Patch version number
    pub patch: u16,
    /// Build number
    pub build: u32,
}

impl Version {
    /// Create a new version
    pub const fn new(major: u16, minor: u16, patch: u16, build: u32) -> Self {
        Self { major, minor, patch, build }
    }

    /// Current RayOS version
    pub const fn current() -> Self {
        Self::new(0, 5, 0, 2026010901) // v0.5.0 build 2026010901
    }

    /// Compare versions (returns -1, 0, or 1)
    pub fn compare(&self, other: &Version) -> i8 {
        if self.major > other.major { return 1; }
        if self.major < other.major { return -1; }
        if self.minor > other.minor { return 1; }
        if self.minor < other.minor { return -1; }
        if self.patch > other.patch { return 1; }
        if self.patch < other.patch { return -1; }
        if self.build > other.build { return 1; }
        if self.build < other.build { return -1; }
        0
    }

    /// Check if this version is newer than other
    pub fn is_newer_than(&self, other: &Version) -> bool {
        self.compare(other) > 0
    }

    /// Format version to buffer (returns bytes written)
    pub fn format(&self, buf: &mut [u8]) -> usize {
        let mut pos = 0;
        pos += format_u16(self.major, &mut buf[pos..]);
        if pos < buf.len() { buf[pos] = b'.'; pos += 1; }
        pos += format_u16(self.minor, &mut buf[pos..]);
        if pos < buf.len() { buf[pos] = b'.'; pos += 1; }
        pos += format_u16(self.patch, &mut buf[pos..]);
        pos
    }

    /// Format full version with build number
    pub fn format_full(&self, buf: &mut [u8]) -> usize {
        let mut pos = self.format(buf);
        if pos + 10 < buf.len() {
            buf[pos] = b'-';
            pos += 1;
            pos += format_u32(self.build, &mut buf[pos..]);
        }
        pos
    }
}

// ===== Update Channel =====

/// Update channel
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum UpdateChannel {
    /// Stable release channel
    Stable = 0,
    /// Beta testing channel
    Beta = 1,
    /// Development/nightly channel
    Dev = 2,
}

impl UpdateChannel {
    /// Get channel name
    pub fn name(&self) -> &'static [u8] {
        match self {
            Self::Stable => b"stable",
            Self::Beta => b"beta",
            Self::Dev => b"dev",
        }
    }

    /// From u8
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Beta,
            2 => Self::Dev,
            _ => Self::Stable,
        }
    }
}

// ===== Update State =====

/// Update system state
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum UpdateState {
    /// Idle, no update in progress
    Idle = 0,
    /// Checking for updates
    Checking = 1,
    /// Update available
    UpdateAvailable = 2,
    /// Downloading update
    Downloading = 3,
    /// Verifying update
    Verifying = 4,
    /// Applying update
    Applying = 5,
    /// Update complete, reboot required
    PendingReboot = 6,
    /// Update failed
    Failed = 7,
    /// Rolling back
    RollingBack = 8,
}

impl UpdateState {
    /// Get state name
    pub fn name(&self) -> &'static [u8] {
        match self {
            Self::Idle => b"Idle",
            Self::Checking => b"Checking for updates",
            Self::UpdateAvailable => b"Update available",
            Self::Downloading => b"Downloading",
            Self::Verifying => b"Verifying",
            Self::Applying => b"Applying update",
            Self::PendingReboot => b"Reboot required",
            Self::Failed => b"Failed",
            Self::RollingBack => b"Rolling back",
        }
    }
}

// ===== Update Info =====

/// Information about an available update
#[derive(Clone)]
pub struct UpdateInfo {
    /// New version
    pub version: Version,
    /// Download size in bytes
    pub download_size: u64,
    /// Is this a critical/security update
    pub is_critical: bool,
    /// Changelog
    pub changelog: [u8; MAX_CHANGELOG_LEN],
    pub changelog_len: usize,
    /// Release date (YYYYMMDD)
    pub release_date: u32,
}

impl UpdateInfo {
    /// Create empty update info
    pub const fn empty() -> Self {
        Self {
            version: Version::new(0, 0, 0, 0),
            download_size: 0,
            is_critical: false,
            changelog: [0; MAX_CHANGELOG_LEN],
            changelog_len: 0,
            release_date: 0,
        }
    }

    /// Get changelog
    pub fn changelog(&self) -> &[u8] {
        &self.changelog[..self.changelog_len]
    }

    /// Set changelog
    pub fn set_changelog(&mut self, log: &[u8]) {
        let len = log.len().min(MAX_CHANGELOG_LEN);
        self.changelog[..len].copy_from_slice(&log[..len]);
        self.changelog_len = len;
    }
}

// ===== Rollback Slot =====

/// A rollback slot containing a previous version
#[derive(Clone)]
pub struct RollbackSlot {
    /// Whether this slot is occupied
    pub active: bool,
    /// Version stored in this slot
    pub version: Version,
    /// Slot index
    pub index: u8,
    /// Timestamp when this was saved
    pub timestamp: u64,
    /// Size in bytes
    pub size: u64,
}

impl RollbackSlot {
    /// Create empty slot
    pub const fn empty() -> Self {
        Self {
            active: false,
            version: Version::new(0, 0, 0, 0),
            index: 0,
            timestamp: 0,
            size: 0,
        }
    }
}

// ===== Update Manager =====

/// The update manager
pub struct UpdateManager {
    /// Current system version
    current_version: Version,
    /// Update channel
    channel: UpdateChannel,
    /// Current state
    state: UpdateState,
    /// Progress (0-100)
    progress: u8,
    /// Available update info
    available_update: Option<UpdateInfo>,
    /// Rollback slots
    rollback_slots: [RollbackSlot; MAX_ROLLBACK_SLOTS],
    /// Auto-update enabled
    auto_update: bool,
    /// Last check timestamp
    last_check: u64,
    /// Error message
    error: [u8; 128],
    error_len: usize,
}

impl UpdateManager {
    /// Create new update manager
    pub const fn new() -> Self {
        Self {
            current_version: Version::current(),
            channel: UpdateChannel::Stable,
            state: UpdateState::Idle,
            progress: 0,
            available_update: None,
            rollback_slots: [const { RollbackSlot::empty() }; MAX_ROLLBACK_SLOTS],
            auto_update: true,
            last_check: 0,
            error: [0; 128],
            error_len: 0,
        }
    }

    /// Get current version
    pub fn version(&self) -> Version {
        self.current_version
    }

    /// Get channel
    pub fn channel(&self) -> UpdateChannel {
        self.channel
    }

    /// Set channel
    pub fn set_channel(&mut self, channel: UpdateChannel) {
        self.channel = channel;
    }

    /// Get state
    pub fn state(&self) -> UpdateState {
        self.state
    }

    /// Get progress
    pub fn progress(&self) -> u8 {
        self.progress
    }

    /// Get auto-update setting
    pub fn auto_update(&self) -> bool {
        self.auto_update
    }

    /// Set auto-update
    pub fn set_auto_update(&mut self, enabled: bool) {
        self.auto_update = enabled;
    }

    /// Get available update
    pub fn available_update(&self) -> Option<&UpdateInfo> {
        self.available_update.as_ref()
    }

    /// Get rollback slot
    pub fn get_rollback_slot(&self, index: usize) -> Option<&RollbackSlot> {
        if index < MAX_ROLLBACK_SLOTS && self.rollback_slots[index].active {
            Some(&self.rollback_slots[index])
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
        self.state = UpdateState::Failed;
    }

    /// Check for updates (simulated)
    pub fn check_updates(&mut self) -> Result<bool, UpdateError> {
        if self.state != UpdateState::Idle && self.state != UpdateState::Failed {
            return Err(UpdateError::UpdateInProgress);
        }

        self.state = UpdateState::Checking;
        self.progress = 0;
        self.error_len = 0;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UPDATE_CHECK_START\n");

        // Simulate checking for updates
        // In real implementation, would contact update server
        self.last_check = crate::TIMER_TICKS.load(Ordering::Relaxed);

        // Simulate finding an update
        let mut update = UpdateInfo::empty();
        update.version = Version::new(0, 6, 0, 2026020101);
        update.download_size = 128 * 1024 * 1024; // 128 MB
        update.is_critical = false;
        update.set_changelog(b"RayOS v0.6.0\n\n- Improved window management\n- Enhanced graphics performance\n- Bug fixes and stability improvements");
        update.release_date = 20260201;

        let has_update = update.version.is_newer_than(&self.current_version);

        if has_update {
            self.available_update = Some(update);
            self.state = UpdateState::UpdateAvailable;
        } else {
            self.available_update = None;
            self.state = UpdateState::Idle;
        }

        self.progress = 100;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UPDATE_CHECK_COMPLETE:");
            if has_update {
                crate::serial_write_str("UPDATE_AVAILABLE\n");
            } else {
                crate::serial_write_str("UP_TO_DATE\n");
            }
        }

        Ok(has_update)
    }

    /// Download update (simulated)
    pub fn download_update(&mut self) -> Result<(), UpdateError> {
        if self.state != UpdateState::UpdateAvailable {
            return Err(UpdateError::NoUpdateAvailable);
        }

        if self.available_update.is_none() {
            return Err(UpdateError::NoUpdateAvailable);
        }

        self.state = UpdateState::Downloading;
        self.progress = 0;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UPDATE_DOWNLOAD_START\n");

        // Simulate download progress
        for prog in [10, 25, 40, 55, 70, 85, 100] {
            self.progress = prog;
            // In real implementation, would download chunks here
        }

        self.state = UpdateState::Verifying;
        self.progress = 0;

        // Simulate verification
        for prog in [25, 50, 75, 100] {
            self.progress = prog;
        }

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UPDATE_DOWNLOAD_COMPLETE\n");

        self.state = UpdateState::UpdateAvailable;
        self.progress = 100;

        Ok(())
    }

    /// Apply update (simulated)
    pub fn apply_update(&mut self) -> Result<(), UpdateError> {
        if self.available_update.is_none() {
            return Err(UpdateError::NoUpdateAvailable);
        }

        self.state = UpdateState::Applying;
        self.progress = 0;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UPDATE_APPLY_START\n");

        // Save current version to rollback slot
        self.save_rollback()?;

        // Simulate applying update
        for prog in [10, 20, 30, 40, 50, 60, 70, 80, 90, 100] {
            self.progress = prog;
        }

        self.state = UpdateState::PendingReboot;

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UPDATE_APPLY_COMPLETE:REBOOT_REQUIRED\n");

        Ok(())
    }

    /// Save current state to rollback slot
    fn save_rollback(&mut self) -> Result<(), UpdateError> {
        // Shift existing slots
        for i in (1..MAX_ROLLBACK_SLOTS).rev() {
            self.rollback_slots[i] = self.rollback_slots[i - 1].clone();
        }

        // Save current version to slot 0
        self.rollback_slots[0] = RollbackSlot {
            active: true,
            version: self.current_version,
            index: 0,
            timestamp: crate::TIMER_TICKS.load(Ordering::Relaxed),
            size: 128 * 1024 * 1024, // Simulated size
        };

        // Update indices
        for i in 0..MAX_ROLLBACK_SLOTS {
            self.rollback_slots[i].index = i as u8;
        }

        Ok(())
    }

    /// Rollback to previous version
    pub fn rollback(&mut self, slot_index: usize) -> Result<(), UpdateError> {
        if slot_index >= MAX_ROLLBACK_SLOTS {
            return Err(UpdateError::InvalidSlot);
        }

        let slot = &self.rollback_slots[slot_index];
        if !slot.active {
            return Err(UpdateError::SlotEmpty);
        }

        self.state = UpdateState::RollingBack;
        self.progress = 0;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UPDATE_ROLLBACK:");
            crate::serial_write_hex_u64(slot_index as u64);
            crate::serial_write_str("\n");
        }

        // Simulate rollback
        for prog in [20, 40, 60, 80, 100] {
            self.progress = prog;
        }

        self.state = UpdateState::PendingReboot;
        Ok(())
    }

    /// Reset to idle state
    pub fn reset(&mut self) {
        self.state = UpdateState::Idle;
        self.progress = 0;
        self.available_update = None;
        self.error_len = 0;
    }
}

// ===== Update Errors =====

/// Update errors
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// Update already in progress
    UpdateInProgress,
    /// No update available
    NoUpdateAvailable,
    /// Download failed
    DownloadFailed,
    /// Verification failed
    VerificationFailed,
    /// Apply failed
    ApplyFailed,
    /// Invalid rollback slot
    InvalidSlot,
    /// Rollback slot is empty
    SlotEmpty,
    /// Network error
    NetworkError,
    /// Not enough space
    InsufficientSpace,
}

impl UpdateError {
    /// Get error message
    pub fn message(&self) -> &'static str {
        match self {
            Self::UpdateInProgress => "Update already in progress",
            Self::NoUpdateAvailable => "No update available",
            Self::DownloadFailed => "Download failed",
            Self::VerificationFailed => "Verification failed",
            Self::ApplyFailed => "Apply failed",
            Self::InvalidSlot => "Invalid rollback slot",
            Self::SlotEmpty => "Rollback slot is empty",
            Self::NetworkError => "Network error",
            Self::InsufficientSpace => "Insufficient space",
        }
    }
}

// ===== Global Update Manager =====

static mut UPDATE_MANAGER: UpdateManager = UpdateManager::new();
static UPDATE_LOCK: AtomicBool = AtomicBool::new(false);
static UPDATE_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn lock_update() {
    while UPDATE_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

fn unlock_update() {
    UPDATE_LOCK.store(false, Ordering::Release);
}

// ===== Public API =====

/// Initialize update system
pub fn init() {
    if UPDATE_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    UPDATE_INITIALIZED.store(true, Ordering::Release);

    #[cfg(feature = "serial_debug")]
    crate::serial_write_str("RAYOS_UPDATE_INITIALIZED\n");
}

/// Check if initialized
pub fn is_initialized() -> bool {
    UPDATE_INITIALIZED.load(Ordering::Relaxed)
}

/// Get current version
pub fn current_version() -> Version {
    lock_update();
    let v = unsafe { UPDATE_MANAGER.version() };
    unlock_update();
    v
}

/// Get update channel
pub fn channel() -> UpdateChannel {
    lock_update();
    let c = unsafe { UPDATE_MANAGER.channel() };
    unlock_update();
    c
}

/// Set update channel
pub fn set_channel(channel: UpdateChannel) {
    lock_update();
    unsafe { UPDATE_MANAGER.set_channel(channel); }
    unlock_update();
}

/// Get update state
pub fn state() -> UpdateState {
    lock_update();
    let s = unsafe { UPDATE_MANAGER.state() };
    unlock_update();
    s
}

/// Get progress
pub fn progress() -> u8 {
    lock_update();
    let p = unsafe { UPDATE_MANAGER.progress() };
    unlock_update();
    p
}

/// Check for updates
pub fn check_updates() -> Result<bool, UpdateError> {
    if !is_initialized() {
        init();
    }
    lock_update();
    let result = unsafe { UPDATE_MANAGER.check_updates() };
    unlock_update();
    result
}

/// Get available update info
pub fn available_update() -> Option<UpdateInfo> {
    lock_update();
    let result = unsafe { UPDATE_MANAGER.available_update().cloned() };
    unlock_update();
    result
}

/// Download update
pub fn download_update() -> Result<(), UpdateError> {
    lock_update();
    let result = unsafe { UPDATE_MANAGER.download_update() };
    unlock_update();
    result
}

/// Apply update
pub fn apply_update() -> Result<(), UpdateError> {
    lock_update();
    let result = unsafe { UPDATE_MANAGER.apply_update() };
    unlock_update();
    result
}

/// Rollback to previous version
pub fn rollback(slot: usize) -> Result<(), UpdateError> {
    lock_update();
    let result = unsafe { UPDATE_MANAGER.rollback(slot) };
    unlock_update();
    result
}

/// Get rollback slot
pub fn get_rollback_slot(index: usize) -> Option<RollbackSlot> {
    lock_update();
    let result = unsafe { UPDATE_MANAGER.get_rollback_slot(index).cloned() };
    unlock_update();
    result
}

/// Get auto-update setting
pub fn auto_update() -> bool {
    lock_update();
    let a = unsafe { UPDATE_MANAGER.auto_update() };
    unlock_update();
    a
}

/// Set auto-update
pub fn set_auto_update(enabled: bool) {
    lock_update();
    unsafe { UPDATE_MANAGER.set_auto_update(enabled); }
    unlock_update();
}

/// Reset update state
pub fn reset() {
    lock_update();
    unsafe { UPDATE_MANAGER.reset(); }
    unlock_update();
}

// ===== Helper Functions =====

fn format_u16(mut n: u16, buf: &mut [u8]) -> usize {
    if n == 0 {
        if !buf.is_empty() { buf[0] = b'0'; }
        return 1;
    }
    let mut temp = [0u8; 5];
    let mut i = 5;
    while n > 0 && i > 0 {
        i -= 1;
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let len = 5 - i;
    let copy_len = len.min(buf.len());
    buf[..copy_len].copy_from_slice(&temp[i..i + copy_len]);
    copy_len
}

fn format_u32(mut n: u32, buf: &mut [u8]) -> usize {
    if n == 0 {
        if !buf.is_empty() { buf[0] = b'0'; }
        return 1;
    }
    let mut temp = [0u8; 10];
    let mut i = 10;
    while n > 0 && i > 0 {
        i -= 1;
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let len = 10 - i;
    let copy_len = len.min(buf.len());
    buf[..copy_len].copy_from_slice(&temp[i..i + copy_len]);
    copy_len
}
