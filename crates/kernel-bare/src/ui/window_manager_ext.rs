//! Window Manager Extensions for RayOS UI
//!
//! Extends the base window manager with advanced features:
//! - Tiling and snap zones
//! - Maximize/restore with geometry preservation
//! - Virtual workspaces
//! - Animated transitions
//!
//! # Markers
//!
//! - `RAYOS_WINDOW:SNAPPED` - Window snapped to zone
//! - `RAYOS_WINDOW:MAXIMIZED` - Window maximized
//! - `RAYOS_WINDOW:TILED` - Window tiled
//! - `RAYOS_WINDOW:WORKSPACE` - Workspace switched
//! - `RAYOS_WINDOW:RESTORED` - Window restored from maximized/minimized

use super::window_manager::{WindowId, WINDOW_ID_NONE};

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of workspaces.
pub const MAX_WORKSPACES: usize = 4;

/// Maximum windows per workspace.
pub const MAX_WINDOWS_PER_WORKSPACE: usize = 16;

/// Snap zone detection threshold in pixels.
pub const SNAP_THRESHOLD: i32 = 20;

/// Edge threshold for resize grips.
pub const RESIZE_GRIP_SIZE: i32 = 8;

// ============================================================================
// Window Layout
// ============================================================================

/// Layout mode for a window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum WindowLayout {
    /// Freely positioned and sized.
    Floating = 0,
    /// Tiled to left half of screen.
    TiledLeft = 1,
    /// Tiled to right half of screen.
    TiledRight = 2,
    /// Tiled to top half of screen.
    TiledTop = 3,
    /// Tiled to bottom half of screen.
    TiledBottom = 4,
    /// Maximized (fills screen minus taskbar).
    Maximized = 5,
    /// Fullscreen (fills entire screen).
    Fullscreen = 6,
}

impl Default for WindowLayout {
    fn default() -> Self {
        WindowLayout::Floating
    }
}

impl WindowLayout {
    /// Returns true if layout is a tiled mode.
    pub fn is_tiled(self) -> bool {
        matches!(self, 
            WindowLayout::TiledLeft | WindowLayout::TiledRight |
            WindowLayout::TiledTop | WindowLayout::TiledBottom)
    }

    /// Returns true if layout fills the screen.
    pub fn is_fullsize(self) -> bool {
        matches!(self, WindowLayout::Maximized | WindowLayout::Fullscreen)
    }

    /// Returns true if window can be moved freely.
    pub fn is_movable(self) -> bool {
        self == WindowLayout::Floating
    }

    /// Returns true if window can be resized freely.
    pub fn is_resizable(self) -> bool {
        self == WindowLayout::Floating
    }
}

// ============================================================================
// Snap Zone
// ============================================================================

/// Screen zones for window snapping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SnapZone {
    /// No snap zone.
    None = 0,
    /// Left half of screen.
    Left = 1,
    /// Right half of screen.
    Right = 2,
    /// Top half of screen.
    Top = 3,
    /// Bottom half of screen.
    Bottom = 4,
    /// Top-left quarter.
    TopLeft = 5,
    /// Top-right quarter.
    TopRight = 6,
    /// Bottom-left quarter.
    BottomLeft = 7,
    /// Bottom-right quarter.
    BottomRight = 8,
    /// Center (maximize).
    Center = 9,
}

impl Default for SnapZone {
    fn default() -> Self {
        SnapZone::None
    }
}

impl SnapZone {
    /// Detect snap zone from mouse position and screen dimensions.
    pub fn detect(mouse_x: i32, mouse_y: i32, screen_w: u32, screen_h: u32) -> Self {
        let sw = screen_w as i32;
        let sh = screen_h as i32;
        let threshold = SNAP_THRESHOLD;

        // Edge detection
        let at_left = mouse_x <= threshold;
        let at_right = mouse_x >= sw - threshold;
        let at_top = mouse_y <= threshold;
        let at_bottom = mouse_y >= sh - threshold;

        // Corner priority
        if at_top && at_left {
            SnapZone::TopLeft
        } else if at_top && at_right {
            SnapZone::TopRight
        } else if at_bottom && at_left {
            SnapZone::BottomLeft
        } else if at_bottom && at_right {
            SnapZone::BottomRight
        } else if at_left {
            SnapZone::Left
        } else if at_right {
            SnapZone::Right
        } else if at_top {
            SnapZone::Center // Top edge = maximize
        } else if at_bottom {
            SnapZone::Bottom
        } else {
            SnapZone::None
        }
    }

    /// Calculate window geometry for this snap zone.
    pub fn geometry(self, screen_w: u32, screen_h: u32, taskbar_h: u32) -> (i32, i32, u32, u32) {
        let sw = screen_w as i32;
        let sh = (screen_h - taskbar_h) as i32;
        let half_w = screen_w / 2;
        let half_h = (screen_h - taskbar_h) / 2;

        match self {
            SnapZone::None => (0, 0, 400, 300), // Default size
            SnapZone::Left => (0, 0, half_w, screen_h - taskbar_h),
            SnapZone::Right => (sw / 2, 0, half_w, screen_h - taskbar_h),
            SnapZone::Top => (0, 0, screen_w, half_h),
            SnapZone::Bottom => (0, sh / 2, screen_w, half_h),
            SnapZone::TopLeft => (0, 0, half_w, half_h),
            SnapZone::TopRight => (sw / 2, 0, half_w, half_h),
            SnapZone::BottomLeft => (0, sh / 2, half_w, half_h),
            SnapZone::BottomRight => (sw / 2, sh / 2, half_w, half_h),
            SnapZone::Center => (0, 0, screen_w, screen_h - taskbar_h), // Maximized
        }
    }

