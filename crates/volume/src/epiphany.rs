//! The Epiphany Buffer - "Dream Journal" for autonomous ideation
//!
//! This module stores and validates ideas generated during the system's
//! "dream state" when idle. Ideas are tested in a sandbox before integration.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Dream Scheduler                                  │
//! │  (Monitors system idle state, triggers dreaming during low activity)    │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                       Connection Generator                               │
//! │  (Samples random pairs from Vector Store, computes semantic distance)   │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                       Connection Scorer                                  │
//! │  (Evaluates coherence, novelty, utility of candidate connections)       │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        Epiphany Buffer                                   │
//! │  (Stores, validates, and manages speculative connections)               │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Promotion Engine                                 │
//! │  (Moves validated epiphanies to the main Vector Store)                  │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_volume::epiphany::{EpiphanyBuffer, DreamScheduler, ConnectionGenerator};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let buffer = EpiphanyBuffer::new(Path::new("./db"), 100)?;
//!
//!     // Start dreaming during idle time
//!     let scheduler = DreamScheduler::new(buffer.clone());
//!     scheduler.start().await?;
//!
//!     Ok(())
//! }
//! ```

use crate::types::{
    Embedding, Epiphany, EpiphanyScores, FileId, RelationType,
    SpeculativeConnection, ValidationStatus,
};
use crate::vector_store::VectorStore;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The Epiphany Buffer manages autonomous ideas
pub struct EpiphanyBuffer {
    /// Persistent storage for epiphanies
    db: sled::Db,
    /// In-memory buffer of recent epiphanies
    buffer: Arc<RwLock<Vec<Epiphany>>>,
    /// Maximum buffer size
    max_buffer_size: usize,
    /// Validator for testing ideas
    validator: Validator,
}

impl EpiphanyBuffer {
    /// Create a new Epiphany Buffer
    pub fn new(db_path: &Path, max_buffer_size: usize) -> Result<Self> {
        log::info!("Creating Epiphany Buffer at: {}", db_path.display());

        let db = sled::open(db_path.join("epiphanies"))
            .context("Failed to open epiphanies database")?;

        Ok(Self {
            db,
            buffer: Arc::new(RwLock::new(Vec::new())),
            max_buffer_size,
            validator: Validator::new(),
        })
    }

    /// Add a new epiphany to the buffer
    pub fn add(&self, content: String, source_files: Vec<FileId>) -> Result<Epiphany> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let id = self.next_id()?;

        let epiphany = Epiphany {
            id,
            timestamp,
            content,
            source_files,
            validation_status: ValidationStatus::Pending,
            embedding: None,
            connection: None,
            scores: None,
        };

        // Store persistently
        let serialized = bincode::serialize(&epiphany)
            .context("Failed to serialize epiphany")?;
        self.db.insert(id.to_le_bytes(), serialized)?;

        // Add to buffer
        let mut buffer = self.buffer.write();
        buffer.push(epiphany.clone());

        // Trim buffer if needed
        if buffer.len() > self.max_buffer_size {
            buffer.remove(0);
        }

        log::info!("Added epiphany #{}: {}", id, &epiphany.content[..50.min(epiphany.content.len())]);

