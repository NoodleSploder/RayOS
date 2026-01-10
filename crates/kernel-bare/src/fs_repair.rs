// ===== RayOS Filesystem Repair & Recovery Tools (Phase 9B Task 5) =====
// Filesystem consistency checking, repair operations, rescue shell

use core::sync::atomic::{AtomicU32, Ordering};

// ===== Constants =====

const MAX_FS_ERRORS: usize = 128;
const MAX_REPAIR_ACTIONS: usize = 64;
const MAX_INODES_TO_CHECK: usize = 1024;
const SUPERBLOCK_MAGIC: u64 = 0x5241594F53465321;  // "RAYOSFS!"

// ===== Filesystem Type =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FilesystemType {
    /// RayOS native filesystem
    RayFs,
    /// ext4
    Ext4,
    /// FAT32
    Fat32,
    /// NTFS (read-only)
    Ntfs,
    /// Btrfs
    Btrfs,
    /// XFS
    Xfs,
    /// F2FS (Flash-Friendly)
    F2fs,
    /// Unknown
    Unknown,
}

impl FilesystemType {
    pub fn from_magic(magic: u64) -> Self {
        match magic {
            0x5241594F53465321 => FilesystemType::RayFs,
            0xEF53 => FilesystemType::Ext4,
            0x41455832 => FilesystemType::Ext4,  // EXT2/3/4
            0x4D534653 => FilesystemType::Fat32, // MSDOS
            0x5346544E => FilesystemType::Ntfs,  // NTFS
            0x9123683E => FilesystemType::Btrfs,
            0x58465342 => FilesystemType::Xfs,   // XFS
            0xF2F52010 => FilesystemType::F2fs,
            _ => FilesystemType::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FilesystemType::RayFs => "rayfs",
            FilesystemType::Ext4 => "ext4",
            FilesystemType::Fat32 => "fat32",
            FilesystemType::Ntfs => "ntfs",
            FilesystemType::Btrfs => "btrfs",
            FilesystemType::Xfs => "xfs",
            FilesystemType::F2fs => "f2fs",
            FilesystemType::Unknown => "unknown",
        }
    }
}

// ===== Filesystem Error Types =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FsErrorType {
    /// Superblock corruption
    SuperblockCorrupt,
    /// Bad block found
    BadBlock,
    /// Inode corruption
    InodeCorrupt,
    /// Directory entry invalid
    InvalidDirEntry,
    /// Block bitmap mismatch
    BlockBitmapMismatch,
    /// Inode bitmap mismatch
    InodeBitmapMismatch,
    /// Orphaned inode
    OrphanedInode,
    /// Duplicate block allocation
    DuplicateBlock,
    /// Invalid block pointer
    InvalidBlockPointer,
    /// Invalid inode link count
    InvalidLinkCount,
    /// Journal corruption
    JournalCorrupt,
    /// Extent tree corruption
    ExtentTreeCorrupt,
    /// Directory structure loop
    DirectoryLoop,
    /// File size mismatch
    FileSizeMismatch,
    /// Invalid timestamp
    InvalidTimestamp,
    /// Missing parent directory
    MissingParent,
    /// Cross-linked files
    CrossLinked,
}

impl FsErrorType {
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            FsErrorType::SuperblockCorrupt => ErrorSeverity::Critical,
            FsErrorType::JournalCorrupt => ErrorSeverity::Critical,
            FsErrorType::BadBlock => ErrorSeverity::High,
            FsErrorType::InodeCorrupt => ErrorSeverity::High,
            FsErrorType::ExtentTreeCorrupt => ErrorSeverity::High,
            FsErrorType::DuplicateBlock => ErrorSeverity::High,
            FsErrorType::CrossLinked => ErrorSeverity::High,
            FsErrorType::InvalidBlockPointer => ErrorSeverity::Medium,
            FsErrorType::OrphanedInode => ErrorSeverity::Medium,
            FsErrorType::InvalidDirEntry => ErrorSeverity::Medium,
            FsErrorType::DirectoryLoop => ErrorSeverity::Medium,
            FsErrorType::BlockBitmapMismatch => ErrorSeverity::Low,
            FsErrorType::InodeBitmapMismatch => ErrorSeverity::Low,
            FsErrorType::InvalidLinkCount => ErrorSeverity::Low,
            FsErrorType::FileSizeMismatch => ErrorSeverity::Low,
            FsErrorType::InvalidTimestamp => ErrorSeverity::Low,
            FsErrorType::MissingParent => ErrorSeverity::Medium,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

// ===== Filesystem Error =====

#[derive(Copy, Clone)]
pub struct FsError {
    /// Error type
    pub error_type: FsErrorType,
    /// Block number (if applicable)
    pub block: u64,
    /// Inode number (if applicable)
    pub inode: u64,
    /// Additional context
    pub context: u64,
    /// Can be auto-repaired
    pub auto_repairable: bool,
}

impl FsError {
    pub fn new(error_type: FsErrorType, block: u64, inode: u64) -> Self {
        FsError {
            error_type,
            block,
            inode,
            context: 0,
            auto_repairable: Self::can_auto_repair(error_type),
        }
    }

