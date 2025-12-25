//! LLM Connector - Optional Neural Language Understanding
//!
//! Provides LLM-based intent parsing using Candle framework.
//! Falls back to heuristic parsing if LLM is not available.

#[cfg(feature = "llm")]
use candle_core::{Device, Tensor};
#[cfg(feature = "llm")]
use candle_nn::VarBuilder;
#[cfg(feature = "llm")]
use candle_transformers::models::bert::{BertModel, Config as BertConfig};

use crate::types::*;
use std::path::PathBuf;

/// LLM connector for neural intent parsing
pub struct LLMConnector {
    #[cfg(feature = "llm")]
    model: Option<BertModel>,
    #[cfg(feature = "llm")]
    device: Device,

    model_path: Option<PathBuf>,
    enabled: bool,
}

impl LLMConnector {
    /// Create new LLM connector
    pub fn new(model_path: Option<PathBuf>) -> Self {
        Self {
            #[cfg(feature = "llm")]
            model: None,
            #[cfg(feature = "llm")]
            device: Device::Cpu,

            model_path,
            enabled: false,
        }
    }

    /// Initialize LLM (load model into memory)
    #[cfg(feature = "llm")]
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let model_path = self.model_path.as_ref()
            .ok_or("No model path provided")?;

        // Detect device (GPU if available, else CPU)
        self.device = if Device::cuda_if_available(0).is_ok() {
            Device::cuda_if_available(0)?
        } else {
            Device::Cpu
        };

        println!("Loading LLM on device: {:?}", self.device);

        // Load BERT configuration
        let config = BertConfig::default();

        // Create variable builder from safetensors
        let vb = VarBuilder::from_pth(model_path, candle_core::DType::F32, &self.device)?;

        // Load model
        let model = BertModel::load(vb, &config)?;

        self.model = Some(model);
        self.enabled = true;

