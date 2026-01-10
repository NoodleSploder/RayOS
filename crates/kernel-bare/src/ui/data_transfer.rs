//! Data Transfer Format System for RayOS UI
//!
//! Format registry and conversion for clipboard and drag-drop operations.
//!
//! # Overview
//!
//! The Data Transfer system provides:
//! - Format registration and discovery
//! - Format conversion between types
//! - Data serialization/deserialization
//! - MIME type negotiation
//! - Binary and text encoding
//!
//! # Markers
//!
//! - `RAYOS_TRANSFER:REGISTERED` - Format registered
//! - `RAYOS_TRANSFER:CONVERTED` - Data converted between formats
//! - `RAYOS_TRANSFER:SERIALIZED` - Data serialized
//! - `RAYOS_TRANSFER:DESERIALIZED` - Data deserialized
//! - `RAYOS_TRANSFER:NEGOTIATED` - Format negotiated

use super::clipboard::ClipboardFormat;

// ============================================================================
// Constants
// ============================================================================

/// Maximum registered formats.
pub const MAX_FORMATS: usize = 64;

/// Maximum converters.
pub const MAX_CONVERTERS: usize = 32;

/// Maximum transfer buffer size.
pub const MAX_BUFFER_SIZE: usize = 1024 * 1024; // 1MB

/// Maximum inline buffer size.
pub const MAX_INLINE_SIZE: usize = 4096;

/// Maximum MIME type length.
pub const MAX_MIME_LEN: usize = 64;

/// Maximum format name length.
pub const MAX_FORMAT_NAME_LEN: usize = 32;

// ============================================================================
// Format ID
// ============================================================================

/// Unique format identifier.
pub type FormatId = u32;

/// Reserved format IDs for standard types.
pub mod standard_formats {
    use super::FormatId;

    pub const TEXT_PLAIN: FormatId = 1;
    pub const TEXT_HTML: FormatId = 2;
    pub const TEXT_RTF: FormatId = 3;
    pub const TEXT_URI_LIST: FormatId = 4;
    pub const IMAGE_PNG: FormatId = 5;
    pub const IMAGE_JPEG: FormatId = 6;
    pub const IMAGE_BMP: FormatId = 7;
    pub const APPLICATION_OCTET_STREAM: FormatId = 8;
    pub const RAYOS_FILE_LIST: FormatId = 100;
    pub const RAYOS_WINDOW_REF: FormatId = 101;
    pub const RAYOS_APP_DATA: FormatId = 102;
}

// ============================================================================
// Format Descriptor
// ============================================================================

/// Format encoding type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum FormatEncoding {
    /// Raw binary data.
    Binary = 0,
    /// UTF-8 text.
    Utf8 = 1,
    /// Base64 encoded.
    Base64 = 2,
    /// Hex encoded.
    Hex = 3,
}

impl Default for FormatEncoding {
    fn default() -> Self {
        FormatEncoding::Binary
    }
}

/// Format descriptor.
#[derive(Clone, Copy)]
pub struct FormatDescriptor {
    /// Format ID.
    pub id: FormatId,
    /// Format name.
    pub name: [u8; MAX_FORMAT_NAME_LEN],
    /// Name length.
    pub name_len: usize,
    /// MIME type.
    pub mime: [u8; MAX_MIME_LEN],
    /// MIME length.
    pub mime_len: usize,
    /// Encoding type.
    pub encoding: FormatEncoding,
    /// Is text-based format.
    pub is_text: bool,
    /// Priority (for format negotiation).
    pub priority: u8,
    /// Registered flag.
    pub registered: bool,
}

