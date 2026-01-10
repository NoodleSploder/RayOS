//! Shell Integration for RayOS UI
//!
//! Desktop shell with app launcher, taskbar, and notification area.
//! Provides a complete desktop environment experience.
//!
//! # Overview
//!
//! The Shell Integration provides:
//! - Taskbar with window buttons and focus indication
//! - App launcher overlay with search
//! - Notification area with system tray icons
//! - Desktop icons for quick access
//! - Notification toasts with timeout
//!
//! # Markers
//!
//! - `RAYOS_SHELL:TASKBAR` - Taskbar updated
//! - `RAYOS_SHELL:LAUNCHER` - App launcher opened/closed
//! - `RAYOS_SHELL:NOTIFY` - Notification displayed
//! - `RAYOS_SHELL:COMMAND` - Shell command executed
//! - `RAYOS_SHELL:DESKTOP` - Desktop action performed

use super::window_manager::WindowId;
use super::app_runtime::AppId;

// ============================================================================
// Constants
// ============================================================================

/// Maximum taskbar entries.
pub const MAX_TASKBAR_ENTRIES: usize = 16;

/// Maximum apps in launcher.
pub const MAX_LAUNCHER_APPS: usize = 24;

/// Maximum tray icons.
pub const MAX_TRAY_ICONS: usize = 8;

/// Maximum notification queue.
pub const MAX_NOTIFICATIONS: usize = 8;

/// Maximum desktop icons.
pub const MAX_DESKTOP_ICONS: usize = 16;

/// Default notification timeout in milliseconds.
pub const DEFAULT_NOTIFICATION_TIMEOUT_MS: u64 = 5000;

/// Taskbar height in pixels.
pub const TASKBAR_HEIGHT: u32 = 48;

/// Taskbar button width.
pub const TASKBAR_BUTTON_WIDTH: u32 = 200;

/// Tray icon size.
pub const TRAY_ICON_SIZE: u32 = 24;

/// Desktop icon size.
pub const DESKTOP_ICON_SIZE: u32 = 64;

// ============================================================================
// Taskbar Entry
// ============================================================================

/// Entry in the taskbar.
#[derive(Clone, Copy)]
pub struct TaskbarEntry {
    /// Window ID for this entry.
    pub window_id: WindowId,
    /// App ID (if known).
    pub app_id: AppId,
    /// Entry title (null-terminated).
    pub title: [u8; 32],
    /// Icon index (0 = default).
    pub icon_index: u16,
    /// Entry state.
    pub state: TaskbarEntryState,
    /// Whether entry is pinned.
    pub pinned: bool,
    /// Flash count for attention.
    pub flash_count: u8,
}

/// Taskbar entry state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TaskbarEntryState {
    /// Normal state.
    Normal = 0,
    /// Entry is focused.
    Focused = 1,
    /// Entry is minimized.
    Minimized = 2,
    /// Entry needs attention.
    Attention = 3,
    /// Entry is loading.
    Loading = 4,
}

impl Default for TaskbarEntryState {
    fn default() -> Self {
        TaskbarEntryState::Normal
    }
}

impl TaskbarEntry {
    /// Create an empty entry.
    pub const fn empty() -> Self {
        Self {
            window_id: 0,
            app_id: 0,
            title: [0u8; 32],
            icon_index: 0,
            state: TaskbarEntryState::Normal,
            pinned: false,
            flash_count: 0,
        }
    }

    /// Create a new entry.
    pub fn new(window_id: WindowId, title: &str) -> Self {
        let mut entry = Self::empty();
        entry.window_id = window_id;

        let title_bytes = title.as_bytes();
        let copy_len = title_bytes.len().min(31);
        entry.title[..copy_len].copy_from_slice(&title_bytes[..copy_len]);

        entry
    }

    /// Get title as string.
    pub fn title_str(&self) -> &str {
        let len = self.title.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.title[..len]).unwrap_or("")
    }

    /// Check if entry is valid.
    pub fn is_valid(&self) -> bool {
        self.window_id != 0
    }
}

// ============================================================================
// Taskbar
// ============================================================================

/// Desktop taskbar.
pub struct Taskbar {
    /// Taskbar entries.
    entries: [TaskbarEntry; MAX_TASKBAR_ENTRIES],
    /// Number of entries.
    count: usize,
    /// Currently hovered entry index.
    hovered_index: Option<usize>,
    /// Taskbar position.
    position: TaskbarPosition,
    /// Taskbar visibility.
    visible: bool,
    /// Auto-hide mode.
    auto_hide: bool,
    /// Whether taskbar is currently hidden (auto-hide).
    hidden: bool,
    /// Taskbar bounds.
    bounds: (i32, i32, u32, u32),
}