    fn can_auto_repair(error_type: FsErrorType) -> bool {
        match error_type {
            FsErrorType::OrphanedInode => true,
            FsErrorType::BlockBitmapMismatch => true,
            FsErrorType::InodeBitmapMismatch => true,
            FsErrorType::InvalidLinkCount => true,
            FsErrorType::InvalidTimestamp => true,
            FsErrorType::FileSizeMismatch => true,
            _ => false,
        }
    }
}

// ===== Repair Action =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RepairAction {
    /// Fix superblock from backup
    RestoreSuperblock,
    /// Mark block as bad
    MarkBadBlock,
    /// Clear inode
    ClearInode,
    /// Remove directory entry
    RemoveDirEntry,
    /// Fix block bitmap
    FixBlockBitmap,
    /// Fix inode bitmap
    FixInodeBitmap,
    /// Move to lost+found
    MoveToLostFound,
    /// Deallocate duplicate blocks
    DeallocateDuplicate,
    /// Clear block pointer
    ClearBlockPointer,
    /// Fix link count
    FixLinkCount,
    /// Replay journal
    ReplayJournal,
    /// Rebuild extent tree
    RebuildExtentTree,
    /// Break directory loop
    BreakDirectoryLoop,
    /// Update file size
    UpdateFileSize,
    /// Fix timestamp
    FixTimestamp,
    /// Create missing parent
    CreateParent,
    /// Unlink cross-linked file
    UnlinkCrossLinked,
}

#[derive(Copy, Clone)]
pub struct RepairOperation {
    /// Action to take
    pub action: RepairAction,
    /// Target block
    pub block: u64,
    /// Target inode
    pub inode: u64,
    /// Action data
    pub data: u64,
    /// Has been executed
    pub executed: bool,
    /// Was successful
    pub success: bool,
}

impl RepairOperation {
    pub fn new(action: RepairAction, block: u64, inode: u64) -> Self {
        RepairOperation {
            action,
            block,
            inode,
            data: 0,
            executed: false,
            success: false,
        }
    }
}

// ===== Check Phase =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CheckPhase {
    /// Not started
    NotStarted,
    /// Checking superblock
    Superblock,
    /// Checking journal
    Journal,
    /// Checking block groups
    BlockGroups,
    /// Checking inodes
    Inodes,
    /// Checking directory structure
    Directories,
    /// Checking block references
    BlockRefs,
    /// Checking link counts
    LinkCounts,
    /// Final reconciliation
    Reconciliation,
    /// Complete
    Complete,
    /// Aborted
    Aborted,
}

// ===== Filesystem Checker =====

pub struct FsChecker {
    /// Filesystem type
    pub fs_type: FilesystemType,
    /// Device path/identifier
    pub device_id: u32,
    /// Current phase
    pub phase: CheckPhase,
    /// Errors found
    errors: [FsError; MAX_FS_ERRORS],
    error_count: usize,
    /// Repair operations planned
    repairs: [RepairOperation; MAX_REPAIR_ACTIONS],
    repair_count: usize,
    /// Total inodes checked
    pub inodes_checked: u64,
    /// Total blocks checked
    pub blocks_checked: u64,
    /// Total files found
    pub files_found: u64,
    /// Total directories found
    pub dirs_found: u64,
    /// Is read-only check
    pub read_only: bool,
    /// Auto-repair enabled
    pub auto_repair: bool,
}

impl FsChecker {
    pub fn new(fs_type: FilesystemType, device_id: u32) -> Self {
        FsChecker {
            fs_type,
            device_id,
            phase: CheckPhase::NotStarted,
            errors: [FsError::new(FsErrorType::SuperblockCorrupt, 0, 0); MAX_FS_ERRORS],
            error_count: 0,
            repairs: [RepairOperation::new(RepairAction::RestoreSuperblock, 0, 0); MAX_REPAIR_ACTIONS],
            repair_count: 0,
            inodes_checked: 0,
            blocks_checked: 0,
            files_found: 0,
            dirs_found: 0,
            read_only: true,
            auto_repair: false,
        }
    }

