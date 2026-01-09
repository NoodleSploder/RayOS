//! UI Input Handler for RayOS
//!
//! Handles mouse and keyboard input for the windowed UI system.
//! Implements window dragging, resizing, focus, and maximize/restore.

use super::renderer::{self, CursorType};
use super::window_manager::{self, Window, WindowId, WindowType, WINDOW_ID_NONE};
use super::compositor;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};

// ===== Mouse State =====

/// Current mouse X position
static MOUSE_X: AtomicI32 = AtomicI32::new(0);
/// Current mouse Y position
static MOUSE_Y: AtomicI32 = AtomicI32::new(0);
/// Mouse button state (bit 0 = left, bit 1 = middle, bit 2 = right)
static MOUSE_BUTTONS: AtomicU32 = AtomicU32::new(0);
/// Last click timestamp (for double-click detection)
static LAST_CLICK_TIME: AtomicU64 = AtomicU64::new(0);
/// Last click position X
static LAST_CLICK_X: AtomicI32 = AtomicI32::new(0);
/// Last click position Y
static LAST_CLICK_Y: AtomicI32 = AtomicI32::new(0);
/// Input handler initialized
static INPUT_INITIALIZED: AtomicBool = AtomicBool::new(false);
/// Current cursor type
static CURRENT_CURSOR: AtomicU32 = AtomicU32::new(0);

// ===== Drag State =====

/// Currently dragging window ID (WINDOW_ID_NONE = not dragging)
static DRAG_WINDOW_ID: AtomicU32 = AtomicU32::new(WINDOW_ID_NONE);
/// Drag operation type
static DRAG_OPERATION: AtomicU32 = AtomicU32::new(0);
/// Drag start X (mouse position when drag started)
static DRAG_START_X: AtomicI32 = AtomicI32::new(0);
/// Drag start Y
static DRAG_START_Y: AtomicI32 = AtomicI32::new(0);
/// Window position when drag started (for move)
static DRAG_WIN_X: AtomicI32 = AtomicI32::new(0);
static DRAG_WIN_Y: AtomicI32 = AtomicI32::new(0);
/// Window size when drag started (for resize)
static DRAG_WIN_W: AtomicU32 = AtomicU32::new(0);
static DRAG_WIN_H: AtomicU32 = AtomicU32::new(0);

// ===== Text Input State =====

/// Window ID that has text input focus (WINDOW_ID_NONE = no text focus)
static TEXT_INPUT_WINDOW: AtomicU32 = AtomicU32::new(WINDOW_ID_NONE);
/// Text input buffer (256 bytes max)
static mut TEXT_INPUT_BUFFER: [u8; 256] = [0u8; 256];
/// Current text input length
static TEXT_INPUT_LEN: AtomicU32 = AtomicU32::new(0);
/// Cursor position in text input
static TEXT_INPUT_CURSOR: AtomicU32 = AtomicU32::new(0);
/// Text input is active (blinking cursor, etc.)
static TEXT_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Drag operation types
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum DragOp {
    None = 0,
    Move = 1,
    ResizeN = 2,
    ResizeS = 3,
    ResizeE = 4,
    ResizeW = 5,
    ResizeNW = 6,
    ResizeNE = 7,
    ResizeSW = 8,
    ResizeSE = 9,
}

impl From<u32> for DragOp {
    fn from(v: u32) -> Self {
        match v {
            1 => DragOp::Move,
            2 => DragOp::ResizeN,
            3 => DragOp::ResizeS,
            4 => DragOp::ResizeE,
            5 => DragOp::ResizeW,
            6 => DragOp::ResizeNW,
            7 => DragOp::ResizeNE,
            8 => DragOp::ResizeSW,
            9 => DragOp::ResizeSE,
            _ => DragOp::None,
        }
    }
}

// ===== Resize Edge Detection =====

/// Edge size for resize grab (in pixels)
const RESIZE_EDGE: i32 = 6;

