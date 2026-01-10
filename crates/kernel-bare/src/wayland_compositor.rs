// ===== Phase 23 Task 2: Wayland Compositor & Surfaces =====
// Implements wl_compositor, wl_surface, wl_shm, and wl_buffer
// Provides surface management, buffer attachment, and shared memory allocation


// Surface limits
const MAX_SURFACES: usize = 32;
const MAX_BUFFERS: usize = 64;
const MAX_SHM_POOLS: usize = 16;
const MAX_REGIONS: usize = 32;

// Buffer format support
const BUFFER_FORMAT_ARGB8888: u32 = 0;
const BUFFER_FORMAT_XRGB8888: u32 = 1;

// Surface state flags
const SURFACE_STATE_DIRTY: u32 = 0x01;
const SURFACE_STATE_MAPPED: u32 = 0x02;

/// Shared Memory Pool
#[derive(Clone, Copy)]
pub struct ShmPool {
    id: u32,
    pool_id: u32,
    size: usize,
    offset: usize,
    in_use: bool,
}

impl ShmPool {
    const UNINIT: Self = ShmPool {
        id: 0,
        pool_id: 0,
        size: 0,
        offset: 0,
        in_use: false,
    };

    fn new(id: u32, pool_id: u32, size: usize) -> Self {
        ShmPool {
            id,
            pool_id,
            size,
            offset: 0,
            in_use: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_pool_id(&self) -> u32 {
        self.pool_id
    }

    pub fn get_size(&self) -> usize {
        self.size
    }

    pub fn resize(&mut self, new_size: usize) {
        self.size = new_size;
    }
}

/// Wayland Buffer
#[derive(Clone, Copy)]
pub struct WaylandBuffer {
    id: u32,
    pool_id: u32,
    offset: usize,
    width: u32,
    height: u32,
    stride: u32,
    format: u32,
    in_use: bool,
    referenced: bool,
}

impl WaylandBuffer {
    const UNINIT: Self = WaylandBuffer {
        id: 0,
        pool_id: 0,
        offset: 0,
        width: 0,
        height: 0,
        stride: 0,
        format: 0,
        in_use: false,
        referenced: false,
    };

    fn new(id: u32, pool_id: u32, offset: usize, width: u32, height: u32, stride: u32, format: u32) -> Self {
        WaylandBuffer {
            id,
            pool_id,
            offset,
            width,
            height,
            stride,
            format,
            in_use: true,
            referenced: true,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }

    pub fn get_stride(&self) -> u32 {
        self.stride
    }

    pub fn get_format(&self) -> u32 {
        self.format
    }

    pub fn release(&mut self) {
        self.referenced = false;
    }

    pub fn is_referenced(&self) -> bool {
        self.referenced
    }
}

/// Damage Region (rectangle)
#[derive(Clone, Copy)]
pub struct DamageRegion {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    in_use: bool,
}

impl DamageRegion {
    const UNINIT: Self = DamageRegion {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        in_use: false,
    };

    fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        DamageRegion {
            x,
            y,
            width,
            height,
            in_use: true,
        }
    }

    pub fn intersects(&self, other: &DamageRegion) -> bool {
        self.x < (other.x + other.width as i32) &&
        (self.x + self.width as i32) > other.x &&
        self.y < (other.y + other.height as i32) &&
        (self.y + self.height as i32) > other.y
    }
}

/// Viewport transformation
#[derive(Clone, Copy)]
pub struct Viewport {
    scale: i32,
    src_x: i32,
    src_y: i32,
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
}

impl Viewport {
    pub fn new() -> Self {
        Viewport {
            scale: 1,
            src_x: 0,
            src_y: 0,
            src_width: 0,
            src_height: 0,
            dst_width: 0,
            dst_height: 0,
        }
    }

    pub fn set_scale(&mut self, scale: i32) {
        self.scale = scale.max(1);
    }

    pub fn set_source(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.src_x = x;
        self.src_y = y;
        self.src_width = width;
        self.src_height = height;
    }

    pub fn set_destination(&mut self, width: u32, height: u32) {
        self.dst_width = width;
        self.dst_height = height;
    }

    pub fn get_scale(&self) -> i32 {
        self.scale
    }
}

/// Wayland Surface
#[derive(Clone, Copy)]
pub struct Surface {
    id: u32,
    in_use: bool,
    current_buffer: Option<u32>,
    pending_buffer: Option<u32>,
    state_flags: u32,
    damage_regions: [Option<DamageRegion>; 4],
    damage_count: usize,
    viewport: Viewport,
    role: u32, // 0=unassigned, 1=toplevel, 2=popup
}

impl Surface {
    const UNINIT: Self = Surface {
        id: 0,
        in_use: false,
        current_buffer: None,
        pending_buffer: None,
        state_flags: 0,
        damage_regions: [None; 4],
        damage_count: 0,
        viewport: Viewport {
            scale: 1,
            src_x: 0,
            src_y: 0,
            src_width: 0,
            src_height: 0,
            dst_width: 0,
            dst_height: 0,
        },
        role: 0,
    };

    fn new(id: u32) -> Self {
        Surface {
            id,
            in_use: true,
            current_buffer: None,
            pending_buffer: None,
            state_flags: SURFACE_STATE_DIRTY,
            damage_regions: [None; 4],
            damage_count: 0,
            viewport: Viewport::new(),
            role: 0,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn attach_buffer(&mut self, buffer_id: Option<u32>) -> Result<(), &'static str> {
        if buffer_id.is_some() {
            self.pending_buffer = buffer_id;
            unsafe {
                if let Some(_output) = core::fmt::write(
                    &mut Logger,
                    format_args!("[RAYOS_SURFACE:BUFFER_ATTACHED] surface_id={} buffer_id={}\n",
                        self.id, buffer_id.unwrap_or(0))
                ).ok() {
                    // Marker emitted
                }
            }
            Ok(())
        } else {
            self.pending_buffer = None;
            Ok(())
        }
    }

    pub fn commit(&mut self) -> Result<(), &'static str> {
        self.current_buffer = self.pending_buffer;
        self.state_flags &= !SURFACE_STATE_DIRTY;
        self.state_flags |= SURFACE_STATE_MAPPED;

        unsafe {
            if let Some(_output) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SURFACE:COMMIT] surface_id={} buffer_id={}\n",
                    self.id, self.current_buffer.unwrap_or(0))
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn damage(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<(), &'static str> {
        if self.damage_count < 4 {
            let region = DamageRegion::new(x, y, width, height);
            self.damage_regions[self.damage_count] = Some(region);
            self.damage_count += 1;
            self.state_flags |= SURFACE_STATE_DIRTY;

            unsafe {
                if let Some(_output) = core::fmt::write(
                    &mut Logger,
                    format_args!("[RAYOS_SURFACE:DAMAGE] surface_id={} x={} y={} w={} h={}\n",
                        self.id, x, y, width, height)
                ).ok() {
                    // Marker emitted
                }
            }
            Ok(())
        } else {
            Err("damage buffer full")
        }
    }

    pub fn set_viewport(&mut self, scale: i32, src_x: i32, src_y: i32, src_width: u32, src_height: u32, dst_width: u32, dst_height: u32) -> Result<(), &'static str> {
        self.viewport.set_scale(scale);
        self.viewport.set_source(src_x, src_y, src_width, src_height);
        self.viewport.set_destination(dst_width, dst_height);

        unsafe {
            if let Some(_output) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SURFACE:VIEWPORT_SET] surface_id={} scale={}\n",
                    self.id, scale)
            ).ok() {
                // Marker emitted
            }
        }
        Ok(())
    }

    pub fn destroy(&mut self) {
        self.in_use = false;
        self.current_buffer = None;
        self.pending_buffer = None;

        unsafe {
            if let Some(_output) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SURFACE:DESTROY] surface_id={}\n", self.id)
            ).ok() {
                // Marker emitted
            }
        }
    }

    pub fn get_current_buffer(&self) -> Option<u32> {
        self.current_buffer
    }

    pub fn is_mapped(&self) -> bool {
        (self.state_flags & SURFACE_STATE_MAPPED) != 0
    }

    pub fn is_dirty(&self) -> bool {
        (self.state_flags & SURFACE_STATE_DIRTY) != 0
    }

    pub fn reset_damage(&mut self) {
        self.damage_count = 0;
        self.damage_regions = [None; 4];
        self.state_flags &= !SURFACE_STATE_DIRTY;
    }

    pub fn get_viewport(&self) -> &Viewport {
        &self.viewport
    }
}

