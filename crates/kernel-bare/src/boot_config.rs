// RAYOS Phase 9B Task 1: Boot Configuration & Chainloading
// UEFI boot variable management, secure boot, and chainloading support
// File: crates/kernel-bare/src/boot_config.rs

use core::sync::atomic::{AtomicBool, Ordering};

// ============================================================================
// UEFI BOOT VARIABLES
// ============================================================================

/// UEFI boot variable names
pub mod efi_vars {
    pub const BOOT_CURRENT: &str = "BootCurrent";
    pub const BOOT_NEXT: &str = "BootNext";
    pub const BOOT_ORDER: &str = "BootOrder";
    pub const TIMEOUT: &str = "Timeout";
    pub const SECURE_BOOT: &str = "SecureBoot";
    pub const SETUP_MODE: &str = "SetupMode";
    pub const PK: &str = "PK";           // Platform Key
    pub const KEK: &str = "KEK";         // Key Exchange Key
    pub const DB: &str = "db";           // Signature Database
    pub const DBX: &str = "dbx";         // Forbidden Signatures
}

/// Boot option attribute flags
pub mod boot_attrs {
    pub const ACTIVE: u32 = 0x00000001;
    pub const FORCE_RECONNECT: u32 = 0x00000002;
    pub const HIDDEN: u32 = 0x00000008;
    pub const CATEGORY_BOOT: u32 = 0x00000000;
    pub const CATEGORY_APP: u32 = 0x00000100;
}

/// UEFI boot option structure
#[derive(Clone, Copy)]
pub struct UefiBootOption {
    /// Boot option number (Boot0000 through BootFFFF)
    pub number: u16,
    /// Attributes
    pub attributes: u32,
    /// Description (UTF-16LE, truncated to 64 chars)
    pub description: [u16; 64],
    pub description_len: usize,
    /// Device path (simplified: partition GUID + file path)
    pub partition_guid: [u8; 16],
    pub file_path: [u8; 128],
    pub file_path_len: usize,
    /// Optional data
    pub optional_data: [u8; 64],
    pub optional_data_len: usize,
}

impl UefiBootOption {
    pub const fn new(number: u16) -> Self {
        UefiBootOption {
            number,
            attributes: boot_attrs::ACTIVE,
            description: [0; 64],
            description_len: 0,
            partition_guid: [0; 16],
            file_path: [0; 128],
            file_path_len: 0,
            optional_data: [0; 64],
            optional_data_len: 0,
        }
    }

    /// Set description from ASCII string
    pub fn set_description(&mut self, desc: &[u8]) {
        let len = desc.len().min(63);
        for i in 0..len {
            self.description[i] = desc[i] as u16;
        }
        self.description_len = len;
    }

    /// Set file path
    pub fn set_file_path(&mut self, path: &[u8]) {
        let len = path.len().min(127);
        self.file_path[..len].copy_from_slice(&path[..len]);
        self.file_path_len = len;
    }

    /// Check if active
    pub fn is_active(&self) -> bool {
        (self.attributes & boot_attrs::ACTIVE) != 0
    }

    /// Check if hidden
    pub fn is_hidden(&self) -> bool {
        (self.attributes & boot_attrs::HIDDEN) != 0
    }

    /// Enable this boot option
    pub fn enable(&mut self) {
        self.attributes |= boot_attrs::ACTIVE;
    }

    /// Disable this boot option
    pub fn disable(&mut self) {
        self.attributes &= !boot_attrs::ACTIVE;
    }

    /// Hide from boot menu
    pub fn hide(&mut self) {
        self.attributes |= boot_attrs::HIDDEN;
    }

    /// Show in boot menu
    pub fn show(&mut self) {
        self.attributes &= !boot_attrs::HIDDEN;
    }
}

// ============================================================================
// BOOT ORDER MANAGEMENT
// ============================================================================

const MAX_BOOT_OPTIONS: usize = 32;

/// Boot order manager
pub struct BootOrder {
    /// Ordered list of boot option numbers
    pub order: [u16; MAX_BOOT_OPTIONS],
    /// Number of entries
    pub count: usize,
    /// Timeout in seconds
    pub timeout: u16,
    /// Default option index
    pub default_index: usize,
}

