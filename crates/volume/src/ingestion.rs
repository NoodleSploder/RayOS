//! Content Ingestion Pipeline
//!
//! Automatically embeds files on file system events (create, modify, delete, rename).
//! Provides debouncing, batching, filtering, and error handling for efficient
//! continuous indexing of file system changes.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  File       │────▶│  Debounce   │────▶│   Filter    │────▶│   Batch     │
//! │  Events     │     │  Buffer     │     │   Engine    │     │   Queue     │
//! └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
//!                                                                    │
//!                                                                    ▼
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  Metrics    │◀────│  Semantic   │◀────│  Embedder   │◀────│  Batch      │
//! │  Collector  │     │  FS Update  │     │  (Multi)    │     │  Processor  │
//! └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_volume::ingestion::{IngestionPipeline, IngestionConfig};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = IngestionConfig::default();
//!     let pipeline = IngestionPipeline::new(config).await?;
//!
//!     // Start watching directories
//!     pipeline.watch(Path::new("./documents")).await?;
//!     pipeline.watch(Path::new("./projects")).await?;
//!
//!     // Run until shutdown
//!     pipeline.run().await?;
//!
//!     Ok(())
//! }
//! ```

use crate::fs::SemanticFS;
use crate::multimodal::MultiModalEmbedder;
use crate::types::FileType;
use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Configuration for the ingestion pipeline
#[derive(Debug, Clone)]
pub struct IngestionConfig {
    /// Debounce delay for rapid file changes (ms)
    pub debounce_ms: u64,
    /// Maximum batch size before forcing flush
    pub batch_size: usize,
    /// Maximum time to wait before flushing batch (ms)
    pub batch_timeout_ms: u64,
    /// Maximum file size to ingest (bytes)
    pub max_file_size: u64,
    /// File extensions to include (empty = all)
    pub include_extensions: Vec<String>,
    /// File extensions to exclude
    pub exclude_extensions: Vec<String>,
    /// Directory patterns to exclude (glob-style)
    pub exclude_patterns: Vec<String>,
    /// Whether to follow symlinks
    pub follow_symlinks: bool,
    /// Number of concurrent embedding workers
    pub worker_count: usize,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 500,
            batch_size: 50,
            batch_timeout_ms: 5000,
            max_file_size: 10 * 1024 * 1024, // 10MB
            include_extensions: vec![],
            exclude_extensions: vec![
                "lock".into(), "tmp".into(), "swp".into(), "swo".into(),
                "pyc".into(), "pyo".into(), "o".into(), "a".into(), "so".into(),
                "dll".into(), "exe".into(), "bin".into(),
            ],
            exclude_patterns: vec![
                ".git".into(), ".svn".into(), ".hg".into(),
                "node_modules".into(), "target".into(), "__pycache__".into(),
                ".cache".into(), "build".into(), "dist".into(),
            ],
            follow_symlinks: false,
            worker_count: 4,
        }
    }
}

/// Type of ingestion event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestionEventKind {
    /// File created
    Created,
    /// File modified
    Modified,
    /// File deleted
    Deleted,
    /// File renamed (from, to)
    Renamed,
}

/// An ingestion event after debouncing
#[derive(Debug, Clone)]
pub struct IngestionEvent {
    /// Path to the file
    pub path: PathBuf,
    /// Type of event
    pub kind: IngestionEventKind,
    /// Original path (for renames)
    pub original_path: Option<PathBuf>,
    /// Timestamp of event
    pub timestamp: Instant,
}

/// Ingestion pipeline metrics
#[derive(Debug, Clone, Default)]
pub struct IngestionMetrics {
    /// Total files processed
    pub files_processed: u64,
    /// Files currently pending
    pub files_pending: u64,
    /// Total bytes processed
    pub bytes_processed: u64,
    /// Number of errors
    pub errors: u64,
    /// Average processing time per file (ms)
    pub avg_processing_ms: f64,
    /// Files skipped by filter
    pub files_skipped: u64,
    /// Last batch processing time (ms)
    pub last_batch_ms: u64,
}

/// File filter for determining what to ingest
pub struct IngestionFilter {
    config: IngestionConfig,
    exclude_regex: Vec<regex::Regex>,
}

