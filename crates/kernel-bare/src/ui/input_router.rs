//! Input Router & Focus Engine for RayOS UI
//!
//! Routes keyboard/mouse events to focused window with capture and grab support.
//! Provides Alt-Tab focus cycling, global shortcuts, and VM input injection.
//!
//! # Overview
//!
//! The Input Router is responsible for:
//! - Routing keyboard/mouse events to the focused window
//! - Managing focus stack for Alt-Tab cycling
//! - Handling keyboard/pointer grabs for modal interactions
//! - Global shortcut bindings (Super+D, Super+L, etc.)
//! - VM surface input injection via virtio-input
//!
//! # Markers
//!
//! - `RAYOS_INPUT:FOCUSED` - Window received focus
//! - `RAYOS_INPUT:GRABBED` - Keyboard/pointer grabbed
//! - `RAYOS_INPUT:SHORTCUT` - Global shortcut triggered
//! - `RAYOS_INPUT:ROUTED` - Event routed to target
//! - `RAYOS_INPUT:RELEASED` - Grab released

use super::window_manager::{WindowId, WINDOW_ID_NONE};
use super::surface_manager::SurfaceId;

// ============================================================================
// Constants
// ============================================================================

/// Maximum focus history size.
pub const MAX_FOCUS_HISTORY: usize = 8;

/// Maximum global shortcut bindings.
pub const MAX_SHORTCUTS: usize = 32;

/// Maximum pending events in queue.
pub const MAX_EVENT_QUEUE: usize = 64;

/// Maximum active grabs.
pub const MAX_GRABS: usize = 4;

/// No target ID.
pub const TARGET_NONE: u32 = 0;

// ============================================================================
// Input Target
// ============================================================================

/// Target for input events.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum InputTarget {
    /// No target.
    None = 0,
    /// Specific window.
    Window(WindowId),
    /// VM surface (for virtio-input injection).
    Surface(SurfaceId),
    /// System-level handler (global shortcuts).
    System = 3,
    /// Desktop background.
    Desktop = 4,
}

impl Default for InputTarget {
    fn default() -> Self {
        InputTarget::None
    }
}

impl InputTarget {
    /// Check if target is none.
    pub fn is_none(&self) -> bool {
        matches!(self, InputTarget::None)
    }

    /// Get window ID if target is a window.
    pub fn window_id(&self) -> Option<WindowId> {
        match self {
            InputTarget::Window(id) => Some(*id),
            _ => None,
        }
    }

    /// Get surface ID if target is a surface.
    pub fn surface_id(&self) -> Option<SurfaceId> {
        match self {
            InputTarget::Surface(id) => Some(*id),
            _ => None,
        }
    }
}

// ============================================================================
// Focus Policy
// ============================================================================

/// Focus policy for window selection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum FocusPolicy {
    /// Click to focus.
    ClickToFocus = 0,
    /// Focus follows mouse pointer.
    FocusFollowsMouse = 1,
    /// Focus only set explicitly.
    Explicit = 2,
}

impl Default for FocusPolicy {
    fn default() -> Self {
        FocusPolicy::ClickToFocus
    }
}

// ============================================================================
// Keyboard Scan Codes
// ============================================================================

/// Common keyboard scan codes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum KeyCode {
    /// Unknown/invalid key.
    Unknown = 0,
    /// Escape key.
    Escape = 1,
    /// Tab key.
    Tab = 15,
    /// Left Alt.
    LeftAlt = 56,
    /// Right Alt.
    RightAlt = 100,
    /// Left Super (Windows key).
    LeftSuper = 125,
    /// Right Super.
    RightSuper = 126,
    /// Left Shift.
    LeftShift = 42,
    /// Right Shift.
    RightShift = 54,
    /// Left Control.
    LeftCtrl = 29,
    /// Right Control.
    RightCtrl = 97,
    /// Enter key.
    Enter = 28,
    /// Backspace.
    Backspace = 14,
    /// Delete.
    Delete = 111,
    /// Home.
    Home = 102,
    /// End.
    End = 107,
    /// Page Up.
    PageUp = 104,
    /// Page Down.
    PageDown = 109,
    /// Left arrow.
    Left = 105,
    /// Right arrow.
    Right = 106,
    /// Up arrow.
    Up = 103,
    /// Down arrow.
    Down = 108,
    /// D key.
    D = 32,
    /// L key.
    L = 38,
    /// Q key.
    Q = 16,
    /// F1-F12.
    F1 = 59,
    F2 = 60,
    F3 = 61,
    F4 = 62,
    F5 = 63,
    F6 = 64,
    F7 = 65,
    F8 = 66,
    F9 = 67,
    F10 = 68,
    F11 = 87,
    F12 = 88,
}

impl From<u8> for KeyCode {
    fn from(scancode: u8) -> Self {
        match scancode {
            1 => KeyCode::Escape,
            15 => KeyCode::Tab,
            56 => KeyCode::LeftAlt,
            100 => KeyCode::RightAlt,
            125 => KeyCode::LeftSuper,
            126 => KeyCode::RightSuper,
            42 => KeyCode::LeftShift,
            54 => KeyCode::RightShift,
            29 => KeyCode::LeftCtrl,
            97 => KeyCode::RightCtrl,
            28 => KeyCode::Enter,
            14 => KeyCode::Backspace,
            111 => KeyCode::Delete,
            102 => KeyCode::Home,
            107 => KeyCode::End,
            104 => KeyCode::PageUp,
            109 => KeyCode::PageDown,
            105 => KeyCode::Left,
            106 => KeyCode::Right,
            103 => KeyCode::Up,
            108 => KeyCode::Down,
            32 => KeyCode::D,
            38 => KeyCode::L,
            16 => KeyCode::Q,
            59 => KeyCode::F1,
            60 => KeyCode::F2,
            61 => KeyCode::F3,
            62 => KeyCode::F4,
            63 => KeyCode::F5,
            64 => KeyCode::F6,
            65 => KeyCode::F7,
            66 => KeyCode::F8,
            67 => KeyCode::F9,
            68 => KeyCode::F10,
            87 => KeyCode::F11,
            88 => KeyCode::F12,
            _ => KeyCode::Unknown,
        }
    }
}