/// Taskbar position.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TaskbarPosition {
    /// Bottom of screen.
    Bottom = 0,
    /// Top of screen.
    Top = 1,
    /// Left of screen.
    Left = 2,
    /// Right of screen.
    Right = 3,
}

impl Default for TaskbarPosition {
    fn default() -> Self {
        TaskbarPosition::Bottom
    }
}

impl Taskbar {
    /// Create a new taskbar.
    pub const fn new() -> Self {
        Self {
            entries: [TaskbarEntry::empty(); MAX_TASKBAR_ENTRIES],
            count: 0,
            hovered_index: None,
            position: TaskbarPosition::Bottom,
            visible: true,
            auto_hide: false,
            hidden: false,
            bounds: (0, 0, 0, 0),
        }
    }

    /// Initialize taskbar for a screen size.
    pub fn init(&mut self, screen_width: u32, screen_height: u32) {
        match self.position {
            TaskbarPosition::Bottom => {
                self.bounds = (
                    0,
                    (screen_height - TASKBAR_HEIGHT) as i32,
                    screen_width,
                    TASKBAR_HEIGHT,
                );
            }
            TaskbarPosition::Top => {
                self.bounds = (0, 0, screen_width, TASKBAR_HEIGHT);
            }
            TaskbarPosition::Left => {
                self.bounds = (0, 0, TASKBAR_HEIGHT, screen_height);
            }
            TaskbarPosition::Right => {
                self.bounds = (
                    (screen_width - TASKBAR_HEIGHT) as i32,
                    0,
                    TASKBAR_HEIGHT,
                    screen_height,
                );
            }
        }
    }

    /// Add a window to the taskbar.
    pub fn add_window(&mut self, window_id: WindowId, title: &str) -> bool {
        // Check if already exists
        for entry in &self.entries[..self.count] {
            if entry.window_id == window_id {
                return false;
            }
        }

        if self.count >= MAX_TASKBAR_ENTRIES {
            return false;
        }

        self.entries[self.count] = TaskbarEntry::new(window_id, title);
        self.count += 1;
        // RAYOS_SHELL:TASKBAR
        true
    }

    /// Remove a window from the taskbar.
    pub fn remove_window(&mut self, window_id: WindowId) -> bool {
        for i in 0..self.count {
            if self.entries[i].window_id == window_id {
                // Shift entries
                for j in i..self.count - 1 {
                    self.entries[j] = self.entries[j + 1];
                }
                self.entries[self.count - 1] = TaskbarEntry::empty();
                self.count -= 1;
                // RAYOS_SHELL:TASKBAR
                return true;
            }
        }
        false
    }

    /// Update window title.
    pub fn update_title(&mut self, window_id: WindowId, title: &str) {
        for entry in &mut self.entries[..self.count] {
            if entry.window_id == window_id {
                let title_bytes = title.as_bytes();
                let copy_len = title_bytes.len().min(31);
                entry.title = [0u8; 32];
                entry.title[..copy_len].copy_from_slice(&title_bytes[..copy_len]);
                return;
            }
        }
    }

    /// Set focused window.
    pub fn set_focused(&mut self, window_id: WindowId) {
        for entry in &mut self.entries[..self.count] {
            if entry.window_id == window_id {
                entry.state = TaskbarEntryState::Focused;
            } else if entry.state == TaskbarEntryState::Focused {
                entry.state = TaskbarEntryState::Normal;
            }
        }
    }

    /// Set window minimized state.
    pub fn set_minimized(&mut self, window_id: WindowId, minimized: bool) {
        for entry in &mut self.entries[..self.count] {
            if entry.window_id == window_id {
                entry.state = if minimized {
                    TaskbarEntryState::Minimized
                } else {
                    TaskbarEntryState::Normal
                };
                return;
            }
        }
    }

    /// Request attention for a window.
    pub fn request_attention(&mut self, window_id: WindowId) {
        for entry in &mut self.entries[..self.count] {
            if entry.window_id == window_id && entry.state != TaskbarEntryState::Focused {
                entry.state = TaskbarEntryState::Attention;
                entry.flash_count = 5;
                return;
            }
        }
    }

    /// Hit test to find entry at position.
    pub fn hit_test(&self, x: i32, y: i32) -> Option<usize> {
        let (bx, by, bw, bh) = self.bounds;
        if x < bx || y < by || x >= bx + bw as i32 || y >= by + bh as i32 {
            return None;
        }

        // Calculate which entry was clicked
        let relative_x = (x - bx) as u32;
        let entry_index = (relative_x / TASKBAR_BUTTON_WIDTH) as usize;

        if entry_index < self.count {
            Some(entry_index)
        } else {
            None
        }
    }

    /// Get entry at index.
    pub fn get(&self, index: usize) -> Option<&TaskbarEntry> {
        if index < self.count {
            Some(&self.entries[index])
        } else {
            None
        }
    }

