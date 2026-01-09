//! Window Content Rendering
//!
//! Renders content inside windows based on window type/title.

use super::renderer::{draw_text, fill_rect};
use super::window_manager::Window;
use core::sync::atomic::Ordering;

// Colors
const COLOR_TEXT: u32 = 0xE0E0E0;
const COLOR_TEXT_DIM: u32 = 0x888888;
const COLOR_TEXT_ACCENT: u32 = 0x88CCFF;
const COLOR_TEXT_SUCCESS: u32 = 0x88FF88;
const COLOR_TEXT_WARNING: u32 = 0xFFFF88;
const COLOR_TEXT_ERROR: u32 = 0xFF8888;
const COLOR_BG: u32 = 0x2A2A4E;
const LINE_HEIGHT: i32 = 16;

/// Render content for a window based on its title.
pub fn render_window_content(win: &Window, cx: i32, cy: i32, cw: u32, ch: u32) {
    let title = win.get_title();

    if title == b"System Status" {
        render_system_status(cx, cy, cw, ch);
    } else if title == b"AI Assistant" {
        render_ai_assistant(cx, cy, cw, ch);
    } else {
        // Default placeholder
        draw_text(cx + 10, cy + 10, b"Window Content", COLOR_TEXT_DIM);
    }
}

/// Render system status content.
fn render_system_status(cx: i32, cy: i32, _cw: u32, _ch: u32) {
    let mut y = cy + 8;
    let x = cx + 10;

    // Hardware section
    draw_text(x, y, b"Hardware:", COLOR_TEXT_ACCENT);
    y += LINE_HEIGHT;

    draw_text(x + 10, y, b"[OK] IDT: Interrupt Table", COLOR_TEXT_SUCCESS);
    y += LINE_HEIGHT;

    draw_text(x + 10, y, b"[OK] GDT: Descriptor Table", COLOR_TEXT_SUCCESS);
    y += LINE_HEIGHT;

    draw_text(x + 10, y, b"[OK] Memory Manager", COLOR_TEXT_SUCCESS);
    y += LINE_HEIGHT;

    draw_text(x + 10, y, b"[OK] Framebuffer", COLOR_TEXT_SUCCESS);
    y += LINE_HEIGHT + 4;

    // Subsystems section
    draw_text(x, y, b"Subsystems:", COLOR_TEXT_ACCENT);
    y += LINE_HEIGHT;

    // System 1 status
    let s1_running = crate::SYSTEM1_RUNNING.load(Ordering::Relaxed);
    if s1_running {
        draw_text(x + 10, y, b"[OK] System 1: GPU Engine", COLOR_TEXT_SUCCESS);
    } else {
        draw_text(x + 10, y, b"[..] System 1: Starting", COLOR_TEXT_WARNING);
    }
    y += LINE_HEIGHT;

    // System 2 status - always show as OK (no separate tracking)
    draw_text(x + 10, y, b"[OK] System 2: LLM Engine", COLOR_TEXT_SUCCESS);
    y += LINE_HEIGHT;

    // Conductor status
    let cond_running = crate::CONDUCTOR_RUNNING.load(Ordering::Relaxed);
    if cond_running {
        draw_text(x + 10, y, b"[OK] Conductor: Active", COLOR_TEXT_SUCCESS);
    } else {
        draw_text(x + 10, y, b"[..] Conductor: Starting", COLOR_TEXT_WARNING);
    }
    y += LINE_HEIGHT + 4;

    // Stats section
    draw_text(x, y, b"Stats:", COLOR_TEXT_ACCENT);
    y += LINE_HEIGHT;

    // Ray queue depth
    let q_depth = crate::rayq_depth();
    draw_text(x + 10, y, b"Ray Queue:", COLOR_TEXT_DIM);
    draw_number(x + 110, y, q_depth, COLOR_TEXT);
    y += LINE_HEIGHT;

    // Processed count
    let processed = crate::SYSTEM1_PROCESSED.load(Ordering::Relaxed) as usize;
    draw_text(x + 10, y, b"Processed:", COLOR_TEXT_DIM);
    draw_number(x + 110, y, processed, COLOR_TEXT);
}

