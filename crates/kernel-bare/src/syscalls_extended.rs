// RAYOS Phase 9A Task 4: Extended Syscall Implementations
// Full POSIX-like syscall interface for applications
// File: crates/kernel-bare/src/syscalls_extended.rs

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// ============================================================================
// FILE DESCRIPTOR TABLE
// ============================================================================

const MAX_FDS: usize = 256;
const MAX_PROCESSES_FD: usize = 64;

/// File descriptor flags
pub mod fd_flags {
    pub const O_RDONLY: u32 = 0x0000;
    pub const O_WRONLY: u32 = 0x0001;
    pub const O_RDWR: u32 = 0x0002;
    pub const O_CREAT: u32 = 0x0040;
    pub const O_TRUNC: u32 = 0x0200;
    pub const O_APPEND: u32 = 0x0400;
    pub const O_NONBLOCK: u32 = 0x0800;
    pub const O_CLOEXEC: u32 = 0x80000;
}

/// Seek whence values
pub mod seek_whence {
    pub const SEEK_SET: u32 = 0;
    pub const SEEK_CUR: u32 = 1;
    pub const SEEK_END: u32 = 2;
}

/// File type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FdType {
    Closed,
    File,
    Directory,
    Pipe,
    Socket,
    Device,
    Stdin,
    Stdout,
    Stderr,
}

/// File descriptor entry
#[derive(Clone, Copy)]
pub struct FileDescriptor {
    pub fd_type: FdType,
    pub flags: u32,
    pub position: u64,
    pub file_index: usize,  // Index into memfs or special device
    pub in_use: bool,
}

impl FileDescriptor {
    pub const fn empty() -> Self {
        FileDescriptor {
            fd_type: FdType::Closed,
            flags: 0,
            position: 0,
            file_index: 0,
            in_use: false,
        }
    }

    pub fn new(fd_type: FdType, flags: u32, file_index: usize) -> Self {
        FileDescriptor {
            fd_type,
            flags,
            position: 0,
            file_index,
            in_use: true,
        }
    }
}

/// Per-process file descriptor table
pub struct FdTable {
    pub fds: [FileDescriptor; MAX_FDS],
    pub pid: u32,
}

impl FdTable {
    pub const fn new(pid: u32) -> Self {
        FdTable {
            fds: [FileDescriptor::empty(); MAX_FDS],
            pid,
        }
    }

    pub fn init_stdio(&mut self) {
        // FD 0 = stdin
        self.fds[0] = FileDescriptor::new(FdType::Stdin, fd_flags::O_RDONLY, 0);
        // FD 1 = stdout
        self.fds[1] = FileDescriptor::new(FdType::Stdout, fd_flags::O_WRONLY, 0);
        // FD 2 = stderr
        self.fds[2] = FileDescriptor::new(FdType::Stderr, fd_flags::O_WRONLY, 0);
    }

    pub fn alloc_fd(&mut self) -> Option<usize> {
        for i in 3..MAX_FDS {  // Start at 3, after stdio
            if !self.fds[i].in_use {
                return Some(i);
            }
        }
        None
    }

    pub fn get_fd(&self, fd: usize) -> Option<&FileDescriptor> {
        if fd < MAX_FDS && self.fds[fd].in_use {
            Some(&self.fds[fd])
        } else {
            None
        }
    }

    pub fn get_fd_mut(&mut self, fd: usize) -> Option<&mut FileDescriptor> {
        if fd < MAX_FDS && self.fds[fd].in_use {
            Some(&mut self.fds[fd])
        } else {
            None
        }
    }

    pub fn close_fd(&mut self, fd: usize) -> bool {
        if fd < MAX_FDS && self.fds[fd].in_use {
            self.fds[fd] = FileDescriptor::empty();
            true
        } else {
            false
        }
    }
}

// ============================================================================
// SIGNAL HANDLING
// ============================================================================

/// Signal numbers
pub mod signals {
    pub const SIGHUP: u32 = 1;
    pub const SIGINT: u32 = 2;
    pub const SIGQUIT: u32 = 3;
    pub const SIGILL: u32 = 4;
    pub const SIGTRAP: u32 = 5;
    pub const SIGABRT: u32 = 6;
    pub const SIGBUS: u32 = 7;
    pub const SIGFPE: u32 = 8;
    pub const SIGKILL: u32 = 9;
    pub const SIGUSR1: u32 = 10;
    pub const SIGSEGV: u32 = 11;
    pub const SIGUSR2: u32 = 12;
    pub const SIGPIPE: u32 = 13;
    pub const SIGALRM: u32 = 14;
    pub const SIGTERM: u32 = 15;
    pub const SIGCHLD: u32 = 17;
    pub const SIGCONT: u32 = 18;
    pub const SIGSTOP: u32 = 19;
    pub const SIGTSTP: u32 = 20;
}

