# Phase 9A Task 2: File System Writes - Implementation Plan

## Objective
Extend RayOS filesystem with write operations, enabling persistent data storage and file modifications.

## Current Status

**Existing (from Phase 7):**
- ✅ FAT32 filesystem driver (read-only)
- ✅ Directory traversal
- ✅ File reading capabilities
- ✅ Sector-based I/O

**Missing (to implement):**
- ❌ File write operations
- ❌ File creation
- ❌ File deletion
- ❌ Directory creation
- ❌ FAT table updates
- ❌ File truncation
- ❌ Directory removal

## Deliverables

### 1. Core File Operations (400-500 lines)
**Scope:**
- `create_file(path)` - Create new file
- `write_file(fd, data)` - Write to file
- `delete_file(path)` - Delete file
- `append_file(path, data)` - Append to file
- `truncate_file(path, size)` - Resize file

**Architecture:**
```rust
pub fn create_file(path: &str) -> Result<u32, u32> {
    // Parse path into directory and filename
    // Find/create parent directory entry
    // Allocate first cluster if needed
    // Create directory entry
    // Update FAT table
    // Return file descriptor
}

pub fn write_file(path: &str, data: &[u8], offset: u32) -> Result<u32, u32> {
    // Find file in directory
    // Allocate clusters as needed
    // Write data to clusters
    // Update FAT chain
    // Update directory entry (size, timestamp)
    // Return bytes written
}
```

### 2. Directory Operations (300-400 lines)
**Scope:**
- `mkdir(path)` - Create directory
- `rmdir(path)` - Remove empty directory
- `list_dir(path)` - Enhanced listing with sizes/times
- `rename(old_path, new_path)` - Rename files/dirs

### 3. FAT32 Extensions (250-350 lines)
**Scope:**
- Cluster allocation algorithm
- FAT table updates
- Free cluster tracking
- FAT caching for performance
- Consistency validation

### 4. Shell Integration (150-200 lines)
**New Commands:**
- `touch` - Create empty file
- `mkdir` - Create directory
- `rm` - Delete file
- `rmdir` - Remove directory
- `cp` - Copy file
- `mv`/`rename` - Move/rename
- `cat` - Display file contents
- `echo FILE` - Append text to file
- `stat` - File information

**Modified Commands:**
- `ls` - Show real filesystem instead of hardcoded

## Implementation Strategy

### Phase 1: FAT32 Write Support (Days 1-2)
1. Implement free cluster allocation
2. Implement FAT table updates
3. Implement cluster chain management
4. Add file creation in directory

### Phase 2: Core File Operations (Days 2-3)
1. Implement write_file()
2. Implement create_file()
3. Implement delete_file()
4. Add append/truncate support

### Phase 3: Directory Operations (Days 3-4)
1. Implement mkdir()
2. Implement rmdir()
3. Implement list_dir() with real data
4. Implement rename()

### Phase 4: Shell Integration & Testing (Day 4-5)
1. Add touch, mkdir, rm, cp, cat commands
2. Update ls to use real filesystem
3. Test all operations
4. Verify persistence

## Technical Details

### FAT32 Write Mechanics

**Cluster Allocation:**
```rust
fn allocate_cluster(&mut self) -> u32 {
    // Scan FAT for free clusters (0x00000000)
    // Mark as in-use (0x0FFFFFFF for EOF)
    // Return cluster number
}

fn free_cluster(&mut self, cluster: u32) {
    // Mark as free (0x00000000)
    // Update FAT table
}
```

**File Creation:**
1. Parse path into directory path and filename
2. Navigate to parent directory
3. Create directory entry with:
   - Filename (8.3 format or LFN)
   - Starting cluster (allocate first cluster)
   - File size (0 initially)
   - Attributes (archive, read-only)
   - Creation/modification timestamps
4. Write directory entry to disk
5. Return file descriptor

**File Writing:**
1. Find file in directory (get starting cluster)
2. Follow FAT chain to end
3. If more clusters needed, allocate new ones
4. Write data to clusters
5. Update FAT chain
6. Update file size in directory entry
7. Flush directory entry to disk

### Persistence Considerations

**Immediate Write:**
- All FAT updates must be synchronous
- Directory entries written immediately
- No caching layer (for simplicity)

**Crash Safety:**
- Assume clean shutdown
- No journaling (Phase 9B feature)
- Data may be lost on sudden power-off

