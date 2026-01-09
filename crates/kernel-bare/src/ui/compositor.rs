//! Compositor for RayOS UI
//!
//! Composites all windows to the framebuffer in z-order.
//! Supports animated window transitions with alpha blending.

use super::renderer::{
    self, fill_rect, fill_rect_alpha, draw_rect, draw_text,
    COLOR_BACKGROUND, COLOR_WINDOW_BG, COLOR_TITLE_BAR, COLOR_TITLE_FOCUSED,
    COLOR_TEXT, COLOR_BORDER, COLOR_CLOSE_HOVER, COLOR_ACCENT, COLOR_TEXT_DIM,
    FONT_HEIGHT,
};
use super::window_manager::{self, Window, WindowType, WINDOW_ID_NONE};
use super::animation::{self, AnimatedProperties, AnimationCallback};
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
    /// Whether animations are enabled
    animations_enabled: bool,
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
            animations_enabled: true,
        }
    }

    /// Initialize the compositor.
    pub fn init(&mut self) {
        self.dirty = true;
        self.frame_count = 0;
        self.initialized = true;
        self.animations_enabled = true;  // Performance optimized with bit-shift blending

        // Initialize animation system (but it starts disabled)
        animation::init();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_COMPOSITOR_INIT:ok\n");
        }
    }

    /// Enable or disable window animations.
    pub fn set_animations_enabled(&mut self, enabled: bool) {
        self.animations_enabled = enabled;
        animation::set_enabled(enabled);
    }

    /// Check if animations are enabled.
    pub fn animations_enabled(&self) -> bool {
        self.animations_enabled
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

        // Tick animations and handle callbacks
        let callbacks = animation::tick();
        for (window_id, callback) in callbacks.iter() {
            if *window_id == 0 {
                break;
            }
            match callback {
                AnimationCallback::DestroyWindow => {
                    window_manager::get_mut().destroy_window(*window_id);
                }
                AnimationCallback::HideWindow => {
                    if let Some(win) = window_manager::get_mut().get_window_mut(*window_id) {
                        win.visible = false;
                    }
                }
                AnimationCallback::ShowWindow => {
                    if let Some(win) = window_manager::get_mut().get_window_mut(*window_id) {
                        win.visible = true;
                    }
                }
                AnimationCallback::FocusWindow => {
                    window_manager::get_mut().set_focus(*window_id);
                }
                AnimationCallback::None => {}
            }
        }

        // If animations are active, we need to keep redrawing
        if animation::has_active() {
            self.dirty = true;
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

        // Render each window (with animation support)
        for i in 0..count {
            let id = window_ids[i];
            if let Some(win) = wm.get_window(id) {
                // Check if this window is being animated
                if let Some(anim_props) = animation::get_animated_properties(id) {
                    // Render with animated properties
                    self.render_window_animated(win, &anim_props);
                } else if win.visible {
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

    /// Render a window with animated properties (position, size, opacity).
    fn render_window_animated(&self, win: &Window, props: &AnimatedProperties) {
        // Skip if fully transparent
        if props.opacity == 0 {
            return;
        }

        // For now, render with alpha blending at the animated position
        // Full animation rendering with scaling would require more complex compositing
        match win.window_type {
            WindowType::Desktop => {
                // Desktop doesn't animate
            }
            WindowType::Normal | WindowType::Dialog | WindowType::VmSurface => {
                self.render_decorated_window_alpha(win, props);
            }
            WindowType::Panel => {
                self.render_panel_alpha(win, props);
            }
            WindowType::Popup => {
                self.render_popup_alpha(win, props);
            }
        }
    }

    /// Render a decorated window with animation properties.
    fn render_decorated_window_alpha(&self, win: &Window, props: &AnimatedProperties) {
        let x = props.x;
        let y = props.y;
        let alpha = props.opacity;

        // Use animated dimensions if scaling is applied, otherwise use window dimensions
        let (w, h) = if props.scale < 1000 {
            (props.width.max(1), props.height.max(1))
        } else {
            (win.width, win.height)
        };

        // Calculate total dimensions with decorations
        let bw = win.border_width();
        let tbh = win.title_bar_height();
        let tw = w + 2 * bw;
        let th = h + tbh + bw;

        // Window border (alpha blended)
        // Draw as four rectangles
        fill_rect_alpha(x, y, tw, bw, COLOR_BORDER, alpha); // Top
        fill_rect_alpha(x, y + th as i32 - bw as i32, tw, bw, COLOR_BORDER, alpha); // Bottom
        fill_rect_alpha(x, y + bw as i32, bw, th - 2 * bw, COLOR_BORDER, alpha); // Left
        fill_rect_alpha(x + tw as i32 - bw as i32, y + bw as i32, bw, th - 2 * bw, COLOR_BORDER, alpha); // Right

        // Title bar background
        let title_color = if win.focused { COLOR_TITLE_FOCUSED } else { COLOR_TITLE_BAR };
        fill_rect_alpha(x + bw as i32, y + bw as i32, tw - 2 * bw, tbh - bw, title_color, alpha);

        // Title text (only if reasonably visible)
        if alpha > 128 {
            let title = win.get_title();
            if !title.is_empty() {
                let text_x = x + bw as i32 + 8;
                let text_y = y + bw as i32 + (tbh - bw - FONT_HEIGHT as u32) as i32 / 2 + 2;
                draw_text(text_x, text_y, title, COLOR_TEXT);
            }
        }

        // Close button (only if reasonably visible)
        if alpha > 128 {
            let btn_x = x + tw as i32 - CLOSE_BUTTON_SIZE as i32 - CLOSE_BUTTON_MARGIN as i32 - bw as i32;
            let btn_y = y + bw as i32 + (tbh - bw - CLOSE_BUTTON_SIZE) as i32 / 2;
            self.render_close_button(btn_x, btn_y, win.focused);
        }

        // Window content area
        let cx = x + bw as i32;
        let cy = y + tbh as i32;
        fill_rect_alpha(cx, cy, w, h, COLOR_WINDOW_BG, alpha);

        // Render window content (only if reasonably visible)
        if alpha > 64 {
            super::content::render_window_content(win, cx, cy, w, h);
        }
    }

    /// Render a panel with animation properties.
    fn render_panel_alpha(&self, win: &Window, props: &AnimatedProperties) {
        let alpha = props.opacity;
        fill_rect_alpha(props.x, props.y, props.width, props.height, COLOR_TITLE_BAR, alpha);

        if alpha > 128 {
            draw_text(props.x + 10, props.y + 8, b"RayOS", COLOR_ACCENT);
            draw_text(props.x + 100, props.y + 8, b"|", COLOR_BORDER);
            draw_text(props.x + 120, props.y + 8, b"Ready", COLOR_TEXT);
        }
    }

    /// Render a popup with animation properties.
    fn render_popup_alpha(&self, win: &Window, props: &AnimatedProperties) {
        let alpha = props.opacity;
        fill_rect_alpha(props.x, props.y, props.width, props.height, COLOR_WINDOW_BG, alpha);

        // Border (as four rectangles)
        fill_rect_alpha(props.x, props.y, props.width, 1, COLOR_BORDER, alpha);
        fill_rect_alpha(props.x, props.y + props.height as i32 - 1, props.width, 1, COLOR_BORDER, alpha);
        fill_rect_alpha(props.x, props.y, 1, props.height, COLOR_BORDER, alpha);
        fill_rect_alpha(props.x + props.width as i32 - 1, props.y, 1, props.height, COLOR_BORDER, alpha);
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
        // Render decorated window - content.rs handles the VM surface blitting
        self.render_decorated_window(win);
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

/// Get the current frame count.
pub fn frame_count() -> u64 {
    get().frame_count()
}

/// Composite all windows to the framebuffer.
pub fn composite() {
    get_mut().composite();
}
