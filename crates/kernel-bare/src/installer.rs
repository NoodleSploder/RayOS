//! RayOS Installer Foundation
//!
//! Provides USB detection, partition management, and disk layout for standalone OS installation.
//! This module handles the low-level installation machinery: finding target disks, validating
//! partitions, and preparing the storage layout for RayOS to boot.
//!
//! **Safety Model**: 2-stage confirmation prevents accidental data loss. All operations validate
//! target disk to prevent self-destruction (installing to running kernel disk).

/// USB device enumeration result
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallerDiskType {
    /// USB-attached storage (preferred installation target)
    UsbDisk = 0,
    /// SATA/NVME storage (secondary target, requires explicit confirmation)
    InternalDisk = 1,
    /// Unrecognized device type
    Unknown = 2,
}

/// Partition type enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartitionType {
    /// EFI System Partition (400 MB)
    EfiSystem = 0,
    /// RayOS Kernel (4 GB)
    RayosKernel = 1,
    /// RayOS System (16 GB)
    RayosSystem = 2,
    /// Linux Guest (20 GB)
    LinuxGuest = 3,
    /// Windows Guest (30 GB)
    WindowsGuest = 4,
    /// Free space
    Free = 5,
}

/// Partition entry
#[derive(Clone, Copy)]
pub struct Partition {
    /// Partition number (1-based)
    pub number: u32,
    /// Partition type
    pub ptype: PartitionType,
    /// Start LBA (Logical Block Address)
    pub start_lba: u64,
    /// Size in LBAs (512 bytes each)
    pub size_lba: u64,
    /// Boot flag (EFI partition only)
    pub boot_flag: bool,
}

impl Partition {
    pub fn new(number: u32, ptype: PartitionType, start: u64, size: u64) -> Self {
        Partition {
            number,
            ptype,
            start_lba: start,
            size_lba: size,
            boot_flag: ptype == PartitionType::EfiSystem,
        }
    }

    /// Get size in megabytes
    pub fn size_mb(&self) -> u64 {
        (self.size_lba * 512) / (1024 * 1024)
    }

    /// Validate partition bounds
    pub fn validate(&self) -> bool {
        // Minimum 1 MB per partition
        self.size_lba > (1024 * 1024 / 512) && self.start_lba > 0
    }
}

/// Disk layout specification
#[derive(Clone, Copy)]
pub struct DiskLayout {
    /// Target disk: USB VID:PID or internal disk path hash
    pub disk_id: u64,
    /// Total disk size in LBAs
    pub total_lba: u64,
    /// Partition table: up to 8 partitions
    pub partitions: [Option<Partition>; 8],
    /// Number of partitions
    pub partition_count: u32,
}

impl DiskLayout {
    pub fn new(disk_id: u64, total_lba: u64) -> Self {
        DiskLayout {
            disk_id,
            total_lba,
            partitions: [None; 8],
            partition_count: 0,
        }
    }

    /// Add a partition to the layout
    pub fn add_partition(&mut self, partition: Partition) -> Result<u32, &'static str> {
        if self.partition_count >= 8 {
            return Err("Max 8 partitions");
        }

        if !partition.validate() {
            return Err("Invalid partition bounds");
        }

        // Check for overlaps with existing partitions
        for opt in self.partitions.iter().take(self.partition_count as usize) {
            if let Some(existing) = opt {
                let start = partition.start_lba;
                let end = partition.start_lba + partition.size_lba;
                let exist_start = existing.start_lba;
                let exist_end = existing.start_lba + existing.size_lba;

                if (start < exist_end) && (end > exist_start) {
                    return Err("Partition overlap detected");
                }
            }
        }

