//! RayOS Desktop Shell
//!
//! Main integration point for the UI framework. Initializes all components
//! and provides the main tick loop for the UI.

use super::{renderer, window_manager, compositor, input, animation};
use super::window_manager::WindowType;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Whether the shell has been initialized.
static SHELL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Tick counter for timing.
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Last frame tick - used for frame rate limiting.
static LAST_FRAME_TICK: AtomicU64 = AtomicU64::new(0);

/// Desktop window ID.
static DESKTOP_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

/// Linux Desktop window ID (0 = not created)
static LINUX_DESKTOP_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

/// Windows Desktop window ID (0 = not created)
static WINDOWS_DESKTOP_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

/// Process Explorer window ID (0 = not created)
static PROCESS_EXPLORER_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

/// System Log window ID (0 = not created)
static SYSTEM_LOG_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

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

    // Initialize System 1 reflex engine
    crate::system1::init();

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

    // Tick the reflex engine (System 1)
    input::tick_reflex_engine();

    // Increment our internal tick counter
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);

    // Use the kernel's TIMER_TICKS for real-time frame pacing
    // TIMER_TICKS increments at ~100Hz, so we target ~30fps (every 3 ticks)
    let now = crate::TIMER_TICKS.load(Ordering::Relaxed);
    let last_frame = LAST_FRAME_TICK.load(Ordering::Relaxed);

    // Frame rate limiting: ~30fps for smooth updates without tearing
    const MIN_FRAME_INTERVAL: u64 = 3;

    // Check if we need a refresh for dynamic content (Process Explorer, VM window, System Log)
    let process_explorer_id = PROCESS_EXPLORER_WINDOW_ID.load(Ordering::Relaxed) as u32;
    let linux_window_id = LINUX_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    let system_log_id = SYSTEM_LOG_WINDOW_ID.load(Ordering::Relaxed) as u32;
    let has_dynamic_window = process_explorer_id != 0 || linux_window_id != 0 || system_log_id != 0;

    // Mark dirty periodically for dynamic windows at ~10fps (every 10 timer ticks)
    if has_dynamic_window && (now / 10) != (last_frame / 10) {
        compositor::mark_dirty();
    }

    // Only composite if dirty AND enough time has passed since last frame
    let elapsed = now.saturating_sub(last_frame);
    let did_composite = if compositor::is_dirty() && elapsed >= MIN_FRAME_INTERVAL {
        compositor::composite();
        LAST_FRAME_TICK.store(now, Ordering::Relaxed);
        true
    } else {
        false
    };

    // Update cursor display (draw on top of composited content)
    // Update when we composited, or when cursor moved (at frame rate limit)
    if did_composite || (input::cursor_needs_update() && elapsed >= MIN_FRAME_INTERVAL) {
        input::update_cursor();
    }
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

    // Start pop-in animation
    animation::animate_pop_in(id, x, y, width, height);

    compositor::mark_dirty();
    Some(id)
}

/// Create a VM surface window.
pub fn create_vm_window(title: &[u8], x: i32, y: i32, width: u32, height: u32) -> Option<u32> {
    let id = window_manager::create_window(title, x, y, width, height, WindowType::VmSurface)?;

    // Start fade-in animation for VM windows (pop-in might be jarring for large surfaces)
    animation::animate_fade_in(id, x, y, width, height);

    compositor::mark_dirty();
    Some(id)
}

/// Close a window.
pub fn close_window(id: u32) {
    // Get window position for animation
    if let Some(win) = window_manager::get().get_window(id) {
        // Start pop-out animation (will destroy window on completion)
        animation::animate_pop_out(id, win.x, win.y, win.width, win.height);
        compositor::mark_dirty();
    } else {
        // Window not found, just destroy it
        window_manager::destroy_window(id);
        compositor::mark_dirty();
    }
}

/// Close a window immediately without animation.
pub fn close_window_immediate(id: u32) {
    animation::cancel_for_window(id);
    window_manager::destroy_window(id);
    compositor::mark_dirty();
}

/// Focus a window.
pub fn focus_window(id: u32) {
    window_manager::set_focus(id);
    window_manager::raise_window(id);
    compositor::mark_dirty();
}

/// Move a window (immediate, no animation).
pub fn move_window(id: u32, x: i32, y: i32) {
    window_manager::get_mut().move_window(id, x, y);
    compositor::mark_dirty();
}

