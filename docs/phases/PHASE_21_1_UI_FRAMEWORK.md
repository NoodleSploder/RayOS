# Phase 21.1: RayOS Native UI Framework - Implementation Plan

**Phase**: 21.1 (UI Framework Foundation)
**Status**: ✅ Phase 1 COMPLETE - Verified with Screenshot
**Created**: January 8, 2026
**Target**: Native windowed desktop environment

---

## Overview

This phase implements the **RayOS Native UI Framework** - a complete graphical shell that replaces raw framebuffer blitting with proper window management, compositing, and native widgets.

**Design Document**: [RAYOS_UI_FRAMEWORK.md](docs/RAYOS_UI_FRAMEWORK.md)

---

## Phase 1: Core Framework ✅ COMPLETE

### Task 1.1: Create UI Module Structure
**Status**: ✅ Complete
**Files**: `crates/kernel-bare/src/ui/mod.rs`

Create the base module with exports for all UI components.

```rust
// Module structure
pub mod renderer;
pub mod window_manager;
pub mod compositor;
// (later phases add more)
```

---

### Task 1.2: Implement Renderer (`renderer.rs`)
**Status**: ✅ Complete
**Files**: `crates/kernel-bare/src/ui/renderer.rs`
**Lines**: ~300

Low-level drawing primitives that render directly to framebuffer.

**Functions**:
- `fill_rect(x, y, w, h, color)` - Solid rectangle
- `draw_rect(x, y, w, h, color)` - Rectangle outline
- `draw_char(x, y, ch, color)` - Single character
- `draw_text(x, y, text, color)` - Text string
- `blit_rgba(x, y, w, h, src, stride)` - Blit surface

**Constants**:
```rust
const COLOR_BACKGROUND: u32 = 0x1E1E2E;   // Desktop background
const COLOR_WINDOW_BG: u32 = 0x2D2D3D;    // Window content area
const COLOR_TITLE_BAR: u32 = 0x3D3D5C;    // Title bar unfocused
const COLOR_TITLE_FOCUSED: u32 = 0x5D5D8C; // Title bar focused
const COLOR_TEXT: u32 = 0xE0E0E0;          // Primary text
const COLOR_ACCENT: u32 = 0x00FF88;        // RayOS green
const COLOR_BORDER: u32 = 0x4D4D6D;        // Window border
```

---

### Task 1.3: Implement Window Manager (`window_manager.rs`)
**Status**: ✅ Complete
**Files**: `crates/kernel-bare/src/ui/window_manager.rs`
**Lines**: ~400

Manages window lifecycle, z-order, and focus.

**Structures**:
```rust
pub type WindowId = u32;

pub struct Window {
    pub id: WindowId,
    pub title: [u8; 64],
    pub title_len: usize,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub visible: bool,
    pub focused: bool,
    pub decorations: bool,
    pub window_type: WindowType,
}

pub enum WindowType {
    Desktop,        // Background
    Normal,         // Standard window
    VmSurface,      // VM framebuffer
    Panel,          // Status bar
}

pub struct WindowManager {
    windows: [Option<Window>; 16],
    window_count: usize,
    z_order: [WindowId; 16],
    focus: Option<WindowId>,
    next_id: WindowId,
}
```

**Functions**:
- `init()` - Initialize window manager
- `create_window(title, x, y, w, h, type)` - Create window
- `destroy_window(id)` - Destroy window
- `get_window(id)` - Get window by ID
- `get_window_mut(id)` - Get mutable window
- `set_focus(id)` - Set focused window
- `get_focused()` - Get focused window
- `move_window(id, x, y)` - Move window
- `resize_window(id, w, h)` - Resize window
- `raise_window(id)` - Bring to front
- `iter_z_order()` - Iterate windows back-to-front

---

### Task 1.4: Implement Compositor (`compositor.rs`)
**Status**: ✅ Complete
**Files**: `crates/kernel-bare/src/ui/compositor.rs`
**Lines**: ~400

Composites all windows to the framebuffer.

**Structures**:
```rust
pub struct Compositor {
    framebuffer_addr: u64,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_stride: u32,
    dirty: bool,
}
```

**Functions**:
- `init(fb_addr, width, height, stride)` - Initialize with framebuffer
- `composite()` - Render all windows to framebuffer
- `render_window(window)` - Render single window
- `render_decorations(window)` - Draw title bar, border
- `render_desktop()` - Draw background
- `mark_dirty()` - Mark for redraw
- `is_dirty()` - Check if redraw needed

**Window Rendering**:
```
Title bar: 24 pixels high, includes title text
Border: 2 pixels on sides and bottom
Content area: window.width x window.height
Close button: 16x16 in top-right of title bar
```

---

### Task 1.5: Implement Shell (`shell.rs`)
**Status**: ✅ Complete
**Files**: `crates/kernel-bare/src/ui/shell.rs`
**Lines**: ~200

Main integration point for the UI shell.

**Functions**:
- `init()` - Initialize the entire UI shell
- `tick()` - Per-frame update
- `create_test_window()` - Create initial test window
- `get_window_manager()` - Access window manager
- `get_compositor()` - Access compositor