    /// Convert to window layout.
    pub fn to_layout(self) -> WindowLayout {
        match self {
            SnapZone::None => WindowLayout::Floating,
            SnapZone::Left => WindowLayout::TiledLeft,
            SnapZone::Right => WindowLayout::TiledRight,
            SnapZone::Top => WindowLayout::TiledTop,
            SnapZone::Bottom => WindowLayout::TiledBottom,
            SnapZone::TopLeft | SnapZone::TopRight |
            SnapZone::BottomLeft | SnapZone::BottomRight => WindowLayout::Floating, // Quarter tiles are floating with fixed pos
            SnapZone::Center => WindowLayout::Maximized,
        }
    }
}

// ============================================================================
// Window Constraints
// ============================================================================

/// Size and behavior constraints for a window.
#[derive(Clone, Copy, Default)]
pub struct WindowConstraints {
    /// Minimum width in pixels.
    pub min_width: u32,
    /// Minimum height in pixels.
    pub min_height: u32,
    /// Maximum width (0 = no limit).
    pub max_width: u32,
    /// Maximum height (0 = no limit).
    pub max_height: u32,
    /// Fixed aspect ratio (width/height * 1000, 0 = none).
    pub aspect_ratio: u32,
    /// Whether window can be resized.
    pub resizable: bool,
    /// Whether window can be moved.
    pub movable: bool,
    /// Whether window can be minimized.
    pub minimizable: bool,
    /// Whether window can be maximized.
    pub maximizable: bool,
    /// Whether window can be closed.
    pub closable: bool,
}

impl WindowConstraints {
    /// Create default constraints.
    pub const fn new() -> Self {
        Self {
            min_width: 100,
            min_height: 50,
            max_width: 0,
            max_height: 0,
            aspect_ratio: 0,
            resizable: true,
            movable: true,
            minimizable: true,
            maximizable: true,
            closable: true,
        }
    }

    /// Create dialog constraints (centered, not maximizable).
    pub const fn dialog() -> Self {
        Self {
            min_width: 200,
            min_height: 100,
            max_width: 0,
            max_height: 0,
            aspect_ratio: 0,
            resizable: true,
            movable: true,
            minimizable: false,
            maximizable: false,
            closable: true,
        }
    }

    /// Create fixed-size constraints.
    pub const fn fixed(width: u32, height: u32) -> Self {
        Self {
            min_width: width,
            min_height: height,
            max_width: width,
            max_height: height,
            aspect_ratio: 0,
            resizable: false,
            movable: true,
            minimizable: true,
            maximizable: false,
            closable: true,
        }
    }

    /// Clamp dimensions to constraints.
    pub fn clamp(&self, width: u32, height: u32) -> (u32, u32) {
        let mut w = width.max(self.min_width);
        let mut h = height.max(self.min_height);

        if self.max_width > 0 {
            w = w.min(self.max_width);
        }
        if self.max_height > 0 {
            h = h.min(self.max_height);
        }

        // Apply aspect ratio if set
        if self.aspect_ratio > 0 {
            let target_w = (h as u64 * self.aspect_ratio as u64 / 1000) as u32;
            if target_w >= self.min_width && (self.max_width == 0 || target_w <= self.max_width) {
                w = target_w;
            }
        }

        (w, h)
    }
}

// ============================================================================
// Saved Geometry
// ============================================================================

/// Saved window geometry for restore operations.
#[derive(Clone, Copy, Default)]
pub struct SavedGeometry {
    /// X position.
    pub x: i32,
    /// Y position.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Whether this geometry is valid.
    pub valid: bool,
}

impl SavedGeometry {
    /// Create from current window position/size.
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height, valid: true }
    }

    /// Create invalid/empty geometry.
    pub const fn empty() -> Self {
        Self { x: 0, y: 0, width: 0, height: 0, valid: false }
    }

    /// Save current geometry.
    pub fn save(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.x = x;
        self.y = y;
        self.width = width;
        self.height = height;
        self.valid = true;
    }

    /// Invalidate saved geometry.
    pub fn clear(&mut self) {
        self.valid = false;
    }
}

// ============================================================================
// Window State Extension
// ============================================================================

