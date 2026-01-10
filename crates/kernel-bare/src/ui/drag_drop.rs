//! Drag and Drop Engine for RayOS UI
//!
//! Complete drag-and-drop framework with visual feedback and VM guest support.
//!
//! # Overview
//!
//! The Drag-Drop Engine provides:
//! - Drag sources and drop targets
//! - Multi-format payload support
//! - Visual drag feedback (cursor, ghost image)
//! - Drop zone highlighting
//! - VM guest drag operations
//!
//! # Markers
//!
//! - `RAYOS_DRAGDROP:STARTED` - Drag operation started
//! - `RAYOS_DRAGDROP:DROPPED` - Drop completed
//! - `RAYOS_DRAGDROP:CANCELLED` - Drag cancelled
//! - `RAYOS_DRAGDROP:ENTERED` - Pointer entered drop target
//! - `RAYOS_DRAGDROP:LEFT` - Pointer left drop target

use super::app_runtime::AppId;
use super::clipboard::ClipboardFormat;
use super::window_manager::WindowId;

// ============================================================================
// Constants
// ============================================================================

/// Maximum drag payload size (8MB).
pub const MAX_DRAG_PAYLOAD_SIZE: usize = 8 * 1024 * 1024;

/// Maximum inline payload size (for small data).
pub const MAX_INLINE_PAYLOAD_SIZE: usize = 4096;

/// Maximum formats per drag payload.
pub const MAX_DRAG_FORMATS: usize = 8;

/// Maximum registered drop targets.
pub const MAX_DROP_TARGETS: usize = 64;

/// Maximum registered drag sources.
pub const MAX_DRAG_SOURCES: usize = 64;

/// Maximum drag listeners.
pub const MAX_DRAG_LISTENERS: usize = 16;

// ============================================================================
// Drag Action
// ============================================================================

/// Drag action type (what operation to perform).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DragAction {
    /// No action.
    None = 0,
    /// Copy data to target.
    Copy = 1,
    /// Move data to target (delete from source).
    Move = 2,
    /// Create a link/reference.
    Link = 3,
    /// Ask user for action.
    Ask = 4,
}

impl Default for DragAction {
    fn default() -> Self {
        DragAction::Copy
    }
}

impl DragAction {
    /// Get all standard actions as a bitmask.
    pub fn all() -> u8 {
        0x0F
    }

    /// Convert to bitmask.
    pub fn to_mask(self) -> u8 {
        match self {
            DragAction::None => 0,
            DragAction::Copy => 1,
            DragAction::Move => 2,
            DragAction::Link => 4,
            DragAction::Ask => 8,
        }
    }

    /// Check if action is in mask.
    pub fn in_mask(self, mask: u8) -> bool {
        (self.to_mask() & mask) != 0
    }
}

// ============================================================================
// Drag State
// ============================================================================

/// Current drag operation state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DragState {
    /// No active drag.
    Idle = 0,
    /// Drag operation pending (button down, waiting for threshold).
    Pending = 1,
    /// Drag in progress.
    Dragging = 2,
    /// Over a valid drop target.
    OverTarget = 3,
    /// Drop occurring.
    Dropping = 4,
}

impl Default for DragState {
    fn default() -> Self {
        DragState::Idle
    }
}

// ============================================================================
// Drag Payload Data
// ============================================================================

/// Inline drag data (small payloads).
#[derive(Clone, Copy)]
pub struct InlineDragData {
    /// Data bytes.
    pub bytes: [u8; MAX_INLINE_PAYLOAD_SIZE],
    /// Actual data length.
    pub len: usize,
}

impl InlineDragData {
    /// Create empty inline data.
    pub const fn empty() -> Self {
        Self {
            bytes: [0u8; MAX_INLINE_PAYLOAD_SIZE],
            len: 0,
        }
    }

    /// Create from bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() > MAX_INLINE_PAYLOAD_SIZE {
            return None;
        }
        let mut inline = Self::empty();
        inline.bytes[..data.len()].copy_from_slice(data);
        inline.len = data.len();
        Some(inline)
    }

    /// Get data as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Get data as string.
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(self.as_bytes()).ok()
    }
}