// ============================================================================
// Modifier Keys
// ============================================================================

/// Modifier key state flags.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers {
    /// Shift key pressed.
    pub shift: bool,
    /// Control key pressed.
    pub ctrl: bool,
    /// Alt key pressed.
    pub alt: bool,
    /// Super/Windows key pressed.
    pub super_key: bool,
}

impl Modifiers {
    /// Create empty modifiers.
    pub const fn none() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            super_key: false,
        }
    }

    /// Check if any modifier is pressed.
    pub fn any(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.super_key
    }

    /// Check if no modifiers are pressed.
    pub fn is_none(&self) -> bool {
        !self.any()
    }

    /// Match against required modifiers.
    pub fn matches(&self, required: &Modifiers) -> bool {
        self.shift == required.shift
            && self.ctrl == required.ctrl
            && self.alt == required.alt
            && self.super_key == required.super_key
    }

    /// Pack modifiers into a byte.
    pub fn pack(&self) -> u8 {
        let mut packed = 0u8;
        if self.shift {
            packed |= 1;
        }
        if self.ctrl {
            packed |= 2;
        }
        if self.alt {
            packed |= 4;
        }
        if self.super_key {
            packed |= 8;
        }
        packed
    }

    /// Unpack modifiers from a byte.
    pub fn unpack(packed: u8) -> Self {
        Self {
            shift: (packed & 1) != 0,
            ctrl: (packed & 2) != 0,
            alt: (packed & 4) != 0,
            super_key: (packed & 8) != 0,
        }
    }
}

// ============================================================================
// Input Events
// ============================================================================

/// Mouse button identifiers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum MouseButton {
    /// Left mouse button.
    Left = 0,
    /// Middle mouse button.
    Middle = 1,
    /// Right mouse button.
    Right = 2,
}

/// Input event types.
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    /// Key pressed.
    KeyDown {
        scancode: u8,
        keycode: KeyCode,
        modifiers: Modifiers,
    },
    /// Key released.
    KeyUp {
        scancode: u8,
        keycode: KeyCode,
        modifiers: Modifiers,
    },
    /// Mouse moved.
    MouseMove {
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
    },
    /// Mouse button pressed.
    MouseDown {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    /// Mouse button released.
    MouseUp {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    /// Mouse wheel scroll.
    MouseScroll {
        delta_x: i32,
        delta_y: i32,
    },
    /// Touch gesture (for future).
    Gesture {
        gesture_type: u8,
        x: i32,
        y: i32,
        param: i32,
    },
}

impl Default for InputEvent {
    fn default() -> Self {
        InputEvent::KeyDown {
            scancode: 0,
            keycode: KeyCode::Unknown,
            modifiers: Modifiers::none(),
        }
    }
}

impl InputEvent {
    /// Check if this is a keyboard event.
    pub fn is_keyboard(&self) -> bool {
        matches!(self, InputEvent::KeyDown { .. } | InputEvent::KeyUp { .. })
    }

    /// Check if this is a mouse event.
    pub fn is_mouse(&self) -> bool {
        matches!(
            self,
            InputEvent::MouseMove { .. }
                | InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseScroll { .. }
        )
    }

    /// Get the scancode if this is a key event.
    pub fn scancode(&self) -> Option<u8> {
        match self {
            InputEvent::KeyDown { scancode, .. } | InputEvent::KeyUp { scancode, .. } => {
                Some(*scancode)
            }
            _ => None,
        }
    }

    /// Get mouse position if this is a positional mouse event.
    pub fn position(&self) -> Option<(i32, i32)> {
        match self {
            InputEvent::MouseMove { x, y, .. }
            | InputEvent::MouseDown { x, y, .. }
            | InputEvent::MouseUp { x, y, .. } => Some((*x, *y)),
            _ => None,
        }
    }
}

// ============================================================================
// Focus Stack
// ============================================================================

/// Focus history for Alt-Tab cycling.
pub struct FocusStack {
    /// Window IDs in focus order (most recent first).
    history: [WindowId; MAX_FOCUS_HISTORY],
    /// Number of valid entries.
    count: usize,
    /// Current cycle position (for Alt-Tab).
    cycle_pos: usize,
    /// Alt-Tab is active.
    cycling: bool,
}

impl FocusStack {
    /// Create a new focus stack.
    pub const fn new() -> Self {
        Self {
            history: [WINDOW_ID_NONE; MAX_FOCUS_HISTORY],
            count: 0,
            cycle_pos: 0,
            cycling: false,
        }
    }

    /// Push a window to the front of the focus stack.
    pub fn push(&mut self, window_id: WindowId) {
        if window_id == WINDOW_ID_NONE {
            return;
        }

        // Remove existing entry if present
        self.remove(window_id);

        // Shift everything right
        if self.count >= MAX_FOCUS_HISTORY {
            self.count = MAX_FOCUS_HISTORY - 1;
        }
        for i in (1..=self.count).rev() {
            self.history[i] = self.history[i - 1];
        }

        // Insert at front
        self.history[0] = window_id;
        self.count += 1;
    }

    /// Remove a window from the stack.
    pub fn remove(&mut self, window_id: WindowId) {
        let mut i = 0;
        while i < self.count {
            if self.history[i] == window_id {
                // Shift left
                for j in i..self.count - 1 {
                    self.history[j] = self.history[j + 1];
                }
                self.history[self.count - 1] = WINDOW_ID_NONE;
                self.count -= 1;
            } else {
                i += 1;
            }
        }
    }

    /// Get the currently focused window.
    pub fn current(&self) -> WindowId {
        if self.count > 0 {
            self.history[0]
        } else {
            WINDOW_ID_NONE
        }
    }

    /// Start Alt-Tab cycling.
    pub fn start_cycle(&mut self) {
        self.cycling = true;
        self.cycle_pos = 0;
    }

    /// Cycle to next window in focus history.
    pub fn cycle_next(&mut self) -> WindowId {
        if self.count == 0 {
            return WINDOW_ID_NONE;
        }
        self.cycle_pos = (self.cycle_pos + 1) % self.count;
        self.history[self.cycle_pos]
    }

    /// Cycle to previous window in focus history.
    pub fn cycle_prev(&mut self) -> WindowId {
        if self.count == 0 {
            return WINDOW_ID_NONE;
        }
        if self.cycle_pos == 0 {
            self.cycle_pos = self.count - 1;
        } else {
            self.cycle_pos -= 1;
        }
        self.history[self.cycle_pos]
    }

    /// Finish cycling and apply selection.
    pub fn finish_cycle(&mut self) -> WindowId {
        if !self.cycling {
            return self.current();
        }
        self.cycling = false;

        // Move selected window to front
        let selected = self.history[self.cycle_pos];
        if self.cycle_pos > 0 {
            self.remove(selected);
            self.push(selected);
        }
        selected
    }

    /// Check if Alt-Tab cycling is active.
    pub fn is_cycling(&self) -> bool {
        self.cycling
    }

    /// Get current cycle position.
    pub fn cycle_position(&self) -> usize {
        self.cycle_pos
    }

    /// Get window at index.
    pub fn get(&self, index: usize) -> WindowId {
        if index < self.count {
            self.history[index]
        } else {
            WINDOW_ID_NONE
        }
    }

    /// Get number of windows in stack.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if stack is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Clear the focus stack.
    pub fn clear(&mut self) {
        for i in 0..MAX_FOCUS_HISTORY {
            self.history[i] = WINDOW_ID_NONE;
        }
        self.count = 0;
        self.cycle_pos = 0;
        self.cycling = false;
    }
}

// ============================================================================
// Grab Types
// ============================================================================

/// Grab type for exclusive input capture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum GrabType {
    /// No grab.
    None = 0,
    /// Keyboard grab (exclusive keyboard input).
    Keyboard = 1,
    /// Pointer grab (exclusive mouse input).
    Pointer = 2,
    /// Both keyboard and pointer.
    Both = 3,
}