/// Wayland Compositor
pub struct WaylandCompositor {
    id: u32,
    surfaces: [Surface; MAX_SURFACES],
    surface_count: usize,
    buffers: [WaylandBuffer; MAX_BUFFERS],
    buffer_count: usize,
    pools: [ShmPool; MAX_SHM_POOLS],
    pool_count: usize,
    next_surface_id: u32,
    next_buffer_id: u32,
    next_pool_id: u32,
}

impl WaylandCompositor {
    pub fn new() -> Self {
        unsafe {
            if let Some(_output) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_COMPOSITOR:CREATE] compositor_id=1\n")
            ).ok() {
                // Marker emitted
            }
        }

        WaylandCompositor {
            id: 1,
            surfaces: [Surface::UNINIT; MAX_SURFACES],
            surface_count: 0,
            buffers: [WaylandBuffer::UNINIT; MAX_BUFFERS],
            buffer_count: 0,
            pools: [ShmPool::UNINIT; MAX_SHM_POOLS],
            pool_count: 0,
            next_surface_id: 10,
            next_buffer_id: 100,
            next_pool_id: 1000,
        }
    }

    pub fn advertise_global(&self) {
        unsafe {
            if let Some(_output) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_COMPOSITOR:GLOBAL_ADVERTISED] interface=wl_compositor version=4\n")
            ).ok() {
                // Marker emitted
            }
        }
    }

    pub fn create_surface(&mut self) -> Result<u32, &'static str> {
        if self.surface_count >= MAX_SURFACES {
            return Err("surface limit exceeded");
        }

        let surface_id = self.next_surface_id;
        self.next_surface_id += 1;

        let surface = Surface::new(surface_id);
        self.surfaces[self.surface_count] = surface;
        self.surface_count += 1;

        Ok(surface_id)
    }

    pub fn create_region(&mut self) -> Result<(), &'static str> {
        // Regions are simple damage rectangles managed by surfaces
        Ok(())
    }

    pub fn get_surface(&self, surface_id: u32) -> Option<&Surface> {
        self.surfaces[..self.surface_count]
            .iter()
            .find(|s| s.in_use && s.id == surface_id)
    }

    pub fn get_surface_mut(&mut self, surface_id: u32) -> Option<&mut Surface> {
        self.surfaces[..self.surface_count]
            .iter_mut()
            .find(|s| s.in_use && s.id == surface_id)
    }

    pub fn destroy_surface(&mut self, surface_id: u32) -> Result<(), &'static str> {
        for surface in self.surfaces[..self.surface_count].iter_mut() {
            if surface.in_use && surface.id == surface_id {
                surface.destroy();
                return Ok(());
            }
        }
        Err("surface not found")
    }

    pub fn create_shm_pool(&mut self, size: usize) -> Result<u32, &'static str> {
        if self.pool_count >= MAX_SHM_POOLS {
            return Err("pool limit exceeded");
        }

        let pool_id = self.next_pool_id;
        self.next_pool_id += 1;

        let pool = ShmPool::new(self.pool_count as u32, pool_id, size);
        self.pools[self.pool_count] = pool;
        self.pool_count += 1;

        unsafe {
            if let Some(_output) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SHM:POOL_CREATE] pool_id={} size={}\n", pool_id, size)
            ).ok() {
                // Marker emitted
            }
        }

        Ok(pool_id)
    }

    pub fn resize_pool(&mut self, pool_id: u32, new_size: usize) -> Result<(), &'static str> {
        for pool in self.pools[..self.pool_count].iter_mut() {
            if pool.in_use && pool.pool_id == pool_id {
                pool.resize(new_size);
                return Ok(());
            }
        }
        Err("pool not found")
    }

    pub fn create_buffer(
        &mut self,
        pool_id: u32,
        offset: usize,
        width: u32,
        height: u32,
        stride: u32,
        format: u32,
    ) -> Result<u32, &'static str> {
        if self.buffer_count >= MAX_BUFFERS {
            return Err("buffer limit exceeded");
        }

        // Validate format
        if format != BUFFER_FORMAT_ARGB8888 && format != BUFFER_FORMAT_XRGB8888 {
            return Err("unsupported format");
        }

        // Find pool and check bounds
        let mut pool_found = false;
        for pool in self.pools[..self.pool_count].iter() {
            if pool.in_use && pool.pool_id == pool_id {
                let required_size = offset + (height * stride) as usize;
                if required_size > pool.size {
                    return Err("buffer exceeds pool size");
                }
                pool_found = true;
                break;
            }
        }

        if !pool_found {
            return Err("pool not found");
        }

        let buffer_id = self.next_buffer_id;
        self.next_buffer_id += 1;

        let buffer = WaylandBuffer::new(buffer_id, pool_id, offset, width, height, stride, format);
        self.buffers[self.buffer_count] = buffer;
        self.buffer_count += 1;

        unsafe {
            if let Some(_output) = core::fmt::write(
                &mut Logger,
                format_args!("[RAYOS_SHM:BUFFER_CREATE] buffer_id={} w={} h={} format={}\n",
                    buffer_id, width, height, format)
            ).ok() {
                // Marker emitted
            }
        }

        Ok(buffer_id)
    }

    pub fn get_buffer(&self, buffer_id: u32) -> Option<&WaylandBuffer> {
        self.buffers[..self.buffer_count]
            .iter()
            .find(|b| b.in_use && b.id == buffer_id)
    }

    pub fn get_buffer_mut(&mut self, buffer_id: u32) -> Option<&mut WaylandBuffer> {
        self.buffers[..self.buffer_count]
            .iter_mut()
            .find(|b| b.in_use && b.id == buffer_id)
    }

    pub fn release_buffer(&mut self, buffer_id: u32) -> Result<(), &'static str> {
        for buffer in self.buffers[..self.buffer_count].iter_mut() {
            if buffer.in_use && buffer.id == buffer_id {
                buffer.release();
                return Ok(());
            }
        }
        Err("buffer not found")
    }

    pub fn get_surface_count(&self) -> usize {
        self.surfaces[..self.surface_count].iter().filter(|s| s.in_use).count()
    }

    pub fn get_buffer_count(&self) -> usize {
        self.buffers[..self.buffer_count].iter().filter(|b| b.in_use).count()
    }

    pub fn get_pool_count(&self) -> usize {
        self.pools[..self.pool_count].iter().filter(|p| p.in_use).count()
    }
}

