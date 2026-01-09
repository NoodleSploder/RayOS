//! System 1: The Reflex Engine (Subconscious)
//!
//! Handles millisecond-latency reflexive responses:
//! - Pattern matching for input sequences
//! - Learned reflexes (trained by System 2)
//! - Gesture recognition
//! - Attention signals to System 2
//!
//! Designed to run as a persistent loop, eventually on GPU compute shaders.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Maximum number of stored reflexes
pub const MAX_REFLEXES: usize = 64;

/// Maximum pattern length for a reflex trigger
pub const MAX_PATTERN_LEN: usize = 8;

/// Maximum number of pending attention signals
pub const MAX_ATTENTION_SIGNALS: usize = 16;

/// History buffer size for pattern matching
pub const INPUT_HISTORY_SIZE: usize = 32;

// ============================================================================
// INPUT EVENT TYPES
// ============================================================================

/// Input event types that can trigger reflexes
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum InputEventType {
    None = 0,
    MouseMove = 1,
    MouseDown = 2,
    MouseUp = 3,
    MouseClick = 4,      // Down + Up in quick succession
    MouseDoubleClick = 5,
    MouseTripleClick = 6,
    KeyDown = 7,
    KeyUp = 8,
    KeyPress = 9,        // Down + Up
    GestureSwipeLeft = 10,
    GestureSwipeRight = 11,
    GestureSwipeUp = 12,
    GestureSwipeDown = 13,
    GesturePinchIn = 14,
    GesturePinchOut = 15,
    GestureRotate = 16,
    GazeFocus = 17,      // Eye tracking: focused on target
    GazeAway = 18,       // Eye tracking: looked away
    VoiceCommand = 19,   // Voice input detected
    Timeout = 20,        // Time-based trigger
}

impl Default for InputEventType {
    fn default() -> Self {
        InputEventType::None
    }
}

/// A single input event in the history
#[derive(Clone, Copy, Default)]
pub struct InputEvent {
    /// Event type
    pub event_type: InputEventType,
    /// Modifier keys (shift, ctrl, alt, meta)
    pub modifiers: u8,
    /// Primary data (keycode, button, etc.)
    pub data: u16,
    /// Secondary data (x coord, etc.)
    pub x: i16,
    /// Tertiary data (y coord, etc.)
    pub y: i16,
    /// Timestamp (frame counter)
    pub timestamp: u32,
}

impl InputEvent {
    pub const fn new(event_type: InputEventType, data: u16) -> Self {
        Self {
            event_type,
            modifiers: 0,
            data,
            x: 0,
            y: 0,
            timestamp: 0,
        }
    }

    pub const fn with_modifiers(mut self, modifiers: u8) -> Self {
        self.modifiers = modifiers;
        self
    }

    pub const fn with_position(mut self, x: i16, y: i16) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    /// Check if this event matches a pattern element
    pub fn matches(&self, pattern: &PatternElement) -> bool {
        if pattern.event_type != self.event_type {
            return false;
        }
        if pattern.require_data && pattern.data != self.data {
            return false;
        }
        if pattern.require_modifiers && pattern.modifiers != self.modifiers {
            return false;
        }
        true
    }
}

// ============================================================================
// REFLEX PATTERNS
// ============================================================================

/// A single element in a reflex pattern
#[derive(Clone, Copy, Default)]
pub struct PatternElement {
    pub event_type: InputEventType,
    pub data: u16,
    pub modifiers: u8,
    pub require_data: bool,
    pub require_modifiers: bool,
    /// Max frames between this and next event (0 = no limit)
    pub max_gap: u16,
}

impl PatternElement {
    pub const fn new(event_type: InputEventType) -> Self {
        Self {
            event_type,
            data: 0,
            modifiers: 0,
            require_data: false,
            require_modifiers: false,
            max_gap: 30, // Default: ~500ms at 60fps
        }
    }

    pub const fn with_data(mut self, data: u16) -> Self {
        self.data = data;
        self.require_data = true;
        self
    }

    pub const fn with_modifiers(mut self, modifiers: u8) -> Self {
        self.modifiers = modifiers;
        self.require_modifiers = true;
        self
    }

    pub const fn with_max_gap(mut self, frames: u16) -> Self {
        self.max_gap = frames;
        self
    }
}

// ============================================================================
// REFLEX ACTIONS
// ============================================================================

/// Action to take when a reflex fires
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ReflexAction {
    None = 0,
    /// Open an application by ID
    OpenApp = 1,
    /// Close the focused window
    CloseWindow = 2,
    /// Minimize the focused window
    MinimizeWindow = 3,
    /// Maximize/restore the focused window
    ToggleMaximize = 4,
    /// Switch to next window
    NextWindow = 5,
    /// Switch to previous window
    PrevWindow = 6,
    /// Show app launcher
    ShowLauncher = 7,
    /// Show/hide panel
    TogglePanel = 8,
    /// Emit a custom ray to System 2
    EmitRay = 9,
    /// Signal attention to System 2 (for learning)
    SignalAttention = 10,
    /// Execute a shell command by ID
    RunCommand = 11,
    /// Trigger notification
    Notify = 12,
    /// Copy selection
    Copy = 13,
    /// Paste clipboard
    Paste = 14,
    /// Undo last action
    Undo = 15,
    /// Redo last undone action
    Redo = 16,
}

impl Default for ReflexAction {
    fn default() -> Self {
        ReflexAction::None
    }
}

// ============================================================================
// REFLEX DEFINITION
// ============================================================================

/// A complete reflex: pattern → action
#[derive(Clone, Copy)]
pub struct Reflex {
    /// Unique reflex ID
    pub id: u32,
    /// Human-readable name (null-terminated, max 31 chars)
    pub name: [u8; 32],
    /// Pattern to match
    pub pattern: [PatternElement; MAX_PATTERN_LEN],
    /// Number of elements in pattern
    pub pattern_len: u8,
    /// Action to execute
    pub action: ReflexAction,
    /// Action argument (app ID, command ID, etc.)
    pub action_arg: u32,
    /// Priority (higher = checked first)
    pub priority: u8,
    /// Is this reflex enabled?
    pub enabled: bool,
    /// Was this learned from System 2?
    pub learned: bool,
    /// How many times has this fired?
    pub fire_count: u32,
    /// Last fire timestamp
    pub last_fired: u32,
}

impl Default for Reflex {
    fn default() -> Self {
        Self {
            id: 0,
            name: [0; 32],
            pattern: [PatternElement::default(); MAX_PATTERN_LEN],
            pattern_len: 0,
            action: ReflexAction::None,
            action_arg: 0,
            priority: 128,
            enabled: false,
            learned: false,
            fire_count: 0,
            last_fired: 0,
        }
    }
}

impl Reflex {
    pub fn new(id: u32, name: &[u8]) -> Self {
        let mut r = Self::default();
        r.id = id;
        let len = name.len().min(31);
        r.name[..len].copy_from_slice(&name[..len]);
        r
    }

    pub fn set_pattern(&mut self, pattern: &[PatternElement]) {
        let len = pattern.len().min(MAX_PATTERN_LEN);
        self.pattern[..len].copy_from_slice(&pattern[..len]);
        self.pattern_len = len as u8;
    }

    pub fn set_action(&mut self, action: ReflexAction, arg: u32) {
        self.action = action;
        self.action_arg = arg;
        self.enabled = true;
    }
}

// ============================================================================
// ATTENTION SIGNALS (System 1 → System 2)
// ============================================================================