    pub fn add_error(&mut self, error: FsError) {
        if self.error_count < MAX_FS_ERRORS {
            self.errors[self.error_count] = error;
            self.error_count += 1;

            // Auto-generate repair if possible
            if self.auto_repair && error.auto_repairable {
                self.plan_repair_for_error(&error);
            }
        }
    }

    fn plan_repair_for_error(&mut self, error: &FsError) {
        let action = match error.error_type {
            FsErrorType::OrphanedInode => RepairAction::MoveToLostFound,
            FsErrorType::BlockBitmapMismatch => RepairAction::FixBlockBitmap,
            FsErrorType::InodeBitmapMismatch => RepairAction::FixInodeBitmap,
            FsErrorType::InvalidLinkCount => RepairAction::FixLinkCount,
            FsErrorType::InvalidTimestamp => RepairAction::FixTimestamp,
            FsErrorType::FileSizeMismatch => RepairAction::UpdateFileSize,
            _ => return,
        };

        self.add_repair(RepairOperation::new(action, error.block, error.inode));
    }

    pub fn add_repair(&mut self, repair: RepairOperation) {
        if self.repair_count < MAX_REPAIR_ACTIONS {
            self.repairs[self.repair_count] = repair;
            self.repair_count += 1;
        }
    }

    pub fn check_superblock(&mut self) -> bool {
        self.phase = CheckPhase::Superblock;
        // Would read and verify superblock
        // Check magic, version, block size, etc.
        true
    }

    pub fn check_journal(&mut self) -> bool {
        self.phase = CheckPhase::Journal;
        // Would check journal integrity
        // Replay if needed
        true
    }

    pub fn check_block_groups(&mut self) -> bool {
        self.phase = CheckPhase::BlockGroups;
        // Would check block group descriptors
        // Verify checksums
        true
    }

    pub fn check_inodes(&mut self) -> bool {
        self.phase = CheckPhase::Inodes;
        // Would scan all inodes
        // Check for corruption, valid pointers
        for _ in 0..MAX_INODES_TO_CHECK {
            self.inodes_checked += 1;
        }
        true
    }

    pub fn check_directories(&mut self) -> bool {
        self.phase = CheckPhase::Directories;
        // Would traverse directory tree
        // Check for loops, missing entries
        true
    }

    pub fn check_block_refs(&mut self) -> bool {
        self.phase = CheckPhase::BlockRefs;
        // Would verify all block references
        // Detect duplicates, invalid pointers
        true
    }

    pub fn check_link_counts(&mut self) -> bool {
        self.phase = CheckPhase::LinkCounts;
        // Would verify link counts match reality
        true
    }

    pub fn run_full_check(&mut self) -> bool {
        if !self.check_superblock() { self.phase = CheckPhase::Aborted; return false; }
        if !self.check_journal() { self.phase = CheckPhase::Aborted; return false; }
        if !self.check_block_groups() { self.phase = CheckPhase::Aborted; return false; }
        if !self.check_inodes() { self.phase = CheckPhase::Aborted; return false; }
        if !self.check_directories() { self.phase = CheckPhase::Aborted; return false; }
        if !self.check_block_refs() { self.phase = CheckPhase::Aborted; return false; }
        if !self.check_link_counts() { self.phase = CheckPhase::Aborted; return false; }

        self.phase = CheckPhase::Complete;
        true
    }

    pub fn execute_repairs(&mut self) -> usize {
        if self.read_only {
            return 0;
        }

        let mut success_count = 0;
        for i in 0..self.repair_count {
            self.repairs[i].executed = true;
            // Would execute repair based on action type
            self.repairs[i].success = true;
            success_count += 1;
        }
        success_count
    }

    pub fn error_count(&self) -> usize {
        self.error_count
    }

    pub fn repair_count(&self) -> usize {
        self.repair_count
    }

    pub fn errors(&self) -> &[FsError] {
        &self.errors[..self.error_count]
    }

    pub fn has_critical_errors(&self) -> bool {
        for i in 0..self.error_count {
            if self.errors[i].error_type.severity() == ErrorSeverity::Critical {
                return true;
            }
        }
        false
    }
}

// ===== Lost+Found Manager =====

pub struct LostFoundManager {
    /// Inode number of lost+found directory
    pub lost_found_inode: u64,
    /// Next entry number
    next_entry: AtomicU32,
    /// Entries recovered
    pub entries_recovered: u32,
}