    /// Get number of entries.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Get taskbar bounds.
    pub fn bounds(&self) -> (i32, i32, u32, u32) {
        self.bounds
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible && !self.hidden
    }

    /// Set auto-hide mode.
    pub fn set_auto_hide(&mut self, auto_hide: bool) {
        self.auto_hide = auto_hide;
    }

    /// Toggle visibility (for auto-hide).
    pub fn toggle_hidden(&mut self) {
        if self.auto_hide {
            self.hidden = !self.hidden;
        }
    }
}

// ============================================================================
// App Launcher Menu
// ============================================================================

/// App entry in launcher.
#[derive(Clone, Copy)]
pub struct LauncherAppEntry {
    /// App identifier.
    pub identifier: [u8; 64],
    /// Display name.
    pub name: [u8; 32],
    /// Icon index.
    pub icon_index: u16,
    /// Category.
    pub category: AppCategory,
    /// Whether app is pinned to launcher.
    pub pinned: bool,
    /// Usage count (for sorting).
    pub usage_count: u32,
}

/// App category.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum AppCategory {
    /// Uncategorized.
    Other = 0,
    /// System apps.
    System = 1,
    /// Utilities.
    Utilities = 2,
    /// Development.
    Development = 3,
    /// Games.
    Games = 4,
    /// Graphics.
    Graphics = 5,
    /// Internet.
    Internet = 6,
    /// Office.
    Office = 7,
    /// Multimedia.
    Multimedia = 8,
}

impl Default for AppCategory {
    fn default() -> Self {
        AppCategory::Other
    }
}

impl LauncherAppEntry {
    /// Create an empty entry.
    pub const fn empty() -> Self {
        Self {
            identifier: [0u8; 64],
            name: [0u8; 32],
            icon_index: 0,
            category: AppCategory::Other,
            pinned: false,
            usage_count: 0,
        }
    }

    /// Create a new entry.
    pub fn new(identifier: &str, name: &str, category: AppCategory) -> Self {
        let mut entry = Self::empty();

        let id_bytes = identifier.as_bytes();
        let id_len = id_bytes.len().min(63);
        entry.identifier[..id_len].copy_from_slice(&id_bytes[..id_len]);

        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len().min(31);
        entry.name[..name_len].copy_from_slice(&name_bytes[..name_len]);

        entry.category = category;
        entry
    }

    /// Get name as string.
    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }

    /// Get identifier as string.
    pub fn identifier_str(&self) -> &str {
        let len = self.identifier.iter().position(|&b| b == 0).unwrap_or(64);
        core::str::from_utf8(&self.identifier[..len]).unwrap_or("")
    }

    /// Check if entry is valid.
    pub fn is_valid(&self) -> bool {
        self.name[0] != 0
    }
}

/// App launcher menu.
pub struct AppLauncherMenu {
    /// Available apps.
    apps: [LauncherAppEntry; MAX_LAUNCHER_APPS],
    /// Number of apps.
    app_count: usize,
    /// Search filter (null-terminated).
    search_filter: [u8; 32],
    /// Selected index.
    selected_index: usize,
    /// Whether launcher is open.
    open: bool,
    /// Launcher bounds.
    bounds: (i32, i32, u32, u32),
    /// Filter category.
    filter_category: Option<AppCategory>,
}

impl AppLauncherMenu {
    /// Create a new launcher menu.
    pub const fn new() -> Self {
        Self {
            apps: [LauncherAppEntry::empty(); MAX_LAUNCHER_APPS],
            app_count: 0,
            search_filter: [0u8; 32],
            selected_index: 0,
            open: false,
            bounds: (0, 0, 400, 500),
            filter_category: None,
        }
    }

    /// Register an app.
    pub fn register_app(&mut self, entry: LauncherAppEntry) -> bool {
        if self.app_count >= MAX_LAUNCHER_APPS {
            return false;
        }
        self.apps[self.app_count] = entry;
        self.app_count += 1;
        true
    }

    /// Unregister an app by identifier.
    pub fn unregister_app(&mut self, identifier: &str) -> bool {
        for i in 0..self.app_count {
            if self.apps[i].identifier_str() == identifier {
                for j in i..self.app_count - 1 {
                    self.apps[j] = self.apps[j + 1];
                }
                self.apps[self.app_count - 1] = LauncherAppEntry::empty();
                self.app_count -= 1;
                return true;
            }
        }
        false
    }

    /// Open the launcher.
    pub fn open(&mut self) {
        self.open = true;
        self.selected_index = 0;
        self.search_filter = [0u8; 32];
        // RAYOS_SHELL:LAUNCHER
    }

    /// Close the launcher.
    pub fn close(&mut self) {
        self.open = false;
        // RAYOS_SHELL:LAUNCHER
    }

