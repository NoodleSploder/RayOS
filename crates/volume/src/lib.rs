//! # RayOS Volume - Phase 3: The Memory
//!
//! A semantic file system that organizes data by meaning, not location.
//! Files are automatically embedded into a vector space where similarity
//! corresponds to semantic relatedness.
//!
//! ## Architecture
//!
//! - **Embedder**: Converts files into high-dimensional vectors
//! - **Multi-Modal Embedder**: Unified embedding for text, code, images, and audio
//! - **Vector Store**: Persistent storage for embeddings
//! - **HNSW Indexer**: Fast similarity search across millions of embeddings
//! - **GPU Search**: Optional GPU-accelerated similarity search (with `gpu` feature)
//! - **Semantic FS**: High-level API for file operations
//! - **Epiphany Buffer**: Autonomous idea generation and validation
//!
//! ## GPU Acceleration
//!
//! When compiled with `--features gpu`, the indexer can use GPU compute shaders
//! to perform parallel similarity search across all vectors simultaneously.
//! This provides massive speedups for large datasets (10-100x faster than CPU).
//!
//! ```bash
//! cargo build --features gpu
//! ```
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
pub mod gpu_search;
pub mod hnsw;
pub mod indexer;
pub mod ingestion;
pub mod multimodal;
pub mod query_interface;
pub mod relationship_graph;
pub mod types;
pub mod vector_store;

pub use embedder::Embedder;
pub use epiphany::{
    EpiphanyBuffer, EpiphanyStats, ConnectionGenerator, ConnectionConfig,
    DreamScheduler, DreamConfig, DreamStats, PromotionEngine,
};
pub use fs::SemanticFS;
pub use hnsw::{HnswIndex, HnswConfig, HnswStats, DistanceMetric, SearchResult as HnswSearchResult};
pub use indexer::HNSWIndexer;
pub use ingestion::{IngestionPipeline, IngestionConfig, IngestionMetrics, IngestionEvent, IngestionEventKind};
pub use multimodal::{MultiModalEmbedder, Modality, CodeLanguage, ImageFeatures, AudioFeatures};
pub use query_interface::{
    SemanticQueryEngine, QueryParser, QueryExpander, MultiFactorRanker,
    QueryContext, ParsedQuery, QueryIntent, QueryResponse, FormattedResult,
    RankingWeights, RankingScore, TimeRange,
};
pub use relationship_graph::{
    KnowledgeGraph, ConceptNode, NodeType, RelationEdge, RelatedNode,
    GraphStats, InferenceEngine, InferenceConfig, GraphQuery,
};
pub use types::*;
pub use vector_store::{VectorStore, SemanticSearchResult};

#[cfg(feature = "gpu")]
pub use gpu_search::GpuSearchEngine;

// Re-export for convenience
pub use anyhow::Result;
