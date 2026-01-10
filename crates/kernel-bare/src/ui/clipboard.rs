//! Clipboard Manager for RayOS UI
//!
//! Unified clipboard with format negotiation and VM guest bridge.
//! Supports multiple formats, history, and cross-app/VM sharing.
//!
//! # Overview
//!
//! The Clipboard Manager provides:
//! - Multiple clipboard formats (text, HTML, images, files)
//! - Clipboard history with metadata
//! - Primary selection (middle-click) and system clipboard
//! - VM guest clipboard synchronization
//! - Policy-based access control
//!
//! # Markers
//!
//! - `RAYOS_CLIPBOARD:COPIED` - Data copied to clipboard
//! - `RAYOS_CLIPBOARD:PASTED` - Data pasted from clipboard
//! - `RAYOS_CLIPBOARD:CLEARED` - Clipboard cleared
//! - `RAYOS_CLIPBOARD:SYNCED` - Clipboard synced with VM guest
//! - `RAYOS_CLIPBOARD:FORMAT` - Format negotiation occurred

use super::app_runtime::AppId;

// ============================================================================
// Constants
// ============================================================================

/// Maximum clipboard data size (16MB).
pub const MAX_CLIPBOARD_SIZE: usize = 16 * 1024 * 1024;

/// Maximum inline data size (for small entries).
pub const MAX_INLINE_SIZE: usize = 4096;

/// Maximum clipboard history entries.
pub const MAX_HISTORY_ENTRIES: usize = 10;

/// Maximum formats per clipboard entry.
pub const MAX_FORMATS_PER_ENTRY: usize = 8;

/// Maximum watchers for clipboard changes.
pub const MAX_CLIPBOARD_WATCHERS: usize = 8;

// ============================================================================
// Clipboard Format
// ============================================================================

/// Clipboard data format identifier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum ClipboardFormat {
    /// No format / empty.
    None = 0,
    /// Plain text (UTF-8).
    Text = 1,
    /// Rich text (RTF).
    RichText = 2,
    /// HTML content.
    Html = 3,
    /// PNG image data.
    ImagePng = 4,
    /// JPEG image data.
    ImageJpeg = 5,
    /// BMP image data.
    ImageBmp = 6,
    /// File path list (newline-separated).
    FilePaths = 7,
    /// URI list (newline-separated).
    UriList = 8,
    /// Raw binary data.
    Binary = 9,
    /// Custom format (application-defined).
    Custom(u16) = 10,
}

impl Default for ClipboardFormat {
    fn default() -> Self {
        ClipboardFormat::None
    }
}

impl ClipboardFormat {
    /// Check if this is a text-based format.
    pub fn is_text(&self) -> bool {
        matches!(
            self,
            ClipboardFormat::Text
                | ClipboardFormat::RichText
                | ClipboardFormat::Html
                | ClipboardFormat::FilePaths
                | ClipboardFormat::UriList
        )
    }

    /// Check if this is an image format.
    pub fn is_image(&self) -> bool {
        matches!(
            self,
            ClipboardFormat::ImagePng | ClipboardFormat::ImageJpeg | ClipboardFormat::ImageBmp
        )
    }

