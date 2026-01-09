//! RayOS System Log
//!
//! In-kernel event journal for diagnostics and troubleshooting.
//! Provides a ring buffer of log entries that can be viewed in the UI.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

// ===== Constants =====

/// Maximum number of log entries in the ring buffer
pub const LOG_BUFFER_ENTRIES: usize = 1024;

/// Size of each log entry in bytes
pub const LOG_ENTRY_SIZE: usize = 64;

/// Maximum message length per entry
pub const LOG_MESSAGE_MAX: usize = 52;

// ===== Severity Levels =====

/// Verbose debugging information
pub const SEVERITY_TRACE: u8 = 0;
/// Development/debugging info
pub const SEVERITY_DEBUG: u8 = 1;
/// Normal operational events
pub const SEVERITY_INFO: u8 = 2;
/// Potential issues
pub const SEVERITY_WARN: u8 = 3;
/// Error conditions
pub const SEVERITY_ERROR: u8 = 4;
/// Critical failures
pub const SEVERITY_FATAL: u8 = 5;

// ===== Subsystem Tags =====

/// Core kernel
pub const SUBSYSTEM_KERNEL: u8 = 0x01;
/// Heap/page allocator
pub const SUBSYSTEM_MEMORY: u8 = 0x02;
/// Interrupt handlers
pub const SUBSYSTEM_IRQ: u8 = 0x03;
/// Timer subsystem
pub const SUBSYSTEM_TIMER: u8 = 0x04;
/// PS/2 keyboard
pub const SUBSYSTEM_KEYBOARD: u8 = 0x05;
/// PS/2 mouse
pub const SUBSYSTEM_MOUSE: u8 = 0x06;
/// Persistent storage / volumes
pub const SUBSYSTEM_STORAGE: u8 = 0x07;
/// Window manager, compositor
pub const SUBSYSTEM_UI: u8 = 0x10;
/// Input handling
pub const SUBSYSTEM_INPUT: u8 = 0x11;
/// Rendering
pub const SUBSYSTEM_RENDER: u8 = 0x12;
/// Hypervisor
pub const SUBSYSTEM_VMM: u8 = 0x20;
/// Linux/Windows guest
pub const SUBSYSTEM_GUEST: u8 = 0x21;
/// virtio devices
pub const SUBSYSTEM_VIRTIO: u8 = 0x22;
/// Ray queue system
pub const SUBSYSTEM_RAY: u8 = 0x30;
/// Conductor orchestrator
pub const SUBSYSTEM_CONDUCTOR: u8 = 0x31;
/// AI/LLM subsystem
pub const SUBSYSTEM_AI: u8 = 0x40;
/// Security/policy
pub const SUBSYSTEM_SECURITY: u8 = 0x50;
/// RayApp framework
pub const SUBSYSTEM_APP: u8 = 0x60;

// ===== Log Entry Structure =====

/// A single log entry in the ring buffer.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LogEntry {
    /// Timestamp in timer ticks since boot
    pub timestamp: u64,
    /// Severity level (SEVERITY_*)
    pub severity: u8,
    /// Subsystem tag (SUBSYSTEM_*)
    pub subsystem: u8,
    /// Message length (0-52)
    pub message_len: u8,
    /// Reserved for future use
    pub _reserved: u8,
    /// Message bytes (null-padded)
    pub message: [u8; LOG_MESSAGE_MAX],
}

impl LogEntry {
    /// Create an empty log entry.
    pub const fn empty() -> Self {
        Self {
            timestamp: 0,
            severity: 0,
            subsystem: 0,
            message_len: 0,
            _reserved: 0,
            message: [0u8; LOG_MESSAGE_MAX],
        }
    }

    /// Get the message as a byte slice.
    pub fn message_bytes(&self) -> &[u8] {
        let len = (self.message_len as usize).min(LOG_MESSAGE_MAX);
        &self.message[..len]
    }

    /// Check if this entry is valid (has been written).
    pub fn is_valid(&self) -> bool {
        self.timestamp != 0 || self.message_len != 0
    }
}

// ===== Ring Buffer =====

/// The log ring buffer (statically allocated).
static mut LOG_BUFFER: [LogEntry; LOG_BUFFER_ENTRIES] = [LogEntry::empty(); LOG_BUFFER_ENTRIES];

/// Write index (next slot to write)
static WRITE_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Total entries ever written (for wrap detection)
static TOTAL_ENTRIES: AtomicU64 = AtomicU64::new(0);

/// Whether the log has been initialized
static LOG_INITIALIZED: AtomicUsize = AtomicUsize::new(0);

// ===== Core API =====

