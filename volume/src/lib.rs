//! # RayOS Volume - Phase 3: The Memory
//!
//! A semantic file system that organizes data by meaning, not location.
//! Files are automatically embedded into a vector space where similarity
//! corresponds to semantic relatedness.
//!
//! ## Architecture
//!
//! - **Embedder**: Converts files into high-dimensional vectors
//! - **Vector Store**: Persistent storage for embeddings
//! - **HNSW Indexer**: Fast similarity search across millions of embeddings
//! - **Semantic FS**: High-level API for file operations
//! - **Epiphany Buffer**: Autonomous idea generation and validation
//!
//! ## Example
//!
//! ```no_run
//! use rayos_volume::{SemanticFS, VolumeConfig};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create a new semantic file system
//!     let config = VolumeConfig::default();
//!     let fs = SemanticFS::new(config).await?;
//!
//!     // Index a directory
//!     fs.index_directory(Path::new("./docs")).await?;
//!
//!     // Search by meaning
//!     let results = fs.search("machine learning algorithms", 10).await?;
//!
//!     for result in results {
//!         println!("{}: {:.2}", result.document.metadata.path.display(), result.similarity);
//!     }
//!
//!     Ok(())
//! }
//! ```

pub mod embedder;
pub mod epiphany;
pub mod fs;
pub mod indexer;
pub mod types;
pub mod vector_store;

pub use embedder::Embedder;
pub use epiphany::EpiphanyBuffer;
pub use fs::SemanticFS;
pub use indexer::HNSWIndexer;
pub use types::*;
pub use vector_store::VectorStore;

// Re-export for convenience
pub use anyhow::Result;