// Simple logging helper
struct Logger;

impl core::fmt::Write for Logger {
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        // In a real implementation, this would write to kernel log
        // For now, it's a no-op but the format_args! call still executes
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compositor_creation() {
        let compositor = WaylandCompositor::new();
        assert_eq!(compositor.id, 1);
        assert_eq!(compositor.get_surface_count(), 0);
        assert_eq!(compositor.get_buffer_count(), 0);
        assert_eq!(compositor.get_pool_count(), 0);
    }

    #[test]
    fn test_surface_creation() {
        let mut compositor = WaylandCompositor::new();
        let result = compositor.create_surface();
        assert!(result.is_ok());
        assert_eq!(compositor.get_surface_count(), 1);

        let surface_id = result.unwrap();
        let surface = compositor.get_surface(surface_id);
        assert!(surface.is_some());
        assert_eq!(surface.unwrap().get_id(), surface_id);
    }

    #[test]
    fn test_buffer_attachment() {
        let mut compositor = WaylandCompositor::new();
        let surface_id = compositor.create_surface().unwrap();
        let surface = compositor.get_surface_mut(surface_id).unwrap();

        let result = surface.attach_buffer(Some(100));
        assert!(result.is_ok());
        assert!(surface.get_current_buffer().is_none()); // Not committed yet
    }

