//! RayOS App Store
//!
//! Provides app discovery, browsing, and installation functionality.
//! Integrates with the package format and loader for seamless app management.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Maximum number of apps in the catalog
pub const MAX_CATALOG_APPS: usize = 128;

/// Maximum number of featured apps
pub const MAX_FEATURED_APPS: usize = 8;

/// Maximum number of categories
pub const MAX_CATEGORIES: usize = 16;

/// Maximum app name length
pub const MAX_APP_NAME_LEN: usize = 64;

/// Maximum app description length
pub const MAX_APP_DESC_LEN: usize = 256;

/// Maximum author name length
pub const MAX_AUTHOR_LEN: usize = 64;

/// Maximum version string length
pub const MAX_VERSION_LEN: usize = 32;

// ===== App Categories =====

/// App category enumeration
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum AppCategory {
    /// All apps (no filter)
    All = 0,
    /// Productivity apps (documents, spreadsheets, notes)
    Productivity = 1,
    /// Utility apps (file managers, calculators, settings)
    Utilities = 2,
    /// Development tools (editors, compilers, debuggers)
    Development = 3,
    /// Games and entertainment
    Games = 4,
    /// Graphics and media
    Media = 5,
    /// Communication (chat, email, video)
    Communication = 6,
    /// System tools and administration
    System = 7,
    /// Education and learning
    Education = 8,
    /// Science and research
    Science = 9,
}

impl AppCategory {
    /// Get category name as bytes
    pub fn name(&self) -> &'static [u8] {
        match self {
            Self::All => b"All",
            Self::Productivity => b"Productivity",
            Self::Utilities => b"Utilities",
            Self::Development => b"Development",
            Self::Games => b"Games",
            Self::Media => b"Media",
            Self::Communication => b"Communication",
            Self::System => b"System",
            Self::Education => b"Education",
            Self::Science => b"Science",
        }
    }

    /// Get category from u8
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Productivity,
            2 => Self::Utilities,
            3 => Self::Development,
            4 => Self::Games,
            5 => Self::Media,
            6 => Self::Communication,
            7 => Self::System,
            8 => Self::Education,
            9 => Self::Science,
            _ => Self::All,
        }
    }
}

// ===== App Listing =====

/// An app listing in the store catalog
#[derive(Clone)]
pub struct AppListing {
    /// Whether this slot is active
    pub active: bool,
    /// Unique catalog ID
    pub catalog_id: u32,
    /// App identifier (e.g., "com.example.myapp")
    pub app_id: [u8; MAX_APP_NAME_LEN],
    pub app_id_len: usize,
    /// Display name
    pub name: [u8; MAX_APP_NAME_LEN],
    pub name_len: usize,
    /// Short description
    pub description: [u8; MAX_APP_DESC_LEN],
    pub description_len: usize,
    /// Author/publisher name
    pub author: [u8; MAX_AUTHOR_LEN],
    pub author_len: usize,
    /// Current version string
    pub version: [u8; MAX_VERSION_LEN],
    pub version_len: usize,
    /// Category
    pub category: AppCategory,
    /// Download size in bytes
    pub download_size: u32,
    /// Install size in bytes
    pub install_size: u32,
    /// Rating (0-50, representing 0.0-5.0 stars)
    pub rating: u8,
    /// Number of ratings
    pub rating_count: u32,
    /// Download count
    pub download_count: u32,
    /// Is this app featured
    pub featured: bool,
    /// Is this app installed locally
    pub installed: bool,
    /// Has update available
    pub update_available: bool,
    /// Required capabilities (permission flags)
    pub capabilities: u32,
    /// Minimum RayOS version required
    pub min_rayos_version: u32,
}

