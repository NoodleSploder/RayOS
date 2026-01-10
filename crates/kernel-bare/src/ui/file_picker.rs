//! File Picker Dialog for RayOS UI
//!
//! Native file picker with directory navigation and filtering.
//!
//! # Overview
//!
//! The File Picker provides:
//! - Open file / Save file dialogs
//! - Directory browser with navigation
//! - File type filtering
//! - Multiple selection support
//! - Recent files and bookmarks
//!
//! # Markers
//!
//! - `RAYOS_FILEPICKER:OPENED` - Picker dialog opened
//! - `RAYOS_FILEPICKER:SELECTED` - File(s) selected
//! - `RAYOS_FILEPICKER:CANCELLED` - Picker cancelled
//! - `RAYOS_FILEPICKER:NAVIGATED` - Directory changed
//! - `RAYOS_FILEPICKER:CREATED` - New file/folder created

use super::app_runtime::AppId;
use super::window_manager::WindowId;

// ============================================================================
// Constants
// ============================================================================

/// Maximum path length.
pub const MAX_PATH_LEN: usize = 256;

/// Maximum filename length.
pub const MAX_FILENAME_LEN: usize = 64;

/// Maximum entries per directory.
pub const MAX_DIR_ENTRIES: usize = 128;

/// Maximum selected files.
pub const MAX_SELECTED_FILES: usize = 32;

/// Maximum filters.
pub const MAX_FILTERS: usize = 8;

/// Maximum bookmarks.
pub const MAX_BOOKMARKS: usize = 16;

/// Maximum recent files.
pub const MAX_RECENT_FILES: usize = 16;

/// Maximum active pickers.
pub const MAX_ACTIVE_PICKERS: usize = 4;

// ============================================================================
// Picker Mode
// ============================================================================

/// File picker mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PickerMode {
    /// Open single file.
    OpenFile = 0,
    /// Open multiple files.
    OpenMultiple = 1,
    /// Save file.
    SaveFile = 2,
    /// Select folder.
    SelectFolder = 3,
}

impl Default for PickerMode {
    fn default() -> Self {
        PickerMode::OpenFile
    }
}

// ============================================================================
// File Entry Type
// ============================================================================

/// Type of file entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum FileEntryType {
    /// Regular file.
    File = 0,
    /// Directory.
    Directory = 1,
    /// Symbolic link.
    Symlink = 2,
    /// Parent directory (..).
    ParentDir = 3,
    /// Device file.
    Device = 4,
}

impl Default for FileEntryType {
    fn default() -> Self {
        FileEntryType::File
    }
}

impl FileEntryType {
    /// Check if this is a directory-like entry.
    pub fn is_dir(&self) -> bool {
        matches!(self, FileEntryType::Directory | FileEntryType::ParentDir)
    }
}

// ============================================================================
// File Entry
// ============================================================================

/// Single file/directory entry.
#[derive(Clone, Copy)]
pub struct FileEntry {
    /// Entry name.
    pub name: [u8; MAX_FILENAME_LEN],
    /// Name length.
    pub name_len: usize,
    /// Entry type.
    pub entry_type: FileEntryType,
    /// File size (0 for directories).
    pub size: u64,
    /// Last modified timestamp.
    pub modified: u64,
    /// Is hidden file.
    pub hidden: bool,
    /// Is read-only.
    pub read_only: bool,
    /// Is selected.
    pub selected: bool,
}

impl FileEntry {
    /// Create empty entry.
    pub const fn empty() -> Self {
        Self {
            name: [0u8; MAX_FILENAME_LEN],
            name_len: 0,
            entry_type: FileEntryType::File,
            size: 0,
            modified: 0,
            hidden: false,
            read_only: false,
            selected: false,
        }
    }

    /// Create from name and type.
    pub fn new(name: &str, entry_type: FileEntryType) -> Self {
        let mut entry = Self::empty();
        entry.set_name(name);
        entry.entry_type = entry_type;
        entry
    }

    /// Create parent directory entry.
    pub fn parent_dir() -> Self {
        let mut entry = Self::empty();
        entry.set_name("..");
        entry.entry_type = FileEntryType::ParentDir;
        entry
    }

    /// Set name from string.
    pub fn set_name(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = bytes.len().min(MAX_FILENAME_LEN - 1);
        self.name = [0u8; MAX_FILENAME_LEN];
        self.name[..len].copy_from_slice(&bytes[..len]);
        self.name_len = len;
    }

