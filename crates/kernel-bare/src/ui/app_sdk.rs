//! RayOS App SDK
//!
//! Provides a stable API for building native RayOS applications.
//!
//! # Overview
//!
//! The SDK provides:
//! - **AppDescriptor** - App metadata (name, version, icon)
//! - **AppContext** - Runtime context with access to windows, rendering, input
//! - **App trait** - Lifecycle callbacks (init, frame, event, destroy)
//!
//! # Example
//!
//! ```ignore
//! struct MyApp {
//!     counter: u32,
//! }
//!
//! impl App for MyApp {
//!     fn descriptor() -> AppDescriptor {
//!         AppDescriptor::new("My App", "1.0.0")
//!     }
//!
//!     fn on_init(&mut self, ctx: &mut AppContext) {
//!         ctx.set_window_title("My App");
//!     }
//!
//!     fn on_frame(&mut self, ctx: &mut AppContext) {
//!         ctx.draw_text(10, 10, "Hello from My App!", 0xFFFFFF);
//!     }
//!
//!     fn on_event(&mut self, ctx: &mut AppContext, event: AppEvent) {
//!         // Handle input events
//!     }
//! }
//! ```

use super::renderer::{draw_text, fill_rect, draw_rect};
use super::window_manager::WindowId;
use super::widgets::{Button, Label, TextInput, Rect};

// ============================================================================
// App Descriptor
// ============================================================================

/// App metadata used for registration and display.
#[derive(Clone)]
pub struct AppDescriptor {
    /// Display name shown in title bar and app launcher
    pub name: [u8; 64],
    pub name_len: usize,

    /// Version string (e.g., "1.0.0")
    pub version: [u8; 16],
    pub version_len: usize,

    /// Author/publisher name
    pub author: [u8; 64],
    pub author_len: usize,

    /// Short description
    pub description: [u8; 256],
    pub description_len: usize,

    /// Unique app identifier (reverse domain style)
    pub app_id: [u8; 64],
    pub app_id_len: usize,

    /// Minimum window size
    pub min_width: u32,
    pub min_height: u32,

    /// Preferred window size
    pub preferred_width: u32,
    pub preferred_height: u32,

    /// App capabilities/permissions
    pub capabilities: AppCapabilities,
}

impl AppDescriptor {
    /// Create a new app descriptor with name and version.
    pub const fn new(name: &[u8], version: &[u8]) -> Self {
        let mut desc = Self {
            name: [0u8; 64],
            name_len: 0,
            version: [0u8; 16],
            version_len: 0,
            author: [0u8; 64],
            author_len: 0,
            description: [0u8; 256],
            description_len: 0,
            app_id: [0u8; 64],
            app_id_len: 0,
            min_width: 200,
            min_height: 100,
            preferred_width: 400,
            preferred_height: 300,
            capabilities: AppCapabilities::empty(),
        };

        // Copy name
        let mut i = 0;
        while i < name.len() && i < 64 {
            desc.name[i] = name[i];
            i += 1;
        }
        desc.name_len = i;

        // Copy version
        i = 0;
        while i < version.len() && i < 16 {
            desc.version[i] = version[i];
            i += 1;
        }
        desc.version_len = i;

        desc
    }

    /// Set the author name.
    pub const fn with_author(mut self, author: &[u8]) -> Self {
        let mut i = 0;
        while i < author.len() && i < 64 {
            self.author[i] = author[i];
            i += 1;
        }
        self.author_len = i;
        self
    }

    /// Set the description.
    pub const fn with_description(mut self, desc: &[u8]) -> Self {
        let mut i = 0;
        while i < desc.len() && i < 256 {
            self.description[i] = desc[i];
            i += 1;
        }
        self.description_len = i;
        self
    }

    /// Set the app ID.
    pub const fn with_app_id(mut self, app_id: &[u8]) -> Self {
        let mut i = 0;
        while i < app_id.len() && i < 64 {
            self.app_id[i] = app_id[i];
            i += 1;
        }
        self.app_id_len = i;
        self
    }

    /// Set preferred window size.
    pub const fn with_size(mut self, width: u32, height: u32) -> Self {
        self.preferred_width = width;
        self.preferred_height = height;
        self
    }

    /// Set minimum window size.
    pub const fn with_min_size(mut self, width: u32, height: u32) -> Self {
        self.min_width = width;
        self.min_height = height;
        self
    }

    /// Set capabilities.
    pub const fn with_capabilities(mut self, caps: AppCapabilities) -> Self {
        self.capabilities = caps;
        self
    }

    /// Get name as byte slice.
    pub fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    /// Get version as byte slice.
    pub fn version_bytes(&self) -> &[u8] {
        &self.version[..self.version_len]
    }
}

