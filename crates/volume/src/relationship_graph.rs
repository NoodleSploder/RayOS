//! Relationship Inference - Automatic Concept Linking
//!
//! Builds and maintains a knowledge graph of relationships between files and concepts.
//! Automatically infers connections based on semantic similarity, content analysis,
//! temporal patterns, and structural cues.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        Knowledge Graph                                   │
//! │  ┌───────────┐        ┌───────────┐        ┌───────────┐                │
//! │  │  Concept  │──edge──│  Concept  │──edge──│  Concept  │                │
//! │  │   Node    │        │   Node    │        │   Node    │                │
//! │  └───────────┘        └───────────┘        └───────────┘                │
//! │        │                    │                    │                      │
//! │        └────────────────────┴────────────────────┘                      │
//! │                             │                                           │
//! │                    Relationship Edges                                   │
//! │         (SimilarTo, References, DependsOn, ...)                         │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//! ┌────────────────────────────────┴────────────────────────────────────────┐
//! │                      Inference Engine                                    │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
//! │  │   Semantic   │  │   Content    │  │   Temporal   │  │  Structural  │ │
//! │  │   Analyzer   │  │   Analyzer   │  │   Analyzer   │  │   Analyzer   │ │
//! │  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘ │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_volume::relationship_graph::{KnowledgeGraph, InferenceEngine};
//! use rayos_volume::VectorStore;
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let store = VectorStore::new(Path::new("./db/vectors"))?;
//!     let graph = KnowledgeGraph::new(Path::new("./db/graph"))?;
//!
//!     let engine = InferenceEngine::new(graph.clone(), store);
//!
//!     // Infer relationships for a new file
//!     engine.infer_for_file(file_id).await?;
//!
//!     // Query related concepts
//!     let related = graph.get_related(file_id, 10)?;
//!
//!     // Find shortest path between concepts
//!     let path = graph.find_path(source_id, target_id)?;
//!
//!     Ok(())
//! }
//! ```

use crate::types::{Document, FileId, RelationType};
use crate::vector_store::VectorStore;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// Core Graph Types
// =============================================================================

/// A node in the knowledge graph (represents a file/concept)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptNode {
    /// File ID this node represents
    pub id: FileId,
    /// Human-readable label (usually filename)
    pub label: String,
    /// Node type/category
    pub node_type: NodeType,
    /// Importance score (based on connectivity)
    pub importance: f32,
    /// When the node was created
    pub created: u64,
    /// When the node was last updated
    pub updated: u64,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

/// Types of nodes in the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeType {
    /// A file in the filesystem
    File,
    /// An extracted concept (e.g., "machine learning")
    Concept,
    /// A tag or category
    Tag,
    /// A person/author
    Person,
    /// A project or collection
    Project,
}

/// An edge connecting two nodes in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationEdge {
    /// Source node ID
    pub source: FileId,
    /// Target node ID
    pub target: FileId,
    /// Type of relationship
    pub relation: RelationType,
    /// Strength/confidence of the relationship (0-1)
    pub strength: f32,
    /// Whether this edge was inferred automatically
    pub inferred: bool,
    /// When the edge was created
    pub created: u64,
    /// Evidence/reason for this relationship
    pub evidence: String,
}

impl RelationEdge {
    /// Create a new relationship edge
    pub fn new(
        source: FileId,
        target: FileId,
        relation: RelationType,
        strength: f32,
        evidence: &str,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            source,
            target,
            relation,
            strength,
            inferred: true,
            created: now,
            evidence: evidence.to_string(),
        }
    }

    /// Create an edge key for storage
    fn storage_key(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(24);
        key.extend_from_slice(&self.source.0.to_le_bytes());
        key.extend_from_slice(&self.target.0.to_le_bytes());
        key.extend_from_slice(&(self.relation as u8).to_le_bytes());
        key
    }
}

/// Result of a relationship query
#[derive(Debug, Clone)]
pub struct RelatedNode {
    /// The related file ID
    pub id: FileId,
    /// The relationship type
    pub relation: RelationType,
    /// Strength of the relationship
    pub strength: f32,
    /// Distance (number of hops) from the source
    pub distance: usize,
    /// Path from source to this node (if distance > 1)
    pub path: Vec<FileId>,
}

// =============================================================================
// Knowledge Graph
// =============================================================================