/// Grab entry for a window.
#[derive(Clone, Copy)]
pub struct GrabEntry {
    /// Window that owns the grab.
    pub window_id: WindowId,
    /// Type of grab.
    pub grab_type: GrabType,
    /// Whether grab is active.
    pub active: bool,
    /// Timestamp when grab was acquired.
    pub timestamp: u64,
    /// Reason for grab (for debugging).
    pub reason: GrabReason,
}

/// Reason for acquiring a grab.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum GrabReason {
    /// No reason / not grabbed.
    None = 0,
    /// Modal dialog.
    Modal = 1,
    /// Window resize operation.
    Resize = 2,
    /// Window drag operation.
    Drag = 3,
    /// Menu open.
    Menu = 4,
    /// VM surface active.
    VmActive = 5,
    /// Capture for screenshot.
    Capture = 6,
}

impl GrabEntry {
    /// Create an empty grab entry.
    pub const fn empty() -> Self {
        Self {
            window_id: WINDOW_ID_NONE,
            grab_type: GrabType::None,
            active: false,
            timestamp: 0,
            reason: GrabReason::None,
        }
    }

    /// Check if this entry is active.
    pub fn is_active(&self) -> bool {
        self.active && self.grab_type != GrabType::None
    }

    /// Check if this entry has keyboard grab.
    pub fn has_keyboard(&self) -> bool {
        self.active && matches!(self.grab_type, GrabType::Keyboard | GrabType::Both)
    }

    /// Check if this entry has pointer grab.
    pub fn has_pointer(&self) -> bool {
        self.active && matches!(self.grab_type, GrabType::Pointer | GrabType::Both)
    }
}

// ============================================================================
// Shortcut Actions
// ============================================================================

/// Actions triggered by global shortcuts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ShortcutAction {
    /// No action.
    None = 0,
    /// Show desktop (minimize all).
    ShowDesktop = 1,
    /// Lock screen.
    LockScreen = 2,
    /// Open app launcher.
    OpenLauncher = 3,
    /// Open terminal.
    OpenTerminal = 4,
    /// Close focused window.
    CloseWindow = 5,
    /// Maximize/restore focused window.
    ToggleMaximize = 6,
    /// Minimize focused window.
    MinimizeWindow = 7,
    /// Switch to workspace N (1-4).
    SwitchWorkspace1 = 8,
    SwitchWorkspace2 = 9,
    SwitchWorkspace3 = 10,
    SwitchWorkspace4 = 11,
    /// Move window to workspace N.
    MoveToWorkspace1 = 12,
    MoveToWorkspace2 = 13,
    MoveToWorkspace3 = 14,
    MoveToWorkspace4 = 15,
    /// Start Alt-Tab cycling.
    StartAltTab = 16,
    /// Screenshot.
    Screenshot = 17,
    /// Open settings.
    OpenSettings = 18,
    /// Logout.
    Logout = 19,
    /// Custom action (user-defined).
    Custom(u8) = 20,
}