/// Which edge(s) is the point near?
fn detect_resize_edge(win: &Window, px: i32, py: i32) -> DragOp {
    if !win.decorations {
        return DragOp::None;
    }

    let x = win.x;
    let y = win.y;
    let w = win.total_width() as i32;
    let h = win.total_height() as i32;

    // Check corners first (they take priority)
    let on_left = px >= x && px < x + RESIZE_EDGE;
    let on_right = px >= x + w - RESIZE_EDGE && px < x + w;
    let on_top = py >= y && py < y + RESIZE_EDGE;
    let on_bottom = py >= y + h - RESIZE_EDGE && py < y + h;

    if on_top && on_left { return DragOp::ResizeNW; }
    if on_top && on_right { return DragOp::ResizeNE; }
    if on_bottom && on_left { return DragOp::ResizeSW; }
    if on_bottom && on_right { return DragOp::ResizeSE; }
    if on_top { return DragOp::ResizeN; }
    if on_bottom { return DragOp::ResizeS; }
    if on_left { return DragOp::ResizeW; }
    if on_right { return DragOp::ResizeE; }

    DragOp::None
}

/// Get cursor type for a resize operation
fn cursor_for_drag_op(op: DragOp) -> CursorType {
    match op {
        DragOp::Move => CursorType::Move,
        DragOp::ResizeN | DragOp::ResizeS => CursorType::ResizeV,
        DragOp::ResizeE | DragOp::ResizeW => CursorType::ResizeH,
        DragOp::ResizeNW | DragOp::ResizeSE => CursorType::ResizeNWSE,
        DragOp::ResizeNE | DragOp::ResizeSW => CursorType::ResizeNESW,
        DragOp::None => CursorType::Arrow,
    }
}

// ===== Maximize State =====

/// Saved window positions before maximize (for restore)
/// Array of (id, x, y, w, h) tuples
static mut SAVED_POSITIONS: [(WindowId, i32, i32, u32, u32); 16] = [(0, 0, 0, 0, 0); 16];

/// Check if window is maximized (covers full screen)
fn is_window_maximized(win: &Window) -> bool {
    let (screen_w, screen_h) = renderer::get_dimensions();
    // Consider maximized if window covers almost full screen
    win.x <= 0 && win.y <= 0
        && win.total_width() >= screen_w as u32 - 10
        && win.total_height() >= screen_h as u32 - 40 // Leave room for panel
}

/// Save window position before maximize
fn save_window_position(win: &Window) {
    unsafe {
        for slot in SAVED_POSITIONS.iter_mut() {
            if slot.0 == win.id || slot.0 == WINDOW_ID_NONE {
                *slot = (win.id, win.x, win.y, win.width, win.height);
                return;
            }
        }
    }
}

/// Get saved position for window
fn get_saved_position(id: WindowId) -> Option<(i32, i32, u32, u32)> {
    unsafe {
        for slot in SAVED_POSITIONS.iter() {
            if slot.0 == id {
                return Some((slot.1, slot.2, slot.3, slot.4));
            }
        }
    }
    None
}

/// Clear saved position
fn clear_saved_position(id: WindowId) {
    unsafe {
        for slot in SAVED_POSITIONS.iter_mut() {
            if slot.0 == id {
                *slot = (WINDOW_ID_NONE, 0, 0, 0, 0);
                return;
            }
        }
    }
}

// ===== Public API =====

/// Screen dimensions for clamping
static SCREEN_WIDTH: AtomicI32 = AtomicI32::new(1024);
static SCREEN_HEIGHT: AtomicI32 = AtomicI32::new(768);

/// Initialize the input handler
pub fn init(screen_w: i32, screen_h: i32) {
    if INPUT_INITIALIZED.load(Ordering::Acquire) {
        return;
    }

    SCREEN_WIDTH.store(screen_w, Ordering::Relaxed);
    SCREEN_HEIGHT.store(screen_h, Ordering::Relaxed);

    // Center cursor initially
    MOUSE_X.store(screen_w / 2, Ordering::Relaxed);
    MOUSE_Y.store(screen_h / 2, Ordering::Relaxed);

    INPUT_INITIALIZED.store(true, Ordering::Release);

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_UI_INPUT_INIT:ok\n");
    }
}

