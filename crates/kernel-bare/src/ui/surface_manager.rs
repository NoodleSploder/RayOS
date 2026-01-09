//! Surface Manager for RayOS UI
//!
//! Manages multiple GuestSurface instances with z-order, focus, and lifecycle tracking.
//! Provides frame buffering, metrics, and window binding for compositor integration.
//!
//! # Overview
//!
//! The Surface Manager is responsible for:
//! - Registering and tracking guest surfaces (VM framebuffers)
//! - Binding surfaces to windows for compositor rendering
//! - Frame buffering with backpressure signaling
//! - Metrics collection (FPS, latency, dropped frames)
//!
//! # Markers
//!
//! - `RAYOS_SURFACE:REGISTERED` - New surface registered
//! - `RAYOS_SURFACE:PRESENTED` - Surface presented to compositor
//! - `RAYOS_SURFACE:HIDDEN` - Surface hidden from view
//! - `RAYOS_SURFACE:DESTROYED` - Surface destroyed and resources freed
//! - `RAYOS_SURFACE:FRAME` - New frame received

use super::window_manager::WindowId;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of surfaces supported.
pub const MAX_SURFACES: usize = 16;

/// Maximum frames buffered per surface.
pub const MAX_BUFFERED_FRAMES: usize = 8;

/// Invalid/null surface ID.
pub const SURFACE_ID_NONE: SurfaceId = 0;

// ============================================================================
// Surface ID
// ============================================================================

/// Unique surface identifier.
pub type SurfaceId = u32;

/// Counter for generating unique surface IDs.
static mut NEXT_SURFACE_ID: SurfaceId = 1;

/// Generate a new unique surface ID.
fn next_surface_id() -> SurfaceId {
    // SAFETY: Single-threaded kernel context
    unsafe {
        let id = NEXT_SURFACE_ID;
        NEXT_SURFACE_ID = NEXT_SURFACE_ID.wrapping_add(1);
        if NEXT_SURFACE_ID == SURFACE_ID_NONE {
            NEXT_SURFACE_ID = 1;
        }
        id
    }
}

// ============================================================================
// Surface State
// ============================================================================

/// Lifecycle state of a surface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SurfaceState {
    /// Surface slot is empty/unused.
    Empty = 0,
    /// Surface registered but no frames received yet.
    NotReady = 1,
    /// Surface has received frames and is ready to present.
    Ready = 2,
    /// Surface is currently being composited to display.
    Presented = 3,
    /// Surface is hidden (not composited but still alive).
    Hidden = 4,
    /// Surface is being destroyed, resources being freed.
    Destroying = 5,
}

impl Default for SurfaceState {
    fn default() -> Self {
        SurfaceState::Empty
    }
}

impl SurfaceState {
    /// Returns true if the surface can receive frames.
    pub fn can_receive_frames(self) -> bool {
        matches!(self, SurfaceState::NotReady | SurfaceState::Ready | 
                       SurfaceState::Presented | SurfaceState::Hidden)
    }

    /// Returns true if the surface is visible.
    pub fn is_visible(self) -> bool {
        self == SurfaceState::Presented
    }

    /// Returns true if the surface is active (not empty/destroying).
    pub fn is_active(self) -> bool {
        !matches!(self, SurfaceState::Empty | SurfaceState::Destroying)
    }
}

// ============================================================================
// Surface Format
// ============================================================================

/// Pixel format of surface data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SurfaceFormat {
    /// Unknown/uninitialized format.
    Unknown = 0,
    /// 32-bit BGRA (blue, green, red, alpha).
    Bgra8888 = 1,
    /// 32-bit RGBA (red, green, blue, alpha).
    Rgba8888 = 2,
    /// 32-bit XRGB (RGB with unused alpha byte).
    Xrgb8888 = 3,
    /// 24-bit RGB (no alpha).
    Rgb888 = 4,
    /// 16-bit RGB (5-6-5 layout).
    Rgb565 = 5,
}

impl Default for SurfaceFormat {
    fn default() -> Self {
        SurfaceFormat::Unknown
    }
}

impl SurfaceFormat {
    /// Bytes per pixel for this format.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            SurfaceFormat::Unknown => 0,
            SurfaceFormat::Bgra8888 => 4,
            SurfaceFormat::Rgba8888 => 4,
            SurfaceFormat::Xrgb8888 => 4,
            SurfaceFormat::Rgb888 => 3,
            SurfaceFormat::Rgb565 => 2,
        }
    }

    /// Returns true if format has alpha channel.
    pub fn has_alpha(self) -> bool {
        matches!(self, SurfaceFormat::Bgra8888 | SurfaceFormat::Rgba8888)
    }
}

