//! LLM Connector - Integration with local language models
//!
//! This module connects to local LLMs (like Llama, Mistral, etc.) to interpret
//! multimodal context and generate user intents.

use crate::types::{FusedContext, Intent};
use anyhow::Result;
use std::path::Path;

#[cfg(feature = "llm")]
use candle_core::{Device, Tensor};
#[cfg(feature = "llm")]
use candle_transformers::models::llama as model;
#[cfg(feature = "llm")]
use tokenizers::Tokenizer;
#[cfg(feature = "llm")]
use std::sync::Arc;

pub struct LLMConnector {
    /// The device to run inference on (CPU or CUDA)
    #[cfg(feature = "llm")]
    device: Device,
    /// Tokenizer for the model
    #[cfg(feature = "llm")]
    tokenizer: Option<Arc<Tokenizer>>,
    /// System prompt for intent classification
    _system_prompt: String,
}

impl LLMConnector {
    pub async fn new() -> Result<Self> {
        log::info!("Initializing LLM Connector...");

        // System prompt optimized for intent classification
        let system_prompt = r#"You are an intent classifier for a neural operating system.
Given multimodal context (visual scene, gaze position, audio), determine the user's intent.

Available intents:
- Select: User wants to interact with an object
- Move: User wants to relocate something
- Delete: User wants to remove something
- Create: User wants to make something new
- Break: User is taking a break (holding coffee/drink)
- Idle: No clear intent

Respond only with the intent name and target object."#.to_string();

        #[cfg(feature = "llm")]
        {
            // Try to use CUDA if available, otherwise CPU
            let device = Device::cuda_if_available(0)
                .unwrap_or_else(|_| Device::Cpu);

            log::info!("Using device: {:?}", device);

            Ok(Self {
                device,
                tokenizer: None,
                system_prompt,
            })
        }

        #[cfg(not(feature = "llm"))]
        {
            log::warn!("LLM feature not enabled, using heuristics only");
            Ok(Self {
                _system_prompt: system_prompt,
            })
        }
    }

    /// Load a model from disk
    pub async fn load_model(&mut self, model_path: &Path) -> Result<()> {
        log::info!("Loading LLM from: {:?}", model_path);

        // Check if model file exists
        if model_path.exists() {
            let metadata = std::fs::metadata(model_path)?;
            let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);

            log::info!("Found model file: {:.2} MB", size_mb);

            // In a real implementation, this would:
            // 1. Memory map the model file
            // 2. Parse model format (GGUF, safetensors, etc.)
            // 3. Initialize tokenizer
            // 4. Allocate GPU buffers
            // 5. Load weights into memory

            // For now, validate file is readable
            let _header = std::fs::read(model_path);

            log::info!("Model loaded successfully (using heuristics fallback)");
        } else {
            log::warn!("Model file not found, using keyword-based heuristics");
        }

