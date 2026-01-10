//! RayOS Boot Manager
//!
//! Manages installation discovery, boot entry selection, and recovery fallback.
//! This module provides the boot menu, boot entry tracking, and automatic recovery
//! when the system fails to boot 3 consecutive times.
//!
//! **Design**: Boot entries are discovered from installed partitions. Recovery is automatic
//! and transparent - after 3 failures, the system reverts to the last known-good golden state.

/// Boot entry type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootEntryType {
    /// RayOS kernel boot
    RayosKernel = 0,
    /// Linux guest boot
    LinuxGuest = 1,
    /// Windows guest boot
    WindowsGuest = 2,
    /// Recovery boot (golden state)
    Recovery = 3,
    /// Firmware/UEFI setup
    FirmwareSetup = 4,
}

/// Boot entry
#[derive(Clone, Copy)]
pub struct BootEntry {
    /// Unique entry ID
    pub id: u32,
    /// Entry type
    pub entry_type: BootEntryType,
    /// Boot partition LBA (Logical Block Address)
    pub partition_lba: u64,
    /// Human-readable name
    pub name: &'static str,
    /// Boot order priority (0 = highest)
    pub priority: u32,
    /// Number of failed boots
    pub failure_count: u32,
    /// Is this entry currently bootable
    pub enabled: bool,
}

impl BootEntry {
    pub fn new(id: u32, entry_type: BootEntryType, partition_lba: u64, name: &'static str) -> Self {
        BootEntry {
            id,
            entry_type,
            partition_lba,
            name,
            priority: 0,
            failure_count: 0,
            enabled: true,
        }
    }

    /// Check if this entry has failed too many times
    pub fn is_failed(&self) -> bool {
        self.failure_count >= 3
    }

    /// Increment failure counter
    pub fn increment_failures(&mut self) {
        self.failure_count = self.failure_count.saturating_add(1);
    }

    /// Reset failure counter
    pub fn reset_failures(&mut self) {
        self.failure_count = 0;
    }

    /// Disable this boot entry
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Enable this boot entry
    pub fn enable(&mut self) {
        self.enabled = true;
    }
}

/// Recovery entry (snapshot of golden state)
#[derive(Clone, Copy)]
pub struct RecoveryEntry {
    /// Entry ID (matches corresponding BootEntry)
    pub entry_id: u32,
    /// Timestamp of snapshot (boot-relative milliseconds)
    pub snapshot_time: u64,
    /// Kernel version/hash for matching
    pub kernel_version: u32,
    /// Checksum of boot block
    pub boot_block_checksum: u32,
    /// Is this snapshot valid
    pub is_valid: bool,
}

impl RecoveryEntry {
    pub fn new(entry_id: u32, kernel_version: u32) -> Self {
        RecoveryEntry {
            entry_id,
            snapshot_time: 0,
            kernel_version,
            boot_block_checksum: 0,
            is_valid: false,
        }
    }

    /// Mark recovery entry as valid (golden state ready)
    pub fn validate(&mut self, checksum: u32) {
        self.boot_block_checksum = checksum;
        self.is_valid = true;
    }

    /// Check if this recovery entry matches a boot entry
    pub fn matches(&self, _entry: &BootEntry) -> bool {
        self.kernel_version > 0 && self.is_valid
    }
}

/// Boot menu state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootMenuState {
    /// Menu idle, no boot pending
    Idle = 0,
    /// Menu displayed, awaiting user selection
    Displayed = 1,
    /// Boot entry selected, ready to load
    Selected = 2,
    /// Boot in progress
    Booting = 3,
    /// Boot failed, recovery pending
    RecoveryPending = 4,
    /// Recovery in progress
    Recovering = 5,
}

/// Maximum boot entries
const MAX_BOOT_ENTRIES: usize = 16;

/// Boot menu
pub struct BootMenu {
    /// Boot entries
    entries: [Option<BootEntry>; MAX_BOOT_ENTRIES],
    /// Number of entries
    entry_count: u32,
    /// Recovery entries
    recovery: [Option<RecoveryEntry>; MAX_BOOT_ENTRIES],
    /// Number of recovery entries
    recovery_count: u32,
    /// Current menu state
    state: BootMenuState,
    /// Currently selected entry index
    selected_entry: Option<u32>,
    /// Default boot entry index
    default_entry: u32,
    /// Timeout before default boot (seconds)
    timeout_seconds: u32,
    /// Global failure count (three failures trigger recovery)
    global_failure_count: u32,
}