impl BootOrder {
    pub const fn new() -> Self {
        BootOrder {
            order: [0; MAX_BOOT_OPTIONS],
            count: 0,
            timeout: 5,
            default_index: 0,
        }
    }

    /// Add boot option to order
    pub fn add(&mut self, option_number: u16) -> Result<(), &'static str> {
        if self.count >= MAX_BOOT_OPTIONS {
            return Err("Boot order full");
        }

        // Check for duplicates
        for i in 0..self.count {
            if self.order[i] == option_number {
                return Err("Option already in boot order");
            }
        }

        self.order[self.count] = option_number;
        self.count += 1;
        Ok(())
    }

    /// Remove boot option from order
    pub fn remove(&mut self, option_number: u16) -> Result<(), &'static str> {
        for i in 0..self.count {
            if self.order[i] == option_number {
                // Shift remaining entries
                for j in i..(self.count - 1) {
                    self.order[j] = self.order[j + 1];
                }
                self.count -= 1;
                return Ok(());
            }
        }
        Err("Option not in boot order")
    }

    /// Move option to front (first priority)
    pub fn move_to_front(&mut self, option_number: u16) -> Result<(), &'static str> {
        let mut found_idx = None;
        for i in 0..self.count {
            if self.order[i] == option_number {
                found_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = found_idx {
            // Shift entries and place at front
            let opt = self.order[idx];
            for i in (1..=idx).rev() {
                self.order[i] = self.order[i - 1];
            }
            self.order[0] = opt;
            Ok(())
        } else {
            Err("Option not in boot order")
        }
    }

    /// Get first bootable option
    pub fn first(&self) -> Option<u16> {
        if self.count > 0 {
            Some(self.order[0])
        } else {
            None
        }
    }

    /// Set default option
    pub fn set_default(&mut self, option_number: u16) -> Result<(), &'static str> {
        for i in 0..self.count {
            if self.order[i] == option_number {
                self.default_index = i;
                return Ok(());
            }
        }
        Err("Option not in boot order")
    }
}

// ============================================================================
// SECURE BOOT
// ============================================================================

/// Secure boot state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecureBootState {
    /// Secure boot disabled
    Disabled = 0,
    /// Secure boot enabled, setup mode
    SetupMode = 1,
    /// Secure boot enabled, user mode (enforcing)
    UserMode = 2,
    /// Secure boot deployed (cannot return to setup)
    Deployed = 3,
}

/// Certificate type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertificateType {
    /// X.509 certificate (SHA-256)
    X509Sha256 = 0,
    /// Raw SHA-256 hash
    Sha256Hash = 1,
    /// X.509 certificate (SHA-384)
    X509Sha384 = 2,
    /// X.509 certificate (SHA-512)
    X509Sha512 = 3,
}

/// Secure boot certificate entry
#[derive(Clone, Copy)]
pub struct SecureBootCert {
    /// Certificate type
    pub cert_type: CertificateType,
    /// Owner GUID
    pub owner: [u8; 16],
    /// Certificate data (truncated for embedded use)
    pub data: [u8; 256],
    pub data_len: usize,
    /// Is this a revocation entry (for dbx)
    pub is_revocation: bool,
}

impl SecureBootCert {
    pub const fn empty() -> Self {
        SecureBootCert {
            cert_type: CertificateType::X509Sha256,
            owner: [0; 16],
            data: [0; 256],
            data_len: 0,
            is_revocation: false,
        }
    }

    pub fn new_hash(hash: &[u8; 32], owner: [u8; 16]) -> Self {
        let mut cert = SecureBootCert::empty();
        cert.cert_type = CertificateType::Sha256Hash;
        cert.owner = owner;
        cert.data[..32].copy_from_slice(hash);
        cert.data_len = 32;
        cert
    }
}

/// Secure boot configuration
pub struct SecureBootConfig {
    /// Current state
    pub state: SecureBootState,
    /// Platform Key (PK) - single entry
    pub pk: Option<SecureBootCert>,
    /// Key Exchange Keys (KEK)
    pub kek: [Option<SecureBootCert>; 4],
    pub kek_count: usize,
    /// Signature Database (db) - allowed signatures
    pub db: [Option<SecureBootCert>; 16],
    pub db_count: usize,
    /// Forbidden Database (dbx) - revoked signatures
    pub dbx: [Option<SecureBootCert>; 16],
    pub dbx_count: usize,
}