impl FormatDescriptor {
    /// Create empty descriptor.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            name: [0u8; MAX_FORMAT_NAME_LEN],
            name_len: 0,
            mime: [0u8; MAX_MIME_LEN],
            mime_len: 0,
            encoding: FormatEncoding::Binary,
            is_text: false,
            priority: 0,
            registered: false,
        }
    }

    /// Create new descriptor.
    pub fn new(id: FormatId, name: &str, mime: &str, is_text: bool) -> Self {
        let mut desc = Self::empty();
        desc.id = id;
        desc.set_name(name);
        desc.set_mime(mime);
        desc.is_text = is_text;
        desc.encoding = if is_text {
            FormatEncoding::Utf8
        } else {
            FormatEncoding::Binary
        };
        desc.registered = true;
        desc
    }

    /// Set name.
    pub fn set_name(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = bytes.len().min(MAX_FORMAT_NAME_LEN - 1);
        self.name = [0u8; MAX_FORMAT_NAME_LEN];
        self.name[..len].copy_from_slice(&bytes[..len]);
        self.name_len = len;
    }

    /// Get name.
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }

    /// Set MIME type.
    pub fn set_mime(&mut self, mime: &str) {
        let bytes = mime.as_bytes();
        let len = bytes.len().min(MAX_MIME_LEN - 1);
        self.mime = [0u8; MAX_MIME_LEN];
        self.mime[..len].copy_from_slice(&bytes[..len]);
        self.mime_len = len;
    }

    /// Get MIME type.
    pub fn mime(&self) -> &str {
        core::str::from_utf8(&self.mime[..self.mime_len]).unwrap_or("")
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        self.registered && self.id > 0
    }

    /// Convert from ClipboardFormat.
    pub fn from_clipboard_format(format: ClipboardFormat) -> Self {
        match format {
            ClipboardFormat::None => Self::empty(),
            ClipboardFormat::Text => Self::new(standard_formats::TEXT_PLAIN, "text", "text/plain", true),
            ClipboardFormat::RichText => Self::new(standard_formats::TEXT_RTF, "rtf", "text/rtf", true),
            ClipboardFormat::Html => Self::new(standard_formats::TEXT_HTML, "html", "text/html", true),
            ClipboardFormat::ImagePng => Self::new(standard_formats::IMAGE_PNG, "png", "image/png", false),
            ClipboardFormat::ImageJpeg => Self::new(standard_formats::IMAGE_JPEG, "jpeg", "image/jpeg", false),
            ClipboardFormat::ImageBmp => Self::new(standard_formats::IMAGE_BMP, "bmp", "image/bmp", false),
            ClipboardFormat::FilePaths => Self::new(standard_formats::RAYOS_FILE_LIST, "files", "application/x-rayos-files", true),
            ClipboardFormat::UriList => Self::new(standard_formats::TEXT_URI_LIST, "uris", "text/uri-list", true),
            ClipboardFormat::Binary => Self::new(standard_formats::APPLICATION_OCTET_STREAM, "binary", "application/octet-stream", false),
            ClipboardFormat::Custom(id) => {
                let mut desc = Self::new(id as FormatId + 1000, "custom", "application/x-custom", false);
                desc.priority = 50;
                desc
            }
        }
    }
}

// ============================================================================
// Transfer Buffer
// ============================================================================

/// Inline transfer data.
#[derive(Clone, Copy)]
pub struct InlineBuffer {
    /// Data bytes.
    pub bytes: [u8; MAX_INLINE_SIZE],
    /// Data length.
    pub len: usize,
}

impl InlineBuffer {
    /// Create empty buffer.
    pub const fn empty() -> Self {
        Self {
            bytes: [0u8; MAX_INLINE_SIZE],
            len: 0,
        }
    }

    /// Create from bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() > MAX_INLINE_SIZE {
            return None;
        }
        let mut buf = Self::empty();
        buf.bytes[..data.len()].copy_from_slice(data);
        buf.len = data.len();
        Some(buf)
    }

    /// Get data.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(self.as_bytes()).ok()
    }

    /// Append bytes.
    pub fn append(&mut self, data: &[u8]) -> bool {
        if self.len + data.len() > MAX_INLINE_SIZE {
            return false;
        }
        self.bytes[self.len..self.len + data.len()].copy_from_slice(data);
        self.len += data.len();
        true
    }

    /// Clear buffer.
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

/// Transfer data storage.
#[derive(Clone, Copy)]
pub enum TransferData {
    /// No data.
    Empty,
    /// Inline data.
    Inline(InlineBuffer),
    /// External reference.
    External { offset: usize, len: usize },
}

impl Default for TransferData {
    fn default() -> Self {
        TransferData::Empty
    }
}