/// Initialize the system log.
/// Should be called early in kernel initialization.
pub fn init() {
    // Clear the buffer
    unsafe {
        for entry in LOG_BUFFER.iter_mut() {
            *entry = LogEntry::empty();
        }
    }
    WRITE_INDEX.store(0, Ordering::Release);
    TOTAL_ENTRIES.store(0, Ordering::Release);
    LOG_INITIALIZED.store(1, Ordering::Release);
}

/// Check if the log is initialized.
pub fn is_initialized() -> bool {
    LOG_INITIALIZED.load(Ordering::Acquire) != 0
}

/// Log an event to the system log.
///
/// # Arguments
/// * `severity` - Log severity (SEVERITY_*)
/// * `subsystem` - Source subsystem (SUBSYSTEM_*)
/// * `message` - Message bytes (truncated to LOG_MESSAGE_MAX)
pub fn log(severity: u8, subsystem: u8, message: &[u8]) {
    if !is_initialized() {
        return;
    }

    // Get timestamp from kernel timer
    let timestamp = crate::TIMER_TICKS.load(Ordering::Relaxed);

    // Prepare the entry
    let mut entry = LogEntry::empty();
    entry.timestamp = timestamp;
    entry.severity = severity;
    entry.subsystem = subsystem;

    // Copy message (truncate if needed)
    let msg_len = message.len().min(LOG_MESSAGE_MAX);
    entry.message_len = msg_len as u8;
    entry.message[..msg_len].copy_from_slice(&message[..msg_len]);

    // Atomically get the next write slot
    let slot = WRITE_INDEX.fetch_add(1, Ordering::AcqRel) % LOG_BUFFER_ENTRIES;

    // Write the entry
    unsafe {
        LOG_BUFFER[slot] = entry;
    }

    // Increment total count
    TOTAL_ENTRIES.fetch_add(1, Ordering::Relaxed);
}

/// Log with a formatted message (simple concatenation, no allocation).
/// Concatenates prefix + suffix into one message.
pub fn log_concat(severity: u8, subsystem: u8, prefix: &[u8], suffix: &[u8]) {
    let mut buf = [0u8; LOG_MESSAGE_MAX];
    let mut len = 0usize;

    // Copy prefix
    for &b in prefix {
        if len >= LOG_MESSAGE_MAX {
            break;
        }
        buf[len] = b;
        len += 1;
    }

    // Copy suffix
    for &b in suffix {
        if len >= LOG_MESSAGE_MAX {
            break;
        }
        buf[len] = b;
        len += 1;
    }

    log(severity, subsystem, &buf[..len]);
}

/// Log with a numeric value appended.
pub fn log_with_u64(severity: u8, subsystem: u8, prefix: &[u8], value: u64) {
    let mut buf = [0u8; LOG_MESSAGE_MAX];
    let mut len = 0usize;

    // Copy prefix
    for &b in prefix {
        if len >= LOG_MESSAGE_MAX {
            break;
        }
        buf[len] = b;
        len += 1;
    }

    // Convert number to string (simple decimal)
    if value == 0 {
        if len < LOG_MESSAGE_MAX {
            buf[len] = b'0';
            len += 1;
        }
    } else {
        // Build digits in reverse
        let mut digits = [0u8; 20];
        let mut num_digits = 0usize;
        let mut v = value;
        while v > 0 && num_digits < 20 {
            digits[num_digits] = b'0' + (v % 10) as u8;
            v /= 10;
            num_digits += 1;
        }
        // Append in correct order
        for i in (0..num_digits).rev() {
            if len >= LOG_MESSAGE_MAX {
                break;
            }
            buf[len] = digits[i];
            len += 1;
        }
    }

    log(severity, subsystem, &buf[..len]);
}

// ===== Query API =====

/// Get the total number of entries ever logged.
pub fn total_count() -> u64 {
    TOTAL_ENTRIES.load(Ordering::Relaxed)
}

/// Get the number of entries currently in the buffer.
pub fn entry_count() -> usize {
    let total = TOTAL_ENTRIES.load(Ordering::Relaxed) as usize;
    total.min(LOG_BUFFER_ENTRIES)
}

/// Get a log entry by index (0 = oldest visible entry).
/// Returns None if index is out of range.
pub fn get_entry(index: usize) -> Option<LogEntry> {
    let total = TOTAL_ENTRIES.load(Ordering::Relaxed) as usize;
    let count = total.min(LOG_BUFFER_ENTRIES);

    if index >= count {
        return None;
    }

    // Calculate the actual buffer slot
    let write_idx = WRITE_INDEX.load(Ordering::Acquire);
    let oldest_slot = if total > LOG_BUFFER_ENTRIES {
        write_idx % LOG_BUFFER_ENTRIES
    } else {
        0
    };
    let slot = (oldest_slot + index) % LOG_BUFFER_ENTRIES;

    unsafe { Some(LOG_BUFFER[slot]) }
}