/// Get current mouse position
pub fn mouse_position() -> (i32, i32) {
    (MOUSE_X.load(Ordering::Relaxed), MOUSE_Y.load(Ordering::Relaxed))
}

/// Get current cursor type
pub fn current_cursor() -> CursorType {
    let v = CURRENT_CURSOR.load(Ordering::Relaxed);
    match v {
        1 => CursorType::ResizeH,
        2 => CursorType::ResizeV,
        3 => CursorType::ResizeNWSE,
        4 => CursorType::ResizeNESW,
        5 => CursorType::Move,
        _ => CursorType::Arrow,
    }
}

/// Handle mouse movement (absolute coordinates)
pub fn handle_mouse_move(x: i32, y: i32) {
    let (screen_w, screen_h) = renderer::get_dimensions();

    // Clamp to screen bounds
    let x = x.max(0).min(screen_w as i32 - 1);
    let y = y.max(0).min(screen_h as i32 - 1);

    MOUSE_X.store(x, Ordering::Relaxed);
    MOUSE_Y.store(y, Ordering::Relaxed);

    // Handle active drag
    let drag_id = DRAG_WINDOW_ID.load(Ordering::Relaxed);
    if drag_id != WINDOW_ID_NONE {
        handle_drag_motion(x, y);
        compositor::mark_dirty();
        return;
    }

    // Update cursor based on what's under it
    let wm = window_manager::get();
    if let Some(id) = wm.window_at_point(x, y) {
        if let Some(win) = wm.get_window(id) {
            // Check for resize edge
            let edge_op = detect_resize_edge(win, x, y);
            if edge_op != DragOp::None {
                set_cursor(cursor_for_drag_op(edge_op));
                return;
            }

            // Check for title bar (move cursor)
            if win.in_title_bar(x, y) && !win.in_close_button(x, y) {
                set_cursor(CursorType::Arrow); // Normal cursor in title bar
                return;
            }
        }
    }

    // Default arrow cursor
    set_cursor(CursorType::Arrow);
}

/// Handle relative mouse movement (delta)
pub fn handle_mouse_delta(dx: i32, dy: i32) {
    let x = MOUSE_X.load(Ordering::Relaxed) + dx;
    let y = MOUSE_Y.load(Ordering::Relaxed) + dy;
    handle_mouse_move(x, y);
}

/// Handle mouse button press
/// Returns true if the click was handled
pub fn handle_mouse_button_down(x: i32, y: i32, button: u32, is_double_click: bool) -> bool {
    // Set mouse position
    MOUSE_X.store(x, Ordering::Relaxed);
    MOUSE_Y.store(y, Ordering::Relaxed);

    // Update button state
    let old_buttons = MOUSE_BUTTONS.fetch_or(1 << button, Ordering::Relaxed);

    // Left button (0) special handling
    if button == 0 && (old_buttons & 1) == 0 {
        handle_left_click(x, y, is_double_click);
    }

    compositor::mark_dirty();
    true
}

/// Handle mouse button release
pub fn handle_mouse_button_up(x: i32, y: i32, button: u32) {
    // Set mouse position
    MOUSE_X.store(x, Ordering::Relaxed);
    MOUSE_Y.store(y, Ordering::Relaxed);

    // Update button state
    MOUSE_BUTTONS.fetch_and(!(1 << button), Ordering::Relaxed);

    // End drag if left button released
    if button == 0 {
        end_drag();
    }

    compositor::mark_dirty();
}

/// Check if left button is pressed
pub fn is_left_button_down() -> bool {
    (MOUSE_BUTTONS.load(Ordering::Relaxed) & 1) != 0
}

// ===== Internal Handlers =====

fn set_cursor(cursor: CursorType) {
    CURRENT_CURSOR.store(cursor as u32, Ordering::Relaxed);
}