/// Signal handler type
pub const SIG_DFL: u64 = 0;
pub const SIG_IGN: u64 = 1;

/// Signal state for a process
#[derive(Clone, Copy)]
pub struct SignalState {
    pub handlers: [u64; 32],       // Signal handlers (address or SIG_DFL/SIG_IGN)
    pub pending: u32,              // Bitmask of pending signals
    pub blocked: u32,              // Bitmask of blocked signals
    pub alarm_ticks: u64,          // Ticks until SIGALRM
}

impl SignalState {
    pub const fn new() -> Self {
        SignalState {
            handlers: [SIG_DFL; 32],
            pending: 0,
            blocked: 0,
            alarm_ticks: 0,
        }
    }

    pub fn set_handler(&mut self, sig: u32, handler: u64) -> u64 {
        if sig > 0 && sig < 32 {
            let old = self.handlers[sig as usize];
            self.handlers[sig as usize] = handler;
            old
        } else {
            SIG_DFL
        }
    }

    pub fn send_signal(&mut self, sig: u32) {
        if sig > 0 && sig < 32 {
            // SIGKILL and SIGSTOP cannot be blocked
            if sig != signals::SIGKILL && sig != signals::SIGSTOP {
                if (self.blocked & (1 << sig)) != 0 {
                    self.pending |= 1 << sig;
                    return;
                }
            }
            self.pending |= 1 << sig;
        }
    }

    pub fn has_pending(&self) -> bool {
        (self.pending & !self.blocked) != 0
    }

    pub fn next_pending(&mut self) -> Option<u32> {
        let deliverable = self.pending & !self.blocked;
        if deliverable != 0 {
            for sig in 1..32 {
                if (deliverable & (1 << sig)) != 0 {
                    self.pending &= !(1 << sig);
                    return Some(sig);
                }
            }
        }
        None
    }
}

// ============================================================================
// FILE STAT STRUCTURE
// ============================================================================

/// File statistics (simplified stat structure)
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FileStat {
    pub st_dev: u64,       // Device ID
    pub st_ino: u64,       // Inode number
    pub st_mode: u32,      // File mode (permissions + type)
    pub st_nlink: u32,     // Number of hard links
    pub st_uid: u32,       // Owner user ID
    pub st_gid: u32,       // Owner group ID
    pub st_rdev: u64,      // Device ID (if special file)
    pub st_size: u64,      // File size in bytes
    pub st_blksize: u32,   // Block size for I/O
    pub st_blocks: u64,    // Number of 512-byte blocks
    pub st_atime: u64,     // Access time
    pub st_mtime: u64,     // Modification time
    pub st_ctime: u64,     // Status change time
}

/// File mode bits
pub mod mode_bits {
    pub const S_IFMT: u32 = 0o170000;   // File type mask
    pub const S_IFREG: u32 = 0o100000;  // Regular file
    pub const S_IFDIR: u32 = 0o040000;  // Directory
    pub const S_IFCHR: u32 = 0o020000;  // Character device
    pub const S_IFBLK: u32 = 0o060000;  // Block device
    pub const S_IFIFO: u32 = 0o010000;  // FIFO
    pub const S_IFLNK: u32 = 0o120000;  // Symbolic link
    pub const S_IFSOCK: u32 = 0o140000; // Socket

    pub const S_IRWXU: u32 = 0o0700;    // Owner read/write/execute
    pub const S_IRUSR: u32 = 0o0400;    // Owner read
    pub const S_IWUSR: u32 = 0o0200;    // Owner write
    pub const S_IXUSR: u32 = 0o0100;    // Owner execute
    pub const S_IRWXG: u32 = 0o0070;    // Group read/write/execute
    pub const S_IRGRP: u32 = 0o0040;    // Group read
    pub const S_IWGRP: u32 = 0o0020;    // Group write
    pub const S_IXGRP: u32 = 0o0010;    // Group execute
    pub const S_IRWXO: u32 = 0o0007;    // Other read/write/execute
    pub const S_IROTH: u32 = 0o0004;    // Other read
    pub const S_IWOTH: u32 = 0o0002;    // Other write
    pub const S_IXOTH: u32 = 0o0001;    // Other execute
}

// ============================================================================
// RESOURCE USAGE
// ============================================================================