    #[test]
    fn test_surface_commit() {
        let mut compositor = WaylandCompositor::new();
        let surface_id = compositor.create_surface().unwrap();
        let surface = compositor.get_surface_mut(surface_id).unwrap();

        surface.attach_buffer(Some(100)).unwrap();
        surface.commit().unwrap();

        assert_eq!(surface.get_current_buffer(), Some(100));
        assert!(surface.is_mapped());
    }

    #[test]
    fn test_damage_tracking() {
        let mut compositor = WaylandCompositor::new();
        let surface_id = compositor.create_surface().unwrap();
        let surface = compositor.get_surface_mut(surface_id).unwrap();

        let result = surface.damage(10, 20, 100, 100);
        assert!(result.is_ok());
        assert!(surface.is_dirty());
    }

    #[test]
    fn test_shm_pool_allocation() {
        let mut compositor = WaylandCompositor::new();
        let result = compositor.create_shm_pool(4096);
        assert!(result.is_ok());
        assert_eq!(compositor.get_pool_count(), 1);
    }

    #[test]
    fn test_buffer_creation() {
        let mut compositor = WaylandCompositor::new();
        let pool_id = compositor.create_shm_pool(4096).unwrap();

        let result = compositor.create_buffer(pool_id, 0, 640, 480, 2560, BUFFER_FORMAT_ARGB8888);
        assert!(result.is_ok());
        assert_eq!(compositor.get_buffer_count(), 1);
    }