    /// Get MIME type string for format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            ClipboardFormat::None => "",
            ClipboardFormat::Text => "text/plain",
            ClipboardFormat::RichText => "text/rtf",
            ClipboardFormat::Html => "text/html",
            ClipboardFormat::ImagePng => "image/png",
            ClipboardFormat::ImageJpeg => "image/jpeg",
            ClipboardFormat::ImageBmp => "image/bmp",
            ClipboardFormat::FilePaths => "text/uri-list",
            ClipboardFormat::UriList => "text/uri-list",
            ClipboardFormat::Binary => "application/octet-stream",
            ClipboardFormat::Custom(_) => "application/x-rayos-custom",
        }
    }

    /// Get format from MIME type.
    pub fn from_mime(mime: &str) -> Self {
        match mime {
            "text/plain" => ClipboardFormat::Text,
            "text/rtf" => ClipboardFormat::RichText,
            "text/html" => ClipboardFormat::Html,
            "image/png" => ClipboardFormat::ImagePng,
            "image/jpeg" => ClipboardFormat::ImageJpeg,
            "image/bmp" => ClipboardFormat::ImageBmp,
            "text/uri-list" => ClipboardFormat::UriList,
            "application/octet-stream" => ClipboardFormat::Binary,
            _ => ClipboardFormat::None,
        }
    }

    /// Get numeric ID for format (for bitmask operations).
    pub fn to_id(&self) -> u16 {
        match self {
            ClipboardFormat::None => 0,
            ClipboardFormat::Text => 1,
            ClipboardFormat::RichText => 2,
            ClipboardFormat::Html => 3,
            ClipboardFormat::ImagePng => 4,
            ClipboardFormat::ImageJpeg => 5,
            ClipboardFormat::ImageBmp => 6,
            ClipboardFormat::FilePaths => 7,
            ClipboardFormat::UriList => 8,
            ClipboardFormat::Binary => 9,
            ClipboardFormat::Custom(id) => 10 + *id,
        }
    }
}

// ============================================================================
// Clipboard Data
// ============================================================================

/// Inline clipboard data (small entries).
#[derive(Clone, Copy)]
pub struct InlineData {
    /// Data bytes.
    pub bytes: [u8; MAX_INLINE_SIZE],
    /// Actual data length.
    pub len: usize,
}

impl InlineData {
    /// Create empty inline data.
    pub const fn empty() -> Self {
        Self {
            bytes: [0u8; MAX_INLINE_SIZE],
            len: 0,
        }
    }

    /// Create inline data from bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() > MAX_INLINE_SIZE {
            return None;
        }
        let mut inline = Self::empty();
        inline.bytes[..data.len()].copy_from_slice(data);
        inline.len = data.len();
        Some(inline)
    }

    /// Get data as byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Get data as string (if valid UTF-8).
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(self.as_bytes()).ok()
    }
}

/// Clipboard data storage.
#[derive(Clone, Copy)]
pub enum ClipboardData {
    /// No data.
    Empty,
    /// Inline data (small, copied directly).
    Inline(InlineData),
    /// External reference (offset + length in external buffer).
    External { offset: usize, len: usize },
}

impl Default for ClipboardData {
    fn default() -> Self {
        ClipboardData::Empty
    }
}

impl ClipboardData {
    /// Check if data is empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, ClipboardData::Empty)
    }

    /// Get data length.
    pub fn len(&self) -> usize {
        match self {
            ClipboardData::Empty => 0,
            ClipboardData::Inline(data) => data.len,
            ClipboardData::External { len, .. } => *len,
        }
    }
}

// ============================================================================
// Format Entry
// ============================================================================

/// Single format representation of clipboard content.
#[derive(Clone, Copy)]
pub struct FormatEntry {
    /// Data format.
    pub format: ClipboardFormat,
    /// Data storage.
    pub data: ClipboardData,
}

impl FormatEntry {
    /// Create an empty format entry.
    pub const fn empty() -> Self {
        Self {
            format: ClipboardFormat::None,
            data: ClipboardData::Empty,
        }
    }

    /// Create a text format entry.
    pub fn text(text: &str) -> Option<Self> {
        let inline = InlineData::from_bytes(text.as_bytes())?;
        Some(Self {
            format: ClipboardFormat::Text,
            data: ClipboardData::Inline(inline),
        })
    }

    /// Check if entry is valid.
    pub fn is_valid(&self) -> bool {
        !matches!(self.format, ClipboardFormat::None)
    }
}

// ============================================================================
// Clipboard Entry
// ============================================================================

/// Complete clipboard entry with multiple format representations.
#[derive(Clone, Copy)]
pub struct ClipboardEntry {
    /// Entry ID (unique per clipboard operation).
    pub id: u32,
    /// Available formats.
    pub formats: [FormatEntry; MAX_FORMATS_PER_ENTRY],
    /// Number of formats.
    pub format_count: usize,
    /// Source app ID (0 = system/external).
    pub owner_app: AppId,
    /// Timestamp when copied.
    pub timestamp: u64,
    /// Brief label for history display.
    pub label: [u8; 32],
}

