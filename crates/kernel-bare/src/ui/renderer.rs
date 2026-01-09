//! Low-level rendering primitives for RayOS UI
//!
//! Provides direct framebuffer drawing functions for rectangles, text, and surfaces.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

// ===== Color Palette =====
/// Desktop background - dark blue-gray
pub const COLOR_BACKGROUND: u32 = 0xFF1E1E2E;
/// Window content area - slightly lighter
pub const COLOR_WINDOW_BG: u32 = 0xFF2D2D3D;
/// Window title bar (unfocused)
pub const COLOR_TITLE_BAR: u32 = 0xFF3D3D5C;
/// Window title bar (focused)
pub const COLOR_TITLE_FOCUSED: u32 = 0xFF5D5D8C;
/// Primary text color
pub const COLOR_TEXT: u32 = 0xFFE0E0E0;
/// Secondary/dim text
pub const COLOR_TEXT_DIM: u32 = 0xFF808080;
/// RayOS accent green
pub const COLOR_ACCENT: u32 = 0xFF00FF88;
/// Window border
pub const COLOR_BORDER: u32 = 0xFF4D4D6D;
/// Close button hover
pub const COLOR_CLOSE_HOVER: u32 = 0xFFE04040;
/// Black
pub const COLOR_BLACK: u32 = 0xFF000000;
/// White
pub const COLOR_WHITE: u32 = 0xFFFFFFFF;

// ===== Font Constants =====
pub const FONT_WIDTH: usize = 8;
pub const FONT_HEIGHT: usize = 8;

// ===== Framebuffer State =====
static FB_ADDR: AtomicU64 = AtomicU64::new(0);
static FB_WIDTH: AtomicUsize = AtomicUsize::new(0);
static FB_HEIGHT: AtomicUsize = AtomicUsize::new(0);
static FB_STRIDE: AtomicUsize = AtomicUsize::new(0);
static RENDERER_READY: AtomicUsize = AtomicUsize::new(0);

/// Initialize the renderer with framebuffer parameters.
///
/// # Safety
/// Must be called once during system initialization with valid framebuffer info.
pub fn init(fb_addr: u64, width: usize, height: usize, stride: usize) {
    FB_ADDR.store(fb_addr, Ordering::Release);
    FB_WIDTH.store(width, Ordering::Release);
    FB_HEIGHT.store(height, Ordering::Release);
    FB_STRIDE.store(stride, Ordering::Release);
    RENDERER_READY.store(1, Ordering::Release);

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_UI_RENDERER_INIT:ok\n");
    }
}

/// Check if renderer is initialized and ready.
#[inline]
pub fn is_ready() -> bool {
    RENDERER_READY.load(Ordering::Acquire) != 0
}

/// Get framebuffer dimensions.
#[inline]
pub fn get_dimensions() -> (usize, usize) {
    (FB_WIDTH.load(Ordering::Relaxed), FB_HEIGHT.load(Ordering::Relaxed))
}

/// Draw a single pixel at (x, y) with the given color.
///
/// Coordinates are clipped to framebuffer bounds.
#[inline]
pub fn draw_pixel(x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 {
        return;
    }
    let ux = x as usize;
    let uy = y as usize;
    let width = FB_WIDTH.load(Ordering::Relaxed);
    let height = FB_HEIGHT.load(Ordering::Relaxed);
    let stride = FB_STRIDE.load(Ordering::Relaxed);
    let addr = FB_ADDR.load(Ordering::Relaxed);

    if ux < width && uy < height && addr != 0 {
        unsafe {
            let fb = addr as *mut u32;
            let offset = uy * stride + ux;
            *fb.add(offset) = color;
        }
    }
}

/// Fill a rectangle with a solid color.
///
/// # Arguments
/// * `x`, `y` - Top-left corner
/// * `w`, `h` - Width and height
/// * `color` - Fill color (ARGB format)
pub fn fill_rect(x: i32, y: i32, w: u32, h: u32, color: u32) {
    if !is_ready() || w == 0 || h == 0 {
        return;
    }

    let fb_width = FB_WIDTH.load(Ordering::Relaxed) as i32;
    let fb_height = FB_HEIGHT.load(Ordering::Relaxed) as i32;
    let stride = FB_STRIDE.load(Ordering::Relaxed);
    let addr = FB_ADDR.load(Ordering::Relaxed);

    // Clip to screen bounds
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w as i32).min(fb_width);
    let y1 = (y + h as i32).min(fb_height);

    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let clipped_w = (x1 - x0) as usize;

    unsafe {
        let fb = addr as *mut u32;
        for row in y0..y1 {
            let row_start = fb.add(row as usize * stride + x0 as usize);
            for col in 0..clipped_w {
                *row_start.add(col) = color;
            }
        }
    }
}

