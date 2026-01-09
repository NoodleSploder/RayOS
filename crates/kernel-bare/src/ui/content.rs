//! Window Content Rendering
//!
//! Renders content inside windows based on window type/title.

use super::renderer::{draw_text, fill_rect};
use super::window_manager::{Window, WindowType};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// Colors
const COLOR_TEXT: u32 = 0xE0E0E0;
const COLOR_TEXT_DIM: u32 = 0x888888;
const COLOR_TEXT_ACCENT: u32 = 0x88CCFF;
const COLOR_TEXT_SUCCESS: u32 = 0x88FF88;
const COLOR_TEXT_WARNING: u32 = 0xFFFF88;
const COLOR_TEXT_ERROR: u32 = 0xFF8888;
const COLOR_BG: u32 = 0x2A2A4E;
const LINE_HEIGHT: i32 = 16;

// VM Surface rendering state
static VM_LAST_WIDTH: AtomicU32 = AtomicU32::new(0);
static VM_LAST_HEIGHT: AtomicU32 = AtomicU32::new(0);
static VM_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static VM_LAST_FRAME_TICK: AtomicU64 = AtomicU64::new(0);
static VM_FPS: AtomicU32 = AtomicU32::new(0);
static VM_SHOW_OVERLAY: AtomicU32 = AtomicU32::new(1); // 1 = show overlay by default
static VM_BILINEAR_SCALING: AtomicU32 = AtomicU32::new(0); // 0 = nearest-neighbor, 1 = bilinear