impl Default for ShortcutAction {
    fn default() -> Self {
        ShortcutAction::None
    }
}

// ============================================================================
// Shortcut Binding
// ============================================================================

/// A global shortcut binding.
#[derive(Clone, Copy)]
pub struct ShortcutBinding {
    /// Key code for the shortcut.
    pub keycode: KeyCode,
    /// Required modifiers.
    pub modifiers: Modifiers,
    /// Action to trigger.
    pub action: ShortcutAction,
    /// Whether binding is enabled.
    pub enabled: bool,
}

impl ShortcutBinding {
    /// Create an empty binding.
    pub const fn empty() -> Self {
        Self {
            keycode: KeyCode::Unknown,
            modifiers: Modifiers::none(),
            action: ShortcutAction::None,
            enabled: false,
        }
    }

    /// Create a new binding.
    pub const fn new(keycode: KeyCode, modifiers: Modifiers, action: ShortcutAction) -> Self {
        Self {
            keycode,
            modifiers,
            action,
            enabled: true,
        }
    }

    /// Check if a key event matches this binding.
    pub fn matches(&self, keycode: KeyCode, modifiers: &Modifiers) -> bool {
        self.enabled
            && self.keycode as u8 == keycode as u8
            && self.modifiers.matches(modifiers)
    }
}

// ============================================================================
// Input Filter
// ============================================================================

/// Filter for input routing decisions.
pub struct InputFilter {
    /// Window bounds for hit testing.
    pub bounds: (i32, i32, u32, u32),
    /// Whether window is visible.
    pub visible: bool,
    /// Whether window accepts focus.
    pub focusable: bool,
    /// Whether window accepts keyboard input.
    pub keyboard_enabled: bool,
    /// Whether window accepts mouse input.
    pub mouse_enabled: bool,
}

impl Default for InputFilter {
    fn default() -> Self {
        Self {
            bounds: (0, 0, 0, 0),
            visible: true,
            focusable: true,
            keyboard_enabled: true,
            mouse_enabled: true,
        }
    }
}

impl InputFilter {
    /// Check if a point is within bounds.
    pub fn hit_test(&self, x: i32, y: i32) -> bool {
        if !self.visible {
            return false;
        }
        let (bx, by, bw, bh) = self.bounds;
        x >= bx && x < bx + bw as i32 && y >= by && y < by + bh as i32
    }
}

// ============================================================================
// Routed Event
// ============================================================================

/// An event with routing information.
#[derive(Clone, Copy)]
pub struct RoutedEvent {
    /// The input event.
    pub event: InputEvent,
    /// Target for the event.
    pub target: InputTarget,
    /// Timestamp.
    pub timestamp: u64,
    /// Whether event was handled.
    pub handled: bool,
    /// Whether event should propagate.
    pub propagate: bool,
}

impl RoutedEvent {
    /// Create a new routed event.
    pub fn new(event: InputEvent, target: InputTarget, timestamp: u64) -> Self {
        Self {
            event,
            target,
            timestamp,
            handled: false,
            propagate: true,
        }
    }

    /// Mark event as handled.
    pub fn handle(&mut self) {
        self.handled = true;
        self.propagate = false;
    }

    /// Stop propagation without marking handled.
    pub fn stop_propagation(&mut self) {
        self.propagate = false;
    }
}

// ============================================================================
// Input Router
// ============================================================================

/// Main input router for the UI system.
pub struct InputRouter {
    /// Focus stack for Alt-Tab.
    focus_stack: FocusStack,
    /// Current focus policy.
    focus_policy: FocusPolicy,
    /// Active grabs.
    grabs: [GrabEntry; MAX_GRABS],
    /// Number of active grabs.
    grab_count: usize,
    /// Global shortcut bindings.
    shortcuts: [ShortcutBinding; MAX_SHORTCUTS],
    /// Number of shortcut bindings.
    shortcut_count: usize,
    /// Current modifier state.
    modifiers: Modifiers,
    /// Last mouse position.
    last_mouse_x: i32,
    last_mouse_y: i32,
    /// Timestamp counter.
    timestamp: u64,
    /// Event queue.
    event_queue: [RoutedEvent; MAX_EVENT_QUEUE],
    /// Queue head index.
    queue_head: usize,
    /// Queue tail index.
    queue_tail: usize,
    /// Statistics: events routed.
    stats_routed: u64,
    /// Statistics: events dropped.
    stats_dropped: u64,
    /// Statistics: shortcuts triggered.
    stats_shortcuts: u64,
}

impl InputRouter {
    /// Create a new input router.
    pub const fn new() -> Self {
        Self {
            focus_stack: FocusStack::new(),
            focus_policy: FocusPolicy::ClickToFocus,
            grabs: [GrabEntry::empty(); MAX_GRABS],
            grab_count: 0,
            shortcuts: [ShortcutBinding::empty(); MAX_SHORTCUTS],
            shortcut_count: 0,
            modifiers: Modifiers::none(),
            last_mouse_x: 0,
            last_mouse_y: 0,
            timestamp: 0,
            event_queue: [RoutedEvent {
                event: InputEvent::KeyDown {
                    scancode: 0,
                    keycode: KeyCode::Unknown,
                    modifiers: Modifiers::none(),
                },
                target: InputTarget::None,
                timestamp: 0,
                handled: false,
                propagate: false,
            }; MAX_EVENT_QUEUE],
            queue_head: 0,
            queue_tail: 0,
            stats_routed: 0,
            stats_dropped: 0,
            stats_shortcuts: 0,
        }
    }

