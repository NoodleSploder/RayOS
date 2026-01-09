//! HNSW (Hierarchical Navigable Small World) Index
//!
//! Implements an approximate nearest neighbor (ANN) search index for the
//! Vector Store. HNSW provides O(log n) search time with high recall,
//! making it ideal for semantic similarity search across large embedding collections.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         HNSW Index                                       │
//! │                                                                          │
//! │  Layer 3 (sparse):     [A]─────────────────────[B]                      │
//! │                          │                       │                       │
//! │  Layer 2:              [A]────[C]───────────[B]─[D]                     │
//! │                          │      │             │   │                      │
//! │  Layer 1:              [A]─[E]─[C]─[F]─────[B]─[D]─[G]                   │
//! │                          │  │   │   │       │   │   │                    │
//! │  Layer 0 (dense):      [A][E][C][F][H][I][B][D][G][J][K]...             │
//! │                                                                          │
//! │  Navigation: Start at top layer, greedily descend toward query          │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_volume::hnsw::{HnswIndex, HnswConfig};
//!
//! let config = HnswConfig::default();
//! let mut index = HnswIndex::new(config);
//!
//! // Insert vectors
//! index.insert(0, &[0.1, 0.2, 0.3, 0.4]);
//! index.insert(1, &[0.2, 0.3, 0.4, 0.5]);
//!
//! // Search for nearest neighbors
//! let query = &[0.15, 0.25, 0.35, 0.45];
//! let results = index.search(query, 10);
//! ```

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering};

// =============================================================================
// Configuration
// =============================================================================

/// HNSW index configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HnswConfig {
    /// Maximum number of connections per node at layer 0
    pub m0: usize,
    /// Maximum number of connections per node at higher layers
    pub m: usize,
    /// Size of the dynamic candidate list during construction
    pub ef_construction: usize,
    /// Size of the dynamic candidate list during search
    pub ef_search: usize,
    /// Level generation factor (controls layer distribution)
    pub ml: f64,
    /// Vector dimension
    pub dimension: usize,
    /// Distance metric
    pub metric: DistanceMetric,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m0: 32,           // More connections at base layer
            m: 16,            // Fewer at higher layers
            ef_construction: 200,
            ef_search: 50,
            ml: 1.0 / (16.0_f64).ln(),  // ln(M)
            dimension: 768,   // Common embedding size
            metric: DistanceMetric::Cosine,
        }
    }
}

impl HnswConfig {
    /// Create config optimized for speed
    pub fn fast(dimension: usize) -> Self {
        Self {
            m0: 24,
            m: 12,
            ef_construction: 100,
            ef_search: 20,
            dimension,
            ..Default::default()
        }
    }

    /// Create config optimized for accuracy
    pub fn accurate(dimension: usize) -> Self {
        Self {
            m0: 48,
            m: 24,
            ef_construction: 400,
            ef_search: 100,
            dimension,
            ..Default::default()
        }
    }

    /// Create config for small datasets
    pub fn small(dimension: usize) -> Self {
        Self {
            m0: 16,
            m: 8,
            ef_construction: 50,
            ef_search: 20,
            dimension,
            ..Default::default()
        }
    }
}

/// Distance metric for similarity computation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistanceMetric {
    /// Euclidean distance (L2)
    Euclidean,
    /// Cosine similarity (converted to distance)
    Cosine,
    /// Inner product (dot product)
    InnerProduct,
}

impl DistanceMetric {
    /// Compute distance between two vectors
    pub fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        match self {
            DistanceMetric::Euclidean => euclidean_distance(a, b),
            DistanceMetric::Cosine => cosine_distance(a, b),
            DistanceMetric::InnerProduct => inner_product_distance(a, b),
        }
    }
}

// =============================================================================
// Distance Functions
// =============================================================================

/// Compute Euclidean (L2) distance
fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Compute cosine distance (1 - cosine_similarity)
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let norm = (norm_a * norm_b).sqrt();
    if norm == 0.0 {
        return 1.0; // Maximum distance for zero vectors
    }

    1.0 - (dot / norm)
}

/// Compute inner product distance (negative dot product for max-heap compatibility)
fn inner_product_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    -a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>()
}

// =============================================================================
// Node and Layer Types
// =============================================================================

/// A node in the HNSW graph
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HnswNode {
    /// Unique identifier
    id: u64,
    /// The embedding vector
    vector: Vec<f32>,
    /// Maximum layer this node exists in
    level: usize,
    /// Connections at each layer (layer -> neighbor IDs)
    connections: Vec<Vec<u64>>,
}