/// Move a window with smooth animation.
pub fn move_window_animated(id: u32, to_x: i32, to_y: i32) {
    if let Some(win) = window_manager::get().get_window(id) {
        animation::animate_move(id, win.x, win.y, to_x, to_y, win.width, win.height);
        compositor::mark_dirty();
    }
}

/// Resize a window (immediate, no animation).
pub fn resize_window(id: u32, width: u32, height: u32) {
    window_manager::get_mut().resize_window(id, width, height);
    compositor::mark_dirty();
}

/// Resize a window with smooth animation.
pub fn resize_window_animated(id: u32, to_width: u32, to_height: u32) {
    if let Some(win) = window_manager::get().get_window(id) {
        animation::animate_resize(id, win.x, win.y, win.width, win.height, to_width, to_height);
        compositor::mark_dirty();
    }
}

/// Minimize a window to the dock.
pub fn minimize_window(id: u32, dock_x: i32, dock_y: i32) {
    if let Some(win) = window_manager::get().get_window(id) {
        animation::animate_minimize(id, win.x, win.y, win.width, win.height, dock_x, dock_y);
        compositor::mark_dirty();
    }
}

/// Restore a window from minimized state.
pub fn restore_window(id: u32, dock_x: i32, dock_y: i32) {
    if let Some(win) = window_manager::get().get_window(id) {
        animation::animate_restore(id, dock_x, dock_y, win.x, win.y, win.width, win.height);
        compositor::mark_dirty();
    }
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

// ===== Linux Desktop Window Management =====

/// Show the Linux Desktop window.
///
/// Creates the window if it doesn't exist, or shows/focuses it if hidden.
/// Returns the window ID.
pub fn show_linux_desktop() -> Option<u32> {
    if !SHELL_INITIALIZED.load(Ordering::Acquire) {
        return None;
    }

    let existing_id = LINUX_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;

    if existing_id != 0 {
        // Window exists - just focus and raise it
        let wm = window_manager::get_mut();
        if let Some(win) = wm.get_window_mut(existing_id) {
            win.visible = true;
        }
        window_manager::set_focus(existing_id);
        window_manager::raise_window(existing_id);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_LINUX_DESKTOP_SHOWN:");
            crate::serial_write_hex_u64(existing_id as u64);
            crate::serial_write_str("\n");
        }

        return Some(existing_id);
    }

    // Create a new Linux Desktop window
    // Position it centered and reasonably sized
    let (screen_w, screen_h) = renderer::get_dimensions();
    let win_w = (screen_w as u32).saturating_sub(300).min(640);
    let win_h = (screen_h as u32).saturating_sub(250).min(400);
    let win_x = ((screen_w as u32 - win_w) / 2) as i32;
    // Position with enough vertical space - the title bar starts at y, content below
    // Use y=100 to ensure clear visibility below the top accent bar
    let win_y = 100;

    if let Some(id) = window_manager::create_window(
        b"Linux Desktop",
        win_x,
        win_y,
        win_w,
        win_h,
        WindowType::VmSurface,
    ) {
        LINUX_DESKTOP_WINDOW_ID.store(id as u64, Ordering::Relaxed);
        window_manager::set_focus(id);
        window_manager::raise_window(id);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_LINUX_DESKTOP_CREATED:");
            crate::serial_write_hex_u64(id as u64);
            crate::serial_write_str("\n");
        }

        Some(id)
    } else {
        None
    }
}

/// Hide the Linux Desktop window (but keep it alive).
pub fn hide_linux_desktop() {
    let id = LINUX_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id != 0 {
        let wm = window_manager::get_mut();
        if let Some(win) = wm.get_window_mut(id) {
            win.visible = false;
        }
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UI_LINUX_DESKTOP_HIDDEN\n");
    }
}

/// Close the Linux Desktop window.
pub fn close_linux_desktop() {
    let id = LINUX_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id != 0 {
        window_manager::destroy_window(id);
        LINUX_DESKTOP_WINDOW_ID.store(0, Ordering::Relaxed);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UI_LINUX_DESKTOP_CLOSED\n");
    }
}

/// Check if Linux Desktop window is currently visible.
pub fn is_linux_desktop_visible() -> bool {
    let id = LINUX_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id == 0 {
        return false;
    }
    let wm = window_manager::get();
    if let Some(win) = wm.get_window(id) {
        win.visible
    } else {
        false
    }
}