/// The main knowledge graph for storing and querying relationships
pub struct KnowledgeGraph {
    /// Persistent storage for nodes
    nodes_db: sled::Db,
    /// Persistent storage for edges
    edges_db: sled::Db,
    /// In-memory adjacency list (source -> [(target, relation, strength)])
    adjacency: Arc<RwLock<HashMap<FileId, Vec<(FileId, RelationType, f32)>>>>,
    /// Reverse adjacency for incoming edges
    reverse_adjacency: Arc<RwLock<HashMap<FileId, Vec<(FileId, RelationType, f32)>>>>,
    /// Statistics
    stats: Arc<RwLock<GraphStats>>,
}

/// Graph statistics
#[derive(Debug, Clone, Default)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub edges_by_type: HashMap<RelationType, usize>,
    pub avg_connections: f32,
    pub max_connections: usize,
}

impl KnowledgeGraph {
    /// Create or open a knowledge graph
    pub fn new(db_path: &Path) -> Result<Self> {
        log::info!("Opening knowledge graph at: {}", db_path.display());

        std::fs::create_dir_all(db_path)?;

        let nodes_db = sled::open(db_path.join("nodes"))
            .context("Failed to open nodes database")?;
        let edges_db = sled::open(db_path.join("edges"))
            .context("Failed to open edges database")?;

        let graph = Self {
            nodes_db,
            edges_db,
            adjacency: Arc::new(RwLock::new(HashMap::new())),
            reverse_adjacency: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(GraphStats::default())),
        };

        // Load existing edges into memory
        graph.load_adjacency()?;