## Code Structure

### New Functions in main.rs

```rust
// FAT32 write support
fn allocate_cluster(fs: &mut FAT32FileSystem) -> Option<u32>
fn free_cluster(fs: &mut FAT32FileSystem, cluster: u32)
fn extend_file_chain(fs: &mut FAT32FileSystem, cluster: u32) -> Option<u32>
fn write_fat_sector(fs: &FAT32FileSystem, sector: u32, data: &[u8])
fn write_directory_entry(fs: &FAT32FileSystem, path: &str, entry: &DirectoryEntry)

// File operations
fn create_file(fs: &mut FAT32FileSystem, path: &str) -> Result<u32, u32>
fn write_file(fs: &mut FAT32FileSystem, path: &str, data: &[u8]) -> Result<u32, u32>
fn delete_file(fs: &mut FAT32FileSystem, path: &str) -> Result<(), u32>
fn mkdir(fs: &mut FAT32FileSystem, path: &str) -> Result<(), u32>
fn rmdir(fs: &mut FAT32FileSystem, path: &str) -> Result<(), u32>

// Shell commands
fn cmd_touch(shell: &mut Shell, output: &mut ShellOutput, args: &[&str])
fn cmd_mkdir(shell: &mut Shell, output: &mut ShellOutput, args: &[&str])
fn cmd_rm(shell: &mut Shell, output: &mut ShellOutput, args: &[&str])
fn cmd_cp(shell: &mut Shell, output: &mut ShellOutput, args: &[&str])
fn cmd_cat(shell: &mut Shell, output: &mut ShellOutput, args: &[&str])
```

### Shell Extension

```rust
// In shell.rs execute_command()
"touch" => self.cmd_touch(&mut output, args),
"mkdir" => self.cmd_mkdir(&mut output, args),
"rm" => self.cmd_rm(&mut output, args),
"cp" => self.cmd_cp(&mut output, args),
"cat" => self.cmd_cat(&mut output, args),
"mv" => self.cmd_mv(&mut output, args),
"stat" => self.cmd_stat(&mut output, args),

// Modified
"ls" => self.cmd_ls_real(&mut output, args),
```

## Testing Strategy

### Unit Tests
1. Cluster allocation/deallocation
2. FAT chain management
3. Directory entry creation
4. File size tracking

### Integration Tests
1. Create file, write data, verify
2. Create multiple files
3. Delete files and verify space
4. Create nested directories
5. Copy files
6. Rename/move files
7. Append to files
8. Truncate files

### Shell Tests
1. `touch file.txt` - Create file
2. `echo "hello" >> file.txt` - Write data
3. `cat file.txt` - Read back
4. `mkdir dir` - Create directory
5. `cp file.txt dir/copy.txt` - Copy
6. `rm file.txt` - Delete
7. `rmdir dir` - Remove directory

## Success Criteria

- ✅ All 5 file operations working
- ✅ All 4 directory operations working
- ✅ 8+ new shell commands
- ✅ Real filesystem enumeration in ls
- ✅ Persistence verified (data survives reboot)
- ✅ No crashes on edge cases
- ✅ < 1200 lines of new code
- ✅ Zero compilation errors

## Timeline

- **Days 1-2:** FAT32 write infrastructure (cluster allocation, FAT updates)
- **Days 2-3:** Core file operations (create, write, delete)
- **Days 3-4:** Directory operations and shell integration
- **Day 4-5:** Testing and refinement

**Total: 4-5 days**

## Dependencies

- FAT32 driver from Phase 7 ✅
- Shell module from Phase 9A Task 1 ✅
- Block device I/O ✅
- Process manager (for file descriptors) ✅

## Risk Factors

1. **Cluster allocation complexity** - Mitigate: simple linear scan
2. **FAT table corruption** - Mitigate: single-write consistency
3. **Disk full scenarios** - Mitigate: error handling, graceful failure
4. **Large file handling** - Mitigate: initial 64KB file limit

## Future Improvements

- Phase 9B: Journaling for crash safety
- Phase 9B: File descriptor table for multiple opens
- Phase 9B: Long filename (LFN) support
- Phase 9B: File permissions/ownership
- Phase 9B: Symbolic links

---

**Status: Ready to Implement**
**Start Date: January 7, 2026**
**Target Completion: January 11-12, 2026**