impl HnswNode {
    fn new(id: u64, vector: Vec<f32>, level: usize) -> Self {
        let connections = (0..=level).map(|_| Vec::new()).collect();
        Self {
            id,
            vector,
            level,
            connections,
        }
    }
}

/// A candidate for search with distance
#[derive(Clone, Debug)]
struct Candidate {
    id: u64,
    distance: f32,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (smallest distance first)
        other.distance.partial_cmp(&self.distance).unwrap_or(Ordering::Equal)
    }
}

/// Max-heap candidate (largest distance first)
#[derive(Clone, Debug)]
struct MaxCandidate {
    id: u64,
    distance: f32,
}

impl PartialEq for MaxCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for MaxCandidate {}

impl PartialOrd for MaxCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MaxCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Normal ordering for max-heap (largest distance first)
        self.distance.partial_cmp(&other.distance).unwrap_or(Ordering::Equal)
    }
}

// =============================================================================
// Search Result
// =============================================================================

/// A search result with ID and distance
#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    /// The ID of the found vector
    pub id: u64,
    /// Distance from the query vector
    pub distance: f32,
    /// Similarity score (1 - distance for cosine, varies for others)
    pub similarity: f32,
}

impl SearchResult {
    fn from_candidate(c: Candidate, metric: DistanceMetric) -> Self {
        let similarity = match metric {
            DistanceMetric::Cosine => 1.0 - c.distance,
            DistanceMetric::Euclidean => 1.0 / (1.0 + c.distance),
            DistanceMetric::InnerProduct => -c.distance,
        };
        Self {
            id: c.id,
            distance: c.distance,
            similarity,
        }
    }
}

// =============================================================================
// HNSW Index Statistics
// =============================================================================

/// Statistics about the HNSW index
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HnswStats {
    /// Total number of vectors
    pub num_vectors: usize,
    /// Maximum layer in the index
    pub max_level: usize,
    /// Total number of edges
    pub num_edges: usize,
    /// Average connections per node
    pub avg_connections: f64,
    /// Total searches performed
    pub searches: u64,
    /// Total distance computations
    pub distance_computations: u64,
}

// =============================================================================
// HNSW Index
// =============================================================================

/// The HNSW Index for approximate nearest neighbor search
pub struct HnswIndex {
    /// Configuration
    config: HnswConfig,
    /// All nodes in the index
    nodes: RwLock<HashMap<u64, HnswNode>>,
    /// Entry point (node at the highest level)
    entry_point: RwLock<Option<u64>>,
    /// Current maximum level
    max_level: AtomicUsize,
    /// Statistics
    stats: Arc<HnswStats>,
    /// Search counter
    search_count: AtomicU64,
    /// Distance computation counter
    distance_count: AtomicU64,
}

impl HnswIndex {
    /// Create a new HNSW index
    pub fn new(config: HnswConfig) -> Self {
        Self {
            config,
            nodes: RwLock::new(HashMap::new()),
            entry_point: RwLock::new(None),
            max_level: AtomicUsize::new(0),
            stats: Arc::new(HnswStats::default()),
            search_count: AtomicU64::new(0),
            distance_count: AtomicU64::new(0),
        }
    }

    /// Generate a random level for a new node
    /// Uses the formula: level = floor(-ln(uniform(0,1)) * m_L)
    fn random_level(&self) -> usize {
        let uniform: f64 = fastrand::f64();
        // Avoid ln(0) which is -infinity
        let uniform = uniform.max(f64::MIN_POSITIVE);
        let level = (-uniform.ln() * self.config.ml).floor() as usize;
        level.min(32) // Cap at max level
    }

    /// Compute distance between query and a node
    fn distance(&self, query: &[f32], node_id: u64) -> f32 {
        self.distance_count.fetch_add(1, AtomicOrdering::Relaxed);
        let nodes = self.nodes.read();
        if let Some(node) = nodes.get(&node_id) {
            self.config.metric.distance(query, &node.vector)
        } else {
            f32::INFINITY
        }
    }