/// Resource usage structure
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ResourceUsage {
    pub ru_utime_sec: u64,     // User CPU time (seconds)
    pub ru_utime_usec: u64,    // User CPU time (microseconds)
    pub ru_stime_sec: u64,     // System CPU time (seconds)
    pub ru_stime_usec: u64,    // System CPU time (microseconds)
    pub ru_maxrss: u64,        // Maximum resident set size
    pub ru_minflt: u64,        // Page faults (no I/O)
    pub ru_majflt: u64,        // Page faults (with I/O)
    pub ru_nvcsw: u64,         // Voluntary context switches
    pub ru_nivcsw: u64,        // Involuntary context switches
}

/// Process times structure
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ProcessTimes {
    pub tms_utime: u64,   // User CPU time
    pub tms_stime: u64,   // System CPU time
    pub tms_cutime: u64,  // User CPU time of children
    pub tms_cstime: u64,  // System CPU time of children
}

// ============================================================================
// UNAME STRUCTURE
// ============================================================================

/// System identification
#[repr(C)]
pub struct Utsname {
    pub sysname: [u8; 65],     // Operating system name
    pub nodename: [u8; 65],    // Network node name
    pub release: [u8; 65],     // OS release
    pub version: [u8; 65],     // OS version
    pub machine: [u8; 65],     // Hardware type
}

impl Utsname {
    pub fn rayos() -> Self {
        let mut u = Utsname {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
        };

        // Copy strings into fixed arrays
        Self::copy_str(&mut u.sysname, b"RayOS");
        Self::copy_str(&mut u.nodename, b"rayos-kernel");
        Self::copy_str(&mut u.release, b"1.0.0");
        Self::copy_str(&mut u.version, b"Phase 9A Task 4");
        Self::copy_str(&mut u.machine, b"x86_64");

        u
    }

    fn copy_str(dest: &mut [u8; 65], src: &[u8]) {
        let len = src.len().min(64);
        dest[..len].copy_from_slice(&src[..len]);
        dest[len] = 0;
    }
}

// ============================================================================
// MEMORY MAPPING
// ============================================================================

/// Memory protection flags
pub mod prot_flags {
    pub const PROT_NONE: u32 = 0x0;
    pub const PROT_READ: u32 = 0x1;
    pub const PROT_WRITE: u32 = 0x2;
    pub const PROT_EXEC: u32 = 0x4;
}

/// Memory mapping flags
pub mod map_flags {
    pub const MAP_SHARED: u32 = 0x01;
    pub const MAP_PRIVATE: u32 = 0x02;
    pub const MAP_FIXED: u32 = 0x10;
    pub const MAP_ANONYMOUS: u32 = 0x20;
}

/// Memory mapping entry
#[derive(Clone, Copy)]
pub struct MemoryMapping {
    pub start: u64,
    pub length: u64,
    pub prot: u32,
    pub flags: u32,
    pub file_index: usize,
    pub offset: u64,
    pub in_use: bool,
}

impl MemoryMapping {
    pub const fn empty() -> Self {
        MemoryMapping {
            start: 0,
            length: 0,
            prot: 0,
            flags: 0,
            file_index: 0,
            offset: 0,
            in_use: false,
        }
    }
}

const MAX_MAPPINGS: usize = 64;

/// Per-process memory map table
pub struct MmapTable {
    pub mappings: [MemoryMapping; MAX_MAPPINGS],
    pub next_addr: u64,
    pub brk: u64,  // Program break (heap end)
}

impl MmapTable {
    pub const fn new() -> Self {
        MmapTable {
            mappings: [MemoryMapping::empty(); MAX_MAPPINGS],
            next_addr: 0x0000_7000_0000_0000,  // User space start
            brk: 0x0000_0000_0040_0000,        // Default heap start (4MB)
        }
    }

    pub fn mmap(&mut self, addr: u64, length: u64, prot: u32, flags: u32) -> Option<u64> {
        // Find free slot
        for i in 0..MAX_MAPPINGS {
            if !self.mappings[i].in_use {
                let start = if addr == 0 {
                    let s = self.next_addr;
                    self.next_addr += (length + 0xFFF) & !0xFFF;  // Page align
                    s
                } else {
                    addr
                };

                self.mappings[i] = MemoryMapping {
                    start,
                    length,
                    prot,
                    flags,
                    file_index: 0,
                    offset: 0,
                    in_use: true,
                };

                return Some(start);
            }
        }
        None
    }