    /// Toggle the launcher.
    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    /// Check if open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Set search filter.
    pub fn set_filter(&mut self, filter: &str) {
        self.search_filter = [0u8; 32];
        let filter_bytes = filter.as_bytes();
        let copy_len = filter_bytes.len().min(31);
        self.search_filter[..copy_len].copy_from_slice(&filter_bytes[..copy_len]);
        self.selected_index = 0;
    }

    /// Get filter string.
    pub fn filter_str(&self) -> &str {
        let len = self.search_filter.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.search_filter[..len]).unwrap_or("")
    }

    /// Set category filter.
    pub fn set_category(&mut self, category: Option<AppCategory>) {
        self.filter_category = category;
        self.selected_index = 0;
    }

    /// Check if app matches current filter.
    fn matches_filter(&self, app: &LauncherAppEntry) -> bool {
        // Category filter
        if let Some(cat) = self.filter_category {
            if app.category != cat {
                return false;
            }
        }

        // Text filter
        let filter = self.filter_str();
        if filter.is_empty() {
            return true;
        }

        // Simple case-insensitive substring match (no_std compatible)
        let name = app.name_str();
        str_contains_ignore_case(name, filter)
    }
}

/// Case-insensitive substring search for no_std.
fn str_contains_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    let haystack_bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    
    'outer: for i in 0..=(haystack_bytes.len() - needle_bytes.len()) {
        for j in 0..needle_bytes.len() {
            let h = to_ascii_lower(haystack_bytes[i + j]);
            let n = to_ascii_lower(needle_bytes[j]);
            if h != n {
                continue 'outer;
            }
        }
        return true;
    }
    false
}

/// Convert ASCII byte to lowercase.
#[inline]
fn to_ascii_lower(b: u8) -> u8 {
    if b >= b'A' && b <= b'Z' {
        b + 32
    } else {
        b
    }
}

impl AppLauncherMenu {
    /// Get filtered apps.
    pub fn filtered_apps(&self) -> impl Iterator<Item = &LauncherAppEntry> {
        self.apps[..self.app_count]
            .iter()
            .filter(|app| app.is_valid() && self.matches_filter(app))
    }

    /// Get filtered app count.
    pub fn filtered_count(&self) -> usize {
        self.apps[..self.app_count]
            .iter()
            .filter(|app| app.is_valid() && self.matches_filter(app))
            .count()
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        let max = self.filtered_count();
        if max > 0 && self.selected_index < max - 1 {
            self.selected_index += 1;
        }
    }

    /// Get selected app.
    pub fn selected_app(&self) -> Option<&LauncherAppEntry> {
        self.filtered_apps().nth(self.selected_index)
    }

    /// Increment usage count for an app.
    pub fn increment_usage(&mut self, identifier: &str) {
        for app in &mut self.apps[..self.app_count] {
            if app.identifier_str() == identifier {
                app.usage_count = app.usage_count.saturating_add(1);
                return;
            }
        }
    }

    /// Get bounds.
    pub fn bounds(&self) -> (i32, i32, u32, u32) {
        self.bounds
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: (i32, i32, u32, u32)) {
        self.bounds = bounds;
    }
}

// ============================================================================
// Notification Area
// ============================================================================

/// Tray icon entry.
#[derive(Clone, Copy)]
pub struct TrayIcon {
    /// Unique identifier.
    pub id: u32,
    /// Icon index.
    pub icon_index: u16,
    /// Tooltip text.
    pub tooltip: [u8; 32],
    /// Whether icon is visible.
    pub visible: bool,
    /// Badge count (0 = no badge).
    pub badge: u16,
}

impl TrayIcon {
    /// Create an empty tray icon.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            icon_index: 0,
            tooltip: [0u8; 32],
            visible: false,
            badge: 0,
        }
    }

    /// Create a new tray icon.
    pub fn new(id: u32, icon_index: u16, tooltip: &str) -> Self {
        let mut icon = Self::empty();
        icon.id = id;
        icon.icon_index = icon_index;
        icon.visible = true;

        let tooltip_bytes = tooltip.as_bytes();
        let copy_len = tooltip_bytes.len().min(31);
        icon.tooltip[..copy_len].copy_from_slice(&tooltip_bytes[..copy_len]);

        icon
    }

    /// Get tooltip as string.
    pub fn tooltip_str(&self) -> &str {
        let len = self.tooltip.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.tooltip[..len]).unwrap_or("")
    }
}

/// Notification area (system tray).
pub struct NotificationArea {
    /// Tray icons.
    icons: [TrayIcon; MAX_TRAY_ICONS],
    /// Icon count.
    icon_count: usize,
    /// Clock visible.
    show_clock: bool,
    /// Current time display.
    time_display: [u8; 8],
    /// Area bounds.
    bounds: (i32, i32, u32, u32),
}

