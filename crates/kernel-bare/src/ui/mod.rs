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
//! - `widgets` - Reusable UI widgets (Label, Button, TextInput)
//! - `layout` - Layout containers (VStack, HStack, Grid)
//! - `app_sdk` - App development SDK

pub mod renderer;
pub mod window_manager;
pub mod compositor;
pub mod shell;
pub mod input;
pub mod content;
pub mod widgets;
pub mod layout;
pub mod app_sdk;
pub mod example_apps;

// Re-export key types
pub use renderer::{COLOR_ACCENT, COLOR_BACKGROUND, COLOR_TEXT, COLOR_WINDOW_BG, CursorType};
pub use window_manager::{Window, WindowId, WindowManager, WindowType};
pub use compositor::Compositor;
pub use shell::{ui_shell_init, ui_shell_tick};
pub use input::{
    handle_mouse_move, handle_mouse_button_down, handle_mouse_button_up, mouse_position,
    handle_key_for_mouse, handle_scancode_for_mouse, is_text_input_active,
};

// Re-export Linux Desktop window management functions
pub use shell::{
    show_linux_desktop, hide_linux_desktop, close_linux_desktop,
    is_linux_desktop_visible, is_linux_desktop_focused, linux_desktop_window_id,
};

// Re-export Windows Desktop window management functions
pub use shell::{
    show_windows_desktop, hide_windows_desktop, close_windows_desktop,
    is_windows_desktop_visible, is_windows_desktop_focused, windows_desktop_window_id,
};

// Re-export App SDK types
pub use app_sdk::{
    App, AppCapabilities, AppContext, AppDescriptor, AppEvent, AppInstance,
    AppManager, AppRegistryEntry, Key, MouseButton, app_manager,
};