/// Attention signal types sent to System 2
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum AttentionType {
    None = 0,
    /// User focused on something (gaze + hover)
    UserFocus = 1,
    /// Unusual input pattern detected
    AnomalousInput = 2,
    /// Gesture not matching any reflex
    UnknownGesture = 3,
    /// Repeated action (possible new reflex candidate)
    RepeatedPattern = 4,
    /// User appears confused (hesitation, backtracking)
    UserConfusion = 5,
    /// High-priority external event
    ExternalEvent = 6,
    /// Reflex fired (for learning feedback)
    ReflexFired = 7,
    /// Habit discovered (pattern repeated enough to propose as reflex)
    HabitDiscovered = 8,
}

impl Default for AttentionType {
    fn default() -> Self {
        AttentionType::None
    }
}

/// A signal from System 1 to System 2
#[derive(Clone, Copy, Default)]
pub struct AttentionSignal {
    pub signal_type: AttentionType,
    pub priority: u8,
    pub data: u32,
    pub context: u64,
    pub timestamp: u32,
}

impl AttentionSignal {
    pub fn new(signal_type: AttentionType, data: u32) -> Self {
        Self {
            signal_type,
            priority: 128,
            data,
            context: 0,
            timestamp: 0,
        }
    }
}

// ============================================================================
// PATTERN LEARNING (Habit Detection)
// ============================================================================

/// Maximum number of candidate habits being tracked
pub const MAX_HABIT_CANDIDATES: usize = 16;

/// Maximum pattern length for habit detection
pub const MAX_HABIT_PATTERN_LEN: usize = 4;

/// Number of repetitions before a habit is proposed to System 2
pub const HABIT_THRESHOLD: u32 = 3;

/// Maximum gap (in frames) between repetitions to count as the same habit
pub const HABIT_DECAY_FRAMES: u32 = 3600; // ~1 minute at 60fps

/// A candidate habit that may be promoted to a reflex
#[derive(Clone, Copy)]
pub struct HabitCandidate {
    /// The detected pattern sequence
    pub pattern: [PatternElement; MAX_HABIT_PATTERN_LEN],
    /// Length of the pattern
    pub pattern_len: u8,
    /// How many times this pattern has been observed
    pub occurrence_count: u32,
    /// Frame when last observed
    pub last_seen_frame: u64,
    /// Hash of the pattern for quick comparison
    pub pattern_hash: u32,
    /// Whether this has been proposed to System 2
    pub proposed: bool,
    /// Suggested action (derived from what follows the pattern)
    pub suggested_action: ReflexAction,
    /// Argument for suggested action
    pub suggested_action_arg: u32,
    /// Active slot
    pub active: bool,
}

impl HabitCandidate {
    pub const fn empty() -> Self {
        Self {
            pattern: [PatternElement {
                event_type: InputEventType::None,
                data: 0,
                modifiers: 0,
                require_data: false,
                require_modifiers: false,
                max_gap: 0,
            }; MAX_HABIT_PATTERN_LEN],
            pattern_len: 0,
            occurrence_count: 0,
            last_seen_frame: 0,
            pattern_hash: 0,
            proposed: false,
            suggested_action: ReflexAction::None,
            suggested_action_arg: 0,
            active: false,
        }
    }

    /// Compute a simple hash for a pattern sequence
    pub fn compute_hash(events: &[InputEvent], len: usize) -> u32 {
        let mut hash: u32 = 0;
        for i in 0..len.min(MAX_HABIT_PATTERN_LEN) {
            let ev = &events[i];
            // Mix event type, data, and modifiers into hash
            hash = hash.wrapping_mul(31).wrapping_add(ev.event_type as u32);
            hash = hash.wrapping_mul(31).wrapping_add(ev.data as u32);
            hash = hash.wrapping_mul(31).wrapping_add(ev.modifiers as u32);
        }
        hash
    }

    /// Check if a sequence of events matches this habit's pattern
    pub fn matches(&self, events: &[InputEvent], len: usize) -> bool {
        if len != self.pattern_len as usize {
            return false;
        }
        for i in 0..len {
            let pat = &self.pattern[i];
            let ev = &events[i];
            if pat.event_type != ev.event_type {
                return false;
            }
            if pat.require_data && pat.data != ev.data {
                return false;
            }
            if pat.require_modifiers && pat.modifiers != ev.modifiers {
                return false;
            }
        }
        true
    }
}

/// Habit learning engine - detects repeated patterns
pub struct HabitLearner {
    /// Candidate habits being tracked
    candidates: [HabitCandidate; MAX_HABIT_CANDIDATES],
    /// Number of active candidates
    candidate_count: usize,
    /// Learning enabled
    enabled: bool,
    /// Next habit ID to assign
    next_habit_id: u32,
    /// Statistics
    pub stats: HabitStats,
}

/// Learning statistics
#[derive(Clone, Copy, Default)]
pub struct HabitStats {
    pub patterns_analyzed: u64,
    pub candidates_created: u64,
    pub habits_proposed: u64,
    pub habits_promoted: u64,
    pub candidates_expired: u64,
}

impl HabitLearner {
    pub const fn new() -> Self {
        Self {
            candidates: [HabitCandidate::empty(); MAX_HABIT_CANDIDATES],
            candidate_count: 0,
            enabled: true,
            next_habit_id: 1000, // Start learned IDs at 1000
            stats: HabitStats {
                patterns_analyzed: 0,
                candidates_created: 0,
                habits_proposed: 0,
                habits_promoted: 0,
                candidates_expired: 0,
            },
        }
    }

    /// Analyze recent history for patterns
    /// Returns Some(candidate_index) if a habit should be proposed to System 2
    pub fn analyze_history(
        &mut self,
        history: &[InputEvent],
        history_len: usize,
        current_frame: u64,
    ) -> Option<usize> {
        if !self.enabled || history_len < 2 {
            return None;
        }

        self.stats.patterns_analyzed += 1;

        // Expire old candidates
        self.expire_candidates(current_frame);

        // Try to find patterns of length 2, 3, 4
        for pattern_len in 2..=MAX_HABIT_PATTERN_LEN.min(history_len / 2) {
            // Check if the last pattern_len events match an earlier sequence
            let recent_start = history_len.saturating_sub(pattern_len);
            let recent_events = &history[recent_start..history_len];
            let hash = HabitCandidate::compute_hash(recent_events, pattern_len);

            // Look for this pattern earlier in history
            if self.find_pattern_in_history(history, history_len, recent_events, pattern_len) {
                // Pattern was found earlier! Track it
                if let Some(idx) = self.update_or_create_candidate(
                    recent_events,
                    pattern_len,
                    hash,
                    current_frame,
                ) {
                    let candidate = &self.candidates[idx];
                    if candidate.occurrence_count >= HABIT_THRESHOLD && !candidate.proposed {
                        return Some(idx);
                    }
                }
            }
        }

        None
    }

    /// Look for a pattern earlier in history
    fn find_pattern_in_history(
        &self,
        history: &[InputEvent],
        history_len: usize,
        pattern: &[InputEvent],
        pattern_len: usize,
    ) -> bool {
        if history_len < pattern_len * 2 {
            return false;
        }

        // Search from start up to (but not including) the most recent pattern
        let search_end = history_len.saturating_sub(pattern_len);
        for start in 0..search_end.saturating_sub(pattern_len - 1) {
            let mut matches = true;
            for i in 0..pattern_len {
                let hist_ev = &history[start + i];
                let pat_ev = &pattern[i];
                if hist_ev.event_type != pat_ev.event_type
                    || hist_ev.data != pat_ev.data
                    || hist_ev.modifiers != pat_ev.modifiers
                {
                    matches = false;
                    break;
                }
            }
            if matches {
                return true;
            }
        }
        false
    }