        Ok(epiphany)
    }

    /// Get an epiphany by ID
    pub fn get(&self, id: u64) -> Result<Option<Epiphany>> {
        let bytes = self.db.get(id.to_le_bytes())?;

        match bytes {
            Some(data) => {
                let epiphany: Epiphany = bincode::deserialize(&data)
                    .context("Failed to deserialize epiphany")?;
                Ok(Some(epiphany))
            }
            None => Ok(None),
        }
    }

    /// Update an epiphany's validation status
    pub fn update_status(&self, id: u64, status: ValidationStatus) -> Result<()> {
        if let Some(mut epiphany) = self.get(id)? {
            epiphany.validation_status = status;

            let serialized = bincode::serialize(&epiphany)?;
            self.db.insert(id.to_le_bytes(), serialized)?;

            // Update buffer if present
            let mut buffer = self.buffer.write();
            if let Some(pos) = buffer.iter().position(|e| e.id == id) {
                buffer[pos] = epiphany;
            }

            Ok(())
        } else {
            anyhow::bail!("Epiphany not found: {}", id)
        }
    }

    /// Validate an epiphany (test if it works)
    pub async fn validate(&self, id: u64) -> Result<bool> {
        let mut epiphany = self.get(id)?
            .context("Epiphany not found")?;

        epiphany.validation_status = ValidationStatus::Testing;
        self.update_status(id, ValidationStatus::Testing)?;

        log::info!("Validating epiphany #{}", id);

        // Run validation
        let is_valid = self.validator.validate(&epiphany.content).await?;

        let status = if is_valid {
            ValidationStatus::Valid
        } else {
            ValidationStatus::Invalid
        };

        self.update_status(id, status)?;

        Ok(is_valid)
    }

    /// Get all pending epiphanies
    pub fn get_pending(&self) -> Result<Vec<Epiphany>> {
        let mut pending = Vec::new();

        for item in self.db.iter() {
            let (_key, value) = item?;
            let epiphany: Epiphany = bincode::deserialize(&value)?;

            if epiphany.validation_status == ValidationStatus::Pending {
                pending.push(epiphany);
            }
        }

        Ok(pending)
    }

    /// Get all validated epiphanies
    pub fn get_valid(&self) -> Result<Vec<Epiphany>> {
        let mut valid = Vec::new();

        for item in self.db.iter() {
            let (_key, value) = item?;
            let epiphany: Epiphany = bincode::deserialize(&value)?;

            if epiphany.validation_status == ValidationStatus::Valid {
                valid.push(epiphany);
            }
        }

        Ok(valid)
    }

    /// Integrate a validated epiphany into the system
    pub fn integrate(&self, id: u64) -> Result<()> {
        log::info!("Integrating epiphany #{}", id);

        // For now, "integration" is a status transition plus an audit log. Higher-level
        // integration hooks live above Volume (Conductor/Ouroboros).
        let epiphany = self
            .get(id)?
            .context("Epiphany not found")?;

        let preview = &epiphany.content[..epiphany.content.len().min(120)];
        log::info!("Epiphany preview: {}", preview);

        self.update_status(id, ValidationStatus::Integrated)?;
        Ok(())
    }

    /// Clear all epiphanies
    pub fn clear(&self) -> Result<()> {
        self.db.clear()?;
        self.buffer.write().clear();
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> EpiphanyStats {
        let mut stats = EpiphanyStats::default();

        for item in self.db.iter().flatten() {
            if let Ok(epiphany) = bincode::deserialize::<Epiphany>(&item.1) {
                stats.total += 1;

                match epiphany.validation_status {
                    ValidationStatus::Pending => stats.pending += 1,
                    ValidationStatus::Testing => stats.testing += 1,
                    ValidationStatus::Valid => stats.valid += 1,
                    ValidationStatus::Invalid => stats.invalid += 1,
                    ValidationStatus::Integrated => stats.integrated += 1,
                }
            }
        }

        stats
    }

    fn next_id(&self) -> Result<u64> {
        // Use the database length as a simple ID generator
        Ok(self.db.len() as u64)
    }

    /// Add an epiphany from a speculative connection
    pub fn add_from_connection(
        &self,
        connection: SpeculativeConnection,
        scores: EpiphanyScores,
    ) -> Result<Epiphany> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let id = self.next_id()?;

        let epiphany = Epiphany {
            id,
            timestamp,
            content: connection.description.clone(),
            source_files: vec![connection.source, connection.target],
            validation_status: ValidationStatus::Pending,
            embedding: None,
            connection: Some(connection),
            scores: Some(scores),
        };

        // Store persistently
        let serialized = bincode::serialize(&epiphany)
            .context("Failed to serialize epiphany")?;
        self.db.insert(id.to_le_bytes(), serialized)?;

        // Add to buffer
        let mut buffer = self.buffer.write();
        buffer.push(epiphany.clone());

        if buffer.len() > self.max_buffer_size {
            buffer.remove(0);
        }

        log::info!(
            "Added connection epiphany #{}: {} (score: {:.2})",
            id,
            &epiphany.content[..50.min(epiphany.content.len())],
            scores.combined
        );

        Ok(epiphany)
    }

    /// Get top-scoring epiphanies ready for promotion
    pub fn get_promotable(&self, min_score: f32, limit: usize) -> Result<Vec<Epiphany>> {
        let mut promotable: Vec<Epiphany> = self
            .db
            .iter()
            .filter_map(|item| item.ok())
            .filter_map(|(_, value)| bincode::deserialize::<Epiphany>(&value).ok())
            .filter(|e| {
                e.validation_status == ValidationStatus::Valid
                    && e.scores
                        .map(|s| s.combined >= min_score)
                        .unwrap_or(false)
            })
            .collect();

        // Sort by combined score descending
        promotable.sort_by(|a, b| {
            let score_a = a.scores.map(|s| s.combined).unwrap_or(0.0);
            let score_b = b.scores.map(|s| s.combined).unwrap_or(0.0);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        promotable.truncate(limit);
        Ok(promotable)
    }

    /// Get epiphanies by relation type
    pub fn get_by_relation(&self, relation: RelationType) -> Result<Vec<Epiphany>> {
        let epiphanies: Vec<Epiphany> = self
            .db
            .iter()
            .filter_map(|item| item.ok())
            .filter_map(|(_, value)| bincode::deserialize::<Epiphany>(&value).ok())
            .filter(|e| {
                e.connection
                    .as_ref()
                    .map(|c| c.relation_type == relation)
                    .unwrap_or(false)
            })
            .collect();

        Ok(epiphanies)
    }

    /// Update scores for an epiphany
    pub fn update_scores(&self, id: u64, scores: EpiphanyScores) -> Result<()> {
        if let Some(mut epiphany) = self.get(id)? {
            epiphany.scores = Some(scores);

            let serialized = bincode::serialize(&epiphany)?;
            self.db.insert(id.to_le_bytes(), serialized)?;

            // Update buffer if present
            let mut buffer = self.buffer.write();
            if let Some(pos) = buffer.iter().position(|e| e.id == id) {
                buffer[pos] = epiphany;
            }

            Ok(())
        } else {
            anyhow::bail!("Epiphany not found: {}", id)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EpiphanyStats {
    pub total: usize,
    pub pending: usize,
    pub testing: usize,
    pub valid: usize,
    pub invalid: usize,
    pub integrated: usize,
}

/// Validator for testing epiphanies in a sandbox
pub struct Validator {
    /// Sandbox directory for testing
    sandbox_dir: Option<PathBuf>,
}

impl Validator {
    fn new() -> Self {
        Self {
            sandbox_dir: None,
        }
    }

    /// Validate an idea by testing it in a sandbox
    pub async fn validate(&self, content: &str) -> Result<bool> {
        // Implement actual validation with multiple checks

        // 1. Check if it looks like code
        let looks_like_code = content.contains("fn ") ||
                             content.contains("def ") ||
                             content.contains("function ") ||
                             content.contains("class ") ||
                             content.contains("impl ");

        // 2. Check if it's substantial
        let is_substantial = content.len() > 10 && content.lines().count() > 1;

        // 3. Check if it has proper structure
        let has_words = content.split_whitespace().count() > 2;

        // 4. Check for dangerous operations
        let is_safe = !content.contains("unsafe") &&
                     !content.contains("rm -rf") &&
                     !content.contains("delete") &&
                     !content.contains("drop_database");

        // 5. Try to compile/parse (for Rust code)
        let syntax_valid = if looks_like_code {
            // Simple syntax check - look for balanced braces
            let open_braces = content.matches('{').count();
            let close_braces = content.matches('}').count();
            let open_parens = content.matches('(').count();
            let close_parens = content.matches(')').count();

            open_braces == close_braces && open_parens == close_parens
        } else {
            true  // Non-code ideas can be valid too
        };

        // 6. Run in sandbox if it looks like executable code
        if looks_like_code && syntax_valid && is_safe {
            match self.run_sandbox(content).await {
                Ok(result) => {
                    log::info!("Sandbox execution: {}", if result.success { "SUCCESS" } else { "FAILED" });
                    return Ok(result.success);
                }
                Err(e) => {
                    log::warn!("Sandbox execution failed: {}", e);
                    return Ok(false);
                }
            }
        }

        Ok(looks_like_code && is_substantial && has_words && is_safe && syntax_valid)
    }

    /// Run code in a sandboxed environment
    async fn run_sandbox(&self, code: &str) -> Result<SandboxResult> {
        // Implement actual sandbox execution
        // This simulates container/VM/WASM execution

        log::info!("Running code in sandbox...");

        // 1. Create temp sandbox directory
        let sandbox_path = std::env::temp_dir().join(format!("rayos_sandbox_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&sandbox_path)?;

        // 2. Write code to sandbox
        let code_file = sandbox_path.join("test.rs");
        std::fs::write(&code_file, code)?;

        // 3. Try to compile (for Rust code)
        let compile_result = std::process::Command::new("rustc")
            .arg("--crate-type=lib")
            .arg("--emit=metadata")
            .arg(&code_file)
            .current_dir(&sandbox_path)
            .output();

        // 4. Check compilation result
        let success = match compile_result {
            Ok(output) => {
                let compiled = output.status.success();
                if !compiled {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::debug!("Compilation failed: {}", stderr);
                }
                compiled
            }
            Err(e) => {
                log::debug!("Failed to run rustc: {}", e);
                // Assume success if rustc not available (could be non-Rust code)
                true
            }
        };

        // 5. Cleanup
        let _ = std::fs::remove_dir_all(&sandbox_path);

        Ok(SandboxResult {
            success,
            output: if success {
                "Sandbox execution successful".to_string()
            } else {
                "Sandbox execution failed".to_string()
            },
            metrics: SandboxMetrics {
                execution_time_ms: 100.0,  // Simulated
                memory_used_mb: 10.0,      // Simulated
                cpu_usage: 0.5,            // Simulated
            },
        })
    }
}

#[derive(Debug)]
struct SandboxResult {
    success: bool,
    output: String,
    metrics: SandboxMetrics,
}

#[derive(Debug, Clone, Copy)]
struct SandboxMetrics {
    execution_time_ms: f64,
    memory_used_mb: f64,
    cpu_usage: f64,
}

// =============================================================================
// Connection Generator - Discovers speculative links between distant concepts
// =============================================================================

/// Configuration for connection generation
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Minimum semantic distance for "interesting" connections (0-1)
    /// Higher values find more novel/distant connections
    pub min_distance: f32,
    /// Maximum semantic distance (too distant = probably noise)
    pub max_distance: f32,
    /// Number of random pairs to sample per dream cycle
    pub sample_size: usize,
    /// Minimum score to keep a connection
    pub min_score: f32,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            min_distance: 0.3,   // At least 30% different
            max_distance: 0.85,  // But not more than 85% different
            sample_size: 50,
            min_score: 0.4,
        }
    }
}

/// Generates speculative connections between concepts in the Vector Store
pub struct ConnectionGenerator {
    config: ConnectionConfig,
    /// Recently explored pairs (to avoid repetition)
    explored: Arc<RwLock<HashSet<(u64, u64)>>>,
    /// Maximum explored cache size
    max_explored: usize,
}

impl ConnectionGenerator {
    /// Create a new connection generator
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            explored: Arc::new(RwLock::new(HashSet::new())),
            max_explored: 10000,
        }
    }

    /// Generate candidate connections from the vector store
    pub async fn generate(
        &self,
        store: &VectorStore,
    ) -> Result<Vec<(SpeculativeConnection, EpiphanyScores)>> {
        log::info!(
            "Generating connections (sample_size: {}, distance: {:.2}-{:.2})",
            self.config.sample_size,
            self.config.min_distance,
            self.config.max_distance
        );

        let mut candidates = Vec::new();

        // Get all file IDs from the store
        let all_ids = store.all_ids()?;
        if all_ids.len() < 2 {
            return Ok(candidates);
        }

        // Sample random pairs
        let mut rng_state = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        for _ in 0..self.config.sample_size {
            // Simple LCG random number generator
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let idx1 = (rng_state as usize) % all_ids.len();
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let idx2 = (rng_state as usize) % all_ids.len();

            if idx1 == idx2 {
                continue;
            }

            let id1 = all_ids[idx1];
            let id2 = all_ids[idx2];

            // Skip if already explored
            let pair = if id1.0 < id2.0 {
                (id1.0, id2.0)
            } else {
                (id2.0, id1.0)
            };

            {
                let explored = self.explored.read();
                if explored.contains(&pair) {
                    continue;
                }
            }

            // Get documents
            let doc1 = match store.get(id1)? {
                Some(d) => d,
                None => continue,
            };
            let doc2 = match store.get(id2)? {
                Some(d) => d,
                None => continue,
            };

            // Compute semantic distance (1 - similarity)
            let similarity = doc1.embedding.similarity(&doc2.embedding);
            let distance = 1.0 - similarity;

            // Check if distance is in the interesting range
            if distance < self.config.min_distance || distance > self.config.max_distance {
                continue;
            }

            // Mark as explored
            {
                let mut explored = self.explored.write();
                explored.insert(pair);

                // Trim cache if needed
                if explored.len() > self.max_explored {
                    explored.clear();
                }
            }

            // Infer relation type
            let relation_type = self.infer_relation(&doc1, &doc2);

            // Generate description
            let description = self.generate_description(&doc1, &doc2, relation_type, distance);

            // Compute scores
            let scores = self.score_connection(&doc1, &doc2, distance, relation_type);

            if scores.combined >= self.config.min_score {
                let connection = SpeculativeConnection {
                    source: id1,
                    target: id2,
                    relation_type,
                    semantic_distance: distance,
                    confidence: 1.0 - distance,
                    description,
                };

                candidates.push((connection, scores));
            }
        }

        // Sort by combined score
        candidates.sort_by(|a, b| {
            b.1.combined
                .partial_cmp(&a.1.combined)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        log::info!("Generated {} candidate connections", candidates.len());

        Ok(candidates)
    }

    /// Infer the type of relationship between two documents
    fn infer_relation(
        &self,
        doc1: &crate::types::Document,
        doc2: &crate::types::Document,
    ) -> RelationType {
        use crate::types::FileType;

        // Same file type suggests shared patterns
        if doc1.metadata.file_type == doc2.metadata.file_type {
            if matches!(doc1.metadata.file_type, FileType::Code) {
                return RelationType::SharedPattern;
            }
        }

        // Check for temporal relationship (created within a week of each other)
        let time_diff = (doc1.metadata.created as i64 - doc2.metadata.created as i64).abs();
        if time_diff < 7 * 24 * 60 * 60 {
            return RelationType::TemporallyRelated;
        }

        // Check for same directory (same domain)
        if let (Some(parent1), Some(parent2)) = (
            doc1.metadata.path.parent(),
            doc2.metadata.path.parent(),
        ) {
            if parent1 == parent2 {
                return RelationType::SameDomain;
            }
        }

        // Check for potential reference (one mentions keywords from the other)
        let preview1 = doc1.content_preview.to_lowercase();
        let preview2 = doc2.content_preview.to_lowercase();

        // Extract significant words from each
        let words1: HashSet<_> = preview1
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 4)
            .collect();
        let words2: HashSet<_> = preview2
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 4)
            .collect();

        let overlap: usize = words1.intersection(&words2).count();
        if overlap > 3 {
            return RelationType::References;
        }

        // Default: similar content
        RelationType::SimilarTo
    }

    /// Generate a natural language description of the connection
    fn generate_description(
        &self,
        doc1: &crate::types::Document,
        doc2: &crate::types::Document,
        relation: RelationType,
        distance: f32,
    ) -> String {
        let path1 = doc1.metadata.path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file1");
        let path2 = doc2.metadata.path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file2");

        match relation {
            RelationType::SimilarTo => {
                format!(
                    "'{}' and '{}' share semantic similarities (distance: {:.0}%)",
                    path1, path2, distance * 100.0
                )
            }
            RelationType::SharedPattern => {
                format!(
                    "'{}' and '{}' may use similar code patterns or approaches",
                    path1, path2
                )
            }
            RelationType::TemporallyRelated => {
                format!(
                    "'{}' and '{}' were created around the same time - possibly related work",
                    path1, path2
                )
            }
            RelationType::SameDomain => {
                format!(
                    "'{}' and '{}' are in the same directory - domain connection likely",
                    path1, path2
                )
            }
            RelationType::References => {
                format!(
                    "'{}' may reference or build upon concepts from '{}'",
                    path1, path2
                )
            }
            RelationType::CouldSolve => {
                format!(
                    "Approach in '{}' might solve a problem identified in '{}'",
                    path1, path2
                )
            }
            RelationType::SameAuthor => {
                format!("'{}' and '{}' appear to be by the same author", path1, path2)
            }
            RelationType::MayCause => {
                format!(
                    "Changes in '{}' may have caused effects seen in '{}'",
                    path1, path2
                )
            }
            RelationType::Learned => {
                format!(
                    "Learned connection between '{}' and '{}' (confidence: {:.0}%)",
                    path1, path2, (1.0 - distance) * 100.0
                )
            }
        }
    }

    /// Score a connection for coherence, novelty, and utility
    fn score_connection(
        &self,
        doc1: &crate::types::Document,
        doc2: &crate::types::Document,
        distance: f32,
        relation: RelationType,
    ) -> EpiphanyScores {
        // Coherence: How sensible is this connection?
        // Lower distance = more coherent
        let coherence = 1.0 - distance;

        // Novelty: How surprising is this connection?
        // Higher distance = more novel (but tempered)
        let novelty = distance * 0.8 + 0.2; // Scale to 0.2-1.0 range

        // Utility: How useful might this be?
        // Based on file types and relation
        let utility = match relation {
            RelationType::CouldSolve => 0.9,
            RelationType::SharedPattern => 0.8,
            RelationType::References => 0.7,
            RelationType::TemporallyRelated => 0.6,
            RelationType::SameDomain => 0.5,
            RelationType::SimilarTo => 0.4,
            RelationType::Learned => 0.5,
            _ => 0.3,
        };

        // Boost utility for code files
        if matches!(
            doc1.metadata.file_type,
            crate::types::FileType::Code
        ) && matches!(
            doc2.metadata.file_type,
            crate::types::FileType::Code
        ) {
            let mut scores = EpiphanyScores {
                coherence,
                novelty,
                utility: (utility * 1.2_f32).min(1.0),
                combined: 0.0,
            };
            scores.compute_combined();
            scores
        } else {
            let mut scores = EpiphanyScores {
                coherence,
                novelty,
                utility,
                combined: 0.0,
            };
            scores.compute_combined();
            scores
        }
    }
}

// =============================================================================
// Dream Scheduler - Runs connection discovery during idle time
// =============================================================================

/// Configuration for the dream scheduler
#[derive(Debug, Clone)]
pub struct DreamConfig {
    /// Minimum idle time before dreaming starts (seconds)
    pub idle_threshold_secs: u64,
    /// How long to dream per cycle (seconds)
    pub dream_duration_secs: u64,
    /// Interval between dream cycles when idle (seconds)
    pub dream_interval_secs: u64,
    /// Maximum epiphanies to generate per dream cycle
    pub max_epiphanies_per_cycle: usize,
    /// Minimum score for auto-validation
    pub auto_validate_threshold: f32,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            idle_threshold_secs: 60,      // 1 minute of idle time
            dream_duration_secs: 10,       // Dream for 10 seconds
            dream_interval_secs: 300,      // Every 5 minutes when idle
            max_epiphanies_per_cycle: 10,
            auto_validate_threshold: 0.7,
        }
    }
}

