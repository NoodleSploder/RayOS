//! RayOS Intent - Phase 5: Natural Language Understanding
//!
//! The Intent system translates natural language + sensory context into executable commands.
//!
//! # Architecture
//!
//! - **Intent Parser**: NL → TaskStruct translation with deictic resolution
//! - **Policy Arbiter**: Dynamic resource allocation based on system load
//! - **Context Manager**: Audio-visual sensor fusion
//! - **LLM Connector**: Optional neural language understanding
//!
//! # Usage
//!
//! ```rust,no_run
//! use rayos_intent::{IntentEngine, IntentConfig};
//!
//! let config = IntentConfig::default();
//! let mut engine = IntentEngine::new(config);
//!
//! // Parse user command
//! let result = engine.parse("find all rust files");
//!
//! if result.needs_clarification {
//!     println!("Clarification needed");
//! } else {
//!     // Execute intent
//!     engine.execute(result.intent);
//! }
//! ```

pub mod types;
pub mod intent_parser;
pub mod policy_arbiter;
pub mod context_manager;
pub mod llm_connector;

#[cfg(feature = "conductor")]
pub mod conductor_bridge;

pub use types::*;
use intent_parser::IntentParser;
use policy_arbiter::PolicyArbiter;
use context_manager::ContextManager;
use llm_connector::LLMConnector;

use std::sync::Arc;
use parking_lot::RwLock;

/// Main intent engine that coordinates all components
pub struct IntentEngine {
    config: IntentConfig,
    parser: Arc<RwLock<IntentParser>>,
    arbiter: Arc<PolicyArbiter>,
    context_manager: Arc<ContextManager>,
    llm: Arc<RwLock<LLMConnector>>,
    active_intents: Arc<RwLock<Vec<Intent>>>,
}

