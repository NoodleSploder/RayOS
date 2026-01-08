// ===== RayOS Kernel Logging & Observability Module (Phase 9B Task 3) =====
// System logging, performance monitoring, kernel message capture, debug output

use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering};

// ===== Logging System Constants =====

const LOG_BUFFER_SIZE: usize = 16384;  // 16 KB circular buffer
const MAX_LOG_ENTRIES: usize = 512;     // Max individual log entries
const LOG_LEVELS: usize = 6;             // TRACE, DEBUG, INFO, WARN, ERROR, FATAL

// ===== Log Level Enumeration =====

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Fatal => "FATAL",
        }
    }

    pub fn as_color_code(&self) -> &'static str {
        match self {
            LogLevel::Trace => "\x1b[36m",   // Cyan
            LogLevel::Debug => "\x1b[34m",   // Blue
            LogLevel::Info => "\x1b[32m",    // Green
            LogLevel::Warn => "\x1b[33m",    // Yellow
            LogLevel::Error => "\x1b[31m",   // Red
            LogLevel::Fatal => "\x1b[35m",   // Magenta
        }
    }
}

// ===== Simplified Kernel Logger =====

pub struct KernelLogger {
    // Statistics (atomic for concurrent access)
    total_logs: AtomicUsize,
    logs_by_level: [AtomicUsize; LOG_LEVELS],

    // Configuration
    min_log_level: AtomicUsize,
    enable_colors: AtomicUsize,
}

impl KernelLogger {
    pub const fn new() -> Self {
        KernelLogger {
            total_logs: AtomicUsize::new(0),
            logs_by_level: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
            min_log_level: AtomicUsize::new(LogLevel::Debug as usize),
            enable_colors: AtomicUsize::new(1),
        }
    }

    pub fn set_min_level(&self, level: LogLevel) {
        self.min_log_level.store(level as usize, Ordering::Relaxed);
    }

    pub fn set_colors(&self, enabled: bool) {
        self.enable_colors.store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
    }

    pub fn log(&self, level: LogLevel, _source: &str, _message: &str) {
        let min_level_val = self.min_log_level.load(Ordering::Relaxed);
        
        // Compare as usize values
        if (level as usize) < min_level_val {
            return;  // Filter out messages below minimum level
        }

        // Update statistics
        self.total_logs.fetch_add(1, Ordering::Relaxed);
        if (level as usize) < LOG_LEVELS {
            self.logs_by_level[level as usize].fetch_add(1, Ordering::Relaxed);
        }

        // Note: Actual logging to serial/buffer would happen here
        // For now, just update statistics
    }

    pub fn get_total_logs(&self) -> usize {
        self.total_logs.load(Ordering::Relaxed)
    }

    pub fn get_logs_by_level(&self, level: LogLevel) -> usize {
        if (level as usize) < LOG_LEVELS {
            self.logs_by_level[level as usize].load(Ordering::Relaxed)
        } else {
            0
        }
    }

    pub fn clear(&self) {
        self.total_logs.store(0, Ordering::Relaxed);
        for i in 0..LOG_LEVELS {
            self.logs_by_level[i].store(0, Ordering::Relaxed);
        }
    }
}

// ===== Performance Monitoring =====

pub struct PerformanceMonitor {
    boot_time_ms: u64,
    last_tick_ms: u64,
    sample_count: usize,
    max_samples: usize,
    samples: [u64; 256],  // Performance measurements
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        PerformanceMonitor {
            boot_time_ms: 0,
            last_tick_ms: 0,
            sample_count: 0,
            max_samples: 256,
            samples: [0u64; 256],
        }
    }

    pub fn record_time(&mut self, label: &str, elapsed_ms: u64) {
        if self.sample_count < self.max_samples {
            self.samples[self.sample_count] = elapsed_ms;
            self.sample_count += 1;
        }
    }

    pub fn get_average_time(&self) -> u64 {
        if self.sample_count == 0 {
            return 0;
        }
        let sum: u64 = self.samples[..self.sample_count].iter().sum();
        sum / (self.sample_count as u64)
    }

    pub fn get_max_time(&self) -> u64 {
        if self.sample_count == 0 {
            return 0;
        }
        *self.samples[..self.sample_count].iter().max().unwrap_or(&0)
    }

    pub fn reset(&mut self) {
        self.sample_count = 0;
        for s in self.samples.iter_mut() {
            *s = 0;
        }
    }
}

// ===== Crash Dump & Recovery =====

pub struct CrashDump {
    timestamp: u64,
    exception_code: u32,
    error_message: [u8; 256],
    error_len: usize,
    register_dump: [u64; 16],  // General purpose registers
}

impl CrashDump {
    pub fn new() -> Self {
        CrashDump {
            timestamp: 0,
            exception_code: 0,
            error_message: [0u8; 256],
            error_len: 0,
            register_dump: [0u64; 16],
        }
    }

    pub fn record_exception(&mut self, code: u32, message: &str) {
        self.exception_code = code;
        let len = core::cmp::min(message.len(), 255);
        for i in 0..len {
            self.error_message[i] = message.as_bytes()[i];
        }
        self.error_len = len;
    }

    pub fn set_registers(&mut self, regs: [u64; 16]) {
        self.register_dump = regs;
    }