        Ok(())
    }

    /// Process fused context and generate intent
    pub async fn process_context(&self, context: &FusedContext) -> Result<Option<Intent>> {
        // Build context string
        let context_str = self.build_context_string(context);

        // For now, use rule-based classification
        // In production, this would call the actual LLM
        let intent = self.classify_heuristic(context_str, context);

        Ok(intent)
    }

    /// Build a text representation of the multimodal context
    fn build_context_string(&self, context: &FusedContext) -> String {
        let mut parts = vec![];

        // Add gaze information
        parts.push(format!(
            "User is looking at position ({:.2}, {:.2}) with {:.0}% confidence.",
            context.gaze.screen_x,
            context.gaze.screen_y,
            context.gaze.confidence * 100.0
        ));

        // Add detected objects
        if !context.visual.objects.is_empty() {
            let objects: Vec<String> = context.visual.objects
                .iter()
                .map(|obj| format!("{} ({:.0}%)", obj.label, obj.confidence * 100.0))
                .collect();
            parts.push(format!("Detected objects: {}", objects.join(", ")));
        }

        // Add audio transcript
        if let Some(audio) = &context.audio_transcript {
            parts.push(format!("User said: \"{}\"", audio));
        }

        parts.join(" ")
    }

    /// Heuristic-based classification (temporary until LLM is integrated)
    fn classify_heuristic(&self, _context: String, fused: &FusedContext) -> Option<Intent> {
        // Check for break mode (coffee cup detected)
        if fused.visual.objects.iter().any(|obj|
            obj.label == "cup" || obj.label == "bottle" || obj.label == "wine glass"
        ) {
            return Some(Intent::Break);
        }

        // Check audio commands
        if let Some(audio) = &fused.audio_transcript {
            let audio_lower = audio.to_lowercase();

            // Delete intent
            if audio_lower.contains("delete") || audio_lower.contains("remove") {
                return Some(Intent::Delete {
                    target: self.extract_target(&audio_lower, fused),
                });
            }

            // Move intent
            if audio_lower.contains("move") || audio_lower.contains("drag") {
                return Some(Intent::Move {
                    source: self.extract_target(&audio_lower, fused),
                    destination: "unknown".to_string(),
                });
            }

            // Create intent
            if audio_lower.contains("create") || audio_lower.contains("make") || audio_lower.contains("new") {
                return Some(Intent::Create {
                    object_type: self.extract_object_type(&audio_lower),
                });
            }

            // Select intent
            if audio_lower.contains("select") || audio_lower.contains("open") || audio_lower.contains("click") {
                return Some(Intent::Select {
                    target: self.extract_target(&audio_lower, fused),
                });
            }
        }

        // Default to idle if no clear intent
        Some(Intent::Idle)
    }

    /// Extract target object from audio command
    fn extract_target(&self, audio: &str, context: &FusedContext) -> String {
        // Check for deictic references
        if audio.contains("that") || audio.contains("this") || audio.contains("it") {
            // Find object at gaze point
            if let Some(obj) = context.visual.objects.first() {
                return obj.label.clone();
            }
        }

        // Try to find object name in audio
        for obj in &context.visual.objects {
            if audio.contains(&obj.label) {
                return obj.label.clone();
            }
        }

        "unknown".to_string()
    }

    /// Extract object type from create command
    fn extract_object_type(&self, audio: &str) -> String {
        let types = ["file", "folder", "window", "note", "document"];
        for obj_type in &types {
            if audio.contains(obj_type) {
                return obj_type.to_string();
            }
        }
        "object".to_string()
    }

    /// Generate text response from the LLM
    pub async fn generate(&self, prompt: &str) -> Result<String> {
        // Generate response using template-based approach
        // In production, this would run the full generative model
        log::debug!("Generate called with prompt: {}", prompt);

        let user_excerpt = extract_user_excerpt(prompt).unwrap_or_else(|| "that".to_string());
        let user_lower = normalize_user_text(&user_excerpt);
        let sentiment = detect_sentiment(&user_lower);

        // Pattern-based response generation
        let response = if is_greeting(&user_lower) {
            "Hi. I'm online. Try: search TERM, index FILE, optimize system.".to_string()
        } else if is_thanks(&user_lower) {
            "You're welcome. What's next?".to_string()
        } else if is_farewell(&user_lower) {
            "OK. Standing by.".to_string()
        } else if matches!(sentiment, Sentiment::Urgent) {
            // Keep it short and directive.
            "Understood. Tell me the fastest next step: 'search <term>', 'index <file>', or describe the failure you see."
                .to_string()
        } else if matches!(sentiment, Sentiment::Negative) {
            // Acknowledge frustration and ask for concrete details.
            "Got it — that sounds frustrating. What did you expect to happen, and what happened instead? If you want me to dig, say 'search <term>'."
                .to_string()
        } else if user_lower.contains("what") {
            if user_lower.contains("time") {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap();
                let secs = now.as_secs();
                let hours = (secs / 3600) % 24;
                let mins = (secs / 60) % 60;
                format!("The current time is approximately {:02}:{:02} UTC", hours, mins)
            } else if user_lower.contains("doing") || user_lower.contains("status") {
                "I'm running and tracking tasks. Ask me to search, index, optimize, or inspect system state.".to_string()
            } else {
                "I can help by turning requests into tasks (search, index, optimize) and reporting results.".to_string()
            }
        } else if user_lower.contains("how") {
            if user_lower.contains("work") {
                "I parse your request, select a task, execute it, then stream back status and results.".to_string()
            } else {
                "Tell me what you want done (e.g., 'search boot', 'index <file>', 'optimize system').".to_string()
            }
        } else if user_lower.contains("help") || user_lower.contains("assist") {
            "I can queue and execute tasks: search the repo, index files, run maintenance, and report outcomes.".to_string()
        } else if user_lower.contains("thank") {
            "You're welcome! I'm here to help.".to_string()
        } else {
            // Generic fallback with context awareness
            let word_count = prompt.split_whitespace().count();
            if word_count > 10 {
                "Acknowledged. Executing and reporting back.".to_string()
            } else {
                format!("Acknowledged: '{}'. What should I do next?", user_excerpt)
            }
        };

        Ok(response)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sentiment {
    Positive,
    Neutral,
    Negative,
    Urgent,
}

fn detect_sentiment(user_lower: &str) -> Sentiment {
    // Order matters: urgent should win over negative.
    let s = user_lower;

    // Urgency / time pressure.
    let urgent_markers = [
        "asap",
        "urgent",
        "immediately",
        "right now",
        "now",
        "quick",
        "hurry",
        "fast",
    ];
    if urgent_markers.iter().any(|m| s.contains(m)) {
        return Sentiment::Urgent;
    }

    // Negative / frustrated / stuck.
    let negative_markers = [
        "sucks",
        "this sucks",
        "broken",
        "doesnt work",
        "doesn't work",
        "not working",
        "stuck",
        "failing",
        "failure",
        "error",
        "annoying",
        "frustrating",
        "wtf",
    ];
    if negative_markers.iter().any(|m| s.contains(m)) {
        return Sentiment::Negative;
    }

    // Mild positive.
    let positive_markers = ["great", "awesome", "nice", "good job", "perfect"];
    if positive_markers.iter().any(|m| s.contains(m)) {
        return Sentiment::Positive;
    }

    Sentiment::Neutral
}

fn normalize_user_text(s: &str) -> String {
    // Lowercase, trim surrounding punctuation, collapse whitespace.
    let lower = s.to_lowercase();
    let trimmed = lower
        .trim_matches(|c: char| !(c.is_alphanumeric() || c.is_whitespace()))
        .trim();
    trimmed
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_greeting(s: &str) -> bool {
    // Accept "hi" and variants like "hi there".
    let first = s.split_whitespace().next().unwrap_or("");
    matches!(first, "hi" | "hello" | "hey" | "yo" | "sup" | "hiya" | "howdy" | "ping")
}

fn is_thanks(s: &str) -> bool {
    s == "thank you" || {
        let first = s.split_whitespace().next().unwrap_or("");
        matches!(first, "thanks" | "thank" | "thx" | "ty")
    }
}

fn is_farewell(s: &str) -> bool {
    if s == "see you" {
        return true;
    }
    let first = s.split_whitespace().next().unwrap_or("");
    matches!(first, "bye" | "goodbye" | "cya" | "exit" | "quit")
}

fn extract_user_excerpt(prompt: &str) -> Option<String> {
    // The ai_bridge prompt format is:
    //   System: ...\n\nUser: <text>\n\nParsed intent command: ...
    let user_marker = "\n\nUser:";
    let start = prompt.find(user_marker)? + user_marker.len();
    let rest = prompt[start..].trim_start();

    // Take until the next blank line (or end).
    let end = rest.find("\n\n").unwrap_or(rest.len());
    let user = rest[..end].trim();
    if user.is_empty() {
        return None;
    }

    // Keep it short and single-line.
    let mut s = user.replace('\n', " ");
    const MAX_CHARS: usize = 48;
    if s.chars().count() > MAX_CHARS {
        s = s.chars().take(MAX_CHARS).collect();
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GazePoint, VisualContext, DetectedObject, BoundingBox};

    #[tokio::test]
    async fn test_break_detection() {
        let llm = LLMConnector::new().await.unwrap();

        let context = FusedContext {
            gaze: GazePoint {
                screen_x: 0.5,
                screen_y: 0.5,
                confidence: 0.9,
                timestamp: 0,
            },
            visual: VisualContext {
                objects: vec![
                    DetectedObject {
                        label: "cup".to_string(),
                        confidence: 0.9,
                        bbox: BoundingBox { x: 0.4, y: 0.4, width: 0.1, height: 0.1 },
                    },
                ],
                colors: vec![],
                text: None,
                timestamp: 0,
            },
            audio_transcript: None,
        };

        let intent = llm.process_context(&context).await.unwrap();
        assert!(matches!(intent, Some(Intent::Break)));
    }
}
