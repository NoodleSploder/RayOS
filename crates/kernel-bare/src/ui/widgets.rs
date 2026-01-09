//! Widget Library for RayOS UI
//!
//! Provides reusable UI components for building native RayOS applications.
//!
//! # Widgets
//!
//! - `Label` - Static text display
//! - `Button` - Clickable button with text
//! - `TextInput` - Editable text field
//!
//! # Example
//!
//! ```ignore
//! let label = Label::new(b"Hello, World!");
//! label.render(100, 100);
//!
//! let button = Button::new(b"Click Me");
//! button.render(100, 130);
//! ```

use super::renderer::{
    self, COLOR_ACCENT, COLOR_BACKGROUND, COLOR_BLACK, COLOR_BORDER, COLOR_TEXT,
    COLOR_TEXT_DIM, COLOR_WHITE, COLOR_WINDOW_BG, FONT_HEIGHT, FONT_WIDTH,
};

// ===== Widget Trait =====

/// Rectangle bounds for widget layout
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x + self.width as i32
            && py < self.y + self.height as i32
    }
}

/// Text alignment within a widget
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment within a widget
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum VAlignment {
    Top,
    #[default]
    Center,
    Bottom,
}

/// Widget state for interactive elements
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum WidgetState {
    #[default]
    Normal,
    Hovered,
    Pressed,
    Focused,
    Disabled,
}

// ===== Label Widget =====

/// Maximum label text length
pub const LABEL_MAX_LEN: usize = 128;

/// A static text label widget.
///
/// Labels display non-editable text with configurable color and alignment.
#[derive(Clone)]
pub struct Label {
    text: [u8; LABEL_MAX_LEN],
    text_len: usize,
    color: u32,
    bg_color: Option<u32>,
    alignment: Alignment,
    valignment: VAlignment,
    bounds: Rect,
}

impl Label {
    /// Create a new label with the given text.
    pub fn new(text: &[u8]) -> Self {
        let mut label = Self {
            text: [0u8; LABEL_MAX_LEN],
            text_len: 0,
            color: COLOR_TEXT,
            bg_color: None,
            alignment: Alignment::Left,
            valignment: VAlignment::Center,
            bounds: Rect::default(),
        };
        label.set_text(text);
        label
    }

    /// Set the label text.
    pub fn set_text(&mut self, text: &[u8]) {
        let len = text.len().min(LABEL_MAX_LEN);
        self.text[..len].copy_from_slice(&text[..len]);
        self.text_len = len;
    }

    /// Get the label text.
    pub fn text(&self) -> &[u8] {
        &self.text[..self.text_len]
    }

    /// Set text color.
    pub fn set_color(&mut self, color: u32) {
        self.color = color;
    }

    /// Set background color (None for transparent).
    pub fn set_bg_color(&mut self, color: Option<u32>) {
        self.bg_color = color;
    }

    /// Set horizontal alignment.
    pub fn set_alignment(&mut self, alignment: Alignment) {
        self.alignment = alignment;
    }

    /// Set vertical alignment.
    pub fn set_valignment(&mut self, valignment: VAlignment) {
        self.valignment = valignment;
    }

    /// Set bounds for the label.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Calculate the preferred size for this label.
    pub fn preferred_size(&self) -> (u32, u32) {
        let width = (self.text_len * FONT_WIDTH) as u32;
        let height = FONT_HEIGHT as u32;
        (width, height)
    }

    /// Render the label at the specified position.
    pub fn render(&self, x: i32, y: i32) {
        self.render_in_bounds(Rect::new(x, y, u32::MAX, u32::MAX));
    }

    /// Render the label within the given bounds.
    pub fn render_in_bounds(&self, bounds: Rect) {
        let text_width = (self.text_len * FONT_WIDTH) as u32;
        let text_height = FONT_HEIGHT as u32;

        // Calculate x position based on alignment
        let x = match self.alignment {
            Alignment::Left => bounds.x,
            Alignment::Center => bounds.x + (bounds.width.saturating_sub(text_width) / 2) as i32,
            Alignment::Right => bounds.x + bounds.width.saturating_sub(text_width) as i32,
        };

        // Calculate y position based on vertical alignment
        let y = match self.valignment {
            VAlignment::Top => bounds.y,
            VAlignment::Center => bounds.y + (bounds.height.saturating_sub(text_height) / 2) as i32,
            VAlignment::Bottom => bounds.y + bounds.height.saturating_sub(text_height) as i32,
        };

        // Draw background if set
        if let Some(bg) = self.bg_color {
            renderer::fill_rect(x, y, text_width, text_height, bg);
        }

        // Draw text
        renderer::draw_text(x, y, &self.text[..self.text_len], self.color);
    }
}