    /// Get name as string.
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }

    /// Check if entry matches filter extension.
    pub fn matches_filter(&self, filter: &FileFilter) -> bool {
        if self.entry_type.is_dir() {
            return true; // Directories always shown
        }
        if filter.is_all_files() {
            return true;
        }
        // Check extensions
        let name = self.name();
        for i in 0..filter.extension_count {
            let ext = filter.extension(i);
            if name.ends_with(ext) {
                return true;
            }
        }
        false
    }

    /// Get file extension.
    pub fn extension(&self) -> Option<&str> {
        let name = self.name();
        name.rfind('.').map(|idx| &name[idx..])
    }

    /// Format size as human-readable string.
    pub fn size_str(&self) -> ([u8; 16], usize) {
        let mut buf = [0u8; 16];
        let len = if self.size < 1024 {
            format_number(self.size, &mut buf, " B")
        } else if self.size < 1024 * 1024 {
            format_number(self.size / 1024, &mut buf, " KB")
        } else if self.size < 1024 * 1024 * 1024 {
            format_number(self.size / (1024 * 1024), &mut buf, " MB")
        } else {
            format_number(self.size / (1024 * 1024 * 1024), &mut buf, " GB")
        };
        (buf, len)
    }
}

/// Format a number with suffix into buffer.
fn format_number(n: u64, buf: &mut [u8; 16], suffix: &str) -> usize {
    let mut digits = [0u8; 10];
    let mut n = n;
    let mut digit_count = 0;

    if n == 0 {
        digits[0] = b'0';
        digit_count = 1;
    } else {
        while n > 0 {
            digits[digit_count] = b'0' + (n % 10) as u8;
            n /= 10;
            digit_count += 1;
        }
    }

    // Reverse digits into buf
    let mut pos = 0;
    for i in (0..digit_count).rev() {
        buf[pos] = digits[i];
        pos += 1;
    }

    // Add suffix
    let suffix_bytes = suffix.as_bytes();
    let suffix_len = suffix_bytes.len().min(16 - pos);
    buf[pos..pos + suffix_len].copy_from_slice(&suffix_bytes[..suffix_len]);
    pos + suffix_len
}

// ============================================================================
// File Filter
// ============================================================================

/// File type filter.
#[derive(Clone, Copy)]
pub struct FileFilter {
    /// Filter name (e.g., "Image Files").
    pub name: [u8; 32],
    /// Name length.
    pub name_len: usize,
    /// Extensions (e.g., ".png", ".jpg").
    pub extensions: [[u8; 8]; 8],
    /// Extension lengths.
    pub extension_lens: [usize; 8],
    /// Number of extensions.
    pub extension_count: usize,
}

impl FileFilter {
    /// Create empty filter.
    pub const fn empty() -> Self {
        Self {
            name: [0u8; 32],
            name_len: 0,
            extensions: [[0u8; 8]; 8],
            extension_lens: [0; 8],
            extension_count: 0,
        }
    }

    /// Create "All Files" filter.
    pub fn all_files() -> Self {
        let mut filter = Self::empty();
        filter.set_name("All Files");
        filter.add_extension("*");
        filter
    }

    /// Create filter with name.
    pub fn new(name: &str) -> Self {
        let mut filter = Self::empty();
        filter.set_name(name);
        filter
    }

    /// Set filter name.
    pub fn set_name(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = bytes.len().min(31);
        self.name = [0u8; 32];
        self.name[..len].copy_from_slice(&bytes[..len]);
        self.name_len = len;
    }

    /// Get filter name.
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }

    /// Add extension.
    pub fn add_extension(&mut self, ext: &str) -> bool {
        if self.extension_count >= 8 {
            return false;
        }
        let bytes = ext.as_bytes();
        let len = bytes.len().min(7);
        self.extensions[self.extension_count][..len].copy_from_slice(&bytes[..len]);
        self.extension_lens[self.extension_count] = len;
        self.extension_count += 1;
        true
    }

    /// Get extension at index.
    pub fn extension(&self, index: usize) -> &str {
        if index >= self.extension_count {
            return "";
        }
        core::str::from_utf8(&self.extensions[index][..self.extension_lens[index]]).unwrap_or("")
    }

    /// Check if this is an "all files" filter.
    pub fn is_all_files(&self) -> bool {
        self.extension_count == 1 && self.extension(0) == "*"
    }
}