impl TransferData {
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, TransferData::Empty)
    }

    /// Get data length.
    pub fn len(&self) -> usize {
        match self {
            TransferData::Empty => 0,
            TransferData::Inline(buf) => buf.len,
            TransferData::External { len, .. } => *len,
        }
    }
}

/// Transfer buffer with format info.
#[derive(Clone, Copy)]
pub struct TransferBuffer {
    /// Format ID.
    pub format_id: FormatId,
    /// Data storage.
    pub data: TransferData,
    /// Checksum (CRC32).
    pub checksum: u32,
    /// Timestamp.
    pub timestamp: u64,
}

impl TransferBuffer {
    /// Create empty buffer.
    pub const fn empty() -> Self {
        Self {
            format_id: 0,
            data: TransferData::Empty,
            checksum: 0,
            timestamp: 0,
        }
    }

    /// Create from text.
    pub fn from_text(text: &str) -> Option<Self> {
        let inline = InlineBuffer::from_bytes(text.as_bytes())?;
        Some(Self {
            format_id: standard_formats::TEXT_PLAIN,
            data: TransferData::Inline(inline),
            checksum: simple_crc32(text.as_bytes()),
            timestamp: 0,
        })
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        !self.data.is_empty() && self.format_id > 0
    }

    /// Verify checksum.
    pub fn verify(&self) -> bool {
        match &self.data {
            TransferData::Inline(buf) => simple_crc32(buf.as_bytes()) == self.checksum,
            _ => true, // External data not verified here
        }
    }
}

/// Simple CRC32 for checksums.
fn simple_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB88320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

// ============================================================================
// Format Converter
// ============================================================================

/// Converter function signature.
pub type ConvertFn = fn(input: &[u8], output: &mut [u8]) -> Option<usize>;

/// Format converter.
#[derive(Clone, Copy)]
pub struct FormatConverter {
    /// Converter ID.
    pub id: u32,
    /// Source format.
    pub source_format: FormatId,
    /// Target format.
    pub target_format: FormatId,
    /// Conversion function.
    pub convert: Option<ConvertFn>,
    /// Quality/fidelity (0-100).
    pub quality: u8,
    /// Active flag.
    pub active: bool,
}

impl FormatConverter {
    /// Create empty converter.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            source_format: 0,
            target_format: 0,
            convert: None,
            quality: 100,
            active: false,
        }
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        self.active && self.convert.is_some()
    }

    /// Perform conversion.
    pub fn convert(&self, input: &[u8], output: &mut [u8]) -> Option<usize> {
        if let Some(f) = self.convert {
            f(input, output)
        } else {
            None
        }
    }
}

// ============================================================================
// Data Provider
// ============================================================================

/// Data provider callback.
pub type DataProviderFn = fn(format: FormatId, buffer: &mut [u8]) -> Option<usize>;

/// Data provider for lazy/deferred data.
#[derive(Clone, Copy)]
pub struct DataProvider {
    /// Provider ID.
    pub id: u32,
    /// Supported formats.
    pub formats: [FormatId; 8],
    /// Format count.
    pub format_count: usize,
    /// Provider callback.
    pub provide: Option<DataProviderFn>,
    /// Active flag.
    pub active: bool,
}

impl DataProvider {
    /// Create empty provider.
    pub const fn empty() -> Self {
        Self {
            id: 0,
            formats: [0; 8],
            format_count: 0,
            provide: None,
            active: false,
        }
    }

    /// Check if supports format.
    pub fn supports(&self, format: FormatId) -> bool {
        self.formats[..self.format_count].contains(&format)
    }

    /// Add supported format.
    pub fn add_format(&mut self, format: FormatId) -> bool {
        if self.format_count >= 8 {
            return false;
        }
        self.formats[self.format_count] = format;
        self.format_count += 1;
        true
    }

    /// Get data for format.
    pub fn get(&self, format: FormatId, buffer: &mut [u8]) -> Option<usize> {
        if !self.supports(format) {
            return None;
        }
        if let Some(f) = self.provide {
            f(format, buffer)
        } else {
            None
        }
    }
}

// ============================================================================
// Format Negotiator
// ============================================================================