/// Check if Linux Desktop window is focused.
pub fn is_linux_desktop_focused() -> bool {
    let id = LINUX_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id == 0 {
        return false;
    }
    window_manager::get().get_focused() == id
}

/// Get the Linux Desktop window ID (0 if not created).
pub fn linux_desktop_window_id() -> u32 {
    LINUX_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32
}

// ===== Windows Desktop Window Management =====

/// Show the Windows Desktop window.
///
/// Creates the window if it doesn't exist, or shows/focuses it if hidden.
/// Returns the window ID.
pub fn show_windows_desktop() -> Option<u32> {
    if !SHELL_INITIALIZED.load(Ordering::Acquire) {
        return None;
    }

    let existing_id = WINDOWS_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;

    if existing_id != 0 {
        // Window exists - just focus and raise it
        let wm = window_manager::get_mut();
        if let Some(win) = wm.get_window_mut(existing_id) {
            win.visible = true;
        }
        window_manager::set_focus(existing_id);
        window_manager::raise_window(existing_id);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_WINDOWS_DESKTOP_SHOWN:");
            crate::serial_write_hex_u64(existing_id as u64);
            crate::serial_write_str("\n");
        }

        return Some(existing_id);
    }

    // Create a new Windows Desktop window (VM Surface type)
    // Default to 1024x768 for Windows
    let (screen_w, screen_h) = renderer::get_dimensions();
    let win_w = 1024;
    let win_h = 768;
    // Center the window
    let win_x = ((screen_w as u32).saturating_sub(win_w) / 2) as i32;
    let win_y = ((screen_h as u32).saturating_sub(win_h) / 2) as i32;

    if let Some(id) = window_manager::create_window(
        b"Windows Desktop",
        win_x,
        win_y,
        win_w,
        win_h,
        WindowType::VmSurface,
    ) {
        WINDOWS_DESKTOP_WINDOW_ID.store(id as u64, Ordering::Relaxed);
        window_manager::set_focus(id);
        window_manager::raise_window(id);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_WINDOWS_DESKTOP_CREATED:");
            crate::serial_write_hex_u64(id as u64);
            crate::serial_write_str("\n");
        }

        Some(id)
    } else {
        None
    }
}

/// Hide the Windows Desktop window (keeps it in memory).
pub fn hide_windows_desktop() {
    let id = WINDOWS_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id != 0 {
        let wm = window_manager::get_mut();
        if let Some(win) = wm.get_window_mut(id) {
            win.visible = false;
        }
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UI_WINDOWS_DESKTOP_HIDDEN\n");
    }
}

/// Close the Windows Desktop window completely.
pub fn close_windows_desktop() {
    let id = WINDOWS_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id != 0 {
        window_manager::destroy_window(id);
        WINDOWS_DESKTOP_WINDOW_ID.store(0, Ordering::Relaxed);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UI_WINDOWS_DESKTOP_CLOSED\n");
    }
}

/// Check if Windows Desktop window is currently visible.
pub fn is_windows_desktop_visible() -> bool {
    let id = WINDOWS_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id == 0 {
        return false;
    }
    let wm = window_manager::get();
    if let Some(win) = wm.get_window(id) {
        win.visible
    } else {
        false
    }
}

/// Check if Windows Desktop window is focused.
pub fn is_windows_desktop_focused() -> bool {
    let id = WINDOWS_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id == 0 {
        return false;
    }
    window_manager::get().get_focused() == id
}

/// Get the Windows Desktop window ID (0 if not created).
pub fn windows_desktop_window_id() -> u32 {
    WINDOWS_DESKTOP_WINDOW_ID.load(Ordering::Relaxed) as u32
}

// ===== Process Explorer Window Management =====