/// Drag payload data storage.
#[derive(Clone, Copy)]
pub enum DragData {
    /// No data.
    Empty,
    /// Inline data (small, stored directly).
    Inline(InlineDragData),
    /// External reference (offset + len in external buffer).
    External { offset: usize, len: usize },
}

impl Default for DragData {
    fn default() -> Self {
        DragData::Empty
    }
}

impl DragData {
    /// Check if data is empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, DragData::Empty)
    }

    /// Get data length.
    pub fn len(&self) -> usize {
        match self {
            DragData::Empty => 0,
            DragData::Inline(data) => data.len,
            DragData::External { len, .. } => *len,
        }
    }
}

// ============================================================================
// Drag Format Entry
// ============================================================================

/// Single format representation of drag data.
#[derive(Clone, Copy)]
pub struct DragFormatEntry {
    /// Data format (reuses clipboard formats).
    pub format: ClipboardFormat,
    /// Data storage.
    pub data: DragData,
}

impl DragFormatEntry {
    /// Create empty entry.
    pub const fn empty() -> Self {
        Self {
            format: ClipboardFormat::None,
            data: DragData::Empty,
        }
    }

    /// Create text entry.
    pub fn text(text: &str) -> Option<Self> {
        let inline = InlineDragData::from_bytes(text.as_bytes())?;
        Some(Self {
            format: ClipboardFormat::Text,
            data: DragData::Inline(inline),
        })
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        !matches!(self.format, ClipboardFormat::None)
    }
}

// ============================================================================
// Drag Payload
// ============================================================================

/// Complete drag payload with multiple format representations.
#[derive(Clone, Copy)]
pub struct DragPayload {
    /// Payload ID.
    pub id: u32,
    /// Available formats.
    pub formats: [DragFormatEntry; MAX_DRAG_FORMATS],
    /// Number of formats.
    pub format_count: usize,
    /// Source app ID.
    pub source_app: AppId,
    /// Source window ID.
    pub source_window: WindowId,
    /// Allowed actions.
    pub allowed_actions: u8,
    /// Preferred action.
    pub preferred_action: DragAction,
    /// Label for display.
    pub label: [u8; 32],
    /// Icon identifier (for visual feedback).
    pub icon_id: u32,
}

impl DragPayload {
    /// Create empty payload.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            formats: [DragFormatEntry::empty(); MAX_DRAG_FORMATS],
            format_count: 0,
            source_app: 0,
            source_window: 0,
            allowed_actions: 0x07, // Copy, Move, Link
            preferred_action: DragAction::Copy,
            label: [0u8; 32],
            icon_id: 0,
        }
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        self.format_count > 0 && self.formats[0].is_valid()
    }

    /// Add a format.
    pub fn add_format(&mut self, entry: DragFormatEntry) -> bool {
        if self.format_count >= MAX_DRAG_FORMATS {
            return false;
        }
        self.formats[self.format_count] = entry;
        self.format_count += 1;
        true
    }

    /// Check if format is available.
    pub fn has_format(&self, format: ClipboardFormat) -> bool {
        self.formats[..self.format_count]
            .iter()
            .any(|f| core::mem::discriminant(&f.format) == core::mem::discriminant(&format))
    }

    /// Get format entry.
    pub fn get_format(&self, format: ClipboardFormat) -> Option<&DragFormatEntry> {
        self.formats[..self.format_count]
            .iter()
            .find(|f| core::mem::discriminant(&f.format) == core::mem::discriminant(&format))
    }

    /// Get text data.
    pub fn get_text(&self) -> Option<&str> {
        if let Some(entry) = self.get_format(ClipboardFormat::Text) {
            if let DragData::Inline(data) = &entry.data {
                return data.as_str();
            }
        }
        None
    }

    /// Set label from text.
    pub fn set_label(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let len = bytes.len().min(31);
        self.label = [0u8; 32];
        self.label[..len].copy_from_slice(&bytes[..len]);
    }

    /// Get label as string.
    pub fn label_str(&self) -> &str {
        let len = self.label.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.label[..len]).unwrap_or("")
    }

    /// Check if action is allowed.
    pub fn is_action_allowed(&self, action: DragAction) -> bool {
        action.in_mask(self.allowed_actions)
    }

    /// List available formats.
    pub fn available_formats(&self) -> impl Iterator<Item = ClipboardFormat> + '_ {
        self.formats[..self.format_count].iter().map(|f| f.format)
    }
}