    pub fn munmap(&mut self, addr: u64, length: u64) -> bool {
        for i in 0..MAX_MAPPINGS {
            if self.mappings[i].in_use && self.mappings[i].start == addr {
                let _ = length;  // Would verify length matches
                self.mappings[i] = MemoryMapping::empty();
                return true;
            }
        }
        false
    }

    pub fn brk_set(&mut self, addr: u64) -> u64 {
        if addr == 0 {
            // Query current break
            self.brk
        } else if addr >= 0x0000_0000_0010_0000 && addr < 0x0000_7000_0000_0000 {
            // Set new break
            self.brk = addr;
            self.brk
        } else {
            // Invalid address
            self.brk
        }
    }
}

// ============================================================================
// ERRNO VALUES
// ============================================================================

pub mod errno {
    pub const EPERM: u32 = 1;       // Operation not permitted
    pub const ENOENT: u32 = 2;      // No such file or directory
    pub const ESRCH: u32 = 3;       // No such process
    pub const EINTR: u32 = 4;       // Interrupted system call
    pub const EIO: u32 = 5;         // I/O error
    pub const ENXIO: u32 = 6;       // No such device or address
    pub const E2BIG: u32 = 7;       // Argument list too long
    pub const ENOEXEC: u32 = 8;     // Exec format error
    pub const EBADF: u32 = 9;       // Bad file descriptor
    pub const ECHILD: u32 = 10;     // No child processes
    pub const EAGAIN: u32 = 11;     // Try again
    pub const ENOMEM: u32 = 12;     // Out of memory
    pub const EACCES: u32 = 13;     // Permission denied
    pub const EFAULT: u32 = 14;     // Bad address
    pub const EBUSY: u32 = 16;      // Device or resource busy
    pub const EEXIST: u32 = 17;     // File exists
    pub const EXDEV: u32 = 18;      // Cross-device link
    pub const ENODEV: u32 = 19;     // No such device
    pub const ENOTDIR: u32 = 20;    // Not a directory
    pub const EISDIR: u32 = 21;     // Is a directory
    pub const EINVAL: u32 = 22;     // Invalid argument
    pub const ENFILE: u32 = 23;     // File table overflow
    pub const EMFILE: u32 = 24;     // Too many open files
    pub const ENOTTY: u32 = 25;     // Not a typewriter
    pub const ETXTBSY: u32 = 26;    // Text file busy
    pub const EFBIG: u32 = 27;      // File too large
    pub const ENOSPC: u32 = 28;     // No space left on device
    pub const ESPIPE: u32 = 29;     // Illegal seek
    pub const EROFS: u32 = 30;      // Read-only file system
    pub const EMLINK: u32 = 31;     // Too many links
    pub const EPIPE: u32 = 32;      // Broken pipe
    pub const EDOM: u32 = 33;       // Math argument out of domain
    pub const ERANGE: u32 = 34;     // Math result not representable
    pub const ENOSYS: u32 = 38;     // Function not implemented
    pub const ENOTEMPTY: u32 = 39;  // Directory not empty
    pub const EWOULDBLOCK: u32 = 11; // Same as EAGAIN
}

// ============================================================================
// SYSCONF VALUES
// ============================================================================

pub mod sysconf_names {
    pub const _SC_ARG_MAX: i32 = 0;
    pub const _SC_CHILD_MAX: i32 = 1;
    pub const _SC_CLK_TCK: i32 = 2;
    pub const _SC_NGROUPS_MAX: i32 = 3;
    pub const _SC_OPEN_MAX: i32 = 4;
    pub const _SC_PAGESIZE: i32 = 30;
    pub const _SC_PAGE_SIZE: i32 = 30;  // Same as _SC_PAGESIZE
    pub const _SC_PHYS_PAGES: i32 = 85;
    pub const _SC_NPROCESSORS_CONF: i32 = 83;
    pub const _SC_NPROCESSORS_ONLN: i32 = 84;
}

/// Get system configuration value
pub fn sysconf_value(name: i32) -> Option<u64> {
    match name {
        sysconf_names::_SC_ARG_MAX => Some(0x20000),          // 128KB
        sysconf_names::_SC_CHILD_MAX => Some(1024),
        sysconf_names::_SC_CLK_TCK => Some(100),              // 100 ticks/sec
        sysconf_names::_SC_NGROUPS_MAX => Some(65536),
        sysconf_names::_SC_OPEN_MAX => Some(MAX_FDS as u64),
        sysconf_names::_SC_PAGESIZE => Some(4096),
        sysconf_names::_SC_PHYS_PAGES => Some(262144),        // 1GB / 4KB pages
        sysconf_names::_SC_NPROCESSORS_CONF => Some(1),
        sysconf_names::_SC_NPROCESSORS_ONLN => Some(1),
        _ => None,
    }
}

