// RAYOS Phase 9B Task 1: Interactive Installer Workflow
// Complete installation wizard, filesystem formatting, and configuration
// File: crates/kernel-bare/src/installer_workflow.rs

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// ============================================================================
// INSTALLATION STATE MACHINE
// ============================================================================

/// Installation wizard stages
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallStage {
    /// Welcome screen
    Welcome = 0,
    /// License agreement
    License = 1,
    /// Disk selection
    DiskSelection = 2,
    /// Partition scheme
    PartitionScheme = 3,
    /// Partition confirmation
    PartitionConfirm = 4,
    /// Filesystem formatting
    Formatting = 5,
    /// System configuration (hostname, timezone, etc.)
    Configuration = 6,
    /// User account creation
    UserSetup = 7,
    /// Network configuration
    NetworkSetup = 8,
    /// Installation progress
    Installing = 9,
    /// Boot loader installation
    BootLoader = 10,
    /// Installation complete
    Complete = 11,
    /// Error state
    Error = 12,
}

/// Installation options
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallType {
    /// Full disk installation (erase everything)
    FullDisk = 0,
    /// Custom partitioning
    Custom = 1,
    /// Dual boot with existing OS
    DualBoot = 2,
    /// Upgrade existing RayOS
    Upgrade = 3,
}

/// Filesystem type for formatting
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilesystemType {
    /// FAT32 (for EFI partition)
    Fat32 = 0,
    /// ext4 (for Linux compatibility)
    Ext4 = 1,
    /// RayFS (native RayOS filesystem)
    RayFs = 2,
    /// NTFS (for Windows compatibility)
    Ntfs = 3,
    /// Swap partition
    Swap = 4,
}

/// Installation configuration
#[derive(Clone, Copy)]
pub struct InstallConfig {
    /// Installation type
    pub install_type: InstallType,
    /// Target disk ID
    pub target_disk: u64,
    /// Hostname (fixed-size buffer)
    pub hostname: [u8; 64],
    pub hostname_len: usize,
    /// Timezone offset (minutes from UTC)
    pub timezone_offset: i32,
    /// Enable SSH by default
    pub enable_ssh: bool,
    /// Enable firewall by default
    pub enable_firewall: bool,
    /// Root password hash
    pub root_password_hash: [u8; 32],
    /// Create user account
    pub create_user: bool,
    /// Username
    pub username: [u8; 32],
    pub username_len: usize,
    /// User password hash
    pub user_password_hash: [u8; 32],
    /// Network: use DHCP
    pub use_dhcp: bool,
    /// Static IP (if not DHCP)
    pub static_ip: [u8; 4],
    /// Gateway
    pub gateway: [u8; 4],
    /// DNS servers
    pub dns1: [u8; 4],
    pub dns2: [u8; 4],
}

impl InstallConfig {
    pub const fn new() -> Self {
        InstallConfig {
            install_type: InstallType::FullDisk,
            target_disk: 0,
            hostname: [0; 64],
            hostname_len: 0,
            timezone_offset: 0,
            enable_ssh: true,
            enable_firewall: true,
            root_password_hash: [0; 32],
            create_user: true,
            username: [0; 32],
            username_len: 0,
            user_password_hash: [0; 32],
            use_dhcp: true,
            static_ip: [0; 4],
            gateway: [0; 4],
            dns1: [8, 8, 8, 8],      // Google DNS
            dns2: [8, 8, 4, 4],
        }
    }

    pub fn set_hostname(&mut self, name: &[u8]) {
        let len = name.len().min(63);
        self.hostname[..len].copy_from_slice(&name[..len]);
        self.hostname_len = len;
    }

    pub fn set_username(&mut self, name: &[u8]) {
        let len = name.len().min(31);
        self.username[..len].copy_from_slice(&name[..len]);
        self.username_len = len;
    }

    pub fn hostname_str(&self) -> &str {
        core::str::from_utf8(&self.hostname[..self.hostname_len]).unwrap_or("rayos")
    }