/// Extended state for a window.
#[derive(Clone, Copy, Default)]
pub struct WindowStateExt {
    /// Window ID this state belongs to.
    pub window_id: WindowId,
    /// Current layout mode.
    pub layout: WindowLayout,
    /// Saved geometry for restore.
    pub saved_geometry: SavedGeometry,
    /// Window constraints.
    pub constraints: WindowConstraints,
    /// Assigned workspace (0-based index).
    pub workspace: u8,
    /// Whether window is minimized.
    pub minimized: bool,
    /// Whether window is shaded (title bar only).
    pub shaded: bool,
    /// Whether window is always on top.
    pub always_on_top: bool,
    /// Whether window is sticky (visible on all workspaces).
    pub sticky: bool,
    /// Animation progress (0-1000, 0 = no animation).
    pub anim_progress: u16,
    /// Animation type being played.
    pub anim_type: AnimationType,
}

/// Animation types for window transitions.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum AnimationType {
    #[default]
    None = 0,
    Snap = 1,
    Maximize = 2,
    Restore = 3,
    Minimize = 4,
    Unminimize = 5,
}

impl WindowStateExt {
    /// Create new extended state for a window.
    pub const fn new(window_id: WindowId) -> Self {
        Self {
            window_id,
            layout: WindowLayout::Floating,
            saved_geometry: SavedGeometry::empty(),
            constraints: WindowConstraints::new(),
            workspace: 0,
            minimized: false,
            shaded: false,
            always_on_top: false,
            sticky: false,
            anim_progress: 0,
            anim_type: AnimationType::None,
        }
    }

    /// Returns true if window is visible on given workspace.
    pub fn visible_on_workspace(&self, workspace: u8) -> bool {
        self.sticky || self.workspace == workspace
    }

    /// Save current geometry before layout change.
    pub fn save_geometry(&mut self, x: i32, y: i32, width: u32, height: u32) {
        if self.layout == WindowLayout::Floating && !self.saved_geometry.valid {
            self.saved_geometry.save(x, y, width, height);
        }
    }

    /// Get restore geometry (or default if none saved).
    pub fn restore_geometry(&self) -> (i32, i32, u32, u32) {
        if self.saved_geometry.valid {
            (self.saved_geometry.x, self.saved_geometry.y,
             self.saved_geometry.width, self.saved_geometry.height)
        } else {
            (100, 100, 400, 300)
        }
    }

    /// Start an animation.
    pub fn start_animation(&mut self, anim_type: AnimationType) {
        self.anim_type = anim_type;
        self.anim_progress = 0;
    }

    /// Advance animation. Returns true if still animating.
    pub fn tick_animation(&mut self, delta: u16) -> bool {
        if self.anim_type == AnimationType::None {
            return false;
        }

        self.anim_progress = self.anim_progress.saturating_add(delta);
        if self.anim_progress >= 1000 {
            self.anim_progress = 0;
            self.anim_type = AnimationType::None;
            false
        } else {
            true
        }
    }
}

// ============================================================================
// Workspace
// ============================================================================

/// A virtual workspace containing windows.
#[derive(Clone)]
pub struct Workspace {
    /// Workspace index (0-based).
    pub index: u8,
    /// Workspace name.
    pub name: [u8; 32],
    /// Name length.
    pub name_len: usize,
    /// Window IDs in this workspace (z-order, bottom to top).
    pub windows: [WindowId; MAX_WINDOWS_PER_WORKSPACE],
    /// Number of windows.
    pub window_count: usize,
    /// Currently focused window in this workspace.
    pub focused_window: WindowId,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            index: 0,
            name: [0u8; 32],
            name_len: 0,
            windows: [WINDOW_ID_NONE; MAX_WINDOWS_PER_WORKSPACE],
            window_count: 0,
            focused_window: WINDOW_ID_NONE,
        }
    }
}

impl Workspace {
    /// Create a new workspace.
    pub fn new(index: u8, name: &[u8]) -> Self {
        let mut ws = Self {
            index,
            name: [0u8; 32],
            name_len: 0,
            windows: [WINDOW_ID_NONE; MAX_WINDOWS_PER_WORKSPACE],
            window_count: 0,
            focused_window: WINDOW_ID_NONE,
        };
        let len = name.len().min(32);
        ws.name[..len].copy_from_slice(&name[..len]);
        ws.name_len = len;
        ws
    }

    /// Add a window to this workspace.
    pub fn add_window(&mut self, window_id: WindowId) -> bool {
        if self.window_count >= MAX_WINDOWS_PER_WORKSPACE {
            return false;
        }
        // Check for duplicate
        for i in 0..self.window_count {
            if self.windows[i] == window_id {
                return true; // Already present
            }
        }
        self.windows[self.window_count] = window_id;
        self.window_count += 1;
        true
    }

    /// Remove a window from this workspace.
    pub fn remove_window(&mut self, window_id: WindowId) -> bool {
        for i in 0..self.window_count {
            if self.windows[i] == window_id {
                // Shift remaining windows down
                for j in i..self.window_count - 1 {
                    self.windows[j] = self.windows[j + 1];
                }
                self.windows[self.window_count - 1] = WINDOW_ID_NONE;
                self.window_count -= 1;

                // Update focus if needed
                if self.focused_window == window_id {
                    self.focused_window = if self.window_count > 0 {
                        self.windows[self.window_count - 1]
                    } else {
                        WINDOW_ID_NONE
                    };
                }
                return true;
            }
        }
        false
    }