impl NotificationArea {
    /// Create a new notification area.
    pub const fn new() -> Self {
        Self {
            icons: [TrayIcon::empty(); MAX_TRAY_ICONS],
            icon_count: 0,
            show_clock: true,
            time_display: [0u8; 8],
            bounds: (0, 0, 200, TASKBAR_HEIGHT),
        }
    }

    /// Add a tray icon.
    pub fn add_icon(&mut self, icon: TrayIcon) -> bool {
        if self.icon_count >= MAX_TRAY_ICONS {
            return false;
        }
        self.icons[self.icon_count] = icon;
        self.icon_count += 1;
        true
    }

    /// Remove a tray icon.
    pub fn remove_icon(&mut self, id: u32) -> bool {
        for i in 0..self.icon_count {
            if self.icons[i].id == id {
                for j in i..self.icon_count - 1 {
                    self.icons[j] = self.icons[j + 1];
                }
                self.icons[self.icon_count - 1] = TrayIcon::empty();
                self.icon_count -= 1;
                return true;
            }
        }
        false
    }

    /// Update icon badge.
    pub fn set_badge(&mut self, id: u32, badge: u16) {
        for icon in &mut self.icons[..self.icon_count] {
            if icon.id == id {
                icon.badge = badge;
                return;
            }
        }
    }

    /// Update clock display.
    pub fn update_clock(&mut self, hours: u8, minutes: u8) {
        // Format as HH:MM
        self.time_display[0] = b'0' + (hours / 10);
        self.time_display[1] = b'0' + (hours % 10);
        self.time_display[2] = b':';
        self.time_display[3] = b'0' + (minutes / 10);
        self.time_display[4] = b'0' + (minutes % 10);
        self.time_display[5] = 0;
    }

    /// Get clock display.
    pub fn clock_str(&self) -> &str {
        let len = self.time_display.iter().position(|&b| b == 0).unwrap_or(8);
        core::str::from_utf8(&self.time_display[..len]).unwrap_or("")
    }

    /// Get icon count.
    pub fn icon_count(&self) -> usize {
        self.icon_count
    }

    /// Get icon at index.
    pub fn get_icon(&self, index: usize) -> Option<&TrayIcon> {
        if index < self.icon_count {
            Some(&self.icons[index])
        } else {
            None
        }
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: (i32, i32, u32, u32)) {
        self.bounds = bounds;
    }

    /// Get bounds.
    pub fn bounds(&self) -> (i32, i32, u32, u32) {
        self.bounds
    }
}

// ============================================================================
// Notification Toast
// ============================================================================

/// Notification urgency level.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum NotificationUrgency {
    /// Low urgency (silent).
    Low = 0,
    /// Normal urgency.
    Normal = 1,
    /// High urgency (attention).
    High = 2,
    /// Critical (persistent).
    Critical = 3,
}

impl Default for NotificationUrgency {
    fn default() -> Self {
        NotificationUrgency::Normal
    }
}

/// Notification toast.
#[derive(Clone, Copy)]
pub struct NotificationToast {
    /// Unique notification ID.
    pub id: u32,
    /// Title.
    pub title: [u8; 32],
    /// Body text.
    pub body: [u8; 128],
    /// Icon index.
    pub icon_index: u16,
    /// Urgency level.
    pub urgency: NotificationUrgency,
    /// Timeout in milliseconds (0 = no timeout).
    pub timeout_ms: u64,
    /// Timestamp when shown.
    pub shown_at: u64,
    /// Whether notification is visible.
    pub visible: bool,
    /// Progress value (0-100, 255 = no progress).
    pub progress: u8,
    /// Source app ID.
    pub app_id: AppId,
}

impl NotificationToast {
    /// Create an empty notification.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            title: [0u8; 32],
            body: [0u8; 128],
            icon_index: 0,
            urgency: NotificationUrgency::Normal,
            timeout_ms: DEFAULT_NOTIFICATION_TIMEOUT_MS,
            shown_at: 0,
            visible: false,
            progress: 255,
            app_id: 0,
        }
    }

    /// Create a new notification.
    pub fn new(id: u32, title: &str, body: &str) -> Self {
        let mut toast = Self::empty();
        toast.id = id;
        toast.visible = true;

        let title_bytes = title.as_bytes();
        let title_len = title_bytes.len().min(31);
        toast.title[..title_len].copy_from_slice(&title_bytes[..title_len]);

        let body_bytes = body.as_bytes();
        let body_len = body_bytes.len().min(127);
        toast.body[..body_len].copy_from_slice(&body_bytes[..body_len]);

        toast
    }

    /// Get title as string.
    pub fn title_str(&self) -> &str {
        let len = self.title.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.title[..len]).unwrap_or("")
    }

    /// Get body as string.
    pub fn body_str(&self) -> &str {
        let len = self.body.iter().position(|&b| b == 0).unwrap_or(128);
        core::str::from_utf8(&self.body[..len]).unwrap_or("")
    }

    /// Check if notification has expired.
    pub fn is_expired(&self, current_time: u64) -> bool {
        if self.timeout_ms == 0 || self.urgency == NotificationUrgency::Critical {
            return false;
        }
        current_time >= self.shown_at + self.timeout_ms
    }
}