impl SecureBootConfig {
    pub const fn new() -> Self {
        const NONE_CERT: Option<SecureBootCert> = None;
        SecureBootConfig {
            state: SecureBootState::Disabled,
            pk: None,
            kek: [NONE_CERT; 4],
            kek_count: 0,
            db: [NONE_CERT; 16],
            db_count: 0,
            dbx: [NONE_CERT; 16],
            dbx_count: 0,
        }
    }

    /// Check if secure boot is enforcing
    pub fn is_enforcing(&self) -> bool {
        self.state == SecureBootState::UserMode || self.state == SecureBootState::Deployed
    }

    /// Add certificate to db
    pub fn add_to_db(&mut self, cert: SecureBootCert) -> Result<(), &'static str> {
        if self.db_count >= 16 {
            return Err("Signature database full");
        }
        self.db[self.db_count] = Some(cert);
        self.db_count += 1;
        Ok(())
    }

    /// Add hash to dbx (revoke)
    pub fn revoke_hash(&mut self, hash: &[u8; 32], owner: [u8; 16]) -> Result<(), &'static str> {
        if self.dbx_count >= 16 {
            return Err("Revocation database full");
        }
        let mut cert = SecureBootCert::new_hash(hash, owner);
        cert.is_revocation = true;
        self.dbx[self.dbx_count] = Some(cert);
        self.dbx_count += 1;
        Ok(())
    }

    /// Verify a binary hash against db/dbx
    pub fn verify_hash(&self, hash: &[u8; 32]) -> bool {
        // Check dbx first (revocation takes precedence)
        for i in 0..self.dbx_count {
            if let Some(cert) = &self.dbx[i] {
                if cert.cert_type == CertificateType::Sha256Hash {
                    if &cert.data[..32] == hash {
                        return false;  // Revoked
                    }
                }
            }
        }

        // Check db for allowed signatures
        for i in 0..self.db_count {
            if let Some(cert) = &self.db[i] {
                if cert.cert_type == CertificateType::Sha256Hash {
                    if &cert.data[..32] == hash {
                        return true;  // Allowed
                    }
                }
            }
        }

        // Not found in db
        !self.is_enforcing()  // Allow if not enforcing
    }
}

// ============================================================================
// CHAINLOADING
// ============================================================================

/// Chainload target type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainloadTarget {
    /// EFI application
    EfiApp = 0,
    /// Legacy BIOS (via CSM)
    LegacyBios = 1,
    /// Another EFI boot loader
    EfiBootloader = 2,
    /// Windows Boot Manager
    WindowsBoot = 3,
    /// Linux boot loader
    LinuxBoot = 4,
}

/// Chainload descriptor
#[derive(Clone, Copy)]
pub struct ChainloadDescriptor {
    /// Target type
    pub target: ChainloadTarget,
    /// Partition GUID
    pub partition_guid: [u8; 16],
    /// File path (relative to partition root)
    pub path: [u8; 128],
    pub path_len: usize,
    /// Load address (0 = default)
    pub load_address: u64,
    /// Pass through boot arguments
    pub pass_args: bool,
}

impl ChainloadDescriptor {
    pub const fn new(target: ChainloadTarget) -> Self {
        ChainloadDescriptor {
            target,
            partition_guid: [0; 16],
            path: [0; 128],
            path_len: 0,
            load_address: 0,
            pass_args: false,
        }
    }

    /// Set file path
    pub fn set_path(&mut self, path: &[u8]) {
        let len = path.len().min(127);
        self.path[..len].copy_from_slice(&path[..len]);
        self.path_len = len;
    }

    /// Create Windows Boot Manager chainload
    pub fn windows() -> Self {
        let mut desc = ChainloadDescriptor::new(ChainloadTarget::WindowsBoot);
        desc.set_path(b"\\EFI\\Microsoft\\Boot\\bootmgfw.efi");
        desc
    }

