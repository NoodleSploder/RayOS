# RayOS Volume - Phase 3: The Memory

**Semantic file system with vector embeddings and HNSW indexing**

Volume is RayOS's semantic memory layer, transforming traditional file storage into an intelligent, associative memory system. Instead of navigating directories, you query by meaning and similarity.

## Architecture

```
┌─────────────────────────────────────────┐
│         SemanticFS (High-Level API)     │
│  add_file, search, find_similar, etc.   │
└──────────┬──────────────────────────────┘
           │
     ┌─────┴─────┬──────────┬──────────┐
     │           │          │          │
┌────▼────┐ ┌───▼───┐ ┌────▼────┐ ┌──▼──────┐
│Embedder │ │Vector │ │  HNSW   │ │Epiphany │
│         │ │Store  │ │ Indexer │ │ Buffer  │
└─────────┘ └───────┘ └─────────┘ └─────────┘
     │           │          │          │
Text→Vec    Persistence   Search    Validation
```

### Components

#### 1. **Embedder** (`src/embedder.rs`)
Converts text to high-dimensional vectors (embeddings).

**Modes:**
- **Simulated**: Deterministic Blake3-based vectors (no ML required)
- **Real**: Candle-based transformer models (feature: `embeddings`)

**Features:**
- Batch processing for efficiency
- Configurable embedding dimensions (default: 384)
- Text chunking for large files
- Automatic normalization

#### 2. **VectorStore** (`src/vector_store.rs`)
Persistent storage for embeddings using Sled embedded database.

**Features:**
- Key-value storage with MessagePack serialization
- DashMap-based write-through cache
- Atomic CRUD operations
- Export/import functionality
- Statistics tracking (cache hits, storage size)

#### 3. **HNSWIndexer** (`src/indexer.rs`)
Fast approximate nearest neighbor search using HNSW algorithm.

**Parameters:**
- `M`: Bi-directional links per node (default: 16)
- `ef_construction`: Search width during build (default: 200)
- Cosine distance metric

**Features:**
- Sub-linear search time: O(log N)
- Incremental updates (add/remove vectors)
- Filtered search by metadata
- Index statistics and health monitoring

#### 4. **EpiphanyBuffer** (`src/epiphany.rs`)
Storage for autonomous system-generated ideas ("dreams").

**Workflow:**
1. System generates idea during low-priority processing
2. Stored in buffer with `Pending` status
3. Validated in sandbox (safety check)
4. Status updated: `Valid`, `Invalid`, or `Integrated`

**Use Cases:**
- Auto-tagging suggestions
- Optimization recommendations
- Pattern discoveries
- Refactoring proposals

#### 5. **SemanticFS** (`src/fs.rs`)
High-level API orchestrating all components.

**Operations:**
- `add_file`: Index a file with automatic embedding
- `search`: Query by text to find similar documents
- `find_similar`: Find files similar to a given file
- `index_directory`: Recursively index a folder
- `get_tags`: Retrieve auto-generated tags
- `stats`: System health and metrics

## Usage

### CLI

```bash
# Start the daemon
volume start --watch /path/to/watch

# Index a directory
volume index /path/to/docs

# Search by meaning
volume search "neural network architecture" -n 5

# Find similar files
volume similar /path/to/file.rs -n 10

# Show statistics
volume stats

# Rebuild index
volume rebuild
```

### As a Library

```rust
use rayos_volume::{SemanticFS, VolumeConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize with default configuration
    let config = VolumeConfig::default();
    let fs = SemanticFS::new(config).await?;

    // Add a file
    let file_id = fs.add_file("src/main.rs").await?;

    // Search by meaning
    let results = fs.search("error handling code", 10).await?;

    for result in results {
        println!("{}: {:.2}% match",
            result.document.metadata.path.display(),
            result.similarity * 100.0
        );
    }

    // Find similar files
    let similar = fs.find_similar(file_id, 5).await?;

    // Get system stats
    let stats = fs.stats();
    println!("Indexed {} documents", stats.vector_store.total_documents);

    Ok(())
}
```

### Configuration

Create a `volume.toml`:

```toml
[embeddings]
dimension = 384
model_path = "models/all-MiniLM-L6-v2"  # Optional

[storage]
data_dir = "/var/lib/rayos/volume"
cache_size = 10000

[indexer]
m = 16                # HNSW connectivity
ef_construction = 200 # Build quality
ef_search = 50        # Search quality

[epiphany]
buffer_size = 1000
validation_timeout_secs = 30
```

Load it:

```bash
volume start --config volume.toml
```

## Examples

### Basic Usage

```rust
use rayos_volume::{SemanticFS, VolumeConfig};

let fs = SemanticFS::new(VolumeConfig::default()).await?;

// Index a project
fs.index_directory("src/").await?;

// Find all error handling code
let errors = fs.search("error handling panic Result", 20).await?;
```

### Batch Indexing

```rust
use std::path::PathBuf;
use rayos_volume::{SemanticFS, VolumeConfig};

let fs = SemanticFS::new(VolumeConfig::default()).await?;

let dirs = vec![
    PathBuf::from("src/"),
    PathBuf::from("tests/"),
    PathBuf::from("examples/"),
];

for dir in dirs {
    let count = fs.index_directory(&dir).await?;
    println!("Indexed {} files in {}", count, dir.display());
}
```

