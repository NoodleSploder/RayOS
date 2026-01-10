
const MAX_COMPRESSIBLE_PAGES: usize = 1024;
const MAX_COMPRESSED_PAGES: usize = 256;

/// Compression level enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompressionLevel {
    None = 0,
    Fast = 1,
    Balanced = 2,
    Best = 3,
}

/// Compression policy
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompressionPolicy {
    Threshold = 0,
    TimeBased = 1,
    DemandBased = 2,
}

/// Compressed page metadata
#[derive(Clone, Copy, Debug)]
pub struct CompressedPage {
    pub page_id: u32,
    pub original_size: u32,
    pub compressed_size: u32,
    pub compression_time_us: u32,
    pub decompression_time_us: u32,
    pub access_count: u32,
}

impl CompressedPage {
    pub fn new(page_id: u32, original_size: u32, compressed_size: u32) -> Self {
        CompressedPage {
            page_id,
            original_size,
            compressed_size,
            compression_time_us: 0,
            decompression_time_us: 0,
            access_count: 0,
        }
    }

    pub fn compression_ratio(&self) -> u32 {
        if self.original_size == 0 {
            0
        } else {
            ((self.compressed_size * 100) / self.original_size) as u32
        }
    }
}

/// Compression statistics
#[derive(Clone, Copy, Debug)]
pub struct CompressionStats {
    pub total_pages_compressed: u32,
    pub total_pages_decompressed: u32,
    pub total_original_bytes: u64,
    pub total_compressed_bytes: u64,
    pub average_compression_ratio: u32,
    pub total_compression_time_us: u64,
    pub total_decompression_time_us: u64,
}

impl CompressionStats {
    pub fn new() -> Self {
        CompressionStats {
            total_pages_compressed: 0,
            total_pages_decompressed: 0,
            total_original_bytes: 0,
            total_compressed_bytes: 0,
            average_compression_ratio: 100,
            total_compression_time_us: 0,
            total_decompression_time_us: 0,
        }
    }

    pub fn memory_saved(&self) -> u64 {
        self.total_original_bytes.saturating_sub(self.total_compressed_bytes)
    }

    pub fn overall_ratio(&self) -> u32 {
        if self.total_original_bytes == 0 {
            100
        } else {
            ((self.total_compressed_bytes * 100) / self.total_original_bytes) as u32
        }
    }
}

/// Page pool entry
#[derive(Clone, Copy, Debug)]
pub struct PagePoolEntry {
    pub page_id: u32,
    pub size: u32,
    pub compressed: bool,
    pub last_access_time: u64,
}

impl PagePoolEntry {
    pub fn new(page_id: u32, size: u32) -> Self {
        PagePoolEntry {
            page_id,
            size,
            compressed: false,
            last_access_time: 0,
        }
    }
}

/// Memory Compressor
pub struct MemoryCompressor {
    compression_level: CompressionLevel,
    policy: CompressionPolicy,
    compressed_pages: [Option<CompressedPage>; MAX_COMPRESSED_PAGES],
    page_pool: [Option<PagePoolEntry>; MAX_COMPRESSIBLE_PAGES],
    stats: CompressionStats,
    compressed_page_count: u32,
    compressible_page_count: u32,
}

impl MemoryCompressor {
    pub fn new(level: CompressionLevel) -> Self {
        MemoryCompressor {
            compression_level: level,
            policy: CompressionPolicy::Threshold,
            compressed_pages: [None; MAX_COMPRESSED_PAGES],
            page_pool: [None; MAX_COMPRESSIBLE_PAGES],
            stats: CompressionStats::new(),
            compressed_page_count: 0,
            compressible_page_count: 0,
        }
    }

    pub fn compress_page(&mut self, page_id: u32, original_size: u32, compressed_size: u32) -> bool {
        if self.compressed_page_count >= MAX_COMPRESSED_PAGES as u32 {
            return false;
        }

        let compression_time = match self.compression_level {
            CompressionLevel::None => 0,
            CompressionLevel::Fast => 10,
            CompressionLevel::Balanced => 50,
            CompressionLevel::Best => 200,
        };

        for i in 0..MAX_COMPRESSED_PAGES {
            if self.compressed_pages[i].is_none() {
                let mut compressed = CompressedPage::new(page_id, original_size, compressed_size);
                compressed.compression_time_us = compression_time;
                self.compressed_pages[i] = Some(compressed);
                self.compressed_page_count += 1;

                self.stats.total_pages_compressed += 1;
                self.stats.total_original_bytes += original_size as u64;
                self.stats.total_compressed_bytes += compressed_size as u64;
                self.stats.total_compression_time_us += compression_time as u64;

                return true;
            }
        }
        false
    }

