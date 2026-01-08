# Phase 9A Task 2: File System Writes - Implementation Guide

## Overview

This guide provides a comprehensive roadmap for implementing file system write operations for RayOS. The architecture uses a layered approach with placeholder functions that have detailed TODO lists.

## Current Status

- ✅ **Framework**: Complete
  - FAT32 cluster management functions
  - Directory entry structures
  - Shell command integration

- ⏳ **Core Implementation**: 45% Complete
  - Helper functions with detailed TODO lists
  - Shell commands wired to filesystem API
  - Build system verified (6.14-6.66s, 0 errors)

## Architecture Overview

```
┌─────────────────────────────────────────┐
│       Shell Layer (shell.rs)            │
│  touch, mkdir, rm, cat, cp commands     │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│  File System API (main.rs)              │
│  fs_create_file(), fs_write_file(), ... │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│  Helper Functions (main.rs)             │
│  find_file_in_root(), create_file_entry()│
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│  Block Device (FAT32 read/write)        │
│  FAT32FileSystem, BlockDevice trait     │
└─────────────────────────────────────────┘
```

## Key Code Locations

### Core File System Code
- **File**: `crates/kernel-bare/src/main.rs`
- **FAT32 Structs**: Lines 1435-1500
- **Cluster Management**: Lines 1740-1850
- **File Operations**: Lines 1850-1980
- **Helper Functions**: Lines 1850-1920

### Shell Integration
- **File**: `crates/kernel-bare/src/shell.rs`
- **Command Dispatcher**: Lines 197-207
- **Command Implementations**: Lines 385-520

## Implementation Sequence

### Phase 1: File Lookup (Days 1-2)

**Goal**: Implement `find_file_in_root()` to search for files

**Current Status**: Placeholder with detailed TODO

**Location**: `crates/kernel-bare/src/main.rs` lines 1850-1870

**Implementation Steps**:

1. **Calculate Root Directory Sector**
   ```rust
   let root_start_sector = fs.reserved_sectors + (fs.fat_size * fs.num_fats);
   ```

2. **Read Directory Sector**
   - Need to use existing block device infrastructure
   - Sector size: 512 bytes
   - Directory entries: 32 bytes each (16 per sector)

3. **Parse Directory Entry**
   ```
   Offset  Size    Field
   0       11      Filename (8.3 format, space-padded)
   11      1       Attributes (0x10 = directory, 0x20 = archive)
   20-21   2       Starting cluster (low word, little-endian)
   26-27   2       Starting cluster (high word, little-endian)
   28-31   4       File size (little-endian)
   ```

4. **Entry Validation**
   - 0x00 = End of directory
   - 0xE5 = Deleted entry
   - Skip if attribute byte 11 has 0x10 (directory)

5. **Filename Matching**
   - FAT format: 11 bytes, space-padded
   - Example: "HELLO   TXT" for "hello.txt"
   - Comparison: Direct byte-to-byte match

**Testing Strategy**:
- Create test files on the disk image
- Add debug output to trace directory scanning
- Verify cluster numbers match expected values

### Phase 2: File Creation (Days 2-3)

**Goal**: Implement `create_file_entry()` to create new files

**Current Status**: Placeholder with TODO

**Location**: `crates/kernel-bare/src/main.rs` lines 1872-1885

**Implementation Steps**:

1. **Allocate Cluster from FAT**
   ```rust
   fn allocate_cluster(fs: &mut FAT32FileSystem) -> u32 {
       // Already implemented at lines 1740-1760
       // Scans FAT for first 0x00000000 entry
   }
   ```

2. **Find Free Directory Entry**
   - Scan root directory for first unused slot
   - First byte 0xE5 or 0x00 indicates free

3. **Create Directory Entry Structure**
   ```rust
   DirectoryEntry {
       name: [u8; 8],          // Filename (space-padded)
       ext: [u8; 3],           // Extension (space-padded)
       attributes: 0x20,       // Archive flag
       starting_cluster_low: u16,
       starting_cluster_high: u16,
       file_size: 0,           // New files start empty
       timestamp: // Creation date/time
   }
   ```

