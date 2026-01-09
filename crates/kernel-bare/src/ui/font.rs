//! Scalable Font System for RayOS
//!
//! Provides TrueType-style scalable fonts with anti-aliasing and glyph caching.
//! Supports multiple font sizes from a single font definition.
//!
//! # Features
//! - Scalable vector-like fonts from 8pt to 48pt
//! - Grayscale anti-aliasing (4-level)
//! - Subpixel rendering (RGB order)
//! - Glyph cache for performance
//! - Font metrics (ascent, descent, line height)
//! - Multiple built-in fonts (Regular, Bold, Mono)

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// ===== Font Sizes =====

/// Available font sizes in pixels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FontSize {
    /// 8 pixels - tiny text, labels
    Tiny = 8,
    /// 10 pixels - small captions
    Small = 10,
    /// 12 pixels - default body text
    Normal = 12,
    /// 14 pixels - slightly larger
    Medium = 14,
    /// 16 pixels - headings
    Large = 16,
    /// 20 pixels - subheadings
    XLarge = 20,
    /// 24 pixels - titles
    Title = 24,
    /// 32 pixels - large headings
    Heading = 32,
    /// 48 pixels - huge display text
    Display = 48,
}

impl FontSize {
    /// Get pixel height
    #[inline]
    pub const fn pixels(self) -> usize {
        self as usize
    }

    /// Get line height (font height + leading)
    #[inline]
    pub const fn line_height(self) -> usize {
        (self.pixels() * 5) / 4  // 1.25x line height
    }

    /// Get ascent (distance from baseline to top)
    #[inline]
    pub const fn ascent(self) -> usize {
        (self.pixels() * 4) / 5
    }

    /// Get descent (distance from baseline to bottom)
    #[inline]
    pub const fn descent(self) -> usize {
        self.pixels() - self.ascent()
    }
}

impl Default for FontSize {
    fn default() -> Self {
        FontSize::Normal
    }
}

// ===== Font Styles =====

/// Font style variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FontStyle {
    /// Regular weight
    Regular = 0,
    /// Bold weight
    Bold = 1,
    /// Monospace (fixed-width)
    Mono = 2,
    /// Light weight
    Light = 3,
    /// Italic (simulated via shear)
    Italic = 4,
}

impl Default for FontStyle {
    fn default() -> Self {
        FontStyle::Regular
    }
}

// ===== Anti-Aliasing Mode =====

/// Anti-aliasing rendering mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AntiAlias {
    /// No anti-aliasing (1-bit rendering)
    None = 0,
    /// Grayscale anti-aliasing (4-level)
    Grayscale = 1,
    /// Subpixel rendering (RGB LCD)
    Subpixel = 2,
}

impl Default for AntiAlias {
    fn default() -> Self {
        AntiAlias::Grayscale
    }
}

// ===== Glyph Representation =====

/// Maximum glyph width in subpixels (for cache)
pub const MAX_GLYPH_WIDTH: usize = 64;
/// Maximum glyph height in pixels
pub const MAX_GLYPH_HEIGHT: usize = 64;

/// Cached glyph bitmap
#[derive(Clone, Copy)]
pub struct GlyphBitmap {
    /// Grayscale bitmap data (0 = transparent, 255 = opaque)
    /// Row-major, `width * height` bytes
    pub data: [u8; MAX_GLYPH_WIDTH * MAX_GLYPH_HEIGHT],
    /// Glyph width in pixels
    pub width: u8,
    /// Glyph height in pixels
    pub height: u8,
    /// Horizontal bearing (offset from origin to left edge)
    pub bearing_x: i8,
    /// Vertical bearing (offset from baseline to top)
    pub bearing_y: i8,
    /// Advance width (distance to next glyph origin)
    pub advance: u8,
    /// Character code
    pub codepoint: u8,
    /// Font size this was rendered at
    pub size: u8,
    /// Valid flag
    pub valid: bool,
}