// ============================================================================
// Surface Metadata
// ============================================================================

/// Metadata describing a surface's properties.
#[derive(Clone, Copy, Default)]
pub struct SurfaceMetadata {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel format.
    pub format: SurfaceFormat,
    /// Stride in bytes (bytes per row).
    pub stride: u32,
    /// Current frame sequence number.
    pub frame_seq: u64,
    /// Physical address of backing buffer (if applicable).
    pub backing_phys: u64,
    /// Size of backing buffer in bytes.
    pub backing_size: u64,
}

impl SurfaceMetadata {
    /// Create new metadata with dimensions and format.
    pub const fn new(width: u32, height: u32, format: SurfaceFormat) -> Self {
        let bpp = match format {
            SurfaceFormat::Unknown => 0,
            SurfaceFormat::Bgra8888 => 4,
            SurfaceFormat::Rgba8888 => 4,
            SurfaceFormat::Xrgb8888 => 4,
            SurfaceFormat::Rgb888 => 3,
            SurfaceFormat::Rgb565 => 2,
        };
        Self {
            width,
            height,
            format,
            stride: width * bpp,
            frame_seq: 0,
            backing_phys: 0,
            backing_size: 0,
        }
    }

    /// Calculate total buffer size in bytes.
    pub fn buffer_size(&self) -> u64 {
        self.stride as u64 * self.height as u64
    }

    /// Update dimensions (recalculates stride).
    pub fn set_dimensions(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.stride = width * self.format.bytes_per_pixel() as u32;
    }

    /// Set backing buffer info.
    pub fn set_backing(&mut self, phys: u64, size: u64) {
        self.backing_phys = phys;
        self.backing_size = size;
    }
}

// ============================================================================
// Frame Entry
// ============================================================================

/// A single buffered frame.
#[derive(Clone, Copy, Default)]
pub struct FrameEntry {
    /// Frame sequence number (0 = empty slot).
    pub seq: u64,
    /// Timestamp when frame was submitted (ticks).
    pub submit_time: u64,
    /// Timestamp when frame was presented (ticks, 0 = not yet).
    pub present_time: u64,
    /// Physical address of frame data.
    pub data_phys: u64,
    /// Size of frame data in bytes.
    pub data_size: u64,
    /// Whether this frame is still pending presentation.
    pub pending: bool,
}

impl FrameEntry {
    /// Create a new frame entry.
    pub const fn new(seq: u64, submit_time: u64, data_phys: u64, data_size: u64) -> Self {
        Self {
            seq,
            submit_time,
            present_time: 0,
            data_phys,
            data_size,
            pending: true,
        }
    }

    /// Mark frame as presented.
    pub fn mark_presented(&mut self, time: u64) {
        self.present_time = time;
        self.pending = false;
    }

    /// Calculate latency (present_time - submit_time).
    pub fn latency(&self) -> u64 {
        if self.present_time > 0 && self.present_time >= self.submit_time {
            self.present_time - self.submit_time
        } else {
            0
        }
    }

    /// Returns true if entry is valid.
    pub fn is_valid(&self) -> bool {
        self.seq > 0
    }
}

// ============================================================================
// Surface Frame Buffer
// ============================================================================

/// Ring buffer for frame data per surface.
#[derive(Clone)]
pub struct SurfaceFrameBuffer {
    /// Buffered frames (ring buffer).
    frames: [FrameEntry; MAX_BUFFERED_FRAMES],
    /// Write index (next slot to write).
    write_idx: usize,
    /// Read index (next slot to read/present).
    read_idx: usize,
    /// Number of frames currently buffered.
    count: usize,
    /// Total frames submitted.
    total_submitted: u64,
    /// Total frames presented.
    total_presented: u64,
    /// Total frames dropped (overwritten before present).
    total_dropped: u64,
}

impl Default for SurfaceFrameBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceFrameBuffer {
    /// Create an empty frame buffer.
    pub const fn new() -> Self {
        Self {
            frames: [FrameEntry {
                seq: 0,
                submit_time: 0,
                present_time: 0,
                data_phys: 0,
                data_size: 0,
                pending: false,
            }; MAX_BUFFERED_FRAMES],
            write_idx: 0,
            read_idx: 0,
            count: 0,
            total_submitted: 0,
            total_presented: 0,
            total_dropped: 0,
        }
    }

    /// Returns true if buffer is full.
    pub fn is_full(&self) -> bool {
        self.count >= MAX_BUFFERED_FRAMES
    }