impl ClipboardEntry {
    /// Create an empty entry.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            formats: [FormatEntry::empty(); MAX_FORMATS_PER_ENTRY],
            format_count: 0,
            owner_app: 0,
            timestamp: 0,
            label: [0u8; 32],
        }
    }

    /// Check if entry is valid.
    pub fn is_valid(&self) -> bool {
        self.format_count > 0 && self.formats[0].is_valid()
    }

    /// Add a format to this entry.
    pub fn add_format(&mut self, entry: FormatEntry) -> bool {
        if self.format_count >= MAX_FORMATS_PER_ENTRY {
            return false;
        }
        self.formats[self.format_count] = entry;
        self.format_count += 1;
        true
    }

    /// Check if entry has a specific format.
    pub fn has_format(&self, format: ClipboardFormat) -> bool {
        self.formats[..self.format_count]
            .iter()
            .any(|f| core::mem::discriminant(&f.format) == core::mem::discriminant(&format))
    }

    /// Get format entry for specific format.
    pub fn get_format(&self, format: ClipboardFormat) -> Option<&FormatEntry> {
        self.formats[..self.format_count]
            .iter()
            .find(|f| core::mem::discriminant(&f.format) == core::mem::discriminant(&format))
    }

    /// Get the best text representation.
    pub fn get_text(&self) -> Option<&str> {
        // Try formats in preference order
        for fmt in &[ClipboardFormat::Text, ClipboardFormat::Html, ClipboardFormat::RichText] {
            if let Some(entry) = self.get_format(*fmt) {
                if let ClipboardData::Inline(data) = &entry.data {
                    return data.as_str();
                }
            }
        }
        None
    }

    /// Set label from text preview.
    pub fn set_label_from_text(&mut self, text: &str) {
        let text_bytes = text.as_bytes();
        let copy_len = text_bytes.len().min(31);
        self.label = [0u8; 32];
        self.label[..copy_len].copy_from_slice(&text_bytes[..copy_len]);
    }

    /// Get label as string.
    pub fn label_str(&self) -> &str {
        let len = self.label.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.label[..len]).unwrap_or("")
    }

    /// List available formats.
    pub fn available_formats(&self) -> impl Iterator<Item = ClipboardFormat> + '_ {
        self.formats[..self.format_count].iter().map(|f| f.format)
    }
}

// ============================================================================
// Clipboard Selection
// ============================================================================

/// Clipboard selection type (X11-style).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ClipboardSelection {
    /// Primary selection (middle-click paste).
    Primary = 0,
    /// System clipboard (Ctrl+C/V).
    Clipboard = 1,
}

impl Default for ClipboardSelection {
    fn default() -> Self {
        ClipboardSelection::Clipboard
    }
}

// ============================================================================
// Clipboard Event
// ============================================================================

/// Clipboard event for watchers.
#[derive(Clone, Copy, Debug)]
pub enum ClipboardEvent {
    /// Data copied to clipboard.
    Copied {
        selection: ClipboardSelection,
        entry_id: u32,
        owner_app: AppId,
    },
    /// Data pasted from clipboard.
    Pasted {
        selection: ClipboardSelection,
        entry_id: u32,
        target_app: AppId,
    },
    /// Clipboard cleared.
    Cleared { selection: ClipboardSelection },
    /// Clipboard synced with VM.
    Synced { vm_id: u32, direction: SyncDirection },
    /// Format list changed.
    FormatsChanged { selection: ClipboardSelection },
}

/// Clipboard sync direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SyncDirection {
    /// Host to guest.
    HostToGuest = 0,
    /// Guest to host.
    GuestToHost = 1,
}

// ============================================================================
// Clipboard Watcher
// ============================================================================

/// Watcher callback type.
pub type ClipboardWatcherFn = fn(event: ClipboardEvent);

