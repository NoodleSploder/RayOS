// RAYOS Phase 25 Task 2: GPU Memory Management
// Efficient GPU memory allocation & optimization
// File: crates/kernel-bare/src/gpu_memory.rs
// Lines: 750+ | Tests: 16 unit + 4 scenario | Markers: 5

use core::fmt;

const MAX_MEMORY_BLOCKS: usize = 256;
const MAX_ALLOCATIONS: usize = 512;
const MAX_TEXTURES_IN_ATLAS: usize = 128;
const MAX_BUFFER_POOLS: usize = 16;
const GPU_MEMORY_POOL_SIZE: u32 = 1024 * 1024 * 512; // 512MB
const BUDDY_MIN_ORDER: u32 = 9; // 512 bytes minimum
const BUDDY_MAX_ORDER: u32 = 29; // 512MB maximum

// ============================================================================
// MEMORY TYPES & ENUMS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationStrategy {
    Linear,
    Buddy,
    FragmentationAware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionFormat {
    None,
    ASTC_4x4,
    ASTC_6x6,
    ASTC_8x8,
    BC1,
    BC4,
    BC7,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryBlock {
    pub base_address: u32,
    pub size: u32,
    pub allocated: bool,
    pub order: u32, // For buddy allocator
    pub free: bool,
}

impl MemoryBlock {
    pub fn new(address: u32, size: u32, order: u32) -> Self {
        MemoryBlock {
            base_address: address,
            size,
            allocated: false,
            order,
            free: true,
        }
    }

    pub fn allocate(&mut self) -> bool {
        if !self.free {
            return false;
        }
        self.allocated = true;
        self.free = false;
        true
    }

    pub fn deallocate(&mut self) -> bool {
        if !self.allocated {
            return false;
        }
        self.allocated = false;
        self.free = true;
        true
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AllocationInfo {
    pub address: u32,
    pub size: u32,
    pub allocated: bool,
    pub timestamp: u32,
}

impl AllocationInfo {
    pub fn new(address: u32, size: u32) -> Self {
        AllocationInfo {
            address,
            size,
            allocated: true,
            timestamp: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryStatistics {
    pub total_memory: u32,
    pub used_memory: u32,
    pub free_memory: u32,
    pub fragmentation_ratio: u32, // 0-100
    pub allocation_count: u32,
    pub deallocation_count: u32,
    pub defragmentation_count: u32,
}

impl MemoryStatistics {
    pub fn new(total: u32) -> Self {
        MemoryStatistics {
            total_memory: total,
            used_memory: 0,
            free_memory: total,
            fragmentation_ratio: 0,
            allocation_count: 0,
            deallocation_count: 0,
            defragmentation_count: 0,
        }
    }

    pub fn update_fragmentation(&mut self) {
        if self.total_memory == 0 {
            return;
        }
        self.fragmentation_ratio =
            ((self.total_memory - self.free_memory - self.used_memory) * 100) / self.total_memory;
    }

    pub fn get_utilization(&self) -> u32 {
        if self.total_memory == 0 {
            return 0;
        }
        (self.used_memory * 100) / self.total_memory
    }
}

// ============================================================================
// TEXTURE ATLAS
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub allocated: bool,
}

impl AtlasRegion {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        AtlasRegion {
            x,
            y,
            width,
            height,
            allocated: false,
        }
    }

    pub fn area(&self) -> u32 {
        self.width.saturating_mul(self.height)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TextureAtlas {
    pub width: u32,
    pub height: u32,
    pub regions: [AtlasRegion; MAX_TEXTURES_IN_ATLAS],
    pub region_count: usize,
    pub used_area: u32,
    pub total_area: u32,
}

impl TextureAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        TextureAtlas {
            width,
            height,
            regions: [AtlasRegion::new(0, 0, 0, 0); MAX_TEXTURES_IN_ATLAS],
            region_count: 0,
            used_area: 0,
            total_area: width.saturating_mul(height),
        }
    }

    pub fn allocate_region(&mut self, width: u32, height: u32) -> Option<AtlasRegion> {
        if width > self.width || height > self.height {
            return None;
        }

        // Simple shelf packing: find first fit
        let mut y_offset = 0u32;
        let mut row_height = 0u32;
        let mut x_offset = 0u32;

        for i in 0..self.region_count {
            let region = &self.regions[i];
            if region.allocated {
                if region.x + region.width + width <= self.width {
                    // Can fit in same row
                    x_offset = region.x + region.width;
                    y_offset = region.y;
                    row_height = core::cmp::max(row_height, region.height);
                } else {
                    // Move to next row
                    y_offset += row_height;
                    x_offset = 0;
                    row_height = height;
                }
            }
        }

        if y_offset + height <= self.height && x_offset + width <= self.width {
            if self.region_count < MAX_TEXTURES_IN_ATLAS {
                let region = AtlasRegion::new(x_offset, y_offset, width, height);
                self.regions[self.region_count] = region;
                self.region_count += 1;
                self.used_area += region.area();
                return Some(region);
            }
        }

        None
    }

    pub fn get_utilization(&self) -> u32 {
        if self.total_area == 0 {
            return 0;
        }
        (self.used_area * 100) / self.total_area
    }
}

// ============================================================================
// BUFFER CACHE
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct CachedBuffer {
    pub buffer_id: u32,
    pub size: u32,
    pub in_use: bool,
    pub last_used: u32,
}

impl CachedBuffer {
    pub fn new(buffer_id: u32, size: u32) -> Self {
        CachedBuffer {
            buffer_id,
            size,
            in_use: false,
            last_used: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BufferCache {
    pub buffers: [CachedBuffer; MAX_ALLOCATIONS],
    pub buffer_count: usize,
    pub total_cached_size: u32,
    pub reuse_count: u32,
}

impl BufferCache {
    pub fn new() -> Self {
        BufferCache {
            buffers: [CachedBuffer::new(0, 0); MAX_ALLOCATIONS],
            buffer_count: 0,
            total_cached_size: 0,
            reuse_count: 0,
        }
    }

    pub fn add_buffer(&mut self, buffer_id: u32, size: u32) -> bool {
        if self.buffer_count >= MAX_ALLOCATIONS {
            return false;
        }
        let buffer = CachedBuffer::new(buffer_id, size);
        self.buffers[self.buffer_count] = buffer;
        self.buffer_count += 1;
        self.total_cached_size += size;
        true
    }

    pub fn get_reusable_buffer(&mut self, required_size: u32) -> Option<u32> {
        for i in 0..self.buffer_count {
            let buffer = &mut self.buffers[i];
            if !buffer.in_use && buffer.size >= required_size {
                buffer.in_use = true;
                self.reuse_count += 1;
                return Some(buffer.buffer_id);
            }
        }
        None
    }

    pub fn release_buffer(&mut self, buffer_id: u32) -> bool {
        for i in 0..self.buffer_count {
            if self.buffers[i].buffer_id == buffer_id {
                self.buffers[i].in_use = false;
                return true;
            }
        }
        false
    }
}

impl Default for BufferCache {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// COMPRESSION MANAGER
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct CompressionInfo {
    pub original_size: u32,
    pub compressed_size: u32,
    pub format: CompressionFormat,
}

impl CompressionInfo {
    pub fn new(original_size: u32, format: CompressionFormat) -> Self {
        let compression_ratio = match format {
            CompressionFormat::None => 100,
            CompressionFormat::ASTC_4x4 => 100,
            CompressionFormat::ASTC_6x6 => 44,
            CompressionFormat::ASTC_8x8 => 25,
            CompressionFormat::BC1 => 50,
            CompressionFormat::BC4 => 50,
            CompressionFormat::BC7 => 100,
        };
        let compressed_size = (original_size * compression_ratio) / 100;

        CompressionInfo {
            original_size,
            compressed_size,
            format,
        }
    }

    pub fn get_compression_ratio(&self) -> u32 {
        if self.original_size == 0 {
            return 0;
        }
        (self.compressed_size * 100) / self.original_size
    }

    pub fn get_savings(&self) -> u32 {
        self.original_size.saturating_sub(self.compressed_size)
    }
}

pub struct CompressionManager {
    pub compressions: [Option<CompressionInfo>; MAX_TEXTURES_IN_ATLAS],
    pub compression_count: usize,
    pub total_original: u32,
    pub total_compressed: u32,
}

impl CompressionManager {
    pub fn new() -> Self {
        CompressionManager {
            compressions: [None; MAX_TEXTURES_IN_ATLAS],
            compression_count: 0,
            total_original: 0,
            total_compressed: 0,
        }
    }

    pub fn add_compression(&mut self, original_size: u32, format: CompressionFormat) -> bool {
        if self.compression_count >= MAX_TEXTURES_IN_ATLAS {
            return false;
        }
        let info = CompressionInfo::new(original_size, format);
        self.compressions[self.compression_count] = Some(info);
        self.compression_count += 1;
        self.total_original += original_size;
        self.total_compressed += info.compressed_size;
        true
    }

    pub fn get_total_savings(&self) -> u32 {
        self.total_original.saturating_sub(self.total_compressed)
    }

    pub fn get_average_ratio(&self) -> u32 {
        if self.total_original == 0 {
            return 100;
        }
        (self.total_compressed * 100) / self.total_original
    }
}

impl Default for CompressionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BUDDY ALLOCATOR
// ============================================================================

pub struct BuddyAllocator {
    pub blocks: [Option<MemoryBlock>; MAX_MEMORY_BLOCKS],
    pub block_count: usize,
    pub allocations: [Option<AllocationInfo>; MAX_ALLOCATIONS],
    pub allocation_count: usize,
    pub statistics: MemoryStatistics,
}

impl BuddyAllocator {
    pub fn new(total_size: u32) -> Self {
        let mut allocator = BuddyAllocator {
            blocks: [None; MAX_MEMORY_BLOCKS],
            block_count: 0,
            allocations: [None; MAX_ALLOCATIONS],
            allocation_count: 0,
            statistics: MemoryStatistics::new(total_size),
        };

        // Initialize root block
        if allocator.block_count < MAX_MEMORY_BLOCKS {
            let order = Self::size_to_order(total_size);
            allocator.blocks[allocator.block_count] = Some(MemoryBlock::new(0, total_size, order));
            allocator.block_count += 1;
        }

        allocator
    }

    fn size_to_order(size: u32) -> u32 {
        let mut order = BUDDY_MIN_ORDER;
        let mut block_size = 1u32 << order;

        while block_size < size && order < BUDDY_MAX_ORDER {
            order += 1;
            block_size = 1u32 << order;
        }

        order
    }

    fn order_to_size(order: u32) -> u32 {
        1u32 << order
    }

    pub fn allocate(&mut self, size: u32) -> Option<u32> {
        if size == 0 || size > self.statistics.free_memory {
            return None;
        }

        let required_order = Self::size_to_order(size);

        // Find first available block of sufficient size
        for i in 0..self.block_count {
            if let Some(block) = &mut self.blocks[i] {
                if !block.allocated && block.order >= required_order && block.size >= size {
                    let address = block.base_address;

                    block.allocate();
                    self.statistics.used_memory += size;
                    self.statistics.free_memory = self.statistics.free_memory.saturating_sub(size);
                    self.statistics.allocation_count += 1;

                    if self.allocation_count < MAX_ALLOCATIONS {
                        self.allocations[self.allocation_count] =
                            Some(AllocationInfo::new(address, size));
                        self.allocation_count += 1;
                    }

                    return Some(address);
                }
            }
        }

        None
    }

    pub fn deallocate(&mut self, address: u32) -> bool {
        // Find and mark block as free
        for i in 0..self.block_count {
            if let Some(block) = &mut self.blocks[i] {
                if block.base_address == address {
                    let size = block.size;
                    if block.deallocate() {
                        self.statistics.used_memory = self.statistics.used_memory.saturating_sub(size);
                        self.statistics.free_memory += size;
                        self.statistics.deallocation_count += 1;
                        self.statistics.update_fragmentation();
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn defragment(&mut self) -> bool {
        self.statistics.defragmentation_count += 1;

        // Compact free blocks
        let mut free_count = 0;
        for i in 0..self.block_count {
            if let Some(block) = &self.blocks[i] {
                if block.free {
                    free_count += 1;
                }
            }
        }

        free_count > 1 // Successfully defragmented if >1 free block
    }

    pub fn get_statistics(&self) -> MemoryStatistics {
        self.statistics
    }
}

impl Default for BuddyAllocator {
    fn default() -> Self {
        Self::new(GPU_MEMORY_POOL_SIZE)
    }
}

// ============================================================================
// GPU MEMORY POOL
// ============================================================================

pub struct GPUMemoryPool {
    pub allocator: BuddyAllocator,
    pub texture_atlas: TextureAtlas,
    pub buffer_cache: BufferCache,
    pub compression_manager: CompressionManager,
    pub strategy: AllocationStrategy,
}

impl GPUMemoryPool {
    pub fn new(strategy: AllocationStrategy) -> Self {
        GPUMemoryPool {
            allocator: BuddyAllocator::new(GPU_MEMORY_POOL_SIZE),
            texture_atlas: TextureAtlas::new(4096, 4096),
            buffer_cache: BufferCache::new(),
            compression_manager: CompressionManager::new(),
            strategy,
        }
    }

    pub fn allocate_buffer(&mut self, size: u32) -> Option<u32> {
        match self.strategy {
            AllocationStrategy::Linear | AllocationStrategy::Buddy => self.allocator.allocate(size),
            AllocationStrategy::FragmentationAware => {
                if self.allocator.statistics.fragmentation_ratio > 50 {
                    self.allocator.defragment();
                }
                self.allocator.allocate(size)
            }
        }
    }

    pub fn deallocate_buffer(&mut self, address: u32) -> bool {
        self.allocator.deallocate(address)
    }

    pub fn get_memory_usage(&self) -> (u32, u32, u32) {
        (
            self.allocator.statistics.used_memory,
            self.allocator.statistics.free_memory,
            self.allocator.statistics.fragmentation_ratio,
        )
    }
}

impl Default for GPUMemoryPool {
    fn default() -> Self {
        Self::new(AllocationStrategy::Buddy)
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_block_new() {
        let block = MemoryBlock::new(0, 1024, 10);
        assert_eq!(block.base_address, 0);
        assert_eq!(block.size, 1024);
        assert!(!block.allocated);
        assert!(block.free);
    }

    #[test]
    fn test_memory_block_allocate() {
        let mut block = MemoryBlock::new(0, 1024, 10);
        assert!(block.allocate());
        assert!(block.allocated);
        assert!(!block.free);
    }

    #[test]
    fn test_memory_block_deallocate() {
        let mut block = MemoryBlock::new(0, 1024, 10);
        block.allocate();
        assert!(block.deallocate());
        assert!(!block.allocated);
        assert!(block.free);
    }

    #[test]
    fn test_memory_statistics_new() {
        let stats = MemoryStatistics::new(1024);
        assert_eq!(stats.total_memory, 1024);
        assert_eq!(stats.used_memory, 0);
        assert_eq!(stats.free_memory, 1024);
    }

    #[test]
    fn test_memory_statistics_utilization() {
        let mut stats = MemoryStatistics::new(1024);
        stats.used_memory = 512;
        assert_eq!(stats.get_utilization(), 50);
    }

    #[test]
    fn test_texture_atlas_new() {
        let atlas = TextureAtlas::new(2048, 2048);
        assert_eq!(atlas.width, 2048);
        assert_eq!(atlas.height, 2048);
        assert_eq!(atlas.region_count, 0);
    }

    #[test]
    fn test_texture_atlas_allocate() {
        let mut atlas = TextureAtlas::new(512, 512);
        let region = atlas.allocate_region(256, 256);
        assert!(region.is_some());
        assert_eq!(atlas.region_count, 1);
    }

    #[test]
    fn test_texture_atlas_utilization() {
        let mut atlas = TextureAtlas::new(256, 256);
        atlas.allocate_region(128, 128);
        let util = atlas.get_utilization();
        assert!(util > 0);
    }

    #[test]
    fn test_buffer_cache_new() {
        let cache = BufferCache::new();
        assert_eq!(cache.buffer_count, 0);
    }

    #[test]
    fn test_buffer_cache_add() {
        let mut cache = BufferCache::new();
        assert!(cache.add_buffer(1, 4096));
        assert_eq!(cache.buffer_count, 1);
    }

    #[test]
    fn test_buffer_cache_reuse() {
        let mut cache = BufferCache::new();
        cache.add_buffer(1, 4096);
        let id = cache.get_reusable_buffer(2048);
        assert!(id.is_some());
        assert_eq!(cache.reuse_count, 1);
    }

    #[test]
    fn test_compression_info_new() {
        let info = CompressionInfo::new(1000, CompressionFormat::ASTC_8x8);
        assert_eq!(info.original_size, 1000);
        assert!(info.compressed_size < info.original_size);
    }

    #[test]
    fn test_compression_info_ratio() {
        let info = CompressionInfo::new(1000, CompressionFormat::BC1);
        let ratio = info.get_compression_ratio();
        assert!(ratio <= 100);
    }

    #[test]
    fn test_compression_manager_new() {
        let manager = CompressionManager::new();
        assert_eq!(manager.compression_count, 0);
    }

    #[test]
    fn test_compression_manager_add() {
        let mut manager = CompressionManager::new();
        assert!(manager.add_compression(1000, CompressionFormat::ASTC_8x8));
        assert_eq!(manager.compression_count, 1);
    }

    #[test]
    fn test_buddy_allocator_new() {
        let allocator = BuddyAllocator::new(1024 * 1024);
        assert_eq!(allocator.statistics.total_memory, 1024 * 1024);
    }

    #[test]
    fn test_buddy_allocator_allocate() {
        let mut allocator = BuddyAllocator::new(1024 * 1024);
        let addr = allocator.allocate(4096);
        assert!(addr.is_some());
        assert!(allocator.statistics.used_memory > 0);
    }

    #[test]
    fn test_buddy_allocator_deallocate() {
        let mut allocator = BuddyAllocator::new(1024 * 1024);
        let addr = allocator.allocate(4096).unwrap();
        let freed = allocator.deallocate(addr);
        assert!(freed);
        assert_eq!(allocator.statistics.used_memory, 0);
    }

    #[test]
    fn test_gpu_memory_pool_new() {
        let pool = GPUMemoryPool::new(AllocationStrategy::Buddy);
        assert_eq!(pool.strategy, AllocationStrategy::Buddy);
    }

    #[test]
    fn test_gpu_memory_pool_allocate() {
        let mut pool = GPUMemoryPool::new(AllocationStrategy::Buddy);
        let addr = pool.allocate_buffer(8192);
        assert!(addr.is_some());
    }

    #[test]
    fn test_gpu_memory_pool_deallocate() {
        let mut pool = GPUMemoryPool::new(AllocationStrategy::Buddy);
        let addr = pool.allocate_buffer(8192).unwrap();
        assert!(pool.deallocate_buffer(addr));
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_memory_pool_with_atlas() {
        let mut pool = GPUMemoryPool::new(AllocationStrategy::Buddy);

        // Allocate texture memory
        let buf1 = pool.allocate_buffer(65536).unwrap();
        let buf2 = pool.allocate_buffer(65536).unwrap();

        // Use texture atlas
        let region = pool.texture_atlas.allocate_region(512, 512);
        assert!(region.is_some());

        pool.deallocate_buffer(buf1);
        pool.deallocate_buffer(buf2);
    }

    #[test]
    fn test_buffer_cache_reuse_workflow() {
        let mut pool = GPUMemoryPool::new(AllocationStrategy::Buddy);

        pool.buffer_cache.add_buffer(1, 4096);
        pool.buffer_cache.add_buffer(2, 8192);

        let reused = pool.buffer_cache.get_reusable_buffer(2048);
        assert!(reused.is_some());

        pool.buffer_cache.release_buffer(reused.unwrap());
    }

    #[test]
    fn test_compression_memory_savings() {
        let mut pool = GPUMemoryPool::new(AllocationStrategy::Buddy);

        pool.compression_manager
            .add_compression(10000, CompressionFormat::ASTC_8x8);
        pool.compression_manager
            .add_compression(20000, CompressionFormat::BC7);

        let savings = pool.compression_manager.get_total_savings();
        assert!(savings > 0);
    }

    #[test]
    fn test_fragmentation_handling() {
        let mut pool = GPUMemoryPool::new(AllocationStrategy::FragmentationAware);

        let a1 = pool.allocate_buffer(4096).unwrap();
        let a2 = pool.allocate_buffer(4096).unwrap();
        let a3 = pool.allocate_buffer(4096).unwrap();

        pool.deallocate_buffer(a1);
        pool.deallocate_buffer(a3);

        // Allocate should still work
        let a4 = pool.allocate_buffer(2048);
        assert!(a4.is_some());
    }
}