    /// Update existing candidate or create new one
    fn update_or_create_candidate(
        &mut self,
        events: &[InputEvent],
        pattern_len: usize,
        hash: u32,
        current_frame: u64,
    ) -> Option<usize> {
        // First, look for existing candidate with same hash
        for i in 0..self.candidate_count {
            if self.candidates[i].active && self.candidates[i].pattern_hash == hash {
                // Verify it actually matches (hash collision check)
                if self.candidates[i].matches(events, pattern_len) {
                    self.candidates[i].occurrence_count += 1;
                    self.candidates[i].last_seen_frame = current_frame;
                    return Some(i);
                }
            }
        }

        // Create new candidate if we have space
        if self.candidate_count < MAX_HABIT_CANDIDATES {
            let idx = self.candidate_count;
            self.candidates[idx] = HabitCandidate::empty();
            self.candidates[idx].active = true;
            self.candidates[idx].pattern_hash = hash;
            self.candidates[idx].pattern_len = pattern_len as u8;
            self.candidates[idx].occurrence_count = 1;
            self.candidates[idx].last_seen_frame = current_frame;

            // Copy pattern elements
            for i in 0..pattern_len {
                self.candidates[idx].pattern[i] = PatternElement::new(events[i].event_type)
                    .with_data(events[i].data)
                    .with_modifiers(events[i].modifiers);
            }

            // Infer suggested action from the pattern
            self.candidates[idx].suggested_action = self.infer_action(&events[..pattern_len]);

            self.candidate_count += 1;
            self.stats.candidates_created += 1;
            return Some(idx);
        }

        // No space, try to evict oldest
        let mut oldest_idx = 0;
        let mut oldest_frame = u64::MAX;
        for i in 0..self.candidate_count {
            if self.candidates[i].active && self.candidates[i].last_seen_frame < oldest_frame {
                oldest_frame = self.candidates[i].last_seen_frame;
                oldest_idx = i;
            }
        }

        // Evict if the oldest is significantly stale
        if current_frame.saturating_sub(oldest_frame) > HABIT_DECAY_FRAMES as u64 * 2 {
            let idx = oldest_idx;
            self.candidates[idx] = HabitCandidate::empty();
            self.candidates[idx].active = true;
            self.candidates[idx].pattern_hash = hash;
            self.candidates[idx].pattern_len = pattern_len as u8;
            self.candidates[idx].occurrence_count = 1;
            self.candidates[idx].last_seen_frame = current_frame;

            for i in 0..pattern_len {
                self.candidates[idx].pattern[i] = PatternElement::new(events[i].event_type)
                    .with_data(events[i].data)
                    .with_modifiers(events[i].modifiers);
            }

            self.candidates[idx].suggested_action = self.infer_action(&events[..pattern_len]);
            self.stats.candidates_expired += 1;
            return Some(idx);
        }

        None
    }

    /// Expire candidates that haven't been seen recently
    fn expire_candidates(&mut self, current_frame: u64) {
        for i in 0..self.candidate_count {
            if self.candidates[i].active {
                let age = current_frame.saturating_sub(self.candidates[i].last_seen_frame);
                if age > HABIT_DECAY_FRAMES as u64 && self.candidates[i].occurrence_count < HABIT_THRESHOLD {
                    self.candidates[i].active = false;
                    self.stats.candidates_expired += 1;
                }
            }
        }
    }

    /// Infer a likely action from a pattern
    fn infer_action(&self, pattern: &[InputEvent]) -> ReflexAction {
        if pattern.is_empty() {
            return ReflexAction::None;
        }

        // Analyze the last event in the pattern
        let last = &pattern[pattern.len() - 1];

        match last.event_type {
            // Keyboard shortcuts often want to signal attention
            InputEventType::KeyPress | InputEventType::KeyDown => {
                ReflexAction::SignalAttention
            }
            // Mouse actions often relate to window/UI
            InputEventType::MouseDoubleClick => ReflexAction::SignalAttention,
            InputEventType::MouseTripleClick => ReflexAction::SignalAttention,
            // Gestures often map to navigation
            InputEventType::GestureSwipeLeft => ReflexAction::SignalAttention,
            InputEventType::GestureSwipeRight => ReflexAction::SignalAttention,
            _ => ReflexAction::SignalAttention, // Default to notification
        }
    }

    /// Mark a candidate as proposed (to avoid re-proposing)
    pub fn mark_proposed(&mut self, idx: usize) {
        if idx < self.candidate_count && self.candidates[idx].active {
            self.candidates[idx].proposed = true;
            self.stats.habits_proposed += 1;
        }
    }

    /// Promote a candidate to a real reflex
    /// Returns the reflex if successful
    pub fn promote_candidate(&mut self, idx: usize) -> Option<Reflex> {
        if idx >= self.candidate_count || !self.candidates[idx].active {
            return None;
        }

        let candidate = &self.candidates[idx];
        let id = self.next_habit_id;
        self.next_habit_id += 1;

        let mut reflex = Reflex::new(id, b"habit");
        for i in 0..candidate.pattern_len as usize {
            reflex.pattern[i] = candidate.pattern[i];
        }
        reflex.pattern_len = candidate.pattern_len;
        reflex.action = candidate.suggested_action;
        reflex.action_arg = candidate.suggested_action_arg;
        reflex.learned = true;
        reflex.enabled = true;
        reflex.priority = 64; // Lower priority than built-in reflexes

        self.stats.habits_promoted += 1;

        // Deactivate the candidate
        self.candidates[idx].active = false;

        Some(reflex)
    }

    /// Get a candidate by index
    pub fn get_candidate(&self, idx: usize) -> Option<&HabitCandidate> {
        if idx < self.candidate_count && self.candidates[idx].active {
            Some(&self.candidates[idx])
        } else {
            None
        }
    }

    /// Get number of active candidates
    pub fn active_candidate_count(&self) -> usize {
        let mut count = 0;
        for i in 0..self.candidate_count {
            if self.candidates[i].active {
                count += 1;
            }
        }
        count
    }

    /// Enable/disable learning
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if learning is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Clear all candidates
    pub fn clear(&mut self) {
        for i in 0..MAX_HABIT_CANDIDATES {
            self.candidates[i] = HabitCandidate::empty();
        }
        self.candidate_count = 0;
    }
}

// ============================================================================
// REFLEX ENGINE STATE
// ============================================================================

/// Global reflex engine state
pub struct ReflexEngine {
    /// Registered reflexes
    reflexes: [Reflex; MAX_REFLEXES],
    /// Number of active reflexes
    reflex_count: usize,

    /// Input event history (ring buffer)
    input_history: [InputEvent; INPUT_HISTORY_SIZE],
    /// Next write position in history
    history_head: usize,
    /// Number of events in history
    history_len: usize,

    /// Pending attention signals for System 2
    attention_queue: [AttentionSignal; MAX_ATTENTION_SIGNALS],
    /// Number of pending signals
    attention_count: usize,

    /// Pattern matching state: current best match candidate
    match_candidate: usize,
    /// How many elements of candidate have matched
    match_progress: usize,
    /// Timestamp of last matched element
    match_last_timestamp: u32,

    /// Current frame counter
    frame: u32,

    /// Engine enabled
    enabled: bool,

    /// Statistics
    pub stats: ReflexStats,

    /// Frame count (u64 for extended time)
    frame_count: u64,

    /// Suppress notifications until this frame
    suppress_until_frame: u64,

    /// Attention weights by input type (0-7)
    attention_weights: [u8; 8],