/// Clipboard watcher entry.
#[derive(Clone, Copy)]
pub struct ClipboardWatcher {
    /// Watcher ID.
    pub id: u32,
    /// App ID (0 = system watcher).
    pub app_id: AppId,
    /// Callback function.
    pub callback: Option<ClipboardWatcherFn>,
    /// Active flag.
    pub active: bool,
}

impl ClipboardWatcher {
    /// Create an empty watcher.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            app_id: 0,
            callback: None,
            active: false,
        }
    }
}

// ============================================================================
// Clipboard Policy
// ============================================================================

/// Clipboard access policy for an app.
#[derive(Clone, Copy)]
pub struct ClipboardPolicy {
    /// App ID this policy applies to.
    pub app_id: AppId,
    /// Can read from clipboard.
    pub can_read: bool,
    /// Can write to clipboard.
    pub can_write: bool,
    /// Can access primary selection.
    pub can_use_primary: bool,
    /// Can sync with VMs.
    pub can_vm_sync: bool,
    /// Max data size allowed.
    pub max_size: usize,
}

impl ClipboardPolicy {
    /// Create default (full access) policy.
    pub const fn full_access(app_id: AppId) -> Self {
        Self {
            app_id,
            can_read: true,
            can_write: true,
            can_use_primary: true,
            can_vm_sync: true,
            max_size: MAX_CLIPBOARD_SIZE,
        }
    }

    /// Create restricted (read-only) policy.
    pub const fn read_only(app_id: AppId) -> Self {
        Self {
            app_id,
            can_read: true,
            can_write: false,
            can_use_primary: false,
            can_vm_sync: false,
            max_size: MAX_CLIPBOARD_SIZE,
        }
    }

    /// Create no-access policy.
    pub const fn no_access(app_id: AppId) -> Self {
        Self {
            app_id,
            can_read: false,
            can_write: false,
            can_use_primary: false,
            can_vm_sync: false,
            max_size: 0,
        }
    }
}

// ============================================================================
// Clipboard History
// ============================================================================

/// Clipboard history ring buffer.
pub struct ClipboardHistory {
    /// History entries.
    entries: [ClipboardEntry; MAX_HISTORY_ENTRIES],
    /// Head index (most recent).
    head: usize,
    /// Number of valid entries.
    count: usize,
    /// Total entries ever added.
    total_added: u64,
}

impl ClipboardHistory {
    /// Create empty history.
    pub const fn new() -> Self {
        Self {
            entries: [ClipboardEntry::empty(); MAX_HISTORY_ENTRIES],
            head: 0,
            count: 0,
            total_added: 0,
        }
    }

    /// Add entry to history.
    pub fn push(&mut self, entry: ClipboardEntry) {
        self.head = (self.head + MAX_HISTORY_ENTRIES - 1) % MAX_HISTORY_ENTRIES;
        self.entries[self.head] = entry;
        if self.count < MAX_HISTORY_ENTRIES {
            self.count += 1;
        }
        self.total_added += 1;
    }

    /// Get most recent entry.
    pub fn current(&self) -> Option<&ClipboardEntry> {
        if self.count > 0 {
            Some(&self.entries[self.head])
        } else {
            None
        }
    }

    /// Get entry at history index (0 = most recent).
    pub fn get(&self, index: usize) -> Option<&ClipboardEntry> {
        if index >= self.count {
            return None;
        }
        let idx = (self.head + index) % MAX_HISTORY_ENTRIES;
        Some(&self.entries[idx])
    }

    /// Get history length.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if history is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Clear history.
    pub fn clear(&mut self) {
        for entry in &mut self.entries {
            *entry = ClipboardEntry::empty();
        }
        self.count = 0;
    }

    /// Get total entries ever added.
    pub fn total_added(&self) -> u64 {
        self.total_added
    }
}

// ============================================================================
// Clipboard Manager
// ============================================================================