        Ok(graph)
    }

    /// Load edges from disk into adjacency lists
    fn load_adjacency(&self) -> Result<()> {
        let mut adjacency = self.adjacency.write();
        let mut reverse = self.reverse_adjacency.write();
        let mut stats = self.stats.write();

        for item in self.edges_db.iter() {
            let (_key, value) = item?;
            let edge: RelationEdge = bincode::deserialize(&value)?;

            adjacency
                .entry(edge.source)
                .or_default()
                .push((edge.target, edge.relation, edge.strength));

            reverse
                .entry(edge.target)
                .or_default()
                .push((edge.source, edge.relation, edge.strength));

            stats.total_edges += 1;
            *stats.edges_by_type.entry(edge.relation).or_insert(0) += 1;
        }

        stats.total_nodes = self.nodes_db.len();

        if stats.total_nodes > 0 {
            let total_connections: usize = adjacency.values().map(|v| v.len()).sum();
            stats.avg_connections = total_connections as f32 / stats.total_nodes as f32;
            stats.max_connections = adjacency.values().map(|v| v.len()).max().unwrap_or(0);
        }

        log::info!(
            "Loaded {} nodes, {} edges",
            stats.total_nodes,
            stats.total_edges
        );

        Ok(())
    }

    /// Add or update a node
    pub fn add_node(&self, node: ConceptNode) -> Result<()> {
        let serialized = bincode::serialize(&node)?;
        self.nodes_db.insert(node.id.0.to_le_bytes(), serialized)?;
        self.stats.write().total_nodes = self.nodes_db.len();
        Ok(())
    }

    /// Get a node by ID
    pub fn get_node(&self, id: FileId) -> Result<Option<ConceptNode>> {
        match self.nodes_db.get(id.0.to_le_bytes())? {
            Some(data) => {
                let node: ConceptNode = bincode::deserialize(&data)?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    /// Add a relationship edge
    pub fn add_edge(&self, edge: RelationEdge) -> Result<()> {
        let key = edge.storage_key();
        let serialized = bincode::serialize(&edge)?;
        self.edges_db.insert(key, serialized)?;

        // Update adjacency lists
        {
            let mut adjacency = self.adjacency.write();
            adjacency
                .entry(edge.source)
                .or_default()
                .push((edge.target, edge.relation, edge.strength));
        }

        {
            let mut reverse = self.reverse_adjacency.write();
            reverse
                .entry(edge.target)
                .or_default()
                .push((edge.source, edge.relation, edge.strength));
        }

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_edges += 1;
            *stats.edges_by_type.entry(edge.relation).or_insert(0) += 1;
        }

        Ok(())
    }

    /// Get edges from a node
    pub fn get_edges_from(&self, id: FileId) -> Vec<(FileId, RelationType, f32)> {
        self.adjacency.read().get(&id).cloned().unwrap_or_default()
    }

    /// Get edges to a node
    pub fn get_edges_to(&self, id: FileId) -> Vec<(FileId, RelationType, f32)> {
        self.reverse_adjacency
            .read()
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all related nodes (direct connections)
    pub fn get_related(&self, id: FileId, limit: usize) -> Vec<RelatedNode> {
        let adjacency = self.adjacency.read();
        let edges = adjacency.get(&id).cloned().unwrap_or_default();

        let mut related: Vec<RelatedNode> = edges
            .into_iter()
            .map(|(target, relation, strength)| RelatedNode {
                id: target,
                relation,
                strength,
                distance: 1,
                path: vec![id, target],
            })
            .collect();

        // Sort by strength
        related.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        related.truncate(limit);
        related
    }

    /// Get related nodes up to N hops away
    pub fn get_related_extended(
        &self,
        id: FileId,
        max_hops: usize,
        limit: usize,
    ) -> Vec<RelatedNode> {
        let adjacency = self.adjacency.read();
        let mut visited = HashSet::new();
        let mut results = Vec::new();
        let mut queue: VecDeque<(FileId, Vec<FileId>, usize)> = VecDeque::new();

        visited.insert(id);
        queue.push_back((id, vec![id], 0));

        while let Some((current, path, distance)) = queue.pop_front() {
            if distance >= max_hops {
                continue;
            }

            if let Some(edges) = adjacency.get(&current) {
                for (target, relation, strength) in edges {
                    if visited.contains(target) {
                        continue;
                    }

                    visited.insert(*target);

                    let mut new_path = path.clone();
                    new_path.push(*target);

                    results.push(RelatedNode {
                        id: *target,
                        relation: *relation,
                        strength: *strength * (0.8_f32).powi(distance as i32), // Decay with distance
                        distance: distance + 1,
                        path: new_path.clone(),
                    });

                    if distance + 1 < max_hops {
                        queue.push_back((*target, new_path, distance + 1));
                    }
                }
            }
        }

        // Sort by effective strength
        results.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit);
        results
    }

    /// Find shortest path between two nodes (BFS)
    pub fn find_path(&self, source: FileId, target: FileId) -> Option<Vec<FileId>> {
        if source == target {
            return Some(vec![source]);
        }

        let adjacency = self.adjacency.read();
        let mut visited = HashSet::new();
        let mut queue: VecDeque<(FileId, Vec<FileId>)> = VecDeque::new();

        visited.insert(source);
        queue.push_back((source, vec![source]));

        while let Some((current, path)) = queue.pop_front() {
            if let Some(edges) = adjacency.get(&current) {
                for (next, _, _) in edges {
                    if *next == target {
                        let mut result = path;
                        result.push(*next);
                        return Some(result);
                    }

                    if !visited.contains(next) {
                        visited.insert(*next);
                        let mut new_path = path.clone();
                        new_path.push(*next);
                        queue.push_back((*next, new_path));
                    }
                }
            }
        }

        None
    }

    /// Get nodes by relationship type
    pub fn get_by_relation(&self, relation: RelationType) -> Vec<RelationEdge> {
        let mut edges = Vec::new();

        for item in self.edges_db.iter().flatten() {
            if let Ok(edge) = bincode::deserialize::<RelationEdge>(&item.1) {
                if edge.relation == relation {
                    edges.push(edge);
                }
            }
        }

        edges
    }

    /// Compute importance scores for all nodes (PageRank-like)
    pub fn compute_importance(&self) -> HashMap<FileId, f32> {
        let adjacency = self.adjacency.read();
        let all_nodes: Vec<FileId> = adjacency.keys().cloned().collect();
        let n = all_nodes.len();

        if n == 0 {
            return HashMap::new();
        }

        // Initialize scores
        let mut scores: HashMap<FileId, f32> = all_nodes.iter().map(|id| (*id, 1.0 / n as f32)).collect();
        let damping = 0.85;
        let iterations = 20;

        for _ in 0..iterations {
            let mut new_scores: HashMap<FileId, f32> = HashMap::new();

            for node in &all_nodes {
                let mut score = (1.0 - damping) / n as f32;

                // Sum contributions from incoming edges
                if let Some(incoming) = self.reverse_adjacency.read().get(node) {
                    for (source, _, strength) in incoming {
                        if let Some(source_edges) = adjacency.get(source) {
                            let out_degree = source_edges.len() as f32;
                            let source_score = scores.get(source).copied().unwrap_or(0.0);
                            score += damping * source_score * strength / out_degree;
                        }
                    }
                }

                new_scores.insert(*node, score);
            }

            scores = new_scores;
        }

        scores
    }

    /// Get graph statistics
    pub fn stats(&self) -> GraphStats {
        self.stats.read().clone()
    }

    /// Clear all data
    pub fn clear(&self) -> Result<()> {
        self.nodes_db.clear()?;
        self.edges_db.clear()?;
        self.adjacency.write().clear();
        self.reverse_adjacency.write().clear();
        *self.stats.write() = GraphStats::default();
        Ok(())
    }
}

// =============================================================================
// Inference Engine
// =============================================================================

/// Configuration for relationship inference
#[derive(Debug, Clone)]
pub struct InferenceConfig {
    /// Minimum similarity for "SimilarTo" relationship
    pub similarity_threshold: f32,
    /// Minimum similarity for strong similarity
    pub strong_similarity_threshold: f32,
    /// Maximum neighbors to consider for each file
    pub max_neighbors: usize,
    /// Whether to detect import/reference relationships
    pub detect_references: bool,
    /// Whether to detect temporal relationships
    pub detect_temporal: bool,
    /// Time window for temporal relationships (seconds)
    pub temporal_window_secs: u64,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.5,
            strong_similarity_threshold: 0.75,
            max_neighbors: 20,
            detect_references: true,
            detect_temporal: true,
            temporal_window_secs: 7 * 24 * 60 * 60, // 1 week
        }
    }
}

