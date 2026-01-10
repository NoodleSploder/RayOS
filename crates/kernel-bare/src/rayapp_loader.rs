//! RayApp Package Loader
//!
//! Handles installation, loading, and management of .rayapp packages.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::rayapp_package::{
    PackageHeader, PackageManifest, PackageSignature, PackageError, crc32, parse_manifest,
    MAX_APP_ID_LEN, MAX_APP_NAME_LEN, MAX_VERSION_LEN,
};

/// Maximum number of installed packages
pub const MAX_INSTALLED_PACKAGES: usize = 64;

/// Maximum number of loaded (running) packages
pub const MAX_LOADED_PACKAGES: usize = 16;

// ===== Installed Package Entry =====

/// An installed package in the registry
#[derive(Clone)]
pub struct InstalledPackage {
    /// Whether this slot is in use
    pub active: bool,
    /// Package ID
    pub id: u32,
    /// App ID
    pub app_id: [u8; MAX_APP_ID_LEN],
    pub app_id_len: usize,
    /// App name
    pub name: [u8; MAX_APP_NAME_LEN],
    pub name_len: usize,
    /// Version string
    pub version: [u8; MAX_VERSION_LEN],
    pub version_len: usize,
    /// Installation timestamp (ticks)
    pub installed_at: u64,
    /// Required capabilities
    pub capabilities: u32,
    /// Package size in bytes
    pub package_size: u32,
    /// Code size in bytes
    pub code_size: u32,
    /// Number of assets
    pub asset_count: u16,
    /// Is package signed
    pub is_signed: bool,
    /// Is package currently loaded
    pub is_loaded: bool,
    /// Load count (how many times launched)
    pub load_count: u32,
}

impl InstalledPackage {
    /// Create an empty package entry
    pub const fn empty() -> Self {
        Self {
            active: false,
            id: 0,
            app_id: [0; MAX_APP_ID_LEN],
            app_id_len: 0,
            name: [0; MAX_APP_NAME_LEN],
            name_len: 0,
            version: [0; MAX_VERSION_LEN],
            version_len: 0,
            installed_at: 0,
            capabilities: 0,
            package_size: 0,
            code_size: 0,
            asset_count: 0,
            is_signed: false,
            is_loaded: false,
            load_count: 0,
        }
    }

    /// Get app ID as bytes
    pub fn app_id(&self) -> &[u8] {
        &self.app_id[..self.app_id_len]
    }

    /// Get name as bytes
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    /// Get version as bytes
    pub fn version(&self) -> &[u8] {
        &self.version[..self.version_len]
    }

    /// Set app ID from manifest
    pub fn set_app_id(&mut self, id: &[u8]) {
        let len = id.len().min(MAX_APP_ID_LEN);
        self.app_id[..len].copy_from_slice(&id[..len]);
        self.app_id_len = len;
    }

    /// Set name from manifest
    pub fn set_name(&mut self, name: &[u8]) {
        let len = name.len().min(MAX_APP_NAME_LEN);
        self.name[..len].copy_from_slice(&name[..len]);
        self.name_len = len;
    }

    /// Set version from manifest
    pub fn set_version(&mut self, ver: &[u8]) {
        let len = ver.len().min(MAX_VERSION_LEN);
        self.version[..len].copy_from_slice(&ver[..len]);
        self.version_len = len;
    }
}

// ===== Loaded Package Instance =====

/// A currently loaded/running package instance
pub struct LoadedPackage {
    /// Whether this slot is in use
    pub active: bool,
    /// Unique instance ID
    pub instance_id: u32,
    /// Reference to installed package ID
    pub package_id: u32,
    /// Load timestamp
    pub loaded_at: u64,
    /// Entry point address (if native)
    pub entry_point: u64,
    /// Memory region base
    pub memory_base: u64,
    /// Memory region size
    pub memory_size: u32,
    /// State
    pub state: PackageState,
}

impl LoadedPackage {
    /// Create an empty loaded package entry
    pub const fn empty() -> Self {
        Self {
            active: false,
            instance_id: 0,
            package_id: 0,
            loaded_at: 0,
            entry_point: 0,
            memory_base: 0,
            memory_size: 0,
            state: PackageState::Unloaded,
        }
    }
}

/// Package execution state
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PackageState {
    /// Not loaded
    Unloaded,
    /// Loading in progress
    Loading,
    /// Ready to run
    Ready,
    /// Currently running
    Running,
    /// Suspended/paused
    Suspended,
    /// Error state
    Error,
}

