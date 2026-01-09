//! RayOS Native UI Framework
//!
//! This module provides a native graphical user interface for RayOS,
//! including window management, compositing, and rendering.
//!
//! # Modules
//!
//! - `renderer` - Low-level drawing primitives
//! - `window_manager` - Window lifecycle and state management
//! - `compositor` - Window compositing to framebuffer
//! - `shell` - Desktop shell integration
//! - `input` - Mouse and keyboard input handling

pub mod renderer;
pub mod window_manager;
pub mod compositor;
pub mod shell;
pub mod input;
pub mod content;

// Re-export key types
pub use renderer::{COLOR_ACCENT, COLOR_BACKGROUND, COLOR_TEXT, COLOR_WINDOW_BG, CursorType};
pub use window_manager::{Window, WindowId, WindowManager, WindowType};
pub use compositor::Compositor;
pub use shell::{ui_shell_init, ui_shell_tick};
pub use input::{
    handle_mouse_move, handle_mouse_button_down, handle_mouse_button_up, mouse_position,
    handle_key_for_mouse, handle_scancode_for_mouse,
};
