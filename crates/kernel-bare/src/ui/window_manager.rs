//! Window Manager for RayOS UI
//!
//! Manages window lifecycle, z-order stacking, and focus state.

use core::sync::atomic::{AtomicU32, Ordering};

/// Maximum number of windows supported.
pub const MAX_WINDOWS: usize = 16;

/// Maximum title length in bytes.
pub const MAX_TITLE_LEN: usize = 64;

/// Window identifier type.
pub type WindowId = u32;

/// Invalid/null window ID.
pub const WINDOW_ID_NONE: WindowId = 0;

/// Counter for generating unique window IDs.
static NEXT_WINDOW_ID: AtomicU32 = AtomicU32::new(1);

/// Type of window, affecting rendering and behavior.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum WindowType {
    /// Desktop background (always at bottom, no decorations)
    Desktop = 0,
    /// Normal window with title bar and decorations
    Normal = 1,
    /// VM surface window (renders guest framebuffer)
    VmSurface = 2,
    /// Panel (status bar, dock - no resize, special z-order)
    Panel = 3,
    /// Dialog (modal, centered)
    Dialog = 4,
    /// Popup (tooltip, dropdown - no decorations, temporary)
    Popup = 5,
}

impl Default for WindowType {
    fn default() -> Self {
        WindowType::Normal
    }
}

/// A single window in the UI.
#[derive(Clone)]
pub struct Window {
    /// Unique window identifier (0 = invalid/empty slot)
    pub id: WindowId,
    /// Window title (ASCII bytes)
    pub title: [u8; MAX_TITLE_LEN],
    /// Length of title in bytes
    pub title_len: usize,
    /// X position (top-left corner, screen coordinates)
    pub x: i32,
    /// Y position (top-left corner, screen coordinates)
    pub y: i32,
    /// Window content width (excluding decorations)
    pub width: u32,
    /// Window content height (excluding decorations)
    pub height: u32,
    /// Whether the window is visible
    pub visible: bool,
    /// Whether the window has focus
    pub focused: bool,
    /// Whether to draw window decorations (title bar, border)
    pub decorations: bool,
    /// Window type
    pub window_type: WindowType,
    /// Minimum width
    pub min_width: u32,
    /// Minimum height
    pub min_height: u32,
    /// User data pointer (for VM surface backing, etc.)
    pub user_data: u64,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            id: WINDOW_ID_NONE,
            title: [0u8; MAX_TITLE_LEN],
            title_len: 0,
            x: 0,
            y: 0,
            width: 400,
            height: 300,
            visible: true,
            focused: false,
            decorations: true,
            window_type: WindowType::Normal,
            min_width: 100,
            min_height: 50,
            user_data: 0,
        }
    }
}

impl Window {
    /// Create a new window with the given parameters.
    pub fn new(title: &[u8], x: i32, y: i32, width: u32, height: u32, window_type: WindowType) -> Self {
        let id = NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed);
        let mut win = Self {
            id,
            x,
            y,
            width,
            height,
            window_type,
            // VmSurface windows also get decorations (title bar, close button, etc.)
            decorations: matches!(window_type, WindowType::Normal | WindowType::Dialog | WindowType::VmSurface),
            ..Default::default()
        };
        win.set_title(title);
        win
    }

    /// Set the window title.
    pub fn set_title(&mut self, title: &[u8]) {
        let len = title.len().min(MAX_TITLE_LEN);
        self.title[..len].copy_from_slice(&title[..len]);
        if len < MAX_TITLE_LEN {
            self.title[len..].fill(0);
        }
        self.title_len = len;
    }

    /// Get the window title as a slice.
    pub fn get_title(&self) -> &[u8] {
        &self.title[..self.title_len]
    }

    /// Get the title bar height (0 if no decorations).
    pub fn title_bar_height(&self) -> u32 {
        if self.decorations { 24 } else { 0 }
    }

    /// Get the border width (0 if no decorations).
    pub fn border_width(&self) -> u32 {
        if self.decorations { 2 } else { 0 }
    }

    /// Get the total window width including decorations.
    pub fn total_width(&self) -> u32 {
        self.width + 2 * self.border_width()
    }

    /// Get the total window height including decorations.
    pub fn total_height(&self) -> u32 {
        self.height + self.title_bar_height() + self.border_width()
    }

    /// Get the content area rectangle (x, y, w, h) in screen coordinates.
    pub fn content_rect(&self) -> (i32, i32, u32, u32) {
        let bw = self.border_width() as i32;
        let tbh = self.title_bar_height() as i32;
        (self.x + bw, self.y + tbh, self.width, self.height)
    }

    /// Check if a point (screen coordinates) is inside the window bounds.
    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x + self.total_width() as i32
            && py < self.y + self.total_height() as i32
    }

    /// Check if a point is in the title bar area.
    pub fn in_title_bar(&self, px: i32, py: i32) -> bool {
        if !self.decorations {
            return false;
        }
        px >= self.x
            && py >= self.y
            && px < self.x + self.total_width() as i32
            && py < self.y + self.title_bar_height() as i32
    }

    /// Check if a point is in the close button area.
    pub fn in_close_button(&self, px: i32, py: i32) -> bool {
        if !self.decorations {
            return false;
        }
        // Close button is 16x16 in top-right corner of title bar
        let btn_x = self.x + self.total_width() as i32 - 20;
        let btn_y = self.y + 4;
        px >= btn_x && py >= btn_y && px < btn_x + 16 && py < btn_y + 16
    }
}

