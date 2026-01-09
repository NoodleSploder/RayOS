//! Unified Perception/Logic Pipeline
//!
//! This module provides a single RT (ray tracing) pipeline that unifies:
//! - **Perception**: Processing visual/sensor input
//! - **Logic**: Executing decisions via geometric hit tests
//! - **Semantics**: Computing similarity in embedding space
//!
//! The key insight is that all three operations can be expressed as GPU compute
//! dispatches with the same underlying pattern: parallel element processing.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    UNIFIED RT PIPELINE                                   │
//! │                                                                          │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
//! │  │ Perception  │  │   Logic     │  │  Semantic   │  │   Custom    │     │
//! │  │   Stage     │  │   Stage     │  │   Stage     │  │   Stage     │     │
//! │  │             │  │             │  │             │  │             │     │
//! │  │ • Vision    │  │ • ACL check │  │ • Similarity│  │ • User-     │     │
//! │  │ • Sensors   │  │ • Reflexes  │  │ • Embedding │  │   defined   │     │
//! │  │ • Events    │  │ • Decisions │  │ • KNN       │  │   compute   │     │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘     │
//! │         │                │                │                │            │
//! │         └────────────────┴────────────────┴────────────────┘            │
//! │                                   │                                      │
//! │                          ┌────────▼────────┐                             │
//! │                          │  Pipeline Core  │                             │
//! │                          │   (GPU Compute) │                             │
//! │                          │                 │                             │
//! │                          │ • Batch dispatch│                             │
//! │                          │ • Memory pools  │                             │
//! │                          │ • Async readback│                             │
//! │                          └────────┬────────┘                             │
//! │                                   │                                      │
//! │                          ┌────────▼────────┐                             │
//! │                          │   Output        │                             │
//! │                          │   Aggregation   │                             │
//! │                          └─────────────────┘                             │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_kernel::unified_pipeline::{UnifiedPipeline, PipelineStage, StageInput};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create unified pipeline
//!     let mut pipeline = UnifiedPipeline::new();
//!     pipeline.initialize(&device, &queue).await?;
//!
//!     // Submit work to any stage
//!     let perception = StageInput::perception(visual_data);
//!     let logic = StageInput::logic(access_queries);
//!     let semantic = StageInput::semantic(embedding_queries);
//!
//!     // Execute all stages in a single dispatch
//!     let results = pipeline.execute(&[perception, logic, semantic]).await?;
//!
//!     Ok(())
//! }
//! ```

mod core;
mod stages;

pub use core::{
    UnifiedPipeline, PipelineConfig, PipelineStats, PipelineCapabilities,
    MemoryPool, BufferHandle,
};
pub use stages::{
    PipelineStage, StageInput, StageOutput, StageType,
    PerceptionInput, PerceptionOutput,
    LogicInput, LogicOutput,
    SemanticInput, SemanticOutput,
    CustomInput, CustomOutput,
};