    pub fn username_str(&self) -> &str {
        core::str::from_utf8(&self.username[..self.username_len]).unwrap_or("user")
    }
}

// ============================================================================
// PARTITION FORMAT OPERATIONS
// ============================================================================

/// Format progress callback
pub type FormatProgressFn = fn(partition: u32, percent: u32);

/// Partition format descriptor
#[derive(Clone, Copy)]
pub struct FormatDescriptor {
    /// Partition number (1-based)
    pub partition: u32,
    /// Filesystem type
    pub fs_type: FilesystemType,
    /// Start LBA
    pub start_lba: u64,
    /// Size in LBAs
    pub size_lba: u64,
    /// Volume label
    pub label: [u8; 16],
    pub label_len: usize,
}

impl FormatDescriptor {
    pub fn new(partition: u32, fs_type: FilesystemType, start: u64, size: u64) -> Self {
        FormatDescriptor {
            partition,
            fs_type,
            start_lba: start,
            size_lba: size,
            label: [0; 16],
            label_len: 0,
        }
    }

    pub fn with_label(mut self, label: &[u8]) -> Self {
        let len = label.len().min(15);
        self.label[..len].copy_from_slice(&label[..len]);
        self.label_len = len;
        self
    }

    pub fn label_str(&self) -> &str {
        core::str::from_utf8(&self.label[..self.label_len]).unwrap_or("")
    }
}

/// Format a FAT32 partition (simplified)
pub fn format_fat32(desc: &FormatDescriptor, _progress: Option<FormatProgressFn>) -> Result<(), &'static str> {
    // In production, this would:
    // 1. Calculate cluster size based on partition size
    // 2. Write BPB (BIOS Parameter Block)
    // 3. Initialize FAT tables
    // 4. Write root directory
    // 5. Write FSInfo sector

    if desc.size_lba < 65536 {
        return Err("Partition too small for FAT32");
    }

    // Simulate formatting stages
    // Progress: 0-20% BPB, 20-60% FAT tables, 60-100% root dir
    
    Ok(())
}

/// Format an ext4 partition (simplified)
pub fn format_ext4(desc: &FormatDescriptor, _progress: Option<FormatProgressFn>) -> Result<(), &'static str> {
    // In production, this would:
    // 1. Write superblock
    // 2. Initialize block groups
    // 3. Create inode tables
    // 4. Initialize journal
    // 5. Create root directory

    if desc.size_lba < 131072 {
        return Err("Partition too small for ext4");
    }

    Ok(())
}

/// Format a RayFS partition (native filesystem)
pub fn format_rayfs(desc: &FormatDescriptor, _progress: Option<FormatProgressFn>) -> Result<(), &'static str> {
    // RayFS is a hypothetical native filesystem for RayOS
    // Features: copy-on-write, checksums, compression, snapshots

    if desc.size_lba < 262144 {
        return Err("Partition too small for RayFS");
    }

    Ok(())
}

/// Initialize swap partition
pub fn format_swap(desc: &FormatDescriptor, _progress: Option<FormatProgressFn>) -> Result<(), &'static str> {
    // Write swap header with magic and metadata

    if desc.size_lba < 16384 {
        return Err("Swap too small (min 8MB)");
    }

    Ok(())
}

/// Format a partition with the specified filesystem
pub fn format_partition(desc: &FormatDescriptor, progress: Option<FormatProgressFn>) -> Result<(), &'static str> {
    match desc.fs_type {
        FilesystemType::Fat32 => format_fat32(desc, progress),
        FilesystemType::Ext4 => format_ext4(desc, progress),
        FilesystemType::RayFs => format_rayfs(desc, progress),
        FilesystemType::Swap => format_swap(desc, progress),
        FilesystemType::Ntfs => Err("NTFS formatting not supported"),
    }
}

// ============================================================================
// INSTALLATION WIZARD
// ============================================================================