/// Format negotiation result.
#[derive(Clone, Copy, Debug)]
pub struct NegotiationResult {
    /// Best format found.
    pub format_id: FormatId,
    /// Converter chain (if conversion needed).
    pub converter_ids: [u32; 4],
    /// Number of converters.
    pub converter_count: usize,
    /// Total quality score.
    pub quality: u8,
}

impl NegotiationResult {
    /// Create empty result.
    pub const fn empty() -> Self {
        Self {
            format_id: 0,
            converter_ids: [0; 4],
            converter_count: 0,
            quality: 0,
        }
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        self.format_id > 0
    }

    /// Check if conversion needed.
    pub fn needs_conversion(&self) -> bool {
        self.converter_count > 0
    }
}

/// Format negotiator.
pub struct FormatNegotiator {
    /// Source formats (offered).
    source_formats: [FormatId; 16],
    /// Source format count.
    source_count: usize,
    /// Target formats (accepted).
    target_formats: [FormatId; 16],
    /// Target format count.
    target_count: usize,
    /// Priority overrides.
    priorities: [(FormatId, u8); 16],
    /// Priority count.
    priority_count: usize,
}

impl FormatNegotiator {
    /// Create new negotiator.
    pub const fn new() -> Self {
        Self {
            source_formats: [0; 16],
            source_count: 0,
            target_formats: [0; 16],
            target_count: 0,
            priorities: [(0, 0); 16],
            priority_count: 0,
        }
    }

    /// Add source format.
    pub fn add_source(&mut self, format: FormatId) -> bool {
        if self.source_count >= 16 {
            return false;
        }
        self.source_formats[self.source_count] = format;
        self.source_count += 1;
        true
    }

    /// Add target format.
    pub fn add_target(&mut self, format: FormatId) -> bool {
        if self.target_count >= 16 {
            return false;
        }
        self.target_formats[self.target_count] = format;
        self.target_count += 1;
        true
    }

    /// Set priority for format.
    pub fn set_priority(&mut self, format: FormatId, priority: u8) -> bool {
        // Check if already exists
        for (f, p) in &mut self.priorities[..self.priority_count] {
            if *f == format {
                *p = priority;
                return true;
            }
        }
        // Add new
        if self.priority_count >= 16 {
            return false;
        }
        self.priorities[self.priority_count] = (format, priority);
        self.priority_count += 1;
        true
    }

    /// Get priority for format.
    fn get_priority(&self, format: FormatId) -> u8 {
        for (f, p) in &self.priorities[..self.priority_count] {
            if *f == format {
                return *p;
            }
        }
        // Default priorities
        match format {
            standard_formats::TEXT_PLAIN => 100,
            standard_formats::TEXT_HTML => 90,
            standard_formats::IMAGE_PNG => 80,
            _ => 50,
        }
    }

    /// Negotiate best format.
    pub fn negotiate(&self) -> NegotiationResult {
        let mut best = NegotiationResult::empty();
        let mut best_priority = 0u8;

        // First try direct matches
        for &src in &self.source_formats[..self.source_count] {
            for &tgt in &self.target_formats[..self.target_count] {
                if src == tgt {
                    let priority = self.get_priority(src);
                    if priority > best_priority {
                        best.format_id = src;
                        best.quality = 100;
                        best_priority = priority;
                    }
                }
            }
        }

        // RAYOS_TRANSFER:NEGOTIATED
        best
    }

    /// Clear all formats.
    pub fn clear(&mut self) {
        self.source_count = 0;
        self.target_count = 0;
    }
}

// ============================================================================
// Format Registry
// ============================================================================

/// Format registry.
pub struct FormatRegistry {
    /// Registered formats.
    formats: [FormatDescriptor; MAX_FORMATS],
    /// Format count.
    format_count: usize,
    /// Converters.
    converters: [FormatConverter; MAX_CONVERTERS],
    /// Converter count.
    converter_count: usize,
    /// Next format ID.
    next_format_id: FormatId,
    /// Next converter ID.
    next_converter_id: u32,
}

impl FormatRegistry {
    /// Create new registry.
    pub const fn new() -> Self {
        Self {
            formats: [FormatDescriptor::empty(); MAX_FORMATS],
            format_count: 0,
            converters: [FormatConverter::empty(); MAX_CONVERTERS],
            converter_count: 0,
            next_format_id: 1000, // Start after reserved IDs
            next_converter_id: 1,
        }
    }