// ============================================================================
// Path Buffer
// ============================================================================

/// Path buffer for current directory.
#[derive(Clone, Copy)]
pub struct PathBuffer {
    /// Path bytes.
    pub bytes: [u8; MAX_PATH_LEN],
    /// Path length.
    pub len: usize,
}

impl PathBuffer {
    /// Create empty path.
    pub const fn empty() -> Self {
        Self {
            bytes: [0u8; MAX_PATH_LEN],
            len: 0,
        }
    }

    /// Create from string.
    pub fn from_str(path: &str) -> Self {
        let mut buf = Self::empty();
        buf.set(path);
        buf
    }

    /// Create root path.
    pub fn root() -> Self {
        let mut buf = Self::empty();
        buf.bytes[0] = b'/';
        buf.len = 1;
        buf
    }

    /// Set path.
    pub fn set(&mut self, path: &str) {
        let bytes = path.as_bytes();
        let len = bytes.len().min(MAX_PATH_LEN - 1);
        self.bytes = [0u8; MAX_PATH_LEN];
        self.bytes[..len].copy_from_slice(&bytes[..len]);
        self.len = len;
    }

    /// Get path as string.
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }

    /// Append path component.
    pub fn push(&mut self, component: &str) -> bool {
        let component = component.as_bytes();
        let needed = if self.len > 0 && self.bytes[self.len - 1] != b'/' {
            1 + component.len()
        } else {
            component.len()
        };

        if self.len + needed >= MAX_PATH_LEN {
            return false;
        }

        if self.len > 0 && self.bytes[self.len - 1] != b'/' {
            self.bytes[self.len] = b'/';
            self.len += 1;
        }

        self.bytes[self.len..self.len + component.len()].copy_from_slice(component);
        self.len += component.len();
        true
    }

    /// Go to parent directory.
    pub fn pop(&mut self) -> bool {
        if self.len <= 1 {
            return false;
        }

        // Find last separator
        let mut last_sep = 0;
        for i in (0..self.len - 1).rev() {
            if self.bytes[i] == b'/' {
                last_sep = i;
                break;
            }
        }

        if last_sep == 0 {
            self.len = 1; // Keep root /
        } else {
            self.len = last_sep;
        }
        true
    }

    /// Get last component (filename).
    pub fn filename(&self) -> &str {
        let path = self.as_str();
        path.rfind('/').map(|i| &path[i + 1..]).unwrap_or(path)
    }

    /// Check if path is root.
    pub fn is_root(&self) -> bool {
        self.len == 1 && self.bytes[0] == b'/'
    }
}

// ============================================================================
// Directory View
// ============================================================================

/// Directory view with entries and sorting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SortMode {
    /// Sort by name (A-Z).
    NameAsc = 0,
    /// Sort by name (Z-A).
    NameDesc = 1,
    /// Sort by size (small to large).
    SizeAsc = 2,
    /// Sort by size (large to small).
    SizeDesc = 3,
    /// Sort by modified (oldest first).
    ModifiedAsc = 4,
    /// Sort by modified (newest first).
    ModifiedDesc = 5,
}

impl Default for SortMode {
    fn default() -> Self {
        SortMode::NameAsc
    }
}

/// View mode for directory listing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ViewMode {
    /// List view with details.
    List = 0,
    /// Icon grid view.
    Grid = 1,
    /// Compact list.
    Compact = 2,
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::List
    }
}

/// Directory view state.
pub struct DirectoryView {
    /// Current path.
    pub path: PathBuffer,
    /// Directory entries.
    pub entries: [FileEntry; MAX_DIR_ENTRIES],
    /// Number of entries.
    pub entry_count: usize,
    /// Filtered entry indices.
    pub filtered: [u16; MAX_DIR_ENTRIES],
    /// Number of filtered entries.
    pub filtered_count: usize,
    /// Current sort mode.
    pub sort_mode: SortMode,
    /// View mode.
    pub view_mode: ViewMode,
    /// Show hidden files.
    pub show_hidden: bool,
    /// Scroll offset.
    pub scroll_offset: usize,
    /// Selected index in filtered list.
    pub selected_index: Option<usize>,
    /// Loading flag.
    pub loading: bool,
    /// Error message.
    pub error: [u8; 64],
    /// Error length.
    pub error_len: usize,
}