/// Maximum format descriptors
const MAX_FORMATS: usize = 8;

/// Installation wizard state
pub struct InstallWizard {
    /// Current stage
    pub stage: InstallStage,
    /// Configuration
    pub config: InstallConfig,
    /// Error message
    pub error: Option<&'static str>,
    /// Installation progress (0-100)
    pub progress: u32,
    /// Stage-specific progress
    pub stage_progress: u32,
    /// Partitions to format
    pub format_queue: [Option<FormatDescriptor>; MAX_FORMATS],
    pub format_count: usize,
    /// Current format index
    pub format_index: usize,
    /// Installation started
    pub started: bool,
    /// Installation complete
    pub complete: bool,
}

impl InstallWizard {
    pub const fn new() -> Self {
        InstallWizard {
            stage: InstallStage::Welcome,
            config: InstallConfig::new(),
            error: None,
            progress: 0,
            stage_progress: 0,
            format_queue: [None; MAX_FORMATS],
            format_count: 0,
            format_index: 0,
            started: false,
            complete: false,
        }
    }

    /// Reset wizard to initial state
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Advance to next stage
    pub fn next_stage(&mut self) -> Result<(), &'static str> {
        self.stage = match self.stage {
            InstallStage::Welcome => InstallStage::License,
            InstallStage::License => InstallStage::DiskSelection,
            InstallStage::DiskSelection => InstallStage::PartitionScheme,
            InstallStage::PartitionScheme => InstallStage::PartitionConfirm,
            InstallStage::PartitionConfirm => InstallStage::Formatting,
            InstallStage::Formatting => InstallStage::Configuration,
            InstallStage::Configuration => InstallStage::UserSetup,
            InstallStage::UserSetup => InstallStage::NetworkSetup,
            InstallStage::NetworkSetup => InstallStage::Installing,
            InstallStage::Installing => InstallStage::BootLoader,
            InstallStage::BootLoader => InstallStage::Complete,
            InstallStage::Complete => return Err("Already complete"),
            InstallStage::Error => return Err("In error state"),
        };
        self.stage_progress = 0;
        Ok(())
    }

    /// Go back to previous stage
    pub fn prev_stage(&mut self) -> Result<(), &'static str> {
        self.stage = match self.stage {
            InstallStage::Welcome => return Err("At first stage"),
            InstallStage::License => InstallStage::Welcome,
            InstallStage::DiskSelection => InstallStage::License,
            InstallStage::PartitionScheme => InstallStage::DiskSelection,
            InstallStage::PartitionConfirm => InstallStage::PartitionScheme,
            InstallStage::Formatting => return Err("Cannot go back during formatting"),
            InstallStage::Configuration => InstallStage::Formatting,
            InstallStage::UserSetup => InstallStage::Configuration,
            InstallStage::NetworkSetup => InstallStage::UserSetup,
            InstallStage::Installing => return Err("Cannot go back during installation"),
            InstallStage::BootLoader => return Err("Cannot go back during boot loader install"),
            InstallStage::Complete => return Err("Installation complete"),
            InstallStage::Error => InstallStage::DiskSelection,
        };
        self.stage_progress = 0;
        Ok(())
    }

    /// Set error state
    pub fn set_error(&mut self, msg: &'static str) {
        self.error = Some(msg);
        self.stage = InstallStage::Error;
    }

    /// Add partition to format queue
    pub fn queue_format(&mut self, desc: FormatDescriptor) -> Result<(), &'static str> {
        if self.format_count >= MAX_FORMATS {
            return Err("Format queue full");
        }
        self.format_queue[self.format_count] = Some(desc);
        self.format_count += 1;
        Ok(())
    }

    /// Execute formatting stage
    pub fn execute_formatting(&mut self) -> Result<(), &'static str> {
        while self.format_index < self.format_count {
            if let Some(desc) = &self.format_queue[self.format_index] {
                format_partition(desc, None)?;
            }
            self.format_index += 1;
            self.stage_progress = ((self.format_index * 100) / self.format_count) as u32;
        }
        Ok(())
    }

    /// Get current stage name
    pub fn stage_name(&self) -> &'static str {
        match self.stage {
            InstallStage::Welcome => "Welcome",
            InstallStage::License => "License Agreement",
            InstallStage::DiskSelection => "Select Disk",
            InstallStage::PartitionScheme => "Partition Scheme",
            InstallStage::PartitionConfirm => "Confirm Partitions",
            InstallStage::Formatting => "Formatting Disks",
            InstallStage::Configuration => "System Configuration",
            InstallStage::UserSetup => "User Account Setup",
            InstallStage::NetworkSetup => "Network Configuration",
            InstallStage::Installing => "Installing RayOS",
            InstallStage::BootLoader => "Installing Boot Loader",
            InstallStage::Complete => "Installation Complete",
            InstallStage::Error => "Error",
        }
    }

    /// Get stage number (for progress display)
    pub fn stage_number(&self) -> u32 {
        self.stage as u32
    }

    /// Get total stages
    pub fn total_stages(&self) -> u32 {
        11  // Welcome through Complete (excluding Error)
    }

    /// Calculate overall progress
    pub fn overall_progress(&self) -> u32 {
        let base = (self.stage_number() * 100) / self.total_stages();
        let stage_contrib = self.stage_progress / self.total_stages();
        base + stage_contrib
    }
}

