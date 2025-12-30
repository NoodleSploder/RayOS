/// System 2: The Cognitive Engine - "The Conscious Layer"
///
/// Handles intent analysis, complex decision making, and policy setting.
/// This is where the LLM integration lives.

pub mod intent;
pub mod policy;

pub mod stat_intent;

#[cfg(feature = "candle-infer")]
pub mod candle_intent;

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

    stat_model: stat_intent::StatIntentModel,

    #[cfg(feature = "candle-infer")]
    model: Option<candle_intent::CandleIntentModel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentClass {
    Open,
    Close,
    Search,
    Create,
    Generic,
}

impl IntentClass {
    fn index(self) -> usize {
        match self {
            Self::Open => 0,
            Self::Close => 1,
            Self::Search => 2,
            Self::Create => 3,
            Self::Generic => 4,
        }
    }

    fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Open,
            1 => Self::Close,
            2 => Self::Search,
            3 => Self::Create,
            _ => Self::Generic,
        }
    }
}


impl IntentParser {
    pub fn new() -> Self {
        let stat_model = stat_intent::StatIntentModel::new();

        #[cfg(feature = "candle-infer")]
        {
            let model = candle_intent::CandleIntentModel::new().ok();
            return Self {
                initialized: model.is_some(),
                stat_model,
                model,
            };
        }

        #[cfg(not(feature = "candle-infer"))]
        {
            Self {
                initialized: true,
                stat_model,
            }
        }
    }

    /// Parse text into task rays
    pub async fn parse(&self, input: &str) -> Result<Vec<LogicRay>> {
        log::debug!("Parsing intent: '{}'", input);

        let input_lower = input.to_lowercase();
        let priority = if input_lower.contains("urgent") || input_lower.contains("now") {
            Priority::High
        } else if input_lower.contains("later") || input_lower.contains("eventually") {
            Priority::Low
        } else {
            Priority::Normal
        };

        let (class, inference_path): (IntentClass, &'static str) = {
            #[cfg(feature = "candle-infer")]
            {
                if let Some(model) = &self.model {
                    if let Ok(class) = model.classify(input) {
                        (class, "candle")
                    } else {
                        (self.stat_model.classify(input), "stat")
                    }
                } else {
                    (self.stat_model.classify(input), "stat")
                }
            }

            #[cfg(not(feature = "candle-infer"))]
            {
                (self.stat_model.classify(input), "stat")
            }
        };

        let mut rays = Vec::new();
        match class {
            IntentClass::Open => {
                rays.push(LogicRay::new(
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Self::hash_string(input),
                    priority,
                    1,
                    0,
                ));
            }
            IntentClass::Close => {
                rays.push(LogicRay::new(
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(-1.0, 0.0, 0.0),
                    Self::hash_string(input),
                    priority,
                    2,
                    0,
                ));
            }
            IntentClass::Search => {
                for i in 0..3 {
                    rays.push(LogicRay::new(
                        Vec3::new(0.0, 0.0, 0.0),
                        Vec3::new(0.0, 1.0, 0.0).normalize(),
                        Self::hash_string(&format!("{}-{}", input, i)),
                        priority,
                        3,
                        i as u32,
                    ));
                }
            }
            IntentClass::Create => {
                rays.push(LogicRay::new(
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    Self::hash_string(input),
                    priority,
                    4,
                    0,
                ));
            }
            IntentClass::Generic => {
                rays.push(LogicRay::new(
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Self::hash_string(input),
                    priority,
                    0,
                    0,
                ));
            }
        }

        log::info!(
            "Generated {} rays from intent (class={:?}, inference={}, input='{}')",
            rays.len(),
            class,
            inference_path,
            input
        );

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

#[cfg(all(test, feature = "candle-infer"))]
mod candle_tests {
    use super::*;

    #[tokio::test]
    async fn candle_intent_open_classifies() {
        let parser = IntentParser::new();
        let rays = parser.parse("open the settings").await.unwrap();
        assert!(!rays.is_empty());
        assert_eq!(rays[0].data_ptr, 1);
        assert_eq!(rays[0].direction.x, 1.0);
    }

    #[tokio::test]
    async fn candle_intent_search_classifies() {
        let parser = IntentParser::new();
        let rays = parser.parse("find the logs").await.unwrap();
        assert_eq!(rays.len(), 3);
        assert_eq!(rays[0].data_ptr, 3);
        assert_eq!(rays[0].direction.y, 1.0);
    }
}

#[cfg(test)]
mod stat_intent_tests {
    use super::*;

    #[test]
    fn stat_model_open_classifies() {
        let model = stat_intent::StatIntentModel::new();
        assert_eq!(model.classify("please open settings"), IntentClass::Open);
    }

    #[test]
    fn stat_model_search_classifies() {
        let model = stat_intent::StatIntentModel::new();
        assert_eq!(model.classify("look up the logs"), IntentClass::Search);
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
