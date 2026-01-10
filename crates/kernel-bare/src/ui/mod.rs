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
pub mod font;
pub mod animation;
pub mod surface_manager;
pub mod window_manager_ext;
pub mod input_router;
pub mod app_runtime;
pub mod shell_integration;
pub mod clipboard;
pub mod drag_drop;
pub mod file_picker;
pub mod data_transfer;
pub mod vm_data_bridge;

// Re-export key types

// Re-export Linux Desktop window management functions

// Re-export Windows Desktop window management functions

// Re-export App SDK types

// Re-export Font types

// Re-export Animation types