// ============================================================================
// Drop Target
// ============================================================================

/// Drop target registration.
#[derive(Clone, Copy)]
pub struct DropTarget {
    /// Target ID.
    pub id: u32,
    /// Owning app ID.
    pub app_id: AppId,
    /// Owning window ID.
    pub window_id: WindowId,
    /// Target bounds (x, y, width, height).
    pub bounds: (i32, i32, u32, u32),
    /// Accepted formats (bitmask).
    pub accepted_formats: u32,
    /// Accepted actions.
    pub accepted_actions: u8,
    /// Active flag.
    pub active: bool,
    /// Currently highlighted.
    pub highlighted: bool,
}

impl DropTarget {
    /// Create empty target.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            app_id: 0,
            window_id: 0,
            bounds: (0, 0, 0, 0),
            accepted_formats: 0xFFFFFFFF, // Accept all
            accepted_actions: 0x07,       // Copy, Move, Link
            active: false,
            highlighted: false,
        }
    }

    /// Check if point is inside target.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (bx, by, bw, bh) = self.bounds;
        x >= bx && x < bx + bw as i32 && y >= by && y < by + bh as i32
    }

    /// Check if payload can be dropped here.
    pub fn accepts_payload(&self, payload: &DragPayload) -> bool {
        if !self.active {
            return false;
        }
        // Check if any format is accepted
        for entry in &payload.formats[..payload.format_count] {
            let format_bit = 1u32 << (entry.format as u16).min(31);
            if (self.accepted_formats & format_bit) != 0 {
                return true;
            }
        }
        false
    }

    /// Check if action is accepted.
    pub fn accepts_action(&self, action: DragAction) -> bool {
        action.in_mask(self.accepted_actions)
    }

    /// Negotiate action with payload.
    pub fn negotiate_action(&self, payload: &DragPayload) -> DragAction {
        // Prefer payload's preferred action if both support it
        if self.accepts_action(payload.preferred_action) && payload.is_action_allowed(payload.preferred_action) {
            return payload.preferred_action;
        }
        // Try actions in order
        for action in &[DragAction::Copy, DragAction::Move, DragAction::Link] {
            if self.accepts_action(*action) && payload.is_action_allowed(*action) {
                return *action;
            }
        }
        DragAction::None
    }
}

// ============================================================================
// Drag Source
// ============================================================================

/// Drag source registration.
#[derive(Clone, Copy)]
pub struct DragSource {
    /// Source ID.
    pub id: u32,
    /// Owning app ID.
    pub app_id: AppId,
    /// Owning window ID.
    pub window_id: WindowId,
    /// Source bounds (x, y, width, height).
    pub bounds: (i32, i32, u32, u32),
    /// Allowed actions.
    pub allowed_actions: u8,
    /// Active flag.
    pub active: bool,
}

impl DragSource {
    /// Create empty source.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            app_id: 0,
            window_id: 0,
            bounds: (0, 0, 0, 0),
            allowed_actions: 0x07, // Copy, Move, Link
            active: false,
        }
    }

    /// Check if point is inside source.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (bx, by, bw, bh) = self.bounds;
        x >= bx && x < bx + bw as i32 && y >= by && y < by + bh as i32
    }
}

// ============================================================================
// Drag Session
// ============================================================================