    /// Habit learner for detecting repeated patterns
    habit_learner: HabitLearner,

    /// Whether to auto-promote habits to reflexes
    auto_promote_habits: bool,
}

/// Engine statistics
#[derive(Clone, Copy, Default)]
pub struct ReflexStats {
    pub events_processed: u64,
    pub reflexes_fired: u64,
    pub patterns_checked: u64,
    pub attention_signals_sent: u64,
    pub unknown_gestures: u64,
}

impl ReflexEngine {
    /// Create a new reflex engine (const for static init)
    pub const fn new() -> Self {
        Self {
            reflexes: [Reflex {
                id: 0,
                name: [0; 32],
                pattern: [PatternElement {
                    event_type: InputEventType::None,
                    data: 0,
                    modifiers: 0,
                    require_data: false,
                    require_modifiers: false,
                    max_gap: 0,
                }; MAX_PATTERN_LEN],
                pattern_len: 0,
                action: ReflexAction::None,
                action_arg: 0,
                priority: 0,
                enabled: false,
                learned: false,
                fire_count: 0,
                last_fired: 0,
            }; MAX_REFLEXES],
            reflex_count: 0,
            input_history: [InputEvent {
                event_type: InputEventType::None,
                modifiers: 0,
                data: 0,
                x: 0,
                y: 0,
                timestamp: 0,
            }; INPUT_HISTORY_SIZE],
            history_head: 0,
            history_len: 0,
            attention_queue: [AttentionSignal {
                signal_type: AttentionType::None,
                priority: 0,
                data: 0,
                context: 0,
                timestamp: 0,
            }; MAX_ATTENTION_SIGNALS],
            attention_count: 0,
            match_candidate: 0,
            match_progress: 0,
            match_last_timestamp: 0,
            frame: 0,
            enabled: true,
            stats: ReflexStats {
                events_processed: 0,
                reflexes_fired: 0,
                patterns_checked: 0,
                attention_signals_sent: 0,
                unknown_gestures: 0,
            },
            frame_count: 0,
            suppress_until_frame: 0,
            attention_weights: [100, 100, 100, 100, 100, 100, 100, 100],
            habit_learner: HabitLearner::new(),
            auto_promote_habits: false, // Require System 2 approval by default
        }
    }