impl LostFoundManager {
    pub fn new(lost_found_inode: u64) -> Self {
        LostFoundManager {
            lost_found_inode,
            next_entry: AtomicU32::new(1),
            entries_recovered: 0,
        }
    }

    pub fn recover_inode(&mut self, _inode: u64) -> u32 {
        let entry_num = self.next_entry.fetch_add(1, Ordering::SeqCst);
        // Would create entry #entry_num pointing to inode
        self.entries_recovered += 1;
        entry_num
    }
}

// ===== Rescue Shell =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RescueCommand {
    /// List directory
    Ls,
    /// Change directory
    Cd,
    /// Print working directory
    Pwd,
    /// Cat file contents
    Cat,
    /// Hexdump file/block
    Hexdump,
    /// Mount filesystem
    Mount,
    /// Unmount filesystem
    Umount,
    /// Run fsck
    Fsck,
    /// Dump superblock
    DumpSuper,
    /// Dump inode
    DumpInode,
    /// Read block
    ReadBlock,
    /// Write block (dangerous)
    WriteBlock,
    /// Clear inode
    ClearInode,
    /// Set inode field
    SetInode,
    /// Reboot
    Reboot,
    /// Poweroff
    Poweroff,
    /// Help
    Help,
    /// Exit rescue shell
    Exit,
}

impl RescueCommand {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ls" | "dir" => Some(RescueCommand::Ls),
            "cd" => Some(RescueCommand::Cd),
            "pwd" => Some(RescueCommand::Pwd),
            "cat" | "type" => Some(RescueCommand::Cat),
            "hexdump" | "xxd" => Some(RescueCommand::Hexdump),
            "mount" => Some(RescueCommand::Mount),
            "umount" | "unmount" => Some(RescueCommand::Umount),
            "fsck" | "chkdsk" => Some(RescueCommand::Fsck),
            "dumpsuper" | "dumpe2fs" => Some(RescueCommand::DumpSuper),
            "stat" | "dumpinode" => Some(RescueCommand::DumpInode),
            "readblock" | "dd" => Some(RescueCommand::ReadBlock),
            "writeblock" => Some(RescueCommand::WriteBlock),
            "clri" => Some(RescueCommand::ClearInode),
            "seti" => Some(RescueCommand::SetInode),
            "reboot" => Some(RescueCommand::Reboot),
            "poweroff" | "halt" => Some(RescueCommand::Poweroff),
            "help" | "?" => Some(RescueCommand::Help),
            "exit" | "quit" => Some(RescueCommand::Exit),
            _ => None,
        }
    }
}

pub struct RescueShell {
    /// Current working directory inode
    pub cwd_inode: u64,
    /// Current mounted device
    pub mounted_device: Option<u32>,
    /// Is running
    pub running: bool,
    /// Last command status
    pub last_status: i32,
    /// Command history (simplified)
    history_count: u32,
}

impl RescueShell {
    pub fn new() -> Self {
        RescueShell {
            cwd_inode: 2,  // Root inode
            mounted_device: None,
            running: true,
            last_status: 0,
            history_count: 0,
        }
    }

    pub fn execute(&mut self, cmd: RescueCommand, _args: &[&str]) -> i32 {
        self.history_count += 1;

        match cmd {
            RescueCommand::Help => {
                // Would print help
                0
            }
            RescueCommand::Exit => {
                self.running = false;
                0
            }
            RescueCommand::Pwd => {
                // Would print current directory
                0
            }
            RescueCommand::Ls => {
                // Would list directory
                0
            }
            RescueCommand::Cd => {
                // Would change directory
                0
            }
            RescueCommand::Cat => {
                // Would print file contents
                0
            }
            RescueCommand::Hexdump => {
                // Would hexdump file/block
                0
            }
            RescueCommand::Mount => {
                // Would mount filesystem
                0
            }
            RescueCommand::Umount => {
                self.mounted_device = None;
                0
            }
            RescueCommand::Fsck => {
                // Would run filesystem check
                0
            }
            RescueCommand::DumpSuper => {
                // Would dump superblock info
                0
            }
            RescueCommand::DumpInode => {
                // Would dump inode info
                0
            }
            RescueCommand::ReadBlock => {
                // Would read and display block
                0
            }
            RescueCommand::WriteBlock => {
                // Would write block (with confirmation)
                0
            }
            RescueCommand::ClearInode => {
                // Would clear inode
                0
            }
            RescueCommand::SetInode => {
                // Would set inode field
                0
            }
            RescueCommand::Reboot => {
                // Would trigger reboot
                0
            }
            RescueCommand::Poweroff => {
                // Would trigger poweroff
                0
            }
        }
    }
}

