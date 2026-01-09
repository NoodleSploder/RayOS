//! RayOS Desktop Shell
//!
//! Main integration point for the UI framework. Initializes all components
//! and provides the main tick loop for the UI.

use super::{renderer, window_manager, compositor, input};
use super::window_manager::WindowType;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Whether the shell has been initialized.
static SHELL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Tick counter for timing.
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Desktop window ID.
static DESKTOP_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

/// Initialize the UI shell.
///
/// This should be called once during system startup after the framebuffer
/// is available.
///
/// # Arguments
/// * `fb_addr` - Framebuffer physical/virtual address
/// * `fb_width` - Framebuffer width in pixels
/// * `fb_height` - Framebuffer height in pixels
/// * `fb_stride` - Framebuffer stride in pixels (not bytes)
pub fn ui_shell_init(fb_addr: u64, fb_width: usize, fb_height: usize, fb_stride: usize) {
    if SHELL_INITIALIZED.load(Ordering::Acquire) {
        return;
    }

    // Initialize renderer with framebuffer info
    renderer::init(fb_addr, fb_width, fb_height, fb_stride);

    // Initialize window manager
    window_manager::init();

    // Initialize compositor
    compositor::init();

    // Initialize input handler
    input::init(fb_width as i32, fb_height as i32);

    // Create the desktop window (background)
    let desktop_id = window_manager::create_window(
        b"Desktop",
        0,
        0,
        fb_width as u32,
        fb_height as u32,
        WindowType::Desktop,
    );

    if let Some(id) = desktop_id {
        DESKTOP_WINDOW_ID.store(id as u64, Ordering::Release);
    }

    // Create System Status window (left side)
    window_manager::create_window(
        b"System Status",
        40,
        60,
        380,
        300,
        WindowType::Normal,
    );

    // Create AI Chat window (right side, focused)
    if let Some(id) = window_manager::create_window(
        b"AI Assistant",
        450,
        60,
        400,
        350,
        WindowType::Normal,
    ) {
        // Focus the AI chat window
        window_manager::set_focus(id);
    }

    // Create a status bar at the bottom
    let status_bar_height = 28u32;
    window_manager::create_window(
        b"StatusBar",
        0,
        fb_height as i32 - status_bar_height as i32,
        fb_width as u32,
        status_bar_height,
        WindowType::Panel,
    );

    SHELL_INITIALIZED.store(true, Ordering::Release);

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_UI_SHELL_INIT:ok\n");
    }

    // Do initial composite
    compositor::composite();
}

/// Per-frame update tick.
///
/// Call this regularly from the main kernel loop.
pub fn ui_shell_tick() {
    if !SHELL_INITIALIZED.load(Ordering::Acquire) {
        return;
    }

    let tick = TICK_COUNT.fetch_add(1, Ordering::Relaxed);

    // Mark dirty periodically for now (every 60 ticks for ~60fps refresh)
    // Later this will be driven by actual changes
    if tick % 60 == 0 {
        compositor::mark_dirty();
    }

    // Composite if dirty
    if compositor::is_dirty() {
        compositor::composite();
    }

    // Update cursor display (draw on top of composited content)
    input::update_cursor();
}

/// Check if the shell is initialized.
pub fn is_initialized() -> bool {
    SHELL_INITIALIZED.load(Ordering::Acquire)
}

/// Get the current tick count.
pub fn get_tick_count() -> u64 {
    TICK_COUNT.load(Ordering::Relaxed)
}

/// Force a redraw.
pub fn invalidate() {
    compositor::mark_dirty();
}

/// Create a normal window.
pub fn create_window(title: &[u8], x: i32, y: i32, width: u32, height: u32) -> Option<u32> {
    let id = window_manager::create_window(title, x, y, width, height, WindowType::Normal)?;
    compositor::mark_dirty();
    Some(id)
}

/// Create a VM surface window.
pub fn create_vm_window(title: &[u8], x: i32, y: i32, width: u32, height: u32) -> Option<u32> {
    let id = window_manager::create_window(title, x, y, width, height, WindowType::VmSurface)?;
    compositor::mark_dirty();
    Some(id)
}

/// Close a window.
pub fn close_window(id: u32) {
    window_manager::destroy_window(id);
    compositor::mark_dirty();
}

/// Focus a window.
pub fn focus_window(id: u32) {
    window_manager::set_focus(id);
    window_manager::raise_window(id);
    compositor::mark_dirty();
}

/// Move a window.
pub fn move_window(id: u32, x: i32, y: i32) {
    window_manager::get_mut().move_window(id, x, y);
    compositor::mark_dirty();
}

/// Resize a window.
pub fn resize_window(id: u32, width: u32, height: u32) {
    window_manager::get_mut().resize_window(id, width, height);
    compositor::mark_dirty();
}

/// Handle a click at the given coordinates.
///
/// Returns true if the click was handled.
pub fn handle_click(x: i32, y: i32) -> bool {
    input::handle_mouse_button_down(x, y, 0, false)
}

/// Handle mouse button release.
pub fn handle_click_release(x: i32, y: i32) -> bool {
    input::handle_mouse_button_up(x, y, 0);
    true
}

/// Handle mouse move.
pub fn handle_mouse_move(x: i32, y: i32) -> bool {
    input::handle_mouse_move(x, y);
    true
}

/// Handle mouse delta movement (relative input).
pub fn handle_mouse_delta(dx: i32, dy: i32) {
    input::handle_mouse_delta(dx, dy);
}

/// Get current mouse position.
pub fn get_mouse_position() -> (i32, i32) {
    input::mouse_position()
}