    /// Initialize with default shortcuts.
    pub fn init_default_shortcuts(&mut self) {
        // Super+D = Show Desktop
        self.add_shortcut(ShortcutBinding::new(
            KeyCode::D,
            Modifiers {
                super_key: true,
                ..Modifiers::none()
            },
            ShortcutAction::ShowDesktop,
        ));

        // Super+L = Lock Screen
        self.add_shortcut(ShortcutBinding::new(
            KeyCode::L,
            Modifiers {
                super_key: true,
                ..Modifiers::none()
            },
            ShortcutAction::LockScreen,
        ));

        // Super = Open Launcher
        self.add_shortcut(ShortcutBinding::new(
            KeyCode::LeftSuper,
            Modifiers::none(),
            ShortcutAction::OpenLauncher,
        ));

        // Alt+F4 = Close Window
        self.add_shortcut(ShortcutBinding::new(
            KeyCode::F4,
            Modifiers {
                alt: true,
                ..Modifiers::none()
            },
            ShortcutAction::CloseWindow,
        ));

        // Alt+Tab = Start Alt-Tab
        self.add_shortcut(ShortcutBinding::new(
            KeyCode::Tab,
            Modifiers {
                alt: true,
                ..Modifiers::none()
            },
            ShortcutAction::StartAltTab,
        ));

        // Super+1-4 = Switch Workspace
        self.add_shortcut(ShortcutBinding::new(
            KeyCode::F1,
            Modifiers {
                super_key: true,
                ..Modifiers::none()
            },
            ShortcutAction::SwitchWorkspace1,
        ));
    }

    /// Add a shortcut binding.
    pub fn add_shortcut(&mut self, binding: ShortcutBinding) -> bool {
        if self.shortcut_count >= MAX_SHORTCUTS {
            return false;
        }
        self.shortcuts[self.shortcut_count] = binding;
        self.shortcut_count += 1;
        true
    }

    /// Remove a shortcut by action.
    pub fn remove_shortcut(&mut self, action: ShortcutAction) {
        let mut i = 0;
        while i < self.shortcut_count {
            if core::mem::discriminant(&self.shortcuts[i].action)
                == core::mem::discriminant(&action)
            {
                for j in i..self.shortcut_count - 1 {
                    self.shortcuts[j] = self.shortcuts[j + 1];
                }
                self.shortcut_count -= 1;
            } else {
                i += 1;
            }
        }
    }

    /// Set the focus policy.
    pub fn set_focus_policy(&mut self, policy: FocusPolicy) {
        self.focus_policy = policy;
    }

    /// Get current focus policy.
    pub fn focus_policy(&self) -> FocusPolicy {
        self.focus_policy
    }

    /// Focus a window.
    pub fn focus_window(&mut self, window_id: WindowId) {
        if window_id != WINDOW_ID_NONE {
            self.focus_stack.push(window_id);
            // RAYOS_INPUT:FOCUSED
        }
    }

    /// Get currently focused window.
    pub fn focused_window(&self) -> WindowId {
        self.focus_stack.current()
    }

    /// Remove a window from focus tracking.
    pub fn remove_window(&mut self, window_id: WindowId) {
        self.focus_stack.remove(window_id);
        // Also release any grabs by this window
        self.release_grabs_for(window_id);
    }

    /// Acquire a grab for a window.
    pub fn acquire_grab(
        &mut self,
        window_id: WindowId,
        grab_type: GrabType,
        reason: GrabReason,
    ) -> bool {
        if grab_type == GrabType::None {
            return false;
        }

        // Check for existing grab
        for grab in &self.grabs[..self.grab_count] {
            if grab.is_active() && grab.window_id != window_id {
                // Another window has a grab
                return false;
            }
        }

        // Find or create entry
        for grab in &mut self.grabs[..self.grab_count] {
            if grab.window_id == window_id {
                grab.grab_type = grab_type;
                grab.active = true;
                grab.reason = reason;
                grab.timestamp = self.timestamp;
                // RAYOS_INPUT:GRABBED
                return true;
            }
        }

        if self.grab_count >= MAX_GRABS {
            return false;
        }

        self.grabs[self.grab_count] = GrabEntry {
            window_id,
            grab_type,
            active: true,
            timestamp: self.timestamp,
            reason,
        };
        self.grab_count += 1;
        // RAYOS_INPUT:GRABBED
        true
    }

    /// Release grabs for a window.
    pub fn release_grabs_for(&mut self, window_id: WindowId) {
        for grab in &mut self.grabs[..self.grab_count] {
            if grab.window_id == window_id {
                grab.active = false;
                grab.grab_type = GrabType::None;
                // RAYOS_INPUT:RELEASED
            }
        }
    }

    /// Release all grabs.
    pub fn release_all_grabs(&mut self) {
        for grab in &mut self.grabs[..self.grab_count] {
            grab.active = false;
            grab.grab_type = GrabType::None;
        }
        // RAYOS_INPUT:RELEASED
    }

    /// Get the window with keyboard grab, if any.
    pub fn keyboard_grab_owner(&self) -> WindowId {
        for grab in &self.grabs[..self.grab_count] {
            if grab.has_keyboard() {
                return grab.window_id;
            }
        }
        WINDOW_ID_NONE
    }

    /// Get the window with pointer grab, if any.
    pub fn pointer_grab_owner(&self) -> WindowId {
        for grab in &self.grabs[..self.grab_count] {
            if grab.has_pointer() {
                return grab.window_id;
            }
        }
        WINDOW_ID_NONE
    }