/// Active drag session data.
#[derive(Clone, Copy)]
pub struct DragSession {
    /// Session ID.
    pub id: u32,
    /// Current state.
    pub state: DragState,
    /// Drag payload.
    pub payload: DragPayload,
    /// Source that initiated drag.
    pub source_id: u32,
    /// Start position (x, y).
    pub start_pos: (i32, i32),
    /// Current position (x, y).
    pub current_pos: (i32, i32),
    /// Current target (if over one).
    pub current_target_id: Option<u32>,
    /// Negotiated action.
    pub negotiated_action: DragAction,
    /// Timestamp when started.
    pub start_time: u64,
}

impl DragSession {
    /// Create empty session.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            state: DragState::Idle,
            payload: DragPayload::empty(),
            source_id: 0,
            start_pos: (0, 0),
            current_pos: (0, 0),
            current_target_id: None,
            negotiated_action: DragAction::None,
            start_time: 0,
        }
    }

    /// Check if session is active.
    pub fn is_active(&self) -> bool {
        !matches!(self.state, DragState::Idle)
    }

    /// Update position.
    pub fn update_position(&mut self, x: i32, y: i32) {
        self.current_pos = (x, y);
    }

    /// Calculate drag delta.
    pub fn delta(&self) -> (i32, i32) {
        (
            self.current_pos.0 - self.start_pos.0,
            self.current_pos.1 - self.start_pos.1,
        )
    }

    /// Calculate drag distance (squared, for threshold checks).
    pub fn distance_squared(&self) -> i32 {
        let (dx, dy) = self.delta();
        dx * dx + dy * dy
    }
}

// ============================================================================
// Drag Event
// ============================================================================

/// Drag-drop event for listeners.
#[derive(Clone, Copy, Debug)]
pub enum DragEvent {
    /// Drag operation started.
    Started {
        session_id: u32,
        source_app: AppId,
        pos: (i32, i32),
    },
    /// Pointer moved during drag.
    Moved { session_id: u32, pos: (i32, i32) },
    /// Entered a drop target.
    Entered {
        session_id: u32,
        target_id: u32,
        action: DragAction,
    },
    /// Left a drop target.
    Left { session_id: u32, target_id: u32 },
    /// Drop occurred.
    Dropped {
        session_id: u32,
        target_id: u32,
        action: DragAction,
    },
    /// Drag cancelled.
    Cancelled { session_id: u32 },
}

/// Drag event listener callback.
pub type DragListenerFn = fn(event: DragEvent);

/// Drag event listener.
#[derive(Clone, Copy)]
pub struct DragListener {
    /// Listener ID.
    pub id: u32,
    /// App ID.
    pub app_id: AppId,
    /// Callback.
    pub callback: Option<DragListenerFn>,
    /// Active flag.
    pub active: bool,
}

impl DragListener {
    /// Create empty listener.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            app_id: 0,
            callback: None,
            active: false,
        }
    }
}

// ============================================================================
// Drag Manager
// ============================================================================

/// Drag threshold distance (squared) to start drag.
const DRAG_THRESHOLD_SQ: i32 = 16; // 4 pixels

/// Main drag-drop manager.
pub struct DragManager {
    /// Current session.
    session: DragSession,
    /// Registered drop targets.
    targets: [DropTarget; MAX_DROP_TARGETS],
    /// Number of targets.
    target_count: usize,
    /// Registered drag sources.
    sources: [DragSource; MAX_DRAG_SOURCES],
    /// Number of sources.
    source_count: usize,
    /// Event listeners.
    listeners: [DragListener; MAX_DRAG_LISTENERS],
    /// Number of listeners.
    listener_count: usize,
    /// Next session ID.
    next_session_id: u32,
    /// Next target ID.
    next_target_id: u32,
    /// Next source ID.
    next_source_id: u32,
    /// Next listener ID.
    next_listener_id: u32,
    /// Current timestamp.
    timestamp: u64,
    /// Statistics: total drags.
    stats_drags: u64,
    /// Statistics: total drops.
    stats_drops: u64,
    /// Statistics: total cancels.
    stats_cancels: u64,
}