fn handle_left_click(x: i32, y: i32, is_double_click: bool) {
    let wm = window_manager::get();

    // Find window under cursor
    if let Some(id) = wm.window_at_point(x, y) {
        if let Some(win) = wm.get_window(id) {
            // Check close button
            if win.in_close_button(x, y) {
                window_manager::destroy_window(id);
                compositor::mark_dirty();
                return;
            }

            // Check for maximize button (placeholder - 2nd button from right)
            if win.decorations && is_in_maximize_button(win, x, y) {
                toggle_maximize(id);
                return;
            }

            // Double-click on title bar = toggle maximize
            if is_double_click && win.in_title_bar(x, y) {
                toggle_maximize(id);
                return;
            }

            // Check for resize edge
            let edge_op = detect_resize_edge(win, x, y);
            if edge_op != DragOp::None {
                start_drag(id, edge_op, x, y);
                window_manager::set_focus(id);
                window_manager::raise_window(id);
                return;
            }

            // Check for title bar (start move)
            if win.in_title_bar(x, y) {
                start_drag(id, DragOp::Move, x, y);
                window_manager::set_focus(id);
                window_manager::raise_window(id);
                return;
            }

            // Check for text input area click (AI Assistant window)
            if check_text_input_click(win, id, x, y) {
                window_manager::set_focus(id);
                window_manager::raise_window(id);
                return;
            }

            // Click in window content - just focus, deactivate text input
            deactivate_text_input();
            window_manager::set_focus(id);
            window_manager::raise_window(id);
        }
    } else {
        // Clicked outside all windows - deactivate text input
        deactivate_text_input();
    }
}

fn is_in_maximize_button(win: &Window, px: i32, py: i32) -> bool {
    if !win.decorations {
        return false;
    }
    // Maximize button is 2nd from right (next to close button)
    let btn_x = win.x + win.total_width() as i32 - 40;
    let btn_y = win.y + 4;
    px >= btn_x && py >= btn_y && px < btn_x + 16 && py < btn_y + 16
}

fn toggle_maximize(id: WindowId) {
    let wm = window_manager::get();
    if let Some(win) = wm.get_window(id) {
        if is_window_maximized(win) {
            // Restore to saved position
            if let Some((x, y, w, h)) = get_saved_position(id) {
                let wm_mut = window_manager::get_mut();
                wm_mut.move_window(id, x, y);
                wm_mut.resize_window(id, w, h);
                clear_saved_position(id);
            }
        } else {
            // Save current position and maximize
            save_window_position(win);
            let (screen_w, screen_h) = renderer::get_dimensions();
            let wm_mut = window_manager::get_mut();
            wm_mut.move_window(id, 0, 0);
            // Leave room for panel at bottom
            wm_mut.resize_window(id, screen_w as u32 - 4, screen_h as u32 - 32);
        }
        compositor::mark_dirty();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_WINDOW_MAXIMIZE_TOGGLE:");
            crate::serial_write_hex_u64(id as u64);
            crate::serial_write_str("\n");
        }
    }
}

fn start_drag(id: WindowId, op: DragOp, start_x: i32, start_y: i32) {
    let wm = window_manager::get();
    if let Some(win) = wm.get_window(id) {
        DRAG_WINDOW_ID.store(id, Ordering::Relaxed);
        DRAG_OPERATION.store(op as u32, Ordering::Relaxed);
        DRAG_START_X.store(start_x, Ordering::Relaxed);
        DRAG_START_Y.store(start_y, Ordering::Relaxed);
        DRAG_WIN_X.store(win.x, Ordering::Relaxed);
        DRAG_WIN_Y.store(win.y, Ordering::Relaxed);
        DRAG_WIN_W.store(win.width, Ordering::Relaxed);
        DRAG_WIN_H.store(win.height, Ordering::Relaxed);

        set_cursor(cursor_for_drag_op(op));
    }
}

