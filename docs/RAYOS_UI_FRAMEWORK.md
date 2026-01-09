# RayOS Native UI Framework

**Status**: Implemented (Core Features)
**Last Updated**: January 2026

---

## Overview

RayOS includes a native graphical UI framework that provides a modern desktop experience. This framework enables:

- Native windows with decorations (title bar, borders, controls)
- Mouse and keyboard interaction
- Window management (drag, resize, maximize, close)
- Text input handling
- VM surfaces as managed windows

The UI framework is designed as the **foundation for RayOS native applications**.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         RayOS Desktop Shell                             │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                    RayOS Compositor                               │  │
│  │  • Window management (create, move, resize, close)                │  │
│  │  • Z-order and focus tracking                                     │  │
│  │  • Input event routing (keyboard, mouse)                          │  │
│  │  • Damage tracking and efficient redraws                          │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                   │                                      │
│         ┌─────────────────────────┼─────────────────────────┐           │
│         ▼                         ▼                         ▼           │
│  ┌─────────────┐          ┌─────────────┐          ┌─────────────┐     │
│  │   Desktop   │          │  AI Chat    │          │   Linux     │     │
│  │  Background │          │   Window    │          │ VM Window   │     │
│  │  (native)   │          │  (native)   │          │(virtio-gpu) │     │
│  └─────────────┘          └─────────────┘          └─────────────┘     │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Core Modules

### Source Location

All UI code is in `crates/kernel-bare/src/ui/`:

| File | Purpose |
|------|---------|
| `mod.rs` | Module exports and re-exports |
| `renderer.rs` | Low-level drawing primitives |
| `window_manager.rs` | Window lifecycle and state |
| `compositor.rs` | Composites windows to framebuffer |
| `input.rs` | Mouse and keyboard handling |
| `shell.rs` | Desktop shell integration |
| `content.rs` | Window-specific content rendering |

---

## Window Manager

### Window Structure

```rust
pub struct Window {
    pub id: WindowId,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub window_type: WindowType,
    pub decorations: bool,
    pub visible: bool,
    pub minimized: bool,
    title: [u8; 64],
    title_len: usize,
}
```

### Window Types

```rust
pub enum WindowType {
    Desktop,           // Background desktop surface
    Normal,            // Standard window with decorations
    Dialog,            // Modal dialog
    Panel,             // Status bars, docks
    Popup,             // Menus, tooltips
    VMSurface,         // Guest VM display surface
}
```

### Creating Windows

```rust
use crate::ui::window_manager::{self, WindowType};

let window_id = window_manager::create_window(
    b"Window Title",    // Title (max 64 bytes)
    100,                // X position
    100,                // Y position
    400,                // Width
    300,                // Height
    WindowType::Normal, // Type
);
```

### Window Operations

```rust
// Focus and Z-order
window_manager::set_focus(id);
window_manager::raise_window(id);

// Position and size
let wm = window_manager::get_mut();
wm.move_window(id, new_x, new_y);
wm.resize_window(id, new_width, new_height);

// Visibility
window_manager::destroy_window(id);
```

---

## Renderer

### Drawing Primitives

```rust
use crate::ui::renderer::{draw_text, fill_rect, draw_pixel};

// Text (8x16 bitmap font)
draw_text(x, y, b"Hello World", 0xFFFFFF);

// Filled rectangle
fill_rect(x, y, width, height, 0x2A2A4E);

// Single pixel
draw_pixel(x, y, 0xFF0000);
```

### Color Constants

```rust
pub const COLOR_BACKGROUND: u32 = 0x1A1A2E;  // Desktop background
pub const COLOR_WINDOW_BG: u32 = 0x2A2A4E;   // Window content area
pub const COLOR_TEXT: u32 = 0xE0E0E0;        // Primary text
pub const COLOR_ACCENT: u32 = 0x88CCFF;      // Accent color
```

### Screen Information

```rust
let (width, height) = renderer::get_dimensions();
```

---

## Input Handler

### Mouse Events

```rust
use crate::ui::input;

// Called by PS/2 driver
input::handle_mouse_move(x, y);
input::handle_mouse_button_down(x, y, button, is_double_click);
input::handle_mouse_button_up(x, y, button);

// Query state
let (mx, my) = input::mouse_position();
let pressed = input::is_left_button_down();
```