    /// Register standard formats.
    pub fn register_standard_formats(&mut self) {
        self.register_format(FormatDescriptor::new(
            standard_formats::TEXT_PLAIN,
            "text/plain",
            "text/plain",
            true,
        ));
        self.register_format(FormatDescriptor::new(
            standard_formats::TEXT_HTML,
            "text/html",
            "text/html",
            true,
        ));
        self.register_format(FormatDescriptor::new(
            standard_formats::TEXT_RTF,
            "text/rtf",
            "text/rtf",
            true,
        ));
        self.register_format(FormatDescriptor::new(
            standard_formats::TEXT_URI_LIST,
            "text/uri-list",
            "text/uri-list",
            true,
        ));
        self.register_format(FormatDescriptor::new(
            standard_formats::IMAGE_PNG,
            "image/png",
            "image/png",
            false,
        ));
        self.register_format(FormatDescriptor::new(
            standard_formats::IMAGE_JPEG,
            "image/jpeg",
            "image/jpeg",
            false,
        ));
        self.register_format(FormatDescriptor::new(
            standard_formats::IMAGE_BMP,
            "image/bmp",
            "image/bmp",
            false,
        ));
        self.register_format(FormatDescriptor::new(
            standard_formats::APPLICATION_OCTET_STREAM,
            "application/octet-stream",
            "application/octet-stream",
            false,
        ));
        // RAYOS_TRANSFER:REGISTERED
    }

    /// Register a format.
    pub fn register_format(&mut self, mut desc: FormatDescriptor) -> Option<FormatId> {
        if self.format_count >= MAX_FORMATS {
            return None;
        }

        // Assign ID if not set
        if desc.id == 0 {
            desc.id = self.next_format_id;
            self.next_format_id += 1;
        }

        desc.registered = true;
        self.formats[self.format_count] = desc;
        self.format_count += 1;

        Some(desc.id)
    }

    /// Find format by ID.
    pub fn find(&self, id: FormatId) -> Option<&FormatDescriptor> {
        self.formats[..self.format_count]
            .iter()
            .find(|f| f.id == id && f.is_valid())
    }

    /// Find format by MIME type.
    pub fn find_by_mime(&self, mime: &str) -> Option<&FormatDescriptor> {
        self.formats[..self.format_count]
            .iter()
            .find(|f| f.mime() == mime && f.is_valid())
    }

    /// Register a converter.
    pub fn register_converter(
        &mut self,
        source: FormatId,
        target: FormatId,
        convert: ConvertFn,
        quality: u8,
    ) -> Option<u32> {
        if self.converter_count >= MAX_CONVERTERS {
            return None;
        }

        let id = self.next_converter_id;
        self.next_converter_id += 1;

        self.converters[self.converter_count] = FormatConverter {
            id,
            source_format: source,
            target_format: target,
            convert: Some(convert),
            quality,
            active: true,
        };
        self.converter_count += 1;

        Some(id)
    }

    /// Find converter.
    pub fn find_converter(&self, source: FormatId, target: FormatId) -> Option<&FormatConverter> {
        self.converters[..self.converter_count]
            .iter()
            .find(|c| c.source_format == source && c.target_format == target && c.active)
    }

    /// Convert data between formats.
    pub fn convert(
        &self,
        source: FormatId,
        target: FormatId,
        input: &[u8],
        output: &mut [u8],
    ) -> Option<usize> {
        if source == target {
            // No conversion needed
            let len = input.len().min(output.len());
            output[..len].copy_from_slice(&input[..len]);
            // RAYOS_TRANSFER:CONVERTED
            return Some(len);
        }

        if let Some(converter) = self.find_converter(source, target) {
            converter.convert(input, output)
        } else {
            None
        }
    }

    /// Get all text formats.
    pub fn text_formats(&self) -> impl Iterator<Item = &FormatDescriptor> {
        self.formats[..self.format_count]
            .iter()
            .filter(|f| f.is_text && f.is_valid())
    }