// ===== Button Widget =====

/// Maximum button text length
pub const BUTTON_MAX_LEN: usize = 32;

/// Button padding in pixels
pub const BUTTON_PADDING_X: u32 = 12;
pub const BUTTON_PADDING_Y: u32 = 6;

/// Button colors
pub const BUTTON_BG_NORMAL: u32 = 0xFF3D3D5C;
pub const BUTTON_BG_HOVERED: u32 = 0xFF4D4D7C;
pub const BUTTON_BG_PRESSED: u32 = 0xFF2D2D4C;
pub const BUTTON_BG_DISABLED: u32 = 0xFF2D2D3D;
pub const BUTTON_BORDER: u32 = 0xFF5D5D8C;
pub const BUTTON_TEXT_DISABLED: u32 = 0xFF606060;

/// A clickable button widget.
///
/// Buttons respond to mouse hover and click events.
#[derive(Clone)]
pub struct Button {
    text: [u8; BUTTON_MAX_LEN],
    text_len: usize,
    state: WidgetState,
    bounds: Rect,
    enabled: bool,
}

impl Button {
    /// Create a new button with the given text.
    pub fn new(text: &[u8]) -> Self {
        let mut button = Self {
            text: [0u8; BUTTON_MAX_LEN],
            text_len: 0,
            state: WidgetState::Normal,
            bounds: Rect::default(),
            enabled: true,
        };
        button.set_text(text);
        button
    }

    /// Set the button text.
    pub fn set_text(&mut self, text: &[u8]) {
        let len = text.len().min(BUTTON_MAX_LEN);
        self.text[..len].copy_from_slice(&text[..len]);
        self.text_len = len;
    }

    /// Get the button text.
    pub fn text(&self) -> &[u8] {
        &self.text[..self.text_len]
    }

    /// Set enabled state.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.state = WidgetState::Disabled;
        } else if self.state == WidgetState::Disabled {
            self.state = WidgetState::Normal;
        }
    }

    /// Check if button is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the current state.
    pub fn set_state(&mut self, state: WidgetState) {
        if self.enabled || state == WidgetState::Disabled {
            self.state = state;
        }
    }

    /// Get the current state.
    pub fn state(&self) -> WidgetState {
        self.state
    }

    /// Set bounds for the button.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Get bounds.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Calculate the preferred size for this button.
    pub fn preferred_size(&self) -> (u32, u32) {
        let text_width = (self.text_len * FONT_WIDTH) as u32;
        let text_height = FONT_HEIGHT as u32;
        (
            text_width + BUTTON_PADDING_X * 2,
            text_height + BUTTON_PADDING_Y * 2,
        )
    }

    /// Check if a point is within the button bounds.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.bounds.contains(x, y)
    }

    /// Handle mouse hover - returns true if state changed.
    pub fn on_hover(&mut self, x: i32, y: i32) -> bool {
        if !self.enabled {
            return false;
        }
        let was_hovered = self.state == WidgetState::Hovered;
        let is_hovered = self.contains(x, y);

        if is_hovered && self.state == WidgetState::Normal {
            self.state = WidgetState::Hovered;
            return true;
        } else if !is_hovered && self.state == WidgetState::Hovered {
            self.state = WidgetState::Normal;
            return true;
        }
        false
    }

    /// Handle mouse press - returns true if button was clicked.
    pub fn on_press(&mut self, x: i32, y: i32) -> bool {
        if !self.enabled {
            return false;
        }
        if self.contains(x, y) {
            self.state = WidgetState::Pressed;
            true
        } else {
            false
        }
    }

    /// Handle mouse release - returns true if button was activated (clicked).
    pub fn on_release(&mut self, x: i32, y: i32) -> bool {
        if !self.enabled {
            return false;
        }
        let was_pressed = self.state == WidgetState::Pressed;
        if self.contains(x, y) {
            self.state = WidgetState::Hovered;
            was_pressed // Only activate if we were pressed
        } else {
            self.state = WidgetState::Normal;
            false
        }
    }

    /// Render the button at the specified position.
    pub fn render(&self, x: i32, y: i32) {
        let (width, height) = self.preferred_size();
        self.render_with_size(x, y, width, height);
    }

    /// Render the button with specific dimensions.
    pub fn render_with_size(&self, x: i32, y: i32, width: u32, height: u32) {
        // Choose colors based on state
        let (bg_color, text_color, border_color) = match self.state {
            WidgetState::Normal => (BUTTON_BG_NORMAL, COLOR_TEXT, BUTTON_BORDER),
            WidgetState::Hovered => (BUTTON_BG_HOVERED, COLOR_WHITE, COLOR_ACCENT),
            WidgetState::Pressed => (BUTTON_BG_PRESSED, COLOR_TEXT, COLOR_ACCENT),
            WidgetState::Focused => (BUTTON_BG_NORMAL, COLOR_TEXT, COLOR_ACCENT),
            WidgetState::Disabled => (BUTTON_BG_DISABLED, BUTTON_TEXT_DISABLED, COLOR_BORDER),
        };

        // Draw button background
        renderer::fill_rect(x, y, width, height, bg_color);

        // Draw border
        renderer::draw_rect(x, y, width, height, border_color, 1);

        // Calculate text position (centered)
        let text_width = (self.text_len * FONT_WIDTH) as u32;
        let text_height = FONT_HEIGHT as u32;
        let text_x = x + (width.saturating_sub(text_width) / 2) as i32;
        let text_y = y + (height.saturating_sub(text_height) / 2) as i32;

        // Draw text
        renderer::draw_text(text_x, text_y, &self.text[..self.text_len], text_color);
    }
}