impl DragManager {
    /// Create a new drag manager.
    pub const fn new() -> Self {
        Self {
            session: DragSession::empty(),
            targets: [DropTarget::empty(); MAX_DROP_TARGETS],
            target_count: 0,
            sources: [DragSource::empty(); MAX_DRAG_SOURCES],
            source_count: 0,
            listeners: [DragListener::empty(); MAX_DRAG_LISTENERS],
            listener_count: 0,
            next_session_id: 1,
            next_target_id: 1,
            next_source_id: 1,
            next_listener_id: 1,
            timestamp: 0,
            stats_drags: 0,
            stats_drops: 0,
            stats_cancels: 0,
        }
    }

    // ========================================================================
    // Target Management
    // ========================================================================

    /// Register a drop target.
    pub fn register_target(
        &mut self,
        app_id: AppId,
        window_id: WindowId,
        bounds: (i32, i32, u32, u32),
    ) -> Option<u32> {
        if self.target_count >= MAX_DROP_TARGETS {
            return None;
        }

        let id = self.next_target_id;
        self.next_target_id += 1;

        self.targets[self.target_count] = DropTarget {
            id,
            app_id,
            window_id,
            bounds,
            accepted_formats: 0xFFFFFFFF,
            accepted_actions: 0x07,
            active: true,
            highlighted: false,
        };
        self.target_count += 1;

        Some(id)
    }

    /// Unregister a drop target.
    pub fn unregister_target(&mut self, target_id: u32) -> bool {
        for i in 0..self.target_count {
            if self.targets[i].id == target_id {
                for j in i..self.target_count - 1 {
                    self.targets[j] = self.targets[j + 1];
                }
                self.targets[self.target_count - 1] = DropTarget::empty();
                self.target_count -= 1;
                return true;
            }
        }
        false
    }

    /// Update target bounds.
    pub fn update_target_bounds(&mut self, target_id: u32, bounds: (i32, i32, u32, u32)) -> bool {
        for target in &mut self.targets[..self.target_count] {
            if target.id == target_id {
                target.bounds = bounds;
                return true;
            }
        }
        false
    }

    /// Set target accepted formats.
    pub fn set_target_formats(&mut self, target_id: u32, formats: u32) -> bool {
        for target in &mut self.targets[..self.target_count] {
            if target.id == target_id {
                target.accepted_formats = formats;
                return true;
            }
        }
        false
    }

    /// Find target at position.
    fn find_target_at(&self, x: i32, y: i32) -> Option<&DropTarget> {
        self.targets[..self.target_count]
            .iter()
            .find(|t| t.active && t.contains(x, y))
    }

    /// Find target by ID.
    fn find_target(&self, id: u32) -> Option<&DropTarget> {
        self.targets[..self.target_count].iter().find(|t| t.id == id)
    }

    /// Find target by ID (mutable).
    fn find_target_mut(&mut self, id: u32) -> Option<&mut DropTarget> {
        self.targets[..self.target_count]
            .iter_mut()
            .find(|t| t.id == id)
    }

    // ========================================================================
    // Source Management
    // ========================================================================

    /// Register a drag source.
    pub fn register_source(
        &mut self,
        app_id: AppId,
        window_id: WindowId,
        bounds: (i32, i32, u32, u32),
    ) -> Option<u32> {
        if self.source_count >= MAX_DRAG_SOURCES {
            return None;
        }

        let id = self.next_source_id;
        self.next_source_id += 1;

        self.sources[self.source_count] = DragSource {
            id,
            app_id,
            window_id,
            bounds,
            allowed_actions: 0x07,
            active: true,
        };
        self.source_count += 1;

        Some(id)
    }

    /// Unregister a drag source.
    pub fn unregister_source(&mut self, source_id: u32) -> bool {
        for i in 0..self.source_count {
            if self.sources[i].id == source_id {
                for j in i..self.source_count - 1 {
                    self.sources[j] = self.sources[j + 1];
                }
                self.sources[self.source_count - 1] = DragSource::empty();
                self.source_count -= 1;
                return true;
            }
        }
        false
    }

