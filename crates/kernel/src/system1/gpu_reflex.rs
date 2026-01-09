//! GPU Reflex Engine - Pattern Matching on GPU Compute Shaders
//!
//! Migrates the CPU-based reflex pattern matching to persistent GPU compute shaders.
//! This provides sub-millisecond latency for input processing by running all reflex
//! pattern matching in parallel on the GPU.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                      GPU Reflex Engine                                   │
//! │                                                                          │
//! │  ┌───────────────┐   ┌───────────────┐   ┌───────────────┐              │
//! │  │ Input Buffer  │   │ Reflex Buffer │   │ Output Buffer │              │
//! │  │ (Ring Buffer) │   │ (64 patterns) │   │ (Matched IDs) │              │
//! │  └───────────────┘   └───────────────┘   └───────────────┘              │
//! │          │                   │                   ▲                       │
//! │          └───────────────────┼───────────────────┘                       │
//! │                              ▼                                           │
//! │  ┌─────────────────────────────────────────────────────────────────┐    │
//! │  │                    Pattern Match Shader                          │    │
//! │  │  • One workgroup per reflex pattern                              │    │
//! │  │  • Parallel sliding window over input history                    │    │
//! │  │  • Atomic output of matched reflex IDs                           │    │
//! │  └─────────────────────────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_kernel::system1::gpu_reflex::{GpuReflexEngine, GpuInputEvent, GpuReflex};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let device = ...; // wgpu device
//!     let queue = ...;  // wgpu queue
//!
//!     let mut engine = GpuReflexEngine::new();
//!     engine.initialize(&device).await?;
//!
//!     // Add reflexes
//!     engine.add_reflex(GpuReflex::ctrl_w_close());
//!
//!     // Process input events
//!     engine.push_event(event);
//!
//!     // Run pattern matching on GPU
//!     let matches = engine.dispatch(&device, &queue).await?;
//!
//!     for match_id in matches {
//!         // Execute matched reflex action
//!     }
//!
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use std::sync::Arc;
use parking_lot::Mutex;

// =============================================================================
// Configuration Constants
// =============================================================================

/// Maximum number of reflexes
pub const MAX_GPU_REFLEXES: usize = 64;

/// Maximum pattern length per reflex
pub const MAX_PATTERN_LEN: usize = 8;

/// Input history ring buffer size
pub const INPUT_HISTORY_SIZE: usize = 64;

/// Maximum matches per dispatch
pub const MAX_MATCHES: usize = 16;

// =============================================================================
// GPU Shader
// =============================================================================

/// WGSL compute shader for GPU pattern matching
pub const GPU_REFLEX_SHADER: &str = r#"
// GPU Reflex Engine - Parallel Pattern Matching
//
// Architecture:
// - Each thread group handles one reflex pattern
// - Threads within the group scan the input history in parallel
// - Atomic operations used for thread-safe match reporting

// Input event representation (matches CPU struct layout)
struct GpuInputEvent {
    event_type: u32,    // InputEventType as u32
    modifiers: u32,     // Modifier keys bitmap
    data: u32,          // Primary data (keycode, button)
    timestamp: u32,     // Frame timestamp
}

// Pattern element for matching
struct PatternElement {
    event_type: u32,
    data: u32,
    modifiers: u32,
    require_data: u32,      // bool as u32
    require_modifiers: u32, // bool as u32
    max_gap: u32,           // Max frames between events
    _pad0: u32,
    _pad1: u32,
}

// Complete reflex definition
struct GpuReflex {
    id: u32,
    pattern_len: u32,
    action: u32,
    action_arg: u32,
    priority: u32,
    enabled: u32,
    _pad0: u32,
    _pad1: u32,
    pattern: array<PatternElement, 8>,  // MAX_PATTERN_LEN = 8
}

// Configuration and statistics
struct ReflexConfig {
    reflex_count: u32,
    history_len: u32,
    history_head: u32,
    current_frame: u32,
}

// Match output
struct MatchResult {
    count: atomic<u32>,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    matches: array<u32, 16>,  // MAX_MATCHES = 16
}

// Buffer bindings
@group(0) @binding(0)
var<storage, read> input_history: array<GpuInputEvent, 64>;  // INPUT_HISTORY_SIZE

@group(0) @binding(1)
var<storage, read> reflexes: array<GpuReflex, 64>;  // MAX_GPU_REFLEXES

@group(0) @binding(2)
var<storage, read> config: ReflexConfig;