    /// Create Linux (GRUB) chainload
    pub fn linux_grub() -> Self {
        let mut desc = ChainloadDescriptor::new(ChainloadTarget::LinuxBoot);
        desc.set_path(b"\\EFI\\ubuntu\\grubx64.efi");
        desc
    }

    /// Create systemd-boot chainload
    pub fn systemd_boot() -> Self {
        let mut desc = ChainloadDescriptor::new(ChainloadTarget::LinuxBoot);
        desc.set_path(b"\\EFI\\systemd\\systemd-bootx64.efi");
        desc
    }
}

/// Execute chainload
pub fn execute_chainload(desc: &ChainloadDescriptor) -> Result<(), &'static str> {
    match desc.target {
        ChainloadTarget::EfiApp | ChainloadTarget::EfiBootloader => {
            // Use UEFI LoadImage/StartImage
            Ok(())
        }
        ChainloadTarget::WindowsBoot => {
            // Load Windows Boot Manager
            Ok(())
        }
        ChainloadTarget::LinuxBoot => {
            // Load Linux boot loader
            Ok(())
        }
        ChainloadTarget::LegacyBios => {
            // Requires CSM (Compatibility Support Module)
            Err("Legacy BIOS boot requires CSM")
        }
    }
}

// ============================================================================
// BOOT MENU DISPLAY
// ============================================================================

/// Boot menu entry for display
#[derive(Clone, Copy)]
pub struct BootMenuItem {
    /// Option number
    pub option_number: u16,
    /// Display name
    pub name: [u8; 64],
    pub name_len: usize,
    /// Is currently selected
    pub selected: bool,
    /// Is default entry
    pub is_default: bool,
    /// Entry type icon
    pub icon: BootMenuIcon,
}

/// Boot menu icons
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootMenuIcon {
    RayOS = 0,
    Linux = 1,
    Windows = 2,
    Recovery = 3,
    Settings = 4,
    Other = 5,
}

impl BootMenuItem {
    pub const fn new(option_number: u16) -> Self {
        BootMenuItem {
            option_number,
            name: [0; 64],
            name_len: 0,
            selected: false,
            is_default: false,
            icon: BootMenuIcon::Other,
        }
    }

    pub fn set_name(&mut self, name: &[u8]) {
        let len = name.len().min(63);
        self.name[..len].copy_from_slice(&name[..len]);
        self.name_len = len;
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("Unknown")
    }
}

/// Boot menu state
pub struct BootMenuDisplay {
    /// Menu items
    pub items: [Option<BootMenuItem>; 16],
    pub item_count: usize,
    /// Selected index
    pub selected_index: usize,
    /// Timeout remaining (seconds)
    pub timeout_remaining: u32,
    /// Show timeout countdown
    pub show_timeout: bool,
    /// Menu title
    pub title: &'static str,
}

impl BootMenuDisplay {
    pub const fn new() -> Self {
        const NONE_ITEM: Option<BootMenuItem> = None;
        BootMenuDisplay {
            items: [NONE_ITEM; 16],
            item_count: 0,
            selected_index: 0,
            timeout_remaining: 5,
            show_timeout: true,
            title: "RayOS Boot Menu",
        }
    }

    /// Add menu item
    pub fn add_item(&mut self, item: BootMenuItem) -> Result<(), &'static str> {
        if self.item_count >= 16 {
            return Err("Menu full");
        }
        self.items[self.item_count] = Some(item);
        self.item_count += 1;
        Ok(())
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
        self.cancel_timeout();
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.item_count {
            self.selected_index += 1;
        }
        self.cancel_timeout();
    }

    /// Cancel timeout (user interacted)
    pub fn cancel_timeout(&mut self) {
        self.show_timeout = false;
    }

    /// Tick timeout (call every second)
    pub fn tick(&mut self) -> bool {
        if self.show_timeout && self.timeout_remaining > 0 {
            self.timeout_remaining -= 1;
            if self.timeout_remaining == 0 {
                return true;  // Timeout expired, auto-boot
            }
        }
        false
    }

    /// Get selected item
    pub fn selected(&self) -> Option<&BootMenuItem> {
        self.items[self.selected_index].as_ref()
    }
}

// ============================================================================
// GLOBAL STATE
// ============================================================================

static mut BOOT_CONFIG: Option<BootConfigState> = None;
static BOOT_CONFIG_INIT: AtomicBool = AtomicBool::new(false);