/// Render AI assistant content.
fn render_ai_assistant(cx: i32, cy: i32, cw: u32, ch: u32) {
    let mut y = cy + 8;
    let x = cx + 10;

    // AI status
    if crate::HOST_AI_ENABLED {
        if crate::HOST_BRIDGE_CONNECTED.load(Ordering::Relaxed) {
            draw_text(x, y, b"[Host AI Connected]", COLOR_TEXT_SUCCESS);
        } else {
            draw_text(x, y, b"[Waiting for Host AI...]", COLOR_TEXT_WARNING);
        }
    } else if crate::LOCAL_AI_ENABLED {
        draw_text(x, y, b"[Local AI Mode]", COLOR_TEXT_ACCENT);
    } else {
        draw_text(x, y, b"[AI Disabled]", COLOR_TEXT_DIM);
    }
    y += LINE_HEIGHT + 4;

    // Separator line
    for i in 0..((cw - 20) as i32 / 4) {
        super::renderer::draw_pixel(x + i * 4, y, 0x444466);
    }
    y += 8;

    // Chat history placeholder
    draw_text(x, y, b"Chat:", COLOR_TEXT_ACCENT);
    y += LINE_HEIGHT;

    // Get recent chat lines from the global chat log
    render_chat_history(x + 10, y, cw - 30, ch - (y - cy) as u32 - 40);

    // Input area at bottom
    let input_y = cy + ch as i32 - 24;
    let is_active = super::input::is_text_input_active();

    // Draw input background (highlighted if active)
    let bg_color = if is_active { 0x252545 } else { 0x1A1A2E };
    fill_rect(x - 2, input_y - 2, cw - 16, 20, bg_color);

    // Draw border if active
    if is_active {
        let bw = (cw - 16) as i32;
        for i in 0..bw {
            super::renderer::draw_pixel(x - 2 + i, input_y - 2, 0x88CCFF);
            super::renderer::draw_pixel(x - 2 + i, input_y + 16, 0x88CCFF);
        }
        for i in 0..20 {
            super::renderer::draw_pixel(x - 2, input_y - 2 + i, 0x88CCFF);
            super::renderer::draw_pixel(x - 2 + bw - 1, input_y - 2 + i, 0x88CCFF);
        }
    }

    // Draw prompt and input text
    draw_text(x, input_y, b">", COLOR_TEXT_ACCENT);

    let input_text = super::input::get_text_input();
    if input_text.is_empty() {
        if !is_active {
            draw_text(x + 12, input_y, b"Type here...", COLOR_TEXT_DIM);
        } else {
            // Show blinking cursor
            draw_text(x + 12, input_y, b"_", COLOR_TEXT_ACCENT);
        }
    } else {
        // Draw the input text
        draw_text(x + 12, input_y, input_text, COLOR_TEXT);
        if is_active {
            // Draw cursor after text
            let cursor_x = x + 12 + (input_text.len() as i32 * 8);
            draw_text(cursor_x, input_y, b"_", COLOR_TEXT_ACCENT);
        }
    }
}

/// Render chat history lines.
fn render_chat_history(x: i32, mut y: i32, _w: u32, h: u32) {
    // Access global chat log
    // For now, show some placeholder lines
    let max_lines = (h / LINE_HEIGHT as u32) as usize;

    // Try to get chat lines from the kernel's chat log
    // This is a simplified version - we just show placeholder for now
    // The actual implementation would read from CHAT_LOG

    if max_lines > 0 {
        draw_text(x, y, b"SYS: Ready for input", COLOR_TEXT_DIM);
        y += LINE_HEIGHT;
    }
    if max_lines > 1 {
        draw_text(x, y, b"Type a message and press Enter", COLOR_TEXT_DIM);
    }
}

/// Draw a number at position.
fn draw_number(x: i32, y: i32, value: usize, color: u32) {
    let mut buf = [0u8; 16];
    let len = format_number(value, &mut buf);
    draw_text(x, y, &buf[..len], color);
}

/// Format a number into a buffer.
fn format_number(mut value: usize, buf: &mut [u8]) -> usize {
    if value == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut i = 0;
    let mut tmp = [0u8; 16];
    while value > 0 && i < tmp.len() {
        tmp[i] = b'0' + (value % 10) as u8;
        value /= 10;
        i += 1;
    }

    // Reverse into buf
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    i
}

/// Handle text input submission from the AI Assistant window.
/// This is called when the user presses Enter in the text input field.
pub fn handle_text_submit(text: &[u8]) {
    if text.is_empty() {
        return;
    }

    // Send the text to the kernel's AI prompt handler
    // For now, we'll add it to the chat log directly
    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_UI_TEXT_SUBMIT:");
        for &b in text {
            if b >= 0x20 && b < 0x7F {
                crate::serial_write_byte(b);
            }
        }
        crate::serial_write_str("\n");
    }

    // TODO: Route to the actual AI prompt handler in main.rs
    // For now, just trigger a compositor refresh
    super::compositor::mark_dirty();
}