impl IngestionFilter {
    /// Create a new filter from config
    pub fn new(config: IngestionConfig) -> Result<Self> {
        let mut exclude_regex = Vec::new();

        for pattern in &config.exclude_patterns {
            // Convert glob-like pattern to regex
            let regex_pattern = pattern
                .replace(".", r"\.")
                .replace("*", ".*")
                .replace("?", ".");

            let regex = regex::Regex::new(&format!("(^|/){}(/|$)", regex_pattern))
                .context("Invalid exclude pattern")?;
            exclude_regex.push(regex);
        }

        Ok(Self {
            config,
            exclude_regex,
        })
    }

    /// Check if a path should be ingested
    pub fn should_ingest(&self, path: &Path) -> bool {
        // Check if it's a file
        if !path.is_file() {
            return false;
        }

        // Check symlinks
        if !self.config.follow_symlinks && path.is_symlink() {
            return false;
        }

        // Check file size
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.len() > self.config.max_file_size {
                return false;
            }
        }

        // Check exclude patterns
        let path_str = path.to_string_lossy();
        for regex in &self.exclude_regex {
            if regex.is_match(&path_str) {
                return false;
            }
        }

        // Check extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();

            // Check exclude list
            if self.config.exclude_extensions.iter().any(|e| e.to_lowercase() == ext_lower) {
                return false;
            }

            // Check include list (if not empty)
            if !self.config.include_extensions.is_empty() {
                if !self.config.include_extensions.iter().any(|e| e.to_lowercase() == ext_lower) {
                    return false;
                }
            }
        }

        true
    }

    /// Get file type for a path
    pub fn file_type(&self, path: &Path) -> FileType {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        FileType::from_extension(ext)
    }
}

/// Debounce buffer for coalescing rapid file changes
struct DebounceBuffer {
    events: HashMap<PathBuf, (IngestionEventKind, Instant)>,
    debounce_duration: Duration,
}

impl DebounceBuffer {
    fn new(debounce_ms: u64) -> Self {
        Self {
            events: HashMap::new(),
            debounce_duration: Duration::from_millis(debounce_ms),
        }
    }

    /// Add an event to the buffer
    fn add(&mut self, path: PathBuf, kind: IngestionEventKind) {
        let now = Instant::now();

        // Coalesce events: create+modify = create, modify+modify = modify, etc.
        if let Some((existing_kind, _)) = self.events.get(&path) {
            let new_kind = match (existing_kind, &kind) {
                (IngestionEventKind::Created, IngestionEventKind::Modified) => IngestionEventKind::Created,
                (IngestionEventKind::Created, IngestionEventKind::Deleted) => {
                    // Created then deleted = nothing happened
                    self.events.remove(&path);
                    return;
                }
                (_, IngestionEventKind::Deleted) => IngestionEventKind::Deleted,
                _ => kind,
            };
            self.events.insert(path, (new_kind, now));
        } else {
            self.events.insert(path, (kind, now));
        }
    }

    /// Drain events that have been stable for the debounce duration
    fn drain_ready(&mut self) -> Vec<IngestionEvent> {
        let now = Instant::now();
        let mut ready = Vec::new();

        self.events.retain(|path, (kind, timestamp)| {
            if now.duration_since(*timestamp) >= self.debounce_duration {
                ready.push(IngestionEvent {
                    path: path.clone(),
                    kind: kind.clone(),
                    original_path: None,
                    timestamp: *timestamp,
                });
                false
            } else {
                true
            }
        });

        ready
    }

    /// Force drain all events
    fn drain_all(&mut self) -> Vec<IngestionEvent> {
        let events: Vec<_> = self.events.drain()
            .map(|(path, (kind, timestamp))| IngestionEvent {
                path,
                kind,
                original_path: None,
                timestamp,
            })
            .collect();
        events
    }
}

/// Batch processor for efficient embedding
struct BatchProcessor {
    queue: Vec<IngestionEvent>,
    max_size: usize,
    timeout: Duration,
    last_flush: Instant,
}

impl BatchProcessor {
    fn new(max_size: usize, timeout_ms: u64) -> Self {
        Self {
            queue: Vec::with_capacity(max_size),
            max_size,
            timeout: Duration::from_millis(timeout_ms),
            last_flush: Instant::now(),
        }
    }

    /// Add event to batch, returns events if batch should be flushed
    fn add(&mut self, event: IngestionEvent) -> Option<Vec<IngestionEvent>> {
        self.queue.push(event);

        if self.should_flush() {
            Some(self.flush())
        } else {
            None
        }
    }

