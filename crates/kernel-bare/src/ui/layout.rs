//! Layout System for RayOS UI
//!
//! Provides container widgets for automatic layout of child elements.
//!
//! # Containers
//!
//! - `VStack` - Vertical stacking layout
//! - `HStack` - Horizontal stacking layout
//! - `Grid` - Grid-based layout
//!
//! # Example
//!
//! ```ignore
//! let mut vstack = VStack::new();
//! vstack.add_label(b"Item 1");
//! vstack.add_label(b"Item 2");
//! vstack.add_button(b"Click Me");
//! vstack.render(100, 100, 200);
//! ```

use super::renderer::{self, FONT_HEIGHT, FONT_WIDTH};
use super::widgets::{
    Alignment, Button, Label, Rect, TextInput, VAlignment, WidgetState,
    BUTTON_PADDING_X, BUTTON_PADDING_Y, TEXT_INPUT_PADDING_X, TEXT_INPUT_PADDING_Y,
};

// ===== Layout Constants =====

/// Default spacing between elements in a stack
pub const DEFAULT_SPACING: u32 = 8;

/// Maximum children in a layout container
pub const MAX_CHILDREN: usize = 32;

// ===== Layout Item =====

/// Type of item in a layout container
#[derive(Clone)]
pub enum LayoutItem {
    /// A label widget
    Label(Label),
    /// A button widget
    Button(Button),
    /// A text input widget
    TextInput(TextInput),
    /// Fixed-size spacer
    Spacer(u32),
    /// Flexible spacer (expands to fill)
    Flex(u32), // weight
}

impl LayoutItem {
    /// Get the preferred size of this item.
    pub fn preferred_size(&self) -> (u32, u32) {
        match self {
            LayoutItem::Label(label) => label.preferred_size(),
            LayoutItem::Button(button) => button.preferred_size(),
            LayoutItem::TextInput(input) => input.preferred_size(20),
            LayoutItem::Spacer(size) => (*size, *size),
            LayoutItem::Flex(_) => (0, 0),
        }
    }
}

// ===== VStack =====

/// A vertical stack layout container.
///
/// Children are arranged vertically from top to bottom.
#[derive(Clone)]
pub struct VStack {
    items: [Option<LayoutItem>; MAX_CHILDREN],
    item_count: usize,
    spacing: u32,
    alignment: Alignment,
    bounds: Rect,
    padding: u32,
}

impl VStack {
    /// Create a new empty VStack.
    pub fn new() -> Self {
        Self {
            items: core::array::from_fn(|_| None),
            item_count: 0,
            spacing: DEFAULT_SPACING,
            alignment: Alignment::Left,
            bounds: Rect::default(),
            padding: 0,
        }
    }

    /// Set spacing between items.
    pub fn set_spacing(&mut self, spacing: u32) {
        self.spacing = spacing;
    }

    /// Set horizontal alignment for items.
    pub fn set_alignment(&mut self, alignment: Alignment) {
        self.alignment = alignment;
    }

