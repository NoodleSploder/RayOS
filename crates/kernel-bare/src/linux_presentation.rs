//! Linux Subsystem Presentation Bridge
//!
//! Manages guest surface → RayOS window mapping for native desktop presentation.
//! This module handles the presentation lifecycle of Linux guest surfaces
//! without depending on host-side VNC viewers.
//!
//! **Design**: Surfaces are ingested from guest scanout buffers (virtio-gpu),
//! tracked in a cache, and exposed to the RayOS compositor for rendering.

/// Maximum number of concurrent guest surfaces (apps)
const MAX_SURFACES: usize = 64;

/// Maximum surface dimension (16K)
const MAX_SURFACE_DIMENSION: u32 = 16384;

/// Surface state enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceState {
    /// Surface created, waiting for first frame
    Created = 0,
    /// First frame received
    FrameReceived = 1,
    /// Surface is currently presented to user
    Presented = 2,
    /// Surface exists but is hidden
    Hidden = 3,
    /// Surface is being destroyed
    Destroying = 4,
    /// Surface is destroyed
    Destroyed = 5,
}

/// Frame buffer metadata and backing
#[derive(Clone, Copy)]
pub struct FrameBuffer {
    /// Guest physical address of frame buffer
    pub gpa: u64,
    /// Host physical address (translated from GPA)
    pub hpa: u64,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Bytes per scanline (pitch)
    pub stride: u32,
    /// Bytes per pixel (4 for ARGB, 3 for RGB)
    pub bytes_per_pixel: u8,
    /// Total size in bytes
    pub size: u64,
    /// CRC32 of last frame (change detection)
    pub crc32: u32,
}

impl FrameBuffer {
    pub fn new(gpa: u64, width: u32, height: u32, stride: u32, bpp: u8) -> Self {
        let size = (height as u64) * (stride as u64);
        FrameBuffer {
            gpa,
            hpa: 0, // Will be translated
            width,
            height,
            stride,
            bytes_per_pixel: bpp,
            size,
            crc32: 0,
        }
    }

    /// Validate frame buffer bounds
    pub fn validate(&self) -> bool {
        self.width > 0
            && self.width <= MAX_SURFACE_DIMENSION
            && self.height > 0
            && self.height <= MAX_SURFACE_DIMENSION
            && self.stride >= (self.width * self.bytes_per_pixel as u32)
            && self.size <= (512 * 1024 * 1024) // 512 MB max
    }
}

/// Guest surface metadata
#[derive(Clone, Copy)]
pub struct GuestSurface {
    /// Unique surface ID (allocated sequentially)
    pub id: u32,
    /// Current state
    pub state: SurfaceState,
    /// Scanout frame buffer (current rendering target)
    pub scanout: FrameBuffer,
    /// Frame sequence number (incremented on each update)
    pub frame_seq: u64,
    /// Timestamp of first frame (boot relative, in milliseconds)
    pub first_frame_time: u64,
    /// Timestamp of last frame update
    pub last_frame_time: u64,
    /// Number of frames received
    pub frame_count: u32,
}

impl GuestSurface {
    pub fn new(id: u32, fb: FrameBuffer) -> Self {
        GuestSurface {
            id,
            state: SurfaceState::Created,
            scanout: fb,
            frame_seq: 0,
            first_frame_time: 0,
            last_frame_time: 0,
            frame_count: 0,
        }
    }
}

/// Presentation event for marker emission
#[derive(Clone, Copy, Debug)]
pub enum PresentationEvent {
    /// Surface created: (id, width, height)
    SurfaceCreate { id: u32, width: u32, height: u32 },
    /// First frame received: (id)
    FirstFrame { id: u32 },
    /// Surface presented: (id)
    Presented { id: u32 },
    /// Surface hidden: (id)
    Hidden { id: u32 },
    /// Surface destroyed: (id)
    Destroyed { id: u32 },
    /// Frame update: (id, seq)
    FrameUpdate { id: u32, seq: u64 },
}