### Mouse Features

- Window dragging via title bar
- Window resizing via edges and corners
- Double-click title bar to maximize
- Focus on click
- Cursor type changes based on context

### Text Input

```rust
// Check if text input is active
if input::is_text_input_active() {
    let text = input::get_text_input();
    let cursor = input::get_text_cursor();
}

// Handle keyboard for text input
input::handle_key_for_text_input(ascii_char);

// Clear after submission
input::clear_text_input();
```

---

## Compositor

### Compositing Loop

```rust
// Mark screen as needing redraw
compositor::mark_dirty();

// Composite all windows to framebuffer
compositor::composite();
```

### Rendering Order

1. Desktop background
2. Windows in Z-order (back to front)
3. Window decorations (title bar, borders)
4. Window content via `content::render_window_content()`
5. Mouse cursor (on top)

---

## Content Module

Window-specific content is rendered based on window title:

```rust
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
```

---

## Shell Integration

### Initialization

```rust
use crate::ui::shell;

// Initialize UI framework with framebuffer
shell::ui_shell_init(fb_addr, fb_width, fb_height, fb_stride);

// Check if initialized
if shell::is_initialized() { ... }
```

### Tick Loop

```rust
// Called every frame
shell::ui_shell_tick(tick_count);
```

---

## Visual Design

### Color Palette

| Color | Hex | Usage |
|-------|-----|-------|
| Background | `0x1A1A2E` | Desktop |
| Window BG | `0x2A2A4E` | Content area |
| Title Bar | `0x3A3A5E` | Window chrome |
| Title Focused | `0x4A4A7E` | Active window |
| Text | `0xE0E0E0` | Primary text |
| Text Dim | `0x888888` | Secondary text |
| Accent | `0x88CCFF` | Highlights |
| Success | `0x88FF88` | OK status |
| Warning | `0xFFFF88` | Warning status |
| Error | `0xFF8888` | Error status |

### Window Decorations

```
┌─────────────────────────────────────┬───┬───┬───┐
│ Window Title                        │ — │ □ │ × │
├─────────────────────────────────────┴───┴───┴───┤
│                                                  │
│              Window Content Area                 │
│                                                  │
│                                                  │
└──────────────────────────────────────────────────┘
```

- 24px title bar height
- 2px border width
- Close button (×), Maximize (□), Minimize (—)
- Resize handles on all edges and corners

---

## Implementation Status

### Completed ✅

- [x] Window manager with create/destroy
- [x] Compositor with Z-order rendering
- [x] Window decorations (title bar, borders)
- [x] Mouse cursor with PS/2 driver
- [x] Window dragging
- [x] Window resizing (all edges/corners)
- [x] Double-click maximize
- [x] Window focus tracking
- [x] Text input with cursor
- [x] Keyboard routing to text input
- [x] Content module for window-specific rendering

### In Progress 🟡

- [ ] Widget library (Button, Checkbox, etc.)
- [ ] Layout system (VStack, HStack)
- [ ] Scroll views
- [ ] VM surface integration

### Planned 📋

- [ ] Animations (window open/close)
- [ ] Multiple monitors
- [ ] Themes/styling
- [ ] Accessibility features

---

## Testing

### Headless Test

```bash
./scripts/test-ui-shell-headless.sh
```

Expected markers:
- `RAYOS_UI_RENDERER_INIT:ok`
- `RAYOS_UI_WINDOW_MANAGER_INIT:ok`
- `RAYOS_UI_COMPOSITOR_INIT:ok`
- `RAYOS_UI_SHELL_INIT:ok`
- `RAYOS_UI_WINDOW_CREATED:N`
- `RAYOS_UI_COMPOSITE:ok`

### Interactive Test

```bash
./scripts/run-ui-shell.sh
```

---

## See Also

- [App Development Guide](development/APP_DEVELOPMENT.md) - Building RayOS applications
- [Framework Roadmap](development/FRAMEWORK_ROADMAP.md) - Future development plans
- [Contributing](development/CONTRIBUTING.md) - How to contribute