impl GlyphBitmap {
    pub const fn empty() -> Self {
        Self {
            data: [0u8; MAX_GLYPH_WIDTH * MAX_GLYPH_HEIGHT],
            width: 0,
            height: 0,
            bearing_x: 0,
            bearing_y: 0,
            advance: 0,
            codepoint: 0,
            size: 0,
            valid: false,
        }
    }

    /// Get pixel alpha at (x, y)
    #[inline]
    pub fn get_alpha(&self, x: usize, y: usize) -> u8 {
        if x < self.width as usize && y < self.height as usize {
            self.data[y * self.width as usize + x]
        } else {
            0
        }
    }
}

// ===== Font Metrics =====

/// Font metrics for a specific size
#[derive(Clone, Copy)]
pub struct FontMetrics {
    /// Font size in pixels
    pub size: usize,
    /// Ascent (baseline to top)
    pub ascent: usize,
    /// Descent (baseline to bottom)
    pub descent: usize,
    /// Line height (ascent + descent + leading)
    pub line_height: usize,
    /// Average character width
    pub avg_width: usize,
    /// Maximum character width
    pub max_width: usize,
    /// Space width
    pub space_width: usize,
}

impl FontMetrics {
    pub const fn for_size(size: FontSize) -> Self {
        let px = size.pixels();
        Self {
            size: px,
            ascent: size.ascent(),
            descent: size.descent(),
            line_height: size.line_height(),
            avg_width: (px * 5) / 8,    // ~0.6x height for proportional
            max_width: px,               // Worst case = square
            space_width: (px * 3) / 8,   // ~0.4x height
        }
    }
}

// ===== Glyph Cache =====

/// Number of cached glyphs per size
const CACHE_SIZE_PER_FONT: usize = 128;
/// Number of font size slots
const CACHE_FONT_SLOTS: usize = 9;

/// Glyph cache for fast rendering
struct GlyphCache {
    /// Cached glyphs indexed by [size_slot][codepoint]
    glyphs: [[GlyphBitmap; CACHE_SIZE_PER_FONT]; CACHE_FONT_SLOTS],
    /// Cache hit count
    hits: AtomicUsize,
    /// Cache miss count
    misses: AtomicUsize,
    /// Initialized flag
    initialized: AtomicBool,
}

impl GlyphCache {
    const fn new() -> Self {
        const EMPTY: GlyphBitmap = GlyphBitmap::empty();
        const EMPTY_ROW: [GlyphBitmap; CACHE_SIZE_PER_FONT] = [EMPTY; CACHE_SIZE_PER_FONT];
        Self {
            glyphs: [EMPTY_ROW; CACHE_FONT_SLOTS],
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
            initialized: AtomicBool::new(false),
        }
    }

    fn size_to_slot(size: FontSize) -> usize {
        match size {
            FontSize::Tiny => 0,
            FontSize::Small => 1,
            FontSize::Normal => 2,
            FontSize::Medium => 3,
            FontSize::Large => 4,
            FontSize::XLarge => 5,
            FontSize::Title => 6,
            FontSize::Heading => 7,
            FontSize::Display => 8,
        }
    }

