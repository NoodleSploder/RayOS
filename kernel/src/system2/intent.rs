/// Intent parsing - converts natural language to executable tasks
///
/// This is the "Intent Parser" from the design document.
/// In a production system, this would integrate with a local LLM.

use crate::types::{LogicRay, Priority};
use glam::Vec3;
use anyhow::Result;

/// Task structure from parsed intent
#[derive(Debug, Clone)]
pub struct TaskStruct {
    /// Natural language description
    pub description: String,
    /// Priority level
    pub priority: Priority,
    /// Estimated computational complexity
    pub complexity: f32,
    /// Associated context data
    pub context: Vec<String>,
}

impl TaskStruct {
    /// Convert this task into a ray bundle
    pub fn to_rays(&self, base_task_id: u64) -> Vec<LogicRay> {
        // Decompose task into multiple rays based on complexity
        let ray_count = (self.complexity * 10.0).ceil() as usize;

        (0..ray_count)
            .map(|i| {
                LogicRay::new(
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(
                        (i as f32).cos(),
                        (i as f32).sin(),
                        1.0,
                    )
                    .normalize(),
                    base_task_id + i as u64,
                    self.priority,
                    0,
                    0,
                )
            })
            .collect()
    }
}

/// Intent parsing result
#[derive(Debug)]
pub enum IntentResult {
    /// Single task
    Task(TaskStruct),
    /// Multiple tasks
    TaskBundle(Vec<TaskStruct>),
    /// Ambiguous - needs clarification
    Ambiguous(String),
    /// Not understood
    Unknown,
}

/// Parse intent from natural language
pub fn parse_intent(input: &str) -> Result<IntentResult> {
    let input = input.to_lowercase();

    // Simple keyword-based parsing (placeholder for LLM)
    if input.contains("optimize") {
        Ok(IntentResult::Task(TaskStruct {
            description: "Optimize code".to_string(),
            priority: Priority::High,
            complexity: 5.0,
            context: vec![],
        }))
    } else if input.contains("compile") {
        Ok(IntentResult::Task(TaskStruct {
            description: "Compile project".to_string(),
            priority: Priority::Normal,
            complexity: 3.0,
            context: vec![],
        }))
    } else if input.contains("search") || input.contains("find") {
        Ok(IntentResult::Task(TaskStruct {
            description: "Search operation".to_string(),
            priority: Priority::Normal,
            complexity: 2.0,
            context: vec![],
        }))
    } else {
        Ok(IntentResult::Unknown)
    }
}

/// Context fusion - combines vision and audio
pub struct ContextFusion {
    /// Recent visual context
    visual_history: Vec<(f32, f32)>, // Gaze coordinates
    /// Recent audio context
    audio_history: Vec<String>,
}

impl ContextFusion {
    pub fn new() -> Self {
        Self {
            visual_history: Vec::new(),
            audio_history: Vec::new(),
        }
    }

    /// Add gaze data
    pub fn add_gaze(&mut self, x: f32, y: f32) {
        self.visual_history.push((x, y));

        // Keep only recent history
        if self.visual_history.len() > 100 {
            self.visual_history.remove(0);
        }
    }

    /// Add audio transcript
    pub fn add_audio(&mut self, text: String) {
        self.audio_history.push(text);

        // Keep only recent history
        if self.audio_history.len() > 50 {
            self.audio_history.remove(0);
        }
    }

    /// Fuse contexts and resolve references like "that" or "it"
    pub fn resolve_references(&self, text: &str) -> String {
        // Resolve pronouns using visual and audio history
        let mut resolved = text.to_string();
        let text_lower = text.to_lowercase();

        // Check for deictic references
        if text_lower.contains("that") || text_lower.contains("this") || text_lower.contains("it") {
            // Get most recent visual focus (center of gaze)
            if let Some(&(x, y)) = self.visual_history.last() {
                // Map gaze position to likely UI element
                let region = if x < 0.3 {
                    "left panel"
                } else if x > 0.7 {
                    "right panel"
                } else if y < 0.3 {
                    "top bar"
                } else if y > 0.7 {
                    "bottom panel"
                } else {
                    "center area"
                };

                // Replace pronouns with location-based reference
                resolved = resolved.replace("that", &format!("the item in {}", region));
                resolved = resolved.replace("this", &format!("the item in {}", region));
                resolved = resolved.replace("it", &format!("the item in {}", region));
            }

            // Check recent audio for noun references
            if let Some(last_audio) = self.audio_history.last() {
                // Extract potential noun from previous utterance
                let words: Vec<&str> = last_audio.split_whitespace().collect();
                for (i, word) in words.iter().enumerate() {
                    if i > 0 && ["the", "a", "an"].contains(&words[i-1]) {
                        // Found a likely noun
                        resolved = resolved.replace("it", word);
                        resolved = resolved.replace("that", word);
                        break;
                    }
                }
            }
        }

        resolved
    }
}

impl Default for ContextFusion {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_optimize() {
        let result = parse_intent("optimize this function").unwrap();
        match result {
            IntentResult::Task(task) => {
                assert!(task.description.contains("Optimize"));
                assert_eq!(task.priority, Priority::High);
            }
            _ => panic!("Expected Task variant"),
        }
    }

    #[test]
    fn test_context_fusion() {
        let mut fusion = ContextFusion::new();
        fusion.add_gaze(100.0, 200.0);
        fusion.add_audio("delete that".to_string());

        assert_eq!(fusion.visual_history.len(), 1);
        assert_eq!(fusion.audio_history.len(), 1);
    }
}