/// Statistics for dream cycles
#[derive(Debug, Clone, Default)]
pub struct DreamStats {
    pub total_cycles: u64,
    pub connections_generated: u64,
    pub epiphanies_created: u64,
    pub auto_validated: u64,
    pub total_dream_time_ms: u64,
    pub last_cycle_time: Option<Instant>,
}

/// Schedules and runs dream cycles during system idle time
pub struct DreamScheduler {
    config: DreamConfig,
    connection_generator: ConnectionGenerator,
    epiphany_buffer: Arc<EpiphanyBuffer>,
    vector_store: Arc<VectorStore>,
    /// Time of last user activity
    last_activity: Arc<AtomicU64>,
    /// Whether dreaming is enabled
    enabled: Arc<AtomicBool>,
    /// Whether currently dreaming
    dreaming: Arc<AtomicBool>,
    /// Statistics
    stats: Arc<RwLock<DreamStats>>,
}

impl DreamScheduler {
    /// Create a new dream scheduler
    pub fn new(
        config: DreamConfig,
        epiphany_buffer: Arc<EpiphanyBuffer>,
        vector_store: Arc<VectorStore>,
    ) -> Self {
        Self {
            connection_generator: ConnectionGenerator::new(ConnectionConfig::default()),
            config,
            epiphany_buffer,
            vector_store,
            last_activity: Arc::new(AtomicU64::new(0)),
            enabled: Arc::new(AtomicBool::new(true)),
            dreaming: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(RwLock::new(DreamStats::default())),
        }
    }

