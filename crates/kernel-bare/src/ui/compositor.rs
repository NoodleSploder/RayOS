//! Compositor for RayOS UI
//!
//! Composites all windows to the framebuffer in z-order.

use super::renderer::{
    self, fill_rect, draw_rect, draw_text,
    COLOR_BACKGROUND, COLOR_WINDOW_BG, COLOR_TITLE_BAR, COLOR_TITLE_FOCUSED,
    COLOR_TEXT, COLOR_BORDER, COLOR_CLOSE_HOVER, COLOR_ACCENT, COLOR_TEXT_DIM,
    FONT_HEIGHT,
};
use super::window_manager::{self, Window, WindowType, WINDOW_ID_NONE};
use core::sync::atomic::{AtomicBool, Ordering};

/// Title bar height in pixels.
pub const TITLE_BAR_HEIGHT: u32 = 24;
/// Window border width in pixels.
pub const BORDER_WIDTH: u32 = 2;
/// Close button size.
pub const CLOSE_BUTTON_SIZE: u32 = 16;
/// Close button margin from edge.
pub const CLOSE_BUTTON_MARGIN: u32 = 4;
/// Maximize button size.
pub const MAXIMIZE_BUTTON_SIZE: u32 = 16;
/// Maximize button margin from close button.
pub const MAXIMIZE_BUTTON_MARGIN: u32 = 4;

/// Compositor state.
pub struct Compositor {
    /// Whether a redraw is needed
    dirty: bool,
    /// Frame counter
    frame_count: u64,
    /// Whether compositor is initialized
    initialized: bool,
}

/// Global compositor instance.
static mut COMPOSITOR: Compositor = Compositor::new_const();
static COMPOSITOR_DIRTY: AtomicBool = AtomicBool::new(true);

impl Compositor {
    /// Create a new compositor (const for static initialization).
    pub const fn new_const() -> Self {
        Self {
            dirty: true,
            frame_count: 0,
            initialized: false,
        }
    }