    /// Raise a window to the top of z-order.
    pub fn raise_window(&mut self, window_id: WindowId) {
        let mut found_idx = None;
        for i in 0..self.window_count {
            if self.windows[i] == window_id {
                found_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = found_idx {
            // Shift windows up and put this one at the end
            for i in idx..self.window_count - 1 {
                self.windows[i] = self.windows[i + 1];
            }
            self.windows[self.window_count - 1] = window_id;
            self.focused_window = window_id;
        }
    }

    /// Check if window is in this workspace.
    pub fn contains(&self, window_id: WindowId) -> bool {
        for i in 0..self.window_count {
            if self.windows[i] == window_id {
                return true;
            }
        }
        false
    }

    /// Get windows in z-order (bottom to top).
    pub fn windows_iter(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.windows[..self.window_count].iter().copied()
    }
}

// ============================================================================
// Workspace Manager
// ============================================================================

/// Manages virtual workspaces.
pub struct WorkspaceManager {
    /// All workspaces.
    workspaces: [Workspace; MAX_WORKSPACES],
    /// Currently active workspace index.
    active_workspace: u8,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceManager {
    /// Create a new workspace manager with default workspaces.
    pub fn new() -> Self {
        let mut mgr = Self {
            workspaces: [
                Workspace::new(0, b"Desktop 1"),
                Workspace::new(1, b"Desktop 2"),
                Workspace::new(2, b"Desktop 3"),
                Workspace::new(3, b"Desktop 4"),
            ],
            active_workspace: 0,
        };
        // Set indices properly
        for i in 0..MAX_WORKSPACES {
            mgr.workspaces[i].index = i as u8;
        }
        mgr
    }

    /// Get active workspace index.
    pub fn active(&self) -> u8 {
        self.active_workspace
    }

    /// Get active workspace.
    pub fn active_workspace(&self) -> &Workspace {
        &self.workspaces[self.active_workspace as usize]
    }

    /// Get active workspace (mutable).
    pub fn active_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_workspace as usize]
    }

    /// Get workspace by index.
    pub fn workspace(&self, index: u8) -> Option<&Workspace> {
        if (index as usize) < MAX_WORKSPACES {
            Some(&self.workspaces[index as usize])
        } else {
            None
        }
    }

    /// Get workspace by index (mutable).
    pub fn workspace_mut(&mut self, index: u8) -> Option<&mut Workspace> {
        if (index as usize) < MAX_WORKSPACES {
            Some(&mut self.workspaces[index as usize])
        } else {
            None
        }
    }

    /// Switch to a workspace.
    pub fn switch_to(&mut self, index: u8) -> bool {
        if (index as usize) < MAX_WORKSPACES && index != self.active_workspace {
            self.active_workspace = index;
            
            // Emit marker
            #[cfg(feature = "window_markers")]
            crate::serial_println!("RAYOS_WINDOW:WORKSPACE active={}", index);
            
            true
        } else {
            false
        }
    }

    /// Switch to next workspace.
    pub fn switch_next(&mut self) {
        let next = (self.active_workspace + 1) % MAX_WORKSPACES as u8;
        self.switch_to(next);
    }

    /// Switch to previous workspace.
    pub fn switch_prev(&mut self) {
        let prev = if self.active_workspace == 0 {
            MAX_WORKSPACES as u8 - 1
        } else {
            self.active_workspace - 1
        };
        self.switch_to(prev);
    }

    /// Add window to a workspace.
    pub fn add_window(&mut self, workspace: u8, window_id: WindowId) -> bool {
        if let Some(ws) = self.workspace_mut(workspace) {
            ws.add_window(window_id)
        } else {
            false
        }
    }

    /// Remove window from a workspace.
    pub fn remove_window(&mut self, workspace: u8, window_id: WindowId) -> bool {
        if let Some(ws) = self.workspace_mut(workspace) {
            ws.remove_window(window_id)
        } else {
            false
        }
    }

    /// Move window to a different workspace.
    pub fn move_window(&mut self, window_id: WindowId, from: u8, to: u8) -> bool {
        if from == to {
            return true;
        }
        if self.remove_window(from, window_id) {
            self.add_window(to, window_id)
        } else {
            false
        }
    }

    /// Find which workspace contains a window.
    pub fn find_window(&self, window_id: WindowId) -> Option<u8> {
        for ws in &self.workspaces {
            if ws.contains(window_id) {
                return Some(ws.index);
            }
        }
        None
    }
}

// ============================================================================
// Window Snapper
// ============================================================================

/// Handles window snap-to-edge and snap-to-zone logic.
pub struct WindowSnapper {
    /// Screen width.
    screen_width: u32,
    /// Screen height.
    screen_height: u32,
    /// Taskbar height.
    taskbar_height: u32,
    /// Currently detected snap zone during drag.
    preview_zone: SnapZone,
    /// Whether snap preview is active.
    preview_active: bool,
}

impl Default for WindowSnapper {
    fn default() -> Self {
        Self::new(1024, 768, 32)
    }
}

impl WindowSnapper {
    /// Create a new snapper.
    pub const fn new(screen_width: u32, screen_height: u32, taskbar_height: u32) -> Self {
        Self {
            screen_width,
            screen_height,
            taskbar_height,
            preview_zone: SnapZone::None,
            preview_active: false,
        }
    }