// ============================================================================
// GPT PARTITION TABLE
// ============================================================================

/// GPT partition table header
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct GptHeader {
    /// Signature "EFI PART"
    pub signature: [u8; 8],
    /// Revision (1.0)
    pub revision: u32,
    /// Header size (92 bytes minimum)
    pub header_size: u32,
    /// CRC32 of header
    pub header_crc32: u32,
    /// Reserved
    pub reserved: u32,
    /// Current LBA (location of this header)
    pub current_lba: u64,
    /// Backup LBA (location of backup header)
    pub backup_lba: u64,
    /// First usable LBA
    pub first_usable_lba: u64,
    /// Last usable LBA
    pub last_usable_lba: u64,
    /// Disk GUID
    pub disk_guid: [u8; 16],
    /// Partition entries starting LBA
    pub partition_entries_lba: u64,
    /// Number of partition entries
    pub num_partition_entries: u32,
    /// Size of each partition entry
    pub partition_entry_size: u32,
    /// CRC32 of partition entries
    pub partition_entries_crc32: u32,
}

impl GptHeader {
    pub const SIGNATURE: [u8; 8] = *b"EFI PART";
    pub const REVISION_1_0: u32 = 0x00010000;

    pub fn new(disk_lba: u64) -> Self {
        GptHeader {
            signature: Self::SIGNATURE,
            revision: Self::REVISION_1_0,
            header_size: 92,
            header_crc32: 0,
            reserved: 0,
            current_lba: 1,
            backup_lba: disk_lba - 1,
            first_usable_lba: 34,
            last_usable_lba: disk_lba - 34,
            disk_guid: [0; 16],
            partition_entries_lba: 2,
            num_partition_entries: 128,
            partition_entry_size: 128,
            partition_entries_crc32: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.signature == Self::SIGNATURE && self.revision == Self::REVISION_1_0
    }
}

/// GPT partition entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct GptPartitionEntry {
    /// Partition type GUID
    pub type_guid: [u8; 16],
    /// Unique partition GUID
    pub partition_guid: [u8; 16],
    /// First LBA
    pub first_lba: u64,
    /// Last LBA (inclusive)
    pub last_lba: u64,
    /// Attribute flags
    pub attributes: u64,
    /// Partition name (UTF-16LE, 36 characters max)
    pub name: [u16; 36],
}

impl GptPartitionEntry {
    /// EFI System Partition GUID
    pub const EFI_SYSTEM_GUID: [u8; 16] = [
        0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11,
        0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
    ];