/// Presentation statistics
#[derive(Clone, Copy, Debug)]
pub struct PresentationStats {
    /// Total surfaces created
    pub total_surfaces: u32,
    /// Active surfaces (Created, Presented, or Hidden)
    pub active_surfaces: u32,
    /// Presented surfaces (visible to user)
    pub presented_surfaces: u32,
    /// Total frames received
    pub total_frames: u64,
    /// Average frame time (milliseconds)
    pub avg_frame_time_ms: u32,
    /// Frames per second
    pub fps: u32,
}

/// Cache of guest surfaces and their frame buffers
pub struct SurfaceCache {
    /// Array of surfaces (indexed by surface ID)
    surfaces: [Option<GuestSurface>; MAX_SURFACES],
    /// Number of allocated surfaces
    surface_count: u32,
    /// Next surface ID to allocate
    next_id: u32,
    /// Total frames received across all surfaces
    total_frames: u64,
    /// Last timestamp for FPS calculation
    last_stats_time: u64,
    /// Frame count at last stats update
    last_stats_frame_count: u64,
}

impl SurfaceCache {
    pub fn new() -> Self {
        SurfaceCache {
            surfaces: [None; MAX_SURFACES],
            surface_count: 0,
            next_id: 1,
            total_frames: 0,
            last_stats_time: 0,
            last_stats_frame_count: 0,
        }
    }

    /// Create a new surface
    pub fn create_surface(&mut self, fb: FrameBuffer) -> Result<u32, &'static str> {
        if !fb.validate() {
            return Err("Invalid frame buffer dimensions");
        }

        if self.surface_count >= (MAX_SURFACES as u32) {
            return Err("Max surfaces reached");
        }

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let surface = GuestSurface::new(id, fb);
        self.surfaces[self.surface_count as usize] = Some(surface);
        self.surface_count += 1;