    /// Search for the nearest neighbors of a query at a specific layer
    fn search_layer(
        &self,
        query: &[f32],
        entry_points: Vec<u64>,
        ef: usize,
        layer: usize,
    ) -> Vec<Candidate> {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut candidates: BinaryHeap<Candidate> = BinaryHeap::new();
        let mut results: BinaryHeap<MaxCandidate> = BinaryHeap::new();

        // Initialize with entry points
        for ep in &entry_points {
            visited.insert(*ep);
            let dist = self.distance(query, *ep);
            candidates.push(Candidate { id: *ep, distance: dist });
            results.push(MaxCandidate { id: *ep, distance: dist });
        }

        let nodes = self.nodes.read();

        while let Some(current) = candidates.pop() {
            // Check if we've explored enough
            if let Some(furthest) = results.peek() {
                if current.distance > furthest.distance {
                    break;
                }
            }

            // Explore neighbors at this layer
            if let Some(node) = nodes.get(&current.id) {
                if layer < node.connections.len() {
                    for &neighbor_id in &node.connections[layer] {
                        if visited.insert(neighbor_id) {
                            let dist = self.config.metric.distance(query,
                                &nodes.get(&neighbor_id).map(|n| &n.vector[..]).unwrap_or(&[]));
                            self.distance_count.fetch_add(1, AtomicOrdering::Relaxed);

                            // Add to candidates if promising
                            let dominated = results.len() >= ef &&
                                results.peek().map(|r| dist >= r.distance).unwrap_or(false);

                            if !dominated {
                                candidates.push(Candidate { id: neighbor_id, distance: dist });
                                results.push(MaxCandidate { id: neighbor_id, distance: dist });

                                // Keep results bounded
                                while results.len() > ef {
                                    results.pop();
                                }
                            }
                        }
                    }
                }
            }
        }

        // Convert results to candidates
        results.into_iter()
            .map(|mc| Candidate { id: mc.id, distance: mc.distance })
            .collect()
    }

