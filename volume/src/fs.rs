//! Semantic File System API
//!
//! High-level interface for semantic file operations.
//! Find files by meaning, not by path.

use crate::embedder::Embedder;
use crate::epiphany::EpiphanyBuffer;
use crate::indexer::HNSWIndexer;
use crate::types::*;
use crate::vector_store::VectorStore;
use anyhow::{Context, Result};
use notify::{Event, RecommendedWatcher, RecursiveMode};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use walkdir::WalkDir;

/// The Semantic File System
pub struct SemanticFS {
    /// Vector store for embeddings
    store: Arc<VectorStore>,
    /// HNSW indexer for search
    indexer: Arc<HNSWIndexer>,
    /// Embedder for new files
    embedder: Arc<Embedder>,
    /// Epiphany buffer for autonomous ideas
    epiphany: Arc<EpiphanyBuffer>,
    /// Configuration
    config: VolumeConfig,
    /// File watcher
    watcher: Option<RecommendedWatcher>,
}

impl SemanticFS {
    /// Create a new Semantic File System
    pub async fn new(config: VolumeConfig) -> Result<Self> {
        log::info!("Initializing Semantic File System");
        log::info!("Database path: {}", config.db_path.display());

        // Create directories
        std::fs::create_dir_all(&config.db_path)?;

        // Initialize components
        let store = Arc::new(VectorStore::new(&config.db_path.join("vectors"))?);

        let indexer = Arc::new(HNSWIndexer::new(
            config.embedding_dim,
            config.hnsw_m,
            config.hnsw_ef_construction,
        ));

        let embedder = Arc::new(Embedder::new(
            config.embedding_model.clone(),
            config.embedding_dim,
        ).await?);

        let epiphany = Arc::new(EpiphanyBuffer::new(
            &config.db_path,
            100, // buffer size
        )?);

        // Build initial index
        indexer.build_index(&store)?;

        Ok(Self {
            store,
            indexer,
            embedder,
            epiphany,
            config,
            watcher: None,
        })
    }

    /// Add a file to the semantic file system
    pub async fn add_file(&self, path: &Path) -> Result<FileId> {
        log::info!("Adding file: {}", path.display());

        // Check file size
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > self.config.max_file_size {
            anyhow::bail!("File too large: {} bytes", metadata.len());
        }

        // Generate embedding
        let embedding = self.embedder.embed_file(path).await?;

        // Create metadata
        let file_id = FileId::from_hash(path.to_str().unwrap().as_bytes());
        let content_preview = self.generate_preview(path)?;

        let doc = Document {
            metadata: FileMetadata {
                id: file_id,
                path: path.to_path_buf(),
                file_type: FileType::from_extension(
                    path.extension().and_then(|e| e.to_str()).unwrap_or("")
                ),
                size: metadata.len(),
                created: metadata.created()?.duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                modified: metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                content_hash: self.hash_file(path)?,
                tags: self.auto_tag(path),
            },
            embedding,
            content_preview,
        };

        // Store in vector store
        self.store.store(doc.clone())?;

        // Add to index
        self.indexer.add_document(&doc)?;

        log::info!("Successfully added file: {} (ID: {:?})", path.display(), file_id);

        Ok(file_id)
    }

    /// Search for files by semantic meaning
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        log::debug!("Searching for: {}", query);

        // Embed the query
        let query_embedding = self.embedder.embed_query(query).await?;

        // Create search query
        let search_query = SearchQuery {
            query: Query::Embedding(query_embedding),
            limit,
            threshold: 0.3,
            file_type: None,
            tags: Vec::new(),
        };

        // Search using HNSW
        let results = self.indexer.search_query(&self.store, &search_query)?;

        log::debug!("Found {} results", results.len());