    /// Update screen dimensions.
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Update taskbar height.
    pub fn set_taskbar_height(&mut self, height: u32) {
        self.taskbar_height = height;
    }

    /// Begin drag operation.
    pub fn begin_drag(&mut self) {
        self.preview_zone = SnapZone::None;
        self.preview_active = false;
    }

    /// Update during drag. Returns snap zone if changed.
    pub fn update_drag(&mut self, mouse_x: i32, mouse_y: i32) -> Option<SnapZone> {
        let zone = SnapZone::detect(mouse_x, mouse_y, self.screen_width, self.screen_height);
        
        if zone != self.preview_zone {
            self.preview_zone = zone;
            self.preview_active = zone != SnapZone::None;
            Some(zone)
        } else {
            None
        }
    }

    /// End drag operation. Returns final snap zone if any.
    pub fn end_drag(&mut self) -> SnapZone {
        let zone = self.preview_zone;
        self.preview_zone = SnapZone::None;
        self.preview_active = false;
        zone
    }

    /// Get geometry for current preview zone.
    pub fn preview_geometry(&self) -> Option<(i32, i32, u32, u32)> {
        if self.preview_active && self.preview_zone != SnapZone::None {
            Some(self.preview_zone.geometry(self.screen_width, self.screen_height, self.taskbar_height))
        } else {
            None
        }
    }

    /// Get geometry for a specific zone.
    pub fn zone_geometry(&self, zone: SnapZone) -> (i32, i32, u32, u32) {
        zone.geometry(self.screen_width, self.screen_height, self.taskbar_height)
    }

    /// Calculate snapped geometry for a window.
    pub fn snap(&self, zone: SnapZone) -> (i32, i32, u32, u32, WindowLayout) {
        let geom = zone.geometry(self.screen_width, self.screen_height, self.taskbar_height);
        (geom.0, geom.1, geom.2, geom.3, zone.to_layout())
    }
}

// ============================================================================
// Extended State Registry
// ============================================================================

/// Registry of extended window states.
pub struct WindowStateRegistry {
    /// Extended states indexed by slot.
    states: [WindowStateExt; MAX_WINDOWS_PER_WORKSPACE],
    /// Number of active states.
    count: usize,
}

impl Default for WindowStateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowStateRegistry {
    /// Create new registry.
    pub const fn new() -> Self {
        Self {
            states: [WindowStateExt {
                window_id: WINDOW_ID_NONE,
                layout: WindowLayout::Floating,
                saved_geometry: SavedGeometry { x: 0, y: 0, width: 0, height: 0, valid: false },
                constraints: WindowConstraints {
                    min_width: 100, min_height: 50, max_width: 0, max_height: 0,
                    aspect_ratio: 0, resizable: true, movable: true,
                    minimizable: true, maximizable: true, closable: true,
                },
                workspace: 0,
                minimized: false,
                shaded: false,
                always_on_top: false,
                sticky: false,
                anim_progress: 0,
                anim_type: AnimationType::None,
            }; MAX_WINDOWS_PER_WORKSPACE],
            count: 0,
        }
    }

    /// Register a window.
    pub fn register(&mut self, window_id: WindowId) -> bool {
        // Check if already registered
        for state in &self.states[..self.count] {
            if state.window_id == window_id {
                return true;
            }
        }

        if self.count >= MAX_WINDOWS_PER_WORKSPACE {
            return false;
        }

        self.states[self.count] = WindowStateExt::new(window_id);
        self.count += 1;
        true
    }

    /// Unregister a window.
    pub fn unregister(&mut self, window_id: WindowId) -> bool {
        for i in 0..self.count {
            if self.states[i].window_id == window_id {
                // Shift remaining states
                for j in i..self.count - 1 {
                    self.states[j] = self.states[j + 1];
                }
                self.states[self.count - 1] = WindowStateExt::new(WINDOW_ID_NONE);
                self.count -= 1;
                return true;
            }
        }
        false
    }

    /// Get state for a window.
    pub fn get(&self, window_id: WindowId) -> Option<&WindowStateExt> {
        for state in &self.states[..self.count] {
            if state.window_id == window_id {
                return Some(state);
            }
        }
        None
    }

    /// Get state for a window (mutable).
    pub fn get_mut(&mut self, window_id: WindowId) -> Option<&mut WindowStateExt> {
        for state in &mut self.states[..self.count] {
            if state.window_id == window_id {
                return Some(state);
            }
        }
        None
    }