    /// Initialize with default reflexes
    pub fn init(&mut self) {
        self.install_default_reflexes();
        self.enabled = true;

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_SYSTEM1_REFLEX_INIT:ok\n");
        }
    }

    /// Install built-in reflexes
    fn install_default_reflexes(&mut self) {
        // Ctrl+W: Close window
        self.add_reflex_simple(
            1,
            b"close_window",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(b'W' as u16)
                .with_modifiers(0x01)], // Ctrl
            ReflexAction::CloseWindow,
            0,
        );

        // Ctrl+Q: Close window (alternative)
        self.add_reflex_simple(
            2,
            b"quit_app",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(b'Q' as u16)
                .with_modifiers(0x01)],
            ReflexAction::CloseWindow,
            0,
        );

        // Alt+Tab: Next window
        self.add_reflex_simple(
            3,
            b"next_window",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(0x09) // Tab
                .with_modifiers(0x04)], // Alt
            ReflexAction::NextWindow,
            0,
        );

        // Alt+Shift+Tab: Previous window
        self.add_reflex_simple(
            4,
            b"prev_window",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(0x09) // Tab
                .with_modifiers(0x05)], // Alt + Shift
            ReflexAction::PrevWindow,
            0,
        );

        // Triple-click: Select all (common pattern)
        self.add_reflex_simple(
            5,
            b"triple_click",
            &[PatternElement::new(InputEventType::MouseTripleClick)],
            ReflexAction::EmitRay,
            1, // Ray ID for "select all"
        );

        // Super key: Show launcher
        self.add_reflex_simple(
            6,
            b"show_launcher",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(0x5B)], // Left Super/Windows key
            ReflexAction::ShowLauncher,
            0,
        );

        // Ctrl+C: Copy
        self.add_reflex_simple(
            7,
            b"copy",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(b'C' as u16)
                .with_modifiers(0x01)],
            ReflexAction::Copy,
            0,
        );

        // Ctrl+V: Paste
        self.add_reflex_simple(
            8,
            b"paste",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(b'V' as u16)
                .with_modifiers(0x01)],
            ReflexAction::Paste,
            0,
        );

        // Ctrl+Z: Undo
        self.add_reflex_simple(
            9,
            b"undo",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(b'Z' as u16)
                .with_modifiers(0x01)],
            ReflexAction::Undo,
            0,
        );

        // Ctrl+Shift+Z: Redo
        self.add_reflex_simple(
            10,
            b"redo",
            &[PatternElement::new(InputEventType::KeyPress)
                .with_data(b'Z' as u16)
                .with_modifiers(0x03)], // Ctrl + Shift
            ReflexAction::Redo,
            0,
        );

        #[cfg(feature = "serial_debug")]
        {
            crate::serial_write_str("RAYOS_SYSTEM1_DEFAULT_REFLEXES:");
            // Write count
            let count = self.reflex_count;
            if count >= 10 {
                crate::serial_write_byte(b'0' + (count / 10) as u8);
            }
            crate::serial_write_byte(b'0' + (count % 10) as u8);
            crate::serial_write_str("\n");
        }
    }

    /// Add a simple reflex (helper)
    fn add_reflex_simple(
        &mut self,
        id: u32,
        name: &[u8],
        pattern: &[PatternElement],
        action: ReflexAction,
        action_arg: u32,
    ) {
        if self.reflex_count >= MAX_REFLEXES {
            return;
        }

        let mut reflex = Reflex::new(id, name);
        reflex.set_pattern(pattern);
        reflex.set_action(action, action_arg);
        reflex.priority = 128;

        self.reflexes[self.reflex_count] = reflex;
        self.reflex_count += 1;
    }

    /// Process an input event (called on every input)
    pub fn process_event(&mut self, event: InputEvent) -> Option<(ReflexAction, u32)> {
        if !self.enabled {
            return None;
        }

        self.stats.events_processed += 1;

        // Add to history
        let mut event = event;
        event.timestamp = self.frame;
        self.input_history[self.history_head] = event;
        self.history_head = (self.history_head + 1) % INPUT_HISTORY_SIZE;
        if self.history_len < INPUT_HISTORY_SIZE {
            self.history_len += 1;
        }

        // Run habit learning on the history
        self.check_for_habits();

        // Try to match reflexes
        self.try_match_reflexes(event)
    }

    /// Check history for repeated patterns (habit detection)
    fn check_for_habits(&mut self) {
        if !self.habit_learner.is_enabled() || self.history_len < 4 {
            return;
        }

        // Build a linear view of recent history for analysis
        // (convert ring buffer to linear array)
        let mut linear_history: [InputEvent; INPUT_HISTORY_SIZE] = self.input_history;
        if self.history_len < INPUT_HISTORY_SIZE {
            // History hasn't wrapped yet, it's already linear from index 0
        } else {
            // Unwrap the ring buffer
            let mut idx = 0;
            let start = self.history_head; // oldest item
            for i in 0..INPUT_HISTORY_SIZE {
                linear_history[idx] = self.input_history[(start + i) % INPUT_HISTORY_SIZE];
                idx += 1;
            }
        }

        // Analyze for patterns
        if let Some(candidate_idx) = self.habit_learner.analyze_history(
            &linear_history,
            self.history_len,
            self.frame_count,
        ) {
            // Mark as proposed
            self.habit_learner.mark_proposed(candidate_idx);

            // Send attention signal to System 2 about discovered habit
            if let Some(candidate) = self.habit_learner.get_candidate(candidate_idx) {
                // Encode pattern info in the attention signal
                let pattern_hash = candidate.pattern_hash;
                let occurrence_count = candidate.occurrence_count;

                self.send_attention(AttentionSignal {
                    signal_type: AttentionType::HabitDiscovered,
                    priority: 80,
                    data: pattern_hash,
                    context: ((candidate_idx as u64) << 32) | (occurrence_count as u64),
                    timestamp: self.frame,
                });

                #[cfg(feature = "serial_debug")]
                crate::serial_write_str("HABIT_DISCOVERED\n");

                // Auto-promote if enabled
                if self.auto_promote_habits {
                    if let Some(reflex) = self.habit_learner.promote_candidate(candidate_idx) {
                        let _ = self.add_reflex(reflex);
                        #[cfg(feature = "serial_debug")]
                        crate::serial_write_str("HABIT_AUTO_PROMOTED\n");
                    }
                }
            }
        }
    }

    /// Try to match the current event against all reflexes
    fn try_match_reflexes(&mut self, event: InputEvent) -> Option<(ReflexAction, u32)> {
        // Check if current match candidate is still valid
        if self.match_progress > 0 {
            let candidate_idx = self.match_candidate;
            let candidate = &self.reflexes[candidate_idx];

            if candidate.enabled && (self.match_progress as u8) < candidate.pattern_len {
                let pattern_elem = candidate.pattern[self.match_progress];
                let candidate_id = candidate.id;
                let candidate_action = candidate.action;
                let candidate_arg = candidate.action_arg;
                let candidate_len = candidate.pattern_len;

                // Check timing constraint
                let gap = self.frame.saturating_sub(self.match_last_timestamp);
                if pattern_elem.max_gap > 0 && gap > pattern_elem.max_gap as u32 {
                    // Timeout, reset match
                    self.match_progress = 0;
                } else if event.matches(&pattern_elem) {
                    // Matched next element
                    self.match_progress += 1;
                    self.match_last_timestamp = self.frame;

                    // Check if complete
                    if self.match_progress as u8 >= candidate_len {
                        let action = candidate_action;
                        let arg = candidate_arg;

                        // Update reflex stats
                        self.reflexes[candidate_idx].fire_count += 1;
                        self.reflexes[candidate_idx].last_fired = self.frame;
                        self.stats.reflexes_fired += 1;

                        // Signal to System 2
                        self.send_attention(AttentionSignal {
                            signal_type: AttentionType::ReflexFired,
                            priority: 64,
                            data: candidate_id,
                            context: 0,
                            timestamp: self.frame,
                        });

                        // Reset for next match
                        self.match_progress = 0;
                        return Some((action, arg));
                    }
                    return None;
                }
            }
            // Candidate didn't match, reset
            self.match_progress = 0;
        }

        // Try to start a new match with any reflex
        for i in 0..self.reflex_count {
            let reflex = &self.reflexes[i];
            if !reflex.enabled || reflex.pattern_len == 0 {
                continue;
            }

            self.stats.patterns_checked += 1;

            // Check first pattern element
            if event.matches(&reflex.pattern[0]) {
                if reflex.pattern_len == 1 {
                    // Single-element pattern, fire immediately
                    let action = reflex.action;
                    let arg = reflex.action_arg;
                    let reflex_id = reflex.id;
                    let frame = self.frame;

                    self.reflexes[i].fire_count += 1;
                    self.reflexes[i].last_fired = frame;
                    self.stats.reflexes_fired += 1;

                    self.send_attention(AttentionSignal {
                        signal_type: AttentionType::ReflexFired,
                        priority: 64,
                        data: reflex_id,
                        context: 0,
                        timestamp: frame,
                    });

                    return Some((action, arg));
                } else {
                    // Multi-element pattern, start tracking
                    self.match_candidate = i;
                    self.match_progress = 1;
                    self.match_last_timestamp = self.frame;
                    return None;
                }
            }
        }

        // No reflex matched - check if this is an unknown gesture
        if matches!(
            event.event_type,
            InputEventType::GestureSwipeLeft
                | InputEventType::GestureSwipeRight
                | InputEventType::GestureSwipeUp
                | InputEventType::GestureSwipeDown
                | InputEventType::GesturePinchIn
                | InputEventType::GesturePinchOut
        ) {
            self.stats.unknown_gestures += 1;
            self.send_attention(AttentionSignal {
                signal_type: AttentionType::UnknownGesture,
                priority: 96,
                data: event.event_type as u32,
                context: ((event.x as u64) << 16) | (event.y as u64 & 0xFFFF),
                timestamp: self.frame,
            });
        }

        None
    }

    /// Send an attention signal to System 2
    fn send_attention(&mut self, signal: AttentionSignal) {
        if self.attention_count >= MAX_ATTENTION_SIGNALS {
            // Drop oldest
            for i in 0..(MAX_ATTENTION_SIGNALS - 1) {
                self.attention_queue[i] = self.attention_queue[i + 1];
            }
            self.attention_count = MAX_ATTENTION_SIGNALS - 1;
        }

        self.attention_queue[self.attention_count] = signal;
        self.attention_count += 1;
        self.stats.attention_signals_sent += 1;
    }

    /// Pop an attention signal (for System 2 to consume)
    pub fn pop_attention(&mut self) -> Option<AttentionSignal> {
        if self.attention_count == 0 {
            return None;
        }

        let signal = self.attention_queue[0];
        for i in 0..(self.attention_count - 1) {
            self.attention_queue[i] = self.attention_queue[i + 1];
        }
        self.attention_count -= 1;
        Some(signal)
    }

    /// Tick the engine (called each frame)
    pub fn tick(&mut self) {
        self.frame += 1;
        self.frame_count += 1;

        // Check for pattern timeout
        if self.match_progress > 0 {
            let candidate = &self.reflexes[self.match_candidate];
            if candidate.enabled && (self.match_progress as u8) < candidate.pattern_len {
                let pattern_elem = &candidate.pattern[self.match_progress];
                let gap = self.frame.saturating_sub(self.match_last_timestamp);
                if pattern_elem.max_gap > 0 && gap > pattern_elem.max_gap as u32 {
                    self.match_progress = 0;
                }
            }
        }
    }

    /// Add a new reflex (called by System 2 for learning)
    pub fn add_reflex(&mut self, reflex: Reflex) -> bool {
        if self.reflex_count >= MAX_REFLEXES {
            return false;
        }

        // Check for duplicate ID
        for i in 0..self.reflex_count {
            if self.reflexes[i].id == reflex.id {
                // Update existing
                self.reflexes[i] = reflex;
                return true;
            }
        }

        self.reflexes[self.reflex_count] = reflex;
        self.reflex_count += 1;
        true
    }

    /// Remove a reflex by ID
    pub fn remove_reflex(&mut self, id: u32) -> bool {
        for i in 0..self.reflex_count {
            if self.reflexes[i].id == id {
                // Shift remaining
                for j in i..(self.reflex_count - 1) {
                    self.reflexes[j] = self.reflexes[j + 1];
                }
                self.reflex_count -= 1;
                return true;
            }
        }
        false
    }

    /// Enable/disable a reflex
    pub fn set_reflex_enabled(&mut self, id: u32, enabled: bool) -> bool {
        for i in 0..self.reflex_count {
            if self.reflexes[i].id == id {
                self.reflexes[i].enabled = enabled;
                return true;
            }
        }
        false
    }

    /// Get reflex count
    pub fn reflex_count(&self) -> usize {
        self.reflex_count
    }

    /// Get a reflex by index
    pub fn get_reflex(&self, index: usize) -> Option<&Reflex> {
        if index < self.reflex_count {
            Some(&self.reflexes[index])
        } else {
            None
        }
    }

    /// Enable/disable the engine
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Clear all learned (non-default) reflexes
    pub fn clear_learned_reflexes(&mut self) -> usize {
        let mut removed = 0;
        let mut i = 0;
        while i < self.reflex_count {
            if self.reflexes[i].learned {
                // Shift remaining reflexes down
                for j in i..self.reflex_count.saturating_sub(1) {
                    self.reflexes[j] = self.reflexes[j + 1];
                }
                self.reflex_count = self.reflex_count.saturating_sub(1);
                removed += 1;
                // Don't increment i, check the shifted item
            } else {
                i += 1;
            }
        }
        removed
    }

    /// Get reflex by ID
    pub fn get_reflex_by_id(&self, id: u32) -> Option<&Reflex> {
        for i in 0..self.reflex_count {
            if self.reflexes[i].id == id {
                return Some(&self.reflexes[i]);
            }
        }
        None
    }

    /// Set reflex priority
    pub fn set_reflex_priority(&mut self, id: u32, priority: u8) -> bool {
        for i in 0..self.reflex_count {
            if self.reflexes[i].id == id {
                self.reflexes[i].priority = priority;
                return true;
            }
        }
        false
    }

    /// Count learned reflexes
    pub fn learned_reflex_count(&self) -> usize {
        let mut count = 0;
        for i in 0..self.reflex_count {
            if self.reflexes[i].learned {
                count += 1;
            }
        }
        count
    }

    /// Suppress notifications for a duration
    pub fn suppress_notifications(&mut self, duration_frames: u32) {
        self.suppress_until_frame = self.frame_count + duration_frames as u64;
    }

    /// Check if notifications are suppressed
    pub fn notifications_suppressed(&self) -> bool {
        self.frame_count < self.suppress_until_frame
    }

    /// Boost attention for a target type
    pub fn boost_attention(&mut self, target_type: u8, weight: u8) {
        // Store attention weights in a compact way
        // target_type: 0=mouse, 1=keyboard, 2=gesture, 3=voice, 4=gaze
        if target_type < 8 {
            self.attention_weights[target_type as usize] = weight;
        }
    }

    /// Reset attention weights to default
    pub fn reset_attention_weights(&mut self) {
        for i in 0..8 {
            self.attention_weights[i] = 100; // Default weight
        }
    }
}