// ===== TextInput Widget =====

/// Maximum text input length
pub const TEXT_INPUT_MAX_LEN: usize = 256;

/// TextInput padding
pub const TEXT_INPUT_PADDING_X: u32 = 8;
pub const TEXT_INPUT_PADDING_Y: u32 = 4;

/// TextInput colors
pub const TEXT_INPUT_BG: u32 = 0xFF1E1E2E;
pub const TEXT_INPUT_BG_FOCUSED: u32 = 0xFF252535;
pub const TEXT_INPUT_BORDER: u32 = 0xFF4D4D6D;
pub const TEXT_INPUT_BORDER_FOCUSED: u32 = COLOR_ACCENT;
pub const TEXT_INPUT_PLACEHOLDER: u32 = 0xFF606060;
pub const TEXT_INPUT_CURSOR: u32 = COLOR_ACCENT;

/// An editable text input widget.
///
/// Supports typing, cursor movement, backspace, and selection.
#[derive(Clone)]
pub struct TextInput {
    text: [u8; TEXT_INPUT_MAX_LEN],
    text_len: usize,
    placeholder: [u8; 64],
    placeholder_len: usize,
    cursor_pos: usize,
    selection_start: Option<usize>,
    focused: bool,
    enabled: bool,
    bounds: Rect,
    scroll_offset: usize,
    cursor_visible: bool,
    cursor_blink_counter: u32,
}

impl TextInput {
    /// Create a new empty text input.
    pub fn new() -> Self {
        Self {
            text: [0u8; TEXT_INPUT_MAX_LEN],
            text_len: 0,
            placeholder: [0u8; 64],
            placeholder_len: 0,
            cursor_pos: 0,
            selection_start: None,
            focused: false,
            enabled: true,
            bounds: Rect::default(),
            scroll_offset: 0,
            cursor_visible: true,
            cursor_blink_counter: 0,
        }
    }

    /// Create a text input with initial text.
    pub fn with_text(text: &[u8]) -> Self {
        let mut input = Self::new();
        input.set_text(text);
        input
    }

    /// Set the text content.
    pub fn set_text(&mut self, text: &[u8]) {
        let len = text.len().min(TEXT_INPUT_MAX_LEN);
        self.text[..len].copy_from_slice(&text[..len]);
        self.text_len = len;
        self.cursor_pos = len;
        self.selection_start = None;
    }

    /// Get the current text.
    pub fn text(&self) -> &[u8] {
        &self.text[..self.text_len]
    }

    /// Set placeholder text.
    pub fn set_placeholder(&mut self, text: &[u8]) {
        let len = text.len().min(64);
        self.placeholder[..len].copy_from_slice(&text[..len]);
        self.placeholder_len = len;
    }

    /// Clear the text.
    pub fn clear(&mut self) {
        self.text_len = 0;
        self.cursor_pos = 0;
        self.selection_start = None;
        self.scroll_offset = 0;
    }

