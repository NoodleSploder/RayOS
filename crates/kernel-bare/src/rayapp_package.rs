//! RayApp Package Format (.rayapp)
//!
//! Defines the binary package format for distributing RayOS applications.
//!
//! # Package Structure
//!
//! ```text
//! +-------------------+
//! | Header (64 bytes) |
//! +-------------------+
//! | Manifest (JSON)   |
//! +-------------------+
//! | Code Section      |
//! +-------------------+
//! | Assets Section    |
//! +-------------------+
//! | Signature (256b)  |
//! +-------------------+
//! ```
//!
//! # Magic Number
//!
//! All .rayapp files start with the magic bytes: `RAYAPP01`

use core::sync::atomic::{AtomicU32, Ordering};

/// Magic bytes identifying a .rayapp file: "RAYAPP01"
pub const PACKAGE_MAGIC: [u8; 8] = *b"RAYAPP01";

/// Current package format version
pub const PACKAGE_VERSION: u16 = 1;

/// Maximum manifest size (64 KB)
pub const MAX_MANIFEST_SIZE: u32 = 65536;

/// Maximum code section size (16 MB)
pub const MAX_CODE_SIZE: u32 = 16 * 1024 * 1024;

/// Maximum assets section size (64 MB)
pub const MAX_ASSETS_SIZE: u32 = 64 * 1024 * 1024;

/// Maximum number of assets per package
pub const MAX_ASSETS: usize = 256;

/// Maximum app ID length
pub const MAX_APP_ID_LEN: usize = 64;

/// Maximum app name length
pub const MAX_APP_NAME_LEN: usize = 128;

/// Maximum version string length
pub const MAX_VERSION_LEN: usize = 32;

/// Maximum author string length
pub const MAX_AUTHOR_LEN: usize = 128;

/// Maximum description length
pub const MAX_DESCRIPTION_LEN: usize = 512;

/// Maximum number of dependencies
pub const MAX_DEPENDENCIES: usize = 32;

// ===== Package Header =====

/// Binary header at the start of every .rayapp file.
///
/// Total size: 64 bytes (fixed)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct PackageHeader {
    /// Magic bytes: "RAYAPP01"
    pub magic: [u8; 8],
    /// Package format version
    pub version: u16,
    /// Header flags
    pub flags: u16,
    /// Total package size in bytes
    pub total_size: u32,
    /// Offset to manifest section
    pub manifest_offset: u32,
    /// Size of manifest section
    pub manifest_size: u32,
    /// Offset to code section
    pub code_offset: u32,
    /// Size of code section
    pub code_size: u32,
    /// Offset to assets section
    pub assets_offset: u32,
    /// Size of assets section
    pub assets_size: u32,
    /// Number of assets
    pub asset_count: u16,
    /// Reserved for future use
    pub reserved: [u8; 2],
    /// CRC32 checksum of header (excluding this field)
    pub header_checksum: u32,
    /// CRC32 checksum of entire package content
    pub content_checksum: u32,
    /// Padding to 64 bytes
    pub _padding: [u8; 8],
}

impl PackageHeader {
    /// Size of the header in bytes
    pub const SIZE: usize = 64;

    /// Create a new package header
    pub const fn new() -> Self {
        Self {
            magic: PACKAGE_MAGIC,
            version: PACKAGE_VERSION,
            flags: 0,
            total_size: 0,
            manifest_offset: Self::SIZE as u32,
            manifest_size: 0,
            code_offset: 0,
            code_size: 0,
            assets_offset: 0,
            assets_size: 0,
            asset_count: 0,
            reserved: [0; 2],
            header_checksum: 0,
            content_checksum: 0,
            _padding: [0; 8],
        }
    }

    /// Validate the header magic and version
    pub fn validate(&self) -> Result<(), PackageError> {
        if self.magic != PACKAGE_MAGIC {
            return Err(PackageError::InvalidMagic);
        }
        if self.version != PACKAGE_VERSION {
            return Err(PackageError::UnsupportedVersion);
        }
        if self.manifest_size > MAX_MANIFEST_SIZE {
            return Err(PackageError::ManifestTooLarge);
        }
        if self.code_size > MAX_CODE_SIZE {
            return Err(PackageError::CodeTooLarge);
        }
        if self.assets_size > MAX_ASSETS_SIZE {
            return Err(PackageError::AssetsTooLarge);
        }
        Ok(())
    }
}