// ============================================================================
// App Capabilities
// ============================================================================

/// Permissions/capabilities an app can request.
#[derive(Clone, Copy, Default)]
pub struct AppCapabilities {
    bits: u32,
}

impl AppCapabilities {
    pub const NONE: u32 = 0;
    pub const FILESYSTEM: u32 = 1 << 0;      // Read/write files
    pub const NETWORK: u32 = 1 << 1;          // Network access
    pub const CLIPBOARD: u32 = 1 << 2;        // Clipboard access
    pub const NOTIFICATIONS: u32 = 1 << 3;    // Show notifications
    pub const BACKGROUND: u32 = 1 << 4;       // Run in background
    pub const SYSTEM_TRAY: u32 = 1 << 5;      // System tray icon
    pub const AUDIO: u32 = 1 << 6;            // Audio playback/recording
    pub const CAMERA: u32 = 1 << 7;           // Camera access
    pub const LOCATION: u32 = 1 << 8;         // Location services

    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self { bits }
    }

    pub const fn has(&self, cap: u32) -> bool {
        (self.bits & cap) != 0
    }

    pub const fn with(self, cap: u32) -> Self {
        Self { bits: self.bits | cap }
    }
}

// ============================================================================
// App Events
// ============================================================================

/// Events delivered to apps.
#[derive(Clone, Copy, Debug)]
pub enum AppEvent {
    /// Window was resized
    Resize { width: u32, height: u32 },

    /// Window gained focus
    FocusGained,

    /// Window lost focus
    FocusLost,

    /// Mouse moved within window
    MouseMove { x: i32, y: i32 },

    /// Mouse button pressed
    MouseDown { x: i32, y: i32, button: MouseButton },

    /// Mouse button released
    MouseUp { x: i32, y: i32, button: MouseButton },

    /// Mouse wheel scrolled
    MouseScroll { delta_x: i32, delta_y: i32 },

    /// Key pressed
    KeyDown { scancode: u8, key: Option<Key> },

    /// Key released
    KeyUp { scancode: u8, key: Option<Key> },

    /// Character typed (after keyboard layout processing)
    CharInput { ch: char },

    /// App should close (user clicked X)
    CloseRequested,

    /// Timer expired
    Timer { timer_id: u32 },

    /// Custom app-to-app message
    Message { sender_id: u32, data: u32 },
}

/// Mouse buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// Common key codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Escape, Tab, CapsLock, Shift, Ctrl, Alt, Super,
    Space, Enter, Backspace, Delete, Insert,
    Home, End, PageUp, PageDown,
    Left, Right, Up, Down,
    PrintScreen, ScrollLock, Pause,
}

// ============================================================================
// App Context
// ============================================================================

/// Runtime context provided to apps during lifecycle callbacks.
///
/// Provides access to:
/// - Window operations (title, size, position)
/// - Drawing primitives
/// - Input state
/// - Timers
/// - Inter-app messaging
pub struct AppContext {
    /// Window ID for this app
    pub window_id: WindowId,

    /// Content area bounds (excluding decorations)
    pub content_x: i32,
    pub content_y: i32,
    pub content_width: u32,
    pub content_height: u32,

    /// Current mouse position (relative to content area)
    pub mouse_x: i32,
    pub mouse_y: i32,

    /// Mouse button states
    pub mouse_left: bool,
    pub mouse_right: bool,
    pub mouse_middle: bool,

    /// Modifier key states
    pub shift_held: bool,
    pub ctrl_held: bool,
    pub alt_held: bool,

    /// Frame timing
    pub frame_count: u64,
    pub delta_time_ms: u32,
    pub total_time_ms: u64,

    /// App state
    pub should_close: bool,
    pub needs_redraw: bool,

    // Private state
    pending_events: [Option<AppEvent>; 32],
    event_count: usize,
}

impl AppContext {
    /// Create a new app context for a window.
    pub fn new(window_id: WindowId) -> Self {
        Self {
            window_id,
            content_x: 0,
            content_y: 0,
            content_width: 400,
            content_height: 300,
            mouse_x: 0,
            mouse_y: 0,
            mouse_left: false,
            mouse_right: false,
            mouse_middle: false,
            shift_held: false,
            ctrl_held: false,
            alt_held: false,
            frame_count: 0,
            delta_time_ms: 16,
            total_time_ms: 0,
            should_close: false,
            needs_redraw: true,
            pending_events: [None; 32],
            event_count: 0,
        }
    }

    // ========================================================================
    // Drawing API
    // ========================================================================

    /// Fill a rectangle with a solid color.
    pub fn fill_rect(&self, x: i32, y: i32, w: u32, h: u32, color: u32) {
        let abs_x = self.content_x + x;
        let abs_y = self.content_y + y;
        fill_rect(abs_x, abs_y, w, h, color);
    }

