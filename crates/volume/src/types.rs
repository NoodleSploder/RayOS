//! Core data types for the Volume semantic file system

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A unique identifier for files/documents in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileId(pub u64);

impl FileId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn from_hash(data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        let bytes = hash.as_bytes();
        let mut id_bytes = [0u8; 8];
        id_bytes.copy_from_slice(&bytes[0..8]);
        Self(u64::from_le_bytes(id_bytes))
    }
}

/// Vector embedding representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// The vector values (typically 384, 768, or 1536 dimensions)
    pub vector: Vec<f32>,
    /// Model used to generate this embedding
    pub model: String,
    /// Timestamp of creation
    pub timestamp: u64,
}

impl Embedding {
    pub fn dimension(&self) -> usize {
        self.vector.len()
    }

    /// Cosine similarity between two embeddings
    pub fn similarity(&self, other: &Embedding) -> f32 {
        if self.vector.len() != other.vector.len() {
            return 0.0;
        }

        let dot: f32 = self.vector.iter()
            .zip(other.vector.iter())
            .map(|(a, b)| a * b)
            .sum();

        let mag_a: f32 = self.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = other.vector.iter().map(|x| x * x).sum::<f32>().sqrt();

        if mag_a == 0.0 || mag_b == 0.0 {
            return 0.0;
        }

        dot / (mag_a * mag_b)
    }
}

/// Metadata about a file in the semantic file system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: FileId,
    pub path: PathBuf,
    pub file_type: FileType,
    pub size: u64,
    pub created: u64,
    pub modified: u64,
    pub content_hash: String,
    pub tags: Vec<String>,
}

/// Type of file content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    Text,
    Code,
    Image,
    Audio,
    Video,
    Binary,
    Unknown,
}

impl FileType {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "txt" | "md" | "markdown" => Self::Text,
            "rs" | "py" | "js" | "ts" | "cpp" | "c" | "h" | "java" | "go" => Self::Code,
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => Self::Image,
            "mp3" | "wav" | "ogg" | "flac" => Self::Audio,
            "mp4" | "avi" | "mkv" | "webm" => Self::Video,
            _ => Self::Unknown,
        }
    }
}

/// A document with its embedding in the vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub metadata: FileMetadata,
    pub embedding: Embedding,
    pub content_preview: String,
}

/// Search query with options
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// Query text or embedding
    pub query: Query,
    /// Maximum number of results
    pub limit: usize,
    /// Minimum similarity threshold (0.0 - 1.0)
    pub threshold: f32,
    /// Optional file type filter
    pub file_type: Option<FileType>,
    /// Optional tag filters
    pub tags: Vec<String>,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: Query::Text(String::new()),
            limit: 10,
            threshold: 0.5,
            file_type: None,
            tags: Vec::new(),
        }
    }
}

/// Query can be text (will be embedded) or a pre-computed embedding
#[derive(Debug, Clone)]
pub enum Query {
    Text(String),
    Embedding(Embedding),
}

/// Search result with similarity score
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub document: Document,
    pub similarity: f32,
}

/// Configuration for the Volume system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    /// Path to the database directory
    pub db_path: PathBuf,
    /// Embedding model to use
    pub embedding_model: String,
    /// Embedding dimension
    pub embedding_dim: usize,
    /// HNSW parameters
    pub hnsw_m: usize,  // Number of connections per layer
    pub hnsw_ef_construction: usize,  // Size of dynamic candidate list
    pub hnsw_ef_search: usize,  // Search parameter
    /// Enable file watching
    pub watch_files: bool,
    /// Maximum file size to embed (in bytes)
    pub max_file_size: u64,
}

impl Default for VolumeConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./volume_db"),
            embedding_model: "all-MiniLM-L6-v2".to_string(),
            embedding_dim: 384,
            hnsw_m: 16,
            hnsw_ef_construction: 200,
            hnsw_ef_search: 100,
            watch_files: true,
            max_file_size: 10 * 1024 * 1024,  // 10 MB
        }
    }
}

/// An "epiphany" - an idea generated during dream mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Epiphany {
    pub id: u64,
    pub timestamp: u64,
    pub content: String,
    pub source_files: Vec<FileId>,
    pub validation_status: ValidationStatus,
    pub embedding: Option<Embedding>,
}

/// Validation status of an epiphany
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    Pending,
    Testing,
    Valid,
    Invalid,
    Integrated,
}