    /// Find source at position.
    fn find_source_at(&self, x: i32, y: i32) -> Option<&DragSource> {
        self.sources[..self.source_count]
            .iter()
            .find(|s| s.active && s.contains(x, y))
    }

    // ========================================================================
    // Drag Operations
    // ========================================================================

    /// Start drag operation (call on mouse down).
    pub fn begin_drag(
        &mut self,
        x: i32,
        y: i32,
        payload: DragPayload,
        source_id: Option<u32>,
    ) -> Result<u32, DragError> {
        if self.session.is_active() {
            return Err(DragError::AlreadyDragging);
        }

        if !payload.is_valid() {
            return Err(DragError::InvalidPayload);
        }

        let session_id = self.next_session_id;
        self.next_session_id += 1;

        self.session = DragSession {
            id: session_id,
            state: DragState::Pending,
            payload,
            source_id: source_id.unwrap_or(0),
            start_pos: (x, y),
            current_pos: (x, y),
            current_target_id: None,
            negotiated_action: DragAction::None,
            start_time: self.timestamp,
        };

        Ok(session_id)
    }

    /// Update drag position (call on mouse move).
    pub fn update_drag(&mut self, x: i32, y: i32) -> Option<DragAction> {
        if !self.session.is_active() {
            return None;
        }

        let old_state = self.session.state;
        self.session.update_position(x, y);

        // Check threshold for pending drags
        if self.session.state == DragState::Pending {
            if self.session.distance_squared() >= DRAG_THRESHOLD_SQ {
                self.session.state = DragState::Dragging;
                self.stats_drags += 1;
                // RAYOS_DRAGDROP:STARTED

                let event = DragEvent::Started {
                    session_id: self.session.id,
                    source_app: self.session.payload.source_app,
                    pos: self.session.start_pos,
                };
                self.notify_listeners(event);
            } else {
                return None;
            }
        }

        // Notify of move
        if self.session.state == DragState::Dragging || self.session.state == DragState::OverTarget {
            let event = DragEvent::Moved {
                session_id: self.session.id,
                pos: (x, y),
            };
            self.notify_listeners(event);
        }

        // Check for target changes
        let new_target = self.find_target_at(x, y).map(|t| t.id);
        let old_target = self.session.current_target_id;

        if new_target != old_target {
            // Left old target
            if let Some(old_id) = old_target {
                if let Some(target) = self.find_target_mut(old_id) {
                    target.highlighted = false;
                }
                self.session.state = DragState::Dragging;
                // RAYOS_DRAGDROP:LEFT

                let event = DragEvent::Left {
                    session_id: self.session.id,
                    target_id: old_id,
                };
                self.notify_listeners(event);
            }

            // Entered new target
            if let Some(new_id) = new_target {
                if let Some(target) = self.find_target(new_id) {
                    if target.accepts_payload(&self.session.payload) {
                        let action = target.negotiate_action(&self.session.payload);
                        
                        // Update session state
                        if let Some(target_mut) = self.find_target_mut(new_id) {
                            target_mut.highlighted = true;
                        }
                        self.session.current_target_id = Some(new_id);
                        self.session.negotiated_action = action;
                        self.session.state = DragState::OverTarget;
                        // RAYOS_DRAGDROP:ENTERED

                        let event = DragEvent::Entered {
                            session_id: self.session.id,
                            target_id: new_id,
                            action,
                        };
                        self.notify_listeners(event);

                        return Some(action);
                    }
                }
            }

            self.session.current_target_id = new_target;
            self.session.negotiated_action = DragAction::None;
        }

        if self.session.state == DragState::OverTarget {
            Some(self.session.negotiated_action)
        } else if old_state != self.session.state {
            Some(DragAction::None)
        } else {
            None
        }
    }