    /// Update modifier state from a key event.
    pub fn update_modifiers(&mut self, scancode: u8, pressed: bool) {
        match scancode {
            42 | 54 => self.modifiers.shift = pressed,  // Left/Right Shift
            29 | 97 => self.modifiers.ctrl = pressed,   // Left/Right Ctrl
            56 | 100 => self.modifiers.alt = pressed,   // Left/Right Alt
            125 | 126 => self.modifiers.super_key = pressed, // Left/Right Super
            _ => {}
        }
    }

    /// Get current modifiers.
    pub fn current_modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Check for and trigger global shortcuts.
    fn check_shortcuts(&mut self, keycode: KeyCode) -> Option<ShortcutAction> {
        for binding in &self.shortcuts[..self.shortcut_count] {
            if binding.matches(keycode, &self.modifiers) {
                self.stats_shortcuts += 1;
                // RAYOS_INPUT:SHORTCUT
                return Some(binding.action);
            }
        }
        None
    }

    /// Start Alt-Tab cycling.
    pub fn start_alt_tab(&mut self) {
        self.focus_stack.start_cycle();
    }

    /// Cycle Alt-Tab forward.
    pub fn alt_tab_next(&mut self) -> WindowId {
        self.focus_stack.cycle_next()
    }

    /// Cycle Alt-Tab backward.
    pub fn alt_tab_prev(&mut self) -> WindowId {
        self.focus_stack.cycle_prev()
    }

    /// Finish Alt-Tab and select window.
    pub fn finish_alt_tab(&mut self) -> WindowId {
        self.focus_stack.finish_cycle()
    }

    /// Check if Alt-Tab is active.
    pub fn is_alt_tab_active(&self) -> bool {
        self.focus_stack.is_cycling()
    }

    /// Route a keyboard event.
    pub fn route_keyboard(&mut self, event: InputEvent) -> Option<RoutedEvent> {
        let scancode = event.scancode()?;
        let keycode = KeyCode::from(scancode);

        // Update modifiers
        if let InputEvent::KeyDown { .. } = event {
            self.update_modifiers(scancode, true);
        } else if let InputEvent::KeyUp { .. } = event {
            self.update_modifiers(scancode, false);
        }

        // Check for global shortcuts (on key down only)
        if let InputEvent::KeyDown { .. } = event {
            if let Some(action) = self.check_shortcuts(keycode) {
                // Handle shortcut internally
                self.handle_shortcut_action(action);
                return None; // Shortcut consumed event
            }
        }

        // Route to grab owner or focused window
        let target = if let owner = self.keyboard_grab_owner() {
            if owner != WINDOW_ID_NONE {
                InputTarget::Window(owner)
            } else {
                InputTarget::Window(self.focused_window())
            }
        } else {
            InputTarget::Window(self.focused_window())
        };

        self.timestamp += 1;
        let routed = RoutedEvent::new(event, target, self.timestamp);
        self.stats_routed += 1;
        // RAYOS_INPUT:ROUTED

        Some(routed)
    }

    /// Route a mouse event.
    pub fn route_mouse(
        &mut self,
        event: InputEvent,
        window_lookup: Option<&dyn Fn(i32, i32) -> WindowId>,
    ) -> Option<RoutedEvent> {
        // Update last position
        if let Some((x, y)) = event.position() {
            self.last_mouse_x = x;
            self.last_mouse_y = y;
        }

        // Route to grab owner if any
        let pointer_owner = self.pointer_grab_owner();
        if pointer_owner != WINDOW_ID_NONE {
            self.timestamp += 1;
            let routed = RoutedEvent::new(event, InputTarget::Window(pointer_owner), self.timestamp);
            self.stats_routed += 1;
            // RAYOS_INPUT:ROUTED
            return Some(routed);
        }

        // Focus follows mouse policy
        if self.focus_policy == FocusPolicy::FocusFollowsMouse {
            if let Some(lookup) = window_lookup {
                let hit_window = lookup(self.last_mouse_x, self.last_mouse_y);
                if hit_window != WINDOW_ID_NONE && hit_window != self.focused_window() {
                    self.focus_window(hit_window);
                }
            }
        }

        // Click to focus policy
        if self.focus_policy == FocusPolicy::ClickToFocus {
            if let InputEvent::MouseDown { .. } = event {
                if let Some(lookup) = window_lookup {
                    let hit_window = lookup(self.last_mouse_x, self.last_mouse_y);
                    if hit_window != WINDOW_ID_NONE {
                        self.focus_window(hit_window);
                    }
                }
            }
        }

        // Route to window under cursor or focused window
        let target = if let Some(lookup) = window_lookup {
            let hit = lookup(self.last_mouse_x, self.last_mouse_y);
            if hit != WINDOW_ID_NONE {
                InputTarget::Window(hit)
            } else {
                InputTarget::Desktop
            }
        } else {
            InputTarget::Window(self.focused_window())
        };

        self.timestamp += 1;
        let routed = RoutedEvent::new(event, target, self.timestamp);
        self.stats_routed += 1;
        // RAYOS_INPUT:ROUTED

        Some(routed)
    }

    /// Handle a shortcut action.
    fn handle_shortcut_action(&mut self, action: ShortcutAction) {
        match action {
            ShortcutAction::StartAltTab => {
                self.start_alt_tab();
            }
            ShortcutAction::ShowDesktop => {
                // Minimize all windows - handled by shell
            }
            ShortcutAction::LockScreen => {
                // Lock screen - handled by shell
            }
            ShortcutAction::OpenLauncher => {
                // Open launcher - handled by shell
            }
            ShortcutAction::CloseWindow => {
                // Close focused window - handled by window manager
            }
            _ => {
                // Other actions handled by shell or window manager
            }
        }
    }