    /// Iterate over all states.
    pub fn iter(&self) -> impl Iterator<Item = &WindowStateExt> {
        self.states[..self.count].iter()
    }
}

// ============================================================================
// Window Manager Extension
// ============================================================================

/// Extended window manager functionality.
pub struct WindowManagerExt {
    /// Workspace manager.
    pub workspaces: WorkspaceManager,
    /// Window snapper.
    pub snapper: WindowSnapper,
    /// Extended state registry.
    pub states: WindowStateRegistry,
    /// Screen dimensions.
    screen_width: u32,
    screen_height: u32,
    /// Taskbar height.
    taskbar_height: u32,
}

impl Default for WindowManagerExt {
    fn default() -> Self {
        Self::new(1024, 768, 32)
    }
}

impl WindowManagerExt {
    /// Create new extended window manager.
    pub fn new(screen_width: u32, screen_height: u32, taskbar_height: u32) -> Self {
        Self {
            workspaces: WorkspaceManager::new(),
            snapper: WindowSnapper::new(screen_width, screen_height, taskbar_height),
            states: WindowStateRegistry::new(),
            screen_width,
            screen_height,
            taskbar_height,
        }
    }

    /// Update screen dimensions.
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
        self.snapper.set_screen_size(width, height);
    }

    /// Register a new window.
    pub fn register_window(&mut self, window_id: WindowId, workspace: u8) {
        self.states.register(window_id);
        let ws = workspace.min(MAX_WORKSPACES as u8 - 1);
        if let Some(state) = self.states.get_mut(window_id) {
            state.workspace = ws;
        }
        self.workspaces.add_window(ws, window_id);
    }

    /// Unregister a window.
    pub fn unregister_window(&mut self, window_id: WindowId) {
        if let Some(ws) = self.workspaces.find_window(window_id) {
            self.workspaces.remove_window(ws, window_id);
        }
        self.states.unregister(window_id);
    }

    /// Maximize a window.
    pub fn maximize(&mut self, window_id: WindowId, x: i32, y: i32, width: u32, height: u32) 
        -> Option<(i32, i32, u32, u32)> 
    {
        if let Some(state) = self.states.get_mut(window_id) {
            if state.layout == WindowLayout::Maximized {
                return None; // Already maximized
            }
            
            // Save current geometry
            state.save_geometry(x, y, width, height);
            state.layout = WindowLayout::Maximized;
            state.start_animation(AnimationType::Maximize);
            
            // Emit marker
            #[cfg(feature = "window_markers")]
            crate::serial_println!("RAYOS_WINDOW:MAXIMIZED id={}", window_id);
            
            Some((0, 0, self.screen_width, self.screen_height - self.taskbar_height))
        } else {
            None
        }
    }

    /// Restore a window from maximized/tiled state.
    pub fn restore(&mut self, window_id: WindowId) -> Option<(i32, i32, u32, u32)> {
        if let Some(state) = self.states.get_mut(window_id) {
            if state.layout == WindowLayout::Floating {
                return None; // Already floating
            }
            
            let geom = state.restore_geometry();
            state.layout = WindowLayout::Floating;
            state.saved_geometry.clear();
            state.start_animation(AnimationType::Restore);
            
            // Emit marker
            #[cfg(feature = "window_markers")]
            crate::serial_println!("RAYOS_WINDOW:RESTORED id={}", window_id);
            
            Some(geom)
        } else {
            None
        }
    }

    /// Toggle maximize/restore.
    pub fn toggle_maximize(&mut self, window_id: WindowId, x: i32, y: i32, width: u32, height: u32) 
        -> Option<(i32, i32, u32, u32)> 
    {
        let is_maximized = self.states.get(window_id)
            .map(|s| s.layout == WindowLayout::Maximized)
            .unwrap_or(false);
        
        if is_maximized {
            self.restore(window_id)
        } else {
            self.maximize(window_id, x, y, width, height)
        }
    }

    /// Snap a window to a zone.
    pub fn snap_to_zone(&mut self, window_id: WindowId, zone: SnapZone, 
                         x: i32, y: i32, width: u32, height: u32) 
        -> Option<(i32, i32, u32, u32)> 
    {
        if zone == SnapZone::None {
            return self.restore(window_id);
        }

        if let Some(state) = self.states.get_mut(window_id) {
            // Save geometry if coming from floating
            state.save_geometry(x, y, width, height);
            state.layout = zone.to_layout();
            state.start_animation(AnimationType::Snap);
            
            // Emit marker
            #[cfg(feature = "window_markers")]
            crate::serial_println!("RAYOS_WINDOW:SNAPPED id={} zone={:?}", window_id, zone);
            
            let geom = self.snapper.zone_geometry(zone);
            Some(geom)
        } else {
            None
        }
    }

    /// Tile windows left and right.
    pub fn tile_lr(&mut self, left_id: WindowId, right_id: WindowId,
                   left_x: i32, left_y: i32, left_w: u32, left_h: u32,
                   right_x: i32, right_y: i32, right_w: u32, right_h: u32)
        -> ((i32, i32, u32, u32), (i32, i32, u32, u32))
    {
        let left_geom = self.snap_to_zone(left_id, SnapZone::Left, left_x, left_y, left_w, left_h)
            .unwrap_or((0, 0, self.screen_width / 2, self.screen_height - self.taskbar_height));
        let right_geom = self.snap_to_zone(right_id, SnapZone::Right, right_x, right_y, right_w, right_h)
            .unwrap_or((self.screen_width as i32 / 2, 0, self.screen_width / 2, self.screen_height - self.taskbar_height));
        
        // Emit marker
        #[cfg(feature = "window_markers")]
        crate::serial_println!("RAYOS_WINDOW:TILED left={} right={}", left_id, right_id);
        
        (left_geom, right_geom)
    }