/// Draw a rectangle outline (unfilled).
///
/// # Arguments
/// * `x`, `y` - Top-left corner
/// * `w`, `h` - Width and height
/// * `color` - Border color
/// * `thickness` - Border thickness in pixels
pub fn draw_rect(x: i32, y: i32, w: u32, h: u32, color: u32, thickness: u32) {
    if w == 0 || h == 0 || thickness == 0 {
        return;
    }

    let t = thickness.min(w / 2).min(h / 2);

    // Top edge
    fill_rect(x, y, w, t, color);
    // Bottom edge
    fill_rect(x, y + h as i32 - t as i32, w, t, color);
    // Left edge
    fill_rect(x, y + t as i32, t, h - 2 * t, color);
    // Right edge
    fill_rect(x + w as i32 - t as i32, y + t as i32, t, h - 2 * t, color);
}

/// Draw a single character at (x, y).
///
/// Uses the built-in 8x8 bitmap font.
pub fn draw_char(x: i32, y: i32, ch: u8, color: u32) {
    let glyph = get_glyph(ch);
    for row in 0..FONT_HEIGHT {
        let byte = glyph[row];
        for col in 0..FONT_WIDTH {
            if byte & (1 << (7 - col)) != 0 {
                draw_pixel(x + col as i32, y + row as i32, color);
            }
        }
    }
}

/// Draw a character with background color.
pub fn draw_char_bg(x: i32, y: i32, ch: u8, fg: u32, bg: u32) {
    let glyph = get_glyph(ch);
    for row in 0..FONT_HEIGHT {
        let byte = glyph[row];
        for col in 0..FONT_WIDTH {
            let color = if byte & (1 << (7 - col)) != 0 { fg } else { bg };
            draw_pixel(x + col as i32, y + row as i32, color);
        }
    }
}

/// Draw a text string at (x, y).
///
/// # Arguments
/// * `x`, `y` - Position of first character
/// * `text` - Byte slice (ASCII)
/// * `color` - Text color
pub fn draw_text(x: i32, y: i32, text: &[u8], color: u32) {
    for (i, &ch) in text.iter().enumerate() {
        draw_char(x + (i * FONT_WIDTH) as i32, y, ch, color);
    }
}

/// Draw text with a background color.
pub fn draw_text_bg(x: i32, y: i32, text: &[u8], fg: u32, bg: u32) {
    for (i, &ch) in text.iter().enumerate() {
        draw_char_bg(x + (i * FONT_WIDTH) as i32, y, ch, fg, bg);
    }
}

/// Blit an RGBA surface to the framebuffer.
///
/// # Arguments
/// * `dst_x`, `dst_y` - Destination position
/// * `src` - Source pixel data (ARGB format, row-major)
/// * `src_w`, `src_h` - Source dimensions
/// * `src_stride` - Source stride in pixels
///
/// # Safety
/// Caller must ensure `src` points to valid memory of at least
/// `src_h * src_stride * 4` bytes.
pub unsafe fn blit_rgba(
    dst_x: i32,
    dst_y: i32,
    src: *const u32,
    src_w: u32,
    src_h: u32,
    src_stride: u32,
) {
    if !is_ready() || src.is_null() || src_w == 0 || src_h == 0 {
        return;
    }

    let fb_width = FB_WIDTH.load(Ordering::Relaxed) as i32;
    let fb_height = FB_HEIGHT.load(Ordering::Relaxed) as i32;
    let fb_stride = FB_STRIDE.load(Ordering::Relaxed);
    let fb_addr = FB_ADDR.load(Ordering::Relaxed);

    // Calculate visible region
    let src_x0 = if dst_x < 0 { (-dst_x) as u32 } else { 0 };
    let src_y0 = if dst_y < 0 { (-dst_y) as u32 } else { 0 };
    let dst_x0 = dst_x.max(0);
    let dst_y0 = dst_y.max(0);

    let visible_w = ((dst_x + src_w as i32).min(fb_width) - dst_x0).max(0) as u32;
    let visible_h = ((dst_y + src_h as i32).min(fb_height) - dst_y0).max(0) as u32;

    if visible_w == 0 || visible_h == 0 {
        return;
    }

    let fb = fb_addr as *mut u32;

    for row in 0..visible_h {
        let src_row = src.add(((src_y0 + row) * src_stride + src_x0) as usize);
        let dst_row = fb.add((dst_y0 as usize + row as usize) * fb_stride + dst_x0 as usize);

        core::ptr::copy_nonoverlapping(src_row, dst_row, visible_w as usize);
    }
}