    /// Enqueue an event for deferred processing.
    pub fn enqueue(&mut self, event: RoutedEvent) -> bool {
        let next_tail = (self.queue_tail + 1) % MAX_EVENT_QUEUE;
        if next_tail == self.queue_head {
            self.stats_dropped += 1;
            return false;
        }
        self.event_queue[self.queue_tail] = event;
        self.queue_tail = next_tail;
        true
    }

    /// Dequeue an event for processing.
    pub fn dequeue(&mut self) -> Option<RoutedEvent> {
        if self.queue_head == self.queue_tail {
            return None;
        }
        let event = self.event_queue[self.queue_head];
        self.queue_head = (self.queue_head + 1) % MAX_EVENT_QUEUE;
        Some(event)
    }

    /// Get queue size.
    pub fn queue_len(&self) -> usize {
        if self.queue_tail >= self.queue_head {
            self.queue_tail - self.queue_head
        } else {
            MAX_EVENT_QUEUE - self.queue_head + self.queue_tail
        }
    }

    /// Get focus stack reference.
    pub fn focus_stack(&self) -> &FocusStack {
        &self.focus_stack
    }

    /// Get focus stack mutable reference.
    pub fn focus_stack_mut(&mut self) -> &mut FocusStack {
        &mut self.focus_stack
    }

    /// Get routing statistics.
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.stats_routed, self.stats_dropped, self.stats_shortcuts)
    }

    /// Reset statistics.
    pub fn reset_stats(&mut self) {
        self.stats_routed = 0;
        self.stats_dropped = 0;
        self.stats_shortcuts = 0;
    }
}

// ============================================================================
// VM Input Injection
// ============================================================================

/// VM input injection for guest surfaces.
pub struct VmInputInjector {
    /// Target surface ID.
    surface_id: SurfaceId,
    /// Whether injection is active.
    active: bool,
    /// Mouse position within surface.
    surface_x: i32,
    surface_y: i32,
    /// Surface offset in window.
    offset_x: i32,
    offset_y: i32,
}

impl VmInputInjector {
    /// Create a new VM input injector.
    pub const fn new() -> Self {
        Self {
            surface_id: 0,
            active: false,
            surface_x: 0,
            surface_y: 0,
            offset_x: 0,
            offset_y: 0,
        }
    }

    /// Activate injection for a surface.
    pub fn activate(&mut self, surface_id: SurfaceId, offset_x: i32, offset_y: i32) {
        self.surface_id = surface_id;
        self.offset_x = offset_x;
        self.offset_y = offset_y;
        self.active = true;
    }

    /// Deactivate injection.
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Check if injection is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get target surface.
    pub fn surface(&self) -> SurfaceId {
        self.surface_id
    }

    /// Translate mouse position to surface coordinates.
    pub fn translate_position(&mut self, window_x: i32, window_y: i32) -> (i32, i32) {
        self.surface_x = window_x - self.offset_x;
        self.surface_y = window_y - self.offset_y;
        (self.surface_x, self.surface_y)
    }

    /// Inject a keyboard event.
    pub fn inject_key(&self, scancode: u8, pressed: bool) -> Option<VirtioInputEvent> {
        if !self.active {
            return None;
        }
        Some(VirtioInputEvent {
            event_type: VIRTIO_INPUT_EV_KEY,
            code: scancode as u16,
            value: if pressed { 1 } else { 0 },
        })
    }

    /// Inject a mouse move event.
    pub fn inject_mouse_move(&self, x: i32, y: i32) -> Option<[VirtioInputEvent; 2]> {
        if !self.active {
            return None;
        }
        Some([
            VirtioInputEvent {
                event_type: VIRTIO_INPUT_EV_ABS,
                code: VIRTIO_ABS_X,
                value: x as u32,
            },
            VirtioInputEvent {
                event_type: VIRTIO_INPUT_EV_ABS,
                code: VIRTIO_ABS_Y,
                value: y as u32,
            },
        ])
    }

    /// Inject a mouse button event.
    pub fn inject_mouse_button(&self, button: MouseButton, pressed: bool) -> Option<VirtioInputEvent> {
        if !self.active {
            return None;
        }
        let code = match button {
            MouseButton::Left => VIRTIO_BTN_LEFT,
            MouseButton::Middle => VIRTIO_BTN_MIDDLE,
            MouseButton::Right => VIRTIO_BTN_RIGHT,
        };
        Some(VirtioInputEvent {
            event_type: VIRTIO_INPUT_EV_KEY,
            code,
            value: if pressed { 1 } else { 0 },
        })
    }
}

/// Virtio input event for VM injection.
#[derive(Clone, Copy, Debug)]
pub struct VirtioInputEvent {
    /// Event type.
    pub event_type: u16,
    /// Event code.
    pub code: u16,
    /// Event value.
    pub value: u32,
}

// Virtio input constants
const VIRTIO_INPUT_EV_KEY: u16 = 0x01;
const VIRTIO_INPUT_EV_ABS: u16 = 0x03;
const VIRTIO_ABS_X: u16 = 0x00;
const VIRTIO_ABS_Y: u16 = 0x01;
const VIRTIO_BTN_LEFT: u16 = 0x110;
const VIRTIO_BTN_RIGHT: u16 = 0x111;
const VIRTIO_BTN_MIDDLE: u16 = 0x112;

// ============================================================================
// Global Input Router
// ============================================================================

/// Global input router instance.
static mut GLOBAL_INPUT_ROUTER: InputRouter = InputRouter::new();

/// Global VM input injector.
static mut GLOBAL_VM_INJECTOR: VmInputInjector = VmInputInjector::new();

/// Get the global input router.
pub fn input_router() -> &'static InputRouter {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_INPUT_ROUTER }
}