/// Main clipboard manager.
pub struct ClipboardManager {
    /// Primary selection content.
    primary: ClipboardEntry,
    /// System clipboard content.
    clipboard: ClipboardEntry,
    /// Clipboard history.
    history: ClipboardHistory,
    /// Registered watchers.
    watchers: [ClipboardWatcher; MAX_CLIPBOARD_WATCHERS],
    /// Number of active watchers.
    watcher_count: usize,
    /// Next entry ID.
    next_entry_id: u32,
    /// Next watcher ID.
    next_watcher_id: u32,
    /// Current timestamp.
    timestamp: u64,
    /// Statistics: total copies.
    stats_copies: u64,
    /// Statistics: total pastes.
    stats_pastes: u64,
}

impl ClipboardManager {
    /// Create a new clipboard manager.
    pub const fn new() -> Self {
        Self {
            primary: ClipboardEntry::empty(),
            clipboard: ClipboardEntry::empty(),
            history: ClipboardHistory::new(),
            watchers: [ClipboardWatcher::empty(); MAX_CLIPBOARD_WATCHERS],
            watcher_count: 0,
            next_entry_id: 1,
            next_watcher_id: 1,
            timestamp: 0,
            stats_copies: 0,
            stats_pastes: 0,
        }
    }

    /// Set clipboard content.
    pub fn set(
        &mut self,
        selection: ClipboardSelection,
        entry: ClipboardEntry,
        policy: Option<&ClipboardPolicy>,
    ) -> Result<u32, ClipboardError> {
        // Check policy
        if let Some(p) = policy {
            if !p.can_write {
                return Err(ClipboardError::AccessDenied);
            }
            if selection == ClipboardSelection::Primary && !p.can_use_primary {
                return Err(ClipboardError::AccessDenied);
            }
            // Check size
            let total_size: usize = entry.formats[..entry.format_count]
                .iter()
                .map(|f| f.data.len())
                .sum();
            if total_size > p.max_size {
                return Err(ClipboardError::DataTooLarge);
            }
        }

        // Assign ID and timestamp
        let mut entry = entry;
        entry.id = self.next_entry_id;
        self.next_entry_id += 1;
        entry.timestamp = self.timestamp;

        // Store
        match selection {
            ClipboardSelection::Primary => {
                self.primary = entry;
            }
            ClipboardSelection::Clipboard => {
                self.clipboard = entry;
                self.history.push(entry);
            }
        }

        self.stats_copies += 1;
        // RAYOS_CLIPBOARD:COPIED

        // Notify watchers
        let event = ClipboardEvent::Copied {
            selection,
            entry_id: entry.id,
            owner_app: entry.owner_app,
        };
        self.notify_watchers(event);

        Ok(entry.id)
    }

    /// Set text content (convenience method).
    pub fn set_text(
        &mut self,
        selection: ClipboardSelection,
        text: &str,
        owner_app: AppId,
    ) -> Result<u32, ClipboardError> {
        let format_entry = FormatEntry::text(text).ok_or(ClipboardError::DataTooLarge)?;

        let mut entry = ClipboardEntry::empty();
        entry.owner_app = owner_app;
        entry.add_format(format_entry);
        entry.set_label_from_text(text);

        self.set(selection, entry, None)
    }

    /// Get clipboard content.
    pub fn get(
        &mut self,
        selection: ClipboardSelection,
        target_app: AppId,
        policy: Option<&ClipboardPolicy>,
    ) -> Result<&ClipboardEntry, ClipboardError> {
        // Check policy
        if let Some(p) = policy {
            if !p.can_read {
                return Err(ClipboardError::AccessDenied);
            }
            if selection == ClipboardSelection::Primary && !p.can_use_primary {
                return Err(ClipboardError::AccessDenied);
            }
        }

        let entry = match selection {
            ClipboardSelection::Primary => &self.primary,
            ClipboardSelection::Clipboard => &self.clipboard,
        };

        if !entry.is_valid() {
            return Err(ClipboardError::Empty);
        }

        self.stats_pastes += 1;
        // RAYOS_CLIPBOARD:PASTED

        // Notify watchers
        let event = ClipboardEvent::Pasted {
            selection,
            entry_id: entry.id,
            target_app,
        };
        self.notify_watchers(event);

        Ok(entry)
    }