/// Header flags
pub mod flags {
    /// Package is signed
    pub const SIGNED: u16 = 1 << 0;
    /// Package requires elevated permissions
    pub const PRIVILEGED: u16 = 1 << 1;
    /// Package contains native code (vs bytecode)
    pub const NATIVE_CODE: u16 = 1 << 2;
    /// Package is a system component
    pub const SYSTEM: u16 = 1 << 3;
    /// Package is compressed
    pub const COMPRESSED: u16 = 1 << 4;
}

// ===== Package Manifest =====

/// Parsed package manifest (from embedded JSON-like format).
#[derive(Clone)]
pub struct PackageManifest {
    /// Unique application identifier (e.g., "com.example.myapp")
    pub app_id: [u8; MAX_APP_ID_LEN],
    pub app_id_len: usize,
    /// Human-readable name
    pub name: [u8; MAX_APP_NAME_LEN],
    pub name_len: usize,
    /// Version string (semver)
    pub version: [u8; MAX_VERSION_LEN],
    pub version_len: usize,
    /// Author/publisher
    pub author: [u8; MAX_AUTHOR_LEN],
    pub author_len: usize,
    /// Short description
    pub description: [u8; MAX_DESCRIPTION_LEN],
    pub description_len: usize,
    /// Required capabilities (bitflags)
    pub capabilities: u32,
    /// Minimum RayOS version required
    pub min_rayos_version: u32,
    /// Entry point offset within code section
    pub entry_point: u32,
    /// Dependencies
    pub dependencies: [PackageDependency; MAX_DEPENDENCIES],
    pub dependency_count: usize,
}