/// Notification queue.
pub struct NotificationQueue {
    /// Notifications.
    notifications: [NotificationToast; MAX_NOTIFICATIONS],
    /// Active count.
    count: usize,
    /// Next notification ID.
    next_id: u32,
    /// Total notifications shown.
    total_shown: u64,
}

impl NotificationQueue {
    /// Create a new queue.
    pub const fn new() -> Self {
        Self {
            notifications: [NotificationToast::empty(); MAX_NOTIFICATIONS],
            count: 0,
            next_id: 1,
            total_shown: 0,
        }
    }

    /// Show a notification.
    pub fn show(&mut self, mut toast: NotificationToast, current_time: u64) -> u32 {
        toast.id = self.next_id;
        self.next_id += 1;
        toast.shown_at = current_time;
        toast.visible = true;

        // Find empty slot or oldest non-critical
        let mut slot = None;
        for i in 0..MAX_NOTIFICATIONS {
            if !self.notifications[i].visible {
                slot = Some(i);
                break;
            }
        }

        // If full, replace oldest non-critical
        if slot.is_none() {
            let mut oldest_time = u64::MAX;
            for i in 0..MAX_NOTIFICATIONS {
                if self.notifications[i].urgency != NotificationUrgency::Critical
                    && self.notifications[i].shown_at < oldest_time
                {
                    oldest_time = self.notifications[i].shown_at;
                    slot = Some(i);
                }
            }
        }

        if let Some(idx) = slot {
            self.notifications[idx] = toast;
            if idx >= self.count {
                self.count = idx + 1;
            }
            self.total_shown += 1;
            // RAYOS_SHELL:NOTIFY
        }

        toast.id
    }

    /// Dismiss a notification.
    pub fn dismiss(&mut self, id: u32) {
        for notification in &mut self.notifications {
            if notification.id == id {
                notification.visible = false;
                return;
            }
        }
    }

    /// Update notifications (remove expired).
    pub fn update(&mut self, current_time: u64) {
        for notification in &mut self.notifications {
            if notification.visible && notification.is_expired(current_time) {
                notification.visible = false;
            }
        }
    }

    /// Get visible notifications.
    pub fn visible(&self) -> impl Iterator<Item = &NotificationToast> {
        self.notifications.iter().filter(|n| n.visible)
    }

    /// Get visible count.
    pub fn visible_count(&self) -> usize {
        self.notifications.iter().filter(|n| n.visible).count()
    }

    /// Get total shown.
    pub fn total_shown(&self) -> u64 {
        self.total_shown
    }
}

// ============================================================================
// Desktop Icon
// ============================================================================

/// Desktop icon.
#[derive(Clone, Copy)]
pub struct DesktopIcon {
    /// Icon ID.
    pub id: u32,
    /// Display label.
    pub label: [u8; 32],
    /// Icon index.
    pub icon_index: u16,
    /// Position on desktop.
    pub x: i32,
    pub y: i32,
    /// Associated action.
    pub action: DesktopAction,
    /// Whether icon is selected.
    pub selected: bool,
}

/// Desktop icon action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DesktopAction {
    /// No action.
    None = 0,
    /// Launch an app.
    LaunchApp = 1,
    /// Open a folder.
    OpenFolder = 2,
    /// Open a file.
    OpenFile = 3,
    /// Open URL.
    OpenUrl = 4,
}

impl Default for DesktopAction {
    fn default() -> Self {
        DesktopAction::None
    }
}

impl DesktopIcon {
    /// Create an empty icon.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            label: [0u8; 32],
            icon_index: 0,
            x: 0,
            y: 0,
            action: DesktopAction::None,
            selected: false,
        }
    }

    /// Create a new icon.
    pub fn new(id: u32, label: &str, x: i32, y: i32, action: DesktopAction) -> Self {
        let mut icon = Self::empty();
        icon.id = id;
        icon.x = x;
        icon.y = y;
        icon.action = action;

        let label_bytes = label.as_bytes();
        let copy_len = label_bytes.len().min(31);
        icon.label[..copy_len].copy_from_slice(&label_bytes[..copy_len]);

        icon
    }

    /// Get label as string.
    pub fn label_str(&self) -> &str {
        let len = self.label.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.label[..len]).unwrap_or("")
    }

    /// Hit test.
    pub fn hit_test(&self, x: i32, y: i32) -> bool {
        let size = DESKTOP_ICON_SIZE as i32;
        x >= self.x && x < self.x + size && y >= self.y && y < self.y + size
    }
}