    /// Returns true if buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Number of frames currently buffered.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Submit a new frame. Returns true if accepted, false if dropped due to backpressure.
    pub fn submit(&mut self, frame: FrameEntry) -> bool {
        self.total_submitted += 1;

        if self.is_full() {
            // Drop oldest frame (overwrite)
            self.total_dropped += 1;
            self.frames[self.write_idx] = frame;
            self.write_idx = (self.write_idx + 1) % MAX_BUFFERED_FRAMES;
            self.read_idx = (self.read_idx + 1) % MAX_BUFFERED_FRAMES;
            false
        } else {
            self.frames[self.write_idx] = frame;
            self.write_idx = (self.write_idx + 1) % MAX_BUFFERED_FRAMES;
            self.count += 1;
            true
        }
    }

    /// Get next frame to present (does not remove).
    pub fn peek(&self) -> Option<&FrameEntry> {
        if self.is_empty() {
            None
        } else {
            Some(&self.frames[self.read_idx])
        }
    }

    /// Get next frame to present (mutable, does not remove).
    pub fn peek_mut(&mut self) -> Option<&mut FrameEntry> {
        if self.is_empty() {
            None
        } else {
            Some(&mut self.frames[self.read_idx])
        }
    }

    /// Consume (remove) the front frame after presentation.
    pub fn consume(&mut self) -> Option<FrameEntry> {
        if self.is_empty() {
            None
        } else {
            let frame = self.frames[self.read_idx];
            self.frames[self.read_idx] = FrameEntry::default();
            self.read_idx = (self.read_idx + 1) % MAX_BUFFERED_FRAMES;
            self.count -= 1;
            self.total_presented += 1;
            Some(frame)
        }
    }

    /// Clear all buffered frames.
    pub fn clear(&mut self) {
        self.frames = [FrameEntry::default(); MAX_BUFFERED_FRAMES];
        self.write_idx = 0;
        self.read_idx = 0;
        self.count = 0;
    }

    /// Get statistics.
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.total_submitted, self.total_presented, self.total_dropped)
    }
}

// ============================================================================
// Surface Metrics
// ============================================================================

/// Performance metrics for a surface.
#[derive(Clone, Copy, Default)]
pub struct SurfaceMetrics {
    /// Frames per second (rolling average).
    pub fps: u32,
    /// Average frame latency in ticks.
    pub avg_latency: u64,
    /// Total frames submitted.
    pub frames_submitted: u64,
    /// Total frames presented.
    pub frames_presented: u64,
    /// Total frames dropped.
    pub frames_dropped: u64,
    /// Last frame timestamp for FPS calculation.
    last_fps_time: u64,
    /// Frame count since last FPS calculation.
    fps_frame_count: u32,
    /// Latency accumulator for averaging.
    latency_accum: u64,
    /// Latency sample count.
    latency_samples: u32,
}

impl SurfaceMetrics {
    /// Create new metrics.
    pub const fn new() -> Self {
        Self {
            fps: 0,
            avg_latency: 0,
            frames_submitted: 0,
            frames_presented: 0,
            frames_dropped: 0,
            last_fps_time: 0,
            fps_frame_count: 0,
            latency_accum: 0,
            latency_samples: 0,
        }
    }

    /// Record a frame submission.
    pub fn record_submit(&mut self) {
        self.frames_submitted += 1;
    }

    /// Record a frame presentation with latency.
    pub fn record_present(&mut self, latency: u64, current_time: u64) {
        self.frames_presented += 1;
        self.fps_frame_count += 1;
        self.latency_accum += latency;
        self.latency_samples += 1;

        // Update FPS every second (assuming ~1000 ticks/sec)
        if current_time >= self.last_fps_time + 1000 {
            self.fps = self.fps_frame_count;
            self.fps_frame_count = 0;
            self.last_fps_time = current_time;

            // Update average latency
            if self.latency_samples > 0 {
                self.avg_latency = self.latency_accum / self.latency_samples as u64;
                self.latency_accum = 0;
                self.latency_samples = 0;
            }
        }
    }

    /// Record a dropped frame.
    pub fn record_drop(&mut self) {
        self.frames_dropped += 1;
    }

    /// Reset all metrics.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

// ============================================================================
// Surface Binding
// ============================================================================

/// Binding between a surface and a window.
#[derive(Clone, Copy, Default)]
pub struct SurfaceBinding {
    /// Surface ID (0 = unbound).
    pub surface_id: SurfaceId,
    /// Window ID (0 = unbound).
    pub window_id: WindowId,
    /// Whether binding is active.
    pub active: bool,
    /// Timestamp when bound.
    pub bound_at: u64,
}

impl SurfaceBinding {
    /// Create a new binding.
    pub const fn new(surface_id: SurfaceId, window_id: WindowId, timestamp: u64) -> Self {
        Self {
            surface_id,
            window_id,
            active: true,
            bound_at: timestamp,
        }
    }