    /// Select neighbors using the simple heuristic
    fn select_neighbors_simple(&self, candidates: &[Candidate], m: usize) -> Vec<u64> {
        let mut sorted: Vec<_> = candidates.to_vec();
        sorted.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(Ordering::Equal));
        sorted.into_iter().take(m).map(|c| c.id).collect()
    }

    /// Insert a vector into the index
    pub fn insert(&self, id: u64, vector: &[f32]) {
        // If dimension is 0, this is a dynamic-dimension index (infer from first vector)
        // Otherwise, validate dimension
        if self.config.dimension != 0 && vector.len() != self.config.dimension {
            // Log warning but still insert - some use cases need flexibility
            log::warn!(
                "Vector dimension mismatch: expected {}, got {}. Inserting anyway.",
                self.config.dimension, vector.len()
            );
        }

        let level = self.random_level();
        let mut node = HnswNode::new(id, vector.to_vec(), level);

        // Get current entry point and max level
        let entry_point = *self.entry_point.read();
        let current_max = self.max_level.load(AtomicOrdering::Acquire);

        match entry_point {
            None => {
                // First node - set as entry point
                self.nodes.write().insert(id, node);
                *self.entry_point.write() = Some(id);
                self.max_level.store(level, AtomicOrdering::Release);
            }
            Some(ep) => {
                let mut current_ep = vec![ep];

                // Traverse from top to insertion level (greedy search)
                for lc in ((level + 1)..=current_max).rev() {
                    let candidates = self.search_layer(vector, current_ep.clone(), 1, lc);
                    if let Some(nearest) = candidates.first() {
                        current_ep = vec![nearest.id];
                    }
                }

                // Build connections at each layer
                for lc in (0..=level.min(current_max)).rev() {
                    let ef = self.config.ef_construction;
                    let candidates = self.search_layer(vector, current_ep.clone(), ef, lc);

                    // Select neighbors
                    let m = if lc == 0 { self.config.m0 } else { self.config.m };
                    let neighbors = self.select_neighbors_simple(&candidates, m);

                    node.connections[lc] = neighbors.clone();

                    // Add bidirectional edges
                    {
                        let mut nodes = self.nodes.write();
                        for &neighbor_id in &neighbors {
                            // First, check if pruning is needed and collect data immutably
                            let prune_data: Option<(Vec<f32>, Vec<u64>, Vec<Candidate>)> = {
                                if let Some(neighbor) = nodes.get(&neighbor_id) {
                                    if lc < neighbor.connections.len() {
                                        let mut conn = neighbor.connections[lc].clone();
                                        conn.push(id);

                                        if conn.len() > m {
                                            let neighbor_vec = neighbor.vector.clone();
                                            let candidates: Vec<Candidate> = conn
                                                .iter()
                                                .map(|&nid| {
                                                    let dist = if let Some(n) = nodes.get(&nid) {
                                                        self.config.metric.distance(&neighbor_vec, &n.vector)
                                                    } else {
                                                        f32::INFINITY
                                                    };
                                                    Candidate { id: nid, distance: dist }
                                                })
                                                .collect();
                                            Some((neighbor_vec, conn, candidates))
                                        } else {
                                            Some((Vec::new(), conn, Vec::new()))
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            };

                            // Now apply the changes mutably
                            if let Some((_, conn, candidates)) = prune_data {
                                if let Some(neighbor) = nodes.get_mut(&neighbor_id) {
                                    if lc < neighbor.connections.len() {
                                        if candidates.is_empty() {
                                            // No pruning needed, just add the connection
                                            neighbor.connections[lc] = conn;
                                        } else {
                                            // Prune to keep nearest
                                            let mut sorted_candidates = candidates;
                                            sorted_candidates.sort_by(|a, b|
                                                a.distance.partial_cmp(&b.distance).unwrap_or(Ordering::Equal));
                                            neighbor.connections[lc] = sorted_candidates.into_iter()
                                                .take(m)
                                                .map(|c| c.id)
                                                .collect();
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Update entry points for next layer
                    current_ep = candidates.into_iter().map(|c| c.id).collect();
                }

                // Insert the node
                self.nodes.write().insert(id, node);

                // Update entry point if new node is at higher level
                if level > current_max {
                    *self.entry_point.write() = Some(id);
                    self.max_level.store(level, AtomicOrdering::Release);
                }
            }
        }
    }

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        self.search_count.fetch_add(1, AtomicOrdering::Relaxed);

        // Only validate dimension if config dimension is non-zero
        if self.config.dimension != 0 && query.len() != self.config.dimension {
            log::warn!(
                "Query dimension mismatch: expected {}, got {}",
                self.config.dimension, query.len()
            );
            return Vec::new();
        }

        let entry_point = *self.entry_point.read();
        let ep = match entry_point {
            Some(ep) => ep,
            None => return Vec::new(),
        };

        let max_level = self.max_level.load(AtomicOrdering::Acquire);
        let mut current_ep = vec![ep];

        // Traverse from top layer to layer 1 (greedy search)
        for lc in (1..=max_level).rev() {
            let candidates = self.search_layer(query, current_ep, 1, lc);
            current_ep = candidates.into_iter().map(|c| c.id).collect();
            if current_ep.is_empty() {
                current_ep = vec![ep];
            }
        }

        // Search at layer 0 with larger ef
        let ef = self.config.ef_search.max(k);
        let candidates = self.search_layer(query, current_ep, ef, 0);

        // Return top-k results
        let mut results: Vec<_> = candidates.into_iter()
            .map(|c| SearchResult::from_candidate(c, self.config.metric))
            .collect();
        results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(Ordering::Equal));
        results.truncate(k);
        results
    }

    /// Remove a vector from the index
    pub fn remove(&self, id: u64) -> bool {
        let mut nodes = self.nodes.write();

        if let Some(node) = nodes.remove(&id) {
            // Remove edges pointing to this node
            for lc in 0..node.connections.len() {
                for &neighbor_id in &node.connections[lc] {
                    if let Some(neighbor) = nodes.get_mut(&neighbor_id) {
                        if lc < neighbor.connections.len() {
                            neighbor.connections[lc].retain(|&x| x != id);
                        }
                    }
                }
            }

            // Update entry point if needed
            drop(nodes);
            let mut ep = self.entry_point.write();
            if *ep == Some(id) {
                let nodes = self.nodes.read();
                // Find new entry point (highest level node)
                *ep = nodes.iter()
                    .max_by_key(|(_, n)| n.level)
                    .map(|(id, _)| *id);

                if let Some(new_ep) = *ep {
                    if let Some(node) = nodes.get(&new_ep) {
                        self.max_level.store(node.level, AtomicOrdering::Release);
                    }
                } else {
                    self.max_level.store(0, AtomicOrdering::Release);
                }
            }

            true
        } else {
            false
        }
    }

    /// Get the number of vectors in the index
    pub fn len(&self) -> usize {
        self.nodes.read().len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.read().is_empty()
    }

    /// Get index statistics
    pub fn stats(&self) -> HnswStats {
        let nodes = self.nodes.read();
        let num_vectors = nodes.len();
        let max_level = self.max_level.load(AtomicOrdering::Acquire);

        let mut num_edges = 0;
        for node in nodes.values() {
            for layer in &node.connections {
                num_edges += layer.len();
            }
        }

        let avg_connections = if num_vectors > 0 {
            num_edges as f64 / num_vectors as f64
        } else {
            0.0
        };

        HnswStats {
            num_vectors,
            max_level,
            num_edges,
            avg_connections,
            searches: self.search_count.load(AtomicOrdering::Relaxed),
            distance_computations: self.distance_count.load(AtomicOrdering::Relaxed),
        }
    }

    /// Clear the index
    pub fn clear(&self) {
        self.nodes.write().clear();
        *self.entry_point.write() = None;
        self.max_level.store(0, AtomicOrdering::Release);
    }

    /// Check if a vector exists
    pub fn contains(&self, id: u64) -> bool {
        self.nodes.read().contains_key(&id)
    }

    /// Get a vector by ID
    pub fn get(&self, id: u64) -> Option<Vec<f32>> {
        self.nodes.read().get(&id).map(|n| n.vector.clone())
    }

    /// Serialize the index
    pub fn serialize(&self) -> anyhow::Result<Vec<u8>> {
        let nodes = self.nodes.read();
        let entry_point = *self.entry_point.read();
        let max_level = self.max_level.load(AtomicOrdering::Acquire);

        let data = HnswSerializable {
            config: self.config.clone(),
            nodes: nodes.clone(),
            entry_point,
            max_level,
        };

        bincode::serialize(&data).map_err(|e| anyhow::anyhow!("Serialization error: {}", e))
    }

    /// Deserialize the index
    pub fn deserialize(data: &[u8]) -> anyhow::Result<Self> {
        let serializable: HnswSerializable = bincode::deserialize(data)
            .map_err(|e| anyhow::anyhow!("Deserialization error: {}", e))?;

        let index = Self {
            config: serializable.config,
            nodes: RwLock::new(serializable.nodes),
            entry_point: RwLock::new(serializable.entry_point),
            max_level: AtomicUsize::new(serializable.max_level),
            stats: Arc::new(HnswStats::default()),
            search_count: AtomicU64::new(0),
            distance_count: AtomicU64::new(0),
        };

        Ok(index)
    }
}

/// Serializable HNSW state
#[derive(Serialize, Deserialize)]
struct HnswSerializable {
    config: HnswConfig,
    nodes: HashMap<u64, HnswNode>,
    entry_point: Option<u64>,
    max_level: usize,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn random_vector(dim: usize) -> Vec<f32> {
        (0..dim).map(|_| fastrand::f32() * 2.0 - 1.0).collect()
    }

    fn normalized_vector(dim: usize) -> Vec<f32> {
        let v: Vec<f32> = (0..dim).map(|_| fastrand::f32() * 2.0 - 1.0).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter().map(|x| x / norm).collect()
        } else {
            v
        }
    }

    #[test]
    fn test_config_default() {
        let config = HnswConfig::default();
        assert_eq!(config.m, 16);
        assert_eq!(config.m0, 32);
        assert_eq!(config.dimension, 768);
    }

    #[test]
    fn test_config_fast() {
        let config = HnswConfig::fast(512);
        assert_eq!(config.dimension, 512);
        assert!(config.ef_search < HnswConfig::accurate(512).ef_search);
    }

    #[test]
    fn test_distance_euclidean() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let dist = euclidean_distance(&a, &b);
        assert!((dist - 2.0_f32.sqrt()).abs() < 0.001);
    }

    #[test]
    fn test_distance_cosine() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let dist = cosine_distance(&a, &b);
        assert!(dist.abs() < 0.001); // Same direction = 0 distance

        let c = vec![0.0, 1.0, 0.0];
        let dist2 = cosine_distance(&a, &c);
        assert!((dist2 - 1.0).abs() < 0.001); // Orthogonal = 1 distance
    }

    #[test]
    fn test_empty_index() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        assert!(index.is_empty());
        assert_eq!(index.len(), 0);

        let results = index.search(&random_vector(32), 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_single_insert() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        let v = normalized_vector(32);
        index.insert(1, &v);

        assert_eq!(index.len(), 1);
        assert!(index.contains(1));
        assert!(!index.contains(2));
    }

    #[test]
    fn test_insert_and_search() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        // Insert some vectors
        for i in 0..100 {
            index.insert(i, &normalized_vector(32));
        }

        assert_eq!(index.len(), 100);

        // Search should return results
        let query = normalized_vector(32);
        let results = index.search(&query, 10);

        assert!(!results.is_empty());
        assert!(results.len() <= 10);

        // Results should be sorted by distance
        for i in 1..results.len() {
            assert!(results[i].distance >= results[i - 1].distance);
        }
    }

    #[test]
    fn test_exact_match() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        let target = normalized_vector(32);
        index.insert(42, &target);

        // Add more vectors
        for i in 0..50 {
            if i != 42 {
                index.insert(i, &normalized_vector(32));
            }
        }

        // Search for the exact vector
        let results = index.search(&target, 1);

        assert!(!results.is_empty());
        assert_eq!(results[0].id, 42);
        assert!(results[0].distance < 0.001);
    }

    #[test]
    fn test_remove() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        for i in 0..10 {
            index.insert(i, &normalized_vector(32));
        }

        assert_eq!(index.len(), 10);
        assert!(index.contains(5));

        assert!(index.remove(5));

        assert_eq!(index.len(), 9);
        assert!(!index.contains(5));
        assert!(!index.remove(5)); // Already removed
    }

    #[test]
    fn test_get_vector() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        let v = normalized_vector(32);
        index.insert(1, &v);

        let retrieved = index.get(1);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), v);

        assert!(index.get(999).is_none());
    }

    #[test]
    fn test_stats() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        for i in 0..20 {
            index.insert(i, &normalized_vector(32));
        }

        let stats = index.stats();
        assert_eq!(stats.num_vectors, 20);
        assert!(stats.num_edges > 0);
    }

    #[test]
    fn test_clear() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        for i in 0..10 {
            index.insert(i, &normalized_vector(32));
        }

        assert_eq!(index.len(), 10);

        index.clear();

        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        let vectors: Vec<_> = (0..10).map(|_| normalized_vector(32)).collect();
        for (i, v) in vectors.iter().enumerate() {
            index.insert(i as u64, v);
        }

        let serialized = index.serialize().unwrap();
        let restored = HnswIndex::deserialize(&serialized).unwrap();

        assert_eq!(restored.len(), 10);

        // Verify vectors are the same
        for (i, v) in vectors.iter().enumerate() {
            let retrieved = restored.get(i as u64);
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap(), *v);
        }
    }

    #[test]
    fn test_recall_quality() {
        // Test that HNSW achieves reasonable recall 
        let mut config = HnswConfig::fast(32);
        config.ef_search = 100; // High ef_search for better recall
        config.ef_construction = 200;
        let index = HnswIndex::new(config);
        
        // Insert some random vectors
        let n = 100;
        let mut vectors: Vec<Vec<f32>> = Vec::new();
        for _ in 0..n {
            vectors.push(normalized_vector(32));
        }
        
        for (i, v) in vectors.iter().enumerate() {
            index.insert(i as u64, v);
        }
        
        // Query for the first inserted vector (exact match should work)
        let query = &vectors[0];
        let k = 5;
        
        let results = index.search(query, k);
        
        // We should find at least one result
        assert!(!results.is_empty(), "No results found");
        
        // The first result should be very close to the query (exact or near-exact match)
        assert!(results[0].distance < 0.1, 
            "First result should be very close. Got distance: {}", results[0].distance);
        
        // The query vector is vectors[0], so we should find id=0
        let found_exact = results.iter().any(|r| r.id == 0);
        assert!(found_exact, "Failed to find exact match (id=0). Results: {:?}", 
            results.iter().map(|r| (r.id, r.distance)).collect::<Vec<_>>());
    }

    #[test]
    fn test_different_metrics() {
        let dim = 32;

        for metric in [DistanceMetric::Euclidean, DistanceMetric::Cosine, DistanceMetric::InnerProduct] {
            let config = HnswConfig {
                dimension: dim,
                metric,
                ..HnswConfig::small(dim)
            };
            let index = HnswIndex::new(config);

            for i in 0..20 {
                index.insert(i, &normalized_vector(dim));
            }

            let results = index.search(&normalized_vector(dim), 5);
            assert!(!results.is_empty());
        }
    }

    #[test]
    fn test_search_result_similarity() {
        let config = HnswConfig::small(32);
        let index = HnswIndex::new(config);

        let v = normalized_vector(32);
        index.insert(0, &v);

        let results = index.search(&v, 1);
        assert!(!results.is_empty());

        // For cosine, similarity should be close to 1.0 for identical vectors
        assert!(results[0].similarity > 0.99);
    }
}