fn handle_drag_motion(x: i32, y: i32) {
    let id = DRAG_WINDOW_ID.load(Ordering::Relaxed);
    if id == WINDOW_ID_NONE {
        return;
    }

    let op = DragOp::from(DRAG_OPERATION.load(Ordering::Relaxed));
    let start_x = DRAG_START_X.load(Ordering::Relaxed);
    let start_y = DRAG_START_Y.load(Ordering::Relaxed);
    let win_x = DRAG_WIN_X.load(Ordering::Relaxed);
    let win_y = DRAG_WIN_Y.load(Ordering::Relaxed);
    let win_w = DRAG_WIN_W.load(Ordering::Relaxed);
    let win_h = DRAG_WIN_H.load(Ordering::Relaxed);

    let dx = x - start_x;
    let dy = y - start_y;

    let wm = window_manager::get_mut();

    match op {
        DragOp::Move => {
            wm.move_window(id, win_x + dx, win_y + dy);
        }
        DragOp::ResizeE => {
            let new_w = ((win_w as i32) + dx).max(100) as u32;
            wm.resize_window(id, new_w, win_h);
        }
        DragOp::ResizeS => {
            let new_h = ((win_h as i32) + dy).max(50) as u32;
            wm.resize_window(id, win_w, new_h);
        }
        DragOp::ResizeW => {
            let new_w = ((win_w as i32) - dx).max(100) as u32;
            let new_x = win_x + (win_w as i32 - new_w as i32);
            wm.move_window(id, new_x, win_y);
            wm.resize_window(id, new_w, win_h);
        }
        DragOp::ResizeN => {
            let new_h = ((win_h as i32) - dy).max(50) as u32;
            let new_y = win_y + (win_h as i32 - new_h as i32);
            wm.move_window(id, win_x, new_y);
            wm.resize_window(id, win_w, new_h);
        }
        DragOp::ResizeSE => {
            let new_w = ((win_w as i32) + dx).max(100) as u32;
            let new_h = ((win_h as i32) + dy).max(50) as u32;
            wm.resize_window(id, new_w, new_h);
        }
        DragOp::ResizeNW => {
            let new_w = ((win_w as i32) - dx).max(100) as u32;
            let new_h = ((win_h as i32) - dy).max(50) as u32;
            let new_x = win_x + (win_w as i32 - new_w as i32);
            let new_y = win_y + (win_h as i32 - new_h as i32);
            wm.move_window(id, new_x, new_y);
            wm.resize_window(id, new_w, new_h);
        }
        DragOp::ResizeNE => {
            let new_w = ((win_w as i32) + dx).max(100) as u32;
            let new_h = ((win_h as i32) - dy).max(50) as u32;
            let new_y = win_y + (win_h as i32 - new_h as i32);
            wm.move_window(id, win_x, new_y);
            wm.resize_window(id, new_w, new_h);
        }
        DragOp::ResizeSW => {
            let new_w = ((win_w as i32) - dx).max(100) as u32;
            let new_h = ((win_h as i32) + dy).max(50) as u32;
            let new_x = win_x + (win_w as i32 - new_w as i32);
            wm.move_window(id, new_x, win_y);
            wm.resize_window(id, new_w, new_h);
        }
        DragOp::None => {}
    }
}

fn end_drag() {
    DRAG_WINDOW_ID.store(WINDOW_ID_NONE, Ordering::Relaxed);
    DRAG_OPERATION.store(DragOp::None as u32, Ordering::Relaxed);
    set_cursor(CursorType::Arrow);

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_UI_DRAG_END:ok\n");
    }
}

/// Called each frame to update cursor display
pub fn update_cursor() {
    let x = MOUSE_X.load(Ordering::Relaxed);
    let y = MOUSE_Y.load(Ordering::Relaxed);
    let cursor_type = current_cursor();

    renderer::cursor_show(x, y, cursor_type);
}

// ===== Text Input Handling =====