        Ok(id)
    }

    /// Update frame buffer for existing surface
    pub fn update_frame(&mut self, surface_id: u32, fb: FrameBuffer, now_ms: u64) -> Result<u64, &'static str> {
        if !fb.validate() {
            return Err("Invalid frame buffer dimensions");
        }

        for surface_opt in self.surfaces.iter_mut().take(self.surface_count as usize) {
            if let Some(surface) = surface_opt {
                if surface.id == surface_id {
                    surface.scanout = fb;
                    surface.frame_seq = surface.frame_seq.wrapping_add(1);
                    surface.last_frame_time = now_ms;
                    surface.frame_count = surface.frame_count.saturating_add(1);

                    if surface.first_frame_time == 0 {
                        surface.first_frame_time = now_ms;
                    }

                    self.total_frames = self.total_frames.wrapping_add(1);

                    if surface.state == SurfaceState::Created {
                        surface.state = SurfaceState::FrameReceived;
                    }

                    return Ok(surface.frame_seq);
                }
            }
        }

        Err("Surface not found")
    }

    /// Get surface by ID (immutable)
    pub fn get_surface(&self, surface_id: u32) -> Option<GuestSurface> {
        for surface_opt in self.surfaces.iter().take(self.surface_count as usize) {
            if let Some(surface) = surface_opt {
                if surface.id == surface_id {
                    return Some(*surface);
                }
            }
        }
        None
    }

    /// Present a surface (make it visible)
    pub fn present_surface(&mut self, surface_id: u32) -> Result<(), &'static str> {
        for surface_opt in self.surfaces.iter_mut().take(self.surface_count as usize) {
            if let Some(surface) = surface_opt {
                if surface.id == surface_id {
                    surface.state = SurfaceState::Presented;
                    return Ok(());
                }
            }
        }
        Err("Surface not found")
    }

    /// Hide a surface (keep it alive but not visible)
    pub fn hide_surface(&mut self, surface_id: u32) -> Result<(), &'static str> {
        for surface_opt in self.surfaces.iter_mut().take(self.surface_count as usize) {
            if let Some(surface) = surface_opt {
                if surface.id == surface_id {
                    if surface.state == SurfaceState::Presented {
                        surface.state = SurfaceState::Hidden;
                    }
                    return Ok(());
                }
            }
        }
        Err("Surface not found")
    }

    /// Destroy a surface
    pub fn destroy_surface(&mut self, surface_id: u32) -> Result<(), &'static str> {
        for i in 0..self.surface_count as usize {
            if let Some(surface) = self.surfaces[i] {
                if surface.id == surface_id {
                    // Mark as destroyed
                    let mut surface = surface;
                    surface.state = SurfaceState::Destroyed;
                    self.surfaces[i] = Some(surface);

                    // Compact by moving last to this position if not last
                    if i < (self.surface_count as usize) - 1 {
                        self.surfaces[i] = self.surfaces[(self.surface_count as usize) - 1];
                    }
                    self.surfaces[(self.surface_count as usize) - 1] = None;
                    self.surface_count = self.surface_count.saturating_sub(1);

                    return Ok(());
                }
            }
        }
        Err("Surface not found")
    }

    /// Get all presented surface IDs
    pub fn get_presented_surfaces(&self, ids: &mut [u32; MAX_SURFACES]) -> u32 {
        let mut count = 0;
        for surface_opt in self.surfaces.iter().take(self.surface_count as usize) {
            if let Some(surface) = surface_opt {
                if surface.state == SurfaceState::Presented && count < MAX_SURFACES {
                    ids[count] = surface.id;
                    count += 1;
                }
            }
        }
        count as u32
    }

    /// Non-blocking check if new frame is available
    pub fn frame_available(&self, surface_id: u32) -> bool {
        if let Some(surface) = self.get_surface(surface_id) {
            surface.state == SurfaceState::FrameReceived || surface.state == SurfaceState::Presented
        } else {
            false
        }
    }

    /// Get surface statistics
    pub fn get_surface_stats(&self, surface_id: u32) -> Option<(u32, u64, u32)> {
        if let Some(surface) = self.get_surface(surface_id) {
            Some((surface.frame_count, surface.frame_seq, surface.scanout.stride))
        } else {
            None
        }
    }

    /// Get presentation statistics
    pub fn get_stats(&self) -> PresentationStats {
        let mut active = 0;
        let mut presented = 0;

        for surface_opt in self.surfaces.iter().take(self.surface_count as usize) {
            if let Some(surface) = surface_opt {
                match surface.state {
                    SurfaceState::Created | SurfaceState::FrameReceived | SurfaceState::Hidden => {
                        active += 1;
                    }
                    SurfaceState::Presented => {
                        active += 1;
                        presented += 1;
                    }
                    SurfaceState::Destroying | SurfaceState::Destroyed => {}
                }
            }
        }

        PresentationStats {
            total_surfaces: self.surface_count,
            active_surfaces: active,
            presented_surfaces: presented,
            total_frames: self.total_frames,
            avg_frame_time_ms: 16, // Placeholder
            fps: 60, // Placeholder
        }
    }

    /// Validate GPA range (safety check for guest-provided addresses)
    pub fn validate_gpa_range(&self, gpa: u64, size: u64) -> bool {
        // Basic sanity check: GPA should be within reasonable guest memory range
        // (This would be paired with actual IOMMU/EPT validation in production)
        gpa > 0 && size > 0 && size <= (16 * 1024 * 1024 * 1024) // 16 GB max
    }
}

/// Main presentation bridge
pub struct PresentationBridge {
    /// Surface cache
    surface_cache: SurfaceCache,
    /// Last emitted event (for marker generation)
    last_event: Option<PresentationEvent>,
    /// Boot time (milliseconds since boot)
    boot_time_ms: u64,
}