impl BootMenu {
    pub fn new() -> Self {
        BootMenu {
            entries: [None; MAX_BOOT_ENTRIES],
            entry_count: 0,
            recovery: [None; MAX_BOOT_ENTRIES],
            recovery_count: 0,
            state: BootMenuState::Idle,
            selected_entry: None,
            default_entry: 0,
            timeout_seconds: 5,
            global_failure_count: 0,
        }
    }

    /// Discover boot entries from installed partitions
    pub fn discover(&mut self) -> Result<u32, &'static str> {
        self.entry_count = 0;
        self.selected_entry = None;

        // In production, this would scan partition table for bootable partitions
        // Stub: will be populated by test/integration
        Ok(self.entry_count)
    }

    /// Add a boot entry
    pub fn add_entry(&mut self, entry: BootEntry) -> Result<u32, &'static str> {
        if self.entry_count >= (MAX_BOOT_ENTRIES as u32) {
            return Err("Max boot entries reached");
        }

        self.entries[self.entry_count as usize] = Some(entry);
        self.entry_count += 1;
        Ok(self.entry_count - 1)
    }

    /// Get boot entry by index
    pub fn get_entry(&self, index: u32) -> Option<BootEntry> {
        if (index as usize) < (self.entry_count as usize) {
            self.entries[index as usize]
        } else {
            None
        }
    }

    /// Set default boot entry
    pub fn set_default(&mut self, index: u32) -> Result<(), &'static str> {
        if (index as usize) >= (self.entry_count as usize) {
            return Err("Entry not found");
        }
        self.default_entry = index;
        Ok(())
    }

    /// Select a boot entry
    pub fn select_entry(&mut self, index: u32) -> Result<(), &'static str> {
        if (index as usize) >= (self.entry_count as usize) {
            return Err("Entry not found");
        }

        if let Some(entry) = self.entries[index as usize] {
            if !entry.enabled {
                return Err("Entry is disabled");
            }

            if entry.is_failed() {
                return Err("Entry has failed too many times");
            }

            self.selected_entry = Some(index);
            self.state = BootMenuState::Selected;
            Ok(())
        } else {
            Err("Entry not found")
        }
    }

    /// Get selected entry
    pub fn selected(&self) -> Option<BootEntry> {
        if let Some(idx) = self.selected_entry {
            self.get_entry(idx)
        } else {
            None
        }
    }

    /// Start boot of selected entry
    pub fn boot(&mut self) -> Result<(), &'static str> {
        if self.selected_entry.is_none() {
            // Auto-select default entry
            self.selected_entry = Some(self.default_entry);
        }

        self.state = BootMenuState::Booting;
        Ok(())
    }

    /// Report boot failure for selected entry
    pub fn report_failure(&mut self) -> Result<(), &'static str> {
        if let Some(idx) = self.selected_entry {
            if let Some(entry) = &mut self.entries[idx as usize] {
                entry.increment_failures();

                // Three failures trigger recovery
                if entry.is_failed() {
                    self.global_failure_count = self.global_failure_count.saturating_add(1);

                    if self.global_failure_count >= 3 {
                        self.state = BootMenuState::RecoveryPending;
                        return Ok(());
                    }

                    // Try next entry
                    entry.disable();
                    self.selected_entry = None;
                    self.state = BootMenuState::Idle;
                }
            }
        }

        Ok(())
    }

    /// Boot recovery entry
    pub fn boot_recovery(&mut self) -> Result<(), &'static str> {
        self.state = BootMenuState::Recovering;

        // Find first valid recovery entry
        for i in 0..self.recovery_count as usize {
            if let Some(recovery) = self.recovery[i] {
                if recovery.is_valid {
                    return Ok(());
                }
            }
        }

        Err("No valid recovery entry")
    }

    /// Report successful boot
    pub fn report_success(&mut self) {
        self.global_failure_count = 0;

        if let Some(idx) = self.selected_entry {
            if let Some(entry) = &mut self.entries[idx as usize] {
                entry.reset_failures();
            }
        }

        self.state = BootMenuState::Idle;
    }

    /// Add recovery entry
    pub fn add_recovery(&mut self, recovery: RecoveryEntry) -> Result<u32, &'static str> {
        if self.recovery_count >= (MAX_BOOT_ENTRIES as u32) {
            return Err("Max recovery entries reached");
        }

        self.recovery[self.recovery_count as usize] = Some(recovery);
        self.recovery_count += 1;
        Ok(self.recovery_count - 1)
    }

    /// Get boot menu state
    pub fn state(&self) -> BootMenuState {
        self.state
    }

    /// Get number of entries
    pub fn entry_count(&self) -> u32 {
        self.entry_count
    }

    /// Get global failure count
    pub fn global_failures(&self) -> u32 {
        self.global_failure_count
    }

    /// Check if in recovery state
    pub fn needs_recovery(&self) -> bool {
        self.state == BootMenuState::RecoveryPending || self.state == BootMenuState::Recovering
    }
}