/// Check if click is in a text input area and activate it.
/// Returns true if a text input was activated.
fn check_text_input_click(win: &Window, id: WindowId, x: i32, y: i32) -> bool {
    let title = win.get_title();

    // Only AI Assistant window has text input for now
    if title != b"AI Assistant" {
        return false;
    }

    // Get content area coordinates using content_rect
    let (cx, cy, _cw, ch) = win.content_rect();

    // Input area is at the bottom of the window (24px tall, with 8px margin)
    let input_y = cy + ch as i32 - 24;
    let input_x = cx + 8;
    let input_w = win.width as i32 - 26;
    let input_h = 20;

    // Check if click is within input area
    if x >= input_x && x < input_x + input_w && y >= input_y && y < input_y + input_h {
        activate_text_input(id);
        return true;
    }

    false
}

/// Activate text input for a window.
fn activate_text_input(window_id: WindowId) {
    TEXT_INPUT_WINDOW.store(window_id, Ordering::Relaxed);
    TEXT_INPUT_ACTIVE.store(true, Ordering::Relaxed);
    compositor::mark_dirty();

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_UI_TEXT_INPUT_ACTIVE:");
        crate::serial_write_hex_u64(window_id as u64);
        crate::serial_write_str("\n");
    }
}

/// Deactivate text input.
fn deactivate_text_input() {
    if TEXT_INPUT_ACTIVE.load(Ordering::Relaxed) {
        TEXT_INPUT_ACTIVE.store(false, Ordering::Relaxed);
        TEXT_INPUT_WINDOW.store(WINDOW_ID_NONE, Ordering::Relaxed);
        compositor::mark_dirty();
    }
}

/// Check if text input is currently active.
pub fn is_text_input_active() -> bool {
    TEXT_INPUT_ACTIVE.load(Ordering::Relaxed)
}

/// Get the current text input buffer contents.
pub fn get_text_input() -> &'static [u8] {
    let len = TEXT_INPUT_LEN.load(Ordering::Relaxed) as usize;
    unsafe { &TEXT_INPUT_BUFFER[..len] }
}

/// Get the text input cursor position.
pub fn get_text_cursor() -> usize {
    TEXT_INPUT_CURSOR.load(Ordering::Relaxed) as usize
}

/// Clear the text input buffer.
pub fn clear_text_input() {
    TEXT_INPUT_LEN.store(0, Ordering::Relaxed);
    TEXT_INPUT_CURSOR.store(0, Ordering::Relaxed);
    compositor::mark_dirty();
}

/// Handle a key press for text input.
/// Returns true if the key was consumed by text input.
pub fn handle_key_for_text_input(ascii: u8) -> bool {
    if !TEXT_INPUT_ACTIVE.load(Ordering::Relaxed) {
        return false;
    }

    let mut len = TEXT_INPUT_LEN.load(Ordering::Relaxed) as usize;
    let mut cursor = TEXT_INPUT_CURSOR.load(Ordering::Relaxed) as usize;

    match ascii {
        // Backspace
        0x08 | 0x7F => {
            if cursor > 0 {
                unsafe {
                    // Shift characters left
                    for i in cursor..len {
                        TEXT_INPUT_BUFFER[i - 1] = TEXT_INPUT_BUFFER[i];
                    }
                }
                len -= 1;
                cursor -= 1;
                TEXT_INPUT_LEN.store(len as u32, Ordering::Relaxed);
                TEXT_INPUT_CURSOR.store(cursor as u32, Ordering::Relaxed);
                compositor::mark_dirty();
            }
            true
        }
        // Enter - submit the input
        b'\n' | b'\r' => {
            if len > 0 {
                // Call the content module to handle submission
                super::content::handle_text_submit(get_text_input());
                clear_text_input();
            }
            true
        }
        // Escape - deactivate
        0x1B => {
            deactivate_text_input();
            true
        }
        // Printable ASCII
        0x20..=0x7E => {
            if len < 255 {
                unsafe {
                    // Shift characters right if not at end
                    for i in (cursor..len).rev() {
                        TEXT_INPUT_BUFFER[i + 1] = TEXT_INPUT_BUFFER[i];
                    }
                    TEXT_INPUT_BUFFER[cursor] = ascii;
                }
                len += 1;
                cursor += 1;
                TEXT_INPUT_LEN.store(len as u32, Ordering::Relaxed);
                TEXT_INPUT_CURSOR.store(cursor as u32, Ordering::Relaxed);
                compositor::mark_dirty();
            }
            true
        }
        _ => false,
    }
}