// ============================================================================
// GLOBAL INSTANCE
// ============================================================================

static mut REFLEX_ENGINE: ReflexEngine = ReflexEngine::new();

/// Initialize the global reflex engine
pub fn init() {
    unsafe {
        REFLEX_ENGINE.init();
    }
}

/// Process an input event through the reflex engine
pub fn process_event(event: InputEvent) -> Option<(ReflexAction, u32)> {
    unsafe { REFLEX_ENGINE.process_event(event) }
}

/// Tick the reflex engine
pub fn tick() {
    unsafe {
        REFLEX_ENGINE.tick();
    }
}

/// Pop an attention signal for System 2
pub fn pop_attention() -> Option<AttentionSignal> {
    unsafe { REFLEX_ENGINE.pop_attention() }
}

/// Add a learned reflex
pub fn add_reflex(reflex: Reflex) -> bool {
    unsafe { REFLEX_ENGINE.add_reflex(reflex) }
}

/// Get engine statistics
pub fn stats() -> ReflexStats {
    unsafe { REFLEX_ENGINE.stats }
}

/// Get reflex count
pub fn reflex_count() -> usize {
    unsafe { REFLEX_ENGINE.reflex_count() }
}

/// Get a reflex by index
pub fn get_reflex(index: usize) -> Option<Reflex> {
    unsafe { REFLEX_ENGINE.get_reflex(index).copied() }
}

/// Remove a reflex by ID
pub fn remove_reflex(id: u32) -> bool {
    unsafe { REFLEX_ENGINE.remove_reflex(id) }
}

/// Enable or disable a reflex
pub fn set_reflex_enabled(id: u32, enabled: bool) -> bool {
    unsafe { REFLEX_ENGINE.set_reflex_enabled(id, enabled) }
}

/// Enable or disable the entire reflex engine
pub fn set_enabled(enabled: bool) {
    unsafe { REFLEX_ENGINE.set_enabled(enabled) }
}

/// Check if engine is enabled
pub fn is_enabled() -> bool {
    unsafe { REFLEX_ENGINE.is_enabled() }
}

/// Clear all learned (non-default) reflexes
pub fn clear_learned_reflexes() -> usize {
    unsafe { REFLEX_ENGINE.clear_learned_reflexes() }
}

/// Get reflex by ID
pub fn get_reflex_by_id(id: u32) -> Option<Reflex> {
    unsafe { REFLEX_ENGINE.get_reflex_by_id(id).copied() }
}

/// Update reflex priority
pub fn set_reflex_priority(id: u32, priority: u8) -> bool {
    unsafe { REFLEX_ENGINE.set_reflex_priority(id, priority) }
}

// ============================================================================
// SYSTEM 2 CONTROL COMMANDS (Downward Commands)
// ============================================================================

/// Commands that System 2 can issue to control System 1
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ControlCommand {
    /// No operation
    Nop = 0,
    /// Enable the reflex engine
    EnableEngine = 1,
    /// Disable the reflex engine
    DisableEngine = 2,
    /// Add a new reflex
    AddReflex = 3,
    /// Remove a reflex by ID
    RemoveReflex = 4,
    /// Enable a specific reflex
    EnableReflex = 5,
    /// Disable a specific reflex
    DisableReflex = 6,
    /// Update reflex priority
    SetPriority = 7,
    /// Clear all learned reflexes
    ClearLearned = 8,
    /// Suppress notifications for duration (frames)
    SuppressNotifications = 9,
    /// Increase attention weight for a target
    BoostAttention = 10,
    /// Reset attention weights to default
    ResetAttention = 11,
    /// Enable habit learning
    EnableLearning = 12,
    /// Disable habit learning
    DisableLearning = 13,
    /// Enable auto-promotion of habits
    EnableAutoPromote = 14,
    /// Disable auto-promotion of habits
    DisableAutoPromote = 15,
    /// Promote a habit candidate to a reflex
    PromoteHabit = 16,
    /// Reject a habit candidate
    RejectHabit = 17,
    /// Clear all habit candidates
    ClearHabits = 18,
}

/// A control command with parameters
#[derive(Clone, Copy)]
pub struct System2Command {
    /// The command type
    pub command: ControlCommand,
    /// Primary argument (reflex ID, duration, etc.)
    pub arg1: u32,
    /// Secondary argument (priority, action type, etc.)
    pub arg2: u32,
    /// Tertiary argument (action arg, etc.)
    pub arg3: u32,
}

impl System2Command {
    pub const fn new(command: ControlCommand) -> Self {
        Self {
            command,
            arg1: 0,
            arg2: 0,
            arg3: 0,
        }
    }

    pub const fn with_args(mut self, arg1: u32, arg2: u32, arg3: u32) -> Self {
        self.arg1 = arg1;
        self.arg2 = arg2;
        self.arg3 = arg3;
        self
    }
}

/// Result of executing a control command
#[derive(Clone, Copy, Debug)]
pub struct CommandResult {
    pub success: bool,
    pub reflex_id: u32,
    pub message_code: u8,
}

impl CommandResult {
    pub const fn ok(reflex_id: u32) -> Self {
        Self { success: true, reflex_id, message_code: 0 }
    }