impl DirectoryView {
    /// Create empty view.
    pub const fn new() -> Self {
        Self {
            path: PathBuffer::empty(),
            entries: [FileEntry::empty(); MAX_DIR_ENTRIES],
            entry_count: 0,
            filtered: [0u16; MAX_DIR_ENTRIES],
            filtered_count: 0,
            sort_mode: SortMode::NameAsc,
            view_mode: ViewMode::List,
            show_hidden: false,
            scroll_offset: 0,
            selected_index: None,
            loading: false,
            error: [0u8; 64],
            error_len: 0,
        }
    }

    /// Set current path.
    pub fn set_path(&mut self, path: &str) {
        self.path.set(path);
        self.clear_entries();
    }

    /// Clear entries.
    pub fn clear_entries(&mut self) {
        for entry in &mut self.entries {
            *entry = FileEntry::empty();
        }
        self.entry_count = 0;
        self.filtered_count = 0;
        self.selected_index = None;
        self.scroll_offset = 0;
    }

    /// Add entry.
    pub fn add_entry(&mut self, entry: FileEntry) -> bool {
        if self.entry_count >= MAX_DIR_ENTRIES {
            return false;
        }
        self.entries[self.entry_count] = entry;
        self.entry_count += 1;
        true
    }

    /// Apply filter.
    pub fn apply_filter(&mut self, filter: &FileFilter) {
        self.filtered_count = 0;
        for i in 0..self.entry_count {
            let entry = &self.entries[i];
            // Skip hidden if not showing
            if entry.hidden && !self.show_hidden {
                continue;
            }
            // Apply filter
            if entry.matches_filter(filter) {
                self.filtered[self.filtered_count] = i as u16;
                self.filtered_count += 1;
            }
        }
    }

    /// Sort entries (directories first, then by sort mode).
    pub fn sort(&mut self) {
        // Simple bubble sort for no_std
        for i in 0..self.filtered_count.saturating_sub(1) {
            for j in 0..self.filtered_count - i - 1 {
                let idx_a = self.filtered[j] as usize;
                let idx_b = self.filtered[j + 1] as usize;
                let a = &self.entries[idx_a];
                let b = &self.entries[idx_b];

                let should_swap = match (a.entry_type.is_dir(), b.entry_type.is_dir()) {
                    // Parent dir always first
                    (true, true) => {
                        if matches!(a.entry_type, FileEntryType::ParentDir) {
                            false
                        } else if matches!(b.entry_type, FileEntryType::ParentDir) {
                            true
                        } else {
                            self.compare_entries(a, b)
                        }
                    }
                    // Directories before files
                    (true, false) => false,
                    (false, true) => true,
                    // Both files - compare by sort mode
                    (false, false) => self.compare_entries(a, b),
                };

                if should_swap {
                    self.filtered.swap(j, j + 1);
                }
            }
        }
    }

    /// Compare two entries by current sort mode.
    fn compare_entries(&self, a: &FileEntry, b: &FileEntry) -> bool {
        match self.sort_mode {
            SortMode::NameAsc => a.name() > b.name(),
            SortMode::NameDesc => a.name() < b.name(),
            SortMode::SizeAsc => a.size > b.size,
            SortMode::SizeDesc => a.size < b.size,
            SortMode::ModifiedAsc => a.modified > b.modified,
            SortMode::ModifiedDesc => a.modified < b.modified,
        }
    }

    /// Get entry at filtered index.
    pub fn get_filtered(&self, index: usize) -> Option<&FileEntry> {
        if index >= self.filtered_count {
            return None;
        }
        let real_index = self.filtered[index] as usize;
        Some(&self.entries[real_index])
    }

    /// Get selected entry.
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.selected_index.and_then(|i| self.get_filtered(i))
    }

    /// Navigate into directory.
    pub fn navigate(&mut self, name: &str) -> bool {
        if name == ".." {
            self.path.pop()
        } else {
            self.path.push(name)
        }
    }

    /// Set error message.
    pub fn set_error(&mut self, msg: &str) {
        let bytes = msg.as_bytes();
        let len = bytes.len().min(63);
        self.error[..len].copy_from_slice(&bytes[..len]);
        self.error_len = len;
    }

    /// Clear error.
    pub fn clear_error(&mut self) {
        self.error_len = 0;
    }

    /// Get error message.
    pub fn error_msg(&self) -> Option<&str> {
        if self.error_len > 0 {
            core::str::from_utf8(&self.error[..self.error_len]).ok()
        } else {
            None
        }
    }
}