    /// Draw a rectangle outline.
    pub fn draw_rect(&self, x: i32, y: i32, w: u32, h: u32, color: u32) {
        let abs_x = self.content_x + x;
        let abs_y = self.content_y + y;
        draw_rect(abs_x, abs_y, w, h, color, 1);
    }

    /// Draw text at a position.
    pub fn draw_text(&self, x: i32, y: i32, text: &[u8], color: u32) {
        let abs_x = self.content_x + x;
        let abs_y = self.content_y + y;
        draw_text(abs_x, abs_y, text, color);
    }

    /// Clear the content area with a color.
    pub fn clear(&self, color: u32) {
        self.fill_rect(0, 0, self.content_width, self.content_height, color);
    }

    // ========================================================================
    // Widget API
    // ========================================================================

    /// Create a label widget.
    pub fn label(&self, _x: i32, _y: i32, text: &[u8]) -> Label {
        Label::new(text)
    }

    /// Create a button widget.
    pub fn button(&self, _x: i32, _y: i32, _w: u32, _h: u32, text: &[u8]) -> Button {
        Button::new(text)
    }

    /// Create a text input widget.
    pub fn text_input(&self, _x: i32, _y: i32, _w: u32) -> TextInput {
        TextInput::new()
    }

    // ========================================================================
    // Input API
    // ========================================================================

    /// Check if a point is within a rectangle.
    pub fn point_in_rect(&self, px: i32, py: i32, x: i32, y: i32, w: u32, h: u32) -> bool {
        px >= x && px < x + w as i32 && py >= y && py < y + h as i32
    }

    /// Check if mouse is hovering over a rectangle.
    pub fn is_hovered(&self, x: i32, y: i32, w: u32, h: u32) -> bool {
        self.point_in_rect(self.mouse_x, self.mouse_y, x, y, w, h)
    }

    /// Check if mouse is clicking on a rectangle.
    pub fn is_clicked(&self, x: i32, y: i32, w: u32, h: u32) -> bool {
        self.mouse_left && self.is_hovered(x, y, w, h)
    }

    // ========================================================================
    // Window API
    // ========================================================================

    /// Request the window be redrawn.
    pub fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    /// Request the window be closed.
    pub fn close(&mut self) {
        self.should_close = true;
    }

    /// Get the content area dimensions.
    pub fn size(&self) -> (u32, u32) {
        (self.content_width, self.content_height)
    }

    // ========================================================================
    // Event API
    // ========================================================================

    /// Push an event to the queue.
    pub fn push_event(&mut self, event: AppEvent) {
        if self.event_count < 32 {
            self.pending_events[self.event_count] = Some(event);
            self.event_count += 1;
        }
    }

    /// Pop an event from the queue.
    pub fn pop_event(&mut self) -> Option<AppEvent> {
        if self.event_count > 0 {
            self.event_count -= 1;
            self.pending_events[self.event_count].take()
        } else {
            None
        }
    }

    /// Check if there are pending events.
    pub fn has_events(&self) -> bool {
        self.event_count > 0
    }
}

// ============================================================================
// App Trait
// ============================================================================

/// The main trait that all RayOS apps must implement.
pub trait App {
    /// Returns the app descriptor with metadata.
    fn descriptor() -> AppDescriptor where Self: Sized;

    /// Called once when the app is initialized.
    fn on_init(&mut self, ctx: &mut AppContext);

    /// Called every frame to render the app.
    fn on_frame(&mut self, ctx: &mut AppContext);

    /// Called when an event occurs.
    fn on_event(&mut self, ctx: &mut AppContext, event: AppEvent);

    /// Called when the app is about to be destroyed.
    fn on_destroy(&mut self, _ctx: &mut AppContext) {
        // Default: do nothing
    }
}

// ============================================================================
// App Instance (Runtime)
// ============================================================================

/// Maximum number of registered apps.
const MAX_APPS: usize = 16;

/// Maximum number of running app instances.
const MAX_INSTANCES: usize = 32;

/// App registry entry.
pub struct AppRegistryEntry {
    pub descriptor: AppDescriptor,
    pub create_fn: fn() -> AppInstance,
}

/// A running app instance.
pub struct AppInstance {
    pub descriptor: AppDescriptor,
    pub context: AppContext,
    // App-specific state would be stored here via trait object
    // For now we use a simple callback-based approach
    pub on_frame_fn: Option<fn(&mut AppContext)>,
    pub on_event_fn: Option<fn(&mut AppContext, AppEvent)>,
}