4. **Write Directory Entry**
   - Serialize entry to 32 bytes (see `directory_entry_to_bytes`)
   - Write to root directory sector
   - Flush FAT changes

5. **Update FAT**
   ```
   FAT[new_cluster] = 0x0FFFFFFF  // End-of-chain marker
   Flush FAT sector to disk
   ```

**Testing Strategy**:
- Verify `touch test.txt` creates entry in directory
- Check cluster allocation matches FAT
- Verify file can be found with `find_file_in_root()`

### Phase 3: File Writing (Days 3-4)

**Goal**: Implement `fs_write_file()` to write data to files

**Current Status**: Placeholder with TODO

**Location**: `crates/kernel-bare/src/main.rs` lines 1925-1945

**Implementation Steps**:

1. **Find or Create File**
   - Use `find_file_in_root()` to locate
   - Or create new entry if doesn't exist

2. **Calculate Clusters Needed**
   ```rust
   let clusters_needed = (data.len() + cluster_size - 1) / cluster_size;
   ```

3. **Allocate Additional Clusters**
   ```rust
   // Allocate cluster_count - current_clusters new clusters
   for i in 0..new_clusters {
       let new_cluster = allocate_cluster(fs);
       link_clusters(fs, current_cluster, new_cluster);
       current_cluster = new_cluster;
   }
   ```

4. **Write Data to Clusters**
   ```rust
   for chunk in data.chunks(cluster_size) {
       write_to_cluster(fs, current_cluster, chunk);
       move_to_next_cluster();
   }
   ```

5. **Update File Metadata**
   - Set new file size in directory entry
   - Update modification timestamp
   - Mark as modified/dirty

6. **Flush Changes**
   - Write FAT sectors
   - Write directory sectors
   - Write data sectors

**Testing Strategy**:
- Write data via `fs_write_file()`
- Verify with `fs_file_size()` that size updated
- Read back to verify data integrity

### Phase 4: Directory Operations (Days 4-5)

**Goal**: Implement directory creation/listing

**Current Status**: Placeholders with TODO

**Locations**:
- `fs_mkdir()`: Lines 1955-1965
- `fs_rmdir()`: Lines 1967-1978
- `fs_list_dir()`: Lines 1988-2008

**Implementation Notes**:

**mkdir**:
1. Similar to file creation but with directory flag
2. Create "." and ".." entries in new directory
3. Link to parent

**rmdir**:
1. Verify directory is empty (only . and ..)
2. Free allocated cluster
3. Remove directory entry from parent
4. Update parent's FAT chain

**list_dir**:
1. Read directory sectors
2. Format entries as text:
   ```
   FILENAME.EXT  <size>  <date> <time>  <FILE|DIR>
   ```
3. Return formatted buffer

### Phase 5: File Reading & Deletion (Days 5)

**Goal**: Implement remaining operations

**Locations**:
- `fs_delete_file()`: Lines 1947-1955
- `fs_copy_file()`: Lines 1980-1990
- `fs_file_size()`: Lines 2000-2008

**Implementation Notes**:

**delete_file**:
1. Find file in directory
2. Walk FAT chain freeing clusters
3. Mark directory entry as deleted (0xE5)
4. Flush changes

**copy_file**:
1. Read source file in chunks
2. Create destination file
3. Write chunks to destination
4. Close both files

## Integration Points

### Shell Command Wiring

Each command needs to call the filesystem function:

```rust
// In shell.rs cmd_touch()
match fs_create_file(filename_str) {
    Ok(_) => writeln!(output, "File created"),
    Err(code) => writeln!(output, "Error: {}", code),
}
```

### Syscall Integration

When syscall layer is implemented:
```rust
sys_write(fd, data) -> fs_write_file(path, data)
sys_mkdir(path) -> fs_mkdir(path)
sys_unlink(path) -> fs_delete_file(path)
```

## Testing Checklist

### Unit Tests
- [ ] File lookup finds existing files
- [ ] File creation allocates clusters
- [ ] File writing stores data correctly
- [ ] Directory creation works
- [ ] File deletion frees clusters