    /// Check if binding is valid.
    pub fn is_valid(&self) -> bool {
        self.active && self.surface_id != SURFACE_ID_NONE && self.window_id != 0
    }

    /// Unbind.
    pub fn unbind(&mut self) {
        self.active = false;
    }
}

// ============================================================================
// Guest Surface Entry
// ============================================================================

/// A registered guest surface with all associated data.
#[derive(Clone)]
pub struct GuestSurfaceEntry {
    /// Unique surface ID.
    pub id: SurfaceId,
    /// Current state.
    pub state: SurfaceState,
    /// Surface metadata (dimensions, format).
    pub metadata: SurfaceMetadata,
    /// Frame buffer for pending frames.
    pub frame_buffer: SurfaceFrameBuffer,
    /// Performance metrics.
    pub metrics: SurfaceMetrics,
    /// Window binding (if bound).
    pub binding: SurfaceBinding,
    /// Source identifier (e.g., VM name hash).
    pub source_id: u64,
    /// Creation timestamp.
    pub created_at: u64,
    /// Last activity timestamp.
    pub last_activity: u64,
}

impl Default for GuestSurfaceEntry {
    fn default() -> Self {
        Self {
            id: SURFACE_ID_NONE,
            state: SurfaceState::Empty,
            metadata: SurfaceMetadata::default(),
            frame_buffer: SurfaceFrameBuffer::new(),
            metrics: SurfaceMetrics::new(),
            binding: SurfaceBinding::default(),
            source_id: 0,
            created_at: 0,
            last_activity: 0,
        }
    }
}

impl GuestSurfaceEntry {
    /// Create a new surface entry.
    pub fn new(id: SurfaceId, metadata: SurfaceMetadata, source_id: u64, timestamp: u64) -> Self {
        Self {
            id,
            state: SurfaceState::NotReady,
            metadata,
            frame_buffer: SurfaceFrameBuffer::new(),
            metrics: SurfaceMetrics::new(),
            binding: SurfaceBinding::default(),
            source_id,
            created_at: timestamp,
            last_activity: timestamp,
        }
    }

    /// Returns true if surface is valid (has ID).
    pub fn is_valid(&self) -> bool {
        self.id != SURFACE_ID_NONE && self.state.is_active()
    }

    /// Bind to a window.
    pub fn bind_window(&mut self, window_id: WindowId, timestamp: u64) {
        self.binding = SurfaceBinding::new(self.id, window_id, timestamp);
        self.last_activity = timestamp;
    }

    /// Unbind from window.
    pub fn unbind_window(&mut self) {
        self.binding.unbind();
    }

    /// Submit a new frame.
    pub fn submit_frame(&mut self, frame: FrameEntry, timestamp: u64) -> bool {
        self.last_activity = timestamp;
        self.metrics.record_submit();

        let accepted = self.frame_buffer.submit(frame);
        if !accepted {
            self.metrics.record_drop();
        }

        // Transition to Ready if first frame
        if self.state == SurfaceState::NotReady {
            self.state = SurfaceState::Ready;
        }

        accepted
    }

    /// Present the next frame.
    pub fn present_frame(&mut self, timestamp: u64) -> Option<FrameEntry> {
        if let Some(frame) = self.frame_buffer.peek_mut() {
            frame.mark_presented(timestamp);
            let latency = frame.latency();
            self.metrics.record_present(latency, timestamp);
        }
        self.frame_buffer.consume()
    }

    /// Set state to presented.
    pub fn present(&mut self, timestamp: u64) {
        if self.state == SurfaceState::Ready || self.state == SurfaceState::Hidden {
            self.state = SurfaceState::Presented;
            self.last_activity = timestamp;
        }
    }

    /// Set state to hidden.
    pub fn hide(&mut self, timestamp: u64) {
        if self.state == SurfaceState::Presented {
            self.state = SurfaceState::Hidden;
            self.last_activity = timestamp;
        }
    }

    /// Begin destruction.
    pub fn destroy(&mut self) {
        self.state = SurfaceState::Destroying;
        self.binding.unbind();
        self.frame_buffer.clear();
    }
}

// ============================================================================
// Surface Registry
// ============================================================================

/// Registry of all guest surfaces.
pub struct SurfaceRegistry {
    /// Surface entries.
    surfaces: [GuestSurfaceEntry; MAX_SURFACES],
    /// Number of active surfaces.
    count: usize,
}

impl Default for SurfaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceRegistry {
    /// Create an empty registry.
    pub const fn new() -> Self {
        // Manual array initialization for const fn
        Self {
            surfaces: [
                GuestSurfaceEntry::empty(), GuestSurfaceEntry::empty(),
                GuestSurfaceEntry::empty(), GuestSurfaceEntry::empty(),
                GuestSurfaceEntry::empty(), GuestSurfaceEntry::empty(),
                GuestSurfaceEntry::empty(), GuestSurfaceEntry::empty(),
                GuestSurfaceEntry::empty(), GuestSurfaceEntry::empty(),
                GuestSurfaceEntry::empty(), GuestSurfaceEntry::empty(),
                GuestSurfaceEntry::empty(), GuestSurfaceEntry::empty(),
                GuestSurfaceEntry::empty(), GuestSurfaceEntry::empty(),
            ],
            count: 0,
        }
    }