    pub fn get_error_message(&self) -> &str {
        unsafe {
            core::str::from_utf8_unchecked(&self.error_message[..self.error_len])
        }
    }
}

// ===== System Health Monitor =====

pub struct HealthMonitor {
    last_heartbeat: u64,
    watchdog_timeout_ms: u32,
    component_status: [u32; 10],  // Bitmask of component health
    failure_count: u32,
    recovery_attempts: u32,
}

impl HealthMonitor {
    pub fn new() -> Self {
        HealthMonitor {
            last_heartbeat: 0,
            watchdog_timeout_ms: 5000,  // 5 seconds
            component_status: [0xFFFFFFFFu32; 10],  // All healthy (all bits set)
            failure_count: 0,
            recovery_attempts: 0,
        }
    }

    pub fn heartbeat(&mut self, timestamp_ms: u64) {
        self.last_heartbeat = timestamp_ms;
    }

    pub fn set_component_status(&mut self, component_id: usize, healthy: bool) {
        if component_id < 10 {
            if healthy {
                self.component_status[component_id] |= 1;
            } else {
                self.component_status[component_id] &= !1;
            }
        }
    }

    pub fn is_component_healthy(&self, component_id: usize) -> bool {
        if component_id < 10 {
            (self.component_status[component_id] & 1) != 0
        } else {
            false
        }
    }

    pub fn check_watchdog(&self, current_time_ms: u64) -> bool {
        // Watchdog expired if no heartbeat in timeout period
        current_time_ms - self.last_heartbeat <= (self.watchdog_timeout_ms as u64)
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
    }

    pub fn record_recovery_attempt(&mut self) {
        self.recovery_attempts += 1;
    }

    pub fn get_system_health(&self) -> f32 {
        // Health percentage (0-100%)
        let healthy_components = self.component_status.iter()
            .filter(|s| (**s & 1) != 0)
            .count();
        ((healthy_components as f32) / 10.0) * 100.0
    }
}

// ===== Global Logger Instance =====

static KERNEL_LOGGER: KernelLogger = KernelLogger::new();

pub fn kernel_logger() -> &'static KernelLogger {
    &KERNEL_LOGGER
}

// ===== Logging Macros (for convenience) =====

#[macro_export]
macro_rules! log_info {
    ($src:expr, $msg:expr) => {
        $crate::logging::kernel_logger().log(
            $crate::logging::LogLevel::Info,
            $src,
            $msg,
        )
    };
}

#[macro_export]
macro_rules! log_warn {
    ($src:expr, $msg:expr) => {
        $crate::logging::kernel_logger().log(
            $crate::logging::LogLevel::Warn,
            $src,
            $msg,
        )
    };
}

#[macro_export]
macro_rules! log_error {
    ($src:expr, $msg:expr) => {
        $crate::logging::kernel_logger().log(
            $crate::logging::LogLevel::Error,
            $src,
            $msg,
        )
    };
}

// ===== Debug Output Functions =====

pub fn dump_system_state(output: &mut dyn Write) {
    let logger = kernel_logger();
    let _ = writeln!(output, "System State Dump:");
    let _ = writeln!(output, "  Total log messages: {}", logger.get_total_logs());
    let _ = writeln!(output, "  Info: {}", logger.get_logs_by_level(LogLevel::Info));
    let _ = writeln!(output, "  Warnings: {}", logger.get_logs_by_level(LogLevel::Warn));
    let _ = writeln!(output, "  Errors: {}", logger.get_logs_by_level(LogLevel::Error));
}

pub fn dump_health_status(health: &HealthMonitor, output: &mut dyn Write) {
    let _ = writeln!(output, "System Health Report:");
    let _ = writeln!(output, "  Overall health: {:.1}%", health.get_system_health());
    let _ = writeln!(output, "  Failures recorded: {}", health.failure_count);
    let _ = writeln!(output, "  Recovery attempts: {}", health.recovery_attempts);
}

// ===== Testing Functions =====

pub fn test_logging_system() -> bool {
    let logger = kernel_logger();
    
    // Record some test messages
    logger.log(LogLevel::Info, "test", "Test info message");
    logger.log(LogLevel::Warn, "test", "Test warning message");
    logger.log(LogLevel::Error, "test", "Test error message");

    // Verify counts
    let total = logger.get_total_logs();
    if total < 3 {
        return false;
    }

    true
}

pub fn test_performance_monitor() -> bool {
    let mut monitor = PerformanceMonitor::new();

    monitor.record_time("boot", 100);
    monitor.record_time("init", 50);
    monitor.record_time("shell", 25);

    let avg = monitor.get_average_time();
    let max = monitor.get_max_time();

    if avg == 0 || max == 0 {
        return false;
    }

    true
}

pub fn test_health_monitor() -> bool {
    let mut health = HealthMonitor::new();

    health.set_component_status(0, true);
    health.set_component_status(1, true);
    health.set_component_status(2, false);

    if !health.is_component_healthy(0) {
        return false;
    }

    if health.is_component_healthy(2) {
        return false;
    }

    let h = health.get_system_health();
    if h <= 0.0 || h > 100.0 {
        return false;
    }

    true
}