@group(0) @binding(3)
var<storage, read_write> match_result: MatchResult;

// Workgroup shared memory for collaborative matching
var<workgroup> pattern_matched: array<u32, 8>;  // Track which pattern elements matched

// Check if an input event matches a pattern element
fn event_matches_pattern(event: GpuInputEvent, pattern: PatternElement) -> bool {
    // Event type must match
    if (event.event_type != pattern.event_type) {
        return false;
    }

    // Check data if required
    if (pattern.require_data != 0u && event.data != pattern.data) {
        return false;
    }

    // Check modifiers if required
    if (pattern.require_modifiers != 0u && event.modifiers != pattern.modifiers) {
        return false;
    }

    return true;
}

// Get event from ring buffer
fn get_history_event(idx: u32) -> GpuInputEvent {
    let actual_idx = idx % 64u;  // INPUT_HISTORY_SIZE
    return input_history[actual_idx];
}

// Main compute shader - pattern matching
@compute @workgroup_size(64)
fn reflex_match_main(@builtin(global_invocation_id) global_id: vec3<u32>,
                     @builtin(local_invocation_id) local_id: vec3<u32>,
                     @builtin(workgroup_id) workgroup_id: vec3<u32>) {
    let thread_id = local_id.x;
    let reflex_idx = workgroup_id.x;

    // Early exit if this reflex doesn't exist or is disabled
    if (reflex_idx >= config.reflex_count) {
        return;
    }

    let reflex = reflexes[reflex_idx];
    if (reflex.enabled == 0u || reflex.pattern_len == 0u) {
        return;
    }

    let pattern_len = reflex.pattern_len;
    let history_len = config.history_len;

    // Not enough history to match pattern
    if (history_len < pattern_len) {
        return;
    }

    // Initialize shared memory
    if (thread_id < 8u) {
        pattern_matched[thread_id] = 0u;
    }
    workgroupBarrier();

    // Each thread checks one starting position in history
    // Thread 0 checks most recent events (end of history)
    let max_start_positions = min(64u, history_len - pattern_len + 1u);

    if (thread_id < max_start_positions) {
        // Calculate starting position (most recent first)
        let start_pos = history_len - pattern_len - thread_id;

        // Try to match the pattern starting at this position
        var matched = true;
        var prev_timestamp = 0u;

        for (var i = 0u; i < pattern_len; i = i + 1u) {
            let event_idx = start_pos + i;
            let event = get_history_event(config.history_head - history_len + event_idx);
            let pattern = reflex.pattern[i];

            // Check event match
            if (!event_matches_pattern(event, pattern)) {
                matched = false;
                break;
            }

            // Check timing constraint
            if (i > 0u && pattern.max_gap > 0u) {
                let gap = event.timestamp - prev_timestamp;
                if (gap > pattern.max_gap) {
                    matched = false;
                    break;
                }
            }

            prev_timestamp = event.timestamp;
        }

        // If this thread found a match, record it
        if (matched) {
            // Use atomic to claim a slot in the match array
            let match_idx = atomicAdd(&match_result.count, 1u);
            if (match_idx < 16u) {
                match_result.matches[match_idx] = reflex.id;
            }
        }
    }
}

// Secondary shader: Gesture recognition (runs after pattern matching)
@compute @workgroup_size(256)
fn gesture_detect_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let thread_id = global_id.x;

    // Gesture detection logic
    // Analyzes motion patterns across recent events

    // TODO: Implement gesture detection algorithms
    // - Swipe detection (consecutive moves in same direction)
    // - Pinch detection (two-finger convergence)
    // - Rotate detection (circular motion)
}
"#;

// =============================================================================
// CPU-Side Types (matching GPU layout)
// =============================================================================

/// Input event for GPU processing
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuInputEvent {
    /// Event type (InputEventType as u32)
    pub event_type: u32,
    /// Modifier keys (shift=1, ctrl=2, alt=4, meta=8)
    pub modifiers: u32,
    /// Primary data (keycode, button, etc.)
    pub data: u32,
    /// Frame timestamp
    pub timestamp: u32,
}

impl GpuInputEvent {
    pub const NONE: u32 = 0;
    pub const KEY_PRESS: u32 = 9;
    pub const KEY_DOWN: u32 = 7;
    pub const KEY_UP: u32 = 8;
    pub const MOUSE_CLICK: u32 = 4;
    pub const MOUSE_DOUBLE_CLICK: u32 = 5;
    pub const MOUSE_TRIPLE_CLICK: u32 = 6;