    pub const fn err(code: u8) -> Self {
        Self { success: false, reflex_id: 0, message_code: code }
    }
}

/// Message codes for command results
pub mod message_codes {
    pub const OK: u8 = 0;
    pub const ENGINE_DISABLED: u8 = 1;
    pub const REFLEX_NOT_FOUND: u8 = 2;
    pub const REFLEX_LIMIT_REACHED: u8 = 3;
    pub const INVALID_PATTERN: u8 = 4;
    pub const INVALID_ACTION: u8 = 5;
    pub const DUPLICATE_ID: u8 = 6;
}

/// Execute a control command from System 2
pub fn execute_command(cmd: System2Command) -> CommandResult {
    match cmd.command {
        ControlCommand::Nop => CommandResult::ok(0),

        ControlCommand::EnableEngine => {
            set_enabled(true);
            #[cfg(feature = "serial_debug")]
            crate::serial_write_str("S2CMD:enable_engine\n");
            CommandResult::ok(0)
        }

        ControlCommand::DisableEngine => {
            set_enabled(false);
            #[cfg(feature = "serial_debug")]
            crate::serial_write_str("S2CMD:disable_engine\n");
            CommandResult::ok(0)
        }

        ControlCommand::RemoveReflex => {
            let id = cmd.arg1;
            if remove_reflex(id) {
                #[cfg(feature = "serial_debug")]
                crate::serial_write_str("S2CMD:remove_reflex:ok\n");
                CommandResult::ok(id)
            } else {
                CommandResult::err(message_codes::REFLEX_NOT_FOUND)
            }
        }

        ControlCommand::EnableReflex => {
            let id = cmd.arg1;
            if set_reflex_enabled(id, true) {
                CommandResult::ok(id)
            } else {
                CommandResult::err(message_codes::REFLEX_NOT_FOUND)
            }
        }

        ControlCommand::DisableReflex => {
            let id = cmd.arg1;
            if set_reflex_enabled(id, false) {
                CommandResult::ok(id)
            } else {
                CommandResult::err(message_codes::REFLEX_NOT_FOUND)
            }
        }

        ControlCommand::SetPriority => {
            let id = cmd.arg1;
            let priority = cmd.arg2 as u8;
            if set_reflex_priority(id, priority) {
                CommandResult::ok(id)
            } else {
                CommandResult::err(message_codes::REFLEX_NOT_FOUND)
            }
        }

        ControlCommand::ClearLearned => {
            let count = clear_learned_reflexes();
            #[cfg(feature = "serial_debug")]
            crate::serial_write_str("S2CMD:clear_learned\n");
            CommandResult::ok(count as u32)
        }

        ControlCommand::SuppressNotifications => {
            let duration = cmd.arg1;
            unsafe { REFLEX_ENGINE.suppress_notifications(duration) };
            CommandResult::ok(duration)
        }

        ControlCommand::BoostAttention => {
            // arg1 = target type, arg2 = weight boost
            unsafe { REFLEX_ENGINE.boost_attention(cmd.arg1 as u8, cmd.arg2 as u8) };
            CommandResult::ok(0)
        }

        ControlCommand::ResetAttention => {
            unsafe { REFLEX_ENGINE.reset_attention_weights() };
            CommandResult::ok(0)
        }

        ControlCommand::AddReflex => {
            // For AddReflex, use the builder functions below
            CommandResult::err(message_codes::INVALID_PATTERN)
        }

        ControlCommand::EnableLearning => {
            set_learning_enabled(true);
            #[cfg(feature = "serial_debug")]
            crate::serial_write_str("S2CMD:enable_learning\n");
            CommandResult::ok(0)
        }

        ControlCommand::DisableLearning => {
            set_learning_enabled(false);
            #[cfg(feature = "serial_debug")]
            crate::serial_write_str("S2CMD:disable_learning\n");
            CommandResult::ok(0)
        }

        ControlCommand::EnableAutoPromote => {
            set_auto_promote(true);
            #[cfg(feature = "serial_debug")]
            crate::serial_write_str("S2CMD:enable_auto_promote\n");
            CommandResult::ok(0)
        }

        ControlCommand::DisableAutoPromote => {
            set_auto_promote(false);
            CommandResult::ok(0)
        }

        ControlCommand::PromoteHabit => {
            let idx = cmd.arg1 as usize;
            if let Some(reflex_id) = promote_habit(idx) {
                #[cfg(feature = "serial_debug")]
                crate::serial_write_str("S2CMD:promote_habit:ok\n");
                CommandResult::ok(reflex_id)
            } else {
                CommandResult::err(message_codes::REFLEX_NOT_FOUND)
            }
        }

        ControlCommand::RejectHabit => {
            let idx = cmd.arg1 as usize;
            if reject_habit(idx) {
                CommandResult::ok(0)
            } else {
                CommandResult::err(message_codes::REFLEX_NOT_FOUND)
            }
        }

        ControlCommand::ClearHabits => {
            clear_habit_candidates();
            CommandResult::ok(0)
        }
    }
}

/// Create and add a simple key-based reflex from System 2
///
/// # Arguments
/// * `id` - Unique reflex ID (use 1000+ for learned reflexes)
/// * `keycode` - The key that triggers this reflex
/// * `modifiers` - Modifier keys (0x01=Ctrl, 0x02=Shift, 0x04=Alt, 0x08=Meta)
/// * `action` - Action to perform
/// * `action_arg` - Argument for the action
pub fn add_key_reflex(
    id: u32,
    keycode: u8,
    modifiers: u8,
    action: ReflexAction,
    action_arg: u32,
) -> CommandResult {
    let mut reflex = Reflex::new(id, b"learned");
    reflex.pattern[0] = PatternElement::new(InputEventType::KeyPress)
        .with_data(keycode as u16)
        .with_modifiers(modifiers);
    reflex.pattern_len = 1;
    reflex.action = action;
    reflex.action_arg = action_arg;
    reflex.learned = true;
    reflex.enabled = true;

    if add_reflex(reflex) {
        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("S2CMD:add_key_reflex:ok\n");
        CommandResult::ok(id)
    } else {
        CommandResult::err(message_codes::REFLEX_LIMIT_REACHED)
    }
}

/// Create and add a gesture-based reflex from System 2
pub fn add_gesture_reflex(
    id: u32,
    gesture: InputEventType,
    action: ReflexAction,
    action_arg: u32,
) -> CommandResult {
    // Validate it's a gesture type
    let is_gesture = matches!(
        gesture,
        InputEventType::GestureSwipeLeft
            | InputEventType::GestureSwipeRight
            | InputEventType::GestureSwipeUp
            | InputEventType::GestureSwipeDown
            | InputEventType::GesturePinchIn
            | InputEventType::GesturePinchOut
            | InputEventType::GestureRotate
    );

    if !is_gesture {
        return CommandResult::err(message_codes::INVALID_PATTERN);
    }

    let mut reflex = Reflex::new(id, b"gesture");
    reflex.pattern[0] = PatternElement::new(gesture);
    reflex.pattern_len = 1;
    reflex.action = action;
    reflex.action_arg = action_arg;
    reflex.learned = true;
    reflex.enabled = true;

    if add_reflex(reflex) {
        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("S2CMD:add_gesture_reflex:ok\n");
        CommandResult::ok(id)
    } else {
        CommandResult::err(message_codes::REFLEX_LIMIT_REACHED)
    }
}