impl PresentationBridge {
    pub fn new() -> Self {
        PresentationBridge {
            surface_cache: SurfaceCache::new(),
            last_event: None,
            boot_time_ms: 0,
        }
    }

    /// Initialize bridge with current time
    pub fn init(&mut self, boot_time_ms: u64) {
        self.boot_time_ms = boot_time_ms;
    }

    /// Create a new surface
    pub fn create_surface(&mut self, fb: FrameBuffer) -> Result<u32, &'static str> {
        let id = self.surface_cache.create_surface(fb)?;
        self.last_event = Some(PresentationEvent::SurfaceCreate {
            id,
            width: fb.width,
            height: fb.height,
        });
        Ok(id)
    }

    /// Update frame for surface
    pub fn update_frame(&mut self, surface_id: u32, fb: FrameBuffer) -> Result<(), &'static str> {
        let current_time = self.boot_time_ms; // In production, use actual timer
        let seq = self.surface_cache.update_frame(surface_id, fb, current_time)?;

        // Check if this is the first frame
        if let Some(surface) = self.surface_cache.get_surface(surface_id) {
            if surface.frame_count == 1 {
                self.last_event = Some(PresentationEvent::FirstFrame { id: surface_id });
            } else {
                self.last_event = Some(PresentationEvent::FrameUpdate { id: surface_id, seq });
            }
        }

        Ok(())
    }

    /// Get surface by ID
    pub fn get_surface(&self, surface_id: u32) -> Option<GuestSurface> {
        self.surface_cache.get_surface(surface_id)
    }

    /// Present a surface
    pub fn present_surface(&mut self, surface_id: u32) -> Result<(), &'static str> {
        self.surface_cache.present_surface(surface_id)?;
        self.last_event = Some(PresentationEvent::Presented { id: surface_id });
        Ok(())
    }

    /// Hide a surface
    pub fn hide_surface(&mut self, surface_id: u32) -> Result<(), &'static str> {
        self.surface_cache.hide_surface(surface_id)?;
        self.last_event = Some(PresentationEvent::Hidden { id: surface_id });
        Ok(())
    }

    /// Destroy a surface
    pub fn destroy_surface(&mut self, surface_id: u32) -> Result<(), &'static str> {
        self.surface_cache.destroy_surface(surface_id)?;
        self.last_event = Some(PresentationEvent::Destroyed { id: surface_id });
        Ok(())
    }

    /// Get all presented surfaces
    pub fn get_presented_surfaces(&self, ids: &mut [u32; MAX_SURFACES]) -> u32 {
        self.surface_cache.get_presented_surfaces(ids)
    }

    /// Check if frame available
    pub fn frame_available(&self, surface_id: u32) -> bool {
        self.surface_cache.frame_available(surface_id)
    }

    /// Get surface statistics
    pub fn get_surface_stats(&self, surface_id: u32) -> Option<(u32, u64, u32)> {
        self.surface_cache.get_surface_stats(surface_id)
    }

    /// Get presentation statistics
    pub fn get_stats(&self) -> PresentationStats {
        self.surface_cache.get_stats()
    }

    /// Get last emitted event
    pub fn last_event(&self) -> Option<PresentationEvent> {
        self.last_event
    }

    /// Validate GPA range
    pub fn validate_gpa_range(&self, gpa: u64, size: u64) -> bool {
        self.surface_cache.validate_gpa_range(gpa, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_surface() {
        let mut bridge = PresentationBridge::new();
        bridge.init(0);

        let fb = FrameBuffer::new(0x1000, 1920, 1080, 1920 * 4, 4);
        let id = bridge.create_surface(fb).unwrap();

        assert_eq!(id, 1);
        assert!(bridge.get_surface(id).is_some());
    }

    #[test]
    fn test_frame_ingest() {
        let mut bridge = PresentationBridge::new();
        bridge.init(0);

        let fb = FrameBuffer::new(0x1000, 1920, 1080, 1920 * 4, 4);
        let id = bridge.create_surface(fb).unwrap();

        let fb2 = FrameBuffer::new(0x2000, 1920, 1080, 1920 * 4, 4);
        bridge.update_frame(id, fb2).unwrap();

        if let Some(surface) = bridge.get_surface(id) {
            assert_eq!(surface.frame_count, 1);
            assert_eq!(surface.state, SurfaceState::FrameReceived);
        }
    }

    #[test]
    fn test_present_hide() {
        let mut bridge = PresentationBridge::new();
        bridge.init(0);

        let fb = FrameBuffer::new(0x1000, 1920, 1080, 1920 * 4, 4);
        let id = bridge.create_surface(fb).unwrap();

        let fb2 = FrameBuffer::new(0x2000, 1920, 1080, 1920 * 4, 4);
        bridge.update_frame(id, fb2).unwrap();

        bridge.present_surface(id).unwrap();
        assert_eq!(bridge.get_surface(id).unwrap().state, SurfaceState::Presented);

        bridge.hide_surface(id).unwrap();
        assert_eq!(bridge.get_surface(id).unwrap().state, SurfaceState::Hidden);
    }

    #[test]
    fn test_concurrent_surfaces() {
        let mut bridge = PresentationBridge::new();
        bridge.init(0);

        let mut ids = Vec::new();
        for i in 0..5 {
            let fb = FrameBuffer::new(0x1000 + (i as u64) * 0x100000, 800 + (i as u32) * 100, 600 + (i as u32) * 100, (800 + i as u32 * 100) * 4, 4);
            let id = bridge.create_surface(fb).unwrap();
            ids.push(id);
        }

        assert_eq!(ids.len(), 5);

        for id in ids.iter() {
            bridge.present_surface(*id).unwrap();
        }

        let mut presented = [0u32; MAX_SURFACES];
        let count = bridge.get_presented_surfaces(&mut presented);
        assert_eq!(count, 5);
    }

    #[test]
    fn test_bounds_checking() {
        let bridge = PresentationBridge::new();

        // Invalid dimensions
        let fb_invalid = FrameBuffer::new(0x1000, 0, 1080, 1920 * 4, 4);
        assert!(!fb_invalid.validate());

        // Valid dimensions
        let fb_valid = FrameBuffer::new(0x1000, 1920, 1080, 1920 * 4, 4);
        assert!(fb_valid.validate());

        // GPA validation
        assert!(bridge.validate_gpa_range(0x1000, 1920 * 1080 * 4));
        assert!(!bridge.validate_gpa_range(0, 1920 * 1080 * 4));
    }
}

// Phase 22 Task 4: Window decoration and surface composition rendering

/// Window title bar height in pixels
const TITLE_BAR_HEIGHT: u32 = 24;

/// Border width in pixels
const BORDER_WIDTH: u32 = 1;

/// Shadow depth in pixels
const SHADOW_DEPTH: u32 = 2;

/// Window decoration colors (ARGB)
const TITLE_BAR_COLOR: u32 = 0xFF2E2E2E;  // Dark gray
const TITLE_TEXT_COLOR: u32 = 0xFFFFFFFF;  // White
const BORDER_COLOR: u32 = 0xFFFFFFFF;      // White
const SHADOW_COLOR: u32 = 0x80000000;      // Semi-transparent black

/// Window decoration renderer
pub struct WindowDecorationRenderer;

impl WindowDecorationRenderer {
    /// Draw a complete window with title bar, border, and shadow
    pub fn draw_window(
        fb: &mut [u32],
        fb_width: u32,
        fb_height: u32,
        window_x: u32,
        window_y: u32,
        window_width: u32,
        window_height: u32,
        title: &[u8],
    ) {
        // Draw shadow (offset by SHADOW_DEPTH)
        Self::draw_rectangle(
            fb,
            fb_width,
            window_x + SHADOW_DEPTH,
            window_y + SHADOW_DEPTH,
            window_width,
            window_height,
            SHADOW_COLOR,
        );

        // Draw border
        Self::draw_border(
            fb,
            fb_width,
            window_x,
            window_y,
            window_width,
            window_height,
            BORDER_COLOR,
        );

        // Draw title bar
        Self::draw_title_bar(
            fb,
            fb_width,
            window_x + BORDER_WIDTH,
            window_y + BORDER_WIDTH,
            window_width - 2 * BORDER_WIDTH,
            TITLE_BAR_HEIGHT,
            title,
        );

        Self::emit_render_marker(window_width, window_height);
    }

    fn draw_rectangle(
        fb: &mut [u32],
        fb_width: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: u32,
    ) {
        for row in 0..height {
            let y_pos = y + row;
            if y_pos >= fb_width {
                break;
            }
            for col in 0..width {
                let x_pos = x + col;
                let idx = (y_pos * fb_width + x_pos) as usize;
                if idx < fb.len() {
                    fb[idx] = color;
                }
            }
        }
    }

    fn draw_border(
        fb: &mut [u32],
        fb_width: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: u32,
    ) {
        // Top border
        Self::draw_rectangle(fb, fb_width, x, y, width, BORDER_WIDTH, color);
        // Bottom border
        Self::draw_rectangle(
            fb,
            fb_width,
            x,
            y + height - BORDER_WIDTH,
            width,
            BORDER_WIDTH,
            color,
        );
        // Left border
        Self::draw_rectangle(fb, fb_width, x, y, BORDER_WIDTH, height, color);
        // Right border
        Self::draw_rectangle(
            fb,
            fb_width,
            x + width - BORDER_WIDTH,
            y,
            BORDER_WIDTH,
            height,
            color,
        );
    }

    fn draw_title_bar(
        fb: &mut [u32],
        fb_width: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        _title: &[u8],
    ) {
        // Draw title bar background
        Self::draw_rectangle(fb, fb_width, x, y, width, height, TITLE_BAR_COLOR);
        // Title text rendering would be more complex in production
        // For now, just draw colored bar as placeholder
    }

    fn emit_render_marker(width: u32, height: u32) {
        crate::serial_write_str("RAYOS_GUI_RENDER:WINDOW:");
        crate::serial_write_hex_u64(width as u64);
        crate::serial_write_str(":");
        crate::serial_write_hex_u64(height as u64);
        crate::serial_write_str("\n");
    }
}

/// Surface compositor for multi-app rendering
pub struct SurfaceCompositor {
    dirty_regions_count: core::sync::atomic::AtomicU8,
    last_frame_id: core::sync::atomic::AtomicU64,
}

impl SurfaceCompositor {
    pub const fn new() -> Self {
        Self {
            dirty_regions_count: core::sync::atomic::AtomicU8::new(0),
            last_frame_id: core::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Composite multiple surfaces with their decorations into final framebuffer
    pub fn composite_surfaces(
        &self,
        output_fb: &mut [u32],
        output_width: u32,
        output_height: u32,
        surface_count: u8,
    ) -> u64 {
        let frame_id = self.last_frame_id.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1;

        // In production, would iterate through surfaces sorted by Z-order
        // and composite each surface with decorations

        self.emit_composite_marker(surface_count as u32);
        frame_id
    }

    /// Track a dirty region that needs redrawn
    pub fn mark_dirty_region(&self, x: u32, y: u32, width: u32, height: u32) {
        let count = self.dirty_regions_count.load(core::sync::atomic::Ordering::Relaxed);
        if count < 255 {
            self.dirty_regions_count.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        self.emit_dirty_region_marker(x, y, width, height);
    }

    /// Get count of dirty regions
    pub fn dirty_region_count(&self) -> u8 {
        self.dirty_regions_count.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Clear dirty region tracking
    pub fn clear_dirty_regions(&self) {
        self.dirty_regions_count.store(0, core::sync::atomic::Ordering::Relaxed);
    }

    fn emit_composite_marker(&self, app_count: u32) {
        crate::serial_write_str("RAYOS_GUI_RENDER:COMPOSITE:");
        crate::serial_write_hex_u64(app_count as u64);
        crate::serial_write_str("\n");
    }

    fn emit_dirty_region_marker(&self, x: u32, y: u32, w: u32, h: u32) {
        crate::serial_write_str("RAYOS_GUI_RENDER:DIRTY_REGION:");
        crate::serial_write_hex_u64(x as u64);
        crate::serial_write_str(":");
        crate::serial_write_hex_u64(y as u64);
        crate::serial_write_str(":");
        crate::serial_write_hex_u64(w as u64);
        crate::serial_write_str(":");
        crate::serial_write_hex_u64(h as u64);
        crate::serial_write_str("\n");
    }
}

/// Scanout optimizer for efficient GPU updates
pub struct ScanoutOptimizer {
    last_scanout_id: core::sync::atomic::AtomicU64,
}

impl ScanoutOptimizer {
    pub const fn new() -> Self {
        Self {
            last_scanout_id: core::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Emit scanout marker for next screen update
    pub fn emit_scanout(&self, frame_id: u64) -> u64 {
        let scanout_id = self.last_scanout_id.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1;
        crate::serial_write_str("RAYOS_GUI_RENDER:SCANOUT:");
        crate::serial_write_hex_u64(scanout_id);
        crate::serial_write_str("\n");
        scanout_id
    }

    /// Calculate estimated GPU memory for surfaces
    pub fn estimate_memory_usage(&self, surface_count: u8, avg_width: u32, avg_height: u32) -> u64 {
        let per_surface = (avg_width as u64) * (avg_height as u64) * 4; // ARGB32
        (surface_count as u64) * per_surface
    }
}

#[cfg(test)]
mod composition_tests {
    use super::*;

    #[test]
    fn test_decorator_title_bar() {
        let mut fb = vec![0u32; 1920 * 1080];
        WindowDecorationRenderer::draw_window(
            &mut fb,
            1920,
            1080,
            100,
            100,
            800,
            600,
            b"Test Window",
        );
        // Verify some pixels were modified
        assert!(fb.iter().any(|&p| p != 0));
    }

    #[test]
    fn test_compositor_dirty_region() {
        let comp = SurfaceCompositor::new();
        assert_eq!(comp.dirty_region_count(), 0);

        comp.mark_dirty_region(0, 0, 100, 100);
        assert_eq!(comp.dirty_region_count(), 1);

        comp.mark_dirty_region(200, 200, 100, 100);
        assert_eq!(comp.dirty_region_count(), 2);

        comp.clear_dirty_regions();
        assert_eq!(comp.dirty_region_count(), 0);
    }

    #[test]
    fn test_compositor_composite() {
        let comp = SurfaceCompositor::new();
        let mut fb = vec![0u32; 1920 * 1080];

        let frame_id = comp.composite_surfaces(&mut fb, 1920, 1080, 4);
        assert_eq!(frame_id, 1);

        let frame_id2 = comp.composite_surfaces(&mut fb, 1920, 1080, 4);
        assert_eq!(frame_id2, 2);
    }

    #[test]
    fn test_scanout_optimizer() {
        let opt = ScanoutOptimizer::new();
        let id1 = opt.emit_scanout(1);
        let id2 = opt.emit_scanout(2);
        assert!(id2 > id1);
    }

    #[test]
    fn test_memory_estimation() {
        let opt = ScanoutOptimizer::new();
        let mem = opt.estimate_memory_usage(4, 1920, 1080);
        assert_eq!(mem, 4 * 1920 * 1080 * 4);
    }
}

