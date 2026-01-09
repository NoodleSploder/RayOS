//! Vector Store - The "Hippocampus" of RayOS
//!
//! Stores embeddings in a persistent, GPU-accessible format with HNSW indexing
//! for O(log n) approximate nearest neighbor search. Organizes vectors so
//! semantically similar content is physically co-located.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        Vector Store (Hippocampus)                        │
//! │                                                                          │
//! │  ┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐        │
//! │  │  Sled Database  │   │   HNSW Index    │   │  Memory Cache   │        │
//! │  │                 │   │                 │   │                 │        │
//! │  │  • Documents    │   │  • Fast ANN     │   │  • Hot data     │        │
//! │  │  • Metadata     │   │  • O(log n)     │   │  • LRU eviction │        │
//! │  │  • Persistence  │   │  • High recall  │   │  • Zero-copy    │        │
//! │  └────────┬────────┘   └────────┬────────┘   └────────┬────────┘        │
//! │           │                     │                     │                  │
//! │           └─────────────────────┴─────────────────────┘                  │
//! │                                 │                                        │
//! │                    ┌────────────▼────────────┐                           │
//! │                    │     Semantic Search     │                           │
//! │                    │   (GPU + HNSW hybrid)   │                           │
//! │                    └─────────────────────────┘                           │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

use crate::hnsw::{HnswConfig, HnswIndex, SearchResult as HnswSearchResult};
use crate::types::{Document, Embedding, FileId};
use anyhow::{Context, Result};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// The Vector Store manages embedding storage and retrieval with HNSW indexing
pub struct VectorStore {
    /// Persistent key-value store for metadata
    db: sled::Db,
    /// In-memory cache of embeddings for fast access
    cache: Arc<DashMap<FileId, Document>>,
    /// HNSW index for approximate nearest neighbor search
    hnsw_index: Arc<RwLock<HnswIndex>>,
    /// Statistics
    stats: Arc<RwLock<StoreStats>>,
    /// Store path for index persistence
    store_path: std::path::PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoreStats {
    pub total_documents: usize,
    pub total_embeddings: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub bytes_stored: u64,
    pub hnsw_searches: u64,
    pub hnsw_index_size: usize,
}

/// Result from semantic search
#[derive(Debug, Clone)]
pub struct SemanticSearchResult {
    /// The matching document
    pub document: Document,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,
    /// Distance from query
    pub distance: f32,
}

impl VectorStore {
    /// Create or open a vector store with dynamic dimension HNSW
    pub fn new(db_path: &Path) -> Result<Self> {
        // Use dimension 0 for dynamic dimension (inferred from first vector)
        let config = HnswConfig {
            dimension: 0,
            ..HnswConfig::default()
        };
        Self::with_config(db_path, config)
    }