impl AppInstance {
    pub fn new(descriptor: AppDescriptor, window_id: WindowId) -> Self {
        Self {
            descriptor,
            context: AppContext::new(window_id),
            on_frame_fn: None,
            on_event_fn: None,
        }
    }

    /// Update the context with current window state.
    pub fn update_context(&mut self, cx: i32, cy: i32, cw: u32, ch: u32) {
        self.context.content_x = cx;
        self.context.content_y = cy;
        self.context.content_width = cw;
        self.context.content_height = ch;
    }

    /// Tick the app (call on_frame).
    pub fn tick(&mut self) {
        self.context.frame_count += 1;
        self.context.total_time_ms += self.context.delta_time_ms as u64;

        if let Some(f) = self.on_frame_fn {
            f(&mut self.context);
        }
    }

    /// Deliver an event to the app.
    pub fn deliver_event(&mut self, event: AppEvent) {
        if let Some(f) = self.on_event_fn {
            f(&mut self.context, event);
        }
    }
}

// ============================================================================
// App Manager
// ============================================================================

/// Manages app registration and instances.
pub struct AppManager {
    /// Registered apps
    registry: [Option<AppRegistryEntry>; MAX_APPS],
    registry_count: usize,

    /// Running instances
    instances: [Option<AppInstance>; MAX_INSTANCES],
    instance_count: usize,
}

impl AppManager {
    pub const fn new() -> Self {
        const NONE_ENTRY: Option<AppRegistryEntry> = None;
        const NONE_INSTANCE: Option<AppInstance> = None;

        Self {
            registry: [NONE_ENTRY; MAX_APPS],
            registry_count: 0,
            instances: [NONE_INSTANCE; MAX_INSTANCES],
            instance_count: 0,
        }
    }

    /// Register an app type.
    pub fn register(&mut self, entry: AppRegistryEntry) -> bool {
        if self.registry_count < MAX_APPS {
            self.registry[self.registry_count] = Some(entry);
            self.registry_count += 1;
            true
        } else {
            false
        }
    }

    /// Launch an app by index.
    pub fn launch(&mut self, app_index: usize, window_id: WindowId) -> Option<usize> {
        if app_index >= self.registry_count {
            return None;
        }

        if self.instance_count >= MAX_INSTANCES {
            return None;
        }

        let entry = self.registry[app_index].as_ref()?;
        let instance = AppInstance::new(entry.descriptor.clone(), window_id);

        // Find empty slot
        for i in 0..MAX_INSTANCES {
            if self.instances[i].is_none() {
                self.instances[i] = Some(instance);
                self.instance_count += 1;
                return Some(i);
            }
        }

        None
    }

    /// Close an app instance.
    pub fn close(&mut self, instance_index: usize) -> bool {
        if instance_index < MAX_INSTANCES && self.instances[instance_index].is_some() {
            self.instances[instance_index] = None;
            self.instance_count -= 1;
            true
        } else {
            false
        }
    }

    /// Get a mutable reference to an instance.
    pub fn get_instance_mut(&mut self, index: usize) -> Option<&mut AppInstance> {
        self.instances.get_mut(index).and_then(|opt| opt.as_mut())
    }

    /// Find instance by window ID.
    pub fn find_by_window(&mut self, window_id: WindowId) -> Option<&mut AppInstance> {
        for instance in self.instances.iter_mut() {
            if let Some(inst) = instance {
                if inst.context.window_id == window_id {
                    return Some(inst);
                }
            }
        }
        None
    }

    /// Tick all running instances.
    pub fn tick_all(&mut self) {
        for instance in self.instances.iter_mut() {
            if let Some(inst) = instance {
                inst.tick();
            }
        }
    }

    /// Get registered app count.
    pub fn app_count(&self) -> usize {
        self.registry_count
    }

    /// Get running instance count.
    pub fn instance_count(&self) -> usize {
        self.instance_count
    }

    /// Get app descriptor by index.
    pub fn get_app_descriptor(&self, index: usize) -> Option<&AppDescriptor> {
        self.registry.get(index)
            .and_then(|opt| opt.as_ref())
            .map(|entry| &entry.descriptor)
    }
}

// Global app manager
static mut APP_MANAGER: AppManager = AppManager::new();

/// Get a reference to the global app manager.
pub fn app_manager() -> &'static mut AppManager {
    unsafe { &mut APP_MANAGER }
}

// ============================================================================
// Helper Macros
// ============================================================================

/// Define a RayOS app.
#[macro_export]
macro_rules! rayos_app {
    ($app_type:ty) => {
        impl $crate::ui::app_sdk::App for $app_type {
            fn descriptor() -> $crate::ui::app_sdk::AppDescriptor {
                <$app_type>::DESCRIPTOR
            }
        }
    };
}
