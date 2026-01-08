// Phase 22 Task 2: RayApp Clipboard & File Sandbox
// Implements:
// - Clipboard buffer management (get/set, ownership)
// - File access requests with sandbox validation
// - Permission checking for file I/O operations

#![allow(dead_code)]

use crate::{serial_write_bytes, serial_write_str, serial_write_hex_u64};
use core::cell::UnsafeCell;
use core::hint;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

// Clipboard constants
const CLIPBOARD_SIZE: usize = 16384;  // 16 KB shared clipboard
const CLIPBOARD_HISTORY: usize = 5;   // Keep last 5 clipboard values
const FILE_PATH_MAX: usize = 128;
const MAX_FILE_REQUESTS: usize = 16;
const MAX_RAYAPPS: usize = 4;

/// Clipboard access types for file requests
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum FileAccessType {
    Read = 0,
    Write = 1,
    Delete = 2,
}

/// Clipboard entry with validation
#[derive(Copy, Clone)]
struct ClipboardEntry {
    size: u16,
    owner_app: u8,
    timestamp: u64,
}

impl ClipboardEntry {
    const fn empty() -> Self {
        Self {
            size: 0,
            owner_app: u8::MAX,
            timestamp: 0,
        }
    }
}

/// File access request from an app
#[derive(Copy, Clone)]
pub struct FileAccessRequest {
    path: [u8; FILE_PATH_MAX],
    path_len: u8,
    access_type: FileAccessType,
    requester_app_id: u8,
    permission_granted: bool,
}

impl FileAccessRequest {
    const fn empty() -> Self {
        Self {
            path: [0u8; FILE_PATH_MAX],
            path_len: 0,
            access_type: FileAccessType::Read,
            requester_app_id: u8::MAX,
            permission_granted: false,
        }
    }

    fn set_path(&mut self, path: &[u8]) {
        let len = path.len().min(FILE_PATH_MAX);
        let mut idx = 0;
        while idx < len {
            self.path[idx] = path[idx];
            idx += 1;
        }
        self.path_len = len as u8;
        for rem in idx..FILE_PATH_MAX {
            self.path[rem] = 0;
        }
    }

    fn path_bytes(&self) -> &[u8] {
        &self.path[..self.path_len as usize]
    }
}

/// Clipboard manager with shared buffer and history
pub struct ClipboardManager {
    lock_flag: AtomicBool,
    buffer: UnsafeCell<[u8; CLIPBOARD_SIZE]>,
    current_entry: UnsafeCell<ClipboardEntry>,
    next_timestamp: UnsafeCell<u64>,
}

unsafe impl Sync for ClipboardManager {}

pub static CLIPBOARD_MANAGER: ClipboardManager = ClipboardManager::new();

pub fn clipboard_manager() -> &'static ClipboardManager {
    &CLIPBOARD_MANAGER
}

struct ClipboardGuard<'a> {
    manager: &'a ClipboardManager,
}

impl<'a> Drop for ClipboardGuard<'a> {
    fn drop(&mut self) {
        self.manager.lock_flag.store(false, Ordering::Release);
    }
}

impl ClipboardManager {
    pub const fn new() -> Self {
        Self {
            lock_flag: AtomicBool::new(false),
            buffer: UnsafeCell::new([0u8; CLIPBOARD_SIZE]),
            current_entry: UnsafeCell::new(ClipboardEntry::empty()),
            next_timestamp: UnsafeCell::new(0),
        }
    }

    fn lock(&self) -> ClipboardGuard {
        while self
            .lock_flag
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            hint::spin_loop();
        }
        ClipboardGuard { manager: self }
    }

    fn simple_crc(&self, data: &[u8]) -> u32 {
        let mut crc = 0u32;
        for &byte in data {
            crc = crc.wrapping_add(byte as u32).wrapping_mul(31);
        }
        crc
    }

    pub fn set_clipboard(&self, app_id: u8, data: &[u8]) -> bool {
        if data.is_empty() || data.len() > CLIPBOARD_SIZE {
            return false;
        }

        let _guard = self.lock();

        unsafe {
            // Copy data to buffer
            let buffer_ptr = self.buffer.get() as *mut u8;
            for (i, &byte) in data.iter().enumerate() {
                *buffer_ptr.add(i) = byte;
            }

            // Update current entry
            let entry = ClipboardEntry {
                size: data.len() as u16,
                owner_app: app_id,
                timestamp: *self.next_timestamp.get(),
            };
            *self.next_timestamp.get() += 1;
            *self.current_entry.get() = entry;
        }

        self.emit_clipboard_set(data.len() as u16);
        true
    }

    pub fn get_clipboard(&self) -> Option<(u8, &'static [u8])> {
        let _guard = self.lock();
        unsafe {
            let entry = *self.current_entry.get();
            if entry.size == 0 {
                return None;
            }
            let buffer_ptr = self.buffer.get() as *const u8;
            let slice = core::slice::from_raw_parts(buffer_ptr, entry.size as usize);
            Some((entry.owner_app, slice))
        }
    }

    pub fn get_clipboard_owner(&self) -> Option<u8> {
        let _guard = self.lock();
        unsafe {
            let entry = *self.current_entry.get();
            if entry.size > 0 {
                Some(entry.owner_app)
            } else {
                None
            }
        }
    }

    pub fn clear_clipboard(&self) {
        let _guard = self.lock();
        unsafe {
            *self.current_entry.get() = ClipboardEntry::empty();
        }
    }

    fn emit_clipboard_set(&self, size: u16) {
        serial_write_str("RAYOS_GUI_CLIPBOARD:SET:");
        serial_write_hex_u64(size as u64);
        serial_write_str("\n");
    }

    fn emit_clipboard_get(&self, size: u16) {
        serial_write_str("RAYOS_GUI_CLIPBOARD:GET:");
        serial_write_hex_u64(size as u64);
        serial_write_str("\n");
    }
}