    /// Set padding around the container.
    pub fn set_padding(&mut self, padding: u32) {
        self.padding = padding;
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Add a generic item.
    pub fn add_item(&mut self, item: LayoutItem) -> bool {
        if self.item_count >= MAX_CHILDREN {
            return false;
        }
        self.items[self.item_count] = Some(item);
        self.item_count += 1;
        true
    }

    /// Add a label.
    pub fn add_label(&mut self, text: &[u8]) -> bool {
        self.add_item(LayoutItem::Label(Label::new(text)))
    }

    /// Add a label with color.
    pub fn add_label_colored(&mut self, text: &[u8], color: u32) -> bool {
        let mut label = Label::new(text);
        label.set_color(color);
        self.add_item(LayoutItem::Label(label))
    }

    /// Add a button and return its index.
    pub fn add_button(&mut self, text: &[u8]) -> Option<usize> {
        if self.item_count >= MAX_CHILDREN {
            return None;
        }
        let idx = self.item_count;
        self.items[idx] = Some(LayoutItem::Button(Button::new(text)));
        self.item_count += 1;
        Some(idx)
    }

    /// Add a text input and return its index.
    pub fn add_text_input(&mut self) -> Option<usize> {
        if self.item_count >= MAX_CHILDREN {
            return None;
        }
        let idx = self.item_count;
        self.items[idx] = Some(LayoutItem::TextInput(TextInput::new()));
        self.item_count += 1;
        Some(idx)
    }

    /// Add a fixed spacer.
    pub fn add_spacer(&mut self, size: u32) -> bool {
        self.add_item(LayoutItem::Spacer(size))
    }

    /// Add a flexible spacer.
    pub fn add_flex(&mut self, weight: u32) -> bool {
        self.add_item(LayoutItem::Flex(weight))
    }

    /// Get a mutable reference to a button by index.
    pub fn get_button_mut(&mut self, idx: usize) -> Option<&mut Button> {
        if idx >= self.item_count {
            return None;
        }
        match &mut self.items[idx] {
            Some(LayoutItem::Button(btn)) => Some(btn),
            _ => None,
        }
    }

    /// Get a mutable reference to a text input by index.
    pub fn get_text_input_mut(&mut self, idx: usize) -> Option<&mut TextInput> {
        if idx >= self.item_count {
            return None;
        }
        match &mut self.items[idx] {
            Some(LayoutItem::TextInput(input)) => Some(input),
            _ => None,
        }
    }

    /// Calculate the preferred size for this VStack.
    pub fn preferred_size(&self) -> (u32, u32) {
        let mut total_height = self.padding * 2;
        let mut max_width = 0u32;

        for i in 0..self.item_count {
            if let Some(item) = &self.items[i] {
                let (w, h) = item.preferred_size();
                max_width = max_width.max(w);
                total_height += h;
                if i > 0 {
                    total_height += self.spacing;
                }
            }
        }

        (max_width + self.padding * 2, total_height)
    }

    /// Calculate layout positions for all items.
    fn calculate_layout(&self, x: i32, y: i32, width: u32) -> [(i32, i32, u32, u32); MAX_CHILDREN] {
        let mut positions: [(i32, i32, u32, u32); MAX_CHILDREN] = [(0, 0, 0, 0); MAX_CHILDREN];
        let inner_x = x + self.padding as i32;
        let mut current_y = y + self.padding as i32;
        let inner_width = width.saturating_sub(self.padding * 2);

        // First pass: calculate fixed sizes and count flex items
        let mut flex_total_weight = 0u32;
        let mut fixed_height = 0u32;

        for i in 0..self.item_count {
            if let Some(item) = &self.items[i] {
                match item {
                    LayoutItem::Flex(weight) => flex_total_weight += weight,
                    _ => {
                        let (_, h) = item.preferred_size();
                        fixed_height += h;
                        if i > 0 {
                            fixed_height += self.spacing;
                        }
                    }
                }
            }
        }

        // Second pass: assign positions
        for i in 0..self.item_count {
            if let Some(item) = &self.items[i] {
                let (pref_w, pref_h) = item.preferred_size();

                // Calculate item x based on alignment
                let item_width = match item {
                    LayoutItem::TextInput(_) => inner_width, // Text inputs expand
                    LayoutItem::Button(_) => pref_w.min(inner_width),
                    _ => pref_w,
                };

                let item_x = match self.alignment {
                    Alignment::Left => inner_x,
                    Alignment::Center => inner_x + (inner_width.saturating_sub(item_width) / 2) as i32,
                    Alignment::Right => inner_x + inner_width.saturating_sub(item_width) as i32,
                };

                positions[i] = (item_x, current_y, item_width, pref_h);
                current_y += pref_h as i32 + self.spacing as i32;
            }
        }

        positions
    }

    /// Handle mouse hover on buttons. Returns index of hovered button if any.
    pub fn on_hover(&mut self, mx: i32, my: i32, x: i32, y: i32, width: u32) -> Option<usize> {
        let positions = self.calculate_layout(x, y, width);
        let mut hovered = None;

        for i in 0..self.item_count {
            if let Some(LayoutItem::Button(btn)) = &mut self.items[i] {
                let (bx, by, bw, bh) = positions[i];
                let bounds = Rect::new(bx, by, bw, bh);
                btn.set_bounds(bounds);
                if btn.on_hover(mx, my) {
                    hovered = Some(i);
                }
            }
        }

        hovered
    }

    /// Handle mouse press. Returns index of pressed button if any.
    pub fn on_press(&mut self, mx: i32, my: i32, x: i32, y: i32, width: u32) -> Option<usize> {
        let positions = self.calculate_layout(x, y, width);

        for i in 0..self.item_count {
            if let Some(LayoutItem::Button(btn)) = &mut self.items[i] {
                let (bx, by, bw, bh) = positions[i];
                let bounds = Rect::new(bx, by, bw, bh);
                btn.set_bounds(bounds);
                if btn.on_press(mx, my) {
                    return Some(i);
                }
            }
        }

        None
    }

    /// Handle mouse release. Returns index of clicked button if any.
    pub fn on_release(&mut self, mx: i32, my: i32, x: i32, y: i32, width: u32) -> Option<usize> {
        let positions = self.calculate_layout(x, y, width);

        for i in 0..self.item_count {
            if let Some(LayoutItem::Button(btn)) = &mut self.items[i] {
                let (bx, by, bw, bh) = positions[i];
                let bounds = Rect::new(bx, by, bw, bh);
                btn.set_bounds(bounds);
                if btn.on_release(mx, my) {
                    return Some(i);
                }
            }
        }

        None
    }

    /// Handle click for text inputs. Returns index of focused input if any.
    pub fn on_click(&mut self, mx: i32, my: i32, x: i32, y: i32, width: u32) -> Option<usize> {
        let positions = self.calculate_layout(x, y, width);
        let mut focused = None;

        for i in 0..self.item_count {
            if let Some(LayoutItem::TextInput(input)) = &mut self.items[i] {
                let (ix, iy, iw, ih) = positions[i];
                let bounds = Rect::new(ix, iy, iw, ih);
                input.set_bounds(bounds);
                if input.on_click(mx, my) && input.is_focused() {
                    focused = Some(i);
                }
            }
        }

        focused
    }

    /// Render the VStack at the specified position.
    pub fn render(&self, x: i32, y: i32, width: u32) {
        let positions = self.calculate_layout(x, y, width);

        for i in 0..self.item_count {
            if let Some(item) = &self.items[i] {
                let (ix, iy, iw, ih) = positions[i];
                match item {
                    LayoutItem::Label(label) => {
                        label.render_in_bounds(Rect::new(ix, iy, iw, ih));
                    }
                    LayoutItem::Button(button) => {
                        button.render_with_size(ix, iy, iw, ih);
                    }
                    LayoutItem::TextInput(input) => {
                        input.render(ix, iy, iw);
                    }
                    LayoutItem::Spacer(_) | LayoutItem::Flex(_) => {
                        // Spacers don't render anything
                    }
                }
            }
        }
    }
}

impl Default for VStack {
    fn default() -> Self {
        Self::new()
    }
}

// ===== HStack =====

/// A horizontal stack layout container.
///
/// Children are arranged horizontally from left to right.
#[derive(Clone)]
pub struct HStack {
    items: [Option<LayoutItem>; MAX_CHILDREN],
    item_count: usize,
    spacing: u32,
    valignment: VAlignment,
    bounds: Rect,
    padding: u32,
}

impl HStack {
    /// Create a new empty HStack.
    pub fn new() -> Self {
        Self {
            items: core::array::from_fn(|_| None),
            item_count: 0,
            spacing: DEFAULT_SPACING,
            valignment: VAlignment::Center,
            bounds: Rect::default(),
            padding: 0,
        }
    }