/// Analyzes content to infer relationships
pub struct InferenceEngine {
    graph: Arc<KnowledgeGraph>,
    store: Arc<VectorStore>,
    config: InferenceConfig,
}

impl InferenceEngine {
    /// Create a new inference engine
    pub fn new(
        graph: Arc<KnowledgeGraph>,
        store: Arc<VectorStore>,
        config: InferenceConfig,
    ) -> Self {
        Self {
            graph,
            store,
            config,
        }
    }

    /// Infer relationships for a single file
    pub async fn infer_for_file(&self, file_id: FileId) -> Result<Vec<RelationEdge>> {
        let doc = self
            .store
            .get(file_id)?
            .context("File not found in vector store")?;

        let mut edges = Vec::new();

        // 1. Find semantically similar files
        edges.extend(self.infer_semantic_relations(&doc)?);

        // 2. Detect references/imports
        if self.config.detect_references {
            edges.extend(self.infer_reference_relations(&doc)?);
        }

        // 3. Detect temporal relationships
        if self.config.detect_temporal {
            edges.extend(self.infer_temporal_relations(&doc)?);
        }

        // 4. Detect structural relationships (same directory, etc.)
        edges.extend(self.infer_structural_relations(&doc)?);

        // Add edges to graph
        for edge in &edges {
            self.graph.add_edge(edge.clone())?;
        }

        // Ensure node exists
        let node = ConceptNode {
            id: file_id,
            label: doc
                .metadata
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            node_type: NodeType::File,
            importance: 0.0,
            created: doc.metadata.created,
            updated: doc.metadata.modified,
            metadata: HashMap::new(),
        };
        self.graph.add_node(node)?;

        log::info!(
            "Inferred {} relationships for {:?}",
            edges.len(),
            doc.metadata.path
        );

        Ok(edges)
    }

    /// Infer semantic similarity relationships
    fn infer_semantic_relations(&self, doc: &Document) -> Result<Vec<RelationEdge>> {
        let mut edges = Vec::new();

        // Get all documents and compute similarities
        let all_docs = self.store.iter()?;

        for other in all_docs {
            if other.metadata.id == doc.metadata.id {
                continue;
            }

            let similarity = doc.embedding.similarity(&other.embedding);

            if similarity >= self.config.strong_similarity_threshold {
                edges.push(RelationEdge::new(
                    doc.metadata.id,
                    other.metadata.id,
                    RelationType::SimilarTo,
                    similarity,
                    &format!("High semantic similarity: {:.0}%", similarity * 100.0),
                ));
            } else if similarity >= self.config.similarity_threshold {
                edges.push(RelationEdge::new(
                    doc.metadata.id,
                    other.metadata.id,
                    RelationType::SimilarTo,
                    similarity,
                    &format!("Moderate semantic similarity: {:.0}%", similarity * 100.0),
                ));
            }
        }

        // Keep only top N
        edges.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        edges.truncate(self.config.max_neighbors);

        Ok(edges)
    }