    /// Get text content (convenience method).
    pub fn get_text(
        &mut self,
        selection: ClipboardSelection,
        target_app: AppId,
    ) -> Result<&str, ClipboardError> {
        let entry = self.get(selection, target_app, None)?;
        entry.get_text().ok_or(ClipboardError::FormatNotAvailable)
    }

    /// Clear clipboard.
    pub fn clear(&mut self, selection: ClipboardSelection) {
        match selection {
            ClipboardSelection::Primary => {
                self.primary = ClipboardEntry::empty();
            }
            ClipboardSelection::Clipboard => {
                self.clipboard = ClipboardEntry::empty();
            }
        }
        // RAYOS_CLIPBOARD:CLEARED

        let event = ClipboardEvent::Cleared { selection };
        self.notify_watchers(event);
    }

    /// Get clipboard history.
    pub fn history(&self) -> &ClipboardHistory {
        &self.history
    }

    /// Get entry from history by index.
    pub fn history_entry(&self, index: usize) -> Option<&ClipboardEntry> {
        self.history.get(index)
    }

    /// Restore entry from history to clipboard.
    pub fn restore_from_history(&mut self, index: usize) -> Result<u32, ClipboardError> {
        let entry = self.history.get(index).ok_or(ClipboardError::NotFound)?;
        let mut entry = *entry;
        entry.id = self.next_entry_id;
        self.next_entry_id += 1;
        entry.timestamp = self.timestamp;

        self.clipboard = entry;
        self.stats_copies += 1;

        Ok(entry.id)
    }

    /// Check if format is available.
    pub fn has_format(&self, selection: ClipboardSelection, format: ClipboardFormat) -> bool {
        let entry = match selection {
            ClipboardSelection::Primary => &self.primary,
            ClipboardSelection::Clipboard => &self.clipboard,
        };
        entry.has_format(format)
    }

    /// List available formats.
    pub fn available_formats(&self, selection: ClipboardSelection) -> impl Iterator<Item = ClipboardFormat> + '_ {
        let entry = match selection {
            ClipboardSelection::Primary => &self.primary,
            ClipboardSelection::Clipboard => &self.clipboard,
        };
        entry.available_formats()
    }

    /// Register a clipboard watcher.
    pub fn add_watcher(&mut self, app_id: AppId, callback: ClipboardWatcherFn) -> Option<u32> {
        if self.watcher_count >= MAX_CLIPBOARD_WATCHERS {
            return None;
        }

        let id = self.next_watcher_id;
        self.next_watcher_id += 1;

        self.watchers[self.watcher_count] = ClipboardWatcher {
            id,
            app_id,
            callback: Some(callback),
            active: true,
        };
        self.watcher_count += 1;

        Some(id)
    }

    /// Remove a watcher.
    pub fn remove_watcher(&mut self, watcher_id: u32) -> bool {
        for i in 0..self.watcher_count {
            if self.watchers[i].id == watcher_id {
                // Shift remaining watchers
                for j in i..self.watcher_count - 1 {
                    self.watchers[j] = self.watchers[j + 1];
                }
                self.watchers[self.watcher_count - 1] = ClipboardWatcher::empty();
                self.watcher_count -= 1;
                return true;
            }
        }
        false
    }

    /// Notify all watchers of an event.
    fn notify_watchers(&self, event: ClipboardEvent) {
        for watcher in &self.watchers[..self.watcher_count] {
            if watcher.active {
                if let Some(callback) = watcher.callback {
                    callback(event);
                }
            }
        }
    }

    /// Tick the clipboard manager (update timestamp).
    pub fn tick(&mut self) {
        self.timestamp += 1;
    }

    /// Get statistics.
    pub fn stats(&self) -> (u64, u64, usize) {
        (self.stats_copies, self.stats_pastes, self.history.len())
    }
}