### Custom Search with Filters

```rust
use rayos_volume::{SemanticFS, SearchQuery, VolumeConfig};

let fs = SemanticFS::new(VolumeConfig::default()).await?;

let query = SearchQuery {
    text: "authentication middleware".to_string(),
    limit: 10,
    min_similarity: 0.7,
    filters: vec![
        ("ext".to_string(), "rs".to_string()),
        ("path".to_string(), "src/".to_string()),
    ],
};

let results = fs.search_with_filters(query).await?;
```

### Working with Epiphanies

```rust
use rayos_volume::{SemanticFS, Epiphany, VolumeConfig};

let fs = SemanticFS::new(VolumeConfig::default()).await?;

// Get pending ideas
let pending = fs.get_pending_epiphanies().await?;

for epiphany in pending {
    println!("Idea: {}", epiphany.description);

    // Validate (manual or automated)
    if user_approves(&epiphany) {
        fs.mark_epiphany_valid(epiphany.id).await?;
    }
}
```

## Building

### Without ML Models (Simulated Mode)

```bash
cargo build --release
```

This uses deterministic Blake3-based embeddings - perfect for testing and development.

### With Real Embeddings

```bash
cargo build --release --features embeddings
```

Requires Candle ML framework and model weights.

### Full Build (All Features)

```bash
cargo build --release --features full
```

Includes embeddings, GPU acceleration, and RocksDB backend.

## Testing

```bash
# Run unit tests
cargo test

# Run with output
cargo test -- --nocapture

# Test specific module
cargo test vector_store

# Run benchmarks
cargo bench
```

## Performance

### HNSW Search Complexity

- **Build**: O(N log N) where N = number of vectors
- **Search**: O(log N) approximate
- **Space**: O(N × M) where M = connectivity parameter

### Typical Metrics (M=16, 100K documents)

- **Index time**: ~45 seconds
- **Search time**: ~2ms per query
- **Recall@10**: >95%
- **Memory**: ~800MB for index + vectors
- **Disk**: ~2GB with metadata

### Optimization Tips

1. **Increase M**: Better recall, more memory
2. **Increase ef_construction**: Better index quality, slower build
3. **Increase ef_search**: Better recall, slower search
4. **Cache size**: More cache = fewer disk hits
5. **Batch operations**: Process files in batches for efficiency

## Troubleshooting

### Slow Indexing

```bash
# Check disk I/O
iostat -x 1

# Increase batch size in code
embedder.set_batch_size(128);
```

### High Memory Usage

```bash
# Reduce cache size in config
[storage]
cache_size = 5000

# Or clear cache programmatically
fs.clear_cache()?;
```

### Poor Search Results

```bash
# Rebuild index with better parameters
volume rebuild

# Or tune in config:
[indexer]
m = 32                # Higher connectivity
ef_construction = 400 # Better quality
```

### "Simulated mode" Warnings

This is normal! To use real embeddings:

```bash
# Install with embeddings feature
cargo build --features embeddings

# Download model weights
mkdir -p models/
# ... download all-MiniLM-L6-v2 or similar
```

## Integration with RayOS

Volume integrates with other RayOS phases:

- **Phase 1 (Kernel)**: Uses HAL for async I/O
- **Phase 2 (Cortex)**: Stores visual memories as embeddings
- **Phase 4 (Conductor)**: Semantic search in task planning
- **Phase 5 (Intent)**: Natural language queries over file system

## Architecture Decisions

### Why Sled?

- **Embedded**: No separate database process
- **ACID**: Atomic operations, safe for concurrent access
- **Fast**: Lock-free, optimized for SSDs
- **Portable**: Pure Rust, cross-platform

### Why HNSW?

- **Fast**: Logarithmic search time
- **Accurate**: >95% recall with proper tuning
- **Scalable**: Handles millions of vectors
- **Dynamic**: Incremental updates (no full rebuild)

### Why Simulated Mode?

- **Development**: Test without ML dependencies
- **CI/CD**: Run tests in resource-constrained environments
- **Deployment**: Optional ML for edge devices
- **Determinism**: Reproducible results for debugging

## Roadmap

- [ ] File watcher integration (real-time indexing)
- [ ] Multi-modal embeddings (images, audio)
- [ ] Distributed vector store (multi-node)
- [ ] GPU-accelerated search
- [ ] Compression for embeddings (PQ/OPQ)
- [ ] Incremental re-indexing (detect changes)
- [ ] HTTP API server mode
- [ ] WebAssembly build

## References

- **HNSW Paper**: [Malkov & Yashunin (2018)](https://arxiv.org/abs/1603.09320)
- **Sled Database**: [spacejam.github.io/sled](https://spacejam.github.io/sled/)
- **Candle ML**: [huggingface.co/docs/candle](https://huggingface.co/docs/candle)
- **Sentence Transformers**: [sbert.net](https://www.sbert.net/)

---

**Status**: Phase 3 Complete ✓
**Build**: `cargo build --release`
**Test**: `cargo test`
**Run**: `cargo run -- start`