impl AppListing {
    /// Create an empty app listing
    pub const fn empty() -> Self {
        Self {
            active: false,
            catalog_id: 0,
            app_id: [0; MAX_APP_NAME_LEN],
            app_id_len: 0,
            name: [0; MAX_APP_NAME_LEN],
            name_len: 0,
            description: [0; MAX_APP_DESC_LEN],
            description_len: 0,
            author: [0; MAX_AUTHOR_LEN],
            author_len: 0,
            version: [0; MAX_VERSION_LEN],
            version_len: 0,
            category: AppCategory::All,
            download_size: 0,
            install_size: 0,
            rating: 0,
            rating_count: 0,
            download_count: 0,
            featured: false,
            installed: false,
            update_available: false,
            capabilities: 0,
            min_rayos_version: 0,
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

    /// Get description as bytes
    pub fn description(&self) -> &[u8] {
        &self.description[..self.description_len]
    }

    /// Get author as bytes
    pub fn author(&self) -> &[u8] {
        &self.author[..self.author_len]
    }

    /// Get version as bytes
    pub fn version(&self) -> &[u8] {
        &self.version[..self.version_len]
    }

    /// Set app ID
    pub fn set_app_id(&mut self, id: &[u8]) {
        let len = id.len().min(MAX_APP_NAME_LEN);
        self.app_id[..len].copy_from_slice(&id[..len]);
        self.app_id_len = len;
    }

    /// Set name
    pub fn set_name(&mut self, name: &[u8]) {
        let len = name.len().min(MAX_APP_NAME_LEN);
        self.name[..len].copy_from_slice(&name[..len]);
        self.name_len = len;
    }

    /// Set description
    pub fn set_description(&mut self, desc: &[u8]) {
        let len = desc.len().min(MAX_APP_DESC_LEN);
        self.description[..len].copy_from_slice(&desc[..len]);
        self.description_len = len;
    }

    /// Set author
    pub fn set_author(&mut self, author: &[u8]) {
        let len = author.len().min(MAX_AUTHOR_LEN);
        self.author[..len].copy_from_slice(&author[..len]);
        self.author_len = len;
    }

    /// Set version
    pub fn set_version(&mut self, ver: &[u8]) {
        let len = ver.len().min(MAX_VERSION_LEN);
        self.version[..len].copy_from_slice(&ver[..len]);
        self.version_len = len;
    }

    /// Get rating as float (0.0-5.0)
    pub fn rating_float(&self) -> f32 {
        self.rating as f32 / 10.0
    }

    /// Format download size as human readable
    pub fn format_size(&self, buf: &mut [u8]) -> usize {
        format_bytes(self.download_size as u64, buf)
    }
}

// ===== App Catalog =====

/// The app store catalog
pub struct AppCatalog {
    /// All app listings
    apps: [AppListing; MAX_CATALOG_APPS],
    /// Number of active apps
    app_count: usize,
    /// Featured app IDs
    featured_ids: [u32; MAX_FEATURED_APPS],
    /// Number of featured apps
    featured_count: usize,
    /// Last catalog sync timestamp
    last_sync: u64,
    /// Catalog version
    catalog_version: u32,
}

impl AppCatalog {
    /// Create a new empty catalog
    pub const fn new() -> Self {
        Self {
            apps: [const { AppListing::empty() }; MAX_CATALOG_APPS],
            app_count: 0,
            featured_ids: [0; MAX_FEATURED_APPS],
            featured_count: 0,
            last_sync: 0,
            catalog_version: 0,
        }
    }

    /// Initialize catalog with sample apps
    pub fn init_sample_catalog(&mut self) {
        self.app_count = 0;

        // Add sample apps
        self.add_sample_app(
            b"com.rayos.calculator",
            b"Calculator",
            b"A simple calculator with basic and scientific modes",
            b"RayOS Team",
            b"1.0.0",
            AppCategory::Utilities,
            45, 128, // rating, rating_count
            1024 * 50, // 50 KB
            true, // featured
        );

        self.add_sample_app(
            b"com.rayos.notepad",
            b"Notepad",
            b"Simple text editor for quick notes and documents",
            b"RayOS Team",
            b"1.2.0",
            AppCategory::Productivity,
            42, 256,
            1024 * 80,
            true,
        );

        self.add_sample_app(
            b"com.rayos.terminal",
            b"Terminal",
            b"Advanced terminal emulator with tabs and themes",
            b"RayOS Team",
            b"2.0.0",
            AppCategory::Development,
            48, 512,
            1024 * 200,
            true,
        );

        self.add_sample_app(
            b"com.rayos.filemanager",
            b"Files",
            b"Browse and manage your files with ease",
            b"RayOS Team",
            b"1.5.0",
            AppCategory::Utilities,
            44, 384,
            1024 * 150,
            true,
        );

        self.add_sample_app(
            b"com.rayos.settings",
            b"Settings",
            b"Configure your RayOS system preferences",
            b"RayOS Team",
            b"1.0.0",
            AppCategory::System,
            40, 192,
            1024 * 100,
            false,
        );

        self.add_sample_app(
            b"com.rayos.imageeditor",
            b"Image Editor",
            b"Edit photos and create graphics with powerful tools",
            b"RayOS Team",
            b"0.9.0",
            AppCategory::Media,
            38, 64,
            1024 * 500,
            false,
        );

        self.add_sample_app(
            b"com.rayos.musicplayer",
            b"Music",
            b"Play your favorite music with playlists and equalizer",
            b"RayOS Team",
            b"1.1.0",
            AppCategory::Media,
            43, 320,
            1024 * 180,
            false,
        );

        self.add_sample_app(
            b"com.rayos.codeeditor",
            b"Code Editor",
            b"Lightweight code editor with syntax highlighting",
            b"RayOS Team",
            b"1.3.0",
            AppCategory::Development,
            46, 640,
            1024 * 300,
            true,
        );

        self.add_sample_app(
            b"com.rayos.chat",
            b"Chat",
            b"Instant messaging with end-to-end encryption",
            b"RayOS Team",
            b"0.8.0",
            AppCategory::Communication,
            35, 96,
            1024 * 120,
            false,
        );

        self.add_sample_app(
            b"com.rayos.solitaire",
            b"Solitaire",
            b"Classic card game for relaxation",
            b"RayOS Games",
            b"1.0.0",
            AppCategory::Games,
            41, 448,
            1024 * 60,
            false,
        );

        self.add_sample_app(
            b"com.rayos.snake",
            b"Snake",
            b"Classic snake game - how long can you grow?",
            b"RayOS Games",
            b"1.0.0",
            AppCategory::Games,
            39, 256,
            1024 * 40,
            false,
        );

        self.add_sample_app(
            b"com.rayos.systemmonitor",
            b"System Monitor",
            b"Real-time CPU, memory, and process monitoring",
            b"RayOS Team",
            b"1.0.0",
            AppCategory::System,
            44, 288,
            1024 * 90,
            false,
        );

        // Update catalog version
        self.catalog_version = 1;
        self.last_sync = crate::TIMER_TICKS.load(Ordering::Relaxed);
    }

    /// Add a sample app to the catalog
    fn add_sample_app(
        &mut self,
        app_id: &[u8],
        name: &[u8],
        desc: &[u8],
        author: &[u8],
        version: &[u8],
        category: AppCategory,
        rating: u8,
        rating_count: u32,
        size: u32,
        featured: bool,
    ) {
        if self.app_count >= MAX_CATALOG_APPS {
            return;
        }

        let idx = self.app_count;
        let app = &mut self.apps[idx];

        app.active = true;
        app.catalog_id = (idx + 1) as u32;
        app.set_app_id(app_id);
        app.set_name(name);
        app.set_description(desc);
        app.set_author(author);
        app.set_version(version);
        app.category = category;
        app.rating = rating;
        app.rating_count = rating_count;
        app.download_size = size;
        app.install_size = size * 2; // Rough estimate
        app.download_count = rating_count * 10; // Fake download count
        app.featured = featured;
        app.installed = false;
        app.update_available = false;

        if featured && self.featured_count < MAX_FEATURED_APPS {
            self.featured_ids[self.featured_count] = app.catalog_id;
            self.featured_count += 1;
        }

        self.app_count += 1;
    }

    /// Get total app count
    pub fn count(&self) -> usize {
        self.app_count
    }

    /// Get app by index
    pub fn get(&self, index: usize) -> Option<&AppListing> {
        if index < self.app_count && self.apps[index].active {
            Some(&self.apps[index])
        } else {
            None
        }
    }

    /// Get app by catalog ID
    pub fn get_by_id(&self, catalog_id: u32) -> Option<&AppListing> {
        for i in 0..self.app_count {
            if self.apps[i].active && self.apps[i].catalog_id == catalog_id {
                return Some(&self.apps[i]);
            }
        }
        None
    }

    /// Get mutable app by catalog ID
    pub fn get_by_id_mut(&mut self, catalog_id: u32) -> Option<&mut AppListing> {
        for i in 0..self.app_count {
            if self.apps[i].active && self.apps[i].catalog_id == catalog_id {
                return Some(&mut self.apps[i]);
            }
        }
        None
    }

    /// Get featured apps
    pub fn get_featured(&self) -> impl Iterator<Item = &AppListing> {
        (0..self.featured_count)
            .filter_map(|i| self.get_by_id(self.featured_ids[i]))
    }

    /// Get apps by category
    pub fn get_by_category(&self, category: AppCategory) -> impl Iterator<Item = &AppListing> {
        self.apps[..self.app_count].iter().filter(move |app| {
            app.active && (category == AppCategory::All || app.category == category)
        })
    }

    /// Search apps by name (case-insensitive substring match)
    pub fn search<'a>(&'a self, query: &'a [u8]) -> impl Iterator<Item = &'a AppListing> + 'a {
        self.apps[..self.app_count].iter().filter(move |app| {
            if !app.active || query.is_empty() {
                return app.active;
            }
            // Simple substring search (case-insensitive)
            let name = app.name();
            if name.len() < query.len() {
                return false;
            }
            for i in 0..=(name.len() - query.len()) {
                let mut matches = true;
                for j in 0..query.len() {
                    if name[i + j].to_ascii_lowercase() != query[j].to_ascii_lowercase() {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    return true;
                }
            }
            false
        })
    }

    /// Mark app as installed
    pub fn mark_installed(&mut self, catalog_id: u32) {
        if let Some(app) = self.get_by_id_mut(catalog_id) {
            app.installed = true;
            app.download_count += 1;
        }
    }

    /// Mark app as uninstalled
    pub fn mark_uninstalled(&mut self, catalog_id: u32) {
        if let Some(app) = self.get_by_id_mut(catalog_id) {
            app.installed = false;
        }
    }

    /// Get catalog version
    pub fn version(&self) -> u32 {
        self.catalog_version
    }

    /// Get last sync timestamp
    pub fn last_sync(&self) -> u64 {
        self.last_sync
    }
}

// ===== Global Store State =====

/// Global app catalog
static mut APP_CATALOG: AppCatalog = AppCatalog::new();

/// Catalog lock
static CATALOG_LOCK: AtomicBool = AtomicBool::new(false);

/// Store initialized flag
static STORE_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Install in progress flag
static INSTALL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Current install progress (0-100)
static INSTALL_PROGRESS: AtomicU32 = AtomicU32::new(0);

/// Current installing app ID
static INSTALLING_APP_ID: AtomicU32 = AtomicU32::new(0);

fn lock_catalog() {
    while CATALOG_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

fn unlock_catalog() {
    CATALOG_LOCK.store(false, Ordering::Release);
}

// ===== Public Store API =====

/// Initialize the app store
pub fn init_store() {
    if STORE_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    lock_catalog();
    unsafe {
        APP_CATALOG.init_sample_catalog();
    }
    unlock_catalog();

    STORE_INITIALIZED.store(true, Ordering::Release);

    #[cfg(feature = "serial_debug")]
    crate::serial_write_str("RAYOS_STORE_INITIALIZED\n");
}

/// Check if store is initialized
pub fn is_initialized() -> bool {
    STORE_INITIALIZED.load(Ordering::Relaxed)
}

/// Get total app count
pub fn app_count() -> usize {
    lock_catalog();
    let count = unsafe { APP_CATALOG.count() };
    unlock_catalog();
    count
}

/// Get app by index
pub fn get_app(index: usize) -> Option<AppListing> {
    lock_catalog();
    let result = unsafe { APP_CATALOG.get(index).cloned() };
    unlock_catalog();
    result
}

/// Get app by catalog ID
pub fn get_app_by_id(catalog_id: u32) -> Option<AppListing> {
    lock_catalog();
    let result = unsafe { APP_CATALOG.get_by_id(catalog_id).cloned() };
    unlock_catalog();
    result
}

/// Get featured apps (returns up to MAX_FEATURED_APPS)
pub fn get_featured_apps() -> ([Option<AppListing>; MAX_FEATURED_APPS], usize) {
    let mut result: [Option<AppListing>; MAX_FEATURED_APPS] = [const { None }; MAX_FEATURED_APPS];
    let mut count = 0;

    lock_catalog();
    unsafe {
        for app in APP_CATALOG.get_featured() {
            if count < MAX_FEATURED_APPS {
                result[count] = Some(app.clone());
                count += 1;
            }
        }
    }
    unlock_catalog();

    (result, count)
}

/// Get apps by category
pub fn get_apps_by_category(category: AppCategory) -> ([Option<AppListing>; MAX_CATALOG_APPS], usize) {
    let mut result: [Option<AppListing>; MAX_CATALOG_APPS] = [const { None }; MAX_CATALOG_APPS];
    let mut count = 0;

    lock_catalog();
    unsafe {
        for app in APP_CATALOG.get_by_category(category) {
            if count < MAX_CATALOG_APPS {
                result[count] = Some(app.clone());
                count += 1;
            }
        }
    }
    unlock_catalog();

    (result, count)
}

/// Search apps by name
pub fn search_apps(query: &[u8]) -> ([Option<AppListing>; MAX_CATALOG_APPS], usize) {
    let mut result: [Option<AppListing>; MAX_CATALOG_APPS] = [const { None }; MAX_CATALOG_APPS];
    let mut count = 0;

    lock_catalog();
    unsafe {
        for app in APP_CATALOG.search(query) {
            if count < MAX_CATALOG_APPS {
                result[count] = Some(app.clone());
                count += 1;
            }
        }
    }
    unlock_catalog();

    (result, count)
}

/// Install an app from the store
pub fn install_app(catalog_id: u32) -> Result<(), StoreError> {
    // Check if already installing
    if INSTALL_IN_PROGRESS.load(Ordering::Relaxed) {
        return Err(StoreError::InstallInProgress);
    }

    // Get app info
    let app = match get_app_by_id(catalog_id) {
        Some(a) => a,
        None => return Err(StoreError::AppNotFound),
    };

    if app.installed {
        return Err(StoreError::AlreadyInstalled);
    }

    // Start install
    INSTALL_IN_PROGRESS.store(true, Ordering::Release);
    INSTALLING_APP_ID.store(catalog_id, Ordering::Relaxed);
    INSTALL_PROGRESS.store(0, Ordering::Relaxed);

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_STORE_INSTALL_START:");
        crate::serial_write_hex_u64(catalog_id as u64);
        crate::serial_write_str("\n");
    }

    // Simulate download progress
    for progress in [10, 25, 50, 75, 90, 100] {
        INSTALL_PROGRESS.store(progress, Ordering::Relaxed);
        // In real implementation, this would be async with actual download
    }

    // Mark as installed
    lock_catalog();
    unsafe {
        APP_CATALOG.mark_installed(catalog_id);
    }
    unlock_catalog();

    // Complete install
    INSTALL_IN_PROGRESS.store(false, Ordering::Release);
    INSTALLING_APP_ID.store(0, Ordering::Relaxed);

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_STORE_INSTALL_COMPLETE:");
        crate::serial_write_hex_u64(catalog_id as u64);
        crate::serial_write_str("\n");
    }

    Ok(())
}

/// Uninstall an app
pub fn uninstall_app(catalog_id: u32) -> Result<(), StoreError> {
    let app = match get_app_by_id(catalog_id) {
        Some(a) => a,
        None => return Err(StoreError::AppNotFound),
    };

    if !app.installed {
        return Err(StoreError::NotInstalled);
    }

    lock_catalog();
    unsafe {
        APP_CATALOG.mark_uninstalled(catalog_id);
    }
    unlock_catalog();

    #[cfg(feature = "serial_debug")]
    {
        crate::serial_write_str("RAYOS_STORE_UNINSTALL:");
        crate::serial_write_hex_u64(catalog_id as u64);
        crate::serial_write_str("\n");
    }

    Ok(())
}

/// Check for updates
pub fn check_updates() -> usize {
    // In a real implementation, this would check the remote catalog
    // For now, return 0 updates
    0
}

/// Get install progress (0-100)
pub fn install_progress() -> u32 {
    INSTALL_PROGRESS.load(Ordering::Relaxed)
}

/// Check if install is in progress
pub fn is_installing() -> bool {
    INSTALL_IN_PROGRESS.load(Ordering::Relaxed)
}

/// Get currently installing app ID
pub fn installing_app_id() -> u32 {
    INSTALLING_APP_ID.load(Ordering::Relaxed)
}

/// Get catalog version
pub fn catalog_version() -> u32 {
    lock_catalog();
    let ver = unsafe { APP_CATALOG.version() };
    unlock_catalog();
    ver
}

// ===== Store Errors =====

/// Store operation errors
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// App not found in catalog
    AppNotFound,
    /// App already installed
    AlreadyInstalled,
    /// App not installed
    NotInstalled,
    /// Install already in progress
    InstallInProgress,
    /// Download failed
    DownloadFailed,
    /// Verification failed
    VerificationFailed,
    /// Not enough space
    InsufficientSpace,
    /// Network error
    NetworkError,
    /// Permission denied
    PermissionDenied,
}