// ============================================================================
// Shell Command
// ============================================================================

/// Shell command type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ShellCommand {
    /// No command.
    None = 0,
    /// Launch an app.
    Launch = 1,
    /// Close a window.
    Close = 2,
    /// Minimize a window.
    Minimize = 3,
    /// Maximize a window.
    Maximize = 4,
    /// Switch workspace.
    SwitchWorkspace = 5,
    /// Lock screen.
    Lock = 6,
    /// Logout.
    Logout = 7,
    /// Shutdown.
    Shutdown = 8,
    /// Restart.
    Restart = 9,
    /// Open settings.
    Settings = 10,
    /// Show desktop.
    ShowDesktop = 11,
}

impl Default for ShellCommand {
    fn default() -> Self {
        ShellCommand::None
    }
}

// ============================================================================
// Shell State
// ============================================================================

/// Desktop shell state.
pub struct ShellState {
    /// Taskbar.
    pub taskbar: Taskbar,
    /// App launcher menu.
    pub launcher: AppLauncherMenu,
    /// Notification area.
    pub notification_area: NotificationArea,
    /// Notification queue.
    pub notifications: NotificationQueue,
    /// Desktop icons.
    pub desktop_icons: [DesktopIcon; MAX_DESKTOP_ICONS],
    /// Desktop icon count.
    pub desktop_icon_count: usize,
    /// Current timestamp.
    timestamp: u64,
    /// Screen dimensions.
    screen_width: u32,
    screen_height: u32,
    /// Lock screen active.
    locked: bool,
}

impl ShellState {
    /// Create a new shell state.
    pub const fn new() -> Self {
        Self {
            taskbar: Taskbar::new(),
            launcher: AppLauncherMenu::new(),
            notification_area: NotificationArea::new(),
            notifications: NotificationQueue::new(),
            desktop_icons: [DesktopIcon::empty(); MAX_DESKTOP_ICONS],
            desktop_icon_count: 0,
            timestamp: 0,
            screen_width: 0,
            screen_height: 0,
            locked: false,
        }
    }

    /// Initialize shell for screen size.
    pub fn init(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;

        // Initialize taskbar
        self.taskbar.init(width, height);

        // Position notification area at right end of taskbar
        let taskbar_bounds = self.taskbar.bounds();
        let notif_width = 200u32;
        self.notification_area.set_bounds((
            (width - notif_width) as i32,
            taskbar_bounds.1,
            notif_width,
            TASKBAR_HEIGHT,
        ));

        // Center launcher
        let launcher_width = 400u32;
        let launcher_height = 500u32;
        self.launcher.set_bounds((
            ((width - launcher_width) / 2) as i32,
            ((height - launcher_height) / 2) as i32,
            launcher_width,
            launcher_height,
        ));
    }

    /// Tick the shell state.
    pub fn tick(&mut self) {
        self.timestamp += 1;

        // Update notifications (expire old ones)
        self.notifications.update(self.timestamp);
    }

    /// Add a desktop icon.
    pub fn add_desktop_icon(&mut self, icon: DesktopIcon) -> bool {
        if self.desktop_icon_count >= MAX_DESKTOP_ICONS {
            return false;
        }
        self.desktop_icons[self.desktop_icon_count] = icon;
        self.desktop_icon_count += 1;
        // RAYOS_SHELL:DESKTOP
        true
    }

    /// Remove a desktop icon.
    pub fn remove_desktop_icon(&mut self, id: u32) -> bool {
        for i in 0..self.desktop_icon_count {
            if self.desktop_icons[i].id == id {
                for j in i..self.desktop_icon_count - 1 {
                    self.desktop_icons[j] = self.desktop_icons[j + 1];
                }
                self.desktop_icons[self.desktop_icon_count - 1] = DesktopIcon::empty();
                self.desktop_icon_count -= 1;
                return true;
            }
        }
        false
    }

    /// Hit test desktop icons.
    pub fn hit_test_desktop(&self, x: i32, y: i32) -> Option<u32> {
        for icon in &self.desktop_icons[..self.desktop_icon_count] {
            if icon.hit_test(x, y) {
                return Some(icon.id);
            }
        }
        None
    }

    /// Execute a shell command.
    pub fn execute(&mut self, command: ShellCommand) {
        // RAYOS_SHELL:COMMAND
        match command {
            ShellCommand::Lock => {
                self.locked = true;
            }
            ShellCommand::ShowDesktop => {
                // Minimize all windows - handled by window manager
            }
            ShellCommand::Logout => {
                // Close all apps - handled by app runtime
            }
            _ => {}
        }
    }

    /// Check if screen is locked.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Unlock screen.
    pub fn unlock(&mut self) {
        self.locked = false;
    }