    /// Infer reference/import relationships from content
    fn infer_reference_relations(&self, doc: &Document) -> Result<Vec<RelationEdge>> {
        let mut edges = Vec::new();
        let content = &doc.content_preview;

        // Look for import statements
        let import_patterns = [
            r#"import\s+["\']([^"\']+)["\']"#,      // JS/TS import
            r#"from\s+["\']([^"\']+)["\']"#,        // Python from
            r#"use\s+(\w+(?:::\w+)*)"#,              // Rust use
            r#"#include\s+[<"]([^>"]+)[>"]"#,       // C/C++ include
            r#"require\s*\(["\']([^"\']+)["\']\)"#, // Node require
        ];

        for pattern in &import_patterns {
            if let Ok(regex) = regex::Regex::new(pattern) {
                for cap in regex.captures_iter(content) {
                    if let Some(import_name) = cap.get(1) {
                        let import_str = import_name.as_str();

                        // Try to find the imported file
                        if let Some(target) = self.find_file_by_name(import_str)? {
                            edges.push(RelationEdge::new(
                                doc.metadata.id,
                                target,
                                RelationType::References,
                                0.9,
                                &format!("Imports/references: {}", import_str),
                            ));
                        }
                    }
                }
            }
        }

        Ok(edges)
    }

    /// Infer temporal relationships (created around the same time)
    fn infer_temporal_relations(&self, doc: &Document) -> Result<Vec<RelationEdge>> {
        let mut edges = Vec::new();
        let window = self.config.temporal_window_secs;

        for other in self.store.iter()? {
            if other.metadata.id == doc.metadata.id {
                continue;
            }

            let time_diff =
                (doc.metadata.created as i64 - other.metadata.created as i64).unsigned_abs();

            if time_diff <= window {
                let strength = 1.0 - (time_diff as f32 / window as f32);
                if strength > 0.3 {
                    edges.push(RelationEdge::new(
                        doc.metadata.id,
                        other.metadata.id,
                        RelationType::TemporallyRelated,
                        strength,
                        &format!("Created within {} days of each other", time_diff / 86400),
                    ));
                }
            }
        }

        // Limit temporal relations
        edges.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        edges.truncate(5);

        Ok(edges)
    }

    /// Infer structural relationships (same directory, project, etc.)
    fn infer_structural_relations(&self, doc: &Document) -> Result<Vec<RelationEdge>> {
        let mut edges = Vec::new();

        let doc_parent = doc.metadata.path.parent();

        for other in self.store.iter()? {
            if other.metadata.id == doc.metadata.id {
                continue;
            }

            // Same directory
            if doc_parent == other.metadata.path.parent() {
                edges.push(RelationEdge::new(
                    doc.metadata.id,
                    other.metadata.id,
                    RelationType::SameDomain,
                    0.7,
                    "Same directory",
                ));
            }

            // Same file extension (likely same type of content)
            if doc.metadata.path.extension() == other.metadata.path.extension() {
                edges.push(RelationEdge::new(
                    doc.metadata.id,
                    other.metadata.id,
                    RelationType::SharedPattern,
                    0.4,
                    "Same file type/extension",
                ));
            }
        }

        // Limit structural relations
        edges.truncate(10);

        Ok(edges)
    }

    /// Try to find a file by name or partial path
    fn find_file_by_name(&self, name: &str) -> Result<Option<FileId>> {
        let name_lower = name.to_lowercase();

        for doc in self.store.iter()? {
            let path_str = doc.metadata.path.to_string_lossy().to_lowercase();
            let file_name = doc
                .metadata
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();

            if file_name.contains(&name_lower) || path_str.ends_with(&name_lower) {
                return Ok(Some(doc.metadata.id));
            }
        }

        Ok(None)
    }