/// Get the global input router mutably.
pub fn input_router_mut() -> &'static mut InputRouter {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_INPUT_ROUTER }
}

/// Get the global VM input injector.
pub fn vm_injector() -> &'static VmInputInjector {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_VM_INJECTOR }
}

/// Get the global VM input injector mutably.
pub fn vm_injector_mut() -> &'static mut VmInputInjector {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_VM_INJECTOR }
}

/// Initialize the input router with default settings.
pub fn init_input_router() {
    let router = input_router_mut();
    router.init_default_shortcuts();
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_stack_basic() {
        let mut stack = FocusStack::new();
        assert!(stack.is_empty());

        stack.push(1);
        stack.push(2);
        stack.push(3);

        assert_eq!(stack.len(), 3);
        assert_eq!(stack.current(), 3);
        assert_eq!(stack.get(0), 3);
        assert_eq!(stack.get(1), 2);
        assert_eq!(stack.get(2), 1);
    }

    #[test]
    fn test_focus_stack_remove() {
        let mut stack = FocusStack::new();
        stack.push(1);
        stack.push(2);
        stack.push(3);

        stack.remove(2);

        assert_eq!(stack.len(), 2);
        assert_eq!(stack.get(0), 3);
        assert_eq!(stack.get(1), 1);
    }

    #[test]
    fn test_focus_stack_cycling() {
        let mut stack = FocusStack::new();
        stack.push(1);
        stack.push(2);
        stack.push(3);

        stack.start_cycle();
        assert!(stack.is_cycling());

        assert_eq!(stack.cycle_next(), 2);
        assert_eq!(stack.cycle_next(), 1);
        assert_eq!(stack.cycle_next(), 3);

        let selected = stack.finish_cycle();
        assert!(!stack.is_cycling());
        assert_eq!(selected, 3);
    }

    #[test]
    fn test_modifiers_pack_unpack() {
        let mods = Modifiers {
            shift: true,
            ctrl: false,
            alt: true,
            super_key: false,
        };

        let packed = mods.pack();
        let unpacked = Modifiers::unpack(packed);

        assert!(unpacked.shift);
        assert!(!unpacked.ctrl);
        assert!(unpacked.alt);
        assert!(!unpacked.super_key);
    }

    #[test]
    fn test_shortcut_binding_match() {
        let binding = ShortcutBinding::new(
            KeyCode::D,
            Modifiers {
                super_key: true,
                ..Modifiers::none()
            },
            ShortcutAction::ShowDesktop,
        );

        // Should match
        let mods = Modifiers {
            super_key: true,
            ..Modifiers::none()
        };
        assert!(binding.matches(KeyCode::D, &mods));

        // Should not match (wrong key)
        assert!(!binding.matches(KeyCode::L, &mods));

        // Should not match (missing modifier)
        let mods2 = Modifiers::none();
        assert!(!binding.matches(KeyCode::D, &mods2));
    }

    #[test]
    fn test_input_router_shortcuts() {
        let mut router = InputRouter::new();
        router.init_default_shortcuts();

        // Verify shortcuts were added
        assert!(router.shortcut_count > 0);
    }

    #[test]
    fn test_input_router_grab() {
        let mut router = InputRouter::new();

        // Acquire grab
        assert!(router.acquire_grab(1, GrabType::Keyboard, GrabReason::Modal));
        assert_eq!(router.keyboard_grab_owner(), 1);

        // Cannot acquire conflicting grab
        assert!(!router.acquire_grab(2, GrabType::Keyboard, GrabReason::Modal));

        // Release
        router.release_grabs_for(1);
        assert_eq!(router.keyboard_grab_owner(), WINDOW_ID_NONE);
    }

    #[test]
    fn test_input_router_focus() {
        let mut router = InputRouter::new();

        router.focus_window(1);
        router.focus_window(2);

        assert_eq!(router.focused_window(), 2);

        router.remove_window(2);
        assert_eq!(router.focused_window(), 1);
    }

    #[test]
    fn test_input_event_queue() {
        let mut router = InputRouter::new();

        let event = InputEvent::KeyDown {
            scancode: 32,
            keycode: KeyCode::D,
            modifiers: Modifiers::none(),
        };
        let routed = RoutedEvent::new(event, InputTarget::System, 1);

        assert!(router.enqueue(routed));
        assert_eq!(router.queue_len(), 1);

        let dequeued = router.dequeue();
        assert!(dequeued.is_some());
        assert_eq!(router.queue_len(), 0);
    }

    #[test]
    fn test_input_filter_hit_test() {
        let filter = InputFilter {
            bounds: (100, 100, 200, 150),
            visible: true,
            ..Default::default()
        };

        assert!(filter.hit_test(150, 150));
        assert!(filter.hit_test(100, 100));
        assert!(!filter.hit_test(50, 50));
        assert!(!filter.hit_test(350, 150));
    }

    #[test]
    fn test_vm_input_injector() {
        let mut injector = VmInputInjector::new();

        injector.activate(1, 10, 20);
        assert!(injector.is_active());

        let (sx, sy) = injector.translate_position(110, 120);
        assert_eq!(sx, 100);
        assert_eq!(sy, 100);

        let key_event = injector.inject_key(32, true);
        assert!(key_event.is_some());

        injector.deactivate();
        assert!(!injector.is_active());
    }

    #[test]
    fn test_input_target() {
        let target = InputTarget::Window(42);
        assert_eq!(target.window_id(), Some(42));
        assert_eq!(target.surface_id(), None);
        assert!(!target.is_none());

        let target2 = InputTarget::None;
        assert!(target2.is_none());
    }
}