    /// Get all image formats.
    pub fn image_formats(&self) -> impl Iterator<Item = &FormatDescriptor> {
        self.formats[..self.format_count]
            .iter()
            .filter(|f| !f.is_text && f.mime().starts_with("image/") && f.is_valid())
    }
}

// ============================================================================
// Serialization Helpers
// ============================================================================

/// Serialize text to buffer.
pub fn serialize_text(text: &str, buffer: &mut [u8]) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.len() > buffer.len() {
        return None;
    }
    buffer[..bytes.len()].copy_from_slice(bytes);
    // RAYOS_TRANSFER:SERIALIZED
    Some(bytes.len())
}

/// Deserialize text from buffer.
pub fn deserialize_text(buffer: &[u8]) -> Option<&str> {
    // RAYOS_TRANSFER:DESERIALIZED
    core::str::from_utf8(buffer).ok()
}

/// Serialize u32 to buffer (little-endian).
pub fn serialize_u32(value: u32, buffer: &mut [u8]) -> Option<usize> {
    if buffer.len() < 4 {
        return None;
    }
    buffer[0] = value as u8;
    buffer[1] = (value >> 8) as u8;
    buffer[2] = (value >> 16) as u8;
    buffer[3] = (value >> 24) as u8;
    Some(4)
}

/// Deserialize u32 from buffer (little-endian).
pub fn deserialize_u32(buffer: &[u8]) -> Option<u32> {
    if buffer.len() < 4 {
        return None;
    }
    Some(
        buffer[0] as u32
            | (buffer[1] as u32) << 8
            | (buffer[2] as u32) << 16
            | (buffer[3] as u32) << 24,
    )
}

/// Serialize u64 to buffer (little-endian).
pub fn serialize_u64(value: u64, buffer: &mut [u8]) -> Option<usize> {
    if buffer.len() < 8 {
        return None;
    }
    for i in 0..8 {
        buffer[i] = (value >> (i * 8)) as u8;
    }
    Some(8)
}

/// Deserialize u64 from buffer (little-endian).
pub fn deserialize_u64(buffer: &[u8]) -> Option<u64> {
    if buffer.len() < 8 {
        return None;
    }
    let mut value = 0u64;
    for i in 0..8 {
        value |= (buffer[i] as u64) << (i * 8);
    }
    Some(value)
}

// ============================================================================
// Base64 Encoding
// ============================================================================

/// Base64 character table.
const BASE64_TABLE: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 encode data.
pub fn base64_encode(input: &[u8], output: &mut [u8]) -> Option<usize> {
    let output_len = ((input.len() + 2) / 3) * 4;
    if output.len() < output_len {
        return None;
    }

    let mut out_idx = 0;
    let mut i = 0;

    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        output[out_idx] = BASE64_TABLE[((n >> 18) & 0x3F) as usize];
        output[out_idx + 1] = BASE64_TABLE[((n >> 12) & 0x3F) as usize];
        output[out_idx + 2] = BASE64_TABLE[((n >> 6) & 0x3F) as usize];
        output[out_idx + 3] = BASE64_TABLE[(n & 0x3F) as usize];
        out_idx += 4;
        i += 3;
    }

    // Handle remaining bytes
    let remaining = input.len() - i;
    if remaining == 1 {
        let n = (input[i] as u32) << 16;
        output[out_idx] = BASE64_TABLE[((n >> 18) & 0x3F) as usize];
        output[out_idx + 1] = BASE64_TABLE[((n >> 12) & 0x3F) as usize];
        output[out_idx + 2] = b'=';
        output[out_idx + 3] = b'=';
        out_idx += 4;
    } else if remaining == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        output[out_idx] = BASE64_TABLE[((n >> 18) & 0x3F) as usize];
        output[out_idx + 1] = BASE64_TABLE[((n >> 12) & 0x3F) as usize];
        output[out_idx + 2] = BASE64_TABLE[((n >> 6) & 0x3F) as usize];
        output[out_idx + 3] = b'=';
        out_idx += 4;
    }

    Some(out_idx)
}

/// Base64 decode character.
fn base64_decode_char(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        b'=' => Some(0),
        _ => None,
    }
}