    /// Create with custom HNSW configuration
    pub fn with_config(db_path: &Path, hnsw_config: HnswConfig) -> Result<Self> {
        log::info!("Opening vector store at: {}", db_path.display());

        let db = sled::open(db_path)
            .context("Failed to open sled database")?;

        // Try to load existing HNSW index
        let index_path = db_path.join("hnsw.index");
        let hnsw_index = if index_path.exists() {
            match std::fs::read(&index_path) {
                Ok(data) => {
                    match HnswIndex::deserialize(&data) {
                        Ok(index) => {
                            log::info!("Loaded HNSW index with {} vectors", index.len());
                            index
                        }
                        Err(e) => {
                            log::warn!("Failed to load HNSW index: {}, creating new", e);
                            HnswIndex::new(hnsw_config)
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to read HNSW index file: {}, creating new", e);
                    HnswIndex::new(hnsw_config)
                }
            }
        } else {
            HnswIndex::new(hnsw_config)
        };

        Ok(Self {
            db,
            cache: Arc::new(DashMap::new()),
            hnsw_index: Arc::new(RwLock::new(hnsw_index)),
            stats: Arc::new(RwLock::new(StoreStats::default())),
            store_path: db_path.to_path_buf(),
        })
    }

    /// Store a document with its embedding
    pub fn store(&self, document: Document) -> Result<()> {
        let file_id = document.metadata.id;
        let embedding_vector = document.embedding.vector.clone();

        // Serialize the document
        let serialized = bincode::serialize(&document)
            .context("Failed to serialize document")?;

        // Store in persistent database
        self.db.insert(
            file_id.0.to_le_bytes(),
            serialized.clone()
        ).context("Failed to insert into database")?;

        // Add to HNSW index
        {
            let mut index = self.hnsw_index.write();
            // Remove old entry if exists
            index.remove(file_id.0);
            // Insert new embedding
            if !embedding_vector.is_empty() {
                index.insert(file_id.0, &embedding_vector);
            }
        }

        // Update cache
        self.cache.insert(file_id, document);

        // Update stats
        let mut stats = self.stats.write();
        stats.total_documents += 1;
        stats.total_embeddings += 1;
        stats.bytes_stored += serialized.len() as u64;
        stats.hnsw_index_size = self.hnsw_index.read().len();

        Ok(())
    }

    /// Retrieve a document by ID
    pub fn get(&self, file_id: FileId) -> Result<Option<Document>> {
        // Check cache first
        if let Some(doc) = self.cache.get(&file_id) {
            self.stats.write().cache_hits += 1;
            return Ok(Some(doc.clone()));
        }

        self.stats.write().cache_misses += 1;

        // Load from disk
        let bytes = self.db.get(file_id.0.to_le_bytes())
            .context("Failed to read from database")?;

        match bytes {
            Some(data) => {
                let document: Document = bincode::deserialize(&data)
                    .context("Failed to deserialize document")?;

                // Populate cache
                self.cache.insert(file_id, document.clone());

                Ok(Some(document))
            }
            None => Ok(None),
        }
    }

    /// Delete a document
    pub fn delete(&self, file_id: FileId) -> Result<bool> {
        self.cache.remove(&file_id);

        // Remove from HNSW index
        self.hnsw_index.write().remove(file_id.0);

        let removed = self.db.remove(file_id.0.to_le_bytes())
            .context("Failed to delete from database")?;

        if removed.is_some() {
            let mut stats = self.stats.write();
            stats.total_documents -= 1;
            stats.hnsw_index_size = self.hnsw_index.read().len();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Update a document's embedding
    pub fn update(&self, file_id: FileId, embedding: Embedding) -> Result<()> {
        if let Some(mut doc) = self.get(file_id)? {
            doc.embedding = embedding;
            self.store(doc)?;
            Ok(())
        } else {
            anyhow::bail!("Document not found: {:?}", file_id)
        }
    }

    /// Get all documents (for indexing)
    pub fn iter(&self) -> Result<Vec<Document>> {
        let mut documents = Vec::new();

        for item in self.db.iter() {
            let (_key, value) = item.context("Failed to iterate database")?;
            let document: Document = bincode::deserialize(&value)
                .context("Failed to deserialize document")?;
            documents.push(document);
        }

        Ok(documents)
    }

    /// Get all file IDs in the store
    pub fn all_ids(&self) -> Result<Vec<FileId>> {
        let mut ids = Vec::new();
        for item in self.db.iter() {
            let (key, _) = item?;
            if key.len() == 8 {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&key);
                ids.push(FileId(u64::from_le_bytes(bytes)));
            }
        }
        Ok(ids)
    }

    /// Get all embeddings as a matrix (for GPU processing)
    pub fn get_embedding_matrix(&self) -> Result<(Vec<FileId>, Vec<Vec<f32>>)> {
        let documents = self.iter()?;
        let mut ids = Vec::new();
        let mut embeddings = Vec::new();

        for doc in documents {
            ids.push(doc.metadata.id);
            embeddings.push(doc.embedding.vector);
        }

        Ok((ids, embeddings))
    }

    /// Get store statistics
    pub fn stats(&self) -> StoreStats {
        let mut stats = self.stats.read().clone();
        stats.hnsw_index_size = self.hnsw_index.read().len();
        stats
    }

    /// Semantic search: find similar documents using HNSW index
    ///
    /// Returns documents ranked by similarity to the query embedding.
    pub fn semantic_search(&self, query: &[f32], k: usize) -> Result<Vec<SemanticSearchResult>> {
        // Update stats
        self.stats.write().hnsw_searches += 1;

        // Search HNSW index
        let results = self.hnsw_index.read().search(query, k);

        // Fetch documents for results
        let mut search_results = Vec::with_capacity(results.len());
        for result in results {
            let file_id = FileId(result.id);
            if let Some(document) = self.get(file_id)? {
                search_results.push(SemanticSearchResult {
                    document,
                    similarity: result.similarity,
                    distance: result.distance,
                });
            }
        }

        Ok(search_results)
    }

    /// Find similar documents to a given document
    pub fn find_similar(&self, file_id: FileId, k: usize) -> Result<Vec<SemanticSearchResult>> {
        let doc = self.get(file_id)?
            .context("Document not found")?;

        // Search for k+1 to exclude the query document itself
        let mut results = self.semantic_search(&doc.embedding.vector, k + 1)?;

        // Remove the query document from results
        results.retain(|r| r.document.metadata.id != file_id);
        results.truncate(k);

        Ok(results)
    }

    /// Rebuild the HNSW index from stored documents
    ///
    /// Use this after bulk loading or if the index becomes corrupted.
    pub fn rebuild_index(&self) -> Result<usize> {
        log::info!("Rebuilding HNSW index...");

        let documents = self.iter()?;
        let count = documents.len();

        // Clear and rebuild
        {
            let mut index = self.hnsw_index.write();
            index.clear();

            for doc in &documents {
                if !doc.embedding.vector.is_empty() {
                    index.insert(doc.metadata.id.0, &doc.embedding.vector);
                }
            }
        }

        log::info!("HNSW index rebuilt with {} vectors", count);
        self.stats.write().hnsw_index_size = count;

        Ok(count)
    }

    /// Persist the HNSW index to disk
    pub fn save_index(&self) -> Result<()> {
        let index_path = self.store_path.join("hnsw.index");
        let data = self.hnsw_index.read().serialize()?;
        std::fs::write(&index_path, data)
            .context("Failed to write HNSW index")?;
        log::info!("HNSW index saved to {}", index_path.display());
        Ok(())
    }

    /// Get HNSW index statistics
    pub fn hnsw_stats(&self) -> crate::hnsw::HnswStats {
        self.hnsw_index.read().stats()
    }

    /// Compact the database (reclaim space)
    pub fn compact(&self) -> Result<()> {
        log::info!("Compacting vector store...");
        // Sled doesn't expose a compact method, but flush helps
        self.db.flush().context("Failed to flush database")?;
        Ok(())
    }

    /// Clear all data (dangerous!)
    pub fn clear(&self) -> Result<()> {
        log::warn!("Clearing all vector store data!");
        self.cache.clear();
        self.hnsw_index.write().clear();
        self.db.clear().context("Failed to clear database")?;
        *self.stats.write() = StoreStats::default();
        Ok(())
    }

    /// Export embeddings for backup or transfer
    pub fn export_embeddings(&self, output_path: &Path) -> Result<()> {
        let documents = self.iter()?;
        let json = serde_json::to_string_pretty(&documents)
            .context("Failed to serialize documents")?;
        std::fs::write(output_path, json)
            .context("Failed to write export file")?;
        log::info!("Exported {} documents to {}", documents.len(), output_path.display());
        Ok(())
    }

    /// Import embeddings from backup
    pub fn import_embeddings(&self, input_path: &Path) -> Result<()> {
        let json = std::fs::read_to_string(input_path)
            .context("Failed to read import file")?;
        let documents: Vec<Document> = serde_json::from_str(&json)
            .context("Failed to parse import file")?;

        for doc in documents {
            self.store(doc)?;
        }

        log::info!("Imported documents from {}", input_path.display());
        Ok(())
    }
}

impl Drop for VectorStore {
    fn drop(&mut self) {
        // Save HNSW index
        if let Err(e) = self.save_index() {
            log::error!("Failed to save HNSW index on drop: {}", e);
        }
        // Flush database
        if let Err(e) = self.db.flush() {
            log::error!("Failed to flush database on drop: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileId, FileMetadata, FileType};
    use tempfile::tempdir;

    fn create_test_document() -> Document {
        use std::path::PathBuf;

        Document {
            metadata: FileMetadata {
                id: FileId::new(1),
                path: PathBuf::from("/test/file.txt"),
                file_type: FileType::Text,
                size: 100,
                created: 0,
                modified: 0,
                content_hash: "abc123".to_string(),
                tags: vec!["test".to_string()],
            },
            embedding: Embedding {
                vector: vec![0.1, 0.2, 0.3],
                model: "test".to_string(),
                timestamp: 0,
            },
            content_preview: "Test content".to_string(),
        }
    }

    #[test]
    fn test_store_and_retrieve() {
        let dir = tempdir().unwrap();
        let store = VectorStore::new(dir.path()).unwrap();

        let doc = create_test_document();
        let file_id = doc.metadata.id;

        store.store(doc.clone()).unwrap();

        let retrieved = store.get(file_id).unwrap().unwrap();
        assert_eq!(retrieved.metadata.id, file_id);
    }

    #[test]
    fn test_delete() {
        let dir = tempdir().unwrap();
        let store = VectorStore::new(dir.path()).unwrap();

        let doc = create_test_document();
        let file_id = doc.metadata.id;

        store.store(doc).unwrap();
        assert!(store.delete(file_id).unwrap());
        assert!(store.get(file_id).unwrap().is_none());
    }
}