    /// Set spacing between items.
    pub fn set_spacing(&mut self, spacing: u32) {
        self.spacing = spacing;
    }

    /// Set vertical alignment for items.
    pub fn set_valignment(&mut self, valignment: VAlignment) {
        self.valignment = valignment;
    }

    /// Set padding around the container.
    pub fn set_padding(&mut self, padding: u32) {
        self.padding = padding;
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Add a generic item.
    pub fn add_item(&mut self, item: LayoutItem) -> bool {
        if self.item_count >= MAX_CHILDREN {
            return false;
        }
        self.items[self.item_count] = Some(item);
        self.item_count += 1;
        true
    }

    /// Add a label.
    pub fn add_label(&mut self, text: &[u8]) -> bool {
        self.add_item(LayoutItem::Label(Label::new(text)))
    }

    /// Add a label with color.
    pub fn add_label_colored(&mut self, text: &[u8], color: u32) -> bool {
        let mut label = Label::new(text);
        label.set_color(color);
        self.add_item(LayoutItem::Label(label))
    }

    /// Add a button and return its index.
    pub fn add_button(&mut self, text: &[u8]) -> Option<usize> {
        if self.item_count >= MAX_CHILDREN {
            return None;
        }
        let idx = self.item_count;
        self.items[idx] = Some(LayoutItem::Button(Button::new(text)));
        self.item_count += 1;
        Some(idx)
    }

    /// Add a fixed spacer.
    pub fn add_spacer(&mut self, size: u32) -> bool {
        self.add_item(LayoutItem::Spacer(size))
    }

    /// Add a flexible spacer.
    pub fn add_flex(&mut self, weight: u32) -> bool {
        self.add_item(LayoutItem::Flex(weight))
    }

    /// Get a mutable reference to a button by index.
    pub fn get_button_mut(&mut self, idx: usize) -> Option<&mut Button> {
        if idx >= self.item_count {
            return None;
        }
        match &mut self.items[idx] {
            Some(LayoutItem::Button(btn)) => Some(btn),
            _ => None,
        }
    }

    /// Calculate the preferred size for this HStack.
    pub fn preferred_size(&self) -> (u32, u32) {
        let mut total_width = self.padding * 2;
        let mut max_height = 0u32;

        for i in 0..self.item_count {
            if let Some(item) = &self.items[i] {
                let (w, h) = item.preferred_size();
                max_height = max_height.max(h);
                total_width += w;
                if i > 0 {
                    total_width += self.spacing;
                }
            }
        }

        (total_width, max_height + self.padding * 2)
    }

    /// Calculate layout positions for all items.
    fn calculate_layout(&self, x: i32, y: i32, width: u32) -> [(i32, i32, u32, u32); MAX_CHILDREN] {
        let mut positions: [(i32, i32, u32, u32); MAX_CHILDREN] = [(0, 0, 0, 0); MAX_CHILDREN];
        let mut current_x = x + self.padding as i32;
        let inner_y = y + self.padding as i32;
        let (_, total_height) = self.preferred_size();
        let inner_height = total_height.saturating_sub(self.padding * 2);

        for i in 0..self.item_count {
            if let Some(item) = &self.items[i] {
                let (pref_w, pref_h) = item.preferred_size();

                // Calculate item y based on vertical alignment
                let item_y = match self.valignment {
                    VAlignment::Top => inner_y,
                    VAlignment::Center => inner_y + (inner_height.saturating_sub(pref_h) / 2) as i32,
                    VAlignment::Bottom => inner_y + inner_height.saturating_sub(pref_h) as i32,
                };

                positions[i] = (current_x, item_y, pref_w, pref_h);
                current_x += pref_w as i32 + self.spacing as i32;
            }
        }

        positions
    }

    /// Handle mouse hover on buttons. Returns index of hovered button if any.
    pub fn on_hover(&mut self, mx: i32, my: i32, x: i32, y: i32, width: u32) -> Option<usize> {
        let positions = self.calculate_layout(x, y, width);
        let mut hovered = None;

        for i in 0..self.item_count {
            if let Some(LayoutItem::Button(btn)) = &mut self.items[i] {
                let (bx, by, bw, bh) = positions[i];
                let bounds = Rect::new(bx, by, bw, bh);
                btn.set_bounds(bounds);
                if btn.on_hover(mx, my) {
                    hovered = Some(i);
                }
            }
        }

        hovered
    }

    /// Handle mouse press. Returns index of pressed button if any.
    pub fn on_press(&mut self, mx: i32, my: i32, x: i32, y: i32, width: u32) -> Option<usize> {
        let positions = self.calculate_layout(x, y, width);

        for i in 0..self.item_count {
            if let Some(LayoutItem::Button(btn)) = &mut self.items[i] {
                let (bx, by, bw, bh) = positions[i];
                let bounds = Rect::new(bx, by, bw, bh);
                btn.set_bounds(bounds);
                if btn.on_press(mx, my) {
                    return Some(i);
                }
            }
        }

        None
    }

    /// Handle mouse release. Returns index of clicked button if any.
    pub fn on_release(&mut self, mx: i32, my: i32, x: i32, y: i32, width: u32) -> Option<usize> {
        let positions = self.calculate_layout(x, y, width);

        for i in 0..self.item_count {
            if let Some(LayoutItem::Button(btn)) = &mut self.items[i] {
                let (bx, by, bw, bh) = positions[i];
                let bounds = Rect::new(bx, by, bw, bh);
                btn.set_bounds(bounds);
                if btn.on_release(mx, my) {
                    return Some(i);
                }
            }
        }

        None
    }

    /// Render the HStack at the specified position.
    pub fn render(&self, x: i32, y: i32, width: u32) {
        let positions = self.calculate_layout(x, y, width);

        for i in 0..self.item_count {
            if let Some(item) = &self.items[i] {
                let (ix, iy, iw, ih) = positions[i];
                match item {
                    LayoutItem::Label(label) => {
                        label.render_in_bounds(Rect::new(ix, iy, iw, ih));
                    }
                    LayoutItem::Button(button) => {
                        button.render_with_size(ix, iy, iw, ih);
                    }
                    LayoutItem::TextInput(input) => {
                        input.render(ix, iy, iw);
                    }
                    LayoutItem::Spacer(_) | LayoutItem::Flex(_) => {
                        // Spacers don't render anything
                    }
                }
            }
        }
    }
}

impl Default for HStack {
    fn default() -> Self {
        Self::new()
    }
}

// ===== Grid Layout =====

/// Maximum grid dimensions
pub const MAX_GRID_ROWS: usize = 16;
pub const MAX_GRID_COLS: usize = 16;

/// A grid layout container.
///
/// Children are arranged in a fixed grid of rows and columns.
#[derive(Clone)]
pub struct Grid {
    cells: [[Option<LayoutItem>; MAX_GRID_COLS]; MAX_GRID_ROWS],
    rows: usize,
    cols: usize,
    row_heights: [u32; MAX_GRID_ROWS],
    col_widths: [u32; MAX_GRID_COLS],
    row_spacing: u32,
    col_spacing: u32,
    padding: u32,
}

impl Grid {
    /// Create a new grid with the specified dimensions.
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            cells: core::array::from_fn(|_| core::array::from_fn(|_| None)),
            rows: rows.min(MAX_GRID_ROWS),
            cols: cols.min(MAX_GRID_COLS),
            row_heights: [0; MAX_GRID_ROWS],
            col_widths: [0; MAX_GRID_COLS],
            row_spacing: DEFAULT_SPACING,
            col_spacing: DEFAULT_SPACING,
            padding: 0,
        }
    }