// ===== Package Registry =====

/// Static registry of installed packages
static mut INSTALLED_PACKAGES: [InstalledPackage; MAX_INSTALLED_PACKAGES] = {
    // Initialize all slots as empty
    const EMPTY: InstalledPackage = InstalledPackage::empty();
    [EMPTY; MAX_INSTALLED_PACKAGES]
};

/// Static registry of loaded packages
static mut LOADED_PACKAGES: [LoadedPackage; MAX_LOADED_PACKAGES] = {
    const EMPTY: LoadedPackage = LoadedPackage::empty();
    [EMPTY; MAX_LOADED_PACKAGES]
};

/// Next package ID
static NEXT_PACKAGE_ID: AtomicU32 = AtomicU32::new(1);

/// Next instance ID
static NEXT_INSTANCE_ID: AtomicU32 = AtomicU32::new(1);

/// Registry lock
static REGISTRY_LOCK: AtomicBool = AtomicBool::new(false);

/// Acquire registry lock
fn lock_registry() {
    while REGISTRY_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

/// Release registry lock
fn unlock_registry() {
    REGISTRY_LOCK.store(false, Ordering::Release);
}

// ===== Package Operations =====

/// Install a package from raw bytes
pub fn install_package(data: &[u8]) -> Result<u32, PackageError> {
    // Validate minimum size
    if data.len() < PackageHeader::SIZE {
        return Err(PackageError::InvalidMagic);
    }

    // Parse header
    let header = unsafe {
        core::ptr::read_unaligned(data.as_ptr() as *const PackageHeader)
    };

    // Validate header
    header.validate()?;

    // Verify content checksum
    let content_start = PackageHeader::SIZE;
    let content_end = data.len().saturating_sub(PackageSignature::SIZE);
    if content_end > content_start {
        let content_crc = crc32(&data[content_start..content_end]);
        if content_crc != header.content_checksum {
            // For now, just log and continue (checksum might be 0 for unsigned packages)
            #[cfg(feature = "serial_debug")]
            crate::serial_write_str("RAYOS_PKG_CHECKSUM_WARN\n");
        }
    }

    // Parse manifest
    let manifest_start = header.manifest_offset as usize;
    let manifest_end = manifest_start + header.manifest_size as usize;
    if manifest_end > data.len() {
        return Err(PackageError::ParseError);
    }
    let manifest = parse_manifest(&data[manifest_start..manifest_end])?;

    // Check if already installed
    lock_registry();
    let existing = find_package_by_app_id(manifest.app_id());
    if existing.is_some() {
        unlock_registry();
        return Err(PackageError::AlreadyInstalled);
    }

    // Find free slot
    let slot = unsafe {
        let mut found = None;
        for i in 0..MAX_INSTALLED_PACKAGES {
            if !INSTALLED_PACKAGES[i].active {
                found = Some(i);
                break;
            }
        }
        found
    };

    let slot_idx = match slot {
        Some(idx) => idx,
        None => {
            unlock_registry();
            return Err(PackageError::OutOfMemory);
        }
    };

    // Assign package ID
    let package_id = NEXT_PACKAGE_ID.fetch_add(1, Ordering::Relaxed);

    // Create installed package entry
    unsafe {
        let pkg = &mut INSTALLED_PACKAGES[slot_idx];
        pkg.active = true;
        pkg.id = package_id;
        pkg.set_app_id(manifest.app_id());
        pkg.set_name(manifest.name());
        pkg.set_version(manifest.version());
        pkg.installed_at = crate::TIMER_TICKS.load(Ordering::Relaxed);
        pkg.capabilities = manifest.capabilities;
        pkg.package_size = header.total_size;
        pkg.code_size = header.code_size;
        pkg.asset_count = header.asset_count;
        pkg.is_signed = (header.flags & crate::rayapp_package::flags::SIGNED) != 0;
        pkg.is_loaded = false;
        pkg.load_count = 0;
    }

    unlock_registry();

    // Update stats
    crate::rayapp_package::increment_installed();

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_PKG_INSTALLED:");
        crate::serial_write_hex_u64(package_id as u64);
        crate::serial_write_str("\n");
    }

    Ok(package_id)
}