    pub const MOD_SHIFT: u32 = 1;
    pub const MOD_CTRL: u32 = 2;
    pub const MOD_ALT: u32 = 4;
    pub const MOD_META: u32 = 8;

    /// Create a key press event
    pub fn key_press(key: u8, modifiers: u32, timestamp: u32) -> Self {
        Self {
            event_type: Self::KEY_PRESS,
            modifiers,
            data: key as u32,
            timestamp,
        }
    }

    /// Create a mouse click event
    pub fn mouse_click(button: u8, timestamp: u32) -> Self {
        Self {
            event_type: Self::MOUSE_CLICK,
            modifiers: 0,
            data: button as u32,
            timestamp,
        }
    }
}

/// Pattern element for GPU
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPatternElement {
    pub event_type: u32,
    pub data: u32,
    pub modifiers: u32,
    pub require_data: u32,
    pub require_modifiers: u32,
    pub max_gap: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

impl GpuPatternElement {
    /// Create a pattern element that matches an event type
    pub fn event(event_type: u32) -> Self {
        Self {
            event_type,
            data: 0,
            modifiers: 0,
            require_data: 0,
            require_modifiers: 0,
            max_gap: 30, // ~500ms at 60fps
            _pad0: 0,
            _pad1: 0,
        }
    }

    /// Require specific key/button data
    pub fn with_data(mut self, data: u32) -> Self {
        self.data = data;
        self.require_data = 1;
        self
    }

    /// Require specific modifiers
    pub fn with_modifiers(mut self, modifiers: u32) -> Self {
        self.modifiers = modifiers;
        self.require_modifiers = 1;
        self
    }

    /// Set maximum gap between events (in frames)
    pub fn with_max_gap(mut self, frames: u32) -> Self {
        self.max_gap = frames;
        self
    }
}

/// Reflex definition for GPU
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuReflex {
    pub id: u32,
    pub pattern_len: u32,
    pub action: u32,
    pub action_arg: u32,
    pub priority: u32,
    pub enabled: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub pattern: [GpuPatternElement; MAX_PATTERN_LEN],
}

impl Default for GpuReflex {
    fn default() -> Self {
        Self {
            id: 0,
            pattern_len: 0,
            action: 0,
            action_arg: 0,
            priority: 0,
            enabled: 0,
            _pad0: 0,
            _pad1: 0,
            pattern: [GpuPatternElement::default(); MAX_PATTERN_LEN],
        }
    }
}

impl GpuReflex {
    /// Reflex action constants
    pub const ACTION_NONE: u32 = 0;
    pub const ACTION_CLOSE_WINDOW: u32 = 2;
    pub const ACTION_MINIMIZE_WINDOW: u32 = 3;
    pub const ACTION_TOGGLE_MAXIMIZE: u32 = 4;
    pub const ACTION_NEXT_WINDOW: u32 = 5;
    pub const ACTION_PREV_WINDOW: u32 = 6;
    pub const ACTION_SHOW_LAUNCHER: u32 = 7;
    pub const ACTION_COPY: u32 = 13;
    pub const ACTION_PASTE: u32 = 14;
    pub const ACTION_UNDO: u32 = 15;
    pub const ACTION_REDO: u32 = 16;

    /// Create a new reflex
    pub fn new(id: u32, action: u32, action_arg: u32) -> Self {
        Self {
            id,
            pattern_len: 0,
            action,
            action_arg,
            priority: 100,
            enabled: 1,
            _pad0: 0,
            _pad1: 0,
            pattern: [GpuPatternElement::default(); MAX_PATTERN_LEN],
        }
    }

    /// Add a pattern element
    pub fn with_pattern(mut self, elements: &[GpuPatternElement]) -> Self {
        let len = elements.len().min(MAX_PATTERN_LEN);
        for (i, elem) in elements.iter().take(len).enumerate() {
            self.pattern[i] = *elem;
        }
        self.pattern_len = len as u32;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    // =========================================================================
    // Common reflex presets
    // =========================================================================

    /// Ctrl+W: Close window
    pub fn ctrl_w_close() -> Self {
        Self::new(1, Self::ACTION_CLOSE_WINDOW, 0)
            .with_pattern(&[
                GpuPatternElement::event(GpuInputEvent::KEY_PRESS)
                    .with_data(b'W' as u32)
                    .with_modifiers(GpuInputEvent::MOD_CTRL),
            ])
    }

    /// Ctrl+C: Copy
    pub fn ctrl_c_copy() -> Self {
        Self::new(2, Self::ACTION_COPY, 0)
            .with_pattern(&[
                GpuPatternElement::event(GpuInputEvent::KEY_PRESS)
                    .with_data(b'C' as u32)
                    .with_modifiers(GpuInputEvent::MOD_CTRL),
            ])
    }

    /// Ctrl+V: Paste
    pub fn ctrl_v_paste() -> Self {
        Self::new(3, Self::ACTION_PASTE, 0)
            .with_pattern(&[
                GpuPatternElement::event(GpuInputEvent::KEY_PRESS)
                    .with_data(b'V' as u32)
                    .with_modifiers(GpuInputEvent::MOD_CTRL),
            ])
    }

    /// Ctrl+Z: Undo
    pub fn ctrl_z_undo() -> Self {
        Self::new(4, Self::ACTION_UNDO, 0)
            .with_pattern(&[
                GpuPatternElement::event(GpuInputEvent::KEY_PRESS)
                    .with_data(b'Z' as u32)
                    .with_modifiers(GpuInputEvent::MOD_CTRL),
            ])
    }

    /// Triple click: Select all
    pub fn triple_click() -> Self {
        Self::new(5, Self::ACTION_COPY, 0) // Could be SELECT_ALL action
            .with_pattern(&[
                GpuPatternElement::event(GpuInputEvent::MOUSE_TRIPLE_CLICK),
            ])
    }

    /// Alt+Tab: Next window
    pub fn alt_tab_next() -> Self {
        Self::new(6, Self::ACTION_NEXT_WINDOW, 0)
            .with_pattern(&[
                GpuPatternElement::event(GpuInputEvent::KEY_PRESS)
                    .with_data(0x09) // Tab keycode
                    .with_modifiers(GpuInputEvent::MOD_ALT),
            ])
    }

    /// Meta key: Show launcher
    pub fn meta_launcher() -> Self {
        Self::new(7, Self::ACTION_SHOW_LAUNCHER, 0)
            .with_pattern(&[
                GpuPatternElement::event(GpuInputEvent::KEY_PRESS)
                    .with_modifiers(GpuInputEvent::MOD_META),
            ])
    }
}

/// GPU configuration buffer
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuReflexConfig {
    pub reflex_count: u32,
    pub history_len: u32,
    pub history_head: u32,
    pub current_frame: u32,
}

/// Match result buffer (with atomics simulated as u32)
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMatchResult {
    pub count: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
    pub matches: [u32; MAX_MATCHES],
}

impl Default for GpuMatchResult {
    fn default() -> Self {
        Self {
            count: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            matches: [0; MAX_MATCHES],
        }
    }
}

// =============================================================================
// GPU Reflex Engine
// =============================================================================

/// GPU state for reflex processing
struct GpuState {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    input_buffer: wgpu::Buffer,
    reflex_buffer: wgpu::Buffer,
    config_buffer: wgpu::Buffer,
    result_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
}

/// The main GPU reflex engine
pub struct GpuReflexEngine {
    /// GPU state (None if not initialized)
    gpu_state: Arc<Mutex<Option<GpuState>>>,

    /// CPU-side reflex storage
    reflexes: Vec<GpuReflex>,

    /// Input history ring buffer
    input_history: [GpuInputEvent; INPUT_HISTORY_SIZE],
    history_head: usize,
    history_len: usize,

    /// Current frame counter
    frame: u32,

    /// Statistics
    pub stats: ReflexStats,
}

/// Engine statistics
#[derive(Debug, Clone, Default)]
pub struct ReflexStats {
    pub events_processed: u64,
    pub gpu_dispatches: u64,
    pub total_matches: u64,
    pub last_dispatch_us: u64,
}

impl GpuReflexEngine {
    /// Create a new GPU reflex engine
    pub fn new() -> Self {
        Self {
            gpu_state: Arc::new(Mutex::new(None)),
            reflexes: Vec::with_capacity(MAX_GPU_REFLEXES),
            input_history: [GpuInputEvent::default(); INPUT_HISTORY_SIZE],
            history_head: 0,
            history_len: 0,
            frame: 0,
            stats: ReflexStats::default(),
        }
    }

    /// Initialize GPU pipeline
    pub async fn initialize(&mut self, device: &wgpu::Device) -> Result<()> {
        log::info!("Initializing GPU Reflex Engine...");

        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Reflex Shader"),
            source: wgpu::ShaderSource::Wgsl(GPU_REFLEX_SHADER.into()),
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GPU Reflex Bind Group Layout"),
            entries: &[
                // Input history buffer (read-only)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Reflexes buffer (read-only)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Config buffer (read-only)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Result buffer (read-write)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU Reflex Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create compute pipeline
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("GPU Reflex Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "reflex_match_main",
        });

        // Create buffers
        let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Reflex Input History"),
            size: (INPUT_HISTORY_SIZE * std::mem::size_of::<GpuInputEvent>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let reflex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Reflex Patterns"),
            size: (MAX_GPU_REFLEXES * std::mem::size_of::<GpuReflex>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let config_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Reflex Config"),
            size: std::mem::size_of::<GpuReflexConfig>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Reflex Results"),
            size: std::mem::size_of::<GpuMatchResult>() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Reflex Readback"),
            size: std::mem::size_of::<GpuMatchResult>() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        *self.gpu_state.lock() = Some(GpuState {
            pipeline,
            bind_group_layout,
            input_buffer,
            reflex_buffer,
            config_buffer,
            result_buffer,
            readback_buffer,
        });

        log::info!("GPU Reflex Engine initialized successfully");
        Ok(())
    }

    /// Check if GPU is initialized
    pub fn is_initialized(&self) -> bool {
        self.gpu_state.lock().is_some()
    }

    /// Add a reflex pattern
    pub fn add_reflex(&mut self, reflex: GpuReflex) {
        if self.reflexes.len() < MAX_GPU_REFLEXES {
            self.reflexes.push(reflex);
        } else {
            log::warn!("Maximum reflexes reached ({})", MAX_GPU_REFLEXES);
        }
    }

    /// Remove a reflex by ID
    pub fn remove_reflex(&mut self, id: u32) {
        self.reflexes.retain(|r| r.id != id);
    }

    /// Enable/disable a reflex
    pub fn set_reflex_enabled(&mut self, id: u32, enabled: bool) {
        if let Some(r) = self.reflexes.iter_mut().find(|r| r.id == id) {
            r.enabled = if enabled { 1 } else { 0 };
        }
    }

    /// Push an input event to the history
    pub fn push_event(&mut self, mut event: GpuInputEvent) {
        event.timestamp = self.frame;
        self.input_history[self.history_head] = event;
        self.history_head = (self.history_head + 1) % INPUT_HISTORY_SIZE;
        if self.history_len < INPUT_HISTORY_SIZE {
            self.history_len += 1;
        }
        self.stats.events_processed += 1;
    }

    /// Advance frame counter
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Install default reflexes
    pub fn install_defaults(&mut self) {
        self.add_reflex(GpuReflex::ctrl_w_close());
        self.add_reflex(GpuReflex::ctrl_c_copy());
        self.add_reflex(GpuReflex::ctrl_v_paste());
        self.add_reflex(GpuReflex::ctrl_z_undo());
        self.add_reflex(GpuReflex::triple_click());
        self.add_reflex(GpuReflex::alt_tab_next());
        self.add_reflex(GpuReflex::meta_launcher());
    }

    /// Dispatch pattern matching to GPU and get results
    pub async fn dispatch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<u32>> {
        let start = std::time::Instant::now();

        let state = self.gpu_state.lock();
        let state = match state.as_ref() {
            Some(s) => s,
            None => anyhow::bail!("GPU not initialized"),
        };

        // Prepare reflex buffer (pad to MAX_GPU_REFLEXES)
        let mut reflexes_data = self.reflexes.clone();
        reflexes_data.resize(MAX_GPU_REFLEXES, GpuReflex::default());

        // Prepare config
        let config = GpuReflexConfig {
            reflex_count: self.reflexes.len() as u32,
            history_len: self.history_len as u32,
            history_head: self.history_head as u32,
            current_frame: self.frame,
        };

        // Clear result buffer
        let clear_result = GpuMatchResult::default();

        // Upload data to GPU
        queue.write_buffer(
            &state.input_buffer,
            0,
            bytemuck::cast_slice(&self.input_history),
        );
        queue.write_buffer(
            &state.reflex_buffer,
            0,
            bytemuck::cast_slice(&reflexes_data),
        );
        queue.write_buffer(&state.config_buffer, 0, bytemuck::bytes_of(&config));
        queue.write_buffer(&state.result_buffer, 0, bytemuck::bytes_of(&clear_result));

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU Reflex Bind Group"),
            layout: &state.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: state.input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: state.reflex_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: state.config_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: state.result_buffer.as_entire_binding(),
                },
            ],
        });

        // Create command encoder and dispatch
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GPU Reflex Encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("GPU Reflex Pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&state.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);

            // One workgroup per reflex
            let workgroup_count = (self.reflexes.len() as u32).max(1);
            pass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        // Copy result to readback buffer
        encoder.copy_buffer_to_buffer(
            &state.result_buffer,
            0,
            &state.readback_buffer,
            0,
            std::mem::size_of::<GpuMatchResult>() as u64,
        );

        // Submit and wait
        queue.submit(std::iter::once(encoder.finish()));

        // Map and read results
        let buffer_slice = state.readback_buffer.slice(..);
        let (tx, rx) = tokio::sync::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::Maintain::Wait);
        rx.await??;

        let data = buffer_slice.get_mapped_range();
        let result: &GpuMatchResult = bytemuck::from_bytes(&data);

        let matches: Vec<u32> = result.matches[..result.count.min(MAX_MATCHES as u32) as usize]
            .to_vec();

        drop(data);
        state.readback_buffer.unmap();

        // Update stats
        self.stats.gpu_dispatches += 1;
        self.stats.total_matches += matches.len() as u64;
        self.stats.last_dispatch_us = start.elapsed().as_micros() as u64;

        log::trace!(
            "GPU reflex dispatch: {} matches in {}µs",
            matches.len(),
            self.stats.last_dispatch_us
        );

        Ok(matches)
    }

    /// Get reflex action and argument for a matched ID
    pub fn get_action(&self, reflex_id: u32) -> Option<(u32, u32)> {
        self.reflexes
            .iter()
            .find(|r| r.id == reflex_id)
            .map(|r| (r.action, r.action_arg))
    }

    /// Get statistics
    pub fn stats(&self) -> &ReflexStats {
        &self.stats
    }
}