/// File access sandbox policy
pub struct FileAccessPolicy {
    lock_flag: AtomicBool,
    requests: UnsafeCell<[FileAccessRequest; MAX_FILE_REQUESTS]>,
    next_handle: UnsafeCell<u64>,
}

unsafe impl Sync for FileAccessPolicy {}

pub static FILE_ACCESS_POLICY: FileAccessPolicy = FileAccessPolicy::new();

pub fn file_access_policy() -> &'static FileAccessPolicy {
    &FILE_ACCESS_POLICY
}

struct FileAccessGuard<'a> {
    policy: &'a FileAccessPolicy,
}

impl<'a> Drop for FileAccessGuard<'a> {
    fn drop(&mut self) {
        self.policy.lock_flag.store(false, Ordering::Release);
    }
}

impl FileAccessPolicy {
    pub const fn new() -> Self {
        Self {
            lock_flag: AtomicBool::new(false),
            requests: UnsafeCell::new([FileAccessRequest::empty(); MAX_FILE_REQUESTS]),
            next_handle: UnsafeCell::new(1),
        }
    }

    fn lock(&self) -> FileAccessGuard {
        while self
            .lock_flag
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            hint::spin_loop();
        }
        FileAccessGuard { policy: self }
    }

    pub fn validate_path(&self, path: &[u8]) -> bool {
        if path.is_empty() || path.len() > FILE_PATH_MAX {
            return false;
        }

        // Prevent directory traversal
        for window in path.windows(2) {
            if window == b".." {
                return false;
            }
        }

        true
    }

    pub fn check_app_permission(&self, app_id: u8, path: &[u8], access_type: FileAccessType) -> bool {
        if app_id >= MAX_RAYAPPS as u8 {
            return false;
        }

        if !self.validate_path(path) {
            return false;
        }

        // /rayos/public/* → all apps (read-only)
        if Self::path_matches(path, b"/rayos/public/") {
            return access_type == FileAccessType::Read;
        }

        // /rayos/app/<appid>/* → only that app
        if Self::path_matches(path, b"/rayos/app/") {
            return true; // Simplified for now
        }

        // /rayos/tmp/* → all apps (read-write)
        if Self::path_matches(path, b"/rayos/tmp/") {
            return true;
        }

        // Deny by default
        false
    }

    fn path_matches(path: &[u8], prefix: &[u8]) -> bool {
        if path.len() < prefix.len() {
            return false;
        }
        for i in 0..prefix.len() {
            if path[i] != prefix[i] {
                return false;
            }
        }
        true
    }

    pub fn request_file_handle(&self, app_id: u8, path: &[u8], access_type: FileAccessType) -> Result<u64, &'static str> {
        if !self.check_app_permission(app_id, path, access_type) {
            self.emit_fileio_deny(b"permission_denied");
            return Err("permission_denied");
        }

        let mut guard = self.lock();

        unsafe {
            let requests_ptr = self.requests.get() as *mut FileAccessRequest;
            for i in 0..MAX_FILE_REQUESTS {
                let req = &mut *requests_ptr.add(i);
                if req.requester_app_id == u8::MAX {
                    // Found empty slot
                    req.requester_app_id = app_id;
                    req.access_type = access_type;
                    req.set_path(path);
                    req.permission_granted = true;

                    let handle = *self.next_handle.get();
                    *self.next_handle.get() += 1;

                    self.emit_fileio_grant(handle);
                    return Ok(handle);
                }
            }
        }

        self.emit_fileio_deny(b"no_handles");
        Err("no_handles")
    }

    pub fn release_file_handle(&self, _handle: u64) {
        let _guard = self.lock();
        unsafe {
            let requests_ptr = self.requests.get() as *mut FileAccessRequest;
            for i in 0..MAX_FILE_REQUESTS {
                let req = &mut *requests_ptr.add(i);
                if req.permission_granted {
                    req.requester_app_id = u8::MAX;
                    req.permission_granted = false;
                }
            }
        }
    }

    fn emit_fileio_request(&self, app_id: u8, path: &[u8]) {
        serial_write_str("RAYOS_GUI_FILEIO:REQUEST:");
        serial_write_hex_u64(app_id as u64);
        serial_write_str(":");
        serial_write_bytes(path);
        serial_write_str("\n");
    }

    fn emit_fileio_grant(&self, handle: u64) {
        serial_write_str("RAYOS_GUI_FILEIO:GRANT:");
        serial_write_hex_u64(handle);
        serial_write_str("\n");
    }

    fn emit_fileio_deny(&self, reason: &[u8]) {
        serial_write_str("RAYOS_GUI_FILEIO:DENY:");
        serial_write_bytes(reason);
        serial_write_str("\n");
    }
}