    /// Set focused state.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if focused {
            self.cursor_visible = true;
            self.cursor_blink_counter = 0;
        }
    }

    /// Check if focused.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Set enabled state.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.focused = false;
        }
    }

    /// Check if enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Get bounds.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Check if a point is within bounds.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.bounds.contains(x, y)
    }

    /// Handle a character input.
    pub fn on_char(&mut self, ch: u8) -> bool {
        if !self.enabled || !self.focused {
            return false;
        }
        // Only accept printable ASCII
        if ch < 0x20 || ch > 0x7E {
            return false;
        }
        if self.text_len >= TEXT_INPUT_MAX_LEN {
            return false;
        }

        // Delete selection first
        self.delete_selection();

        // Insert character at cursor
        if self.cursor_pos < self.text_len {
            // Shift text right
            for i in (self.cursor_pos..self.text_len).rev() {
                self.text[i + 1] = self.text[i];
            }
        }
        self.text[self.cursor_pos] = ch;
        self.text_len += 1;
        self.cursor_pos += 1;
        self.ensure_cursor_visible();
        true
    }

    /// Handle backspace.
    pub fn on_backspace(&mut self) -> bool {
        if !self.enabled || !self.focused {
            return false;
        }
        if self.selection_start.is_some() {
            return self.delete_selection();
        }
        if self.cursor_pos == 0 {
            return false;
        }

        // Shift text left
        for i in self.cursor_pos..self.text_len {
            self.text[i - 1] = self.text[i];
        }
        self.text_len -= 1;
        self.cursor_pos -= 1;
        self.ensure_cursor_visible();
        true
    }

    /// Handle delete key.
    pub fn on_delete(&mut self) -> bool {
        if !self.enabled || !self.focused {
            return false;
        }
        if self.selection_start.is_some() {
            return self.delete_selection();
        }
        if self.cursor_pos >= self.text_len {
            return false;
        }

        // Shift text left
        for i in (self.cursor_pos + 1)..self.text_len {
            self.text[i - 1] = self.text[i];
        }
        self.text_len -= 1;
        true
    }

    /// Move cursor left.
    pub fn cursor_left(&mut self, select: bool) {
        if self.cursor_pos > 0 {
            if select && self.selection_start.is_none() {
                self.selection_start = Some(self.cursor_pos);
            } else if !select {
                self.selection_start = None;
            }
            self.cursor_pos -= 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor right.
    pub fn cursor_right(&mut self, select: bool) {
        if self.cursor_pos < self.text_len {
            if select && self.selection_start.is_none() {
                self.selection_start = Some(self.cursor_pos);
            } else if !select {
                self.selection_start = None;
            }
            self.cursor_pos += 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor to start.
    pub fn cursor_home(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor_pos = 0;
        self.scroll_offset = 0;
    }

    /// Move cursor to end.
    pub fn cursor_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor_pos = self.text_len;
        self.ensure_cursor_visible();
    }

    /// Select all text.
    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor_pos = self.text_len;
    }

    /// Delete selected text.
    fn delete_selection(&mut self) -> bool {
        let Some(sel_start) = self.selection_start else {
            return false;
        };

        let start = sel_start.min(self.cursor_pos);
        let end = sel_start.max(self.cursor_pos);
        let delete_len = end - start;

        if delete_len == 0 {
            self.selection_start = None;
            return false;
        }

        // Shift text left
        for i in end..self.text_len {
            self.text[i - delete_len] = self.text[i];
        }
        self.text_len -= delete_len;
        self.cursor_pos = start;
        self.selection_start = None;
        self.ensure_cursor_visible();
        true
    }

    /// Ensure cursor is visible by adjusting scroll offset.
    fn ensure_cursor_visible(&mut self) {
        if self.bounds.width == 0 {
            return;
        }
        let visible_chars = ((self.bounds.width - TEXT_INPUT_PADDING_X * 2) / FONT_WIDTH as u32) as usize;
        if visible_chars == 0 {
            return;
        }

        if self.cursor_pos < self.scroll_offset {
            self.scroll_offset = self.cursor_pos;
        } else if self.cursor_pos > self.scroll_offset + visible_chars {
            self.scroll_offset = self.cursor_pos - visible_chars;
        }
    }

    /// Update cursor blink (call periodically).
    pub fn tick(&mut self) {
        if self.focused {
            self.cursor_blink_counter += 1;
            if self.cursor_blink_counter >= 30 {
                self.cursor_visible = !self.cursor_visible;
                self.cursor_blink_counter = 0;
            }
        }
    }

    /// Calculate preferred size.
    pub fn preferred_size(&self, min_chars: usize) -> (u32, u32) {
        let width = (min_chars * FONT_WIDTH) as u32 + TEXT_INPUT_PADDING_X * 2;
        let height = FONT_HEIGHT as u32 + TEXT_INPUT_PADDING_Y * 2;
        (width, height)
    }

    /// Handle click to focus and position cursor.
    pub fn on_click(&mut self, x: i32, y: i32) -> bool {
        if !self.enabled {
            return false;
        }
        if self.contains(x, y) {
            self.focused = true;
            self.selection_start = None;

            // Calculate cursor position from click
            let text_x = self.bounds.x + TEXT_INPUT_PADDING_X as i32;
            let click_offset = ((x - text_x) / FONT_WIDTH as i32).max(0) as usize;
            self.cursor_pos = (self.scroll_offset + click_offset).min(self.text_len);
            self.cursor_visible = true;
            self.cursor_blink_counter = 0;
            true
        } else {
            let was_focused = self.focused;
            self.focused = false;
            was_focused
        }
    }

    /// Render the text input.
    pub fn render(&self, x: i32, y: i32, width: u32) {
        let height = FONT_HEIGHT as u32 + TEXT_INPUT_PADDING_Y * 2;

        // Choose colors based on state
        let (bg_color, border_color) = if !self.enabled {
            (BUTTON_BG_DISABLED, COLOR_BORDER)
        } else if self.focused {
            (TEXT_INPUT_BG_FOCUSED, TEXT_INPUT_BORDER_FOCUSED)
        } else {
            (TEXT_INPUT_BG, TEXT_INPUT_BORDER)
        };

        // Draw background
        renderer::fill_rect(x, y, width, height, bg_color);

        // Draw border
        renderer::draw_rect(x, y, width, height, border_color, 1);

        let text_x = x + TEXT_INPUT_PADDING_X as i32;
        let text_y = y + TEXT_INPUT_PADDING_Y as i32;
        let visible_width = width.saturating_sub(TEXT_INPUT_PADDING_X * 2);
        let visible_chars = (visible_width / FONT_WIDTH as u32) as usize;

        // Draw selection highlight if any
        if let Some(sel_start) = self.selection_start {
            let start = sel_start.min(self.cursor_pos).saturating_sub(self.scroll_offset);
            let end = (sel_start.max(self.cursor_pos)).saturating_sub(self.scroll_offset).min(visible_chars);
            if start < end {
                let sel_x = text_x + (start * FONT_WIDTH) as i32;
                let sel_width = ((end - start) * FONT_WIDTH) as u32;
                renderer::fill_rect(sel_x, text_y, sel_width, FONT_HEIGHT as u32, COLOR_ACCENT & 0x80FFFFFF);
            }
        }

        // Draw text or placeholder
        if self.text_len == 0 && self.placeholder_len > 0 {
            let display_len = self.placeholder_len.min(visible_chars);
            renderer::draw_text(text_x, text_y, &self.placeholder[..display_len], TEXT_INPUT_PLACEHOLDER);
        } else {
            let display_start = self.scroll_offset;
            let display_end = (self.scroll_offset + visible_chars).min(self.text_len);
            if display_start < display_end {
                renderer::draw_text(
                    text_x,
                    text_y,
                    &self.text[display_start..display_end],
                    COLOR_TEXT,
                );
            }
        }

        // Draw cursor if focused and visible
        if self.focused && self.cursor_visible && self.enabled {
            let cursor_x = text_x + ((self.cursor_pos - self.scroll_offset) * FONT_WIDTH) as i32;
            renderer::fill_rect(cursor_x, text_y, 2, FONT_HEIGHT as u32, TEXT_INPUT_CURSOR);
        }
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_preferred_size() {
        let label = Label::new(b"Hello");
        let (w, h) = label.preferred_size();
        assert_eq!(w, 5 * FONT_WIDTH as u32);
        assert_eq!(h, FONT_HEIGHT as u32);
    }

    #[test]
    fn test_button_preferred_size() {
        let button = Button::new(b"Click");
        let (w, h) = button.preferred_size();
        assert_eq!(w, 5 * FONT_WIDTH as u32 + BUTTON_PADDING_X * 2);
        assert_eq!(h, FONT_HEIGHT as u32 + BUTTON_PADDING_Y * 2);
    }

    #[test]
    fn test_text_input_typing() {
        let mut input = TextInput::new();
        input.set_focused(true);
        input.set_bounds(Rect::new(0, 0, 200, 20));

        input.on_char(b'H');
        input.on_char(b'i');
        assert_eq!(input.text(), b"Hi");
        assert_eq!(input.cursor_pos, 2);

        input.on_backspace();
        assert_eq!(input.text(), b"H");
        assert_eq!(input.cursor_pos, 1);
    }
}