### Integration Tests
- [ ] `touch` command creates files
- [ ] `cat` command reads files
- [ ] `cp` command copies files
- [ ] `rm` command deletes files
- [ ] `mkdir` command creates directories
- [ ] `ls` command lists contents

### Edge Cases
- [ ] Create file with same name (overwrite vs error)
- [ ] Write to read-only media
- [ ] Fill disk (out of space)
- [ ] Long filenames (FAT limitation)
- [ ] Special characters in names

## Build & Verification

**Build Command**:
```bash
cd crates/kernel-bare
cargo +nightly build --release --target x86_64-unknown-none
```

**Expected Output**:
- Compiles in ~6-7 seconds
- 0 errors
- ~20-25 warnings (unused parameters until implementation)

**Boot & Test**:
```bash
# Build ISO
./scripts/build-iso.sh

# Boot in QEMU and test
# Try commands: touch test.txt; cat test.txt; ls
```

## Common Pitfalls

1. **Cluster vs Sector Confusion**
   - Clusters start at cluster 2 (reserved 0, 1)
   - Sector = reserved + FAT*num_fats + (cluster-2)*sectors_per_cluster

2. **FAT Entry Format**
   - FAT32: 32-bit entries (4 bytes each)
   - End-of-chain: 0x0FFFFFFF
   - Free: 0x00000000
   - Invalid: 0x0FFFFFF6-0x0FFFFFF7

3. **Little-Endian Byte Order**
   - FAT32 uses little-endian (Intel format)
   - cluster_low_word at byte 20, cluster_high_word at byte 26
   - Complete cluster = (high << 16) | low

4. **Filename Padding**
   - FAT 8.3 format uses space padding (0x20)
   - "test.txt" = "TEST    TXT" (all uppercase)
   - Not null-terminated

5. **Directory Entry Format**
   - Always 32 bytes
   - Attributes at offset 11 (bitmap)
   - Timestamps are complex (separate date/time fields)

## References

### FAT32 Specifications
- [FAT32 Format Specification](https://en.wikipedia.org/wiki/Design_of_the_FAT_file_system)
- [Directory Entry Structure](https://en.wikipedia.org/wiki/Design_of_the_FAT_file_system#Directory_entry)

### RayOS Code References
- `FAT32FileSystem` struct: lines 1435-1470
- `DirectoryEntry` struct: lines 1560-1620
- `BlockDevice` trait: lines 1301-1315
- Existing read functions: Phase 7 code

## Progress Tracking

Update this file as implementation progresses:

```markdown
### Phase 1: File Lookup
- [x] find_file_in_root() placeholder
- [ ] Implement root sector calculation
- [ ] Implement directory entry parsing
- [ ] Implement filename matching
- [ ] Test with known files

### Phase 2: File Creation
- [x] create_file_entry() placeholder
- [ ] Implement cluster allocation
- [ ] Implement free entry search
- [ ] Implement entry serialization
- [ ] Test file creation

### Phase 3: File Writing
- [x] fs_write_file() placeholder
- [ ] Implement cluster linking
- [ ] Implement data writing
- [ ] Implement FAT updates
- [ ] Test file writing

### Phase 4: Directory Operations
- [ ] fs_mkdir() implementation
- [ ] fs_rmdir() implementation
- [ ] fs_list_dir() implementation
- [ ] Test all three

### Phase 5: Cleanup
- [ ] fs_delete_file() implementation
- [ ] fs_copy_file() implementation
- [ ] fs_file_size() implementation
- [ ] fs_read_file() implementation
- [ ] Comprehensive testing
```

## Summary

This implementation guide provides:
- ✅ Clear architecture diagram
- ✅ Detailed step-by-step procedures
- ✅ Code locations and line numbers
- ✅ Testing strategies
- ✅ Common pitfalls and solutions
- ✅ FAT32 specification details
- ✅ Build and verification commands

The framework is complete and ready for implementation. Each phase builds on the previous one, with thorough testing at each step.