// ============================================================================
// Clipboard Error
// ============================================================================

/// Clipboard operation errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    /// Clipboard is empty.
    Empty,
    /// Requested format not available.
    FormatNotAvailable,
    /// Access denied by policy.
    AccessDenied,
    /// Data too large.
    DataTooLarge,
    /// Entry not found.
    NotFound,
    /// Internal error.
    Internal,
}

// ============================================================================
// VM Clipboard Bridge
// ============================================================================

/// Virtio clipboard device state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum VirtioClipboardState {
    /// Not initialized.
    Uninitialized = 0,
    /// Ready for operations.
    Ready = 1,
    /// Syncing in progress.
    Syncing = 2,
    /// Error state.
    Error = 3,
}

impl Default for VirtioClipboardState {
    fn default() -> Self {
        VirtioClipboardState::Uninitialized
    }
}

/// Virtio clipboard request type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum VirtioClipboardRequest {
    /// Get format list.
    GetFormats = 1,
    /// Set format list.
    SetFormats = 2,
    /// Get data for format.
    GetData = 3,
    /// Set data for format.
    SetData = 4,
}

/// Clipboard bridge for VM guest communication.
pub struct ClipboardBridge {
    /// VM ID this bridge is for.
    vm_id: u32,
    /// Bridge state.
    state: VirtioClipboardState,
    /// Last sync timestamp.
    last_sync: u64,
    /// Sync pending flag.
    sync_pending: bool,
    /// Last error code.
    last_error: u32,
    /// Statistics: host to guest syncs.
    stats_h2g: u64,
    /// Statistics: guest to host syncs.
    stats_g2h: u64,
}

impl ClipboardBridge {
    /// Create a new bridge for a VM.
    pub const fn new(vm_id: u32) -> Self {
        Self {
            vm_id,
            state: VirtioClipboardState::Uninitialized,
            last_sync: 0,
            sync_pending: false,
            last_error: 0,
            stats_h2g: 0,
            stats_g2h: 0,
        }
    }

    /// Initialize the bridge.
    pub fn init(&mut self) {
        self.state = VirtioClipboardState::Ready;
    }

    /// Get bridge state.
    pub fn state(&self) -> VirtioClipboardState {
        self.state
    }

    /// Check if bridge is ready.
    pub fn is_ready(&self) -> bool {
        self.state == VirtioClipboardState::Ready
    }

    /// Request sync from host to guest.
    pub fn sync_to_guest(&mut self, _entry: &ClipboardEntry) -> Result<(), ClipboardError> {
        if !self.is_ready() {
            return Err(ClipboardError::Internal);
        }

        self.state = VirtioClipboardState::Syncing;
        self.sync_pending = true;

        // In real implementation, this would:
        // 1. Serialize the entry to virtio format
        // 2. Queue a descriptor to the guest
        // 3. Wait for guest acknowledgment

        self.state = VirtioClipboardState::Ready;
        self.sync_pending = false;
        self.stats_h2g += 1;
        // RAYOS_CLIPBOARD:SYNCED

        Ok(())
    }

    /// Handle sync from guest to host.
    pub fn sync_from_guest(&mut self, manager: &mut ClipboardManager) -> Result<u32, ClipboardError> {
        if !self.is_ready() {
            return Err(ClipboardError::Internal);
        }

        self.state = VirtioClipboardState::Syncing;

        // In real implementation, this would:
        // 1. Read data from virtio queue
        // 2. Deserialize to ClipboardEntry
        // 3. Set in clipboard manager

        // Placeholder: create empty entry
        let entry = ClipboardEntry::empty();
        let id = manager.set(ClipboardSelection::Clipboard, entry, None)?;

        self.state = VirtioClipboardState::Ready;
        self.stats_g2h += 1;
        // RAYOS_CLIPBOARD:SYNCED

        Ok(id)
    }

    /// Get VM ID.
    pub fn vm_id(&self) -> u32 {
        self.vm_id
    }

