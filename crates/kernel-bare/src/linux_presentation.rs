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