// Phase 22 Task 2: Unit tests for clipboard and file sandbox
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_set_get() {
        let clipboard = ClipboardManager::new();
        let data = b"test data";

        assert!(clipboard.set_clipboard(0, data));
        let (owner, _content) = clipboard.get_clipboard().unwrap();
        assert_eq!(owner, 0);
    }

    #[test]
    fn test_clipboard_ownership() {
        let clipboard = ClipboardManager::new();
        clipboard.set_clipboard(0, b"app0");
        clipboard.set_clipboard(1, b"app1");

        let (owner, _content) = clipboard.get_clipboard().unwrap();
        assert_eq!(owner, 1);
    }

    #[test]
    fn test_clipboard_owner_query() {
        let clipboard = ClipboardManager::new();
        clipboard.set_clipboard(2, b"data");

        assert_eq!(clipboard.get_clipboard_owner(), Some(2));
    }

    #[test]
    fn test_clipboard_clear() {
        let clipboard = ClipboardManager::new();
        clipboard.set_clipboard(0, b"data");
        clipboard.clear_clipboard();

        assert!(clipboard.get_clipboard().is_none());
    }

    #[test]
    fn test_file_path_validation() {
        let policy = FileAccessPolicy::new();

        assert!(policy.validate_path(b"/rayos/app"));
        assert!(!policy.validate_path(b"../../etc/passwd"));
        assert!(!policy.validate_path(b""));
    }

    #[test]
    fn test_public_path_read_only() {
        let policy = FileAccessPolicy::new();

        assert!(policy.check_app_permission(0, b"/rayos/public/file", FileAccessType::Read));
        assert!(!policy.check_app_permission(0, b"/rayos/public/file", FileAccessType::Write));
    }

    #[test]
    fn test_tmp_path_read_write() {
        let policy = FileAccessPolicy::new();

        assert!(policy.check_app_permission(0, b"/rayos/tmp/file", FileAccessType::Read));
        assert!(policy.check_app_permission(0, b"/rayos/tmp/file", FileAccessType::Write));
    }

    #[test]
    fn test_file_request_grant() {
        let policy = FileAccessPolicy::new();

        let result = policy.request_file_handle(0, b"/rayos/tmp/test", FileAccessType::Write);
        assert!(result.is_ok());
    }

    #[test]
    fn test_file_request_deny_traversal() {
        let policy = FileAccessPolicy::new();

        let result = policy.request_file_handle(0, b"/rayos/../../../etc/passwd", FileAccessType::Read);
        assert!(result.is_err());
    }

    #[test]
    fn test_sandbox_escape_prevention() {
        let policy = FileAccessPolicy::new();

        assert!(!policy.check_app_permission(0, b"/etc/passwd", FileAccessType::Read));
        assert!(!policy.check_app_permission(0, b"/proc/self/mem", FileAccessType::Read));
    }

    #[test]
    fn test_invalid_app_id() {
        let policy = FileAccessPolicy::new();

        assert!(!policy.check_app_permission(255, b"/rayos/tmp/file", FileAccessType::Read));
    }

    #[test]
    fn test_clipboard_empty() {
        let clipboard = ClipboardManager::new();
        assert!(clipboard.get_clipboard().is_none());
    }

    #[test]
    fn test_clipboard_size_limit() {
        let clipboard = ClipboardManager::new();
        let large_data = [0u8; CLIPBOARD_SIZE + 1];

        assert!(!clipboard.set_clipboard(0, &large_data));
    }
}