// ============================================================================
// GLOBAL STATE
// ============================================================================

static mut FD_TABLES: [Option<FdTable>; MAX_PROCESSES_FD] = {
    const NONE: Option<FdTable> = None;
    [NONE; MAX_PROCESSES_FD]
};

static mut SIGNAL_STATES: [SignalState; MAX_PROCESSES_FD] = [SignalState::new(); MAX_PROCESSES_FD];

static mut MMAP_TABLES: [MmapTable; MAX_PROCESSES_FD] = {
    const NEW_TABLE: MmapTable = MmapTable::new();
    [NEW_TABLE; MAX_PROCESSES_FD]
};

static SYSCALL_LOCK: AtomicBool = AtomicBool::new(false);
static SYSCALL_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn acquire_lock() {
    while SYSCALL_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

fn release_lock() {
    SYSCALL_LOCK.store(false, Ordering::Release);
}

/// Initialize syscall subsystem for a process
pub fn syscall_init_process(pid: u32) {
    if pid as usize >= MAX_PROCESSES_FD {
        return;
    }

    acquire_lock();
    unsafe {
        let mut table = FdTable::new(pid);
        table.init_stdio();
        FD_TABLES[pid as usize] = Some(table);
        SIGNAL_STATES[pid as usize] = SignalState::new();
        MMAP_TABLES[pid as usize] = MmapTable::new();
    }
    release_lock();
}

/// Get FD table for process
pub fn get_fd_table(pid: u32) -> Option<&'static mut FdTable> {
    if pid as usize >= MAX_PROCESSES_FD {
        return None;
    }
    unsafe {
        FD_TABLES[pid as usize].as_mut()
    }
}

/// Get signal state for process
pub fn get_signal_state(pid: u32) -> Option<&'static mut SignalState> {
    if pid as usize >= MAX_PROCESSES_FD {
        return None;
    }
    unsafe {
        Some(&mut SIGNAL_STATES[pid as usize])
    }
}

/// Get mmap table for process
pub fn get_mmap_table(pid: u32) -> Option<&'static mut MmapTable> {
    if pid as usize >= MAX_PROCESSES_FD {
        return None;
    }
    unsafe {
        Some(&mut MMAP_TABLES[pid as usize])
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fd_table_alloc() {
        let mut table = FdTable::new(1);
        table.init_stdio();
        let fd = table.alloc_fd();
        assert_eq!(fd, Some(3));
    }

    #[test]
    fn test_fd_table_close() {
        let mut table = FdTable::new(1);
        table.init_stdio();
        assert!(table.close_fd(0));  // Close stdin
        assert!(!table.close_fd(0)); // Already closed
    }

    #[test]
    fn test_signal_state() {
        let mut sig = SignalState::new();
        sig.send_signal(signals::SIGTERM);
        assert!(sig.has_pending());
        assert_eq!(sig.next_pending(), Some(signals::SIGTERM));
        assert!(!sig.has_pending());
    }

    #[test]
    fn test_signal_blocked() {
        let mut sig = SignalState::new();
        sig.blocked = 1 << signals::SIGTERM;
        sig.send_signal(signals::SIGTERM);
        assert!(!sig.has_pending());  // Blocked
        assert_eq!(sig.pending, 1 << signals::SIGTERM);
    }

    #[test]
    fn test_mmap_table() {
        let mut mmap = MmapTable::new();
        let addr = mmap.mmap(0, 4096, prot_flags::PROT_READ, map_flags::MAP_PRIVATE);
        assert!(addr.is_some());
    }

    #[test]
    fn test_brk() {
        let mut mmap = MmapTable::new();
        let old_brk = mmap.brk_set(0);
        assert!(old_brk > 0);
        let new_brk = mmap.brk_set(0x0000_0000_0080_0000);
        assert_eq!(new_brk, 0x0000_0000_0080_0000);
    }

    #[test]
    fn test_utsname() {
        let u = Utsname::rayos();
        assert_eq!(&u.sysname[..5], b"RayOS");
    }

    #[test]
    fn test_sysconf() {
        assert_eq!(sysconf_value(sysconf_names::_SC_PAGESIZE), Some(4096));
        assert_eq!(sysconf_value(sysconf_names::_SC_OPEN_MAX), Some(MAX_FDS as u64));
    }

    #[test]
    fn test_file_stat_size() {
        assert_eq!(core::mem::size_of::<FileStat>(), 128);
    }
}