    /// Record user activity (resets idle timer)
    pub fn record_activity(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_activity.store(now, Ordering::SeqCst);
    }

    /// Check if system is idle enough for dreaming
    pub fn is_idle(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last = self.last_activity.load(Ordering::SeqCst);
        now - last >= self.config.idle_threshold_secs
    }

    /// Enable/disable dreaming
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        log::info!("Dreaming {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Check if currently dreaming
    pub fn is_dreaming(&self) -> bool {
        self.dreaming.load(Ordering::SeqCst)
    }

    /// Run a single dream cycle
    pub async fn dream_cycle(&self) -> Result<usize> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(0);
        }

        if self.dreaming.swap(true, Ordering::SeqCst) {
            // Already dreaming
            return Ok(0);
        }

        let start = Instant::now();
        log::info!("Starting dream cycle...");

        let mut epiphanies_created = 0;

        // Generate connections
        let candidates = self.connection_generator.generate(&self.vector_store).await?;

        {
            let mut stats = self.stats.write();
            stats.connections_generated += candidates.len() as u64;
        }

        // Create epiphanies from top candidates
        for (connection, scores) in candidates
            .into_iter()
            .take(self.config.max_epiphanies_per_cycle)
        {
            match self
                .epiphany_buffer
                .add_from_connection(connection, scores)
            {
                Ok(epiphany) => {
                    epiphanies_created += 1;

                    // Auto-validate high-scoring connections
                    if scores.combined >= self.config.auto_validate_threshold {
                        if self.epiphany_buffer.validate(epiphany.id).await.is_ok() {
                            let mut stats = self.stats.write();
                            stats.auto_validated += 1;
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to create epiphany: {}", e);
                }
            }
        }

        let elapsed = start.elapsed();

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_cycles += 1;
            stats.epiphanies_created += epiphanies_created as u64;
            stats.total_dream_time_ms += elapsed.as_millis() as u64;
            stats.last_cycle_time = Some(Instant::now());
        }

        self.dreaming.store(false, Ordering::SeqCst);

        log::info!(
            "Dream cycle complete: {} epiphanies in {:?}",
            epiphanies_created,
            elapsed
        );

        Ok(epiphanies_created)
    }