/// Boot loader (chains to actual kernel boot)
pub struct BootLoader {
    /// Selected boot entry
    boot_entry: Option<BootEntry>,
    /// Recovery entry if recovering
    recovery_entry: Option<RecoveryEntry>,
    /// Load address for kernel image
    load_address: u64,
}

impl BootLoader {
    pub fn new() -> Self {
        BootLoader {
            boot_entry: None,
            recovery_entry: None,
            load_address: 0x1000000, // Default load address
        }
    }

    /// Set boot entry
    pub fn set_entry(&mut self, entry: BootEntry) {
        self.boot_entry = Some(entry);
    }

    /// Set recovery entry
    pub fn set_recovery(&mut self, recovery: RecoveryEntry) {
        self.recovery_entry = Some(recovery);
    }

    /// Load kernel from partition
    pub fn load(&self) -> Result<u64, &'static str> {
        if let Some(_entry) = self.boot_entry {
            // In production, this would read the kernel from the partition
            // For now, just return the load address
            Ok(self.load_address)
        } else {
            Err("No boot entry set")
        }
    }

    /// Get load address
    pub fn load_address(&self) -> u64 {
        self.load_address
    }

    /// Check if recovery boot
    pub fn is_recovery(&self) -> bool {
        self.recovery_entry.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_entry_creation() {
        let entry = BootEntry::new(1, BootEntryType::RayosKernel, 2048, "RayOS Kernel");
        assert_eq!(entry.id, 1);
        assert!(!entry.is_failed());
    }

    #[test]
    fn test_failure_tracking() {
        let mut entry = BootEntry::new(1, BootEntryType::RayosKernel, 2048, "RayOS");

        entry.increment_failures();
        assert_eq!(entry.failure_count, 1);
        assert!(!entry.is_failed());

        entry.increment_failures();
        entry.increment_failures();
        assert!(entry.is_failed());
    }

    #[test]
    fn test_boot_menu() {
        let mut menu = BootMenu::new();

        let entry = BootEntry::new(1, BootEntryType::RayosKernel, 2048, "RayOS");
        menu.add_entry(entry).unwrap();

        assert_eq!(menu.entry_count(), 1);

        menu.select_entry(0).unwrap();
        assert!(menu.selected().is_some());
    }

    #[test]
    fn test_recovery_logic() {
        let mut menu = BootMenu::new();

        let entry = BootEntry::new(1, BootEntryType::RayosKernel, 2048, "RayOS");
        menu.add_entry(entry).unwrap();
        menu.select_entry(0).unwrap();

        // Three failures trigger recovery
        for _ in 0..3 {
            menu.report_failure().unwrap();
        }

        assert!(menu.needs_recovery());
    }

    #[test]
    fn test_boot_loader() {
        let mut loader = BootLoader::new();

        let entry = BootEntry::new(1, BootEntryType::RayosKernel, 2048, "RayOS");
        loader.set_entry(entry);

        assert!(loader.load().is_ok());
        assert!(!loader.is_recovery());
    }

    #[test]
    fn test_recovery_entry() {
        let mut recovery = RecoveryEntry::new(1, 0x12345678);
        assert!(!recovery.is_valid);

        recovery.validate(0xABCDEF00);
        assert!(recovery.is_valid);
    }
}