/// Show the Process Explorer window.
///
/// Creates the window if it doesn't exist, or shows/focuses it if hidden.
/// Returns the window ID.
pub fn show_process_explorer() -> Option<u32> {
    if !SHELL_INITIALIZED.load(Ordering::Acquire) {
        return None;
    }

    let existing_id = PROCESS_EXPLORER_WINDOW_ID.load(Ordering::Relaxed) as u32;

    if existing_id != 0 {
        // Window exists - just focus and raise it
        let wm = window_manager::get_mut();
        if let Some(win) = wm.get_window_mut(existing_id) {
            win.visible = true;
        }
        window_manager::set_focus(existing_id);
        window_manager::raise_window(existing_id);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_PROCESS_EXPLORER_SHOWN:");
            crate::serial_write_hex_u64(existing_id as u64);
            crate::serial_write_str("\n");
        }

        return Some(existing_id);
    }

    // Create a new Process Explorer window
    // Position it centered and reasonably sized
    let (screen_w, screen_h) = renderer::get_dimensions();
    let win_w = 460;
    let win_h = 400;
    let win_x = ((screen_w as u32 - win_w) / 2) as i32;
    let win_y = ((screen_h as u32 - win_h) / 2) as i32;

    if let Some(id) = window_manager::create_window(
        b"Process Explorer",
        win_x,
        win_y,
        win_w,
        win_h,
        WindowType::Normal,
    ) {
        PROCESS_EXPLORER_WINDOW_ID.store(id as u64, Ordering::Relaxed);
        window_manager::set_focus(id);
        window_manager::raise_window(id);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_PROCESS_EXPLORER_CREATED:");
            crate::serial_write_hex_u64(id as u64);
            crate::serial_write_str("\n");
        }

        Some(id)
    } else {
        None
    }
}

/// Close the Process Explorer window.
pub fn close_process_explorer() {
    let id = PROCESS_EXPLORER_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id != 0 {
        window_manager::destroy_window(id);
        PROCESS_EXPLORER_WINDOW_ID.store(0, Ordering::Relaxed);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UI_PROCESS_EXPLORER_CLOSED\n");
    }
}

/// Check if Process Explorer window is currently visible.
pub fn is_process_explorer_visible() -> bool {
    let id = PROCESS_EXPLORER_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id == 0 {
        return false;
    }
    let wm = window_manager::get();
    if let Some(win) = wm.get_window(id) {
        win.visible
    } else {
        false
    }
}

// ===== System Log Window Management =====

/// Show the System Log window.
///
/// Creates the window if it doesn't exist, or shows/focuses it if hidden.
/// Returns the window ID.
pub fn show_system_log() -> Option<u32> {
    if !SHELL_INITIALIZED.load(Ordering::Acquire) {
        return None;
    }

    let existing_id = SYSTEM_LOG_WINDOW_ID.load(Ordering::Relaxed) as u32;

    if existing_id != 0 {
        // Window exists - just focus and raise it
        let wm = window_manager::get_mut();
        if let Some(win) = wm.get_window_mut(existing_id) {
            win.visible = true;
        }
        window_manager::set_focus(existing_id);
        window_manager::raise_window(existing_id);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_SYSTEM_LOG_SHOWN:");
            crate::serial_write_hex_u64(existing_id as u64);
            crate::serial_write_str("\n");
        }

        return Some(existing_id);
    }

    // Create a new System Log window
    // Position it at bottom-left, wide enough for log entries
    let (_screen_w, screen_h) = renderer::get_dimensions();
    let win_w = 550;
    let win_h = 350;
    let win_x = 40;
    let win_y = (screen_h as i32 - win_h as i32 - 60).max(100);

    if let Some(id) = window_manager::create_window(
        b"System Log",
        win_x,
        win_y,
        win_w,
        win_h,
        WindowType::Normal,
    ) {
        SYSTEM_LOG_WINDOW_ID.store(id as u64, Ordering::Relaxed);
        window_manager::set_focus(id);
        window_manager::raise_window(id);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_SYSTEM_LOG_CREATED:");
            crate::serial_write_hex_u64(id as u64);
            crate::serial_write_str("\n");
        }

        // Log that we opened the log window
        crate::syslog::info(crate::syslog::SUBSYSTEM_UI, b"System Log window opened");

        Some(id)
    } else {
        None
    }
}

/// Close the System Log window.
pub fn close_system_log() {
    let id = SYSTEM_LOG_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id != 0 {
        window_manager::destroy_window(id);
        SYSTEM_LOG_WINDOW_ID.store(0, Ordering::Relaxed);
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("RAYOS_UI_SYSTEM_LOG_CLOSED\n");
    }
}

/// Check if System Log window is currently visible.
pub fn is_system_log_visible() -> bool {
    let id = SYSTEM_LOG_WINDOW_ID.load(Ordering::Relaxed) as u32;
    if id == 0 {
        return false;
    }
    let wm = window_manager::get();
    if let Some(win) = wm.get_window(id) {
        win.visible
    } else {
        false
    }
}