// ===== Recovery Partition Manager =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RecoveryPartitionState {
    /// Not present
    NotPresent,
    /// Present and valid
    Valid,
    /// Present but outdated
    Outdated,
    /// Present but corrupted
    Corrupted,
    /// Currently booted from recovery
    Active,
}

#[derive(Copy, Clone)]
pub struct RecoveryPartition {
    /// Partition GUID
    pub partition_guid: [u8; 16],
    /// Start LBA
    pub start_lba: u64,
    /// Size in sectors
    pub size_sectors: u64,
    /// State
    pub state: RecoveryPartitionState,
    /// Recovery OS version
    pub version: u64,
    /// Last updated timestamp
    pub last_updated: u64,
    /// Contains full OS image
    pub full_image: bool,
    /// Contains repair tools only
    pub tools_only: bool,
}

impl RecoveryPartition {
    pub fn new() -> Self {
        RecoveryPartition {
            partition_guid: [0u8; 16],
            start_lba: 0,
            size_sectors: 0,
            state: RecoveryPartitionState::NotPresent,
            version: 0,
            last_updated: 0,
            full_image: false,
            tools_only: true,
        }
    }

    pub fn is_usable(&self) -> bool {
        matches!(self.state, RecoveryPartitionState::Valid | RecoveryPartitionState::Outdated)
    }
}

pub struct RecoveryPartitionManager {
    /// Primary recovery partition
    pub primary: RecoveryPartition,
    /// Secondary recovery (USB/network)
    pub secondary: Option<RecoveryPartition>,
    /// Currently in recovery mode
    pub in_recovery_mode: bool,
}

impl RecoveryPartitionManager {
    pub fn new() -> Self {
        RecoveryPartitionManager {
            primary: RecoveryPartition::new(),
            secondary: None,
            in_recovery_mode: false,
        }
    }

    pub fn detect_recovery_partitions(&mut self) -> bool {
        // Would scan GPT for recovery partition type GUID
        // Type GUID for recovery: DE94BBA4-06D1-4D40-A16A-BFD50179D6AC
        self.primary.state = RecoveryPartitionState::Valid;
        true
    }

    pub fn boot_to_recovery(&mut self) -> Result<(), &'static str> {
        if !self.primary.is_usable() {
            if let Some(ref sec) = self.secondary {
                if sec.is_usable() {
                    // Boot from secondary
                    self.in_recovery_mode = true;
                    return Ok(());
                }
            }
            return Err("No usable recovery partition");
        }

        // Would set boot-next to recovery and reboot
        self.in_recovery_mode = true;
        Ok(())
    }

    pub fn update_recovery_image(&mut self) -> Result<(), &'static str> {
        if self.primary.state == RecoveryPartitionState::NotPresent {
            return Err("No recovery partition");
        }

        // Would copy current system to recovery
        self.primary.state = RecoveryPartitionState::Valid;
        self.primary.last_updated = 0;  // Would be current timestamp
        Ok(())
    }
}

// ===== Tests =====

pub fn test_fs_checker() -> bool {
    let mut checker = FsChecker::new(FilesystemType::RayFs, 1);
    checker.auto_repair = true;
    checker.read_only = false;

    // Run check
    if !checker.run_full_check() {
        return false;
    }

    if checker.phase != CheckPhase::Complete {
        return false;
    }

    true
}

pub fn test_error_severity() -> bool {
    if FsErrorType::SuperblockCorrupt.severity() != ErrorSeverity::Critical {
        return false;
    }
    if FsErrorType::InvalidTimestamp.severity() != ErrorSeverity::Low {
        return false;
    }
    if FsErrorType::BadBlock.severity() != ErrorSeverity::High {
        return false;
    }
    true
}

pub fn test_rescue_commands() -> bool {
    if RescueCommand::from_str("ls") != Some(RescueCommand::Ls) {
        return false;
    }
    if RescueCommand::from_str("fsck") != Some(RescueCommand::Fsck) {
        return false;
    }
    if RescueCommand::from_str("invalid").is_some() {
        return false;
    }
    true
}

pub fn test_recovery_partition() -> bool {
    let mut manager = RecoveryPartitionManager::new();

    if manager.detect_recovery_partitions() {
        if manager.primary.state != RecoveryPartitionState::Valid {
            return false;
        }
    }

    if manager.boot_to_recovery().is_err() {
        return false;
    }

    if !manager.in_recovery_mode {
        return false;
    }

    true
}