    /// Move window to another workspace.
    pub fn move_to_workspace(&mut self, window_id: WindowId, target_workspace: u8) -> bool {
        if let Some(state) = self.states.get_mut(window_id) {
            let current = state.workspace;
            if current == target_workspace {
                return true;
            }
            state.workspace = target_workspace;
            self.workspaces.move_window(window_id, current, target_workspace)
        } else {
            false
        }
    }

    /// Get windows visible on current workspace.
    pub fn visible_windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        let active = self.workspaces.active();
        self.workspaces.active_workspace().windows_iter().filter(move |&id| {
            self.states.get(id)
                .map(|s| s.visible_on_workspace(active) && !s.minimized)
                .unwrap_or(true)
        })
    }

    /// Tick animations for all windows.
    pub fn tick_animations(&mut self, delta: u16) {
        for state in self.states.states[..self.states.count].iter_mut() {
            state.tick_animation(delta);
        }
    }
}

// ============================================================================
// Global Instance
// ============================================================================

/// Global extended window manager.
static mut WINDOW_MANAGER_EXT: WindowManagerExt = WindowManagerExt {
    workspaces: WorkspaceManager {
        workspaces: [
            Workspace { index: 0, name: [0; 32], name_len: 0, windows: [0; MAX_WINDOWS_PER_WORKSPACE], window_count: 0, focused_window: 0 },
            Workspace { index: 1, name: [0; 32], name_len: 0, windows: [0; MAX_WINDOWS_PER_WORKSPACE], window_count: 0, focused_window: 0 },
            Workspace { index: 2, name: [0; 32], name_len: 0, windows: [0; MAX_WINDOWS_PER_WORKSPACE], window_count: 0, focused_window: 0 },
            Workspace { index: 3, name: [0; 32], name_len: 0, windows: [0; MAX_WINDOWS_PER_WORKSPACE], window_count: 0, focused_window: 0 },
        ],
        active_workspace: 0,
    },
    snapper: WindowSnapper {
        screen_width: 1024,
        screen_height: 768,
        taskbar_height: 32,
        preview_zone: SnapZone::None,
        preview_active: false,
    },
    states: WindowStateRegistry {
        states: [WindowStateExt {
            window_id: WINDOW_ID_NONE,
            layout: WindowLayout::Floating,
            saved_geometry: SavedGeometry { x: 0, y: 0, width: 0, height: 0, valid: false },
            constraints: WindowConstraints {
                min_width: 100, min_height: 50, max_width: 0, max_height: 0,
                aspect_ratio: 0, resizable: true, movable: true,
                minimizable: true, maximizable: true, closable: true,
            },
            workspace: 0,
            minimized: false,
            shaded: false,
            always_on_top: false,
            sticky: false,
            anim_progress: 0,
            anim_type: AnimationType::None,
        }; MAX_WINDOWS_PER_WORKSPACE],
        count: 0,
    },
    screen_width: 1024,
    screen_height: 768,
    taskbar_height: 32,
};

/// Get the global extended window manager.
pub fn get() -> &'static WindowManagerExt {
    // SAFETY: Single-threaded kernel context
    unsafe { &WINDOW_MANAGER_EXT }
}