pub struct BootConfigState {
    pub boot_order: BootOrder,
    pub secure_boot: SecureBootConfig,
    pub menu: BootMenuDisplay,
}

impl BootConfigState {
    pub const fn new() -> Self {
        BootConfigState {
            boot_order: BootOrder::new(),
            secure_boot: SecureBootConfig::new(),
            menu: BootMenuDisplay::new(),
        }
    }
}

pub fn init_boot_config() {
    if !BOOT_CONFIG_INIT.swap(true, Ordering::SeqCst) {
        unsafe {
            BOOT_CONFIG = Some(BootConfigState::new());
        }
    }
}

pub fn get_boot_config() -> Option<&'static mut BootConfigState> {
    unsafe { BOOT_CONFIG.as_mut() }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_option() {
        let mut opt = UefiBootOption::new(0x0001);
        opt.set_description(b"RayOS");
        opt.set_file_path(b"\\EFI\\RayOS\\rayos.efi");
        assert!(opt.is_active());
        assert!(!opt.is_hidden());
    }

    #[test]
    fn test_boot_order() {
        let mut order = BootOrder::new();
        order.add(0x0001).unwrap();
        order.add(0x0002).unwrap();
        order.add(0x0003).unwrap();

        assert_eq!(order.first(), Some(0x0001));

        order.move_to_front(0x0003).unwrap();
        assert_eq!(order.first(), Some(0x0003));
    }

    #[test]
    fn test_secure_boot() {
        let mut config = SecureBootConfig::new();
        assert!(!config.is_enforcing());

        config.state = SecureBootState::UserMode;
        assert!(config.is_enforcing());

        let hash = [0xABu8; 32];
        let owner = [0; 16];
        config.add_to_db(SecureBootCert::new_hash(&hash, owner)).unwrap();
        assert!(config.verify_hash(&hash));

        let bad_hash = [0xCDu8; 32];
        assert!(!config.verify_hash(&bad_hash));
    }

    #[test]
    fn test_secure_boot_revocation() {
        let mut config = SecureBootConfig::new();
        config.state = SecureBootState::UserMode;

        let hash = [0xABu8; 32];
        let owner = [0; 16];

        // Add to db
        config.add_to_db(SecureBootCert::new_hash(&hash, owner)).unwrap();
        assert!(config.verify_hash(&hash));

        // Revoke it
        config.revoke_hash(&hash, owner).unwrap();
        assert!(!config.verify_hash(&hash));  // Now rejected
    }

    #[test]
    fn test_chainload_descriptors() {
        let win = ChainloadDescriptor::windows();
        assert_eq!(win.target, ChainloadTarget::WindowsBoot);

        let linux = ChainloadDescriptor::linux_grub();
        assert_eq!(linux.target, ChainloadTarget::LinuxBoot);
    }

    #[test]
    fn test_boot_menu() {
        let mut menu = BootMenuDisplay::new();

        let mut item = BootMenuItem::new(0x0001);
        item.set_name(b"RayOS");
        item.icon = BootMenuIcon::RayOS;
        menu.add_item(item).unwrap();

        assert_eq!(menu.item_count, 1);
        assert_eq!(menu.selected().unwrap().name_str(), "RayOS");
    }

    #[test]
    fn test_menu_timeout() {
        let mut menu = BootMenuDisplay::new();
        menu.timeout_remaining = 3;

        assert!(!menu.tick());  // 2 left
        assert!(!menu.tick());  // 1 left
        assert!(menu.tick());   // 0, timeout expired
    }

    #[test]
    fn test_menu_navigation() {
        let mut menu = BootMenuDisplay::new();

        for i in 0..3 {
            let mut item = BootMenuItem::new(i);
            item.set_name(b"Entry");
            menu.add_item(item).unwrap();
        }

        assert_eq!(menu.selected_index, 0);
        menu.move_down();
        assert_eq!(menu.selected_index, 1);
        menu.move_down();
        assert_eq!(menu.selected_index, 2);
        menu.move_down();  // At end, should stay
        assert_eq!(menu.selected_index, 2);
        menu.move_up();
        assert_eq!(menu.selected_index, 1);
    }
}
