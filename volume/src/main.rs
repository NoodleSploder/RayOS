//! RayOS Volume - Phase 3: The Memory
//!
//! Main entry point for the semantic file system daemon.

use rayos_volume::{SemanticFS, VolumeConfig};
use anyhow::Result;
use std::path::PathBuf;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "volume")]
#[command(about = "RayOS Volume - Semantic File System", long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the semantic file system daemon
    Start {
        /// Watch directory for changes
        #[arg(short, long, value_name = "DIR")]
        watch: Option<PathBuf>,
    },

    /// Index a directory
    Index {
        /// Directory to index
        directory: PathBuf,
    },

    /// Search for files
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },

    /// Find similar files
    Similar {
        /// Path to the reference file
        file: PathBuf,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },

    /// Show statistics
    Stats,

    /// Rebuild the index
    Rebuild,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("═══════════════════════════════════════");
    log::info!("  RayOS Volume - Phase 3: The Memory");
    log::info!("═══════════════════════════════════════");

    let cli = Cli::parse();

    // Load configuration
    let config = if let Some(config_path) = cli.config {
        log::info!("Loading config from: {}", config_path.display());
        let content = std::fs::read_to_string(config_path)?;
        toml::from_str(&content)?
    } else {
        VolumeConfig::default()
    };

    // Initialize semantic file system
    let fs = SemanticFS::new(config).await?;

    match cli.command {
        Commands::Start { watch } => {
            log::info!("Starting Volume daemon...");

            if let Some(watch_dir) = watch {
                log::info!("Watching directory: {}", watch_dir.display());

                // Set up file watcher using notify
                use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event};
                use std::sync::mpsc::channel;

                let (tx, rx) = channel();
                let mut watcher: RecommendedWatcher = notify::recommended_watcher(tx)?;
                watcher.watch(&watch_dir, RecursiveMode::Recursive)?;

                // Spawn file watcher task
                let fs_clone = fs.clone();
                tokio::spawn(async move {
                    loop {
                        match rx.recv() {
                            Ok(event_result) => {
                                match event_result {
                                    Ok(event) => {
                                        // Handle file system events
                                        for path in event.paths {
                                            if path.is_file() {
                                                log::info!(\"File changed: {}\", path.display());
                                                if let Err(e) = fs_clone.index_file(&path).await {
                                                    log::error!(\"Failed to index {}: {}\", path.display(), e);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => log::error!(\"Watch error: {}\", e),
                                }
                            }
                            Err(e) => {
                                log::error!(\"Channel error: {}\", e);
                                break;
                            }
                        }
                    }
                });

                log::info!(\"File watcher active on {}\", watch_dir.display());
            }

            // Keep running
            tokio::signal::ctrl_c().await?;
            log::info!("Shutting down...");
        }

        Commands::Index { directory } => {
            log::info!("Indexing directory: {}", directory.display());
            let count = fs.index_directory(&directory).await?;
            log::info!("Successfully indexed {} files", count);
        }

        Commands::Search { query, limit } => {
            log::info!("Searching for: {}", query);
            let results = fs.search(&query, limit).await?;

            println!("\nFound {} results:\n", results.len());
            for (i, result) in results.iter().enumerate() {
                println!("{}. {} (similarity: {:.2}%)",
                    i + 1,
                    result.document.metadata.path.display(),
                    result.similarity * 100.0
                );
            }
        }

        Commands::Similar { file, limit } => {
            // First add the file if not already indexed
            let file_id = fs.add_file(&file).await?;

            log::info!("Finding files similar to: {}", file.display());
            let results = fs.find_similar(file_id, limit).await?;

            println!("\nFound {} similar files:\n", results.len());
            for (i, result) in results.iter().enumerate() {
                println!("{}. {} (similarity: {:.2}%)",
                    i + 1,
                    result.document.metadata.path.display(),
                    result.similarity * 100.0
                );
            }
        }

        Commands::Stats => {
            let stats = fs.stats();

            println!("\n=== Vector Store ===");
            println!("Documents: {}", stats.vector_store.total_documents);
            println!("Embeddings: {}", stats.vector_store.total_embeddings);
            println!("Cache hits: {}", stats.vector_store.cache_hits);
            println!("Cache misses: {}", stats.vector_store.cache_misses);
            println!("Storage: {} MB", stats.vector_store.bytes_stored / 1024 / 1024);

            println!("\n=== HNSW Index ===");
            println!("Vectors: {}", stats.index.total_vectors);
            println!("Dimension: {}", stats.index.dimension);
            println!("M: {}", stats.index.m);
            println!("ef_construction: {}", stats.index.ef_construction);

            println!("\n=== Epiphany Buffer ===");
            println!("Total: {}", stats.epiphany.total);
            println!("Pending: {}", stats.epiphany.pending);
            println!("Valid: {}", stats.epiphany.valid);
            println!("Invalid: {}", stats.epiphany.invalid);
            println!("Integrated: {}", stats.epiphany.integrated);
        }

        Commands::Rebuild => {
            log::info!("Rebuilding index...");
            fs.rebuild_index()?;
            log::info!("Index rebuilt successfully");
        }
    }

    Ok(())
}