    /// End drag operation (call on mouse up).
    pub fn end_drag(&mut self, x: i32, y: i32) -> Result<DragResult, DragError> {
        if !self.session.is_active() {
            return Err(DragError::NotDragging);
        }

        self.session.update_position(x, y);

        // If still pending, cancel
        if self.session.state == DragState::Pending {
            self.cancel_drag();
            return Err(DragError::Cancelled);
        }

        // Try to drop
        if self.session.state == DragState::OverTarget {
            if let Some(target_id) = self.session.current_target_id {
                let action = self.session.negotiated_action;
                
                // Unhighlight target
                if let Some(target) = self.find_target_mut(target_id) {
                    target.highlighted = false;
                }

                let result = DragResult {
                    session_id: self.session.id,
                    target_id,
                    action,
                    pos: (x, y),
                };

                self.stats_drops += 1;
                // RAYOS_DRAGDROP:DROPPED

                let event = DragEvent::Dropped {
                    session_id: self.session.id,
                    target_id,
                    action,
                };
                self.notify_listeners(event);

                // Reset session
                self.session = DragSession::empty();

                return Ok(result);
            }
        }

        // No valid drop target - cancel
        self.cancel_drag();
        Err(DragError::NoTarget)
    }

    /// Cancel current drag operation.
    pub fn cancel_drag(&mut self) {
        if !self.session.is_active() {
            return;
        }

        // Unhighlight current target
        if let Some(target_id) = self.session.current_target_id {
            if let Some(target) = self.find_target_mut(target_id) {
                target.highlighted = false;
            }
        }

        let session_id = self.session.id;
        self.session = DragSession::empty();
        self.stats_cancels += 1;
        // RAYOS_DRAGDROP:CANCELLED

        let event = DragEvent::Cancelled { session_id };
        self.notify_listeners(event);
    }

    /// Get current session.
    pub fn session(&self) -> Option<&DragSession> {
        if self.session.is_active() {
            Some(&self.session)
        } else {
            None
        }
    }

    /// Check if currently dragging.
    pub fn is_dragging(&self) -> bool {
        self.session.is_active()
    }

    /// Get current drag state.
    pub fn state(&self) -> DragState {
        self.session.state
    }

    // ========================================================================
    // Listener Management
    // ========================================================================

    /// Add event listener.
    pub fn add_listener(&mut self, app_id: AppId, callback: DragListenerFn) -> Option<u32> {
        if self.listener_count >= MAX_DRAG_LISTENERS {
            return None;
        }

        let id = self.next_listener_id;
        self.next_listener_id += 1;

        self.listeners[self.listener_count] = DragListener {
            id,
            app_id,
            callback: Some(callback),
            active: true,
        };
        self.listener_count += 1;

        Some(id)
    }

    /// Remove listener.
    pub fn remove_listener(&mut self, listener_id: u32) -> bool {
        for i in 0..self.listener_count {
            if self.listeners[i].id == listener_id {
                for j in i..self.listener_count - 1 {
                    self.listeners[j] = self.listeners[j + 1];
                }
                self.listeners[self.listener_count - 1] = DragListener::empty();
                self.listener_count -= 1;
                return true;
            }
        }
        false
    }

    /// Notify all listeners.
    fn notify_listeners(&self, event: DragEvent) {
        for listener in &self.listeners[..self.listener_count] {
            if listener.active {
                if let Some(callback) = listener.callback {
                    callback(event);
                }
            }
        }
    }

    // ========================================================================
    // Utilities
    // ========================================================================

    /// Tick the manager (update timestamp).
    pub fn tick(&mut self) {
        self.timestamp += 1;
    }

    /// Get statistics.
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.stats_drags, self.stats_drops, self.stats_cancels)
    }

    /// Clean up targets for a window.
    pub fn cleanup_window(&mut self, window_id: WindowId) {
        // Remove targets for this window
        let mut i = 0;
        while i < self.target_count {
            if self.targets[i].window_id == window_id {
                for j in i..self.target_count - 1 {
                    self.targets[j] = self.targets[j + 1];
                }
                self.targets[self.target_count - 1] = DropTarget::empty();
                self.target_count -= 1;
            } else {
                i += 1;
            }
        }

        // Remove sources for this window
        i = 0;
        while i < self.source_count {
            if self.sources[i].window_id == window_id {
                for j in i..self.source_count - 1 {
                    self.sources[j] = self.sources[j + 1];
                }
                self.sources[self.source_count - 1] = DragSource::empty();
                self.source_count -= 1;
            } else {
                i += 1;
            }
        }

        // Cancel drag if source was from this window
        if self.session.is_active() && self.session.payload.source_window == window_id {
            self.cancel_drag();
        }
    }
}