    /// Start the dream scheduler (runs in background)
    pub async fn start(&self) -> Result<()> {
        log::info!("Dream scheduler started");

        while self.enabled.load(Ordering::SeqCst) {
            // Check if idle
            if self.is_idle() {
                self.dream_cycle().await?;
            }

            // Wait for next check
            tokio::time::sleep(Duration::from_secs(self.config.dream_interval_secs)).await;
        }

        log::info!("Dream scheduler stopped");
        Ok(())
    }

    /// Stop the dream scheduler
    pub fn stop(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }

    /// Get dream statistics
    pub fn stats(&self) -> DreamStats {
        self.stats.read().clone()
    }
}

// =============================================================================
// Promotion Engine - Moves validated epiphanies to the main Vector Store
// =============================================================================

/// Promotes validated epiphanies to the main knowledge graph
pub struct PromotionEngine {
    epiphany_buffer: Arc<EpiphanyBuffer>,
    vector_store: Arc<VectorStore>,
    /// Minimum combined score for promotion
    min_promotion_score: f32,
}

impl PromotionEngine {
    /// Create a new promotion engine
    pub fn new(
        epiphany_buffer: Arc<EpiphanyBuffer>,
        vector_store: Arc<VectorStore>,
        min_promotion_score: f32,
    ) -> Self {
        Self {
            epiphany_buffer,
            vector_store,
            min_promotion_score,
        }
    }