/// The window manager - manages all windows.
pub struct WindowManager {
    /// Window storage (None = empty slot)
    windows: [Option<Window>; MAX_WINDOWS],
    /// Z-order: window IDs from back to front
    z_order: [WindowId; MAX_WINDOWS],
    /// Number of windows in z_order
    z_count: usize,
    /// Currently focused window ID (WINDOW_ID_NONE = no focus)
    focused_id: WindowId,
    /// Initialized flag
    initialized: bool,
}

/// Global window manager instance.
static mut WINDOW_MANAGER: WindowManager = WindowManager::new_const();

impl WindowManager {
    /// Create a new window manager (const for static initialization).
    pub const fn new_const() -> Self {
        Self {
            windows: [const { None }; MAX_WINDOWS],
            z_order: [WINDOW_ID_NONE; MAX_WINDOWS],
            z_count: 0,
            focused_id: WINDOW_ID_NONE,
            initialized: false,
        }
    }

    /// Initialize the window manager.
    pub fn init(&mut self) {
        self.windows = [const { None }; MAX_WINDOWS];
        self.z_order = [WINDOW_ID_NONE; MAX_WINDOWS];
        self.z_count = 0;
        self.focused_id = WINDOW_ID_NONE;
        self.initialized = true;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_WINDOW_MANAGER_INIT:ok\n");
        }
    }

    /// Create a new window.
    pub fn create_window(
        &mut self,
        title: &[u8],
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        window_type: WindowType,
    ) -> Option<WindowId> {
        // Find empty slot
        let slot = self.windows.iter().position(|w| w.is_none())?;

        let window = Window::new(title, x, y, width, height, window_type);
        let id = window.id;

        self.windows[slot] = Some(window);

        // Add to z-order
        if self.z_count < MAX_WINDOWS {
            // Desktop goes to back, others to front
            if window_type == WindowType::Desktop {
                // Shift everything up and insert at 0
                for i in (0..self.z_count).rev() {
                    self.z_order[i + 1] = self.z_order[i];
                }
                self.z_order[0] = id;
            } else {
                self.z_order[self.z_count] = id;
            }
            self.z_count += 1;
        }

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_WINDOW_CREATED:");
            crate::serial_write_hex_u64(id as u64);
            crate::serial_write_str("\n");
        }

        Some(id)
    }

    /// Destroy a window by ID.
    pub fn destroy_window(&mut self, id: WindowId) -> bool {
        // Find and remove from windows array
        let slot = self.windows.iter().position(|w| {
            w.as_ref().map(|win| win.id == id).unwrap_or(false)
        });

        if let Some(slot) = slot {
            self.windows[slot] = None;

            // Remove from z-order
            if let Some(z_pos) = self.z_order[..self.z_count].iter().position(|&zid| zid == id) {
                for i in z_pos..self.z_count - 1 {
                    self.z_order[i] = self.z_order[i + 1];
                }
                self.z_order[self.z_count - 1] = WINDOW_ID_NONE;
                self.z_count -= 1;
            }

            // Clear focus if this was focused
            if self.focused_id == id {
                self.focused_id = WINDOW_ID_NONE;
            }

            #[cfg(feature = "serial_debug")]
            {
                crate::serial_write_str("RAYOS_UI_WINDOW_DESTROYED:");
                crate::serial_write_hex_u64(id as u64);
                crate::serial_write_str("\n");
            }

            return true;
        }
        false
    }

    /// Get a reference to a window by ID.
    pub fn get_window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find_map(|w| {
            w.as_ref().filter(|win| win.id == id)
        })
    }

    /// Get a mutable reference to a window by ID.
    pub fn get_window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find_map(|w| {
            w.as_mut().filter(|win| win.id == id)
        })
    }

    /// Set focus to a window.
    pub fn set_focus(&mut self, id: WindowId) {
        // Clear focus from previous window
        if let Some(prev) = self.get_window_mut(self.focused_id) {
            prev.focused = false;
        }

        // Set focus to new window
        if let Some(win) = self.get_window_mut(id) {
            win.focused = true;
            self.focused_id = id;

            #[cfg(feature = "serial_debug")]
            {
                crate::serial_write_str("RAYOS_UI_WINDOW_FOCUSED:");
                crate::serial_write_hex_u64(id as u64);
                crate::serial_write_str("\n");
            }
        }
    }

    /// Get the focused window ID.
    pub fn get_focused(&self) -> WindowId {
        self.focused_id
    }

    /// Get the focused window.
    pub fn get_focused_window(&self) -> Option<&Window> {
        self.get_window(self.focused_id)
    }

    /// Raise a window to the front (top of z-order).
    pub fn raise_window(&mut self, id: WindowId) {
        // Don't raise desktop
        if let Some(win) = self.get_window(id) {
            if win.window_type == WindowType::Desktop {
                return;
            }
        }

        // Find in z-order and move to end
        if let Some(pos) = self.z_order[..self.z_count].iter().position(|&zid| zid == id) {
            for i in pos..self.z_count - 1 {
                self.z_order[i] = self.z_order[i + 1];
            }
            self.z_order[self.z_count - 1] = id;
        }
    }

    /// Move a window to a new position.
    pub fn move_window(&mut self, id: WindowId, x: i32, y: i32) {
        if let Some(win) = self.get_window_mut(id) {
            win.x = x;
            win.y = y;
        }
    }

    /// Resize a window.
    pub fn resize_window(&mut self, id: WindowId, width: u32, height: u32) {
        if let Some(win) = self.get_window_mut(id) {
            win.width = width.max(win.min_width);
            win.height = height.max(win.min_height);
        }
    }

    /// Find the topmost window at a given point.
    pub fn window_at_point(&self, x: i32, y: i32) -> Option<WindowId> {
        // Iterate z-order from front to back
        for i in (0..self.z_count).rev() {
            let id = self.z_order[i];
            if let Some(win) = self.get_window(id) {
                if win.visible && win.contains_point(x, y) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Iterate windows in z-order (back to front) for rendering.
    pub fn iter_z_order(&self) -> impl Iterator<Item = &Window> {
        self.z_order[..self.z_count]
            .iter()
            .filter_map(|&id| self.get_window(id))
    }

    /// Get window count.
    pub fn window_count(&self) -> usize {
        self.windows.iter().filter(|w| w.is_some()).count()
    }

    /// Get the focused window ID.
    pub fn focused_window(&self) -> Option<WindowId> {
        if self.focused_id != WINDOW_ID_NONE {
            Some(self.focused_id)
        } else {
            None
        }
    }

    /// Get the next window in z-order (for Alt+Tab).
    pub fn next_window(&self) -> Option<WindowId> {
        if self.z_count < 2 {
            return None;
        }

        // Find current focused position in z-order
        let focused_pos = self.z_order[..self.z_count]
            .iter()
            .position(|&id| id == self.focused_id);

        // Find next non-desktop window
        for offset in 1..self.z_count {
            let pos = match focused_pos {
                Some(p) => (p + self.z_count - offset) % self.z_count,
                None => self.z_count - 1 - offset,
            };
            let id = self.z_order[pos];
            if let Some(win) = self.get_window(id) {
                if win.window_type != WindowType::Desktop
                    && win.window_type != WindowType::Panel
                    && win.visible
                {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Get the previous window in z-order (for Alt+Shift+Tab).
    pub fn prev_window(&self) -> Option<WindowId> {
        if self.z_count < 2 {
            return None;
        }

        // Find current focused position in z-order
        let focused_pos = self.z_order[..self.z_count]
            .iter()
            .position(|&id| id == self.focused_id);

        // Find previous non-desktop window
        for offset in 1..self.z_count {
            let pos = match focused_pos {
                Some(p) => (p + offset) % self.z_count,
                None => offset,
            };
            let id = self.z_order[pos];
            if let Some(win) = self.get_window(id) {
                if win.window_type != WindowType::Desktop
                    && win.window_type != WindowType::Panel
                    && win.visible
                {
                    return Some(id);
                }
            }
        }
        None
    }
}

// ===== Global accessors =====

/// Initialize the global window manager.
pub fn init() {
    unsafe {
        WINDOW_MANAGER.init();
    }
}

/// Get a reference to the global window manager.
pub fn get() -> &'static WindowManager {
    unsafe { &WINDOW_MANAGER }
}

/// Get a mutable reference to the global window manager.
pub fn get_mut() -> &'static mut WindowManager {
    unsafe { &mut WINDOW_MANAGER }
}

/// Create a new window (convenience function).
pub fn create_window(
    title: &[u8],
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    window_type: WindowType,
) -> Option<WindowId> {
    get_mut().create_window(title, x, y, width, height, window_type)
}

/// Destroy a window (convenience function).
pub fn destroy_window(id: WindowId) -> bool {
    get_mut().destroy_window(id)
}

/// Set focus to a window (convenience function).
pub fn set_focus(id: WindowId) {
    get_mut().set_focus(id);
}

/// Raise a window to front (convenience function).
pub fn raise_window(id: WindowId) {
    get_mut().raise_window(id);
}