        Ok(results)
    }

    /// Find similar files to a given file
    pub async fn find_similar(&self, file_id: FileId, limit: usize) -> Result<Vec<SearchResult>> {
        // Get the document
        let doc = self.store.get(file_id)?
            .context("File not found")?;

        // Search using its embedding
        let search_query = SearchQuery {
            query: Query::Embedding(doc.embedding),
            limit: limit + 1, // +1 because the file itself will be in results
            threshold: 0.3,
            file_type: None,
            tags: Vec::new(),
        };

        let mut results = self.indexer.search_query(&self.store, &search_query)?;

        // Remove the query file itself
        results.retain(|r| r.document.metadata.id != file_id);

        Ok(results)
    }

    /// Index an entire directory
    pub async fn index_directory(&self, dir: &Path) -> Result<usize> {
        log::info!("Indexing directory: {}", dir.display());

        let mut count = 0;

        for entry in WalkDir::new(dir).follow_links(true) {
            let entry = entry?;

            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

            // Skip hidden files
            if path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }

            match self.add_file(path).await {
                Ok(_) => {
                    count += 1;
                    if count % 10 == 0 {
                        log::info!("Indexed {} files...", count);
                    }
                }
                Err(e) => {
                    log::warn!("Failed to index {}: {}", path.display(), e);
                }
            }
        }

        log::info!("Successfully indexed {} files", count);

        Ok(count)
    }

    /// Get file by ID
    pub fn get_file(&self, file_id: FileId) -> Result<Option<Document>> {
        self.store.get(file_id)
    }

    /// Delete a file from the system
    pub fn delete_file(&self, file_id: FileId) -> Result<bool> {
        self.store.delete(file_id)
    }

    /// Rebuild the index (after bulk changes)
    pub fn rebuild_index(&self) -> Result<()> {
        log::info!("Rebuilding index...");
        self.indexer.build_index(&self.store)?;
        Ok(())
    }

    /// Add an epiphany (autonomous idea)
    pub fn add_epiphany(&self, content: String, sources: Vec<FileId>) -> Result<u64> {
        let epiphany = self.epiphany.add(content, sources)?;
        Ok(epiphany.id)
    }

    /// Validate an epiphany
    pub async fn validate_epiphany(&self, id: u64) -> Result<bool> {
        self.epiphany.validate(id).await
    }

    /// Get statistics
    pub fn stats(&self) -> FSStats {
        FSStats {
            vector_store: self.store.stats(),
            index: self.indexer.stats(),
            epiphany: self.epiphany.stats(),
        }
    }

    // Helper methods

    fn generate_preview(&self, path: &Path) -> Result<String> {
        let content = std::fs::read_to_string(path)?;
        let preview_len = 200.min(content.len());
        Ok(content[..preview_len].to_string())
    }

    fn hash_file(&self, path: &Path) -> Result<String> {
        let content = std::fs::read(path)?;
        let hash = blake3::hash(&content);
        Ok(hash.to_hex().to_string())
    }

    fn auto_tag(&self, path: &Path) -> Vec<String> {
        let mut tags = Vec::new();

        // Add file extension as tag
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            tags.push(ext.to_string());
        }

        // Add directory name as tag
        if let Some(parent) = path.parent() {
            if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
                tags.push(dir_name.to_string());
            }
        }

        tags
    }
}

#[derive(Debug, Clone)]
pub struct FSStats {
    pub vector_store: crate::vector_store::StoreStats,
    pub index: crate::indexer::IndexStats,
    pub epiphany: crate::epiphany::EpiphanyStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    #[tokio::test]
    async fn test_add_and_search() {
        let dir = tempdir().unwrap();
        let config = VolumeConfig {
            db_path: dir.path().to_path_buf(),
            ..Default::default()
        };

        let fs = SemanticFS::new(config).await.unwrap();

        // Create a test file
        let test_file = dir.path().join("test.txt");
        fs::write(&test_file, "This is a test file about Rust programming").unwrap();

        // Add file
        let _file_id = fs.add_file(&test_file).await.unwrap();

        // Search
        let results = fs.search("Rust programming", 5).await.unwrap();
        assert!(!results.is_empty());
    }
}