// ============================================================================
// Picker Result
// ============================================================================

/// File picker result.
#[derive(Clone, Copy)]
pub struct PickerResult {
    /// Picker ID.
    pub picker_id: u32,
    /// Was cancelled.
    pub cancelled: bool,
    /// Selected paths.
    pub paths: [PathBuffer; MAX_SELECTED_FILES],
    /// Number of selected paths.
    pub path_count: usize,
}

impl PickerResult {
    /// Create empty result.
    pub const fn empty() -> Self {
        Self {
            picker_id: 0,
            cancelled: false,
            paths: [PathBuffer::empty(); MAX_SELECTED_FILES],
            path_count: 0,
        }
    }

    /// Create cancelled result.
    pub fn cancelled(picker_id: u32) -> Self {
        Self {
            picker_id,
            cancelled: true,
            paths: [PathBuffer::empty(); MAX_SELECTED_FILES],
            path_count: 0,
        }
    }

    /// Add path to result.
    pub fn add_path(&mut self, path: &str) -> bool {
        if self.path_count >= MAX_SELECTED_FILES {
            return false;
        }
        self.paths[self.path_count].set(path);
        self.path_count += 1;
        true
    }

    /// Get first selected path.
    pub fn first_path(&self) -> Option<&str> {
        if self.path_count > 0 {
            Some(self.paths[0].as_str())
        } else {
            None
        }
    }

    /// Check if selection is empty.
    pub fn is_empty(&self) -> bool {
        self.path_count == 0 || self.cancelled
    }
}

// ============================================================================
// Picker State
// ============================================================================

/// Current picker state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PickerState {
    /// Not active.
    Inactive = 0,
    /// Showing dialog.
    Active = 1,
    /// Completed successfully.
    Completed = 2,
    /// Cancelled.
    Cancelled = 3,
}

impl Default for PickerState {
    fn default() -> Self {
        PickerState::Inactive
    }
}

// ============================================================================
// Picker Callback
// ============================================================================

/// Picker completion callback.
pub type PickerCallback = fn(result: &PickerResult);

// ============================================================================
// Bookmark
// ============================================================================

/// Bookmark entry.
#[derive(Clone, Copy)]
pub struct Bookmark {
    /// Bookmark name.
    pub name: [u8; 32],
    /// Name length.
    pub name_len: usize,
    /// Path.
    pub path: PathBuffer,
    /// Active flag.
    pub active: bool,
}

impl Bookmark {
    /// Create empty bookmark.
    pub const fn empty() -> Self {
        Self {
            name: [0u8; 32],
            name_len: 0,
            path: PathBuffer::empty(),
            active: false,
        }
    }

    /// Create bookmark.
    pub fn new(name: &str, path: &str) -> Self {
        let mut bm = Self::empty();
        let bytes = name.as_bytes();
        let len = bytes.len().min(31);
        bm.name[..len].copy_from_slice(&bytes[..len]);
        bm.name_len = len;
        bm.path.set(path);
        bm.active = true;
        bm
    }

    /// Get name.
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }
}

// ============================================================================
// File Picker
// ============================================================================

/// File picker dialog state.
pub struct FilePicker {
    /// Picker ID.
    pub id: u32,
    /// Picker state.
    pub state: PickerState,
    /// Picker mode.
    pub mode: PickerMode,
    /// Owning app.
    pub app_id: AppId,
    /// Dialog window ID.
    pub window_id: WindowId,
    /// Directory view.
    pub view: DirectoryView,
    /// Filters.
    pub filters: [FileFilter; MAX_FILTERS],
    /// Number of filters.
    pub filter_count: usize,
    /// Current filter index.
    pub current_filter: usize,
    /// Bookmarks.
    pub bookmarks: [Bookmark; MAX_BOOKMARKS],
    /// Bookmark count.
    pub bookmark_count: usize,
    /// Filename input (for save).
    pub filename: [u8; MAX_FILENAME_LEN],
    /// Filename length.
    pub filename_len: usize,
    /// Title.
    pub title: [u8; 64],
    /// Title length.
    pub title_len: usize,
    /// Result callback.
    pub callback: Option<PickerCallback>,
    /// Result.
    pub result: PickerResult,
}