/// Base64 decode data.
pub fn base64_decode(input: &[u8], output: &mut [u8]) -> Option<usize> {
    if input.len() % 4 != 0 {
        return None;
    }

    let mut out_idx = 0;
    let mut i = 0;

    while i < input.len() {
        let a = base64_decode_char(input[i])?;
        let b = base64_decode_char(input[i + 1])?;
        let c = base64_decode_char(input[i + 2])?;
        let d = base64_decode_char(input[i + 3])?;

        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);

        if out_idx < output.len() {
            output[out_idx] = ((n >> 16) & 0xFF) as u8;
            out_idx += 1;
        }
        if input[i + 2] != b'=' && out_idx < output.len() {
            output[out_idx] = ((n >> 8) & 0xFF) as u8;
            out_idx += 1;
        }
        if input[i + 3] != b'=' && out_idx < output.len() {
            output[out_idx] = (n & 0xFF) as u8;
            out_idx += 1;
        }

        i += 4;
    }

    Some(out_idx)
}

// ============================================================================
// Global Format Registry
// ============================================================================

/// Global format registry.
static mut GLOBAL_REGISTRY: FormatRegistry = FormatRegistry::new();

/// Get format registry.
pub fn format_registry() -> &'static FormatRegistry {
    // SAFETY: Single-threaded kernel
    unsafe { &GLOBAL_REGISTRY }
}

/// Get format registry (mutable).
pub fn format_registry_mut() -> &'static mut FormatRegistry {
    // SAFETY: Single-threaded kernel
    unsafe { &mut GLOBAL_REGISTRY }
}

/// Initialize the format registry.
pub fn init_format_registry() {
    format_registry_mut().register_standard_formats();
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_descriptor() {
        let desc = FormatDescriptor::new(1, "text", "text/plain", true);
        assert_eq!(desc.name(), "text");
        assert_eq!(desc.mime(), "text/plain");
        assert!(desc.is_text);
    }

    #[test]
    fn test_inline_buffer() {
        let buf = InlineBuffer::from_bytes(b"hello").unwrap();
        assert_eq!(buf.as_bytes(), b"hello");
        assert_eq!(buf.as_str(), Some("hello"));
    }

    #[test]
    fn test_transfer_buffer() {
        let buf = TransferBuffer::from_text("test").unwrap();
        assert!(buf.is_valid());
        assert!(buf.verify());
    }

    #[test]
    fn test_crc32() {
        let crc1 = simple_crc32(b"hello");
        let crc2 = simple_crc32(b"hello");
        let crc3 = simple_crc32(b"world");
        assert_eq!(crc1, crc2);
        assert_ne!(crc1, crc3);
    }

    #[test]
    fn test_format_negotiator() {
        let mut neg = FormatNegotiator::new();
        neg.add_source(standard_formats::TEXT_PLAIN);
        neg.add_source(standard_formats::TEXT_HTML);
        neg.add_target(standard_formats::TEXT_PLAIN);

        let result = neg.negotiate();
        assert!(result.is_valid());
        assert_eq!(result.format_id, standard_formats::TEXT_PLAIN);
    }

    #[test]
    fn test_format_registry() {
        let mut registry = FormatRegistry::new();
        registry.register_standard_formats();

        let desc = registry.find(standard_formats::TEXT_PLAIN);
        assert!(desc.is_some());
    }

    #[test]
    fn test_serialization() {
        let mut buf = [0u8; 64];
        let len = serialize_text("hello", &mut buf).unwrap();
        assert_eq!(len, 5);

        let text = deserialize_text(&buf[..len]).unwrap();
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_u32_serialization() {
        let mut buf = [0u8; 4];
        serialize_u32(0x12345678, &mut buf).unwrap();
        let value = deserialize_u32(&buf).unwrap();
        assert_eq!(value, 0x12345678);
    }

    #[test]
    fn test_base64() {
        let input = b"hello";
        let mut encoded = [0u8; 16];
        let enc_len = base64_encode(input, &mut encoded).unwrap();
        
        let mut decoded = [0u8; 16];
        let dec_len = base64_decode(&encoded[..enc_len], &mut decoded).unwrap();
        
        assert_eq!(&decoded[..dec_len], input);
    }
}
