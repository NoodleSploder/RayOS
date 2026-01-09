//! Vector Store - The "Hippocampus" of RayOS
//!
//! Stores embeddings in a persistent, GPU-accessible format.
//! Organizes vectors so semantically similar content is physically co-located.

use crate::types::{Document, Embedding, FileId};
use anyhow::{Context, Result};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// The Vector Store manages embedding storage and retrieval
pub struct VectorStore {
    /// Persistent key-value store for metadata
    db: sled::Db,
    /// In-memory cache of embeddings for fast access
    cache: Arc<DashMap<FileId, Document>>,
    /// Statistics
    stats: Arc<RwLock<StoreStats>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoreStats {
    pub total_documents: usize,
    pub total_embeddings: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub bytes_stored: u64,
}

impl VectorStore {
    /// Create or open a vector store
    pub fn new(db_path: &Path) -> Result<Self> {
        log::info!("Opening vector store at: {}", db_path.display());

        let db = sled::open(db_path)
            .context("Failed to open sled database")?;

        Ok(Self {
            db,
            cache: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(StoreStats::default())),
        })
    }

    /// Store a document with its embedding
    pub fn store(&self, document: Document) -> Result<()> {
        let file_id = document.metadata.id;

        // Serialize the document
        let serialized = bincode::serialize(&document)
            .context("Failed to serialize document")?;

        // Store in persistent database
        self.db.insert(
            file_id.0.to_le_bytes(),
            serialized.clone()
        ).context("Failed to insert into database")?;

        // Update cache
        self.cache.insert(file_id, document);

        // Update stats
        let mut stats = self.stats.write();
        stats.total_documents += 1;
        stats.total_embeddings += 1;
        stats.bytes_stored += serialized.len() as u64;

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

        let removed = self.db.remove(file_id.0.to_le_bytes())
            .context("Failed to delete from database")?;

        if removed.is_some() {
            self.stats.write().total_documents -= 1;
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
        self.stats.read().clone()
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