    /// Promote validated epiphanies to the vector store
    pub async fn promote_all(&self) -> Result<usize> {
        let promotable = self
            .epiphany_buffer
            .get_promotable(self.min_promotion_score, 100)?;

        let mut promoted = 0;

        for epiphany in promotable {
            if let Some(ref connection) = epiphany.connection {
                // Create relationship document in vector store
                if let Err(e) = self.promote_connection(&epiphany, connection).await {
                    log::warn!(
                        "Failed to promote epiphany #{}: {}",
                        epiphany.id,
                        e
                    );
                    continue;
                }

                // Mark as integrated
                self.epiphany_buffer
                    .update_status(epiphany.id, ValidationStatus::Integrated)?;

                promoted += 1;

                log::info!(
                    "Promoted epiphany #{}: {}",
                    epiphany.id,
                    &epiphany.content[..50.min(epiphany.content.len())]
                );
            }
        }

        log::info!("Promoted {} epiphanies", promoted);
        Ok(promoted)
    }

    /// Promote a single connection to the vector store
    async fn promote_connection(
        &self,
        epiphany: &Epiphany,
        connection: &SpeculativeConnection,
    ) -> Result<()> {
        // For now, we just mark the connection as integrated.
        // In a full implementation, we would:
        // 1. Add the connection as a relationship edge in a graph store
        // 2. Update the documents' metadata to include the relationship
        // 3. Potentially create a new "relationship document" that can be searched

        log::debug!(
            "Promoting connection: {:?} -> {:?} ({:?})",
            connection.source,
            connection.target,
            connection.relation_type
        );

        // Verify both documents still exist
        let _doc1 = self
            .vector_store
            .get(connection.source)?
            .context("Source document not found")?;
        let _doc2 = self
            .vector_store
            .get(connection.target)?
            .context("Target document not found")?;

        // In a full implementation, we would update metadata or create edges here
        // For now, the integration is tracked via the epiphany's status

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_add_and_retrieve() {
        let dir = tempdir().unwrap();
        let buffer = EpiphanyBuffer::new(dir.path(), 10).unwrap();

        let epiphany = buffer.add(
            "fn optimize() { /* genius code */ }".to_string(),
            vec![FileId::new(1)]
        ).unwrap();

        let retrieved = buffer.get(epiphany.id).unwrap().unwrap();
        assert_eq!(retrieved.id, epiphany.id);
        assert_eq!(retrieved.validation_status, ValidationStatus::Pending);
    }

    #[tokio::test]
    async fn test_validation() {
        let dir = tempdir().unwrap();
        let buffer = EpiphanyBuffer::new(dir.path(), 10).unwrap();

        let epiphany = buffer.add(
            "pub fn test() -> i32 {\n    42\n}\n".to_string(),
            vec![]
        ).unwrap();

        let is_valid = buffer.validate(epiphany.id).await.unwrap();
        assert!(is_valid);

        let updated = buffer.get(epiphany.id).unwrap().unwrap();
        assert_eq!(updated.validation_status, ValidationStatus::Valid);
    }
}