    #[test]
    fn test_buffer_formats() {
        let mut compositor = WaylandCompositor::new();
        let pool_id = compositor.create_shm_pool(8192).unwrap();

        let argb = compositor.create_buffer(pool_id, 0, 640, 480, 2560, BUFFER_FORMAT_ARGB8888);
        assert!(argb.is_ok());

        let xrgb = compositor.create_buffer(pool_id, 1024, 640, 480, 2560, BUFFER_FORMAT_XRGB8888);
        assert!(xrgb.is_ok());

        let invalid = compositor.create_buffer(pool_id, 2048, 640, 480, 2560, 99);
        assert!(invalid.is_err());
    }

    #[test]
    fn test_viewport_scaling() {
        let mut compositor = WaylandCompositor::new();
        let surface_id = compositor.create_surface().unwrap();
        let surface = compositor.get_surface_mut(surface_id).unwrap();

        let result = surface.set_viewport(2, 0, 0, 640, 480, 1280, 960);
        assert!(result.is_ok());

        assert_eq!(surface.get_viewport().get_scale(), 2);
    }

    #[test]
    fn test_region_creation() {
        let mut compositor = WaylandCompositor::new();
        let result = compositor.create_region();
        assert!(result.is_ok());
    }

    #[test]
    fn test_region_operations() {
        let region1 = DamageRegion::new(0, 0, 100, 100);
        let region2 = DamageRegion::new(50, 50, 100, 100);

        assert!(region1.intersects(&region2));

        let region3 = DamageRegion::new(200, 200, 100, 100);
        assert!(!region1.intersects(&region3));
    }

    #[test]
    fn test_buffer_lifecycle() {
        let mut compositor = WaylandCompositor::new();
        let pool_id = compositor.create_shm_pool(4096).unwrap();
        let buffer_id = compositor.create_buffer(pool_id, 0, 640, 480, 2560, BUFFER_FORMAT_ARGB8888).unwrap();

        let buffer = compositor.get_buffer(buffer_id).unwrap();
        assert!(buffer.is_referenced());

        let _ = compositor.release_buffer(buffer_id);
        let buffer = compositor.get_buffer(buffer_id).unwrap();
        assert!(!buffer.is_referenced());
    }

    #[test]
    fn test_multiple_surfaces() {
        let mut compositor = WaylandCompositor::new();

        for _ in 0..10 {
            let result = compositor.create_surface();
            assert!(result.is_ok());
        }

        assert_eq!(compositor.get_surface_count(), 10);
    }

    #[test]
    fn test_surface_destruction() {
        let mut compositor = WaylandCompositor::new();
        let surface_id = compositor.create_surface().unwrap();

        assert_eq!(compositor.get_surface_count(), 1);
        let result = compositor.destroy_surface(surface_id);
        assert!(result.is_ok());
        assert_eq!(compositor.get_surface_count(), 0);
    }

    #[test]
    fn test_compositor_performance() {
        let mut compositor = WaylandCompositor::new();

        // Create multiple surfaces and buffers
        for _ in 0..10 {
            let surface_id = compositor.create_surface().unwrap();
            let surface = compositor.get_surface_mut(surface_id).unwrap();
            surface.attach_buffer(Some(100)).unwrap();
            surface.commit().unwrap();
        }

        // Create multiple pools and buffers
        for i in 0..8 {
            let pool_id = compositor.create_shm_pool(4096).unwrap();
            for j in 0..8 {
                let offset = (j * 512) as usize;
                let _ = compositor.create_buffer(pool_id, offset, 320, 240, 1280, BUFFER_FORMAT_ARGB8888);
            }
        }

        // Verify counts
        assert_eq!(compositor.get_surface_count(), 10);
        assert!(compositor.get_buffer_count() > 0);
        assert_eq!(compositor.get_pool_count(), 8);
    }
}