/// Clear all log entries.
pub fn clear() {
    if !is_initialized() {
        return;
    }

    unsafe {
        for entry in LOG_BUFFER.iter_mut() {
            *entry = LogEntry::empty();
        }
    }
    WRITE_INDEX.store(0, Ordering::Release);
    TOTAL_ENTRIES.store(0, Ordering::Release);
}

// ===== Convenience Functions =====

/// Log a TRACE level message.
#[inline]
pub fn trace(subsystem: u8, message: &[u8]) {
    log(SEVERITY_TRACE, subsystem, message);
}

/// Log a DEBUG level message.
#[inline]
pub fn debug(subsystem: u8, message: &[u8]) {
    log(SEVERITY_DEBUG, subsystem, message);
}

/// Log an INFO level message.
#[inline]
pub fn info(subsystem: u8, message: &[u8]) {
    log(SEVERITY_INFO, subsystem, message);
}

/// Log a WARN level message.
#[inline]
pub fn warn(subsystem: u8, message: &[u8]) {
    log(SEVERITY_WARN, subsystem, message);
}

/// Log an ERROR level message.
#[inline]
pub fn error(subsystem: u8, message: &[u8]) {
    log(SEVERITY_ERROR, subsystem, message);
}

/// Log a FATAL level message.
#[inline]
pub fn fatal(subsystem: u8, message: &[u8]) {
    log(SEVERITY_FATAL, subsystem, message);
}

// ===== Display Helpers =====

/// Get the name of a severity level.
pub fn severity_name(severity: u8) -> &'static [u8] {
    match severity {
        SEVERITY_TRACE => b"TRACE",
        SEVERITY_DEBUG => b"DEBUG",
        SEVERITY_INFO => b"INFO ",
        SEVERITY_WARN => b"WARN ",
        SEVERITY_ERROR => b"ERROR",
        SEVERITY_FATAL => b"FATAL",
        _ => b"?????",
    }
}

/// Get the color for a severity level (ARGB).
pub fn severity_color(severity: u8) -> u32 {
    match severity {
        SEVERITY_TRACE => 0xFF888888, // Gray
        SEVERITY_DEBUG => 0xFF88CCFF, // Cyan
        SEVERITY_INFO => 0xFFDDDDDD,  // White
        SEVERITY_WARN => 0xFFFFCC44,  // Yellow
        SEVERITY_ERROR => 0xFFFF6666, // Red
        SEVERITY_FATAL => 0xFFFF44FF, // Magenta
        _ => 0xFFDDDDDD,
    }
}

/// Get the name of a subsystem.
pub fn subsystem_name(subsystem: u8) -> &'static [u8] {
    match subsystem {
        SUBSYSTEM_KERNEL => b"KERNEL",
        SUBSYSTEM_MEMORY => b"MEMORY",
        SUBSYSTEM_IRQ => b"IRQ   ",
        SUBSYSTEM_TIMER => b"TIMER ",
        SUBSYSTEM_KEYBOARD => b"KEYBD ",
        SUBSYSTEM_MOUSE => b"MOUSE ",
        SUBSYSTEM_STORAGE => b"STORE ",
        SUBSYSTEM_UI => b"UI    ",
        SUBSYSTEM_INPUT => b"INPUT ",
        SUBSYSTEM_RENDER => b"RENDER",
        SUBSYSTEM_VMM => b"VMM   ",
        SUBSYSTEM_GUEST => b"GUEST ",
        SUBSYSTEM_VIRTIO => b"VIRTIO",
        SUBSYSTEM_RAY => b"RAY   ",
        SUBSYSTEM_CONDUCTOR => b"COND  ",
        SUBSYSTEM_AI => b"AI    ",
        SUBSYSTEM_SECURITY => b"SECUR ",
        SUBSYSTEM_APP => b"APP   ",
        _ => b"??????",
    }
}

/// Format a timestamp as HH:MM:SS (assumes 100Hz tick rate).
pub fn format_timestamp(timestamp: u64, buf: &mut [u8; 12]) -> usize {
    // Assuming 100 ticks per second
    let total_secs = timestamp / 100;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    let centis = (timestamp % 100) as u8;

    // Format as HH:MM:SS.cc
    buf[0] = b'0' + ((hours / 10) % 10) as u8;
    buf[1] = b'0' + (hours % 10) as u8;
    buf[2] = b':';
    buf[3] = b'0' + ((mins / 10) % 10) as u8;
    buf[4] = b'0' + (mins % 10) as u8;
    buf[5] = b':';
    buf[6] = b'0' + ((secs / 10) % 10) as u8;
    buf[7] = b'0' + (secs % 10) as u8;
    buf[8] = b'.';
    buf[9] = b'0' + (centis / 10);
    buf[10] = b'0' + (centis % 10);
    buf[11] = 0;

    11
}