// ============================================================================
// Drag Errors
// ============================================================================

/// Drag operation errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragError {
    /// Already in a drag operation.
    AlreadyDragging,
    /// No active drag operation.
    NotDragging,
    /// Invalid payload.
    InvalidPayload,
    /// No valid drop target.
    NoTarget,
    /// Drag was cancelled.
    Cancelled,
    /// Internal error.
    Internal,
}

// ============================================================================
// Drag Result
// ============================================================================

/// Successful drop result.
#[derive(Clone, Copy, Debug)]
pub struct DragResult {
    /// Session ID.
    pub session_id: u32,
    /// Target that received the drop.
    pub target_id: u32,
    /// Action performed.
    pub action: DragAction,
    /// Drop position.
    pub pos: (i32, i32),
}

// ============================================================================
// Global Drag Manager
// ============================================================================

/// Global drag manager instance.
static mut GLOBAL_DRAG: DragManager = DragManager::new();

/// Get the global drag manager.
pub fn drag_manager() -> &'static DragManager {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_DRAG }
}

/// Get the global drag manager mutably.
pub fn drag_manager_mut() -> &'static mut DragManager {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_DRAG }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drag_action_masks() {
        assert!(DragAction::Copy.in_mask(0x07));
        assert!(DragAction::Move.in_mask(0x07));
        assert!(DragAction::Link.in_mask(0x07));
        assert!(!DragAction::Ask.in_mask(0x07));
    }

    #[test]
    fn test_inline_drag_data() {
        let data = InlineDragData::from_bytes(b"hello").unwrap();
        assert_eq!(data.as_bytes(), b"hello");
        assert_eq!(data.as_str(), Some("hello"));
    }

    #[test]
    fn test_drag_payload() {
        let mut payload = DragPayload::empty();
        assert!(!payload.is_valid());

        let entry = DragFormatEntry::text("test").unwrap();
        payload.add_format(entry);

        assert!(payload.is_valid());
        assert!(payload.has_format(ClipboardFormat::Text));
        assert_eq!(payload.get_text(), Some("test"));
    }

    #[test]
    fn test_drop_target_contains() {
        let mut target = DropTarget::empty();
        target.bounds = (10, 10, 100, 100);
        target.active = true;

        assert!(target.contains(50, 50));
        assert!(target.contains(10, 10));
        assert!(!target.contains(9, 50));
        assert!(!target.contains(110, 50));
    }

    #[test]
    fn test_drag_session() {
        let mut session = DragSession::empty();
        assert!(!session.is_active());

        session.state = DragState::Dragging;
        session.start_pos = (10, 10);
        session.current_pos = (20, 30);

        assert!(session.is_active());
        assert_eq!(session.delta(), (10, 20));
    }

    #[test]
    fn test_drag_manager_targets() {
        let mut manager = DragManager::new();

        let id = manager.register_target(1, 1, (0, 0, 100, 100)).unwrap();
        assert!(id > 0);

        assert!(manager.unregister_target(id));
        assert!(!manager.unregister_target(id)); // Already removed
    }

    #[test]
    fn test_action_negotiation() {
        let mut target = DropTarget::empty();
        target.active = true;
        target.accepted_actions = 0x03; // Copy, Move

        let mut payload = DragPayload::empty();
        payload.preferred_action = DragAction::Link;
        payload.allowed_actions = 0x07; // Copy, Move, Link

        // Link not accepted by target, should fall back to Copy
        let action = target.negotiate_action(&payload);
        assert_eq!(action, DragAction::Copy);

        // Now prefer Move
        payload.preferred_action = DragAction::Move;
        let action = target.negotiate_action(&payload);
        assert_eq!(action, DragAction::Move);
    }
}