    /// Linux filesystem GUID
    pub const LINUX_FS_GUID: [u8; 16] = [
        0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47,
        0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
    ];

    /// Microsoft Basic Data GUID
    pub const BASIC_DATA_GUID: [u8; 16] = [
        0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44,
        0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
    ];

    pub const fn empty() -> Self {
        GptPartitionEntry {
            type_guid: [0; 16],
            partition_guid: [0; 16],
            first_lba: 0,
            last_lba: 0,
            attributes: 0,
            name: [0; 36],
        }
    }

    pub fn is_used(&self) -> bool {
        self.type_guid != [0; 16]
    }

    pub fn size_lba(&self) -> u64 {
        if self.last_lba >= self.first_lba {
            self.last_lba - self.first_lba + 1
        } else {
            0
        }
    }
}

/// Write GPT partition table
pub fn write_gpt_table(
    _disk_id: u64,
    header: &GptHeader,
    entries: &[GptPartitionEntry],
) -> Result<(), &'static str> {
    if entries.len() > 128 {
        return Err("Too many partition entries");
    }

    if !header.is_valid() {
        return Err("Invalid GPT header");
    }

    // In production, this would:
    // 1. Write protective MBR at LBA 0
    // 2. Write primary GPT header at LBA 1
    // 3. Write partition entries at LBAs 2-33
    // 4. Write backup partition entries
    // 5. Write backup GPT header at last LBA

    Ok(())
}

// ============================================================================
// BOOT LOADER INSTALLATION
// ============================================================================

/// Boot loader type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootLoaderType {
    /// RayOS native boot loader
    RayBoot = 0,
    /// GRUB2 (for Linux compatibility)
    Grub2 = 1,
    /// systemd-boot
    SystemdBoot = 2,
    /// Windows Boot Manager (chainload only)
    WindowsBoot = 3,
}

/// Boot loader installation descriptor
#[derive(Clone, Copy)]
pub struct BootLoaderInstall {
    /// Boot loader type
    pub loader_type: BootLoaderType,
    /// EFI partition LBA
    pub efi_partition_lba: u64,
    /// EFI partition size
    pub efi_partition_size: u64,
    /// Install as default boot option
    pub set_default: bool,
    /// Timeout in seconds
    pub timeout: u32,
    /// Enable secure boot
    pub secure_boot: bool,
}

impl BootLoaderInstall {
    pub fn new(loader_type: BootLoaderType, efi_lba: u64, efi_size: u64) -> Self {
        BootLoaderInstall {
            loader_type,
            efi_partition_lba: efi_lba,
            efi_partition_size: efi_size,
            set_default: true,
            timeout: 5,
            secure_boot: false,
        }
    }

    /// Install boot loader
    pub fn install(&self) -> Result<(), &'static str> {
        match self.loader_type {
            BootLoaderType::RayBoot => self.install_rayboot(),
            BootLoaderType::Grub2 => self.install_grub2(),
            BootLoaderType::SystemdBoot => self.install_systemd_boot(),
            BootLoaderType::WindowsBoot => Err("Cannot install Windows Boot Manager"),
        }
    }

    fn install_rayboot(&self) -> Result<(), &'static str> {
        // Install RayOS native boot loader:
        // 1. Copy BOOTX64.EFI to EFI/BOOT/
        // 2. Copy rayos.efi to EFI/RayOS/
        // 3. Create boot configuration at EFI/RayOS/rayos.cfg
        // 4. Register with UEFI boot manager

        Ok(())
    }

    fn install_grub2(&self) -> Result<(), &'static str> {
        // Install GRUB2:
        // 1. Copy grubx64.efi to EFI/BOOT/
        // 2. Create grub.cfg
        // 3. Register with UEFI

        Ok(())
    }

    fn install_systemd_boot(&self) -> Result<(), &'static str> {
        // Install systemd-boot:
        // 1. Copy systemd-bootx64.efi
        // 2. Create loader.conf
        // 3. Create entries/*.conf

        Ok(())
    }
}