impl IntentEngine {
    /// Create new intent engine
    pub fn new(config: IntentConfig) -> Self {
        let parser = Arc::new(RwLock::new(IntentParser::new(config.clone())));
        let arbiter = Arc::new(PolicyArbiter::new(config.clone()));
        let context_manager = Arc::new(ContextManager::new(config.clone()));
        let llm = Arc::new(RwLock::new(LLMConnector::new(config.llm_model_path.clone())));

        Self {
            config,
            parser,
            arbiter,
            context_manager,
            llm,
            active_intents: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Initialize engine (load LLM if enabled)
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.config.enable_llm {
            println!("Initializing LLM...");
            self.llm.write().initialize()?;
        }

        println!("Intent engine initialized");
        Ok(())
    }

    /// Parse user input into intent
    pub fn parse(&self, input: &str) -> ParseResult {
        // Build current context with real system metrics
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();

        let cpu_usage = sys.global_cpu_usage();
        let memory_usage = (sys.used_memory() as f32 / sys.total_memory() as f32) * 100.0;

        let system = SystemContext {
            cpu_usage,
            memory_usage,
            active_tasks: self.active_intents.read().len(),
        };

        let context = self.context_manager.build_context(system);

        // Try LLM first if enabled
        if self.config.enable_llm {
            if let Some(result) = self.llm.read().parse(input, &context) {
                return result;
            }
        }

        // Fall back to pattern matching
        self.parser.write().parse(input, context)
    }

    /// Parse with explicit context
    pub fn parse_with_context(&self, input: &str, context: Context) -> ParseResult {
        // Try LLM first
        if self.config.enable_llm {
            if let Some(result) = self.llm.read().parse(input, &context) {
                return result;
            }
        }

        // Fall back to patterns
        self.parser.write().parse(input, context)
    }

    /// Execute an intent
    pub fn execute(&self, intent: Intent) -> Result<IntentStatus, String> {
        // Allocate resources via policy arbiter
        let policy = self.arbiter.allocate(&intent);

        // Check if should execute
        if !self.arbiter.should_execute(&intent, &policy) {
            return Ok(IntentStatus::Clarifying {
                question: format!("Intent confidence too low ({:.2}). Please clarify.", intent.confidence),
            });
        }

        // Add to active intents
        self.active_intents.write().push(intent.clone());

        // Execute based on command type
        let result = match &intent.command {
            Command::Query { query, filters } => {
                // Perform actual search/query operation
                println!("[Intent] Executing query: {} with {} filters", query, filters.len());

                // Try to find files matching the query
                let search_paths = vec![".", "./src", "./tests"];
                let mut found_files = Vec::new();

                for search_path in search_paths {
                    if let Ok(entries) = std::fs::read_dir(search_path) {
                        for entry in entries.flatten() {
                            if let Ok(path) = entry.path().canonicalize() {
                                let path_str = path.to_string_lossy().to_lowercase();
                                if path_str.contains(&query.to_lowercase()) {
                                    found_files.push(path);
                                    if found_files.len() >= 10 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                if !found_files.is_empty() {
                    println!("[Intent] Found {} matching files", found_files.len());
                    for (i, file) in found_files.iter().take(5).enumerate() {
                        println!("  {}. {}", i + 1, file.display());
                    }
                }

                Ok(IntentStatus::Completed)
            }
            Command::Create { object_type, properties } => {
                // Create new object/file
                println!("[Intent] Creating {} with {} properties", object_type, properties.len());

                if object_type.contains("file") || object_type.contains("document") {
                    // Extract filename from properties
                    if let Some(name) = properties.get("name") {
                        let filename = format!("./{}", name);
                        match std::fs::write(&filename, b"# New file\n\nCreated by RayOS Intent\n") {
                            Ok(_) => {
                                println!("[Intent] Successfully created: {}", filename);
                                Ok(IntentStatus::Completed)
                            }
                            Err(e) => {
                                println!("[Intent] Failed to create file: {}", e);
                                Err(format!("Failed to create file: {}", e))
                            }
                        }
                    } else {
                        Ok(IntentStatus::Completed)
                    }
                } else if object_type.contains("directory") || object_type.contains("folder") {
                    if let Some(name) = properties.get("name") {
                        let dirpath = format!("./{}", name);
                        match std::fs::create_dir_all(&dirpath) {
                            Ok(_) => {
                                println!("[Intent] Successfully created directory: {}", dirpath);
                                Ok(IntentStatus::Completed)
                            }
                            Err(e) => {
                                println!("[Intent] Failed to create directory: {}", e);
                                Err(format!("Failed to create directory: {}", e))
                            }
                        }
                    } else {
                        Ok(IntentStatus::Completed)
                    }
                } else {
                    Ok(IntentStatus::Completed)
                }
            }
            Command::Modify { target, operation } => {
                // Modify existing object
                println!("[Intent] Modifying {:?} with operation {:?}", target, operation);
                Ok(IntentStatus::Completed)
            }
            Command::Delete { target } => {
                // Delete object
                println!("[Intent] Deleting {:?}", target);
                Ok(IntentStatus::Completed)
            }
            Command::Navigate { destination } => {
                // Navigate to location
                println!("[Intent] Navigating to {}", destination);
                Ok(IntentStatus::Completed)
            }
            Command::Execute { action, args } => {
                // Execute actual commands
                println!("[Intent] Executing {} with {} args", action, args.len());

                // Execute shell commands (safely)
                let safe_commands = ["ls", "pwd", "whoami", "date", "echo"];

                if safe_commands.contains(&action.as_str()) {
                    match std::process::Command::new(action)
                        .args(args)
                        .output()
                    {
                        Ok(output) => {
                            if output.status.success() {
                                let stdout = String::from_utf8_lossy(&output.stdout);
                                println!("[Intent] Command output:\n{}", stdout);
                                Ok(IntentStatus::Completed)
                            } else {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                println!("[Intent] Command failed: {}", stderr);
                                Err(format!("Command failed: {}", stderr))
                            }
                        }
                        Err(e) => {
                            println!("[Intent] Failed to execute: {}", e);
                            Err(format!("Failed to execute: {}", e))
                        }
                    }
                } else {
                    println!("[Intent] Command '{}' not in safe list, simulating execution", action);
                    Ok(IntentStatus::Completed)
                }
            }
            Command::Configure { component, settings } => {
                // Configure system
                println!("[Intent] Configuring {} with {} settings", component, settings.len());
                Ok(IntentStatus::Completed)
            }
            Command::Sequence { steps } => {
                // Execute sequence
                println!("[Intent] Executing sequence of {} steps", steps.len());
                for (i, step) in steps.iter().enumerate() {
                    println!("  Step {}: {:?}", i + 1, step);
                }
                Ok(IntentStatus::Completed)
            }
            Command::Ambiguous { possibilities: _, question } => {
                // Need clarification
                Ok(IntentStatus::Clarifying {
                    question: question.clone(),
                })
            }
        };

        // Remove from active intents
        self.active_intents.write().retain(|i| i.id != intent.id);

        result
    }

    /// Allocate a resource policy for an intent without executing it.
    pub fn allocate_resources(&self, intent: &Intent) -> Policy {
        self.arbiter.allocate(intent)
    }

    /// Update gaze from eye tracker
    pub fn update_gaze(&self, position: (f32, f32), focused_object: Option<String>) {
        self.context_manager.update_gaze(position, focused_object);
    }

    /// Update audio from microphone
    pub fn update_audio(&self, transcript: String, raw_audio: Vec<f32>) {
        self.context_manager.update_audio(transcript, raw_audio);
    }

    /// Update visual objects from screen
    pub fn update_visual_objects(&self, objects: Vec<VisualObject>) {
        self.context_manager.update_visual_objects(objects);
    }

    /// Update system metrics
    pub fn update_system_state(&self, cpu: f32, memory: f32, gpu: Option<f32>) {
        let active = self.active_intents.read().len();
        self.arbiter.update_system_state(cpu, memory, gpu, active);
    }

    /// Resolve deictic reference
    pub fn resolve_deictic(&self, reference: &str) -> Option<Target> {
        self.context_manager.resolve_deictic(reference)
    }

    /// Get active intents
    pub fn get_active_intents(&self) -> Vec<Intent> {
        self.active_intents.read().clone()
    }

    /// Get system load factor
    pub fn get_load_factor(&self) -> f32 {
        self.arbiter.get_load_factor()
    }

    /// Get LLM availability
    pub fn is_llm_available(&self) -> bool {
        self.llm.read().is_available()
    }

    /// Get engine info
    pub fn info(&self) -> String {
        format!(
            "Intent Engine\n\
             LLM: {}\n\
             Active Intents: {}\n\
             Load Factor: {:.2}",
            self.llm.read().model_info(),
            self.active_intents.read().len(),
            self.get_load_factor()
        )
    }
}

/// Builder for IntentConfig
pub struct IntentConfigBuilder {
    config: IntentConfig,
}

impl IntentConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: IntentConfig::default(),
        }
    }

    pub fn enable_llm(mut self, enable: bool) -> Self {
        self.config.enable_llm = enable;
        self
    }

    pub fn llm_model_path(mut self, path: std::path::PathBuf) -> Self {
        self.config.llm_model_path = Some(path);
        self
    }

    pub fn confidence_threshold(mut self, threshold: f32) -> Self {
        self.config.confidence_threshold = threshold;
        self
    }

    pub fn enable_fusion(mut self, enable: bool) -> Self {
        self.config.enable_fusion = enable;
        self
    }

    pub fn enforce_policy(mut self, enforce: bool) -> Self {
        self.config.enforce_policy = enforce;
        self
    }

    pub fn build(self) -> IntentConfig {
        self.config
    }
}

impl Default for IntentConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let config = IntentConfig::default();
        let engine = IntentEngine::new(config);

        assert!(!engine.is_llm_available());
        assert_eq!(engine.get_active_intents().len(), 0);
    }

    #[test]
    fn test_parse_simple_query() {
        let config = IntentConfig::default();
        let engine = IntentEngine::new(config);

        let result = engine.parse("find rust files");

        assert!(matches!(result.intent.command, Command::Query { .. }));
    }

    #[test]
    fn test_gaze_update() {
        let config = IntentConfig::default();
        let engine = IntentEngine::new(config);

        engine.update_gaze((100.0, 200.0), Some("file.rs".to_string()));

        let target = engine.resolve_deictic("that");
        assert!(target.is_some());
    }

    #[test]
    fn test_config_builder() {
        let config = IntentConfigBuilder::new()
            .enable_llm(false)
            .confidence_threshold(0.9)
            .enable_fusion(true)
            .enforce_policy(false)
            .build();

        assert!(!config.enable_llm);
        assert_eq!(config.confidence_threshold, 0.9);
        assert!(config.enable_fusion);
        assert!(!config.enforce_policy);
    }

    #[test]
    fn test_system_state_update() {
        let config = IntentConfig::default();
        let engine = IntentEngine::new(config);

        engine.update_system_state(50.0, 60.0, Some(40.0));

        let load = engine.get_load_factor();
        assert!(load > 0.0 && load < 1.0);
    }
}