    /// Get statistics.
    pub fn stats(&self) -> (u64, u64) {
        (self.stats_h2g, self.stats_g2h)
    }

    /// Reset the bridge.
    pub fn reset(&mut self) {
        self.state = VirtioClipboardState::Uninitialized;
        self.sync_pending = false;
        self.last_error = 0;
    }
}

// ============================================================================
// Global Clipboard Manager
// ============================================================================

/// Global clipboard manager instance.
static mut GLOBAL_CLIPBOARD: ClipboardManager = ClipboardManager::new();

/// Get the global clipboard manager.
pub fn clipboard() -> &'static ClipboardManager {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_CLIPBOARD }
}

/// Get the global clipboard manager mutably.
pub fn clipboard_mut() -> &'static mut ClipboardManager {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_CLIPBOARD }
}

/// Copy text to clipboard.
pub fn copy_text(text: &str, owner_app: AppId) -> Result<u32, ClipboardError> {
    clipboard_mut().set_text(ClipboardSelection::Clipboard, text, owner_app)
}

/// Paste text from clipboard.
pub fn paste_text(target_app: AppId) -> Result<&'static str, ClipboardError> {
    clipboard_mut().get_text(ClipboardSelection::Clipboard, target_app)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_format() {
        assert!(ClipboardFormat::Text.is_text());
        assert!(ClipboardFormat::Html.is_text());
        assert!(!ClipboardFormat::ImagePng.is_text());
        assert!(ClipboardFormat::ImagePng.is_image());
    }

    #[test]
    fn test_inline_data() {
        let data = InlineData::from_bytes(b"hello").unwrap();
        assert_eq!(data.as_bytes(), b"hello");
        assert_eq!(data.as_str(), Some("hello"));
    }

    #[test]
    fn test_clipboard_entry() {
        let mut entry = ClipboardEntry::empty();
        assert!(!entry.is_valid());

        let format = FormatEntry::text("test").unwrap();
        entry.add_format(format);

        assert!(entry.is_valid());
        assert!(entry.has_format(ClipboardFormat::Text));
        assert_eq!(entry.get_text(), Some("test"));
    }

    #[test]
    fn test_clipboard_history() {
        let mut history = ClipboardHistory::new();
        assert!(history.is_empty());

        let mut entry = ClipboardEntry::empty();
        entry.id = 1;
        history.push(entry);

        assert_eq!(history.len(), 1);
        assert_eq!(history.current().unwrap().id, 1);
    }

    #[test]
    fn test_clipboard_manager_set_get() {
        let mut manager = ClipboardManager::new();

        let id = manager.set_text(ClipboardSelection::Clipboard, "hello", 1).unwrap();
        assert!(id > 0);

        let text = manager.get_text(ClipboardSelection::Clipboard, 2).unwrap();
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_clipboard_policy() {
        let policy = ClipboardPolicy::read_only(1);
        assert!(policy.can_read);
        assert!(!policy.can_write);

        let policy = ClipboardPolicy::no_access(1);
        assert!(!policy.can_read);
        assert!(!policy.can_write);
    }

    #[test]
    fn test_clipboard_bridge() {
        let mut bridge = ClipboardBridge::new(1);
        assert_eq!(bridge.state(), VirtioClipboardState::Uninitialized);

        bridge.init();
        assert!(bridge.is_ready());
    }

    #[test]
    fn test_format_mime_types() {
        assert_eq!(ClipboardFormat::Text.mime_type(), "text/plain");
        assert_eq!(ClipboardFormat::Html.mime_type(), "text/html");
        assert_eq!(ClipboardFormat::ImagePng.mime_type(), "image/png");

        assert_eq!(ClipboardFormat::from_mime("text/plain"), ClipboardFormat::Text);
    }

    #[test]
    fn test_clipboard_watcher() {
        let mut manager = ClipboardManager::new();

        fn dummy_callback(_: ClipboardEvent) {}

        let id = manager.add_watcher(1, dummy_callback);
        assert!(id.is_some());

        assert!(manager.remove_watcher(id.unwrap()));
    }
}