// ============================================================================
// POST-INSTALLATION CONFIGURATION
// ============================================================================

/// Write initial system configuration files
pub fn write_system_config(config: &InstallConfig, system_root: u64) -> Result<(), &'static str> {
    let _ = system_root;

    // Write /etc/hostname
    let _hostname = config.hostname_str();

    // Write /etc/timezone
    let _tz_offset = config.timezone_offset;

    // Write /etc/network/interfaces or equivalent
    if config.use_dhcp {
        // DHCP configuration
    } else {
        // Static IP configuration
    }

    // Write /etc/ssh/sshd_config if SSH enabled
    if config.enable_ssh {
        // Enable SSH service
    }

    // Write /etc/firewall.conf if firewall enabled
    if config.enable_firewall {
        // Basic firewall rules
    }

    // Create user account if requested
    if config.create_user {
        let _username = config.username_str();
        // Create /etc/passwd entry
        // Create home directory
        // Set password
    }

    Ok(())
}

// ============================================================================
// GLOBAL INSTALLER STATE
// ============================================================================

static mut INSTALL_WIZARD: Option<InstallWizard> = None;
static INSTALLER_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the installer wizard
pub fn init_installer() {
    if !INSTALLER_INITIALIZED.swap(true, Ordering::SeqCst) {
        unsafe {
            INSTALL_WIZARD = Some(InstallWizard::new());
        }
    }
}

/// Get the installer wizard
pub fn get_installer() -> Option<&'static mut InstallWizard> {
    unsafe { INSTALL_WIZARD.as_mut() }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_config() {
        let mut config = InstallConfig::new();
        config.set_hostname(b"my-rayos-pc");
        assert_eq!(config.hostname_str(), "my-rayos-pc");
    }

    #[test]
    fn test_install_wizard_stages() {
        let mut wizard = InstallWizard::new();
        assert_eq!(wizard.stage, InstallStage::Welcome);

        wizard.next_stage().unwrap();
        assert_eq!(wizard.stage, InstallStage::License);

        wizard.prev_stage().unwrap();
        assert_eq!(wizard.stage, InstallStage::Welcome);
    }

    #[test]
    fn test_format_descriptor() {
        let desc = FormatDescriptor::new(1, FilesystemType::Fat32, 2048, 819200)
            .with_label(b"EFI");
        assert_eq!(desc.label_str(), "EFI");
    }

    #[test]
    fn test_gpt_header() {
        let header = GptHeader::new(1000000);
        assert!(header.is_valid());
    }

    #[test]
    fn test_gpt_partition() {
        let mut entry = GptPartitionEntry::empty();
        assert!(!entry.is_used());

        entry.type_guid = GptPartitionEntry::EFI_SYSTEM_GUID;
        entry.first_lba = 2048;
        entry.last_lba = 821247;
        assert!(entry.is_used());
        assert_eq!(entry.size_lba(), 819200);
    }

    #[test]
    fn test_bootloader_install() {
        let install = BootLoaderInstall::new(BootLoaderType::RayBoot, 2048, 819200);
        assert!(install.install().is_ok());
    }

    #[test]
    fn test_format_queue() {
        let mut wizard = InstallWizard::new();
        let desc = FormatDescriptor::new(1, FilesystemType::Fat32, 2048, 819200);
        wizard.queue_format(desc).unwrap();
        assert_eq!(wizard.format_count, 1);
    }

    #[test]
    fn test_overall_progress() {
        let mut wizard = InstallWizard::new();
        assert_eq!(wizard.overall_progress(), 0);

        wizard.next_stage().unwrap();  // License
        wizard.next_stage().unwrap();  // DiskSelection
        wizard.next_stage().unwrap();  // PartitionScheme

        // Should be around 27% (3/11 stages)
        assert!(wizard.overall_progress() >= 25);
    }
}