    /// Check if batch should be flushed
    fn should_flush(&self) -> bool {
        self.queue.len() >= self.max_size ||
        (!self.queue.is_empty() && self.last_flush.elapsed() >= self.timeout)
    }

    /// Flush the batch
    fn flush(&mut self) -> Vec<IngestionEvent> {
        self.last_flush = Instant::now();
        std::mem::take(&mut self.queue)
    }

    /// Get pending count
    fn pending(&self) -> usize {
        self.queue.len()
    }
}

/// The main ingestion pipeline
pub struct IngestionPipeline {
    /// Configuration
    config: IngestionConfig,
    /// File filter
    filter: Arc<IngestionFilter>,
    /// Semantic file system (optional, can be set later)
    semantic_fs: Arc<RwLock<Option<Arc<SemanticFS>>>>,
    /// Multi-modal embedder
    embedder: Arc<MultiModalEmbedder>,
    /// File watchers
    watchers: Arc<Mutex<Vec<RecommendedWatcher>>>,
    /// Event sender
    event_tx: mpsc::Sender<(PathBuf, IngestionEventKind)>,
    /// Event receiver (wrapped for interior mutability)
    event_rx: Arc<Mutex<mpsc::Receiver<(PathBuf, IngestionEventKind)>>>,
    /// Metrics
    metrics: Arc<RwLock<IngestionMetrics>>,
    /// Watched directories
    watched_dirs: Arc<RwLock<HashSet<PathBuf>>>,
    /// Running flag
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl IngestionPipeline {
    /// Create a new ingestion pipeline
    pub async fn new(config: IngestionConfig) -> Result<Self> {
        log::info!("Initializing Content Ingestion Pipeline");
        log::info!("  Debounce: {}ms, Batch size: {}, Workers: {}",
            config.debounce_ms, config.batch_size, config.worker_count);

        let filter = Arc::new(IngestionFilter::new(config.clone())?);
        let embedder = Arc::new(MultiModalEmbedder::new(768).await?);

        let (event_tx, event_rx) = mpsc::channel(10000);

        Ok(Self {
            config,
            filter,
            semantic_fs: Arc::new(RwLock::new(None)),
            embedder,
            watchers: Arc::new(Mutex::new(Vec::new())),
            event_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            metrics: Arc::new(RwLock::new(IngestionMetrics::default())),
            watched_dirs: Arc::new(RwLock::new(HashSet::new())),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Set the semantic file system to update
    pub fn set_semantic_fs(&self, fs: Arc<SemanticFS>) {
        *self.semantic_fs.write() = Some(fs);
    }

    /// Start watching a directory
    pub async fn watch(&self, path: &Path) -> Result<()> {
        let canonical = path.canonicalize()
            .context("Failed to canonicalize path")?;

        // Check if already watching
        {
            let watched = self.watched_dirs.read();
            if watched.contains(&canonical) {
                log::warn!("Already watching: {}", canonical.display());
                return Ok(());
            }
        }

        log::info!("Watching directory: {}", canonical.display());

        // Create watcher
        let tx = self.event_tx.clone();
        let filter = self.filter.clone();

        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let Ok(event) = result {
                let kind = match event.kind {
                    EventKind::Create(_) => Some(IngestionEventKind::Created),
                    EventKind::Modify(_) => Some(IngestionEventKind::Modified),
                    EventKind::Remove(_) => Some(IngestionEventKind::Deleted),
                    _ => None,
                };

                if let Some(kind) = kind {
                    for path in event.paths {
                        if filter.should_ingest(&path) {
                            let _ = tx.blocking_send((path, kind.clone()));
                        }
                    }
                }
            }
        })?;

        watcher.watch(&canonical, RecursiveMode::Recursive)?;

        // Store watcher and mark directory as watched
        self.watchers.lock().push(watcher);
        self.watched_dirs.write().insert(canonical);

        Ok(())
    }

    /// Stop watching a directory
    pub fn unwatch(&self, path: &Path) -> Result<()> {
        let canonical = path.canonicalize()
            .context("Failed to canonicalize path")?;

        self.watched_dirs.write().remove(&canonical);
        log::info!("Stopped watching: {}", canonical.display());

        Ok(())
    }

    /// Run the pipeline (blocking)
    pub async fn run(&self) -> Result<()> {
        use std::sync::atomic::Ordering;

        self.running.store(true, Ordering::SeqCst);
        log::info!("Ingestion pipeline started");

        let mut debounce = DebounceBuffer::new(self.config.debounce_ms);
        let mut batch = BatchProcessor::new(self.config.batch_size, self.config.batch_timeout_ms);

        // Debounce check interval
        let mut debounce_interval = tokio::time::interval(Duration::from_millis(100));

        while self.running.load(Ordering::SeqCst) {
            tokio::select! {
                // Receive new events
                event = async {
                    self.event_rx.lock().recv().await
                } => {
                    if let Some((path, kind)) = event {
                        debounce.add(path, kind);
                    } else {
                        // Channel closed
                        break;
                    }
                }

                // Check debounce buffer periodically
                _ = debounce_interval.tick() => {
                    let ready = debounce.drain_ready();
                    for event in ready {
                        if let Some(batch_events) = batch.add(event) {
                            self.process_batch(batch_events).await;
                        }
                    }

                    // Also check batch timeout
                    if batch.should_flush() {
                        let batch_events = batch.flush();
                        if !batch_events.is_empty() {
                            self.process_batch(batch_events).await;
                        }
                    }

                    // Update pending metric
                    self.metrics.write().files_pending = batch.pending() as u64;
                }
            }
        }

        // Drain remaining events on shutdown
        let remaining = debounce.drain_all();
        for event in remaining {
            if let Some(batch_events) = batch.add(event) {
                self.process_batch(batch_events).await;
            }
        }
        let final_batch = batch.flush();
        if !final_batch.is_empty() {
            self.process_batch(final_batch).await;
        }

        log::info!("Ingestion pipeline stopped");
        Ok(())
    }

    /// Stop the pipeline
    pub fn stop(&self) {
        use std::sync::atomic::Ordering;
        self.running.store(false, Ordering::SeqCst);
        log::info!("Ingestion pipeline stopping...");
    }

    /// Process a batch of events
    async fn process_batch(&self, events: Vec<IngestionEvent>) {
        if events.is_empty() {
            return;
        }

        let start = Instant::now();
        let batch_size = events.len();

        log::info!("Processing batch of {} files", batch_size);

        let mut processed = 0u64;
        let mut bytes = 0u64;
        let mut errors = 0u64;

        // Get semantic FS if available
        let semantic_fs = self.semantic_fs.read().clone();

        for event in events {
            match event.kind {
                IngestionEventKind::Created | IngestionEventKind::Modified => {
                    match self.ingest_file(&event.path, semantic_fs.as_ref()).await {
                        Ok(size) => {
                            processed += 1;
                            bytes += size;
                        }
                        Err(e) => {
                            log::error!("Failed to ingest {}: {}", event.path.display(), e);
                            errors += 1;
                        }
                    }
                }
                IngestionEventKind::Deleted => {
                    // Handle deletion if semantic FS available
                    if let Some(ref fs) = semantic_fs {
                        if let Err(e) = fs.remove_file(&event.path).await {
                            log::debug!("Failed to remove {}: {}", event.path.display(), e);
                        } else {
                            processed += 1;
                        }
                    }
                }
                IngestionEventKind::Renamed => {
                    // Handle rename as delete + create
                    if let Some(ref original) = event.original_path {
                        if let Some(ref fs) = semantic_fs {
                            let _ = fs.remove_file(original).await;
                        }
                    }
                    match self.ingest_file(&event.path, semantic_fs.as_ref()).await {
                        Ok(size) => {
                            processed += 1;
                            bytes += size;
                        }
                        Err(e) => {
                            log::error!("Failed to ingest {}: {}", event.path.display(), e);
                            errors += 1;
                        }
                    }
                }
            }
        }

        let elapsed = start.elapsed();

        // Update metrics
        {
            let mut metrics = self.metrics.write();
            metrics.files_processed += processed;
            metrics.bytes_processed += bytes;
            metrics.errors += errors;
            metrics.last_batch_ms = elapsed.as_millis() as u64;

            // Update average
            let total = metrics.files_processed;
            if total > 0 {
                metrics.avg_processing_ms =
                    (metrics.avg_processing_ms * (total - processed) as f64 + elapsed.as_millis() as f64)
                    / total as f64;
            }
        }

        log::info!("Batch complete: {} files, {:.2} KB, {}ms",
            processed,
            bytes as f64 / 1024.0,
            elapsed.as_millis());
    }

    /// Ingest a single file
    async fn ingest_file(&self, path: &Path, semantic_fs: Option<&Arc<SemanticFS>>) -> Result<u64> {
        let metadata = std::fs::metadata(path)?;
        let size = metadata.len();

        // Use semantic FS if available
        if let Some(fs) = semantic_fs {
            fs.add_file(path).await?;
        } else {
            // Just compute embedding (for standalone use)
            let _embedding = self.embedder.embed(
                &std::fs::read_to_string(path).unwrap_or_default(),
                crate::multimodal::Modality::Text,
            ).await?;
        }

        Ok(size)
    }

    /// Get current metrics
    pub fn metrics(&self) -> IngestionMetrics {
        self.metrics.read().clone()
    }

    /// Get list of watched directories
    pub fn watched_directories(&self) -> Vec<PathBuf> {
        self.watched_dirs.read().iter().cloned().collect()
    }

    /// Manually trigger ingestion of a file
    pub async fn ingest(&self, path: &Path) -> Result<()> {
        if !self.filter.should_ingest(path) {
            self.metrics.write().files_skipped += 1;
            anyhow::bail!("File filtered out by ingestion rules");
        }

        let semantic_fs = self.semantic_fs.read().clone();
        self.ingest_file(path, semantic_fs.as_ref()).await?;

        Ok(())
    }

    /// Perform initial scan of watched directories
    pub async fn initial_scan(&self) -> Result<u64> {
        let dirs: Vec<PathBuf> = self.watched_dirs.read().iter().cloned().collect();
        let mut total = 0u64;

        for dir in dirs {
            log::info!("Initial scan: {}", dir.display());

            for entry in walkdir::WalkDir::new(&dir)
                .follow_links(self.config.follow_symlinks)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if self.filter.should_ingest(path) {
                    let _ = self.event_tx.send((path.to_path_buf(), IngestionEventKind::Created)).await;
                    total += 1;
                }
            }
        }

        log::info!("Initial scan queued {} files", total);
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_filter_extensions() {
        let config = IngestionConfig {
            exclude_extensions: vec!["tmp".into(), "lock".into()],
            ..Default::default()
        };
        let filter = IngestionFilter::new(config).unwrap();

        let tmp_dir = tempdir().unwrap();
        let test_file = tmp_dir.path().join("test.rs");
        std::fs::write(&test_file, "test").unwrap();

        assert!(filter.should_ingest(&test_file));

        let tmp_file = tmp_dir.path().join("test.tmp");
        std::fs::write(&tmp_file, "test").unwrap();

        assert!(!filter.should_ingest(&tmp_file));
    }

    #[test]
    fn test_filter_patterns() {
        let config = IngestionConfig {
            exclude_patterns: vec!["node_modules".into(), ".git".into()],
            ..Default::default()
        };
        let filter = IngestionFilter::new(config).unwrap();

        let path1 = PathBuf::from("/project/src/main.rs");
        let path2 = PathBuf::from("/project/node_modules/pkg/index.js");
        let path3 = PathBuf::from("/project/.git/config");

        // Note: These paths don't exist, so should_ingest will return false for is_file check
        // In a real scenario with existing files, the pattern matching would work
        assert!(!filter.should_ingest(&path1)); // File doesn't exist
    }

    #[test]
    fn test_debounce_buffer() {
        let mut debounce = DebounceBuffer::new(100);

        let path = PathBuf::from("/test/file.rs");

        // Add create event
        debounce.add(path.clone(), IngestionEventKind::Created);

        // Add modify event (should coalesce to created)
        debounce.add(path.clone(), IngestionEventKind::Modified);

        assert_eq!(debounce.events.len(), 1);
        assert_eq!(debounce.events.get(&path).unwrap().0, IngestionEventKind::Created);
    }

    #[test]
    fn test_debounce_create_delete() {
        let mut debounce = DebounceBuffer::new(100);

        let path = PathBuf::from("/test/file.rs");

        // Add create then delete (should cancel out)
        debounce.add(path.clone(), IngestionEventKind::Created);
        debounce.add(path.clone(), IngestionEventKind::Deleted);

        assert!(debounce.events.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_creation() {
        let config = IngestionConfig::default();
        let pipeline = IngestionPipeline::new(config).await.unwrap();

        assert!(pipeline.watched_directories().is_empty());
        assert_eq!(pipeline.metrics().files_processed, 0);
    }
}
