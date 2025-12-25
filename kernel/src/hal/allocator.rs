/// Zero-Copy Allocator - "The Shared Memory"
///
/// Manages Unified Memory pointers so CPU and GPU read the same RAM
/// without copying. Critical for the continuous simulation model.

use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Memory region descriptor
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Virtual address accessible from both CPU and GPU
    pub address: u64,
    /// Size in bytes
    pub size: usize,
    /// Is this region currently mapped?
    pub mapped: bool,
    /// Reference count for shared memory
    pub ref_count: usize,
}

/// Memory pool for fast allocation of common sizes
struct MemoryPool {
    /// Pool of 4KB blocks
    small_blocks: Vec<u64>,
    /// Pool of 64KB blocks
    medium_blocks: Vec<u64>,
    /// Pool of 1MB blocks
    large_blocks: Vec<u64>,
}

impl MemoryPool {
    fn new() -> Self {
        Self {
            small_blocks: Vec::with_capacity(256),
            medium_blocks: Vec::with_capacity(64),
            large_blocks: Vec::with_capacity(16),
        }
    }

    /// Allocate from pool or return None to allocate fresh
    fn allocate(&mut self, size: usize) -> Option<u64> {
        if size <= 4096 {
            self.small_blocks.pop()
        } else if size <= 65536 {
            self.medium_blocks.pop()
        } else if size <= 1048576 {
            self.large_blocks.pop()
        } else {
            None
        }
    }

    /// Return block to pool for reuse
    fn free(&mut self, address: u64, size: usize) {
        if size <= 4096 && self.small_blocks.len() < 256 {
            self.small_blocks.push(address);
        } else if size <= 65536 && self.medium_blocks.len() < 64 {
            self.medium_blocks.push(address);
        } else if size <= 1048576 && self.large_blocks.len() < 16 {
            self.large_blocks.push(address);
        }
        // Otherwise let it be truly freed
    }
}

/// Zero-Copy Allocator for unified memory access
pub struct ZeroCopyAllocator {
    /// Allocated regions
    regions: Arc<RwLock<HashMap<u64, MemoryRegion>>>,
    /// Next allocation address
    next_address: Arc<RwLock<u64>>,
    /// Base address for unified memory
    base_address: u64,
    /// Memory pool for fast reallocation
    pool: Arc<RwLock<MemoryPool>>,
}

impl ZeroCopyAllocator {
    /// Create a new zero-copy allocator
    pub fn new(base_address: u64) -> Self {
        log::info!("Initializing Zero-Copy Allocator at base 0x{:x}", base_address);

        Self {
            regions: Arc::new(RwLock::new(HashMap::new())),
            next_address: Arc::new(RwLock::new(base_address)),
            base_address,
            pool: Arc::new(RwLock::new(MemoryPool::new())),
        }
    }

    /// Allocate a unified memory region
    pub fn allocate(&self, size: usize) -> Result<u64> {
        // Align to 256 bytes (GPU cache line)
        let aligned_size = (size + 255) & !255;

        // Try to get from pool first
        let address = if let Some(pooled_addr) = self.pool.write().allocate(aligned_size) {
            log::debug!("Reusing pooled memory at 0x{:x}", pooled_addr);
            pooled_addr
        } else {
            // Allocate fresh memory
            let mut next_addr = self.next_address.write();
            let address = *next_addr;
            *next_addr += aligned_size as u64;
            address
        };

        let region = MemoryRegion {
            address,
            size: aligned_size,
            mapped: true,
            ref_count: 1,
        };

        self.regions.write().insert(address, region);

        log::debug!("Allocated {} bytes at 0x{:x}", aligned_size, address);

        Ok(address)
    }

    /// Free a unified memory region
    pub fn free(&self, address: u64) -> Result<()> {
        let mut regions = self.regions.write();

        if let Some(region) = regions.get_mut(&address) {
            region.ref_count -= 1;

            if region.ref_count == 0 {
                // Return to pool or free completely
                let size = region.size;
                regions.remove(&address);
                self.pool.write().free(address, size);

                log::debug!("Freed memory at 0x{:x} (returned to pool)", address);
            } else {
                log::debug!("Decremented ref count for 0x{:x} to {}", address, region.ref_count);
            }

            Ok(())
        } else {
            anyhow::bail!("Attempted to free invalid address: 0x{:x}", address)
        }
    }

    /// Increment reference count for shared memory
    pub fn add_ref(&self, address: u64) -> Result<()> {
        let mut regions = self.regions.write();

        if let Some(region) = regions.get_mut(&address) {
            region.ref_count += 1;
            log::debug!("Incremented ref count for 0x{:x} to {}", address, region.ref_count);
            Ok(())
        } else {
            anyhow::bail!("Attempted to add ref to invalid address: 0x{:x}", address)
        }
    }

    /// Get information about a memory region
    pub fn get_region(&self, address: u64) -> Option<MemoryRegion> {
        self.regions.read().get(&address).cloned()
    }

    /// Total allocated memory in bytes
    pub fn total_allocated(&self) -> usize {
        self.regions
            .read()
            .values()
            .map(|r| r.size)
            .sum()
    }

    /// Number of active allocations
    pub fn allocation_count(&self) -> usize {
        self.regions.read().len()
    }
}

impl Default for ZeroCopyAllocator {
    fn default() -> Self {
        // Default base address for unified memory
        Self::new(0x1000_0000_0000)
    }
}
