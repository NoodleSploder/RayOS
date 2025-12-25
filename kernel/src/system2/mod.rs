/// System 2: The Cognitive Engine - "The Conscious Layer"
///
/// Handles intent analysis, complex decision making, and policy setting.
/// This is where the LLM integration lives.

pub mod intent;
pub mod policy;

use crate::types::{LogicRay, Priority, KernelConfig};
use anyhow::Result;
use glam::Vec3;
use std::sync::Arc;

/// System 2 - The Cognitive Engine
pub struct CognitiveEngine {
    /// Intent parser
    intent_parser: Arc<IntentParser>,

    /// Policy arbiter
    policy_arbiter: Arc<PolicyArbiter>,

    /// Configuration
    config: KernelConfig,
}

impl CognitiveEngine {
    /// Create a new Cognitive Engine
    pub fn new(config: KernelConfig) -> Self {
        log::info!("Initializing System 2: Cognitive Engine");

        Self {
            intent_parser: Arc::new(IntentParser::new()),
            policy_arbiter: Arc::new(PolicyArbiter::new()),
            config,
        }
    }

    /// Parse user intent into a task bundle
    pub async fn parse_intent(&self, input: &str) -> Result<Vec<LogicRay>> {
        self.intent_parser.parse(input).await
    }

    /// Determine resource allocation policy
    pub fn arbitrate_policy(&self, system_load: f32) -> PolicyDecision {
        self.policy_arbiter.decide(system_load)
    }

    /// Process multimodal input (vision + audio)
    pub async fn process_multimodal(
        &self,
        text: Option<&str>,
        gaze: Option<(f32, f32)>,
    ) -> Result<Vec<LogicRay>> {
        log::debug!("Processing multimodal input: text={:?}, gaze={:?}", text, gaze);

        let mut rays = Vec::new();

        // Parse text intent if available
        if let Some(text) = text {
            rays.extend(self.parse_intent(text).await?);
        }

        // Convert gaze to intent vector if available
        if let Some((x, y)) = gaze {
            let gaze_ray = self.gaze_to_ray(x, y);
            rays.push(gaze_ray);
        }

        Ok(rays)
    }

    /// Convert gaze coordinates to a logic ray
    fn gaze_to_ray(&self, x: f32, y: f32) -> LogicRay {
        // Map 2D gaze to 3D ray direction
        let origin = Vec3::new(0.0, 0.0, 0.0);
        let direction = Vec3::new(x, y, 1.0).normalize();

        LogicRay::new(
            origin,
            direction,
            self.generate_task_id(),
            Priority::High,
            0, // No data payload yet
            0, // Default logic tree
        )
    }

    fn generate_task_id(&self) -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TASK_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
        TASK_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    }
}

/// Intent Parser - translates natural language to ray bundles
pub struct IntentParser {
    // In a real implementation, this would hold the LLM
    initialized: bool,
}

impl IntentParser {
    pub fn new() -> Self {
        Self {
            initialized: false,
        }
    }

    /// Parse text into task rays
    pub async fn parse(&self, input: &str) -> Result<Vec<LogicRay>> {
        log::debug!("Parsing intent: '{}'", input);

        // Implement LLM-style intent parsing simulation
        // In production, this would call a real LLM (local or API)

        let input_lower = input.to_lowercase();
        let mut rays = Vec::new();

        // Analyze input for action keywords
        let priority = if input_lower.contains("urgent") || input_lower.contains("now") {
            Priority::High
        } else if input_lower.contains("later") || input_lower.contains("eventually") {
            Priority::Low
        } else {
            Priority::Normal
        };

        // Generate rays based on intent type
        if input_lower.contains("open") || input_lower.contains("launch") {
            // File/App opening intent
            rays.push(LogicRay::new(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),  // Direction indicates action type
                Self::hash_string(input),
                priority,
                1,  // Open operation data
                0,  // Default logic tree
            ));
        } else if input_lower.contains("close") || input_lower.contains("exit") {
            rays.push(LogicRay::new(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0),  // Negative direction for close
                Self::hash_string(input),
                priority,
                2,  // Close operation
                0,
            ));
        } else if input_lower.contains("search") || input_lower.contains("find") {
            // Search intent - multiple rays for parallel search
            for i in 0..3 {
                rays.push(LogicRay::new(
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0).normalize(),
                    Self::hash_string(&format!("{}-{}", input, i)),
                    priority,
                    3,  // Search operation
                    i as u32,  // Different search domains
                ));
            }
        } else if input_lower.contains("write") || input_lower.contains("create") {
            rays.push(LogicRay::new(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),  // Z direction for creation
                Self::hash_string(input),
                priority,
                4,  // Create operation
                0,
            ));
        } else {
            // Generic operation
            rays.push(LogicRay::new(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Self::hash_string(input),
                priority,
                0,  // Generic data
                0,
            ));
        }

        log::info!("Generated {} rays from intent: '{}'", rays.len(), input);
        Ok(rays)
    }

    fn hash_string(s: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }
}

/// Policy Arbiter - determines resource allocation
pub struct PolicyArbiter {
    // Policy state
}

impl PolicyArbiter {
    pub fn new() -> Self {
        Self {}
    }

    /// Make a policy decision based on system load
    pub fn decide(&self, system_load: f32) -> PolicyDecision {
        if system_load > 0.9 {
            PolicyDecision {
                gpu_allocation: 0.95,
                priority_boost: 1.5,
                enable_work_stealing: true,
            }
        } else if system_load > 0.5 {
            PolicyDecision {
                gpu_allocation: 0.7,
                priority_boost: 1.0,
                enable_work_stealing: true,
            }
        } else {
            PolicyDecision {
                gpu_allocation: 0.5,
                priority_boost: 1.0,
                enable_work_stealing: false,
            }
        }
    }
}

/// Policy decision output
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    /// Percentage of GPU to allocate (0.0 - 1.0)
    pub gpu_allocation: f32,
    /// Priority multiplier
    pub priority_boost: f32,
    /// Should enable work stealing across GPUs?
    pub enable_work_stealing: bool,
}