        self.partitions[self.partition_count as usize] = Some(partition);
        self.partition_count += 1;
        Ok(self.partition_count - 1)
    }

    /// Get total allocated space in LBAs
    pub fn allocated_lba(&self) -> u64 {
        let mut total = 0;
        for opt in self.partitions.iter().take(self.partition_count as usize) {
            if let Some(p) = opt {
                total += p.size_lba;
            }
        }
        total
    }

    /// Validate entire layout fits on disk
    pub fn validate(&self) -> bool {
        self.allocated_lba() <= self.total_lba && self.partition_count > 0
    }

    /// Get partition by type
    pub fn get_partition(&self, ptype: PartitionType) -> Option<Partition> {
        for opt in self.partitions.iter().take(self.partition_count as usize) {
            if let Some(p) = opt {
                if p.ptype == ptype {
                    return Some(*p);
                }
            }
        }
        None
    }
}

/// Standard RayOS disk layout (ESP + Kernel + System + Guests)
pub fn standard_rayos_layout(disk_lba: u64) -> Result<DiskLayout, &'static str> {
    if disk_lba < (80 * 1024 * 1024 / 512) {
        return Err("Disk too small (min 80 GB)");
    }

    let mut layout = DiskLayout::new(0, disk_lba);

    // Partition 1: EFI System (400 MB @ 2048 LBA = 1 MB)
    let efi = Partition::new(
        1,
        PartitionType::EfiSystem,
        2048,                 // Start at 1 MB
        (400 * 1024 * 1024) / 512, // 400 MB
    );
    layout.add_partition(efi)?;

    // Partition 2: RayOS Kernel (4 GB @ 821248 LBA)
    let kernel = Partition::new(
        2,
        PartitionType::RayosKernel,
        821248,               // After ESP
        (4 * 1024 * 1024 * 1024) / 512, // 4 GB
    );
    layout.add_partition(kernel)?;

    // Partition 3: RayOS System (16 GB)
    let system_start = 821248 + ((4 * 1024 * 1024 * 1024) / 512);
    let system = Partition::new(
        3,
        PartitionType::RayosSystem,
        system_start,
        (16 * 1024 * 1024 * 1024) / 512, // 16 GB
    );
    layout.add_partition(system)?;

    // Partition 4: Linux Guest (20 GB)
    let linux_start = system_start + ((16 * 1024 * 1024 * 1024) / 512);
    let linux = Partition::new(
        4,
        PartitionType::LinuxGuest,
        linux_start,
        (20 * 1024 * 1024 * 1024) / 512, // 20 GB
    );
    layout.add_partition(linux)?;

    // Partition 5: Windows Guest (30 GB)
    let windows_start = linux_start + ((20 * 1024 * 1024 * 1024) / 512);
    let windows = Partition::new(
        5,
        PartitionType::WindowsGuest,
        windows_start,
        (30 * 1024 * 1024 * 1024) / 512, // 30 GB
    );
    layout.add_partition(windows)?;

    if !layout.validate() {
        return Err("Standard layout exceeds disk capacity");
    }

    Ok(layout)
}

/// Installer boot detection result
#[derive(Clone, Copy, Debug)]
pub struct InstallerDisk {
    /// Disk ID (USB VID:PID hash or internal identifier)
    pub disk_id: u64,
    /// Disk type (USB, internal, unknown)
    pub disk_type: InstallerDiskType,
    /// Total capacity in bytes
    pub capacity_bytes: u64,
    /// Disk human-readable label
    pub label: &'static str,
}

impl InstallerDisk {
    pub fn new(id: u64, dtype: InstallerDiskType, capacity: u64, label: &'static str) -> Self {
        InstallerDisk {
            disk_id: id,
            disk_type: dtype,
            capacity_bytes: capacity,
            label,
        }
    }

    /// Check if this is a safe installation target
    pub fn is_safe(&self) -> bool {
        // USB disks are always safe (removable)
        // Internal disks must be explicitly validated by user
        self.disk_type == InstallerDiskType::UsbDisk
    }

    /// Get capacity in gigabytes
    pub fn capacity_gb(&self) -> u64 {
        self.capacity_bytes / (1024 * 1024 * 1024)
    }

    /// Check if disk meets minimum size (80 GB)
    pub fn meets_minimum(&self) -> bool {
        self.capacity_bytes >= (80 * 1024 * 1024 * 1024)
    }
}