/// Get the global extended window manager (mutable).
pub fn get_mut() -> &'static mut WindowManagerExt {
    // SAFETY: Single-threaded kernel context
    unsafe { &mut WINDOW_MANAGER_EXT }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Unit Tests ---

    #[test]
    fn test_window_layout_properties() {
        assert!(WindowLayout::TiledLeft.is_tiled());
        assert!(WindowLayout::TiledRight.is_tiled());
        assert!(!WindowLayout::Floating.is_tiled());
        assert!(WindowLayout::Maximized.is_fullsize());
        assert!(WindowLayout::Floating.is_movable());
        assert!(!WindowLayout::Maximized.is_movable());
    }

    #[test]
    fn test_snap_zone_detect_corners() {
        assert_eq!(SnapZone::detect(5, 5, 1024, 768), SnapZone::TopLeft);
        assert_eq!(SnapZone::detect(1020, 5, 1024, 768), SnapZone::TopRight);
        assert_eq!(SnapZone::detect(5, 760, 1024, 768), SnapZone::BottomLeft);
        assert_eq!(SnapZone::detect(1020, 760, 1024, 768), SnapZone::BottomRight);
    }

    #[test]
    fn test_snap_zone_detect_edges() {
        assert_eq!(SnapZone::detect(5, 400, 1024, 768), SnapZone::Left);
        assert_eq!(SnapZone::detect(1020, 400, 1024, 768), SnapZone::Right);
        assert_eq!(SnapZone::detect(500, 5, 1024, 768), SnapZone::Center);
    }

    #[test]
    fn test_snap_zone_geometry() {
        let (x, y, w, h) = SnapZone::Left.geometry(1024, 768, 32);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
        assert_eq!(w, 512);
        assert_eq!(h, 736);
    }

    #[test]
    fn test_window_constraints_clamp() {
        let constraints = WindowConstraints::new();
        let (w, h) = constraints.clamp(50, 30);
        assert_eq!(w, 100); // min_width
        assert_eq!(h, 50);  // min_height
    }

    #[test]
    fn test_saved_geometry() {
        let mut geom = SavedGeometry::empty();
        assert!(!geom.valid);
        geom.save(10, 20, 300, 200);
        assert!(geom.valid);
        assert_eq!(geom.x, 10);
    }

    #[test]
    fn test_workspace_add_remove() {
        let mut ws = Workspace::new(0, b"Test");
        assert!(ws.add_window(1));
        assert!(ws.add_window(2));
        assert_eq!(ws.window_count, 2);
        assert!(ws.remove_window(1));
        assert_eq!(ws.window_count, 1);
    }

    #[test]
    fn test_workspace_raise() {
        let mut ws = Workspace::new(0, b"Test");
        ws.add_window(1);
        ws.add_window(2);
        ws.add_window(3);
        ws.raise_window(1); // Move 1 to top
        assert_eq!(ws.windows[2], 1);
        assert_eq!(ws.focused_window, 1);
    }

    #[test]
    fn test_workspace_manager_switch() {
        let mut mgr = WorkspaceManager::new();
        assert_eq!(mgr.active(), 0);
        assert!(mgr.switch_to(2));
        assert_eq!(mgr.active(), 2);
    }

    #[test]
    fn test_window_snapper_drag() {
        let mut snapper = WindowSnapper::new(1024, 768, 32);
        snapper.begin_drag();
        let zone = snapper.update_drag(5, 400);
        assert_eq!(zone, Some(SnapZone::Left));
        let final_zone = snapper.end_drag();
        assert_eq!(final_zone, SnapZone::Left);
    }

    #[test]
    fn test_window_state_ext_animation() {
        let mut state = WindowStateExt::new(1);
        state.start_animation(AnimationType::Maximize);
        assert!(state.tick_animation(500));
        assert!(state.tick_animation(600)); // Completes at 1000+
        assert_eq!(state.anim_type, AnimationType::None);
    }

    #[test]
    fn test_state_registry() {
        let mut registry = WindowStateRegistry::new();
        assert!(registry.register(1));
        assert!(registry.register(2));
        assert!(registry.get(1).is_some());
        assert!(registry.unregister(1));
        assert!(registry.get(1).is_none());
    }

    // --- Scenario Tests ---

    #[test]
    fn scenario_maximize_restore() {
        let mut ext = WindowManagerExt::new(1024, 768, 32);
        ext.register_window(1, 0);
        
        // Maximize
        let geom = ext.maximize(1, 100, 100, 400, 300);
        assert!(geom.is_some());
        let (x, y, w, h) = geom.unwrap();
        assert_eq!(x, 0);
        assert_eq!(y, 0);
        assert_eq!(w, 1024);
        assert_eq!(h, 736);
        
        // Restore
        let geom = ext.restore(1);
        assert!(geom.is_some());
        let (x, y, w, h) = geom.unwrap();
        assert_eq!(x, 100);
        assert_eq!(y, 100);
    }

    #[test]
    fn scenario_snap_to_zone() {
        let mut ext = WindowManagerExt::new(1024, 768, 32);
        ext.register_window(1, 0);
        
        let geom = ext.snap_to_zone(1, SnapZone::Left, 100, 100, 400, 300);
        assert!(geom.is_some());
        let (x, y, w, h) = geom.unwrap();
        assert_eq!(x, 0);
        assert_eq!(w, 512);
    }

    #[test]
    fn scenario_workspace_switching() {
        let mut ext = WindowManagerExt::new(1024, 768, 32);
        ext.register_window(1, 0);
        ext.register_window(2, 1);
        
        assert_eq!(ext.workspaces.active(), 0);
        assert!(ext.workspaces.switch_to(1));
        assert_eq!(ext.workspaces.active(), 1);
        
        // Window 2 should be visible
        assert!(ext.workspaces.active_workspace().contains(2));
    }

    #[test]
    fn scenario_move_window_workspace() {
        let mut ext = WindowManagerExt::new(1024, 768, 32);
        ext.register_window(1, 0);
        
        assert!(ext.workspaces.workspace(0).unwrap().contains(1));
        assert!(ext.move_to_workspace(1, 2));
        assert!(!ext.workspaces.workspace(0).unwrap().contains(1));
        assert!(ext.workspaces.workspace(2).unwrap().contains(1));
    }

    #[test]
    fn scenario_tile_windows() {
        let mut ext = WindowManagerExt::new(1024, 768, 32);
        ext.register_window(1, 0);
        ext.register_window(2, 0);
        
        let (left, right) = ext.tile_lr(1, 2, 
            100, 100, 400, 300,
            500, 100, 400, 300);
        
        assert_eq!(left.0, 0); // x = 0
        assert_eq!(left.2, 512); // width = half
        assert_eq!(right.0, 512); // x = half
        assert_eq!(right.2, 512); // width = half
    }
}