    /// Show a notification.
    pub fn notify(&mut self, title: &str, body: &str, urgency: NotificationUrgency) -> u32 {
        let mut toast = NotificationToast::new(0, title, body);
        toast.urgency = urgency;
        self.notifications.show(toast, self.timestamp)
    }

    /// Get screen dimensions.
    pub fn screen_size(&self) -> (u32, u32) {
        (self.screen_width, self.screen_height)
    }
}

// ============================================================================
// Global Shell State
// ============================================================================

/// Global shell state.
static mut GLOBAL_SHELL_STATE: ShellState = ShellState::new();

/// Get the global shell state.
pub fn shell_state() -> &'static ShellState {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_SHELL_STATE }
}

/// Get the global shell state mutably.
pub fn shell_state_mut() -> &'static mut ShellState {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_SHELL_STATE }
}

/// Initialize the shell.
pub fn init_shell(width: u32, height: u32) {
    shell_state_mut().init(width, height);
}

/// Tick the shell.
pub fn shell_tick() {
    shell_state_mut().tick();
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taskbar_entry() {
        let entry = TaskbarEntry::new(1, "Test Window");
        assert!(entry.is_valid());
        assert_eq!(entry.title_str(), "Test Window");
    }

    #[test]
    fn test_taskbar_add_remove() {
        let mut taskbar = Taskbar::new();

        assert!(taskbar.add_window(1, "Window 1"));
        assert!(taskbar.add_window(2, "Window 2"));
        assert_eq!(taskbar.count(), 2);

        assert!(taskbar.remove_window(1));
        assert_eq!(taskbar.count(), 1);
    }

    #[test]
    fn test_launcher_app_entry() {
        let entry = LauncherAppEntry::new("com.example.app", "Example App", AppCategory::Utilities);
        assert!(entry.is_valid());
        assert_eq!(entry.name_str(), "Example App");
        assert_eq!(entry.identifier_str(), "com.example.app");
    }

    #[test]
    fn test_launcher_menu() {
        let mut menu = AppLauncherMenu::new();

        menu.register_app(LauncherAppEntry::new("app1", "App 1", AppCategory::System));
        menu.register_app(LauncherAppEntry::new("app2", "App 2", AppCategory::Utilities));

        assert_eq!(menu.filtered_count(), 2);

        menu.set_category(Some(AppCategory::System));
        assert_eq!(menu.filtered_count(), 1);
    }

    #[test]
    fn test_notification_toast() {
        let toast = NotificationToast::new(1, "Test", "Test body");
        assert_eq!(toast.title_str(), "Test");
        assert_eq!(toast.body_str(), "Test body");
    }

    #[test]
    fn test_notification_queue() {
        let mut queue = NotificationQueue::new();

        let toast = NotificationToast::new(0, "Alert", "Something happened");
        let id = queue.show(toast, 1000);

        assert!(id > 0);
        assert_eq!(queue.visible_count(), 1);

        queue.dismiss(id);
        assert_eq!(queue.visible_count(), 0);
    }

    #[test]
    fn test_notification_expiry() {
        let mut queue = NotificationQueue::new();

        let mut toast = NotificationToast::new(0, "Alert", "Expires soon");
        toast.timeout_ms = 1000;
        queue.show(toast, 1000);

        // Not expired yet
        queue.update(1500);
        assert_eq!(queue.visible_count(), 1);

        // Now expired
        queue.update(2001);
        assert_eq!(queue.visible_count(), 0);
    }

    #[test]
    fn test_tray_icon() {
        let icon = TrayIcon::new(1, 0, "Network");
        assert_eq!(icon.tooltip_str(), "Network");
    }

    #[test]
    fn test_notification_area() {
        let mut area = NotificationArea::new();

        assert!(area.add_icon(TrayIcon::new(1, 0, "Icon 1")));
        assert!(area.add_icon(TrayIcon::new(2, 1, "Icon 2")));
        assert_eq!(area.icon_count(), 2);

        area.set_badge(1, 5);
        assert!(area.remove_icon(1));
        assert_eq!(area.icon_count(), 1);
    }

    #[test]
    fn test_clock_display() {
        let mut area = NotificationArea::new();
        area.update_clock(14, 30);
        assert_eq!(area.clock_str(), "14:30");
    }

    #[test]
    fn test_desktop_icon() {
        let icon = DesktopIcon::new(1, "Files", 100, 100, DesktopAction::OpenFolder);
        assert_eq!(icon.label_str(), "Files");
        assert!(icon.hit_test(120, 120));
        assert!(!icon.hit_test(50, 50));
    }

    #[test]
    fn test_shell_state_init() {
        let mut state = ShellState::new();
        state.init(1920, 1080);

        assert!(state.taskbar.is_visible());
        assert!(!state.launcher.is_open());
        assert!(!state.is_locked());
    }
}