// ===== Keyboard-based Mouse Control =====
// When no actual mouse is available, these functions allow controlling
// the cursor via keyboard. Arrow keys move, Space clicks, etc.

/// Mouse movement step size in pixels
const MOUSE_STEP: i32 = 10;
/// Fast movement step (with shift)
const MOUSE_STEP_FAST: i32 = 40;

/// Handle a keyboard key for mouse control.
/// Returns true if the key was consumed for mouse control.
pub fn handle_key_for_mouse(ascii: u8, shift: bool) -> bool {
    if !INPUT_INITIALIZED.load(Ordering::Acquire) {
        return false;
    }

    let step = if shift { MOUSE_STEP_FAST } else { MOUSE_STEP };

    match ascii {
        // Arrow key equivalents (WASD or arrow scancodes mapped to ASCII)
        b'w' | b'W' => {
            // Up
            handle_mouse_delta(0, -step);
            true
        }
        b's' | b'S' => {
            // Down (but not if it's a shell command)
            handle_mouse_delta(0, step);
            true
        }
        b'a' | b'A' => {
            // Left
            handle_mouse_delta(-step, 0);
            true
        }
        b'd' | b'D' => {
            // Right
            handle_mouse_delta(step, 0);
            true
        }
        b' ' => {
            // Space = left click
            let (x, y) = mouse_position();
            if is_left_button_down() {
                handle_mouse_button_up(x, y, 0);
            } else {
                handle_mouse_button_down(x, y, 0, false);
            }
            true
        }
        b'\n' | b'\r' => {
            // Enter = left click (release immediately)
            let (x, y) = mouse_position();
            handle_mouse_button_down(x, y, 0, false);
            // Release after a short delay would be ideal, but for now just release
            handle_mouse_button_up(x, y, 0);
            true
        }
        b'm' | b'M' => {
            // M = toggle maximize on focused window
            let wm = window_manager::get();
            let id = wm.get_focused();
            if id != WINDOW_ID_NONE {
                toggle_maximize(id);
            }
            true
        }
        b'q' | b'Q' => {
            // Q = close focused window
            let wm = window_manager::get();
            let id = wm.get_focused();
            if id != WINDOW_ID_NONE {
                window_manager::destroy_window(id);
                compositor::mark_dirty();
            }
            true
        }
        _ => false,
    }
}

/// Handle scancode-based input for arrow keys
/// Set 1 scancodes: Up=0x48, Down=0x50, Left=0x4B, Right=0x4D
pub fn handle_scancode_for_mouse(scancode: u8, shift: bool) -> bool {
    if !INPUT_INITIALIZED.load(Ordering::Acquire) {
        return false;
    }

    // Only handle make codes (not break codes which have bit 7 set)
    if scancode & 0x80 != 0 {
        return false;
    }

    let step = if shift { MOUSE_STEP_FAST } else { MOUSE_STEP };

    match scancode {
        0x48 => {
            // Up arrow
            handle_mouse_delta(0, -step);
            true
        }
        0x50 => {
            // Down arrow
            handle_mouse_delta(0, step);
            true
        }
        0x4B => {
            // Left arrow
            handle_mouse_delta(-step, 0);
            true
        }
        0x4D => {
            // Right arrow
            handle_mouse_delta(step, 0);
            true
        }
        0x39 => {
            // Space
            let (x, y) = mouse_position();
            handle_mouse_button_down(x, y, 0, false);
            handle_mouse_button_up(x, y, 0);
            true
        }
        _ => false,
    }
}