/// Draw a horizontal line.
pub fn draw_hline(x: i32, y: i32, w: u32, color: u32) {
    fill_rect(x, y, w, 1, color);
}

/// Draw a vertical line.
pub fn draw_vline(x: i32, y: i32, h: u32, color: u32) {
    fill_rect(x, y, 1, h, color);
}

/// Get font glyph for a character.
fn get_glyph(ch: u8) -> [u8; 8] {
    match ch {
        b' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'!' => [0x18, 0x3C, 0x3C, 0x18, 0x18, 0x00, 0x18, 0x00],
        b'"' => [0x6C, 0x6C, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'#' => [0x6C, 0x6C, 0xFE, 0x6C, 0xFE, 0x6C, 0x6C, 0x00],
        b'$' => [0x18, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x18, 0x00],
        b'%' => [0x00, 0xC6, 0xCC, 0x18, 0x30, 0x66, 0xC6, 0x00],
        b'&' => [0x38, 0x6C, 0x38, 0x76, 0xDC, 0xCC, 0x76, 0x00],
        b'\'' => [0x18, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        b')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        b'*' => [0x00, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0x00, 0x00],
        b'+' => [0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
        b',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30],
        b'-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        b'.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        b'/' => [0x06, 0x0C, 0x18, 0x30, 0x60, 0xC0, 0x80, 0x00],
        b'0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        b'1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        b'2' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        b'3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        b'4' => [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
        b'5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        b'6' => [0x1C, 0x30, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
        b'7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        b'8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        b'9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
        b':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        b';' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x30, 0x00],
        b'<' => [0x0C, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0C, 0x00],
        b'=' => [0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00],
        b'>' => [0x30, 0x18, 0x0C, 0x06, 0x0C, 0x18, 0x30, 0x00],
        b'?' => [0x3C, 0x66, 0x0C, 0x18, 0x18, 0x00, 0x18, 0x00],
        b'@' => [0x3C, 0x66, 0x6E, 0x6E, 0x60, 0x62, 0x3C, 0x00],
        b'A' => [0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        b'B' => [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
        b'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        b'D' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        b'E' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00],
        b'F' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
        b'G' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3C, 0x00],
        b'H' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        b'I' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        b'J' => [0x1E, 0x0C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38, 0x00],
        b'K' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        b'L' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        b'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        b'N' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        b'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        b'P' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        b'Q' => [0x3C, 0x66, 0x66, 0x66, 0x6A, 0x6C, 0x36, 0x00],
        b'R' => [0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x00],
        b'S' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        b'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        b'U' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        b'V' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        b'W' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        b'X' => [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
        b'Y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        b'Z' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
        b'[' => [0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00],
        b'\\' => [0xC0, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x02, 0x00],
        b']' => [0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00],
        b'^' => [0x10, 0x38, 0x6C, 0xC6, 0x00, 0x00, 0x00, 0x00],
        b'_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF],
        b'`' => [0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'a' => [0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00],
        b'b' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        b'c' => [0x00, 0x00, 0x3C, 0x66, 0x60, 0x66, 0x3C, 0x00],
        b'd' => [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
        b'e' => [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00],
        b'f' => [0x1C, 0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x00],
        b'g' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        b'h' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        b'i' => [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
        b'j' => [0x0C, 0x00, 0x1C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38],
        b'k' => [0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
        b'l' => [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        b'm' => [0x00, 0x00, 0x76, 0x7F, 0x6B, 0x6B, 0x63, 0x00],
        b'n' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        b'o' => [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        b'p' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60],
        b'q' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x06],
        b'r' => [0x00, 0x00, 0x6E, 0x70, 0x60, 0x60, 0x60, 0x00],
        b's' => [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
        b't' => [0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x1C, 0x00],
        b'u' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
        b'v' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        b'w' => [0x00, 0x00, 0x63, 0x6B, 0x6B, 0x7F, 0x36, 0x00],
        b'x' => [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
        b'y' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        b'z' => [0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        b'{' => [0x0E, 0x18, 0x18, 0x70, 0x18, 0x18, 0x0E, 0x00],
        b'|' => [0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x18, 0x00],
        b'}' => [0x70, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x70, 0x00],
        b'~' => [0x76, 0xDC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        _ => [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55], // Checkerboard for unknown
    }
}

// ===== Mouse Cursor =====

/// Cursor width in pixels
pub const CURSOR_WIDTH: usize = 12;
/// Cursor height in pixels
pub const CURSOR_HEIGHT: usize = 19;

/// Standard arrow cursor bitmap (1 = white outline, 2 = black fill, 0 = transparent)
static CURSOR_ARROW: [[u8; CURSOR_WIDTH]; CURSOR_HEIGHT] = [
    [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0],
    [2, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0],
    [2, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0],
    [2, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0],
    [2, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0],
    [2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0],
    [2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 0],
    [2, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2],
    [2, 1, 1, 1, 2, 1, 2, 0, 0, 0, 0, 0],
    [2, 1, 1, 2, 0, 2, 1, 2, 0, 0, 0, 0],
    [2, 1, 2, 0, 0, 2, 1, 2, 0, 0, 0, 0],
    [2, 2, 0, 0, 0, 0, 2, 1, 2, 0, 0, 0],
    [2, 0, 0, 0, 0, 0, 2, 1, 2, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 2, 1, 2, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 0],
];

/// Resize horizontal cursor (←→)
static CURSOR_RESIZE_H: [[u8; CURSOR_WIDTH]; CURSOR_HEIGHT] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 2, 0, 0, 0, 0, 0, 0, 2, 0, 0],
    [0, 2, 2, 0, 0, 0, 0, 0, 0, 2, 2, 0],
    [2, 1, 1, 2, 2, 2, 2, 2, 2, 1, 1, 2],
    [2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2],
    [2, 1, 1, 2, 2, 2, 2, 2, 2, 1, 1, 2],
    [0, 2, 2, 0, 0, 0, 0, 0, 0, 2, 2, 0],
    [0, 0, 2, 0, 0, 0, 0, 0, 0, 2, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
];

/// Resize vertical cursor (↑↓)
static CURSOR_RESIZE_V: [[u8; CURSOR_WIDTH]; CURSOR_HEIGHT] = [
    [0, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 2, 1, 1, 1, 1, 2, 0, 0, 0],
    [0, 0, 2, 2, 2, 1, 1, 2, 2, 2, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 2, 2, 2, 1, 1, 2, 2, 2, 0, 0],
    [0, 0, 0, 2, 1, 1, 1, 1, 2, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
];

/// Move/grab cursor (4-way arrows)
static CURSOR_MOVE: [[u8; CURSOR_WIDTH]; CURSOR_HEIGHT] = [
    [0, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 2, 1, 1, 1, 1, 2, 0, 0, 0],
    [0, 0, 2, 2, 2, 1, 1, 2, 2, 2, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 2, 2, 0, 2, 1, 1, 2, 0, 2, 2, 0],
    [2, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2],
    [2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2],
    [2, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2],
    [0, 2, 2, 0, 2, 1, 1, 2, 0, 2, 2, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 2, 2, 2, 1, 1, 2, 2, 2, 0, 0],
    [0, 0, 0, 2, 1, 1, 1, 1, 2, 0, 0, 0],
    [0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
];

/// Cursor type for different operations
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum CursorType {
    Arrow = 0,
    ResizeH = 1,
    ResizeV = 2,
    ResizeNWSE = 3,
    ResizeNESW = 4,
    Move = 5,
}

impl Default for CursorType {
    fn default() -> Self {
        CursorType::Arrow
    }
}

/// Draw a cursor at the given position
pub fn draw_cursor(x: i32, y: i32, cursor_type: CursorType) {
    let bitmap = match cursor_type {
        CursorType::Arrow => &CURSOR_ARROW,
        CursorType::ResizeH => &CURSOR_RESIZE_H,
        CursorType::ResizeV => &CURSOR_RESIZE_V,
        CursorType::ResizeNWSE | CursorType::ResizeNESW => &CURSOR_RESIZE_H, // Use H for now
        CursorType::Move => &CURSOR_MOVE,
    };

    for (row, line) in bitmap.iter().enumerate() {
        for (col, &pixel) in line.iter().enumerate() {
            let color = match pixel {
                1 => COLOR_WHITE,      // White fill
                2 => COLOR_BLACK,      // Black outline
                _ => continue,         // Transparent
            };
            draw_pixel(x + col as i32, y + row as i32, color);
        }
    }
}

/// Backup area for cursor restoration (save what's under the cursor)
static mut CURSOR_BACKUP: [[u32; CURSOR_WIDTH]; CURSOR_HEIGHT] = [[0; CURSOR_WIDTH]; CURSOR_HEIGHT];
static mut CURSOR_BACKUP_X: i32 = -1;
static mut CURSOR_BACKUP_Y: i32 = -1;
static mut CURSOR_VISIBLE: bool = false;

/// Save the area under where the cursor will be drawn
pub fn cursor_save_background(x: i32, y: i32) {
    let width = FB_WIDTH.load(Ordering::Relaxed);
    let height = FB_HEIGHT.load(Ordering::Relaxed);
    let stride = FB_STRIDE.load(Ordering::Relaxed);
    let addr = FB_ADDR.load(Ordering::Relaxed);

    if addr == 0 {
        return;
    }

    unsafe {
        CURSOR_BACKUP_X = x;
        CURSOR_BACKUP_Y = y;

        let fb = addr as *const u32;
        for row in 0..CURSOR_HEIGHT {
            let py = y + row as i32;
            for col in 0..CURSOR_WIDTH {
                let px = x + col as i32;
                if px >= 0 && py >= 0 && (px as usize) < width && (py as usize) < height {
                    let offset = py as usize * stride + px as usize;
                    CURSOR_BACKUP[row][col] = *fb.add(offset);
                } else {
                    CURSOR_BACKUP[row][col] = 0;
                }
            }
        }
    }
}

/// Restore the area under the cursor
pub fn cursor_restore_background() {
    let width = FB_WIDTH.load(Ordering::Relaxed);
    let height = FB_HEIGHT.load(Ordering::Relaxed);
    let stride = FB_STRIDE.load(Ordering::Relaxed);
    let addr = FB_ADDR.load(Ordering::Relaxed);

    if addr == 0 {
        return;
    }

    unsafe {
        if CURSOR_BACKUP_X < 0 || CURSOR_BACKUP_Y < 0 {
            return;
        }

        let fb = addr as *mut u32;
        for row in 0..CURSOR_HEIGHT {
            let py = CURSOR_BACKUP_Y + row as i32;
            for col in 0..CURSOR_WIDTH {
                let px = CURSOR_BACKUP_X + col as i32;
                if px >= 0 && py >= 0 && (px as usize) < width && (py as usize) < height {
                    let offset = py as usize * stride + px as usize;
                    *fb.add(offset) = CURSOR_BACKUP[row][col];
                }
            }
        }

        CURSOR_VISIBLE = false;
    }
}

/// Draw cursor with background save/restore support
pub fn cursor_show(x: i32, y: i32, cursor_type: CursorType) {
    unsafe {
        // Restore previous position if visible
        if CURSOR_VISIBLE {
            cursor_restore_background();
        }

        // Save new position
        cursor_save_background(x, y);

        // Draw cursor
        draw_cursor(x, y, cursor_type);
        CURSOR_VISIBLE = true;
    }
}

/// Hide cursor (restore background)
pub fn cursor_hide() {
    unsafe {
        if CURSOR_VISIBLE {
            cursor_restore_background();
            CURSOR_VISIBLE = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_constants() {
        // Verify alpha channel is set
        assert_eq!(COLOR_BACKGROUND >> 24, 0xFF);
        assert_eq!(COLOR_ACCENT >> 24, 0xFF);
    }
}