/// Format a number into a buffer, returning the number of digits written.
fn format_number_to_buf(mut n: usize, buf: &mut [u8]) -> usize {
    if n == 0 {
        if !buf.is_empty() {
            buf[0] = b'0';
        }
        return 1;
    }
    // Count digits
    let mut temp = n;
    let mut digits = 0;
    while temp > 0 {
        digits += 1;
        temp /= 10;
    }
    // Write digits in reverse
    if digits > buf.len() {
        return 0; // Buffer too small
    }
    for i in (0..digits).rev() {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    digits
}

/// Toggle VM overlay display.
pub fn toggle_vm_overlay() {
    let current = VM_SHOW_OVERLAY.load(Ordering::Relaxed);
    VM_SHOW_OVERLAY.store(if current == 0 { 1 } else { 0 }, Ordering::Relaxed);
}

/// Check if VM overlay is enabled.
pub fn is_vm_overlay_enabled() -> bool {
    VM_SHOW_OVERLAY.load(Ordering::Relaxed) != 0
}

/// Toggle VM bilinear scaling.
pub fn toggle_vm_bilinear() {
    let current = VM_BILINEAR_SCALING.load(Ordering::Relaxed);
    VM_BILINEAR_SCALING.store(if current == 0 { 1 } else { 0 }, Ordering::Relaxed);
}

/// Check if VM bilinear scaling is enabled.
pub fn is_vm_bilinear_enabled() -> bool {
    VM_BILINEAR_SCALING.load(Ordering::Relaxed) != 0
}

/// Render content for a window based on its title.
pub fn render_window_content(win: &Window, cx: i32, cy: i32, cw: u32, ch: u32) {
    // Check if this is a VM surface window
    if matches!(win.window_type, WindowType::VmSurface) {
        let title = win.get_title();
        if title == b"Windows Desktop" {
            render_windows_vm_surface(win, cx, cy, cw, ch);
        } else {
            // Default to Linux for other VM surfaces (including "Linux Desktop")
            render_vm_surface(win, cx, cy, cw, ch);
        }
        return;
    }

    let title = win.get_title();

    if title == b"System Status" {
        render_system_status(cx, cy, cw, ch);
    } else if title == b"AI Assistant" {
        render_ai_assistant(cx, cy, cw, ch);
    } else if title == b"Process Explorer" {
        render_process_explorer(cx, cy, cw, ch);
    } else if title == b"System Log" {
        render_system_log(cx, cy, cw, ch);
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

/// Render Process Explorer content - graphical system monitor.
fn render_process_explorer(cx: i32, cy: i32, cw: u32, ch: u32) {
    let mut y = cy + 8;
    let x = cx + 10;
    let section_bg = 0x1E1E3E;
    let section_width = cw.saturating_sub(24);

    // === System Overview Section ===
    fill_rect(x - 4, y - 2, section_width, 52, section_bg);
    draw_text(x, y, b"System Overview", COLOR_TEXT_ACCENT);
    y += LINE_HEIGHT;

    // Uptime
    let ticks = crate::TIMER_TICKS.load(Ordering::Relaxed);
    let seconds = ticks / 100; // Assuming ~100 Hz timer
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;
    draw_text(x + 4, y, b"Uptime:", COLOR_TEXT_DIM);
    let mut time_buf = [0u8; 16];
    let time_len = format_time(hours, mins, secs, &mut time_buf);
    draw_text(x + 70, y, &time_buf[..time_len], COLOR_TEXT);

    // Memory (placeholder - would need heap stats)
    draw_text(x + 160, y, b"Memory:", COLOR_TEXT_DIM);
    draw_text(x + 230, y, b"512 MB", COLOR_TEXT);
    y += LINE_HEIGHT;

    // IRQ Counts
    let timer_irqs = crate::IRQ_TIMER_COUNT.load(Ordering::Relaxed) as usize;
    let kbd_irqs = crate::IRQ_KBD_COUNT.load(Ordering::Relaxed) as usize;
    let mouse_irqs = crate::MOUSE_IRQ_COUNT.load(Ordering::Relaxed) as usize;

    draw_text(x + 4, y, b"IRQs:", COLOR_TEXT_DIM);
    draw_text(x + 50, y, b"Timer", COLOR_TEXT_DIM);
    draw_number(x + 95, y, timer_irqs, COLOR_TEXT);
    draw_text(x + 160, y, b"Kbd", COLOR_TEXT_DIM);
    draw_number(x + 190, y, kbd_irqs, COLOR_TEXT);
    draw_text(x + 250, y, b"Mouse", COLOR_TEXT_DIM);
    draw_number(x + 300, y, mouse_irqs, COLOR_TEXT);
    y += LINE_HEIGHT + 12;

    // === Ray Queue Section ===
    fill_rect(x - 4, y - 2, section_width, 84, section_bg);
    draw_text(x, y, b"Ray Queue", COLOR_TEXT_ACCENT);
    y += LINE_HEIGHT;

    // Header
    draw_text(x + 4, y, b"ID", COLOR_TEXT_DIM);
    draw_text(x + 50, y, b"Op", COLOR_TEXT_DIM);
    draw_text(x + 120, y, b"Priority", COLOR_TEXT_DIM);
    draw_text(x + 200, y, b"Status", COLOR_TEXT_DIM);
    y += LINE_HEIGHT;

    // Show last processed ray info
    let last_ray_id = crate::SYSTEM1_LAST_RAY_ID.load(Ordering::Relaxed);
    let last_op = crate::SYSTEM1_LAST_OP.load(Ordering::Relaxed);
    let last_prio = crate::SYSTEM1_LAST_PRIO.load(Ordering::Relaxed);

    if last_ray_id > 0 {
        draw_number(x + 4, y, last_ray_id as usize, COLOR_TEXT);
        draw_op_name(x + 50, y, last_op as u8);
        draw_priority(x + 120, y, last_prio as u8);
        draw_text(x + 200, y, b"Complete", COLOR_TEXT_SUCCESS);
    } else {
        draw_text(x + 4, y, b"(no rays processed yet)", COLOR_TEXT_DIM);
    }
    y += LINE_HEIGHT;

    // Queue stats
    let q_depth = crate::rayq_depth();
    let processed = crate::SYSTEM1_PROCESSED.load(Ordering::Relaxed) as usize;
    let enqueued = crate::SYSTEM1_ENQUEUED.load(Ordering::Relaxed) as usize;

    draw_text(x + 4, y, b"Pending:", COLOR_TEXT_DIM);
    draw_number(x + 80, y, q_depth, if q_depth > 10 { COLOR_TEXT_WARNING } else { COLOR_TEXT });
    draw_text(x + 130, y, b"Processed:", COLOR_TEXT_DIM);
    draw_number(x + 210, y, processed, COLOR_TEXT);
    draw_text(x + 280, y, b"Total:", COLOR_TEXT_DIM);
    draw_number(x + 330, y, enqueued, COLOR_TEXT);
    y += LINE_HEIGHT + 12;

    // === Components Section ===
    fill_rect(x - 4, y - 2, section_width, 100, section_bg);
    draw_text(x, y, b"Components", COLOR_TEXT_ACCENT);
    y += LINE_HEIGHT;

    // System 1
    let s1_running = crate::SYSTEM1_RUNNING.load(Ordering::Relaxed);
    draw_status_indicator(x + 4, y, s1_running);
    draw_text(x + 20, y, b"System 1: GPU Engine", COLOR_TEXT);
    draw_text(x + 200, y, if s1_running { b"Running" } else { b"Starting" },
              if s1_running { COLOR_TEXT_SUCCESS } else { COLOR_TEXT_WARNING });
    y += LINE_HEIGHT;

    // System 2
    draw_status_indicator(x + 4, y, true);
    draw_text(x + 20, y, b"System 2: LLM Engine", COLOR_TEXT);
    draw_text(x + 200, y, b"Running", COLOR_TEXT_SUCCESS);
    y += LINE_HEIGHT;

    // Conductor
    let cond_running = crate::CONDUCTOR_RUNNING.load(Ordering::Relaxed);
    draw_status_indicator(x + 4, y, cond_running);
    draw_text(x + 20, y, b"Conductor", COLOR_TEXT);
    draw_text(x + 200, y, if cond_running { b"Active" } else { b"Starting" },
              if cond_running { COLOR_TEXT_SUCCESS } else { COLOR_TEXT_WARNING });
    y += LINE_HEIGHT;

    // Linux VM
    let linux_state = crate::LINUX_DESKTOP_STATE.load(Ordering::Relaxed);
    let linux_running = linux_state >= 2;
    draw_status_indicator(x + 4, y, linux_running);
    draw_text(x + 20, y, b"Linux VM", COLOR_TEXT);
    draw_linux_state(x + 200, y, linux_state as u64);
    y += LINE_HEIGHT + 12;

    // === Performance Section (if there's room) ===
    if (y - cy) + 50 < ch as i32 {
        fill_rect(x - 4, y - 2, section_width, 52, section_bg);
        draw_text(x, y, b"Performance", COLOR_TEXT_ACCENT);
        y += LINE_HEIGHT;

        // VMX exits (if VMM is running)
        #[cfg(feature = "vmm_hypervisor")]
        {
            let vmx_exits = crate::hypervisor::get_vmx_exit_count();
            draw_text(x + 4, y, b"VMX Exits:", COLOR_TEXT_DIM);
            draw_number(x + 90, y, vmx_exits as usize, COLOR_TEXT);
        }
        #[cfg(not(feature = "vmm_hypervisor"))]
        {
            draw_text(x + 4, y, b"VMX Exits:", COLOR_TEXT_DIM);
            draw_text(x + 90, y, b"N/A", COLOR_TEXT_DIM);
        }

        // Frame count from compositor
        let frame_count = super::compositor::frame_count() as usize;
        draw_text(x + 180, y, b"Frames:", COLOR_TEXT_DIM);
        draw_number(x + 250, y, frame_count, COLOR_TEXT);
    }
}

/// Draw a status indicator (filled/hollow circle).
fn draw_status_indicator(x: i32, y: i32, active: bool) {
    let color = if active { COLOR_TEXT_SUCCESS } else { COLOR_TEXT_DIM };
    // Simple 2x2 box as indicator
    fill_rect(x, y + 4, 8, 8, color);
}

/// Draw operation name from op code.
fn draw_op_name(x: i32, y: i32, op: u8) {
    let name: &[u8] = match op {
        0 => b"NOP",
        1 => b"COMPUTE",
        2 => b"RENDER",
        3 => b"STORAGE",
        4 => b"NETWORK",
        _ => b"UNKNOWN",
    };
    draw_text(x, y, name, COLOR_TEXT);
}

/// Draw priority name from priority code.
fn draw_priority(x: i32, y: i32, prio: u8) {
    let (name, color): (&[u8], u32) = match prio {
        0 => (b"Low", COLOR_TEXT_DIM),
        1 => (b"Normal", COLOR_TEXT),
        2 => (b"High", COLOR_TEXT_WARNING),
        3 => (b"Critical", COLOR_TEXT_ERROR),
        _ => (b"?", COLOR_TEXT_DIM),
    };
    draw_text(x, y, name, color);
}

/// Draw Linux VM state.
fn draw_linux_state(x: i32, y: i32, state: u64) {
    let (text, color): (&[u8], u32) = match state {
        0 => (b"Not Started", COLOR_TEXT_DIM),
        1 => (b"Starting...", COLOR_TEXT_WARNING),
        2 => (b"Running (Hidden)", COLOR_TEXT_SUCCESS),
        3 => (b"Running (Visible)", COLOR_TEXT_SUCCESS),
        4 => (b"Stopping...", COLOR_TEXT_WARNING),
        5 => (b"Presenting...", COLOR_TEXT_WARNING),
        _ => (b"Unknown", COLOR_TEXT_DIM),
    };
    draw_text(x, y, text, color);
}

/// Render the System Log window content.
fn render_system_log(cx: i32, cy: i32, cw: u32, ch: u32) {
    use crate::syslog;

    let x = cx + 8;
    let mut y = cy + 8;
    let content_width = cw.saturating_sub(16);

    // Header
    let header_bg = 0xFF252540;
    fill_rect(x - 4, y - 4, content_width + 8, LINE_HEIGHT as u32 + 4, header_bg);
    draw_text(x, y, b"System Log", COLOR_TEXT_ACCENT);

    // Entry count
    let count = syslog::entry_count();
    let total = syslog::total_count();
    draw_text(x + 120, y, b"Entries:", COLOR_TEXT_DIM);
    draw_number(x + 190, y, count, COLOR_TEXT);
    if total > syslog::LOG_BUFFER_ENTRIES as u64 {
        draw_text(x + 240, y, b"(wrapped)", COLOR_TEXT_DIM);
    }
    y += LINE_HEIGHT + 4;

    // Column headers
    let col_time = x;
    let col_sev = x + 90;
    let col_sys = x + 140;
    let col_msg = x + 200;

    fill_rect(x - 4, y - 2, content_width + 8, LINE_HEIGHT as u32, 0xFF1E1E30);
    draw_text(col_time, y, b"Time", COLOR_TEXT_DIM);
    draw_text(col_sev, y, b"Level", COLOR_TEXT_DIM);
    draw_text(col_sys, y, b"System", COLOR_TEXT_DIM);
    draw_text(col_msg, y, b"Message", COLOR_TEXT_DIM);
    y += LINE_HEIGHT + 2;

    // Calculate how many entries we can show
    let available_height = (ch as i32) - (y - cy) - 8;
    let max_visible = (available_height / LINE_HEIGHT as i32).max(0) as usize;

    // Show most recent entries (from bottom)
    let start_idx = if count > max_visible {
        count - max_visible
    } else {
        0
    };

    for idx in start_idx..count {
        if let Some(entry) = syslog::get_entry(idx) {
            // Alternate row background
            if (idx - start_idx) % 2 == 0 {
                fill_rect(x - 4, y - 1, content_width + 8, LINE_HEIGHT as u32, 0xFF1A1A28);
            }

            // Timestamp
            let mut time_buf = [0u8; 12];
            syslog::format_timestamp(entry.timestamp, &mut time_buf);
            draw_text(col_time, y, &time_buf[..11], COLOR_TEXT_DIM);

            // Severity (colored)
            let sev_color = syslog::severity_color(entry.severity);
            let sev_name = syslog::severity_name(entry.severity);
            draw_text(col_sev, y, sev_name, sev_color);

            // Subsystem
            let sys_name = syslog::subsystem_name(entry.subsystem);
            draw_text(col_sys, y, sys_name, COLOR_TEXT_DIM);

            // Message (truncate if needed)
            let msg = entry.message_bytes();
            let max_msg_width = (content_width as i32 - (col_msg - x)) / 8; // approx chars
            let msg_len = msg.len().min(max_msg_width as usize);
            draw_text(col_msg, y, &msg[..msg_len], sev_color);

            y += LINE_HEIGHT;

            // Stop if we run out of space
            if y + LINE_HEIGHT > cy + ch as i32 - 4 {
                break;
            }
        }
    }

    // Footer with stats
    if y + LINE_HEIGHT <= cy + ch as i32 {
        y = cy + ch as i32 - LINE_HEIGHT - 4;
        fill_rect(x - 4, y - 2, content_width + 8, LINE_HEIGHT as u32 + 4, header_bg);
        draw_text(x, y, b"Total logged:", COLOR_TEXT_DIM);
        draw_number(x + 110, y, total as usize, COLOR_TEXT);
    }
}

/// Format time as HH:MM:SS.
fn format_time(hours: u64, mins: u64, secs: u64, buf: &mut [u8]) -> usize {
    let mut i = 0;
    // Hours
    if hours < 10 { buf[i] = b'0'; i += 1; }
    i += format_number(hours as usize, &mut buf[i..]);
    buf[i] = b':'; i += 1;
    // Minutes
    if mins < 10 { buf[i] = b'0'; i += 1; }
    i += format_number(mins as usize, &mut buf[i..]);
    buf[i] = b':'; i += 1;
    // Seconds
    if secs < 10 { buf[i] = b'0'; i += 1; }
    i += format_number(secs as usize, &mut buf[i..]);
    i
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

/// Render VM surface (guest framebuffer) into a window.
fn render_vm_surface(_win: &Window, cx: i32, cy: i32, cw: u32, ch: u32) {
    // Try to get the guest surface snapshot
    if let Some(surface) = crate::guest_surface::surface_snapshot() {
        // Track resolution changes
        let last_w = VM_LAST_WIDTH.load(Ordering::Relaxed);
        let last_h = VM_LAST_HEIGHT.load(Ordering::Relaxed);
        if surface.width != last_w || surface.height != last_h {
            VM_LAST_WIDTH.store(surface.width, Ordering::Relaxed);
            VM_LAST_HEIGHT.store(surface.height, Ordering::Relaxed);

            #[cfg(feature = "serial_debug")]
            {
                crate::serial_write_str("RAYOS_UI_VM_RESOLUTION_CHANGE:");
                crate::serial_write_hex_u64(surface.width as u64);
                crate::serial_write_str("x");
                crate::serial_write_hex_u64(surface.height as u64);
                crate::serial_write_str("\n");
            }
        }

        // Update frame counter and FPS
        VM_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
        let now = crate::TIMER_TICKS.load(Ordering::Relaxed);
        let last_tick = VM_LAST_FRAME_TICK.load(Ordering::Relaxed);
        if now.saturating_sub(last_tick) >= 100 {
            // Update FPS every ~1 second (100 ticks at 100Hz)
            let frames = VM_FRAME_COUNT.swap(0, Ordering::Relaxed);
            let elapsed = now.saturating_sub(last_tick);
            let fps = if elapsed > 0 {
                ((frames * 100) / elapsed) as u32
            } else {
                0
            };
            VM_FPS.store(fps, Ordering::Relaxed);
            VM_LAST_FRAME_TICK.store(now, Ordering::Relaxed);
        }

        // Render the guest surface into this window's content area
        blit_guest_to_window(surface, cx, cy, cw, ch);

        // Draw overlay if enabled
        if VM_SHOW_OVERLAY.load(Ordering::Relaxed) != 0 {
            render_vm_overlay(cx, cy, surface.width, surface.height, cw, ch);
        }
    } else {
        // No surface available - show waiting message
        fill_rect(cx, cy, cw, ch, 0x1A1A2E);

        let center_x = cx + (cw as i32 / 2) - 80;
        let center_y = cy + (ch as i32 / 2) - 8;

        draw_text(center_x, center_y, b"Waiting for Linux...", COLOR_TEXT_DIM);
        draw_text(center_x - 20, center_y + 20, b"Guest VM is starting up", COLOR_TEXT_DIM);
    }
}

/// Render VM status overlay (resolution, FPS, etc.)
fn render_vm_overlay(cx: i32, cy: i32, guest_w: u32, guest_h: u32, win_w: u32, win_h: u32) {
    // Semi-transparent background for overlay
    let overlay_x = cx + 4;
    let overlay_y = cy + 4;
    let overlay_w = 120u32;
    let overlay_h = 36u32;

    // Draw semi-transparent background (darker pixels)
    for y in 0..overlay_h {
        for x in 0..overlay_w {
            let px = overlay_x + x as i32;
            let py = overlay_y + y as i32;
            super::renderer::draw_pixel(px, py, 0x80000000);
        }
    }

    // Resolution line: "1920x1080"
    let mut buf = [0u8; 32];
    let mut pos = 0usize;

    // Format guest resolution
    let w_digits = format_number_to_buf(guest_w as usize, &mut buf[pos..]);
    pos += w_digits;
    buf[pos] = b'x';
    pos += 1;
    let h_digits = format_number_to_buf(guest_h as usize, &mut buf[pos..]);
    pos += h_digits;

    draw_text(overlay_x + 4, overlay_y + 4, &buf[..pos], 0x00FF88);

    // FPS line
    let fps = VM_FPS.load(Ordering::Relaxed);
    let mut fps_buf = [0u8; 16];
    let fps_digits = format_number_to_buf(fps as usize, &mut fps_buf);
    fps_buf[fps_digits] = b' ';
    fps_buf[fps_digits + 1] = b'F';
    fps_buf[fps_digits + 2] = b'P';
    fps_buf[fps_digits + 3] = b'S';

    draw_text(overlay_x + 4, overlay_y + 20, &fps_buf[..fps_digits + 4], 0xFFFF88);

    // Scale indicator on the right side
    if guest_w != win_w || guest_h != win_h {
        // Calculate scale percentage
        let scale_w = (win_w * 100) / guest_w.max(1);
        let scale_h = (win_h * 100) / guest_h.max(1);
        let scale = scale_w.min(scale_h);

        let mut scale_buf = [0u8; 8];
        let scale_digits = format_number_to_buf(scale as usize, &mut scale_buf);
        scale_buf[scale_digits] = b'%';

        draw_text(overlay_x + 70, overlay_y + 4, &scale_buf[..scale_digits + 1], 0x88CCFF);
    }
}

/// Blit guest surface into window content area with scaling.
fn blit_guest_to_window(
    surface: crate::guest_surface::GuestSurface,
    cx: i32,
    cy: i32,
    cw: u32,
    ch: u32,
) {
    if surface.bpp != 32 {
        draw_text(cx + 10, cy + 10, b"Unsupported format", COLOR_TEXT_ERROR);
        return;
    }

    // Safety: check physical address bounds
    let phys_limit = crate::hhdm_phys_limit();
    if surface.backing_phys == 0 || surface.backing_phys >= phys_limit {
        draw_text(cx + 10, cy + 10, b"Invalid surface addr", COLOR_TEXT_ERROR);
        return;
    }

    let src_w = surface.width as usize;
    let src_h = surface.height as usize;
    let src_stride = surface.stride_px as usize;

    if src_w == 0 || src_h == 0 || src_stride < src_w {
        draw_text(cx + 10, cy + 10, b"Invalid dimensions", COLOR_TEXT_ERROR);
        return;
    }

    // Calculate total bytes needed and check bounds
    let bytes_per_pixel = 4usize; // 32bpp
    let surface_size = src_stride.saturating_mul(src_h).saturating_mul(bytes_per_pixel);
    if surface.backing_phys.saturating_add(surface_size as u64) > phys_limit {
        draw_text(cx + 10, cy + 10, b"Surface out of bounds", COLOR_TEXT_ERROR);
        return;
    }

    let dst_w = cw as usize;
    let dst_h = ch as usize;

    if dst_w == 0 || dst_h == 0 {
        return;
    }

    // Calculate scaled dimensions preserving aspect ratio
    let mut scaled_w = dst_w;
    let mut scaled_h = (dst_w * src_h) / src_w;

    if scaled_h > dst_h {
        scaled_h = dst_h;
        scaled_w = (dst_h * src_w) / src_h;
        if scaled_w > dst_w {
            scaled_w = dst_w;
        }
    }

    if scaled_w == 0 || scaled_h == 0 {
        return;
    }

    // Center within window content area
    let off_x = cx + ((dst_w - scaled_w) / 2) as i32;
    let off_y = cy + ((dst_h - scaled_h) / 2) as i32;

    // Clear letterbox areas (draw dark background)
    fill_rect(cx, cy, cw, ch, 0x101020);

    // Choose scaling algorithm
    let use_bilinear = VM_BILINEAR_SCALING.load(Ordering::Relaxed) != 0;

    unsafe {
        let fb_ptr = crate::FB_BASE as *mut u32;
        let fb_stride = crate::FB_STRIDE;
        let fb_width = crate::FB_WIDTH;
        let fb_height = crate::FB_HEIGHT;

        let src_ptr = crate::phys_as_ptr::<u32>(surface.backing_phys);

        if use_bilinear && scaled_w != src_w && scaled_h != src_h {
            // Bilinear scaling for better quality when resizing
            for y in 0..scaled_h {
                let dst_y = off_y + y as i32;
                if dst_y < 0 || dst_y >= fb_height as i32 {
                    continue;
                }

                // Fixed-point source Y coordinate (16.16)
                let src_y_fp = ((y * src_h) << 16) / scaled_h;
                let sy0 = src_y_fp >> 16;
                let sy1 = (sy0 + 1).min(src_h - 1);
                let fy = (src_y_fp & 0xFFFF) as u32; // Fractional part

                let dst_row = (dst_y as usize) * fb_stride;

                for x in 0..scaled_w {
                    let dst_x = off_x + x as i32;
                    if dst_x < 0 || dst_x >= fb_width as i32 {
                        continue;
                    }

                    // Fixed-point source X coordinate (16.16)
                    let src_x_fp = ((x * src_w) << 16) / scaled_w;
                    let sx0 = src_x_fp >> 16;
                    let sx1 = (sx0 + 1).min(src_w - 1);
                    let fx = (src_x_fp & 0xFFFF) as u32; // Fractional part

                    // Sample 4 source pixels
                    let p00 = *src_ptr.add(sy0 * src_stride + sx0);
                    let p10 = *src_ptr.add(sy0 * src_stride + sx1);
                    let p01 = *src_ptr.add(sy1 * src_stride + sx0);
                    let p11 = *src_ptr.add(sy1 * src_stride + sx1);

                    // Bilinear interpolation for each channel
                    let pixel = bilinear_interpolate(p00, p10, p01, p11, fx, fy);
                    *fb_ptr.add(dst_row + dst_x as usize) = pixel;
                }
            }
        } else {
            // Nearest-neighbor scaling (faster, works well for 1:1 or integer scales)
            for y in 0..scaled_h {
                let dst_y = off_y + y as i32;
                if dst_y < 0 || dst_y >= fb_height as i32 {
                    continue;
                }

                let sy = (y * src_h) / scaled_h;
                let dst_row = (dst_y as usize) * fb_stride;
                let src_row = sy * src_stride;

                for x in 0..scaled_w {
                    let dst_x = off_x + x as i32;
                    if dst_x < 0 || dst_x >= fb_width as i32 {
                        continue;
                    }

                    let sx = (x * src_w) / scaled_w;
                    let pixel = *src_ptr.add(src_row + sx);
                    *fb_ptr.add(dst_row + dst_x as usize) = pixel;
                }
            }
        }
    }
}

/// Bilinear interpolation of 4 ARGB pixels.
#[inline]
fn bilinear_interpolate(p00: u32, p10: u32, p01: u32, p11: u32, fx: u32, fy: u32) -> u32 {
    // fx and fy are 16-bit fractions (0-65535)
    let ifx = 65536 - fx;
    let ify = 65536 - fy;

    // Interpolate each channel separately
    let mut result = 0u32;
    for shift in [0, 8, 16, 24] {
        let c00 = ((p00 >> shift) & 0xFF) as u32;
        let c10 = ((p10 >> shift) & 0xFF) as u32;
        let c01 = ((p01 >> shift) & 0xFF) as u32;
        let c11 = ((p11 >> shift) & 0xFF) as u32;

        // Bilinear: lerp in X for top row, lerp in X for bottom row, lerp in Y
        let top = (c00 * ifx + c10 * fx) >> 16;
        let bot = (c01 * ifx + c11 * fx) >> 16;
        let c = (top * ify + bot * fy) >> 16;

        result |= (c & 0xFF) << shift;
    }
    result
}

// ===== Windows VM Surface State =====

static WINDOWS_VM_LAST_WIDTH: AtomicU32 = AtomicU32::new(0);
static WINDOWS_VM_LAST_HEIGHT: AtomicU32 = AtomicU32::new(0);
static WINDOWS_VM_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static WINDOWS_VM_LAST_FRAME_TICK: AtomicU64 = AtomicU64::new(0);
static WINDOWS_VM_FPS: AtomicU32 = AtomicU32::new(0);

/// Render Windows VM surface (guest framebuffer) into a window.
fn render_windows_vm_surface(_win: &Window, cx: i32, cy: i32, cw: u32, ch: u32) {
    // Try to get the Windows guest surface snapshot
    if let Some(surface) = crate::windows_vm::windows_surface_snapshot() {
        // Track resolution changes
        let last_w = WINDOWS_VM_LAST_WIDTH.load(Ordering::Relaxed);
        let last_h = WINDOWS_VM_LAST_HEIGHT.load(Ordering::Relaxed);
        if surface.width != last_w || surface.height != last_h {
            WINDOWS_VM_LAST_WIDTH.store(surface.width, Ordering::Relaxed);
            WINDOWS_VM_LAST_HEIGHT.store(surface.height, Ordering::Relaxed);

            #[cfg(feature = "serial_debug")]
            {
                crate::serial_write_str("RAYOS_UI_WINDOWS_VM_RESOLUTION_CHANGE:");
                crate::serial_write_hex_u64(surface.width as u64);
                crate::serial_write_str("x");
                crate::serial_write_hex_u64(surface.height as u64);
                crate::serial_write_str("\n");
            }
        }

        // Update frame counter and FPS
        WINDOWS_VM_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
        let now = crate::TIMER_TICKS.load(Ordering::Relaxed);
        let last_tick = WINDOWS_VM_LAST_FRAME_TICK.load(Ordering::Relaxed);
        if now.saturating_sub(last_tick) >= 100 {
            // Update FPS every ~1 second (100 ticks at 100Hz)
            let frames = WINDOWS_VM_FRAME_COUNT.swap(0, Ordering::Relaxed);
            let elapsed = now.saturating_sub(last_tick);
            let fps = if elapsed > 0 {
                ((frames * 100) / elapsed) as u32
            } else {
                0
            };
            WINDOWS_VM_FPS.store(fps, Ordering::Relaxed);
            WINDOWS_VM_LAST_FRAME_TICK.store(now, Ordering::Relaxed);
        }

        // Render using the same blit function (Windows surface has same structure)
        blit_windows_surface_to_window(surface, cx, cy, cw, ch);

        // Draw Windows overlay if enabled
        if VM_SHOW_OVERLAY.load(Ordering::Relaxed) != 0 {
            render_windows_vm_overlay(cx, cy, surface.width, surface.height, cw, ch);
        }
    } else {
        // No surface available - show Windows boot/UEFI message
        fill_rect(cx, cy, cw, ch, 0x0078D4); // Windows blue background

        let center_x = cx + (cw as i32 / 2) - 100;
        let center_y = cy + (ch as i32 / 2) - 16;

        draw_text(center_x, center_y, b"Starting Windows...", 0xFFFFFF);
        draw_text(center_x - 10, center_y + 20, b"UEFI/Secure Boot loading", 0xCCCCCC);
    }
}

/// Render Windows VM status overlay.
fn render_windows_vm_overlay(cx: i32, cy: i32, guest_w: u32, guest_h: u32, win_w: u32, win_h: u32) {
    let overlay_x = cx + 4;
    let overlay_y = cy + 4;
    let overlay_w = 130u32;
    let overlay_h = 36u32;

    // Draw semi-transparent background
    for y in 0..overlay_h {
        for x in 0..overlay_w {
            let px = overlay_x + x as i32;
            let py = overlay_y + y as i32;
            super::renderer::draw_pixel(px, py, 0x80000000);
        }
    }

    // "Windows" label and resolution
    draw_text(overlay_x + 4, overlay_y + 4, b"Windows", 0x00A0FF);

    let mut buf = [0u8; 16];
    let mut pos = 0usize;
    let w_digits = format_number_to_buf(guest_w as usize, &mut buf[pos..]);
    pos += w_digits;
    buf[pos] = b'x';
    pos += 1;
    let h_digits = format_number_to_buf(guest_h as usize, &mut buf[pos..]);
    pos += h_digits;
    draw_text(overlay_x + 70, overlay_y + 4, &buf[..pos], 0x88FF88);

    // FPS
    let fps = WINDOWS_VM_FPS.load(Ordering::Relaxed);
    let mut fps_buf = [0u8; 16];
    let fps_digits = format_number_to_buf(fps as usize, &mut fps_buf);
    fps_buf[fps_digits] = b' ';
    fps_buf[fps_digits + 1] = b'F';
    fps_buf[fps_digits + 2] = b'P';
    fps_buf[fps_digits + 3] = b'S';
    draw_text(overlay_x + 4, overlay_y + 20, &fps_buf[..fps_digits + 4], 0xFFFF88);

    // Scale indicator
    if guest_w != win_w || guest_h != win_h {
        let scale_w = (win_w * 100) / guest_w.max(1);
        let scale_h = (win_h * 100) / guest_h.max(1);
        let scale = scale_w.min(scale_h);

        let mut scale_buf = [0u8; 8];
        let scale_digits = format_number_to_buf(scale as usize, &mut scale_buf);
        scale_buf[scale_digits] = b'%';
        draw_text(overlay_x + 70, overlay_y + 20, &scale_buf[..scale_digits + 1], 0x88CCFF);
    }
}

/// Blit Windows surface into window content area with scaling.
fn blit_windows_surface_to_window(
    surface: crate::windows_vm::WindowsSurface,
    cx: i32,
    cy: i32,
    cw: u32,
    ch: u32,
) {
    if surface.bpp != 32 {
        draw_text(cx + 10, cy + 10, b"Unsupported format", COLOR_TEXT_ERROR);
        return;
    }

    let phys_limit = crate::hhdm_phys_limit();
    if surface.backing_phys == 0 || surface.backing_phys >= phys_limit {
        draw_text(cx + 10, cy + 10, b"Invalid surface addr", COLOR_TEXT_ERROR);
        return;
    }

    let src_w = surface.width as usize;
    let src_h = surface.height as usize;
    let src_stride = surface.stride_px as usize;

    if src_w == 0 || src_h == 0 || src_stride < src_w {
        draw_text(cx + 10, cy + 10, b"Invalid dimensions", COLOR_TEXT_ERROR);
        return;
    }

    let bytes_per_pixel = 4usize;
    let surface_size = src_stride.saturating_mul(src_h).saturating_mul(bytes_per_pixel);
    if surface.backing_phys.saturating_add(surface_size as u64) > phys_limit {
        draw_text(cx + 10, cy + 10, b"Surface out of bounds", COLOR_TEXT_ERROR);
        return;
    }

    let dst_w = cw as usize;
    let dst_h = ch as usize;

    if dst_w == 0 || dst_h == 0 {
        return;
    }

    // Calculate scaled dimensions preserving aspect ratio
    let mut scaled_w = dst_w;
    let mut scaled_h = (dst_w * src_h) / src_w;

    if scaled_h > dst_h {
        scaled_h = dst_h;
        scaled_w = (dst_h * src_w) / src_h;
        if scaled_w > dst_w {
            scaled_w = dst_w;
        }
    }

    if scaled_w == 0 || scaled_h == 0 {
        return;
    }

    let off_x = cx + ((dst_w - scaled_w) / 2) as i32;
    let off_y = cy + ((dst_h - scaled_h) / 2) as i32;

    // Windows blue letterbox background
    fill_rect(cx, cy, cw, ch, 0x0078D4);

    let use_bilinear = VM_BILINEAR_SCALING.load(Ordering::Relaxed) != 0;

    unsafe {
        let fb_ptr = crate::FB_BASE as *mut u32;
        let fb_stride = crate::FB_STRIDE;
        let fb_width = crate::FB_WIDTH;
        let fb_height = crate::FB_HEIGHT;

        let src_ptr = crate::phys_as_ptr::<u32>(surface.backing_phys);

        if use_bilinear && scaled_w != src_w && scaled_h != src_h {
            for y in 0..scaled_h {
                let dst_y = off_y + y as i32;
                if dst_y < 0 || dst_y >= fb_height as i32 {
                    continue;
                }

                let src_y_fp = ((y * src_h) << 16) / scaled_h;
                let sy0 = src_y_fp >> 16;
                let sy1 = (sy0 + 1).min(src_h - 1);
                let fy = (src_y_fp & 0xFFFF) as u32;

                let dst_row = (dst_y as usize) * fb_stride;

                for x in 0..scaled_w {
                    let dst_x = off_x + x as i32;
                    if dst_x < 0 || dst_x >= fb_width as i32 {
                        continue;
                    }

                    let src_x_fp = ((x * src_w) << 16) / scaled_w;
                    let sx0 = src_x_fp >> 16;
                    let sx1 = (sx0 + 1).min(src_w - 1);
                    let fx = (src_x_fp & 0xFFFF) as u32;

                    let p00 = *src_ptr.add(sy0 * src_stride + sx0);
                    let p10 = *src_ptr.add(sy0 * src_stride + sx1);
                    let p01 = *src_ptr.add(sy1 * src_stride + sx0);
                    let p11 = *src_ptr.add(sy1 * src_stride + sx1);

                    let pixel = bilinear_interpolate(p00, p10, p01, p11, fx, fy);
                    *fb_ptr.add(dst_row + dst_x as usize) = pixel;
                }
            }
        } else {
            for y in 0..scaled_h {
                let dst_y = off_y + y as i32;
                if dst_y < 0 || dst_y >= fb_height as i32 {
                    continue;
                }

                let sy = (y * src_h) / scaled_h;
                let dst_row = (dst_y as usize) * fb_stride;
                let src_row = sy * src_stride;

                for x in 0..scaled_w {
                    let dst_x = off_x + x as i32;
                    if dst_x < 0 || dst_x >= fb_width as i32 {
                        continue;
                    }

                    let sx = (x * src_w) / scaled_w;
                    let pixel = *src_ptr.add(src_row + sx);
                    *fb_ptr.add(dst_row + dst_x as usize) = pixel;
                }
            }
        }
    }
}

/// Handle text input submission from the AI Assistant window.
/// This is called when the user presses Enter in the text input field.
pub fn handle_text_submit(text: &[u8]) {
    if text.is_empty() {
        return;
    }

    // Check if this is a "show system log" command
    if is_show_system_log_command(text) {
        super::shell::show_system_log();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_SYSTEM_LOG_OPENED\n");
        }

        return;
    }

    // Check if this is a "show process explorer" command
    if is_show_process_explorer_command(text) {
        super::shell::show_process_explorer();

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_UI_PROCESS_EXPLORER_OPENED\n");
        }

        return;
    }

    // Check if this is a "show linux desktop" command
    if is_show_linux_command(text) {
        // Request to show linux desktop via UI shell
        super::shell::show_linux_desktop();

        // Set presentation state so dev_scanout/VMM will publish surface
        crate::guest_surface::set_presentation_state(
            crate::guest_surface::PresentationState::Presented
        );

        // Also trigger the VMM if available
        #[cfg(feature = "vmm_hypervisor")]
        {
            crate::hypervisor::linux_desktop_vmm_request_start();
        }

        // Emit host event for non-VMM path
        #[cfg(not(feature = "vmm_hypervisor"))]
        {
            crate::serial_write_str("RAYOS_HOST_EVENT_V0:SHOW_LINUX_DESKTOP\n");
        }

        return;
    }

    // Send the text to the kernel's AI prompt handler
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

/// Check if the input text is a "show linux desktop" command.
fn is_show_linux_command(text: &[u8]) -> bool {
    // Convert to lowercase for comparison
    let mut buf = [0u8; 64];
    let len = text.len().min(64);

    for i in 0..len {
        buf[i] = if text[i] >= b'A' && text[i] <= b'Z' {
            text[i] + 32
        } else {
            text[i]
        };
    }

    let lower = &buf[..len];

    // Check for various phrasings
    if contains_bytes(lower, b"show linux") || contains_bytes(lower, b"linux desktop") {
        return true;
    }
    if contains_bytes(lower, b"open linux") || contains_bytes(lower, b"start linux") {
        return true;
    }
    if contains_bytes(lower, b"show desktop") && !contains_bytes(lower, b"hide") {
        return true;
    }

    false
}

/// Check if the input text is a "show process explorer" command.
fn is_show_process_explorer_command(text: &[u8]) -> bool {
    // Convert to lowercase for comparison
    let mut buf = [0u8; 64];
    let len = text.len().min(64);

    for i in 0..len {
        buf[i] = if text[i] >= b'A' && text[i] <= b'Z' {
            text[i] + 32
        } else {
            text[i]
        };
    }

    let lower = &buf[..len];

    // Check for various phrasings
    if contains_bytes(lower, b"process explorer") {
        return true;
    }
    if contains_bytes(lower, b"show process") || contains_bytes(lower, b"open process") {
        return true;
    }
    if contains_bytes(lower, b"processes") {
        return true;
    }
    // htop/top shortcuts
    if lower == b"htop" || lower == b"top" {
        return true;
    }

    false
}

/// Check if the input text is a "show system log" command.
fn is_show_system_log_command(text: &[u8]) -> bool {
    // Convert to lowercase for comparison
    let mut buf = [0u8; 64];
    let len = text.len().min(64);

    for i in 0..len {
        buf[i] = if text[i] >= b'A' && text[i] <= b'Z' {
            text[i] + 32
        } else {
            text[i]
        };
    }

    let lower = &buf[..len];

    // Check for various phrasings
    if contains_bytes(lower, b"system log") {
        return true;
    }
    if contains_bytes(lower, b"show log") || contains_bytes(lower, b"open log") {
        return true;
    }
    if contains_bytes(lower, b"syslog") || contains_bytes(lower, b"logs") {
        return true;
    }
    // dmesg shortcut (familiar to Linux users)
    if lower == b"dmesg" || lower == b"log" {
        return true;
    }

    false
}

/// Check if haystack contains needle.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            return true;
        }
    }
    false
}