/// Create and add a multi-tap reflex (e.g., double-tap, triple-tap)
pub fn add_multi_tap_reflex(
    id: u32,
    tap_count: u8,
    max_gap_frames: u16,
    action: ReflexAction,
    action_arg: u32,
) -> CommandResult {
    if tap_count < 2 || tap_count as usize > MAX_PATTERN_LEN {
        return CommandResult::err(message_codes::INVALID_PATTERN);
    }

    let mut reflex = Reflex::new(id, b"multi_tap");
    for i in 0..tap_count as usize {
        reflex.pattern[i] = PatternElement::new(InputEventType::MouseClick)
            .with_max_gap(max_gap_frames);
    }
    reflex.pattern_len = tap_count;
    reflex.action = action;
    reflex.action_arg = action_arg;
    reflex.learned = true;
    reflex.enabled = true;

    if add_reflex(reflex) {
        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("S2CMD:add_multi_tap:ok\n");
        CommandResult::ok(id)
    } else {
        CommandResult::err(message_codes::REFLEX_LIMIT_REACHED)
    }
}

/// Create and add a key sequence reflex (e.g., Konami code)
pub fn add_key_sequence_reflex(
    id: u32,
    keycodes: &[u8],
    max_gap_frames: u16,
    action: ReflexAction,
    action_arg: u32,
) -> CommandResult {
    if keycodes.is_empty() || keycodes.len() > MAX_PATTERN_LEN {
        return CommandResult::err(message_codes::INVALID_PATTERN);
    }

    let mut reflex = Reflex::new(id, b"sequence");
    for (i, &keycode) in keycodes.iter().enumerate() {
        reflex.pattern[i] = PatternElement::new(InputEventType::KeyPress)
            .with_data(keycode as u16)
            .with_max_gap(max_gap_frames);
    }
    reflex.pattern_len = keycodes.len() as u8;
    reflex.action = action;
    reflex.action_arg = action_arg;
    reflex.learned = true;
    reflex.enabled = true;

    if add_reflex(reflex) {
        #[cfg(feature = "serial_debug")]
        crate::serial_write_str("S2CMD:add_sequence:ok\n");
        CommandResult::ok(id)
    } else {
        CommandResult::err(message_codes::REFLEX_LIMIT_REACHED)
    }
}

/// List all active reflexes (returns count and fills buffer)
pub fn list_reflexes(buffer: &mut [u32], max_count: usize) -> usize {
    let count = reflex_count().min(max_count).min(buffer.len());
    for i in 0..count {
        if let Some(reflex) = get_reflex(i) {
            buffer[i] = reflex.id;
        }
    }
    count
}

/// Get a summary of reflex engine state
#[derive(Clone, Copy)]
pub struct EngineState {
    pub enabled: bool,
    pub reflex_count: usize,
    pub learned_count: usize,
    pub events_processed: u64,
    pub reflexes_fired: u64,
    pub attention_pending: usize,
    pub notification_suppressed: bool,
}

pub fn get_engine_state() -> EngineState {
    unsafe {
        EngineState {
            enabled: REFLEX_ENGINE.is_enabled(),
            reflex_count: REFLEX_ENGINE.reflex_count(),
            learned_count: REFLEX_ENGINE.learned_reflex_count(),
            events_processed: REFLEX_ENGINE.stats.events_processed,
            reflexes_fired: REFLEX_ENGINE.stats.reflexes_fired,
            attention_pending: REFLEX_ENGINE.attention_count,
            notification_suppressed: REFLEX_ENGINE.notifications_suppressed(),
        }
    }
}

// ============================================================================
// HABIT LEARNING API (System 2 Control)
// ============================================================================

/// Enable or disable habit learning
pub fn set_learning_enabled(enabled: bool) {
    unsafe { REFLEX_ENGINE.habit_learner.set_enabled(enabled) }
}

/// Check if habit learning is enabled
pub fn is_learning_enabled() -> bool {
    unsafe { REFLEX_ENGINE.habit_learner.is_enabled() }
}

/// Enable auto-promotion of habits to reflexes
pub fn set_auto_promote(enabled: bool) {
    unsafe { REFLEX_ENGINE.auto_promote_habits = enabled }
}

/// Check if auto-promotion is enabled
pub fn is_auto_promote_enabled() -> bool {
    unsafe { REFLEX_ENGINE.auto_promote_habits }
}

/// Get the number of active habit candidates
pub fn habit_candidate_count() -> usize {
    unsafe { REFLEX_ENGINE.habit_learner.active_candidate_count() }
}

/// Get habit learning statistics
pub fn habit_stats() -> HabitStats {
    unsafe { REFLEX_ENGINE.habit_learner.stats }
}

/// Manually promote a habit candidate to a reflex
/// Returns the new reflex ID if successful
pub fn promote_habit(candidate_idx: usize) -> Option<u32> {
    unsafe {
        if let Some(reflex) = REFLEX_ENGINE.habit_learner.promote_candidate(candidate_idx) {
            let id = reflex.id;
            if REFLEX_ENGINE.add_reflex(reflex) {
                return Some(id);
            }
        }
        None
    }
}

/// Reject a habit candidate (remove it from tracking)
pub fn reject_habit(candidate_idx: usize) -> bool {
    unsafe {
        if candidate_idx < MAX_HABIT_CANDIDATES {
            if REFLEX_ENGINE.habit_learner.candidates[candidate_idx].active {
                REFLEX_ENGINE.habit_learner.candidates[candidate_idx].active = false;
                return true;
            }
        }
        false
    }
}

/// Clear all habit candidates
pub fn clear_habit_candidates() {
    unsafe { REFLEX_ENGINE.habit_learner.clear() }
}

/// Get info about a habit candidate
#[derive(Clone, Copy)]
pub struct HabitInfo {
    pub pattern_hash: u32,
    pub pattern_len: u8,
    pub occurrence_count: u32,
    pub last_seen_frame: u64,
    pub suggested_action: ReflexAction,
    pub proposed: bool,
}

pub fn get_habit_info(candidate_idx: usize) -> Option<HabitInfo> {
    unsafe {
        if let Some(candidate) = REFLEX_ENGINE.habit_learner.get_candidate(candidate_idx) {
            Some(HabitInfo {
                pattern_hash: candidate.pattern_hash,
                pattern_len: candidate.pattern_len,
                occurrence_count: candidate.occurrence_count,
                last_seen_frame: candidate.last_seen_frame,
                suggested_action: candidate.suggested_action,
                proposed: candidate.proposed,
            })
        } else {
            None
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        let mut engine = ReflexEngine::new();
        engine.init();

        // Simulate Ctrl+C
        let event = InputEvent::new(InputEventType::KeyPress, b'C' as u16).with_modifiers(0x01);
        let result = engine.process_event(event);

        assert!(result.is_some());
        let (action, _) = result.unwrap();
        assert_eq!(action, ReflexAction::Copy);
    }

    #[test]
    fn test_multi_element_pattern() {
        let mut engine = ReflexEngine::new();

        // Add a double-tap pattern
        engine.add_reflex_simple(
            100,
            b"double_tap",
            &[
                PatternElement::new(InputEventType::MouseClick).with_max_gap(15),
                PatternElement::new(InputEventType::MouseClick),
            ],
            ReflexAction::ShowLauncher,
            0,
        );

        // First tap
        let event1 = InputEvent::new(InputEventType::MouseClick, 0);
        let result1 = engine.process_event(event1);
        assert!(result1.is_none()); // Waiting for second

        // Second tap (within gap)
        engine.frame = 10;
        let event2 = InputEvent::new(InputEventType::MouseClick, 0);
        let result2 = engine.process_event(event2);
        assert!(result2.is_some());
        assert_eq!(result2.unwrap().0, ReflexAction::ShowLauncher);
    }
}