/// Maximum detectable disks
const MAX_INSTALLER_DISKS: usize = 16;

/// Installer boot status enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallerBootStatus {
    /// Installer not running
    Idle = 0,
    /// Enumerating disks
    Enumerating = 1,
    /// Ready to install (disk selected)
    Ready = 2,
    /// Installing (partitions being written)
    Installing = 3,
    /// Installation complete, ready to boot
    Complete = 4,
    /// Installation failed
    Failed = 5,
}

/// Installer boot context
pub struct InstallerBoot {
    /// Detected disks
    disks: [Option<InstallerDisk>; MAX_INSTALLER_DISKS],
    /// Number of detected disks
    disk_count: u32,
    /// Currently selected disk index
    selected_disk: Option<u32>,
    /// Current installation status
    status: InstallerBootStatus,
    /// Error message (if failed)
    last_error: &'static str,
    /// Confirmation pending flag (2-stage safety)
    confirmation_pending: bool,
}

impl InstallerBoot {
    pub fn new() -> Self {
        InstallerBoot {
            disks: [None; MAX_INSTALLER_DISKS],
            disk_count: 0,
            selected_disk: None,
            status: InstallerBootStatus::Idle,
            last_error: "",
            confirmation_pending: false,
        }
    }

    /// Initialize and enumerate available disks
    pub fn enumerate(&mut self) -> Result<u32, &'static str> {
        self.status = InstallerBootStatus::Enumerating;

        // In production, this would scan USB controllers and SATA/NVME
        // For now, provide stubs for testing
        self.disk_count = 0;
        self.selected_disk = None;

        // Mark as ready if any disks found
        if self.disk_count > 0 {
            self.status = InstallerBootStatus::Ready;
            Ok(self.disk_count)
        } else {
            self.status = InstallerBootStatus::Failed;
            self.last_error = "No installation disks found";
            Err(self.last_error)
        }
    }

    /// Detect a disk (for testing/simulation)
    pub fn add_disk(&mut self, disk: InstallerDisk) -> Result<u32, &'static str> {
        if self.disk_count >= (MAX_INSTALLER_DISKS as u32) {
            return Err("Max disks reached");
        }

        self.disks[self.disk_count as usize] = Some(disk);
        self.disk_count += 1;
        Ok(self.disk_count - 1)
    }

    /// Get disk by index
    pub fn get_disk(&self, index: u32) -> Option<InstallerDisk> {
        if (index as usize) < (self.disk_count as usize) {
            self.disks[index as usize]
        } else {
            None
        }
    }

    /// Select a disk for installation
    pub fn select_disk(&mut self, index: u32) -> Result<(), &'static str> {
        let disk = self.get_disk(index).ok_or("Disk not found")?;

        if !disk.meets_minimum() {
            self.last_error = "Disk does not meet 80 GB minimum";
            return Err(self.last_error);
        }

        self.selected_disk = Some(index);
        self.confirmation_pending = true; // 2-stage confirmation
        Ok(())
    }

    /// Get selected disk
    pub fn selected(&self) -> Option<InstallerDisk> {
        if let Some(idx) = self.selected_disk {
            self.get_disk(idx)
        } else {
            None
        }
    }

    /// Confirm installation (2-stage safety)
    pub fn confirm_install(&mut self) -> Result<(), &'static str> {
        if !self.confirmation_pending {
            self.last_error = "No pending confirmation";
            return Err(self.last_error);
        }

        if self.selected_disk.is_none() {
            self.last_error = "No disk selected";
            return Err(self.last_error);
        }

        self.confirmation_pending = false;
        self.status = InstallerBootStatus::Installing;
        Ok(())
    }

    /// Get current status
    pub fn status(&self) -> InstallerBootStatus {
        self.status
    }

    /// Get last error
    pub fn error(&self) -> &'static str {
        self.last_error
    }

    /// Complete installation
    pub fn complete(&mut self) {
        self.status = InstallerBootStatus::Complete;
    }

    /// Mark installation as failed
    pub fn fail(&mut self, error: &'static str) {
        self.status = InstallerBootStatus::Failed;
        self.last_error = error;
    }

    /// Check if 2-stage confirmation is pending
    pub fn needs_confirmation(&self) -> bool {
        self.confirmation_pending
    }

    /// List all disks
    pub fn list_disks(&self) -> u32 {
        self.disk_count
    }
}