impl Default for GpuReflexEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_input_event() {
        let event = GpuInputEvent::key_press(b'A', GpuInputEvent::MOD_CTRL, 100);
        assert_eq!(event.event_type, GpuInputEvent::KEY_PRESS);
        assert_eq!(event.data, b'A' as u32);
        assert_eq!(event.modifiers, GpuInputEvent::MOD_CTRL);
        assert_eq!(event.timestamp, 100);
    }

    #[test]
    fn test_gpu_pattern_element() {
        let pattern = GpuPatternElement::event(GpuInputEvent::KEY_PRESS)
            .with_data(b'W' as u32)
            .with_modifiers(GpuInputEvent::MOD_CTRL)
            .with_max_gap(60);

        assert_eq!(pattern.event_type, GpuInputEvent::KEY_PRESS);
        assert_eq!(pattern.data, b'W' as u32);
        assert_eq!(pattern.require_data, 1);
        assert_eq!(pattern.modifiers, GpuInputEvent::MOD_CTRL);
        assert_eq!(pattern.require_modifiers, 1);
        assert_eq!(pattern.max_gap, 60);
    }

    #[test]
    fn test_gpu_reflex_presets() {
        let close = GpuReflex::ctrl_w_close();
        assert_eq!(close.id, 1);
        assert_eq!(close.action, GpuReflex::ACTION_CLOSE_WINDOW);
        assert_eq!(close.pattern_len, 1);
        assert_eq!(close.pattern[0].data, b'W' as u32);
        assert_eq!(close.pattern[0].modifiers, GpuInputEvent::MOD_CTRL);
    }

    #[test]
    fn test_engine_event_history() {
        let mut engine = GpuReflexEngine::new();

        for i in 0..10 {
            engine.push_event(GpuInputEvent::key_press(b'A' + i, 0, 0));
        }

        assert_eq!(engine.history_len, 10);
        assert_eq!(engine.stats.events_processed, 10);
    }

    #[test]
    fn test_engine_install_defaults() {
        let mut engine = GpuReflexEngine::new();
        engine.install_defaults();

        assert_eq!(engine.reflexes.len(), 7);
        assert!(engine.reflexes.iter().any(|r| r.id == 1)); // ctrl_w_close
        assert!(engine.reflexes.iter().any(|r| r.id == 2)); // ctrl_c_copy
    }
}