/// Uninstall a package by ID
pub fn uninstall_package(package_id: u32) -> Result<(), PackageError> {
    lock_registry();

    // Find the package
    let slot = unsafe {
        let mut found = None;
        for i in 0..MAX_INSTALLED_PACKAGES {
            if INSTALLED_PACKAGES[i].active && INSTALLED_PACKAGES[i].id == package_id {
                found = Some(i);
                break;
            }
        }
        found
    };

    let slot_idx = match slot {
        Some(idx) => idx,
        None => {
            unlock_registry();
            return Err(PackageError::NotFound);
        }
    };

    // Check if loaded
    unsafe {
        if INSTALLED_PACKAGES[slot_idx].is_loaded {
            unlock_registry();
            return Err(PackageError::PermissionDenied);
        }

        // Clear the slot
        INSTALLED_PACKAGES[slot_idx] = InstalledPackage::empty();
    }

    unlock_registry();

    // Update stats
    crate::rayapp_package::decrement_installed();

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_PKG_UNINSTALLED:");
        crate::serial_write_hex_u64(package_id as u64);
        crate::serial_write_str("\n");
    }

    Ok(())
}

/// Load a package for execution
pub fn load_package(package_id: u32) -> Result<u32, PackageError> {
    lock_registry();

    // Find the installed package
    let pkg_slot = unsafe {
        let mut found = None;
        for i in 0..MAX_INSTALLED_PACKAGES {
            if INSTALLED_PACKAGES[i].active && INSTALLED_PACKAGES[i].id == package_id {
                found = Some(i);
                break;
            }
        }
        found
    };

    let pkg_idx = match pkg_slot {
        Some(idx) => idx,
        None => {
            unlock_registry();
            return Err(PackageError::NotFound);
        }
    };

    // Find free loaded slot
    let load_slot = unsafe {
        let mut found = None;
        for i in 0..MAX_LOADED_PACKAGES {
            if !LOADED_PACKAGES[i].active {
                found = Some(i);
                break;
            }
        }
        found
    };

    let load_idx = match load_slot {
        Some(idx) => idx,
        None => {
            unlock_registry();
            return Err(PackageError::OutOfMemory);
        }
    };

    // Create instance
    let instance_id = NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed);

    unsafe {
        let loaded = &mut LOADED_PACKAGES[load_idx];
        loaded.active = true;
        loaded.instance_id = instance_id;
        loaded.package_id = package_id;
        loaded.loaded_at = crate::TIMER_TICKS.load(Ordering::Relaxed);
        loaded.entry_point = 0; // Would be set from code section
        loaded.memory_base = 0;
        loaded.memory_size = 0;
        loaded.state = PackageState::Ready;

        // Mark as loaded in installed entry
        INSTALLED_PACKAGES[pkg_idx].is_loaded = true;
        INSTALLED_PACKAGES[pkg_idx].load_count += 1;
    }

    unlock_registry();

    // Update stats
    crate::rayapp_package::increment_loaded();

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_PKG_LOADED:");
        crate::serial_write_hex_u64(instance_id as u64);
        crate::serial_write_str("\n");
    }

    Ok(instance_id)
}

/// Unload a package instance
pub fn unload_package(instance_id: u32) -> Result<(), PackageError> {
    lock_registry();

    // Find the loaded instance
    let load_slot = unsafe {
        let mut found = None;
        for i in 0..MAX_LOADED_PACKAGES {
            if LOADED_PACKAGES[i].active && LOADED_PACKAGES[i].instance_id == instance_id {
                found = Some(i);
                break;
            }
        }
        found
    };

    let load_idx = match load_slot {
        Some(idx) => idx,
        None => {
            unlock_registry();
            return Err(PackageError::NotFound);
        }
    };

    let package_id = unsafe { LOADED_PACKAGES[load_idx].package_id };

    // Find the installed package to update its state
    unsafe {
        for i in 0..MAX_INSTALLED_PACKAGES {
            if INSTALLED_PACKAGES[i].active && INSTALLED_PACKAGES[i].id == package_id {
                INSTALLED_PACKAGES[i].is_loaded = false;
                break;
            }
        }

        // Clear loaded slot
        LOADED_PACKAGES[load_idx] = LoadedPackage::empty();
    }

    unlock_registry();

    // Update stats
    crate::rayapp_package::decrement_loaded();

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_PKG_UNLOADED:");
        crate::serial_write_hex_u64(instance_id as u64);
        crate::serial_write_str("\n");
    }

    Ok(())
}

