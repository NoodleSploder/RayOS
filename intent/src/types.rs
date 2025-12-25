//! RayOS Intent - Phase 5: Core Data Structures
//!
//! Types for natural language understanding, intent parsing, and cognitive reasoning.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;

/// Unique identifier for an intent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntentId(pub Uuid);

impl IntentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for IntentId {
    fn default() -> Self {
        Self::new()
    }
}

/// User intent parsed from natural language + context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: IntentId,
    pub command: Command,
    pub context: Context,
    pub confidence: f32,  // 0.0 - 1.0
    #[serde(skip, default = "Instant::now")]
    pub timestamp: Instant,
}

/// Parsed command from user input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Create something new
    Create {
        object_type: String,
        properties: HashMap<String, String>,
    },

    /// Modify existing object
    Modify {
        target: Target,
        operation: Operation,
    },

    /// Delete/remove
    Delete {
        target: Target,
    },

    /// Query/search
    Query {
        query: String,
        filters: Vec<Filter>,
    },

    /// Navigate to location
    Navigate {
        destination: String,
    },

    /// Execute action
    Execute {
        action: String,
        args: Vec<String>,
    },

    /// System configuration
    Configure {
        component: String,
        settings: HashMap<String, String>,
    },

    /// Compound command (multiple steps)
    Sequence {
        steps: Vec<Command>,
    },

    /// Ambiguous - needs clarification
    Ambiguous {
        possibilities: Vec<Command>,
        question: String,
    },
}

/// Target of an operation (with deictic resolution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Target {
    /// Direct file/object reference
    Direct { path: PathBuf },

    /// Visual reference ("that", "this")
    Deictic {
        gaze_position: Option<(f32, f32)>,  // Screen coordinates
        object_id: Option<String>,
    },

    /// Named entity
    Named { name: String },

    /// Query-based selection
    Query { query: String },

    /// Selection by ID
    Id { id: String },
}

/// Modification operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Rename { new_name: String },
    Move { destination: PathBuf },
    Edit { changes: Vec<Edit> },
    Optimize,
    Refactor,
    Custom { operation: String, params: HashMap<String, String> },
}

/// Edit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edit {
    pub location: EditLocation,
    pub change: EditChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditLocation {
    LineNumber(usize),
    Range { start: usize, end: usize },
    Pattern { pattern: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditChange {
    Insert { text: String },
    Replace { text: String },
    Delete,
}

/// Query filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub operator: FilterOperator,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOperator {
    Equals,
    Contains,
    StartsWith,
    EndsWith,
    GreaterThan,
    LessThan,
    Matches,  // Regex
}

/// Context from visual and auditory sensors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// Current gaze position
    pub gaze: Option<GazeContext>,

    /// Recent audio transcript
    pub audio: Option<AudioContext>,

    /// Visible objects on screen
    pub visual_objects: Vec<VisualObject>,

    /// Current application/window
    pub application: Option<String>,

    /// File system context
    pub filesystem: Option<FilesystemContext>,

    /// System state
    pub system: SystemContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GazeContext {
    pub position: (f32, f32),  // Screen coordinates
    pub focused_object: Option<String>,
    #[serde(skip, default = "Instant::now")]
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioContext {
    pub transcript: String,
    pub raw_audio: Vec<f32>,  // Optional waveform
    #[serde(skip, default = "Instant::now")]
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualObject {
    pub id: String,
    pub object_type: String,
    pub bounds: (f32, f32, f32, f32),  // (x, y, width, height)
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemContext {
    pub current_directory: PathBuf,
    pub open_files: Vec<PathBuf>,
    pub recent_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContext {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub active_tasks: usize,
}

/// Resource allocation policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub priority: Priority,
    pub resource_limits: ResourceLimits,
    pub constraints: Vec<Constraint>,
}

/// Priority level for resource allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Realtime = 0,    // User-facing, <16ms
    Interactive = 1, // User-initiated, <100ms
    Normal = 2,      // Background work
    Low = 3,         // Deferred work
    Idle = 4,        // Dream mode only
}

/// Resource allocation limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_cpu_percent: f32,
    pub max_memory_mb: f32,
    pub max_gpu_percent: Option<f32>,
    pub max_duration_ms: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_percent: 80.0,
            max_memory_mb: 1024.0,
            max_gpu_percent: Some(50.0),
            max_duration_ms: Some(5000),
        }
    }
}

/// Constraint on execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Constraint {
    /// Must complete before deadline
    Deadline {
        #[serde(skip, default = "Instant::now")]
        timestamp: Instant
    },

    /// Depends on other intents
    DependsOn { intent_ids: Vec<IntentId> },

    /// Requires specific resource
    RequiresResource { resource: String },

    /// Sandbox execution
    Sandboxed,

    /// Reversible (must support undo)
    Reversible,
}

/// Intent execution status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentStatus {
    Parsing,
    Clarifying { question: String },
    Planning,
    Executing,
    Completed,
    Failed { error: String },
    Cancelled,
}

/// Configuration for Intent system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConfig {
    /// Enable LLM-based parsing
    pub enable_llm: bool,

    /// LLM model path
    pub llm_model_path: Option<PathBuf>,

    /// Confidence threshold for auto-execution
    pub confidence_threshold: f32,

    /// Enable audio-visual fusion
    pub enable_fusion: bool,

    /// Default resource limits
    pub default_limits: ResourceLimits,

    /// Enable policy enforcement
    pub enforce_policy: bool,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enable_llm: false,  // Off by default, simulated mode
            llm_model_path: None,
            confidence_threshold: 0.8,
            enable_fusion: true,
            default_limits: ResourceLimits::default(),
            enforce_policy: true,
        }
    }
}

/// Parse result from intent parser
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub intent: Intent,
    pub alternatives: Vec<Intent>,
    pub needs_clarification: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_creation() {
        let intent = Intent {
            id: IntentId::new(),
            command: Command::Query {
                query: "find all rust files".to_string(),
                filters: vec![],
            },
            context: Context {
                gaze: None,
                audio: None,
                visual_objects: vec![],
                application: None,
                filesystem: None,
                system: SystemContext {
                    cpu_usage: 50.0,
                    memory_usage: 60.0,
                    active_tasks: 10,
                },
            },
            confidence: 0.95,
            timestamp: Instant::now(),
        };

        assert!(intent.confidence > 0.9);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Realtime < Priority::Interactive);
        assert!(Priority::Interactive < Priority::Normal);
        assert!(Priority::Normal < Priority::Low);
        assert!(Priority::Low < Priority::Idle);
    }

    #[test]
    fn test_deictic_target() {
        let target = Target::Deictic {
            gaze_position: Some((100.0, 200.0)),
            object_id: Some("file_123".to_string()),
        };

        assert!(matches!(target, Target::Deictic { .. }));
    }
}