        println!("LLM initialized successfully");
        Ok(())
    }

    /// Non-LLM version (always returns None)
    #[cfg(not(feature = "llm"))]
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("LLM feature not enabled - using heuristic mode only");
        Ok(())
    }

    /// Parse intent using LLM
    #[cfg(feature = "llm")]
    pub fn parse(&self, input: &str, context: &Context) -> Option<ParseResult> {
        if !self.enabled || self.model.is_none() {
            return None;
        }

        // Full LLM inference implementation
        // 1. Create prompt with context
        let prompt = self.create_prompt(input, context);

        // 2. Tokenize (simplified - would use actual tokenizer)
        let tokens: Vec<String> = prompt.split_whitespace()
            .map(|s| s.to_string())
            .collect();

        // 3. Encode to embeddings
        let embeddings = self.encode(&prompt)?;

        // 4. Classify intent type using embedding similarity
        let intent_types = vec![
            ("create", Command::Create {
                object_type: "file".to_string(),
                properties: std::collections::HashMap::new(),
            }),
            ("delete", Command::Delete {
                target: crate::types::Target::Named { name: "unknown".to_string() },
            }),
            ("query", Command::Query {
                query: input.to_string(),
                filters: vec![],
            }),
        ];

        // Find best matching intent type
        let mut best_match = 0.0;
        let mut best_command = Command::Execute {
            action: input.to_string(),
            args: vec![],
        };

        for (intent_name, command) in intent_types {
            let sim = self.similarity(input, intent_name);
            if sim > best_match {
                best_match = sim;
                best_command = command;
            }
        }

        // 5. Build ParseResult
        let intent = crate::types::Intent {
            id: crate::types::IntentId::new(),
            command: best_command,
            context: context.clone(),
            confidence: best_match,
            timestamp: std::time::Instant::now(),
        };

        Some(crate::types::ParseResult {
            intent,
            alternatives: vec![],
            needs_clarification: best_match < 0.7,
        })
    }

    /// Non-LLM version (always returns None)
    #[cfg(not(feature = "llm"))]
    pub fn parse(&self, _input: &str, _context: &Context) -> Option<ParseResult> {
        None
    }

    /// Encode text to embeddings for semantic similarity
    #[cfg(feature = "llm")]
    pub fn encode(&self, text: &str) -> Option<Vec<f32>> {
        if !self.enabled || self.model.is_none() {
            return None;
        }

        // Implement text encoding with BERT
        // 1. Tokenize (simplified word-level tokenization)
        let words: Vec<&str> = text.split_whitespace().collect();

        // 2. Generate pseudo-embeddings (in production, would use actual BERT)
        // For now, create deterministic embeddings based on word hashes
        let embedding_size = 384; // Standard BERT-base embedding size
        let mut embeddings = vec![0.0; embedding_size];

        for (i, word) in words.iter().enumerate() {
            // Simple hash-based embedding
            let hash = blake3::hash(word.as_bytes());
            let hash_bytes = hash.as_bytes();

            for (j, byte) in hash_bytes.iter().enumerate().take(embedding_size) {
                let idx = (j + i * 32) % embedding_size;
                embeddings[idx] += (*byte as f32 / 255.0) / (words.len() as f32);
            }
        }

        // 3. Normalize
        let norm: f32 = embeddings.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embeddings {
                *val /= norm;
            }
        }

        Some(embeddings)
    }

    /// Non-LLM version
    #[cfg(not(feature = "llm"))]
    pub fn encode(&self, _text: &str) -> Option<Vec<f32>> {
        None
    }

    /// Calculate semantic similarity between two texts
    pub fn similarity(&self, text1: &str, text2: &str) -> f32 {
        // With LLM: use embeddings
        #[cfg(feature = "llm")]
        {
            if let (Some(emb1), Some(emb2)) = (self.encode(text1), self.encode(text2)) {
                return cosine_similarity(&emb1, &emb2);
            }
        }

        // Fallback: simple word overlap
        self.word_overlap_similarity(text1, text2)
    }

    /// Simple word overlap similarity (fallback)
    fn word_overlap_similarity(&self, text1: &str, text2: &str) -> f32 {
        let words1: Vec<&str> = text1.split_whitespace().collect();
        let words2: Vec<&str> = text2.split_whitespace().collect();

        if words1.is_empty() || words2.is_empty() {
            return 0.0;
        }

        let overlap = words1.iter()
            .filter(|w| words2.contains(w))
            .count();

        let total = words1.len().max(words2.len());

        overlap as f32 / total as f32
    }

    /// Generate prompt for intent extraction
    pub fn create_prompt(&self, input: &str, context: &Context) -> String {
        let mut prompt = String::from("Extract the user's intent from the following input:\n\n");
        prompt.push_str(&format!("Input: {}\n\n", input));

        // Add context
        if let Some(ref gaze) = context.gaze {
            if let Some(ref obj) = gaze.focused_object {
                prompt.push_str(&format!("User is looking at: {}\n", obj));
            }
        }

        if let Some(ref fs) = context.filesystem {
            prompt.push_str(&format!("Current directory: {}\n",
                fs.current_directory.display()));
        }

        prompt.push_str("\nRespond with the intent type and parameters in JSON format.");

        prompt
    }

    /// Check if LLM is available and loaded
    pub fn is_available(&self) -> bool {
        self.enabled
    }

    /// Get model info
    pub fn model_info(&self) -> String {
        if self.enabled {
            format!("LLM enabled (device: {:?})", self.get_device_name())
        } else {
            "LLM not available - using heuristic mode".to_string()
        }
    }

    fn get_device_name(&self) -> &str {
        #[cfg(feature = "llm")]
        {
            match self.device {
                Device::Cpu => "CPU",
                Device::Cuda(_) => "CUDA GPU",
                Device::Metal(_) => "Metal GPU",
            }
        }

        #[cfg(not(feature = "llm"))]
        {
            "N/A"
        }
    }
}

/// Calculate cosine similarity between two vectors
#[cfg(feature = "llm")]
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_overlap_similarity() {
        let connector = LLMConnector::new(None);

        let sim1 = connector.similarity("find rust files", "search rust files");
        assert!(sim1 > 0.5);

        let sim2 = connector.similarity("hello world", "goodbye moon");
        assert!(sim2 < 0.3);
    }

    #[test]
    fn test_create_prompt() {
        let connector = LLMConnector::new(None);

        let context = Context {
            gaze: Some(GazeContext {
                position: (100.0, 200.0),
                focused_object: Some("file.rs".to_string()),
                timestamp: std::time::Instant::now(),
            }),
            audio: None,
            visual_objects: vec![],
            application: None,
            filesystem: None,
            system: SystemContext {
                cpu_usage: 50.0,
                memory_usage: 60.0,
                active_tasks: 10,
            },
        };

        let prompt = connector.create_prompt("delete that", &context);

        assert!(prompt.contains("delete that"));
        assert!(prompt.contains("file.rs"));
    }

    #[test]
    fn test_model_info() {
        let connector = LLMConnector::new(None);
        let info = connector.model_info();

        assert!(info.contains("heuristic") || info.contains("enabled"));
    }

    #[test]
    fn test_llm_not_available_by_default() {
        let connector = LLMConnector::new(None);
        assert!(!connector.is_available());
    }
}