impl StoreError {
    /// Get error message
    pub fn message(&self) -> &'static str {
        match self {
            Self::AppNotFound => "App not found",
            Self::AlreadyInstalled => "App already installed",
            Self::NotInstalled => "App not installed",
            Self::InstallInProgress => "Install already in progress",
            Self::DownloadFailed => "Download failed",
            Self::VerificationFailed => "Verification failed",
            Self::InsufficientSpace => "Insufficient space",
            Self::NetworkError => "Network error",
            Self::PermissionDenied => "Permission denied",
        }
    }
}

// ===== Helper Functions =====

/// Format bytes as human-readable string
fn format_bytes(bytes: u64, buf: &mut [u8]) -> usize {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    let (value, suffix): (u64, &[u8]) = if bytes >= GB {
        (bytes / GB, b" GB")
    } else if bytes >= MB {
        (bytes / MB, b" MB")
    } else if bytes >= KB {
        (bytes / KB, b" KB")
    } else {
        (bytes, b" B")
    };

    let mut pos = format_u64(value, buf);
    for &b in suffix.iter() {
        if pos < buf.len() {
            buf[pos] = b;
            pos += 1;
        }
    }
    pos
}

/// Format u64 as decimal string
fn format_u64(mut n: u64, buf: &mut [u8]) -> usize {
    if n == 0 {
        if !buf.is_empty() {
            buf[0] = b'0';
        }
        return 1;
    }

    let mut temp = [0u8; 20];
    let mut i = 20;
    while n > 0 && i > 0 {
        i -= 1;
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    let len = 20 - i;
    let copy_len = len.min(buf.len());
    buf[..copy_len].copy_from_slice(&temp[i..i + copy_len]);
    copy_len
}