impl PackageManifest {
    /// Create an empty manifest
    pub const fn new() -> Self {
        Self {
            app_id: [0; MAX_APP_ID_LEN],
            app_id_len: 0,
            name: [0; MAX_APP_NAME_LEN],
            name_len: 0,
            version: [0; MAX_VERSION_LEN],
            version_len: 0,
            author: [0; MAX_AUTHOR_LEN],
            author_len: 0,
            description: [0; MAX_DESCRIPTION_LEN],
            description_len: 0,
            capabilities: 0,
            min_rayos_version: 0,
            entry_point: 0,
            dependencies: [PackageDependency::empty(); MAX_DEPENDENCIES],
            dependency_count: 0,
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

    /// Get author as bytes
    pub fn author(&self) -> &[u8] {
        &self.author[..self.author_len]
    }

    /// Get description as bytes
    pub fn description(&self) -> &[u8] {
        &self.description[..self.description_len]
    }

    /// Set app ID from bytes
    pub fn set_app_id(&mut self, id: &[u8]) {
        let len = id.len().min(MAX_APP_ID_LEN);
        self.app_id[..len].copy_from_slice(&id[..len]);
        self.app_id_len = len;
    }

    /// Set name from bytes
    pub fn set_name(&mut self, name: &[u8]) {
        let len = name.len().min(MAX_APP_NAME_LEN);
        self.name[..len].copy_from_slice(&name[..len]);
        self.name_len = len;
    }

    /// Set version from bytes
    pub fn set_version(&mut self, ver: &[u8]) {
        let len = ver.len().min(MAX_VERSION_LEN);
        self.version[..len].copy_from_slice(&ver[..len]);
        self.version_len = len;
    }

    /// Set author from bytes
    pub fn set_author(&mut self, author: &[u8]) {
        let len = author.len().min(MAX_AUTHOR_LEN);
        self.author[..len].copy_from_slice(&author[..len]);
        self.author_len = len;
    }

    /// Set description from bytes
    pub fn set_description(&mut self, desc: &[u8]) {
        let len = desc.len().min(MAX_DESCRIPTION_LEN);
        self.description[..len].copy_from_slice(&desc[..len]);
        self.description_len = len;
    }

    /// Add a dependency
    pub fn add_dependency(&mut self, dep: PackageDependency) -> bool {
        if self.dependency_count >= MAX_DEPENDENCIES {
            return false;
        }
        self.dependencies[self.dependency_count] = dep;
        self.dependency_count += 1;
        true
    }
}

/// Package dependency specification
#[derive(Clone, Copy)]
pub struct PackageDependency {
    /// Dependency app ID
    pub app_id: [u8; MAX_APP_ID_LEN],
    pub app_id_len: usize,
    /// Minimum version required (major, minor, patch)
    pub min_version: (u16, u16, u16),
    /// Whether this is an optional dependency
    pub optional: bool,
}

impl PackageDependency {
    /// Create an empty dependency
    pub const fn empty() -> Self {
        Self {
            app_id: [0; MAX_APP_ID_LEN],
            app_id_len: 0,
            min_version: (0, 0, 0),
            optional: false,
        }
    }

    /// Create a new dependency
    pub fn new(app_id: &[u8], min_version: (u16, u16, u16)) -> Self {
        let mut dep = Self::empty();
        let len = app_id.len().min(MAX_APP_ID_LEN);
        dep.app_id[..len].copy_from_slice(&app_id[..len]);
        dep.app_id_len = len;
        dep.min_version = min_version;
        dep
    }

    /// Get app ID as bytes
    pub fn app_id(&self) -> &[u8] {
        &self.app_id[..self.app_id_len]
    }
}

// ===== Package Capabilities =====

/// Required capabilities for a package (permission flags)
pub mod capabilities {
    /// Access to filesystem
    pub const FILESYSTEM: u32 = 1 << 0;
    /// Network access
    pub const NETWORK: u32 = 1 << 1;
    /// Clipboard access
    pub const CLIPBOARD: u32 = 1 << 2;
    /// System notifications
    pub const NOTIFICATIONS: u32 = 1 << 3;
    /// Hardware access
    pub const HARDWARE: u32 = 1 << 4;
    /// Audio playback
    pub const AUDIO: u32 = 1 << 5;
    /// Camera/video capture
    pub const CAMERA: u32 = 1 << 6;
    /// Location services
    pub const LOCATION: u32 = 1 << 7;
    /// Background execution
    pub const BACKGROUND: u32 = 1 << 8;
    /// IPC with other apps
    pub const IPC: u32 = 1 << 9;
    /// Native code execution
    pub const NATIVE: u32 = 1 << 10;
    /// GPU/graphics access
    pub const GPU: u32 = 1 << 11;
}

// ===== Asset Entry =====

/// Asset table entry describing a packaged resource.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct AssetEntry {
    /// Asset name hash (for fast lookup)
    pub name_hash: u32,
    /// Offset within assets section
    pub offset: u32,
    /// Size in bytes
    pub size: u32,
    /// Asset type
    pub asset_type: u8,
    /// Compression type (0 = none)
    pub compression: u8,
    /// Reserved
    pub _reserved: [u8; 2],
}

impl AssetEntry {
    /// Size of an asset entry
    pub const SIZE: usize = 16;

    /// Create a new asset entry
    pub const fn new(name_hash: u32, offset: u32, size: u32, asset_type: AssetType) -> Self {
        Self {
            name_hash,
            offset,
            size,
            asset_type: asset_type as u8,
            compression: 0,
            _reserved: [0; 2],
        }
    }
}

/// Asset types
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AssetType {
    /// Unknown/binary data
    Binary = 0,
    /// Text file
    Text = 1,
    /// Image (PNG/BMP)
    Image = 2,
    /// Font file
    Font = 3,
    /// Audio file
    Audio = 4,
    /// Configuration/settings
    Config = 5,
    /// Localization strings
    Localization = 6,
    /// Icon
    Icon = 7,
}

// ===== Package Signature =====

/// Cryptographic signature for package verification.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PackageSignature {
    /// Signature algorithm (0 = none, 1 = Ed25519)
    pub algorithm: u8,
    /// Key ID (hash of public key)
    pub key_id: [u8; 8],
    /// Reserved
    pub _reserved: [u8; 7],
    /// Signature bytes (Ed25519 = 64 bytes)
    pub signature: [u8; 64],
    /// Padding to 256 bytes
    pub _padding: [u8; 176],
}

impl PackageSignature {
    /// Size of signature block
    pub const SIZE: usize = 256;

    /// Create an empty (unsigned) signature
    pub const fn empty() -> Self {
        Self {
            algorithm: 0,
            key_id: [0; 8],
            _reserved: [0; 7],
            signature: [0; 64],
            _padding: [0; 176],
        }
    }