    /// Number of active surfaces.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Returns true if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns true if registry is full.
    pub fn is_full(&self) -> bool {
        self.count >= MAX_SURFACES
    }

    /// Register a new surface. Returns surface ID or None if full.
    pub fn register(
        &mut self,
        metadata: SurfaceMetadata,
        source_id: u64,
        timestamp: u64,
    ) -> Option<SurfaceId> {
        if self.is_full() {
            return None;
        }

        // Find empty slot
        for entry in self.surfaces.iter_mut() {
            if entry.state == SurfaceState::Empty {
                let id = next_surface_id();
                *entry = GuestSurfaceEntry::new(id, metadata, source_id, timestamp);
                self.count += 1;
                return Some(id);
            }
        }

        None
    }

    /// Get a surface by ID.
    pub fn get(&self, id: SurfaceId) -> Option<&GuestSurfaceEntry> {
        self.surfaces.iter().find(|s| s.id == id && s.is_valid())
    }

    /// Get a surface by ID (mutable).
    pub fn get_mut(&mut self, id: SurfaceId) -> Option<&mut GuestSurfaceEntry> {
        self.surfaces.iter_mut().find(|s| s.id == id && s.is_valid())
    }

    /// Find surface by source ID.
    pub fn find_by_source(&self, source_id: u64) -> Option<&GuestSurfaceEntry> {
        self.surfaces.iter().find(|s| s.source_id == source_id && s.is_valid())
    }

    /// Find surface bound to a window.
    pub fn find_by_window(&self, window_id: WindowId) -> Option<&GuestSurfaceEntry> {
        self.surfaces.iter().find(|s| {
            s.is_valid() && s.binding.is_valid() && s.binding.window_id == window_id
        })
    }

    /// Find surface bound to a window (mutable).
    pub fn find_by_window_mut(&mut self, window_id: WindowId) -> Option<&mut GuestSurfaceEntry> {
        self.surfaces.iter_mut().find(|s| {
            s.is_valid() && s.binding.is_valid() && s.binding.window_id == window_id
        })
    }

    /// Destroy a surface by ID.
    pub fn destroy(&mut self, id: SurfaceId) -> bool {
        if let Some(entry) = self.surfaces.iter_mut().find(|s| s.id == id) {
            if entry.is_valid() {
                entry.destroy();
                entry.state = SurfaceState::Empty;
                entry.id = SURFACE_ID_NONE;
                self.count = self.count.saturating_sub(1);
                return true;
            }
        }
        false
    }

    /// Iterate over all active surfaces.
    pub fn iter(&self) -> impl Iterator<Item = &GuestSurfaceEntry> {
        self.surfaces.iter().filter(|s| s.is_valid())
    }

    /// Iterate over all active surfaces (mutable).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut GuestSurfaceEntry> {
        self.surfaces.iter_mut().filter(|s| s.is_valid())
    }

    /// Get all presented surfaces (for compositor).
    pub fn presented_surfaces(&self) -> impl Iterator<Item = &GuestSurfaceEntry> {
        self.surfaces.iter().filter(|s| s.state == SurfaceState::Presented)
    }

    /// Unbind all surfaces from a window.
    pub fn unbind_window(&mut self, window_id: WindowId) {
        for entry in self.surfaces.iter_mut() {
            if entry.binding.is_valid() && entry.binding.window_id == window_id {
                entry.unbind_window();
            }
        }
    }
}

impl GuestSurfaceEntry {
    /// Create an empty entry (for const initialization).
    const fn empty() -> Self {
        Self {
            id: SURFACE_ID_NONE,
            state: SurfaceState::Empty,
            metadata: SurfaceMetadata {
                width: 0,
                height: 0,
                format: SurfaceFormat::Unknown,
                stride: 0,
                frame_seq: 0,
                backing_phys: 0,
                backing_size: 0,
            },
            frame_buffer: SurfaceFrameBuffer::new(),
            metrics: SurfaceMetrics::new(),
            binding: SurfaceBinding {
                surface_id: SURFACE_ID_NONE,
                window_id: 0,
                active: false,
                bound_at: 0,
            },
            source_id: 0,
            created_at: 0,
            last_activity: 0,
        }
    }
}