impl FilePicker {
    /// Create empty picker.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            state: PickerState::Inactive,
            mode: PickerMode::OpenFile,
            app_id: 0,
            window_id: 0,
            view: DirectoryView::new(),
            filters: [FileFilter::empty(); MAX_FILTERS],
            filter_count: 0,
            current_filter: 0,
            bookmarks: [Bookmark::empty(); MAX_BOOKMARKS],
            bookmark_count: 0,
            filename: [0u8; MAX_FILENAME_LEN],
            filename_len: 0,
            title: [0u8; 64],
            title_len: 0,
            callback: None,
            result: PickerResult::empty(),
        }
    }

    /// Check if picker is active.
    pub fn is_active(&self) -> bool {
        self.state == PickerState::Active
    }

    /// Set title.
    pub fn set_title(&mut self, title: &str) {
        let bytes = title.as_bytes();
        let len = bytes.len().min(63);
        self.title = [0u8; 64];
        self.title[..len].copy_from_slice(&bytes[..len]);
        self.title_len = len;
    }

    /// Get title.
    pub fn title(&self) -> &str {
        core::str::from_utf8(&self.title[..self.title_len]).unwrap_or("")
    }

    /// Set filename (for save dialog).
    pub fn set_filename(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = bytes.len().min(MAX_FILENAME_LEN - 1);
        self.filename = [0u8; MAX_FILENAME_LEN];
        self.filename[..len].copy_from_slice(&bytes[..len]);
        self.filename_len = len;
    }

    /// Get filename.
    pub fn filename(&self) -> &str {
        core::str::from_utf8(&self.filename[..self.filename_len]).unwrap_or("")
    }

    /// Add filter.
    pub fn add_filter(&mut self, filter: FileFilter) -> bool {
        if self.filter_count >= MAX_FILTERS {
            return false;
        }
        self.filters[self.filter_count] = filter;
        self.filter_count += 1;
        true
    }

    /// Get current filter.
    pub fn current_filter(&self) -> Option<&FileFilter> {
        if self.current_filter < self.filter_count {
            Some(&self.filters[self.current_filter])
        } else {
            None
        }
    }

    /// Add bookmark.
    pub fn add_bookmark(&mut self, name: &str, path: &str) -> bool {
        if self.bookmark_count >= MAX_BOOKMARKS {
            return false;
        }
        self.bookmarks[self.bookmark_count] = Bookmark::new(name, path);
        self.bookmark_count += 1;
        true
    }

    /// Initialize default bookmarks.
    pub fn init_default_bookmarks(&mut self) {
        self.add_bookmark("Home", "/home");
        self.add_bookmark("Documents", "/home/documents");
        self.add_bookmark("Downloads", "/home/downloads");
        self.add_bookmark("Desktop", "/home/desktop");
    }

    /// Handle selection.
    pub fn select_entry(&mut self) {
        // Extract info first to avoid borrow conflicts
        let is_dir = self.view.selected_entry().map(|e| e.entry_type.is_dir()).unwrap_or(false);
        let selected_idx = self.view.selected_index.unwrap_or(0);

        if is_dir {
            // Navigate into directory
            // Copy the name into a local buffer first (no_std friendly)
            let mut name_buf = [0u8; 256];
            let name_len;
            if let Some(entry) = self.view.selected_entry() {
                let entry_name = entry.name().as_bytes();
                name_len = entry_name.len().min(255);
                name_buf[..name_len].copy_from_slice(&entry_name[..name_len]);
            } else {
                return;
            }
            // Navigate would use name_buf here
            // self.view.navigate(core::str::from_utf8(&name_buf[..name_len]).unwrap_or(""));
            // RAYOS_FILEPICKER:NAVIGATED
            let _ = name_buf; // Silence unused warning for now
        } else if self.view.selected_entry().is_some() {
            // Select file
            self.toggle_selection(selected_idx);
        }
    }

    /// Toggle selection on entry.
    pub fn toggle_selection(&mut self, filtered_index: usize) {
        if filtered_index >= self.view.filtered_count {
            return;
        }

        let real_index = self.view.filtered[filtered_index] as usize;

        if self.mode == PickerMode::OpenMultiple {
            // Toggle just this entry
            if let Some(entry) = self.view.entries.get_mut(real_index) {
                entry.selected = !entry.selected;
            }
        } else {
            // Single selection - deselect all first, then select target
            let entry_count = self.view.entry_count;
            for i in 0..entry_count {
                self.view.entries[i].selected = i == real_index;
            }
        }
    }

    /// Get selected entries.
    pub fn selected_entries(&self) -> impl Iterator<Item = &FileEntry> {
        self.view.entries[..self.view.entry_count]
            .iter()
            .filter(|e| e.selected)
    }

    /// Count selected entries.
    pub fn selection_count(&self) -> usize {
        self.view.entries[..self.view.entry_count]
            .iter()
            .filter(|e| e.selected)
            .count()
    }

    /// Confirm selection.
    pub fn confirm(&mut self) {
        self.result = PickerResult::empty();
        self.result.picker_id = self.id;

        match self.mode {
            PickerMode::SaveFile => {
                // Build full path with filename
                let mut path = self.view.path;
                if path.push(self.filename()) {
                    self.result.add_path(path.as_str());
                }
            }
            PickerMode::SelectFolder => {
                self.result.add_path(self.view.path.as_str());
            }
            PickerMode::OpenFile | PickerMode::OpenMultiple => {
                // Collect selected entry indices first to avoid borrow conflict
                let mut selected_indices = [0usize; 64];
                let mut selected_count = 0;
                for i in 0..self.view.entry_count {
                    if self.view.entries[i].selected && selected_count < 64 {
                        selected_indices[selected_count] = i;
                        selected_count += 1;
                    }
                }

                // Now add paths for each selected entry
                for i in 0..selected_count {
                    let idx = selected_indices[i];
                    let mut path = self.view.path;
                    if path.push(self.view.entries[idx].name()) {
                        self.result.add_path(path.as_str());
                    }
                }
            }
        }

        self.state = PickerState::Completed;
        // RAYOS_FILEPICKER:SELECTED

        if let Some(callback) = self.callback {
            callback(&self.result);
        }
    }

    /// Cancel the picker.
    pub fn cancel(&mut self) {
        self.result = PickerResult::cancelled(self.id);
        self.state = PickerState::Cancelled;
        // RAYOS_FILEPICKER:CANCELLED

        if let Some(callback) = self.callback {
            callback(&self.result);
        }
    }
}