    /// Infer relationships for all files in the store
    pub async fn infer_all(&self) -> Result<usize> {
        let all_ids = self.store.all_ids()?;
        let mut total_edges = 0;

        log::info!("Inferring relationships for {} files", all_ids.len());

        for id in all_ids {
            match self.infer_for_file(id).await {
                Ok(edges) => total_edges += edges.len(),
                Err(e) => log::warn!("Failed to infer for {:?}: {}", id, e),
            }
        }

        log::info!("Total relationships inferred: {}", total_edges);

        Ok(total_edges)
    }

    /// Update relationships after a file changes
    pub async fn update_for_file(&self, file_id: FileId) -> Result<()> {
        // Remove old edges from this file
        // (In a full implementation, we'd track and remove old edges)

        // Re-infer
        self.infer_for_file(file_id).await?;

        Ok(())
    }
}

// =============================================================================
// Graph Queries
// =============================================================================

/// Query builder for graph traversal
pub struct GraphQuery {
    start: FileId,
    relation_filter: Option<RelationType>,
    min_strength: f32,
    max_depth: usize,
    limit: usize,
}

impl GraphQuery {
    /// Create a new query starting from a node
    pub fn from(start: FileId) -> Self {
        Self {
            start,
            relation_filter: None,
            min_strength: 0.0,
            max_depth: 3,
            limit: 20,
        }
    }

    /// Filter by relationship type
    pub fn with_relation(mut self, relation: RelationType) -> Self {
        self.relation_filter = Some(relation);
        self
    }

    /// Set minimum strength threshold
    pub fn min_strength(mut self, strength: f32) -> Self {
        self.min_strength = strength;
        self
    }

    /// Set maximum traversal depth
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set result limit
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Execute the query
    pub fn execute(&self, graph: &KnowledgeGraph) -> Vec<RelatedNode> {
        let mut results = graph.get_related_extended(self.start, self.max_depth, self.limit * 2);

        // Apply filters
        results.retain(|r| {
            if r.strength < self.min_strength {
                return false;
            }
            if let Some(rel) = self.relation_filter {
                if r.relation != rel {
                    return false;
                }
            }
            true
        });

        results.truncate(self.limit);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_graph_creation() {
        let dir = tempdir().unwrap();
        let graph = KnowledgeGraph::new(dir.path()).unwrap();

        let stats = graph.stats();
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.total_edges, 0);
    }

    #[test]
    fn test_add_node_and_edge() {
        let dir = tempdir().unwrap();
        let graph = KnowledgeGraph::new(dir.path()).unwrap();

        let node1 = ConceptNode {
            id: FileId(1),
            label: "test1.rs".to_string(),
            node_type: NodeType::File,
            importance: 0.0,
            created: 0,
            updated: 0,
            metadata: HashMap::new(),
        };

        let node2 = ConceptNode {
            id: FileId(2),
            label: "test2.rs".to_string(),
            node_type: NodeType::File,
            importance: 0.0,
            created: 0,
            updated: 0,
            metadata: HashMap::new(),
        };

        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();

        let edge = RelationEdge::new(
            FileId(1),
            FileId(2),
            RelationType::SimilarTo,
            0.8,
            "Test similarity",
        );

        graph.add_edge(edge).unwrap();

        let related = graph.get_related(FileId(1), 10);
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].id, FileId(2));
        assert_eq!(related[0].relation, RelationType::SimilarTo);
    }

    #[test]
    fn test_find_path() {
        let dir = tempdir().unwrap();
        let graph = KnowledgeGraph::new(dir.path()).unwrap();

        // Create a chain: 1 -> 2 -> 3
        graph.add_edge(RelationEdge::new(
            FileId(1), FileId(2), RelationType::References, 0.9, "ref"
        )).unwrap();
        graph.add_edge(RelationEdge::new(
            FileId(2), FileId(3), RelationType::References, 0.9, "ref"
        )).unwrap();

        let path = graph.find_path(FileId(1), FileId(3));
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path, vec![FileId(1), FileId(2), FileId(3)]);
    }

    #[test]
    fn test_graph_query_builder() {
        let query = GraphQuery::from(FileId(1))
            .with_relation(RelationType::SimilarTo)
            .min_strength(0.5)
            .max_depth(2)
            .limit(10);

        assert_eq!(query.start, FileId(1));
        assert_eq!(query.relation_filter, Some(RelationType::SimilarTo));
        assert_eq!(query.min_strength, 0.5);
        assert_eq!(query.max_depth, 2);
        assert_eq!(query.limit, 10);
    }
}
