# RayOS Application Development Guide

**Version**: 0.1 (Preview)
**Status**: Framework in Development
**Last Updated**: January 2026

---

## Introduction

RayOS provides a native application framework for building desktop applications that run directly on the RayOS kernel. This guide covers the architecture, APIs, and development workflow for creating RayOS native applications (RayApps).

> **Note**: The RayApp framework is under active development. APIs may change before the 1.0 release.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Getting Started](#getting-started)
3. [Window Management](#window-management)
4. [Rendering](#rendering)
5. [Input Handling](#input-handling)
6. [Application Lifecycle](#application-lifecycle)
7. [Future: VS Code Extension](#future-vs-code-extension)

---

## Architecture Overview

### The UI Framework Stack

```
┌─────────────────────────────────────────────────────────────┐
│                    Your RayApp                              │
├─────────────────────────────────────────────────────────────┤
│  Content Module (ui/content.rs)                             │
│  - Window-specific rendering                                │
│  - Event handling                                           │
├─────────────────────────────────────────────────────────────┤
│  Window Manager (ui/window_manager.rs)                      │
│  - Window creation, destruction                             │
│  - Position, size, focus management                         │
│  - Z-order tracking                                         │
├─────────────────────────────────────────────────────────────┤
│  Input Handler (ui/input.rs)                                │
│  - Mouse events (move, click, drag)                         │
│  - Keyboard events                                          │
│  - Text input handling                                      │
├─────────────────────────────────────────────────────────────┤
│  Compositor (ui/compositor.rs)                              │
│  - Window compositing                                       │
│  - Damage tracking                                          │
│  - Efficient redraws                                        │
├─────────────────────────────────────────────────────────────┤
│  Renderer (ui/renderer.rs)                                  │
│  - Framebuffer access                                       │
│  - Drawing primitives                                       │
│  - Font rendering                                           │
└─────────────────────────────────────────────────────────────┘
```

### Module Responsibilities

| Module | Purpose |
|--------|---------|
| `renderer` | Low-level pixel operations, text drawing |
| `window_manager` | Window lifecycle and state |
| `compositor` | Composites all windows to screen |
| `input` | Routes mouse/keyboard to windows |
| `content` | Application-specific rendering |
| `shell` | Desktop shell integration |

---

## Getting Started

### Current Development Model

Currently, RayApps are developed by adding content handlers to the kernel. This will evolve to support standalone app binaries.

```rust
// In ui/content.rs - Add your window handler
pub fn render_window_content(win: &Window, cx: i32, cy: i32, cw: u32, ch: u32) {
    let title = win.get_title();

    if title == b"My App" {
        render_my_app(cx, cy, cw, ch);
    } else if title == b"System Status" {
        render_system_status(cx, cy, cw, ch);
    }
    // ... other windows
}

fn render_my_app(cx: i32, cy: i32, cw: u32, ch: u32) {
    // Your app rendering code here
    draw_text(cx + 10, cy + 10, b"Hello, RayOS!", 0xFFFFFF);
}
```

### Creating a Window

```rust
use crate::ui::window_manager::{self, WindowType};

// Create a normal window
let window_id = window_manager::create_window(
    b"My Application",  // Title (max 64 bytes)
    100,                // X position
    100,                // Y position
    400,                // Width
    300,                // Height
    WindowType::Normal, // Window type
);

// Focus the window
if let Some(id) = window_id {
    window_manager::set_focus(id);
}
```

### Window Types

| Type | Description | Decorations |
|------|-------------|-------------|
| `Desktop` | Background desktop surface | No |
| `Normal` | Standard application window | Yes (title bar, borders) |
| `Dialog` | Modal dialog window | Yes |
| `Panel` | Status bars, docks | No |
| `Popup` | Menus, tooltips | No |
| `VMSurface` | Guest VM display surface | Optional |

---

## Rendering

### Drawing Primitives

The renderer provides basic drawing operations:

```rust
use crate::ui::renderer::{draw_text, fill_rect, draw_pixel};

// Draw text
draw_text(x, y, b"Hello World", 0xFFFFFF);

// Draw rectangle
fill_rect(x, y, width, height, 0x2A2A4E);

// Draw pixel
draw_pixel(x, y, 0xFF0000);
```

### Color Format

Colors are 32-bit ARGB (0xAARRGGBB):

```rust
const WHITE: u32 = 0xFFFFFF;
const RED: u32 = 0xFF0000;
const GREEN: u32 = 0x00FF00;
const BLUE: u32 = 0x0000FF;
const TRANSPARENT: u32 = 0x00000000;
```

### Standard Colors

```rust
// From ui/renderer.rs
pub const COLOR_BACKGROUND: u32 = 0x1A1A2E;  // Dark blue-gray
pub const COLOR_WINDOW_BG: u32 = 0x2A2A4E;   // Window background
pub const COLOR_TEXT: u32 = 0xE0E0E0;        // Light gray text
pub const COLOR_ACCENT: u32 = 0x88CCFF;      // Blue accent
```

### Font System

Currently, RayOS uses a built-in 8x16 bitmap font:

```rust
// Text is rendered at 8 pixels per character width
let text = b"Hello";
let text_width = text.len() * 8;  // 40 pixels
let text_height = 16;              // 16 pixels
```

---

## Input Handling

### Mouse Events

Windows receive mouse events through the input system:

```rust
use crate::ui::input;

// Check current mouse position
let (mx, my) = input::mouse_position();

// Check button state
let left_down = input::is_left_button_down();
```

### Text Input

For text input fields:

```rust
use crate::ui::input;

// Check if text input is active
if input::is_text_input_active() {
    // Get current input buffer
    let text = input::get_text_input();

    // Get cursor position
    let cursor = input::get_text_cursor();
}

// Clear input after processing
input::clear_text_input();
```

### Keyboard Events

Keyboard events are routed based on focus:

1. If text input is active → goes to text buffer
2. If window has focus → routed to window handler
3. Otherwise → handled by shell

---

## Application Lifecycle

### Initialization

Apps are initialized during shell startup:

```rust
// In ui/shell.rs
pub fn ui_shell_init(...) {
    // ... framework init ...

    // Create application windows
    window_manager::create_window(
        b"My App",
        100, 100, 400, 300,
        WindowType::Normal,
    );
}
```

### Tick Loop

Apps receive updates through the compositor tick:

```rust
// Called every frame when dirty
pub fn render_window_content(win: &Window, cx: i32, cy: i32, cw: u32, ch: u32) {
    // Render current state
}
```

### Cleanup

Windows are destroyed with:

```rust
window_manager::destroy_window(window_id);
```

---

## Future: VS Code Extension

We are planning a VS Code extension for RayOS development that will provide:

### Planned Features

1. **RayApp Project Templates**
   - Create new RayApp projects with scaffolding
   - Pre-configured build settings

2. **Live Preview**
   - Preview UI layouts in VS Code
   - Hot reload during development

3. **Build Integration**
   - One-click build and deploy
   - QEMU integration for testing

4. **Debugging**
   - Serial console output
   - Breakpoint debugging via GDB

5. **UI Designer**
   - Visual window layout editor
   - Widget palette

### Extension Architecture (Planned)

```
VS Code
├── RayOS Extension
│   ├── Language Support
│   │   └── RayApp manifest syntax
│   ├── Build Tasks
│   │   └── Cargo integration
│   ├── Debug Adapter
│   │   └── GDB/QEMU bridge
│   └── Preview Panel
│       └── Simulated RayOS renderer
```

### Timeline

| Phase | Feature | Target |
|-------|---------|--------|
| 1 | Project templates | Q2 2026 |
| 2 | Build integration | Q2 2026 |
| 3 | Serial debugging | Q3 2026 |
| 4 | Live preview | Q4 2026 |
| 5 | Visual designer | 2027 |

---

## API Reference (Preview)

### Window Manager

```rust
// Create window
fn create_window(title: &[u8], x: i32, y: i32, w: u32, h: u32,
                 wtype: WindowType) -> Option<WindowId>;

// Destroy window
fn destroy_window(id: WindowId);

// Focus management
fn set_focus(id: WindowId);
fn get_focused() -> WindowId;

// Position/size
fn move_window(id: WindowId, x: i32, y: i32);
fn resize_window(id: WindowId, w: u32, h: u32);

// Visibility
fn show_window(id: WindowId);
fn hide_window(id: WindowId);
```

### Renderer

```rust
// Text
fn draw_text(x: i32, y: i32, text: &[u8], color: u32);

// Shapes
fn fill_rect(x: i32, y: i32, w: u32, h: u32, color: u32);
fn draw_pixel(x: i32, y: i32, color: u32);

// Dimensions
fn get_dimensions() -> (usize, usize);
```

### Input

```rust
// Mouse
fn mouse_position() -> (i32, i32);
fn is_left_button_down() -> bool;

// Text input
fn is_text_input_active() -> bool;
fn get_text_input() -> &'static [u8];
fn clear_text_input();
```

---

## Examples

### Simple Status Display

```rust
fn render_status_window(cx: i32, cy: i32, _cw: u32, _ch: u32) {
    let mut y = cy + 10;
    let x = cx + 10;

    draw_text(x, y, b"System Status", 0x88CCFF);
    y += 20;

    draw_text(x, y, b"CPU: OK", 0x88FF88);
    y += 16;

    draw_text(x, y, b"Memory: OK", 0x88FF88);
    y += 16;

    draw_text(x, y, b"Network: OK", 0x88FF88);
}
```

### Interactive Button (Conceptual)

```rust
fn render_with_button(cx: i32, cy: i32, cw: u32, ch: u32) {
    let btn_x = cx + 10;
    let btn_y = cy + ch as i32 - 40;
    let btn_w = 80;
    let btn_h = 24;

    let (mx, my) = input::mouse_position();
    let hover = mx >= btn_x && mx < btn_x + btn_w
             && my >= btn_y && my < btn_y + btn_h;

    let bg = if hover { 0x4A4A7E } else { 0x3A3A5E };
    fill_rect(btn_x, btn_y, btn_w as u32, btn_h as u32, bg);
    draw_text(btn_x + 16, btn_y + 4, b"Click", 0xFFFFFF);
}
```

---

## Next Steps

1. **Read the UI Framework documentation**: [RAYOS_UI_FRAMEWORK.md](RAYOS_UI_FRAMEWORK.md)
2. **Explore the source code**: `crates/kernel-bare/src/ui/`
3. **Run the examples**: `./scripts/run-ui-shell.sh`
4. **Join development**: See [CONTRIBUTING.md](CONTRIBUTING.md)

---

*This documentation is a preview and will be updated as the framework matures.*