    /// Check if package is signed
    pub fn is_signed(&self) -> bool {
        self.algorithm != 0
    }
}

/// Signature algorithms
pub mod signature_algo {
    /// No signature
    pub const NONE: u8 = 0;
    /// Ed25519 signature
    pub const ED25519: u8 = 1;
}

// ===== Package Errors =====

/// Errors that can occur during package operations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageError {
    /// Invalid magic bytes
    InvalidMagic,
    /// Unsupported package version
    UnsupportedVersion,
    /// Manifest too large
    ManifestTooLarge,
    /// Code section too large
    CodeTooLarge,
    /// Assets section too large
    AssetsTooLarge,
    /// Invalid checksum
    ChecksumMismatch,
    /// Invalid signature
    InvalidSignature,
    /// Missing required dependency
    MissingDependency,
    /// Incompatible RayOS version
    IncompatibleVersion,
    /// Insufficient permissions
    PermissionDenied,
    /// Package not found
    NotFound,
    /// Package already installed
    AlreadyInstalled,
    /// I/O error
    IoError,
    /// Parse error in manifest
    ParseError,
    /// Out of memory
    OutOfMemory,
}

impl PackageError {
    /// Get error message
    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidMagic => "Invalid package magic",
            Self::UnsupportedVersion => "Unsupported package version",
            Self::ManifestTooLarge => "Manifest too large",
            Self::CodeTooLarge => "Code section too large",
            Self::AssetsTooLarge => "Assets section too large",
            Self::ChecksumMismatch => "Checksum mismatch",
            Self::InvalidSignature => "Invalid signature",
            Self::MissingDependency => "Missing dependency",
            Self::IncompatibleVersion => "Incompatible RayOS version",
            Self::PermissionDenied => "Permission denied",
            Self::NotFound => "Package not found",
            Self::AlreadyInstalled => "Package already installed",
            Self::IoError => "I/O error",
            Self::ParseError => "Manifest parse error",
            Self::OutOfMemory => "Out of memory",
        }
    }
}

// ===== CRC32 Checksum =====

/// CRC32 lookup table (IEEE polynomial)
static CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// Calculate CRC32 checksum of data
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[index];
    }
    !crc
}

/// Calculate CRC32 checksum incrementally
pub fn crc32_update(crc: u32, data: &[u8]) -> u32 {
    let mut crc = !crc;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[index];
    }
    !crc
}

// ===== Simple Manifest Parser =====