    /// Set row spacing.
    pub fn set_row_spacing(&mut self, spacing: u32) {
        self.row_spacing = spacing;
    }

    /// Set column spacing.
    pub fn set_col_spacing(&mut self, spacing: u32) {
        self.col_spacing = spacing;
    }

    /// Set padding.
    pub fn set_padding(&mut self, padding: u32) {
        self.padding = padding;
    }

    /// Set a fixed row height.
    pub fn set_row_height(&mut self, row: usize, height: u32) {
        if row < self.rows {
            self.row_heights[row] = height;
        }
    }

    /// Set a fixed column width.
    pub fn set_col_width(&mut self, col: usize, width: u32) {
        if col < self.cols {
            self.col_widths[col] = width;
        }
    }

    /// Set an item at the specified cell.
    pub fn set_item(&mut self, row: usize, col: usize, item: LayoutItem) -> bool {
        if row >= self.rows || col >= self.cols {
            return false;
        }
        self.cells[row][col] = Some(item);
        true
    }

    /// Set a label at the specified cell.
    pub fn set_label(&mut self, row: usize, col: usize, text: &[u8]) -> bool {
        self.set_item(row, col, LayoutItem::Label(Label::new(text)))
    }

    /// Set a button at the specified cell.
    pub fn set_button(&mut self, row: usize, col: usize, text: &[u8]) -> bool {
        self.set_item(row, col, LayoutItem::Button(Button::new(text)))
    }