// ============================================================================
// Surface Manager
// ============================================================================

/// Main surface manager orchestrating all surface operations.
pub struct SurfaceManager {
    /// Surface registry.
    registry: SurfaceRegistry,
    /// Current tick/timestamp.
    current_time: u64,
}

impl Default for SurfaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceManager {
    /// Create a new surface manager.
    pub const fn new() -> Self {
        Self {
            registry: SurfaceRegistry::new(),
            current_time: 0,
        }
    }

    /// Update current time.
    pub fn tick(&mut self, timestamp: u64) {
        self.current_time = timestamp;
    }

    /// Register a new surface.
    pub fn register(
        &mut self,
        width: u32,
        height: u32,
        format: SurfaceFormat,
        source_id: u64,
    ) -> Option<SurfaceId> {
        let metadata = SurfaceMetadata::new(width, height, format);
        let id = self.registry.register(metadata, source_id, self.current_time)?;
        
        // Emit marker
        #[cfg(feature = "surface_markers")]
        crate::serial_println!("RAYOS_SURFACE:REGISTERED id={} source={}", id, source_id);
        
        Some(id)
    }

    /// Submit a frame to a surface.
    pub fn submit_frame(
        &mut self,
        surface_id: SurfaceId,
        frame_seq: u64,
        data_phys: u64,
        data_size: u64,
    ) -> bool {
        if let Some(entry) = self.registry.get_mut(surface_id) {
            let frame = FrameEntry::new(frame_seq, self.current_time, data_phys, data_size);
            let accepted = entry.submit_frame(frame, self.current_time);
            
            // Emit marker
            #[cfg(feature = "surface_markers")]
            crate::serial_println!("RAYOS_SURFACE:FRAME id={} seq={} accepted={}", 
                surface_id, frame_seq, accepted);
            
            return accepted;
        }
        false
    }

    /// Present a surface (make visible).
    pub fn present(&mut self, surface_id: SurfaceId) -> bool {
        if let Some(entry) = self.registry.get_mut(surface_id) {
            entry.present(self.current_time);
            
            // Emit marker
            #[cfg(feature = "surface_markers")]
            crate::serial_println!("RAYOS_SURFACE:PRESENTED id={}", surface_id);
            
            return true;
        }
        false
    }

    /// Hide a surface.
    pub fn hide(&mut self, surface_id: SurfaceId) -> bool {
        if let Some(entry) = self.registry.get_mut(surface_id) {
            entry.hide(self.current_time);
            
            // Emit marker
            #[cfg(feature = "surface_markers")]
            crate::serial_println!("RAYOS_SURFACE:HIDDEN id={}", surface_id);
            
            return true;
        }
        false
    }

    /// Destroy a surface.
    pub fn destroy(&mut self, surface_id: SurfaceId) -> bool {
        let result = self.registry.destroy(surface_id);
        
        if result {
            // Emit marker
            #[cfg(feature = "surface_markers")]
            crate::serial_println!("RAYOS_SURFACE:DESTROYED id={}", surface_id);
        }
        
        result
    }

    /// Bind a surface to a window.
    pub fn bind_window(&mut self, surface_id: SurfaceId, window_id: WindowId) -> bool {
        // Unbind any existing surface from this window
        self.registry.unbind_window(window_id);

        if let Some(entry) = self.registry.get_mut(surface_id) {
            entry.bind_window(window_id, self.current_time);
            return true;
        }
        false
    }

    /// Unbind a surface from its window.
    pub fn unbind(&mut self, surface_id: SurfaceId) -> bool {
        if let Some(entry) = self.registry.get_mut(surface_id) {
            entry.unbind_window();
            return true;
        }
        false
    }

    /// Get surface by ID.
    pub fn get(&self, surface_id: SurfaceId) -> Option<&GuestSurfaceEntry> {
        self.registry.get(surface_id)
    }

    /// Get surface by ID (mutable).
    pub fn get_mut(&mut self, surface_id: SurfaceId) -> Option<&mut GuestSurfaceEntry> {
        self.registry.get_mut(surface_id)
    }

    /// Find surface by window.
    pub fn find_by_window(&self, window_id: WindowId) -> Option<&GuestSurfaceEntry> {
        self.registry.find_by_window(window_id)
    }

    /// Get all presented surfaces.
    pub fn presented(&self) -> impl Iterator<Item = &GuestSurfaceEntry> {
        self.registry.presented_surfaces()
    }