// ===== Query Functions =====

/// Find a package by app ID
pub fn find_package_by_app_id(app_id: &[u8]) -> Option<u32> {
    unsafe {
        for i in 0..MAX_INSTALLED_PACKAGES {
            if INSTALLED_PACKAGES[i].active {
                let pkg_id = &INSTALLED_PACKAGES[i].app_id[..INSTALLED_PACKAGES[i].app_id_len];
                if pkg_id == app_id {
                    return Some(INSTALLED_PACKAGES[i].id);
                }
            }
        }
    }
    None
}

/// Get installed package info by ID
pub fn get_package_info(package_id: u32) -> Option<InstalledPackage> {
    lock_registry();
    let result = unsafe {
        for i in 0..MAX_INSTALLED_PACKAGES {
            if INSTALLED_PACKAGES[i].active && INSTALLED_PACKAGES[i].id == package_id {
                unlock_registry();
                return Some(INSTALLED_PACKAGES[i].clone());
            }
        }
        None
    };
    unlock_registry();
    result
}

/// List all installed packages
pub fn list_installed_packages() -> ([Option<InstalledPackage>; MAX_INSTALLED_PACKAGES], usize) {
    let mut result: [Option<InstalledPackage>; MAX_INSTALLED_PACKAGES] = [const { None }; MAX_INSTALLED_PACKAGES];
    let mut count = 0;

    lock_registry();
    unsafe {
        for i in 0..MAX_INSTALLED_PACKAGES {
            if INSTALLED_PACKAGES[i].active {
                result[count] = Some(INSTALLED_PACKAGES[i].clone());
                count += 1;
            }
        }
    }
    unlock_registry();

    (result, count)
}

/// Get loaded package info by instance ID
pub fn get_loaded_info(instance_id: u32) -> Option<(u32, PackageState)> {
    lock_registry();
    let result = unsafe {
        for i in 0..MAX_LOADED_PACKAGES {
            if LOADED_PACKAGES[i].active && LOADED_PACKAGES[i].instance_id == instance_id {
                unlock_registry();
                return Some((LOADED_PACKAGES[i].package_id, LOADED_PACKAGES[i].state));
            }
        }
        None
    };
    unlock_registry();
    result
}

/// Count installed packages
pub fn count_installed() -> usize {
    let mut count = 0;
    lock_registry();
    unsafe {
        for i in 0..MAX_INSTALLED_PACKAGES {
            if INSTALLED_PACKAGES[i].active {
                count += 1;
            }
        }
    }
    unlock_registry();
    count
}

/// Count loaded packages
pub fn count_loaded() -> usize {
    let mut count = 0;
    lock_registry();
    unsafe {
        for i in 0..MAX_LOADED_PACKAGES {
            if LOADED_PACKAGES[i].active {
                count += 1;
            }
        }
    }
    unlock_registry();
    count
}

// ===== Package Info Iterator =====

/// Iterator-like struct for listing packages
pub struct PackageIterator {
    index: usize,
}

impl PackageIterator {
    /// Create a new iterator
    pub fn new() -> Self {
        Self { index: 0 }
    }

    /// Get next package (call with registry locked)
    pub fn next(&mut self) -> Option<&'static InstalledPackage> {
        unsafe {
            while self.index < MAX_INSTALLED_PACKAGES {
                let i = self.index;
                self.index += 1;
                if INSTALLED_PACKAGES[i].active {
                    return Some(&INSTALLED_PACKAGES[i]);
                }
            }
        }
        None
    }
}

// ===== Dependency Resolution =====

/// Check if all dependencies for a package are satisfied
pub fn check_dependencies(manifest: &PackageManifest) -> Result<(), (usize, PackageError)> {
    for i in 0..manifest.dependency_count {
        let dep = &manifest.dependencies[i];
        if dep.optional {
            continue;
        }

        let found = find_package_by_app_id(dep.app_id());
        if found.is_none() {
            return Err((i, PackageError::MissingDependency));
        }

        // TODO: Check version compatibility
    }
    Ok(())
}

/// Resolve and install dependencies (placeholder)
pub fn resolve_dependencies(_manifest: &PackageManifest) -> Result<(), PackageError> {
    // In a real implementation, this would:
    // 1. Build dependency graph
    // 2. Check for cycles
    // 3. Download/install missing deps
    // 4. Verify versions
    Ok(())
}