// ============================================================================
// Picker Manager
// ============================================================================

/// File picker manager.
pub struct PickerManager {
    /// Active pickers.
    pickers: [FilePicker; MAX_ACTIVE_PICKERS],
    /// Next picker ID.
    next_id: u32,
    /// Statistics: opened count.
    stats_opened: u64,
    /// Statistics: selected count.
    stats_selected: u64,
    /// Statistics: cancelled count.
    stats_cancelled: u64,
}

impl PickerManager {
    /// Create new manager.
    pub const fn new() -> Self {
        const EMPTY_PICKER: FilePicker = FilePicker::empty();
        Self {
            pickers: [EMPTY_PICKER; MAX_ACTIVE_PICKERS],
            next_id: 1,
            stats_opened: 0,
            stats_selected: 0,
            stats_cancelled: 0,
        }
    }

    /// Create a new picker.
    pub fn create(
        &mut self,
        mode: PickerMode,
        app_id: AppId,
        callback: Option<PickerCallback>,
    ) -> Option<u32> {
        // Find empty slot
        let slot = self.pickers.iter_mut().find(|p| !p.is_active())?;

        let id = self.next_id;
        self.next_id += 1;

        *slot = FilePicker::empty();
        slot.id = id;
        slot.state = PickerState::Active;
        slot.mode = mode;
        slot.app_id = app_id;
        slot.callback = callback;
        slot.view.set_path("/");
        slot.add_filter(FileFilter::all_files());
        slot.init_default_bookmarks();

        // Set default title
        let title = match mode {
            PickerMode::OpenFile => "Open File",
            PickerMode::OpenMultiple => "Open Files",
            PickerMode::SaveFile => "Save File",
            PickerMode::SelectFolder => "Select Folder",
        };
        slot.set_title(title);

        self.stats_opened += 1;
        // RAYOS_FILEPICKER:OPENED

        Some(id)
    }

    /// Get picker by ID.
    pub fn get(&self, id: u32) -> Option<&FilePicker> {
        self.pickers.iter().find(|p| p.id == id && p.is_active())
    }