    /// Get surface count.
    pub fn count(&self) -> usize {
        self.registry.len()
    }

    /// Get metrics for a surface.
    pub fn metrics(&self, surface_id: SurfaceId) -> Option<SurfaceMetrics> {
        self.registry.get(surface_id).map(|e| e.metrics)
    }
}

// ============================================================================
// Global Instance
// ============================================================================

/// Global surface manager instance.
static mut SURFACE_MANAGER: SurfaceManager = SurfaceManager::new();

/// Get the global surface manager.
pub fn get() -> &'static SurfaceManager {
    // SAFETY: Single-threaded kernel context
    unsafe { &SURFACE_MANAGER }
}

/// Get the global surface manager (mutable).
pub fn get_mut() -> &'static mut SurfaceManager {
    // SAFETY: Single-threaded kernel context
    unsafe { &mut SURFACE_MANAGER }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Unit Tests ---

    #[test]
    fn test_surface_state_transitions() {
        assert!(SurfaceState::NotReady.can_receive_frames());
        assert!(SurfaceState::Ready.can_receive_frames());
        assert!(SurfaceState::Presented.can_receive_frames());
        assert!(SurfaceState::Hidden.can_receive_frames());
        assert!(!SurfaceState::Empty.can_receive_frames());
        assert!(!SurfaceState::Destroying.can_receive_frames());
    }

    #[test]
    fn test_surface_state_visibility() {
        assert!(SurfaceState::Presented.is_visible());
        assert!(!SurfaceState::Hidden.is_visible());
        assert!(!SurfaceState::Ready.is_visible());
    }

    #[test]
    fn test_surface_format_bpp() {
        assert_eq!(SurfaceFormat::Bgra8888.bytes_per_pixel(), 4);
        assert_eq!(SurfaceFormat::Rgba8888.bytes_per_pixel(), 4);
        assert_eq!(SurfaceFormat::Rgb888.bytes_per_pixel(), 3);
        assert_eq!(SurfaceFormat::Rgb565.bytes_per_pixel(), 2);
        assert_eq!(SurfaceFormat::Unknown.bytes_per_pixel(), 0);
    }

    #[test]
    fn test_surface_metadata_buffer_size() {
        let meta = SurfaceMetadata::new(800, 600, SurfaceFormat::Bgra8888);
        assert_eq!(meta.width, 800);
        assert_eq!(meta.height, 600);
        assert_eq!(meta.stride, 800 * 4);
        assert_eq!(meta.buffer_size(), 800 * 4 * 600);
    }

    #[test]
    fn test_frame_entry_latency() {
        let mut frame = FrameEntry::new(1, 100, 0x1000, 1024);
        assert_eq!(frame.latency(), 0);
        frame.mark_presented(150);
        assert_eq!(frame.latency(), 50);
    }

    #[test]
    fn test_frame_buffer_empty() {
        let buffer = SurfaceFrameBuffer::new();
        assert!(buffer.is_empty());
        assert!(!buffer.is_full());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_frame_buffer_submit_consume() {
        let mut buffer = SurfaceFrameBuffer::new();
        let frame = FrameEntry::new(1, 100, 0x1000, 1024);
        
        assert!(buffer.submit(frame));
        assert_eq!(buffer.len(), 1);
        
        let consumed = buffer.consume();
        assert!(consumed.is_some());
        assert_eq!(consumed.unwrap().seq, 1);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_frame_buffer_full_drops() {
        let mut buffer = SurfaceFrameBuffer::new();
        
        // Fill buffer
        for i in 0..MAX_BUFFERED_FRAMES {
            assert!(buffer.submit(FrameEntry::new(i as u64 + 1, 100, 0x1000, 1024)));
        }
        assert!(buffer.is_full());
        
        // Next submit should drop oldest
        assert!(!buffer.submit(FrameEntry::new(100, 100, 0x1000, 1024)));
        let (submitted, _, dropped) = buffer.stats();
        assert_eq!(submitted, MAX_BUFFERED_FRAMES as u64 + 1);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn test_surface_metrics_fps() {
        let mut metrics = SurfaceMetrics::new();
        metrics.record_submit();
        metrics.record_present(10, 500);
        assert_eq!(metrics.frames_submitted, 1);
        assert_eq!(metrics.frames_presented, 1);
    }

    #[test]
    fn test_surface_binding() {
        let binding = SurfaceBinding::new(1, 2, 100);
        assert!(binding.is_valid());
        assert_eq!(binding.surface_id, 1);
        assert_eq!(binding.window_id, 2);
    }

    #[test]
    fn test_surface_registry_register() {
        let mut registry = SurfaceRegistry::new();
        let meta = SurfaceMetadata::new(800, 600, SurfaceFormat::Bgra8888);
        
        let id = registry.register(meta, 123, 0);
        assert!(id.is_some());
        assert_eq!(registry.len(), 1);
        
        let surface = registry.get(id.unwrap());
        assert!(surface.is_some());
        assert_eq!(surface.unwrap().source_id, 123);
    }

    #[test]
    fn test_surface_registry_destroy() {
        let mut registry = SurfaceRegistry::new();
        let meta = SurfaceMetadata::new(800, 600, SurfaceFormat::Bgra8888);
        
        let id = registry.register(meta, 123, 0).unwrap();
        assert_eq!(registry.len(), 1);
        
        assert!(registry.destroy(id));
        assert_eq!(registry.len(), 0);
        assert!(registry.get(id).is_none());
    }

    #[test]
    fn test_surface_entry_lifecycle() {
        let meta = SurfaceMetadata::new(800, 600, SurfaceFormat::Bgra8888);
        let mut entry = GuestSurfaceEntry::new(1, meta, 123, 0);
        
        assert_eq!(entry.state, SurfaceState::NotReady);
        
        // Submit frame transitions to Ready
        let frame = FrameEntry::new(1, 100, 0x1000, 1024);
        entry.submit_frame(frame, 100);
        assert_eq!(entry.state, SurfaceState::Ready);
        
        // Present
        entry.present(200);
        assert_eq!(entry.state, SurfaceState::Presented);
        
        // Hide
        entry.hide(300);
        assert_eq!(entry.state, SurfaceState::Hidden);
    }

    // --- Scenario Tests ---

    #[test]
    fn scenario_multi_surface_registration() {
        let mut registry = SurfaceRegistry::new();
        
        // Register multiple surfaces
        let ids: Vec<_> = (0..4).filter_map(|i| {
            let meta = SurfaceMetadata::new(800, 600, SurfaceFormat::Bgra8888);
            registry.register(meta, 100 + i, i as u64)
        }).collect();
        
        assert_eq!(ids.len(), 4);
        assert_eq!(registry.len(), 4);
        
        // All surfaces should be findable
        for id in &ids {
            assert!(registry.get(*id).is_some());
        }
    }

    #[test]
    fn scenario_surface_window_binding() {
        let mut registry = SurfaceRegistry::new();
        let meta = SurfaceMetadata::new(800, 600, SurfaceFormat::Bgra8888);
        
        let surface_id = registry.register(meta, 123, 0).unwrap();
        let window_id: WindowId = 42;
        
        // Bind surface to window
        if let Some(entry) = registry.get_mut(surface_id) {
            entry.bind_window(window_id, 100);
        }
        
        // Find by window
        let found = registry.find_by_window(window_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, surface_id);
        
        // Unbind
        registry.unbind_window(window_id);
        assert!(registry.find_by_window(window_id).is_none());
    }

    #[test]
    fn scenario_frame_flow() {
        let mut manager = SurfaceManager::new();
        manager.tick(0);
        
        // Register surface
        let id = manager.register(800, 600, SurfaceFormat::Bgra8888, 123).unwrap();
        
        // Submit frames
        for seq in 1..=5 {
            manager.tick(seq * 10);
            manager.submit_frame(id, seq, 0x1000, 1024);
        }
        
        // Check metrics
        let metrics = manager.metrics(id).unwrap();
        assert_eq!(metrics.frames_submitted, 5);
    }

    #[test]
    fn scenario_surface_presentation() {
        let mut manager = SurfaceManager::new();
        manager.tick(0);
        
        let id = manager.register(800, 600, SurfaceFormat::Bgra8888, 123).unwrap();
        
        // Initially not visible
        assert_eq!(manager.presented().count(), 0);
        
        // Submit frame and present
        manager.submit_frame(id, 1, 0x1000, 1024);
        manager.present(id);
        
        assert_eq!(manager.presented().count(), 1);
        
        // Hide
        manager.hide(id);
        assert_eq!(manager.presented().count(), 0);
    }

    #[test]
    fn scenario_backpressure() {
        let mut manager = SurfaceManager::new();
        manager.tick(0);
        
        let id = manager.register(800, 600, SurfaceFormat::Bgra8888, 123).unwrap();
        
        // Fill buffer
        for seq in 1..=MAX_BUFFERED_FRAMES as u64 {
            assert!(manager.submit_frame(id, seq, 0x1000, 1024));
        }
        
        // Next frame should trigger backpressure (returns false)
        assert!(!manager.submit_frame(id, 100, 0x1000, 1024));
        
        // Verify drop count
        let metrics = manager.metrics(id).unwrap();
        assert_eq!(metrics.frames_dropped, 1);
    }
}