/// Parse a key-value pair from manifest bytes.
/// Format: "key": "value" or "key": number
pub fn parse_manifest_field<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    // Search for "key":
    let mut i = 0;
    while i + key.len() + 3 < data.len() {
        // Look for quote before key
        if data[i] == b'"' {
            // Check if key matches
            let mut matches = true;
            for (j, &k) in key.iter().enumerate() {
                if i + 1 + j >= data.len() || data[i + 1 + j] != k {
                    matches = false;
                    break;
                }
            }

            if matches && i + 1 + key.len() < data.len() && data[i + 1 + key.len()] == b'"' {
                // Found key, now find value after ':'
                let mut pos = i + 2 + key.len();
                // Skip whitespace and colon
                while pos < data.len() && (data[pos] == b' ' || data[pos] == b':') {
                    pos += 1;
                }

                if pos < data.len() {
                    if data[pos] == b'"' {
                        // String value
                        let start = pos + 1;
                        let mut end = start;
                        while end < data.len() && data[end] != b'"' {
                            end += 1;
                        }
                        return Some(&data[start..end]);
                    } else if data[pos].is_ascii_digit() {
                        // Numeric value
                        let start = pos;
                        let mut end = start;
                        while end < data.len() && (data[end].is_ascii_digit() || data[end] == b'.') {
                            end += 1;
                        }
                        return Some(&data[start..end]);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Parse a manifest from JSON-like bytes
pub fn parse_manifest(data: &[u8]) -> Result<PackageManifest, PackageError> {
    let mut manifest = PackageManifest::new();

    // Parse required fields
    if let Some(id) = parse_manifest_field(data, b"id") {
        manifest.set_app_id(id);
    } else {
        return Err(PackageError::ParseError);
    }

    if let Some(name) = parse_manifest_field(data, b"name") {
        manifest.set_name(name);
    }

    if let Some(version) = parse_manifest_field(data, b"version") {
        manifest.set_version(version);
    }

    if let Some(author) = parse_manifest_field(data, b"author") {
        manifest.set_author(author);
    }

    if let Some(description) = parse_manifest_field(data, b"description") {
        manifest.set_description(description);
    }

    // Parse capabilities as number
    if let Some(caps) = parse_manifest_field(data, b"capabilities") {
        manifest.capabilities = parse_u32(caps).unwrap_or(0);
    }

    // Parse entry point
    if let Some(entry) = parse_manifest_field(data, b"entry_point") {
        manifest.entry_point = parse_u32(entry).unwrap_or(0);
    }

    Ok(manifest)
}

/// Parse u32 from ASCII bytes
fn parse_u32(data: &[u8]) -> Option<u32> {
    let mut result = 0u32;
    for &b in data {
        if b.is_ascii_digit() {
            result = result.saturating_mul(10).saturating_add((b - b'0') as u32);
        } else {
            break;
        }
    }
    Some(result)
}

// ===== Package Builder =====

/// Builder for creating .rayapp packages
pub struct PackageBuilder {
    manifest: PackageManifest,
    code: [u8; 4096], // Simplified: small code buffer
    code_len: usize,
    assets: [AssetEntry; MAX_ASSETS],
    asset_count: usize,
    flags: u16,
}

impl PackageBuilder {
    /// Create a new package builder
    pub const fn new() -> Self {
        Self {
            manifest: PackageManifest::new(),
            code: [0; 4096],
            code_len: 0,
            assets: [AssetEntry::new(0, 0, 0, AssetType::Binary); MAX_ASSETS],
            asset_count: 0,
            flags: 0,
        }
    }

    /// Set the manifest
    pub fn manifest(mut self, manifest: PackageManifest) -> Self {
        self.manifest = manifest;
        self
    }

    /// Set package flags
    pub fn flags(mut self, flags: u16) -> Self {
        self.flags = flags;
        self
    }

    /// Set code section
    pub fn code(mut self, code: &[u8]) -> Self {
        let len = code.len().min(self.code.len());
        self.code[..len].copy_from_slice(&code[..len]);
        self.code_len = len;
        self
    }

    /// Add an asset
    pub fn add_asset(mut self, entry: AssetEntry) -> Self {
        if self.asset_count < MAX_ASSETS {
            self.assets[self.asset_count] = entry;
            self.asset_count += 1;
        }
        self
    }

    /// Calculate total package size
    pub fn calculate_size(&self) -> u32 {
        let manifest_size = 1024u32; // Fixed manifest area
        let code_size = self.code_len as u32;
        let assets_size = (self.asset_count * AssetEntry::SIZE) as u32;

        PackageHeader::SIZE as u32 + manifest_size + code_size + assets_size + PackageSignature::SIZE as u32
    }
}

// ===== Package Statistics =====

/// Global package statistics
static PACKAGES_INSTALLED: AtomicU32 = AtomicU32::new(0);
static PACKAGES_LOADED: AtomicU32 = AtomicU32::new(0);

/// Get number of installed packages
pub fn installed_count() -> u32 {
    PACKAGES_INSTALLED.load(Ordering::Relaxed)
}

/// Get number of loaded packages
pub fn loaded_count() -> u32 {
    PACKAGES_LOADED.load(Ordering::Relaxed)
}

/// Increment installed count
pub fn increment_installed() {
    PACKAGES_INSTALLED.fetch_add(1, Ordering::Relaxed);
}

/// Increment loaded count
pub fn increment_loaded() {
    PACKAGES_LOADED.fetch_add(1, Ordering::Relaxed);
}

/// Decrement installed count
pub fn decrement_installed() {
    PACKAGES_INSTALLED.fetch_sub(1, Ordering::Relaxed);
}

/// Decrement loaded count
pub fn decrement_loaded() {
    PACKAGES_LOADED.fetch_sub(1, Ordering::Relaxed);
}

#[cfg(feature = "serial_debug")]
fn serial_debug(msg: &str) {
    crate::serial_write_str(msg);
}

#[cfg(not(feature = "serial_debug"))]
fn serial_debug(_msg: &str) {}