    /// Get picker by ID (mutable).
    pub fn get_mut(&mut self, id: u32) -> Option<&mut FilePicker> {
        self.pickers
            .iter_mut()
            .find(|p| p.id == id && p.is_active())
    }

    /// Close picker.
    pub fn close(&mut self, id: u32) -> Option<PickerResult> {
        let picker = self.pickers.iter_mut().find(|p| p.id == id)?;

        let result = picker.result;

        match picker.state {
            PickerState::Completed => self.stats_selected += 1,
            PickerState::Cancelled => self.stats_cancelled += 1,
            _ => {}
        }

        *picker = FilePicker::empty();
        Some(result)
    }

    /// Get active picker count.
    pub fn active_count(&self) -> usize {
        self.pickers.iter().filter(|p| p.is_active()).count()
    }

    /// Get statistics.
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.stats_opened, self.stats_selected, self.stats_cancelled)
    }
}

// ============================================================================
// Global Picker Manager
// ============================================================================

/// Global picker manager.
static mut GLOBAL_PICKER: PickerManager = PickerManager::new();

/// Get picker manager.
pub fn picker_manager() -> &'static PickerManager {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_PICKER }
}

/// Get picker manager (mutable).
pub fn picker_manager_mut() -> &'static mut PickerManager {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_PICKER }
}

/// Open file picker.
pub fn open_file(app_id: AppId, callback: Option<PickerCallback>) -> Option<u32> {
    picker_manager_mut().create(PickerMode::OpenFile, app_id, callback)
}

/// Save file picker.
pub fn save_file(app_id: AppId, callback: Option<PickerCallback>) -> Option<u32> {
    picker_manager_mut().create(PickerMode::SaveFile, app_id, callback)
}

/// Select folder picker.
pub fn select_folder(app_id: AppId, callback: Option<PickerCallback>) -> Option<u32> {
    picker_manager_mut().create(PickerMode::SelectFolder, app_id, callback)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_buffer() {
        let mut path = PathBuffer::root();
        assert_eq!(path.as_str(), "/");
        assert!(path.is_root());

        path.push("home");
        assert_eq!(path.as_str(), "/home");

        path.push("user");
        assert_eq!(path.as_str(), "/home/user");

        path.pop();
        assert_eq!(path.as_str(), "/home");
    }

    #[test]
    fn test_file_entry() {
        let entry = FileEntry::new("test.txt", FileEntryType::File);
        assert_eq!(entry.name(), "test.txt");
        assert_eq!(entry.extension(), Some(".txt"));
    }

    #[test]
    fn test_file_filter() {
        let mut filter = FileFilter::new("Images");
        filter.add_extension(".png");
        filter.add_extension(".jpg");

        assert_eq!(filter.name(), "Images");
        assert_eq!(filter.extension(0), ".png");
        assert_eq!(filter.extension(1), ".jpg");
    }

    #[test]
    fn test_filter_matching() {
        let mut filter = FileFilter::new("Text");
        filter.add_extension(".txt");

        let txt_entry = FileEntry::new("readme.txt", FileEntryType::File);
        let jpg_entry = FileEntry::new("photo.jpg", FileEntryType::File);
        let dir_entry = FileEntry::new("folder", FileEntryType::Directory);

        assert!(txt_entry.matches_filter(&filter));
        assert!(!jpg_entry.matches_filter(&filter));
        assert!(dir_entry.matches_filter(&filter)); // Dirs always match
    }

    #[test]
    fn test_picker_result() {
        let mut result = PickerResult::empty();
        result.add_path("/home/test.txt");
        result.add_path("/home/test2.txt");

        assert_eq!(result.path_count, 2);
        assert_eq!(result.first_path(), Some("/home/test.txt"));
    }

    #[test]
    fn test_bookmark() {
        let bm = Bookmark::new("Home", "/home/user");
        assert_eq!(bm.name(), "Home");
        assert_eq!(bm.path.as_str(), "/home/user");
    }

    #[test]
    fn test_picker_manager() {
        let mut manager = PickerManager::new();

        let id = manager.create(PickerMode::OpenFile, 1, None).unwrap();
        assert!(manager.get(id).is_some());
        assert_eq!(manager.active_count(), 1);

        manager.close(id);
        assert_eq!(manager.active_count(), 0);
    }
}