    /// Get a mutable reference to an item.
    pub fn get_item_mut(&mut self, row: usize, col: usize) -> Option<&mut LayoutItem> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.cells[row][col].as_mut()
    }

    /// Calculate the preferred size for this grid.
    pub fn preferred_size(&self) -> (u32, u32) {
        // Calculate row heights and column widths based on content
        let mut row_heights = [0u32; MAX_GRID_ROWS];
        let mut col_widths = [0u32; MAX_GRID_COLS];

        for row in 0..self.rows {
            for col in 0..self.cols {
                if let Some(item) = &self.cells[row][col] {
                    let (w, h) = item.preferred_size();
                    row_heights[row] = row_heights[row].max(h);
                    col_widths[col] = col_widths[col].max(w);
                }
            }
            // Use fixed height if set
            if self.row_heights[row] > 0 {
                row_heights[row] = self.row_heights[row];
            }
        }

        for col in 0..self.cols {
            if self.col_widths[col] > 0 {
                col_widths[col] = self.col_widths[col];
            }
        }

        let total_width: u32 = col_widths[..self.cols].iter().sum::<u32>()
            + self.col_spacing * (self.cols.saturating_sub(1)) as u32
            + self.padding * 2;

        let total_height: u32 = row_heights[..self.rows].iter().sum::<u32>()
            + self.row_spacing * (self.rows.saturating_sub(1)) as u32
            + self.padding * 2;

        (total_width, total_height)
    }

    /// Render the grid at the specified position.
    pub fn render(&self, x: i32, y: i32) {
        // Calculate actual row heights and column widths
        let mut row_heights = [0u32; MAX_GRID_ROWS];
        let mut col_widths = [0u32; MAX_GRID_COLS];

        for row in 0..self.rows {
            for col in 0..self.cols {
                if let Some(item) = &self.cells[row][col] {
                    let (w, h) = item.preferred_size();
                    row_heights[row] = row_heights[row].max(h);
                    col_widths[col] = col_widths[col].max(w);
                }
            }
            if self.row_heights[row] > 0 {
                row_heights[row] = self.row_heights[row];
            }
        }

        for col in 0..self.cols {
            if self.col_widths[col] > 0 {
                col_widths[col] = self.col_widths[col];
            }
        }

        // Render cells
        let mut current_y = y + self.padding as i32;

        for row in 0..self.rows {
            let mut current_x = x + self.padding as i32;
            let cell_height = row_heights[row];

            for col in 0..self.cols {
                let cell_width = col_widths[col];

                if let Some(item) = &self.cells[row][col] {
                    match item {
                        LayoutItem::Label(label) => {
                            label.render_in_bounds(Rect::new(
                                current_x,
                                current_y,
                                cell_width,
                                cell_height,
                            ));
                        }
                        LayoutItem::Button(button) => {
                            button.render_with_size(current_x, current_y, cell_width, cell_height);
                        }
                        LayoutItem::TextInput(input) => {
                            input.render(current_x, current_y, cell_width);
                        }
                        _ => {}
                    }
                }

                current_x += cell_width as i32 + self.col_spacing as i32;
            }

            current_y += cell_height as i32 + self.row_spacing as i32;
        }
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new(2, 2)
    }
}