    pub fn decompress_page(&mut self, page_id: u32) -> bool {
        for i in 0..MAX_COMPRESSED_PAGES {
            if let Some(page) = self.compressed_pages[i] {
                if page.page_id == page_id {
                    let decompression_time = (page.compression_time_us * 2) / 3;
                    self.stats.total_pages_decompressed += 1;
                    self.stats.total_decompression_time_us += decompression_time as u64;
                    self.compressed_pages[i] = None;
                    self.compressed_page_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn add_compressible_page(&mut self, page_id: u32, size: u32) -> bool {
        if self.compressible_page_count >= MAX_COMPRESSIBLE_PAGES as u32 {
            return false;
        }

        for i in 0..MAX_COMPRESSIBLE_PAGES {
            if self.page_pool[i].is_none() {
                let entry = PagePoolEntry::new(page_id, size);
                self.page_pool[i] = Some(entry);
                self.compressible_page_count += 1;
                return true;
            }
        }
        false
    }

    pub fn remove_page(&mut self, page_id: u32) -> bool {
        for i in 0..MAX_COMPRESSIBLE_PAGES {
            if let Some(entry) = self.page_pool[i] {
                if entry.page_id == page_id {
                    self.page_pool[i] = None;
                    self.compressible_page_count -= 1;
                    self.decompress_page(page_id);
                    return true;
                }
            }
        }
        false
    }

    pub fn set_compression_level(&mut self, level: CompressionLevel) {
        self.compression_level = level;
    }

    pub fn set_policy(&mut self, policy: CompressionPolicy) {
        self.policy = policy;
    }

    pub fn get_compressed_page_count(&self) -> u32 {
        self.compressed_page_count
    }

    pub fn get_compressible_page_count(&self) -> u32 {
        self.compressible_page_count
    }

    pub fn get_stats(&self) -> CompressionStats {
        self.stats
    }

    pub fn select_lru_page(&self) -> Option<PagePoolEntry> {
        let mut min_time = u64::MAX;
        let mut lru_entry = None;

        for i in 0..MAX_COMPRESSIBLE_PAGES {
            if let Some(entry) = self.page_pool[i] {
                if !entry.compressed && entry.last_access_time < min_time {
                    min_time = entry.last_access_time;
                    lru_entry = Some(entry);
                }
            }
        }
        lru_entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_compression() {
        let mut compressor = MemoryCompressor::new(CompressionLevel::Balanced);
        assert!(compressor.compress_page(1, 4096, 2048));
        assert_eq!(compressor.get_compressed_page_count(), 1);
    }

    #[test]
    fn test_compression_levels() {
        let c1 = MemoryCompressor::new(CompressionLevel::Fast);
        let c2 = MemoryCompressor::new(CompressionLevel::Best);
        assert_ne!(c1.compression_level, c2.compression_level);
    }

    #[test]
    fn test_decompression() {
        let mut compressor = MemoryCompressor::new(CompressionLevel::Balanced);
        compressor.compress_page(1, 4096, 2048);
        assert!(compressor.decompress_page(1));
        assert_eq!(compressor.get_compressed_page_count(), 0);
    }

    #[test]
    fn test_memory_savings() {
        let mut compressor = MemoryCompressor::new(CompressionLevel::Balanced);
        compressor.compress_page(1, 4096, 2048);
        let stats = compressor.get_stats();
        assert!(stats.memory_saved() > 0);
    }

    #[test]
    fn test_compression_policy() {
        let mut compressor = MemoryCompressor::new(CompressionLevel::Balanced);
        compressor.set_policy(CompressionPolicy::DemandBased);
        assert_eq!(compressor.policy, CompressionPolicy::DemandBased);
    }

    #[test]
    fn test_page_eviction() {
        let mut compressor = MemoryCompressor::new(CompressionLevel::Balanced);
        compressor.add_compressible_page(1, 4096);
        compressor.add_compressible_page(2, 4096);
        compressor.remove_page(1);
        assert_eq!(compressor.get_compressible_page_count(), 1);
    }

    #[test]
    fn test_compression_stats() {
        let mut compressor = MemoryCompressor::new(CompressionLevel::Balanced);
        compressor.compress_page(1, 4096, 2048);
        compressor.compress_page(2, 4096, 2048);
        let stats = compressor.get_stats();
        assert_eq!(stats.total_pages_compressed, 2);
    }

    #[test]
    fn test_lru_selection() {
        let mut compressor = MemoryCompressor::new(CompressionLevel::Balanced);
        compressor.add_compressible_page(1, 4096);
        compressor.add_compressible_page(2, 4096);
        let lru = compressor.select_lru_page();
        assert!(lru.is_some());
    }
}