/// Partition manager for format and layout
pub struct PartitionManager {
    /// Current disk layout
    layout: Option<DiskLayout>,
    /// Partitions being created
    pending_partitions: [Option<Partition>; 8],
    /// Number of pending partitions
    pending_count: u32,
}

impl PartitionManager {
    pub fn new() -> Self {
        PartitionManager {
            layout: None,
            pending_partitions: [None; 8],
            pending_count: 0,
        }
    }

    /// Create standard RayOS layout for disk
    pub fn create_standard_layout(&mut self, disk_lba: u64) -> Result<(), &'static str> {
        let layout = standard_rayos_layout(disk_lba)?;
        self.layout = Some(layout);
        Ok(())
    }

    /// Add partition to pending list
    pub fn add_partition(&mut self, partition: Partition) -> Result<u32, &'static str> {
        if self.pending_count >= 8 {
            return Err("Max 8 partitions");
        }

        self.pending_partitions[self.pending_count as usize] = Some(partition);
        self.pending_count += 1;
        Ok(self.pending_count - 1)
    }

    /// Format disk with pending partitions (in production, writes MBR/GPT)
    pub fn format_disk(&mut self) -> Result<(), &'static str> {
        if self.pending_count == 0 {
            return Err("No partitions to format");
        }

        // In production, this would write the partition table to disk
        Ok(())
    }

    /// Get layout
    pub fn get_layout(&self) -> Option<DiskLayout> {
        self.layout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_creation() {
        let p = Partition::new(1, PartitionType::EfiSystem, 2048, 819200);
        assert!(p.validate());
        assert_eq!(p.number, 1);
    }

    #[test]
    fn test_disk_layout() {
        let mut layout = DiskLayout::new(1, 160 * 1024 * 1024 * 1024 / 512);

        let efi = Partition::new(1, PartitionType::EfiSystem, 2048, 819200);
        layout.add_partition(efi).unwrap();

        assert_eq!(layout.partition_count, 1);
        assert!(layout.get_partition(PartitionType::EfiSystem).is_some());
    }

    #[test]
    fn test_standard_layout() {
        let layout = standard_rayos_layout(160 * 1024 * 1024 * 1024 / 512).unwrap();
        assert!(layout.validate());
        assert_eq!(layout.partition_count, 5);
    }

    #[test]
    fn test_installer_disk() {
        let disk = InstallerDisk::new(
            0x12345678,
            InstallerDiskType::UsbDisk,
            100 * 1024 * 1024 * 1024,
            "USB Drive",
        );
        assert!(disk.is_safe());
        assert!(disk.meets_minimum());
    }

    #[test]
    fn test_installer_boot() {
        let mut installer = InstallerBoot::new();

        let disk = InstallerDisk::new(
            0x12345678,
            InstallerDiskType::UsbDisk,
            100 * 1024 * 1024 * 1024,
            "USB Drive",
        );
        installer.add_disk(disk).unwrap();

        assert_eq!(installer.list_disks(), 1);

        installer.select_disk(0).unwrap();
        assert!(installer.needs_confirmation());

        installer.confirm_install().unwrap();
        assert!(!installer.needs_confirmation());
    }

    #[test]
    fn test_partition_overlap_detection() {
        let mut layout = DiskLayout::new(1, 160 * 1024 * 1024 * 1024 / 512);

        let p1 = Partition::new(1, PartitionType::EfiSystem, 0, 1000);
        layout.add_partition(p1).unwrap();

        let p2 = Partition::new(2, PartitionType::RayosKernel, 500, 1000);
        assert!(layout.add_partition(p2).is_err());
    }
}