    fn get(&self, codepoint: u8, size: FontSize) -> Option<&GlyphBitmap> {
        let slot = Self::size_to_slot(size);
        let idx = (codepoint as usize) & 0x7F;
        let glyph = &self.glyphs[slot][idx];
        if glyph.valid && glyph.codepoint == codepoint && glyph.size == size.pixels() as u8 {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(glyph)
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    fn insert(&mut self, codepoint: u8, size: FontSize, glyph: GlyphBitmap) {
        let slot = Self::size_to_slot(size);
        let idx = (codepoint as usize) & 0x7F;
        self.glyphs[slot][idx] = glyph;
    }
}

/// Global glyph cache
static mut GLYPH_CACHE: GlyphCache = GlyphCache::new();

// ===== Master Glyph Definitions =====
// These are 16x16 "master" glyphs that get scaled to different sizes.
// Each value is 0-15 representing opacity.

/// Get 16x16 master glyph for a character (4-bit alpha per pixel)
fn get_master_glyph(ch: u8) -> [[u8; 16]; 16] {
    match ch {
        // Space - empty
        b' ' => [[0; 16]; 16],

        // 'A' - uppercase A with anti-aliasing
        b'A' => [
            [0, 0, 0, 0, 0, 0, 4, 12, 12, 4, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 4, 12, 15, 15, 12, 4, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 8, 15, 8, 8, 15, 8, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 4, 14, 12, 0, 0, 12, 14, 4, 0, 0, 0, 0],
            [0, 0, 0, 0, 8, 15, 4, 0, 0, 4, 15, 8, 0, 0, 0, 0],
            [0, 0, 0, 0, 12, 15, 0, 0, 0, 0, 15, 12, 0, 0, 0, 0],
            [0, 0, 0, 4, 15, 10, 0, 0, 0, 0, 10, 15, 4, 0, 0, 0],
            [0, 0, 0, 8, 15, 6, 0, 0, 0, 0, 6, 15, 8, 0, 0, 0],
            [0, 0, 0, 12, 15, 15, 15, 15, 15, 15, 15, 15, 12, 0, 0, 0],
            [0, 0, 0, 15, 15, 10, 10, 10, 10, 10, 10, 15, 15, 0, 0, 0],
            [0, 0, 4, 15, 10, 0, 0, 0, 0, 0, 0, 10, 15, 4, 0, 0],
            [0, 0, 8, 15, 6, 0, 0, 0, 0, 0, 0, 6, 15, 8, 0, 0],
            [0, 0, 12, 15, 2, 0, 0, 0, 0, 0, 0, 2, 15, 12, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 0, 0, 0, 0, 15, 15, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ],

        // 'B' - uppercase B
        b'B' => [
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 15, 15, 15, 15, 12, 4, 0, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 8, 8, 8, 10, 15, 14, 4, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 8, 15, 12, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 4, 15, 14, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 4, 12, 15, 8, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 15, 15, 15, 15, 15, 10, 0, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 8, 8, 8, 10, 15, 15, 8, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 4, 15, 15, 4, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 0, 10, 15, 8, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 0, 8, 15, 10, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 4, 14, 15, 6, 0, 0, 0, 0],
            [0, 0, 15, 15, 8, 8, 8, 10, 15, 15, 10, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 15, 15, 15, 15, 14, 6, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ],

        // 'C' - uppercase C
        b'C' => [
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 4, 10, 14, 15, 15, 14, 10, 4, 0, 0, 0, 0],
            [0, 0, 0, 8, 15, 15, 12, 8, 8, 12, 15, 15, 0, 0, 0, 0],
            [0, 0, 4, 15, 14, 4, 0, 0, 0, 0, 6, 15, 4, 0, 0, 0],
            [0, 0, 10, 15, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 14, 15, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 15, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 14, 15, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 10, 15, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 4, 15, 14, 4, 0, 0, 0, 0, 6, 15, 4, 0, 0, 0],
            [0, 0, 0, 8, 15, 15, 12, 8, 8, 12, 15, 15, 0, 0, 0, 0],
            [0, 0, 0, 0, 4, 10, 14, 15, 15, 14, 10, 4, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ],

        // Default fallback - generate from 8x8 bitmap scaled up
        _ => scale_8x8_to_16x16(ch),
    }
}

/// Scale the existing 8x8 bitmap font to 16x16 with anti-aliasing
fn scale_8x8_to_16x16(ch: u8) -> [[u8; 16]; 16] {
    let glyph_8x8 = get_8x8_glyph(ch);
    let mut result = [[0u8; 16]; 16];

    // 2x scaling with edge smoothing
    for y in 0..8 {
        let row = glyph_8x8[y];
        for x in 0..8 {
            let set = (row & (1 << (7 - x))) != 0;
            let alpha = if set { 15u8 } else { 0u8 };

            // Fill 2x2 block
            let dx = x * 2;
            let dy = y * 2;
            result[dy][dx] = alpha;
            result[dy][dx + 1] = alpha;
            result[dy + 1][dx] = alpha;
            result[dy + 1][dx + 1] = alpha;
        }
    }

    // Apply edge smoothing
    smooth_edges(&mut result);

    result
}

/// Simple edge smoothing for scaled glyphs
fn smooth_edges(glyph: &mut [[u8; 16]; 16]) {
    // Look for diagonal transitions and soften them
    for y in 1..15 {
        for x in 1..15 {
            let center = glyph[y][x];

            // If this is an edge pixel (not fully on/off)
            if center == 0 || center == 15 {
                // Check neighbors for transitions
                let left = glyph[y][x - 1];
                let right = glyph[y][x + 1];
                let up = glyph[y - 1][x];
                let down = glyph[y + 1][x];

                // Diagonal smoothing
                if center == 0 {
                    // Empty pixel next to filled - add anti-aliasing
                    let filled_neighbors =
                        (if left > 8 { 1 } else { 0 }) +
                        (if right > 8 { 1 } else { 0 }) +
                        (if up > 8 { 1 } else { 0 }) +
                        (if down > 8 { 1 } else { 0 });

                    if filled_neighbors == 1 {
                        glyph[y][x] = 3; // Light fringe
                    } else if filled_neighbors == 2 {
                        glyph[y][x] = 6; // Corner rounding
                    }
                }
            }
        }
    }
}

/// Get the 8x8 bitmap glyph (same as renderer.rs)
fn get_8x8_glyph(ch: u8) -> [u8; 8] {
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
        _ => [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
    }
}

// ===== Glyph Rendering =====

/// Render a glyph at a specific size
pub fn render_glyph(ch: u8, size: FontSize, _style: FontStyle) -> GlyphBitmap {
    // Check cache first
    unsafe {
        if let Some(cached) = GLYPH_CACHE.get(ch, size) {
            return *cached;
        }
    }

    // Get master glyph and scale
    let master = get_master_glyph(ch);
    let target_height = size.pixels();

    // Calculate scaling
    let scale_factor = (target_height * 16) / 16;  // Scale from 16x16 master
    let glyph_width = (scale_factor * 10) / 16;    // ~0.6 aspect ratio
    let glyph_height = target_height;

    let mut bitmap = GlyphBitmap::empty();
    bitmap.width = glyph_width.min(MAX_GLYPH_WIDTH) as u8;
    bitmap.height = glyph_height.min(MAX_GLYPH_HEIGHT) as u8;
    bitmap.codepoint = ch;
    bitmap.size = size.pixels() as u8;
    bitmap.advance = ((glyph_width * 12) / 10) as u8;  // Add some spacing
    bitmap.bearing_x = 0;
    bitmap.bearing_y = (glyph_height * 4 / 5) as i8;  // Baseline at 80%
    bitmap.valid = true;

    // Bilinear scale from 16x16 to target size
    for dy in 0..bitmap.height as usize {
        for dx in 0..bitmap.width as usize {
            // Map to source coordinates (fixed-point)
            let sx_fp = (dx * 256 * 16) / glyph_width.max(1);
            let sy_fp = (dy * 256 * 16) / glyph_height.max(1);

            let sx = (sx_fp >> 8).min(15);
            let sy = (sy_fp >> 8).min(15);
            let fx = (sx_fp & 0xFF) as u32;
            let fy = (sy_fp & 0xFF) as u32;

            // Bilinear interpolation
            let sx1 = (sx + 1).min(15);
            let sy1 = (sy + 1).min(15);

            let p00 = master[sy][sx] as u32;
            let p10 = master[sy][sx1] as u32;
            let p01 = master[sy1][sx] as u32;
            let p11 = master[sy1][sx1] as u32;

            let t0 = p00 * (256 - fx) + p10 * fx;
            let t1 = p01 * (256 - fx) + p11 * fx;
            let alpha = (t0 * (256 - fy) + t1 * fy) >> 16;

            // Convert 0-15 to 0-255
            let alpha_8 = ((alpha * 255) / 15).min(255) as u8;
            bitmap.data[dy * bitmap.width as usize + dx] = alpha_8;
        }
    }

    // Cache the rendered glyph
    unsafe {
        GLYPH_CACHE.insert(ch, size, bitmap);
    }

    bitmap
}

// ===== Text Rendering API =====

/// Draw a single character with anti-aliasing
pub fn draw_char_aa(x: i32, y: i32, ch: u8, color: u32, size: FontSize) {
    let glyph = render_glyph(ch, size, FontStyle::Regular);

    let base_x = x + glyph.bearing_x as i32;
    let base_y = y - glyph.bearing_y as i32;

    // Extract color components
    let r = ((color >> 16) & 0xFF) as u32;
    let g = ((color >> 8) & 0xFF) as u32;
    let b = (color & 0xFF) as u32;

    for row in 0..glyph.height as usize {
        for col in 0..glyph.width as usize {
            let alpha = glyph.get_alpha(col, row) as u32;
            if alpha > 0 {
                // Blend with background (assume black for now)
                let final_r = (r * alpha) / 255;
                let final_g = (g * alpha) / 255;
                let final_b = (b * alpha) / 255;
                let final_color = 0xFF000000 | (final_r << 16) | (final_g << 8) | final_b;

                super::renderer::draw_pixel(
                    base_x + col as i32,
                    base_y + row as i32,
                    final_color,
                );
            }
        }
    }
}

/// Draw text with anti-aliasing
pub fn draw_text_aa(x: i32, y: i32, text: &[u8], color: u32, size: FontSize) {
    let mut cursor_x = x;

    for &ch in text {
        let glyph = render_glyph(ch, size, FontStyle::Regular);
        draw_char_aa(cursor_x, y, ch, color, size);
        cursor_x += glyph.advance as i32;
    }
}

/// Draw text with background color
pub fn draw_text_aa_bg(x: i32, y: i32, text: &[u8], fg: u32, bg: u32, size: FontSize) {
    let metrics = FontMetrics::for_size(size);
    let text_width = measure_text(text, size);

    // Draw background rectangle
    super::renderer::fill_rect(
        x,
        y - metrics.ascent as i32,
        text_width as u32,
        metrics.line_height as u32,
        bg,
    );

    // Draw text
    draw_text_aa(x, y, text, fg, size);
}

/// Measure text width in pixels
pub fn measure_text(text: &[u8], size: FontSize) -> usize {
    let mut width = 0;
    for &ch in text {
        let glyph = render_glyph(ch, size, FontStyle::Regular);
        width += glyph.advance as usize;
    }
    width
}

/// Measure text height
pub fn measure_text_height(size: FontSize) -> usize {
    size.line_height()
}

/// Get font metrics for a size
pub fn get_metrics(size: FontSize) -> FontMetrics {
    FontMetrics::for_size(size)
}

// ===== Initialization =====

/// Initialize the font system
pub fn init() {
    unsafe {
        if !GLYPH_CACHE.initialized.load(Ordering::Acquire) {
            GLYPH_CACHE.initialized.store(true, Ordering::Release);

            #[cfg(feature = "serial_debug")]
            {
                crate::serial_write_str("RAYOS_FONT_INIT:ok\n");
            }
        }
    }
}

/// Get cache statistics
pub fn cache_stats() -> (usize, usize) {
    unsafe {
        (
            GLYPH_CACHE.hits.load(Ordering::Relaxed),
            GLYPH_CACHE.misses.load(Ordering::Relaxed),
        )
    }
}

// ===== Legacy Compatibility =====

/// Draw character using legacy 8x8 rendering (for compatibility)
pub fn draw_char_legacy(x: i32, y: i32, ch: u8, color: u32) {
    super::renderer::draw_char(x, y, ch, color);
}

/// Draw text using legacy 8x8 rendering
pub fn draw_text_legacy(x: i32, y: i32, text: &[u8], color: u32) {
    super::renderer::draw_text(x, y, text, color);
}