**Initial Shell Setup**:
```rust
pub fn init() {
    // Initialize components
    renderer::init();
    window_manager::init();
    compositor::init(framebuffer);

    // Create desktop
    window_manager::create_window("Desktop", 0, 0, 1024, 768, WindowType::Desktop);

    // Create test window
    window_manager::create_window("RayOS Terminal", 100, 100, 400, 300, WindowType::Normal);
}
```

---

### Task 1.6: Integrate into Kernel Main
**Status**: ✅ Complete
**Files**: `crates/kernel-bare/src/main.rs`
**Lines**: ~50 modified

Add UI shell to kernel main loop.

**Integration Points**:
```rust
// At init
ui::shell::init();

// In main loop
ui::shell::tick();
if ui::compositor::is_dirty() {
    ui::compositor::composite();
}
```

**Feature Gate**: `ui_shell`

---

## Phase 1 Completion Criteria

- [x] Documentation created (RAYOS_UI_FRAMEWORK.md)
- [x] `ui/mod.rs` - Module structure
- [x] `ui/renderer.rs` - Drawing primitives
- [x] `ui/window_manager.rs` - Window management
- [x] `ui/compositor.rs` - Window compositing
- [x] `ui/shell.rs` - Shell integration
- [x] `main.rs` - Kernel integration
- [x] Test script for UI shell
- [ ] Desktop background visible (needs graphical test)
- [ ] Test window with decorations visible (needs graphical test)
- [ ] No visual corruption (needs graphical test)

---

## Test Script

**File**: `scripts/test-ui-shell-headless.sh`

```bash
#!/usr/bin/env bash
# Test RayOS UI Shell in headless mode
# Expected markers:
#   RAYOS_UI_SHELL_INIT:ok
#   RAYOS_UI_WINDOW_CREATED:<id>
#   RAYOS_UI_COMPOSITE:ok
```

---

## Deterministic Markers

| Marker | Meaning |
|--------|---------|
| `RAYOS_UI_SHELL_INIT:ok` | Shell initialized successfully |
| `RAYOS_UI_WINDOW_CREATED:<id>` | Window created with given ID |
| `RAYOS_UI_WINDOW_FOCUSED:<id>` | Window received focus |
| `RAYOS_UI_COMPOSITE:ok` | Frame composited to framebuffer |
| `RAYOS_UI_RENDERER_INIT:ok` | Renderer initialized |

---

## Future Phases (After Phase 1)

### Phase 2: Input & Interaction
- Mouse cursor rendering
- Click-to-focus
- Window dragging
- Close button

### Phase 3: VM Window Integration
- Linux VM in window
- Input routing to VM
- Surface updates

### Phase 4: AI Chat Window
- Widget toolkit
- TextInput, ScrollArea, Button
- AI Chat UI

### Phase 5: Status Bar
- Panel window type
- Clock, VM status
- System menu

### Phase 6: Polish
- Damage tracking
- Animations
- Keyboard shortcuts

---

## Estimated Effort

| Task | Lines | Time |
|------|-------|------|
| 1.1 mod.rs | 30 | 5 min |
| 1.2 renderer.rs | 300 | 30 min |
| 1.3 window_manager.rs | 400 | 40 min |
| 1.4 compositor.rs | 400 | 40 min |
| 1.5 shell.rs | 200 | 20 min |
| 1.6 main.rs integration | 50 | 15 min |
| Test script | 50 | 10 min |
| **Total** | **~1430** | **~2.5 hours** |

---

## Notes

- All code must be `no_std` compatible
- Use static allocation (fixed-size arrays, no heap)
- Colors use ARGB format (0xAARRGGBB)
- Coordinate system: (0,0) at top-left
- Framebuffer from BOOTBOOT is already available in `main.rs`

---

## Test Results (January 8, 2026)

### Headless Test: PASSED ✅
```
./scripts/test-ui-shell-headless.sh

=== Checking for UI markers ===
  [OK] RAYOS_UI_RENDERER_INIT:ok
  [OK] RAYOS_UI_WINDOW_MANAGER_INIT:ok
  [OK] RAYOS_UI_COMPOSITOR_INIT:ok
  [OK] RAYOS_UI_SHELL_INIT:ok
  [OK] RAYOS_UI_WINDOW_CREATED:
  [OK] RAYOS_UI_COMPOSITE:ok

PASS: Kernel started
PASS: All UI shell markers found
Screenshot saved to: build/ui-shell-screenshot.ppm
```

### Screenshot Analysis
- **Resolution**: 1280×800 pixels
- **Background Coverage**: 76.5% desktop background (#1E1E2E)
- **Window Content**: 15.7% window areas (#2D2D3D)
- **Title Bars**: Unfocused (#3D3D5C) and Focused (#5D5D8C) visible
- **Accent Green**: RayOS accent lines visible (#00FF88)
- **Window Elements**: Close buttons, borders, text rendered

### Windows Created
1. Desktop (fullscreen)
2. "RayOS Terminal" (500×350 at 100,80) - focused
3. "System Info" (300×200 at 150,150)
4. Status Bar Panel (fullwidth at bottom)

---

*Phase 21.1 - RayOS UI Framework*
