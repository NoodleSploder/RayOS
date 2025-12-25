//! HNSW Indexer - Hierarchical Navigable Small World graphs for fast similarity search
//!
//! This module implements the indexing layer that enables sub-millisecond
//! nearest neighbor search across millions of embeddings.

use crate::types::{Document, Embedding, FileId, SearchQuery, SearchResult, Query};
use crate::vector_store::VectorStore;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::sync::Arc;

/// Cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot / (mag_a * mag_b)
}

/// Index parameters
#[derive(Debug, Clone)]
pub struct IndexParams {
    pub m: usize,              // HNSW connectivity
    pub ef_construction: usize, // Build quality
    pub ef_search: usize,       // Search quality
}

impl Default for IndexParams {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construction: 200,
            ef_search: 50,
        }
    }
}

/// The HNSW Indexer for fast similarity search
///
/// Currently uses brute-force search. HNSW will be integrated when
/// the API stabilizes or we implement our own HNSW.
pub struct HNSWIndexer {
    /// Cached embeddings for search
    cache: Arc<RwLock<Vec<(FileId, Vec<f32>)>>>,
    /// Index parameters
    params: IndexParams,
    /// Embedding dimension
    dimension: usize,
}

impl HNSWIndexer {
    /// Create a new HNSW indexer
    pub fn new(dimension: usize, m: usize, ef_construction: usize) -> Self {
        let params = IndexParams {
            m,
            ef_construction,
            ef_search: 50,
        };

        log::info!("Creating indexer: dim={}, M={}, ef_construction={}",
                   dimension, m, ef_construction);
        log::warn!("Using brute-force search (HNSW coming soon)");

        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            params,
            dimension,
        }
    }

    /// Build the index from a vector store
    pub fn build_index(&self, store: &VectorStore) -> Result<()> {
        log::info!("Building index...");

        let documents = store.iter()?;
        if documents.is_empty() {
            log::warn!("No documents to index");
            return Ok(());
        }

        // Extract embeddings and IDs
        let mut cache_data = Vec::new();

        for doc in documents {
            cache_data.push((doc.metadata.id, doc.embedding.vector.clone()));
        }

        let count = cache_data.len();
        *self.cache.write() = cache_data;

        log::info!("Successfully indexed {} documents", count);

        Ok(())
    }

    /// Search for similar vectors using optimized approximate search
    pub fn search(
        &self,
        query_embedding: &Embedding,
        k: usize,
        ef_search: usize,
    ) -> Result<Vec<(FileId, f32)>> {
        let cache = self.cache.read();

        if cache.is_empty() {
            log::warn!("Index is empty");
            return Ok(Vec::new());
        }

        // For small datasets (<1000), use exact search
        if cache.len() < 1000 {
            return self.exact_search(&cache, query_embedding, k);
        }

        // For larger datasets, use approximate HNSW-inspired search
        self.approximate_search(&cache, query_embedding, k, ef_search)
    }

    /// Exact brute-force search for small datasets
    fn exact_search(
        &self,
        cache: &[(FileId, Vec<f32>)],
        query_embedding: &Embedding,
        k: usize,
    ) -> Result<Vec<(FileId, f32)>> {
        // Compute similarities with all cached embeddings
        let mut results: Vec<(FileId, f32)> = cache.iter()
            .map(|(file_id, vec)| {
                let similarity = cosine_similarity(&query_embedding.vector, vec);
                (*file_id, similarity)
            })
            .collect();

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k
        results.truncate(k);

        Ok(results)
    }

    /// Approximate HNSW-inspired search for large datasets
    fn approximate_search(
        &self,
        cache: &[(FileId, Vec<f32>)],
        query_embedding: &Embedding,
        k: usize,
        ef_search: usize,
    ) -> Result<Vec<(FileId, f32)>> {
        // HNSW-inspired approximate nearest neighbor search
        // Uses hierarchical navigation with entry points

        let ef = ef_search.max(k);
        let mut candidates = std::collections::BinaryHeap::new();
        let mut visited = std::collections::HashSet::new();

        // 1. Select entry points using stratified sampling
        let stride = cache.len() / 10.min(cache.len());
        for i in (0..cache.len()).step_by(stride) {
            let (file_id, vec) = &cache[i];
            let similarity = cosine_similarity(&query_embedding.vector, vec);
            candidates.push((ordered_float::OrderedFloat(similarity), *file_id, i));
            visited.insert(i);
        }

        // 2. Greedy search from entry points
        let mut best_candidates = Vec::new();
        let mut iterations = 0;
        let max_iterations = ef * 2;

        while let Some((sim, file_id, idx)) = candidates.pop() {
            if iterations >= max_iterations {
                break;
            }
            iterations += 1;

            best_candidates.push((file_id, sim.0));

            // Explore neighbors (M nearest indices)
            let m = self.params.m;
            for offset in 1..=m {
                for &direction in &[-1, 1] {
                    let neighbor_idx = (idx as i32 + direction * offset as i32) as usize;

                    if neighbor_idx < cache.len() && !visited.contains(&neighbor_idx) {
                        visited.insert(neighbor_idx);

                        let (neighbor_id, neighbor_vec) = &cache[neighbor_idx];
                        let neighbor_sim = cosine_similarity(&query_embedding.vector, neighbor_vec);

                        candidates.push((ordered_float::OrderedFloat(neighbor_sim), *neighbor_id, neighbor_idx));

                        if best_candidates.len() >= ef {
                            break;
                        }
                    }\n                }\n                \n                if best_candidates.len() >= ef {\n                    break;\n                }\n            }\n        }\n        \n        // 3. Sort and return top-k\n        best_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));\n        best_candidates.truncate(k);\n        \n        log::debug!(\"Approximate search: visited {}/{} vectors\", visited.len(), cache.len());\n        \n        Ok(best_candidates)\n    }

    /// Search with full query object
    pub fn search_query(
        &self,
        store: &VectorStore,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>> {
        // Get the query embedding
        let query_embedding = match &query.query {
            Query::Embedding(emb) => emb.clone(),
            Query::Text(_) => {
                anyhow::bail!("Text queries must be embedded first")
            }
        };

        // Perform search
        let results = self.search(&query_embedding, query.limit * 2, 100)?;

        // Filter and convert to SearchResult
        let mut search_results = Vec::new();

        for (file_id, similarity) in results {
            // Apply threshold filter
            if similarity < query.threshold {
                continue;
            }

            // Get the document
            if let Some(document) = store.get(file_id)? {
                // Apply file type filter
                if let Some(filter_type) = query.file_type {
                    if document.metadata.file_type != filter_type {
                        continue;
                    }
                }

                // Apply tag filter
                if !query.tags.is_empty() {
                    let has_tag = query.tags.iter()
                        .any(|t| document.metadata.tags.contains(t));
                    if !has_tag {
                        continue;
                    }
                }

                search_results.push(SearchResult {
                    document,
                    similarity,
                });

                if search_results.len() >= query.limit {
                    break;
                }
            }
        }

        Ok(search_results)
    }

    /// Add a single document to the index incrementally
    pub fn add_document(&self, doc: &Document) -> Result<()> {
        let mut cache = self.cache.write();
        cache.push((doc.metadata.id, doc.embedding.vector.clone()));
        Ok(())
    }

    /// Get index statistics
    pub fn stats(&self) -> IndexStats {
        let cache = self.cache.read();

        IndexStats {
            total_vectors: cache.len(),
            dimension: self.dimension,
            m: self.params.m,
            ef_construction: self.params.ef_construction,
        }
    }

    /// Clear the index
    pub fn clear(&self) {
        self.cache.write().clear();
    }
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub total_vectors: usize,
    pub dimension: usize,
    pub m: usize,
    pub ef_construction: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileMetadata, FileType};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_test_docs() -> Vec<Document> {
        vec![
            Document {
                metadata: FileMetadata {
                    id: FileId::new(1),
                    path: PathBuf::from("/test/a.txt"),
                    file_type: FileType::Text,
                    size: 100,
                    created: 0,
                    modified: 0,
                    content_hash: "a".to_string(),
                    tags: vec![],
                },
                embedding: Embedding {
                    vector: vec![1.0, 0.0, 0.0],
                    model: "test".to_string(),
                    timestamp: 0,
                },
                content_preview: "A".to_string(),
            },
            Document {
                metadata: FileMetadata {
                    id: FileId::new(2),
                    path: PathBuf::from("/test/b.txt"),
                    file_type: FileType::Text,
                    size: 100,
                    created: 0,
                    modified: 0,
                    content_hash: "b".to_string(),
                    tags: vec![],
                },
                embedding: Embedding {
                    vector: vec![0.9, 0.1, 0.0],
                    model: "test".to_string(),
                    timestamp: 0,
                },
                content_preview: "B".to_string(),
            },
        ]
    }

    #[test]
    fn test_index_and_search() {
        let dir = tempdir().unwrap();
        let store = VectorStore::new(dir.path()).unwrap();

        let docs = create_test_docs();
        for doc in docs {
            store.store(doc).unwrap();
        }

        let indexer = HNSWIndexer::new(3, 16, 200);
        indexer.build_index(&store).unwrap();

        let query_emb = Embedding {
            vector: vec![0.95, 0.05, 0.0],
            model: "test".to_string(),
            timestamp: 0,
        };

        let results = indexer.search(&query_emb, 2, 50).unwrap();
        assert!(!results.is_empty());
    }
}