    /// Initialize the compositor.
    pub fn init(&mut self) {
        self.dirty = true;
        self.frame_count = 0;
        self.initialized = true;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_COMPOSITOR_INIT:ok\n");
        }
    }

    /// Mark the compositor as needing a redraw.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        COMPOSITOR_DIRTY.store(true, Ordering::Release);
    }

    /// Check if a redraw is needed.
    pub fn is_dirty(&self) -> bool {
        self.dirty || COMPOSITOR_DIRTY.load(Ordering::Acquire)
    }

    /// Composite all windows to the framebuffer.
    pub fn composite(&mut self) {
        if !self.initialized || !renderer::is_ready() {
            return;
        }

        // Render desktop background
        self.render_desktop();

        // Get window manager and iterate z-order
        let wm = window_manager::get();

        // Collect window IDs first to avoid borrow issues
        let mut window_ids = [WINDOW_ID_NONE; 16];
        let mut count = 0;
        for win in wm.iter_z_order() {
            if count < 16 {
                window_ids[count] = win.id;
                count += 1;
            }
        }

        // Render each window
        for i in 0..count {
            let id = window_ids[i];
            if let Some(win) = wm.get_window(id) {
                if win.visible {
                    self.render_window(win);
                }
            }
        }

        self.dirty = false;
        COMPOSITOR_DIRTY.store(false, Ordering::Release);
        self.frame_count += 1;

        #[cfg(feature = "serial_debug")]
        {
            if self.frame_count == 1 {
                crate::serial_write_str("RAYOS_UI_COMPOSITE:ok\n");
            }
        }
    }

    /// Render the desktop background.
    fn render_desktop(&self) {
        let (width, height) = renderer::get_dimensions();
        if width == 0 || height == 0 {
            return;
        }

        // Solid dark background
        fill_rect(0, 0, width as u32, height as u32, COLOR_BACKGROUND);

        // Draw a subtle gradient effect (optional - darker at top, lighter at bottom)
        // For now, just draw some accent lines
        fill_rect(0, 0, width as u32, 2, COLOR_ACCENT);
        fill_rect(0, height as i32 - 2, width as u32, 2, COLOR_ACCENT);
    }

    /// Render a single window.
    fn render_window(&self, win: &Window) {
        match win.window_type {
            WindowType::Desktop => {
                // Desktop is rendered by render_desktop, skip here
            }
            WindowType::Normal | WindowType::Dialog => {
                self.render_decorated_window(win);
            }
            WindowType::VmSurface => {
                self.render_vm_window(win);
            }
            WindowType::Panel => {
                self.render_panel(win);
            }
            WindowType::Popup => {
                self.render_popup(win);
            }
        }
    }

    /// Render a window with decorations (title bar, border).
    fn render_decorated_window(&self, win: &Window) {
        let x = win.x;
        let y = win.y;
        let tw = win.total_width();
        let th = win.total_height();
        let bw = win.border_width();
        let tbh = win.title_bar_height();

        // Window border
        draw_rect(x, y, tw, th, COLOR_BORDER, bw);

        // Title bar background
        let title_color = if win.focused { COLOR_TITLE_FOCUSED } else { COLOR_TITLE_BAR };
        fill_rect(x + bw as i32, y + bw as i32, tw - 2 * bw, tbh - bw, title_color);

        // Title text
        let title = win.get_title();
        if !title.is_empty() {
            let text_x = x + bw as i32 + 8;
            let text_y = y + bw as i32 + (tbh - bw - FONT_HEIGHT as u32) as i32 / 2 + 2;
            draw_text(text_x, text_y, title, COLOR_TEXT);
        }

        // Close button
        let btn_x = x + tw as i32 - CLOSE_BUTTON_SIZE as i32 - CLOSE_BUTTON_MARGIN as i32 - bw as i32;
        let btn_y = y + bw as i32 + (tbh - bw - CLOSE_BUTTON_SIZE) as i32 / 2;
        self.render_close_button(btn_x, btn_y, win.focused);

        // Maximize button (next to close button)
        let max_btn_x = btn_x - MAXIMIZE_BUTTON_SIZE as i32 - MAXIMIZE_BUTTON_MARGIN as i32;
        let max_btn_y = btn_y;
        self.render_maximize_button(max_btn_x, max_btn_y, win.focused);

        // Window content area
        let (cx, cy, cw, ch) = win.content_rect();
        fill_rect(cx, cy, cw, ch, COLOR_WINDOW_BG);

        // Render window content based on window type/title
        super::content::render_window_content(win, cx, cy, cw, ch);
    }

    /// Render the close button (X).
    fn render_close_button(&self, x: i32, y: i32, focused: bool) {
        let bg = if focused { COLOR_CLOSE_HOVER } else { COLOR_TITLE_BAR };
        fill_rect(x, y, CLOSE_BUTTON_SIZE, CLOSE_BUTTON_SIZE, bg);

        // Draw X
        let color = COLOR_TEXT;
        let cx = x + CLOSE_BUTTON_SIZE as i32 / 2;
        let cy = y + CLOSE_BUTTON_SIZE as i32 / 2;

        // Simple X pattern
        for i in 0..6 {
            renderer::draw_pixel(cx - 3 + i, cy - 3 + i, color);
            renderer::draw_pixel(cx + 3 - i, cy - 3 + i, color);
            renderer::draw_pixel(cx - 3 + i, cy - 2 + i, color);
            renderer::draw_pixel(cx + 3 - i, cy - 2 + i, color);
        }
    }

    /// Render the maximize button (square).
    fn render_maximize_button(&self, x: i32, y: i32, focused: bool) {
        let bg = if focused { COLOR_TITLE_FOCUSED } else { COLOR_TITLE_BAR };
        fill_rect(x, y, MAXIMIZE_BUTTON_SIZE, MAXIMIZE_BUTTON_SIZE, bg);

        // Draw square outline (maximize icon)
        let color = COLOR_TEXT;
        let margin = 3;
        let inner_x = x + margin;
        let inner_y = y + margin;
        let inner_size = (MAXIMIZE_BUTTON_SIZE - 2 * margin as u32) as i32;

        // Top edge
        for i in 0..inner_size {
            renderer::draw_pixel(inner_x + i, inner_y, color);
            renderer::draw_pixel(inner_x + i, inner_y + 1, color); // Thicker top
        }
        // Bottom edge
        for i in 0..inner_size {
            renderer::draw_pixel(inner_x + i, inner_y + inner_size - 1, color);
        }
        // Left edge
        for i in 0..inner_size {
            renderer::draw_pixel(inner_x, inner_y + i, color);
        }
        // Right edge
        for i in 0..inner_size {
            renderer::draw_pixel(inner_x + inner_size - 1, inner_y + i, color);
        }
    }

    /// Render a VM surface window.
    fn render_vm_window(&self, win: &Window) {
        // For now, render like a normal window
        // Later, this will blit the VM's framebuffer
        self.render_decorated_window(win);

        // Overlay "VM Surface" label
        let (cx, cy, _, _) = win.content_rect();
        draw_text(cx + 10, cy + 30, b"[VM Surface]", COLOR_ACCENT);
    }

    /// Render a panel (status bar).
    fn render_panel(&self, win: &Window) {
        // Panels have no decorations, just a background
        fill_rect(win.x, win.y, win.width, win.height, COLOR_TITLE_BAR);

        // Draw some placeholder content
        draw_text(win.x + 10, win.y + 8, b"RayOS", COLOR_ACCENT);
        draw_text(win.x + 100, win.y + 8, b"|", COLOR_BORDER);
        draw_text(win.x + 120, win.y + 8, b"Ready", COLOR_TEXT);
    }

    /// Render a popup window.
    fn render_popup(&self, win: &Window) {
        // Simple filled rect with border
        fill_rect(win.x, win.y, win.width, win.height, COLOR_WINDOW_BG);
        draw_rect(win.x, win.y, win.width, win.height, COLOR_BORDER, 1);
    }

    /// Get the frame count.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

// ===== Global accessors =====

/// Initialize the global compositor.
pub fn init() {
    unsafe {
        COMPOSITOR.init();
    }
}

/// Get a reference to the global compositor.
pub fn get() -> &'static Compositor {
    unsafe { &COMPOSITOR }
}

/// Get a mutable reference to the global compositor.
pub fn get_mut() -> &'static mut Compositor {
    unsafe { &mut COMPOSITOR }
}

/// Mark the compositor as needing a redraw.
pub fn mark_dirty() {
    COMPOSITOR_DIRTY.store(true, Ordering::Release);
}

/// Check if a redraw is needed.
pub fn is_dirty() -> bool {
    COMPOSITOR_DIRTY.load(Ordering::Acquire)
}

/// Composite all windows to the framebuffer.
pub fn composite() {
    get_mut().composite();
}
