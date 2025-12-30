//! LLM Connector - Optional Neural Language Understanding
//!
//! Implements an optional, feature-gated “LLM mode” parsing pipeline:
//!
//! - Tokenization pipeline
//! - Embeddings generation
//! - Neural-ish intent classification (embedding similarity)
//! - Entity extraction
//!
//! If a heavyweight model/tokenizer is provided, we can attempt to load it in
//! `initialize()`. If it is not available, we still run a deterministic
//! lightweight pipeline (no external weights) once initialized.

use crate::types::*;
#[cfg(feature = "llm")]
use regex::Regex;

#[cfg(feature = "llm")]
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(feature = "llm")]
use candle_core::Device;

#[cfg(feature = "llm")]
use candle_core::{DType, Tensor};

#[cfg(feature = "llm")]
use tokenizers::Tokenizer;

/// LLM connector for neural intent parsing
pub struct LLMConnector {
    #[cfg(feature = "llm")]
    device: Device,

    #[cfg(feature = "llm")]
    tokenizer: Option<Tokenizer>,

    #[cfg(feature = "llm")]
    model_path: Option<PathBuf>,

    // Optional neural classifier weights (Candle matmul + softmax).
    // This is the "actual model integration" path for TODO #10.
    #[cfg(feature = "llm")]
    classifier: Option<NeuralIntentClassifier>,

    #[cfg(feature = "llm")]
    embedding_dim: usize,
    enabled: bool,
}

impl LLMConnector {
    /// Create new LLM connector
    pub fn new(_model_path: Option<PathBuf>) -> Self {
        Self {
            #[cfg(feature = "llm")]
            device: Device::Cpu,

            #[cfg(feature = "llm")]
            tokenizer: None,

            #[cfg(feature = "llm")]
            model_path: _model_path,

            #[cfg(feature = "llm")]
            classifier: None,

            #[cfg(feature = "llm")]
            embedding_dim: EMBEDDING_DIM,
            enabled: false,
        }
    }

    /// Initialize LLM (load model into memory)
    #[cfg(feature = "llm")]
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Detect device (GPU if available, else CPU)
        self.device = if Device::cuda_if_available(0).is_ok() {
            Device::cuda_if_available(0)?
        } else {
            Device::Cpu
        };

        println!("Initializing LLM-mode pipeline on device: {:?}", self.device);

        // Best-effort: load a tokenizer if present.
        // Convention:
        // - If model_path points to a directory: look for tokenizer.json inside.
        // - If model_path points to a file: look for sibling tokenizer.json.
        if let Some(path) = self.model_path.as_ref() {
            if let Some(tok_path) = guess_tokenizer_path(path) {
                match Tokenizer::from_file(&tok_path) {
                    Ok(tok) => {
                        println!("Loaded tokenizer: {}", tok_path.display());
                        self.tokenizer = Some(tok);
                    }
                    Err(e) => {
                        println!(
                            "Tokenizer not loaded ({}): {}. Falling back to basic tokenization.",
                            tok_path.display(),
                            e
                        );
                    }
                }
            }

            // Best-effort: load a small intent-classifier model if present.
            // Convention:
            // - If model_path is a directory: look for intent_classifier.json inside.
            // - If model_path is a file: look for sibling intent_classifier.json.
            if let Some(classifier_path) = guess_intent_classifier_path(path) {
                match NeuralIntentClassifier::load_json(&classifier_path, &self.device) {
                    Ok(classifier) => {
                        println!("Loaded intent classifier: {}", classifier_path.display());
                        self.embedding_dim = classifier.embedding_dim;
                        self.classifier = Some(classifier);
                    }
                    Err(e) => {
                        println!(
                            "Intent classifier not loaded ({}): {}. Falling back to prototype similarity.",
                            classifier_path.display(),
                            e
                        );
                    }
                }
            }
        }

        // We always enable the pipeline once initialized, even if no external
        // model/tokenizer is present.
        self.enabled = true;
        println!("LLM-mode pipeline initialized");
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
        if !self.enabled {
            return None;
        }

        // For the lightweight LLM-mode fallback, classify based on the raw input.
        // Using the full prompt text tends to drown the signal in boilerplate.
        let tokens = self.tokenize(input);
        let embedding = self.embed_tokens(&tokens);
        let (kind, confidence) = self.classify_intent(&embedding);
        let entities = extract_entities(input);
        let command = build_command(kind, input, &entities, context);

        let intent = Intent {
            id: IntentId::new(),
            command,
            context: context.clone(),
            confidence,
            timestamp: std::time::Instant::now(),
        };

        Some(ParseResult {
            intent,
            alternatives: vec![],
            needs_clarification: confidence < 0.7,
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
        if !self.enabled {
            return None;
        }

        let tokens = self.tokenize(text);
        Some(self.embed_tokens(&tokens))
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

    #[cfg(feature = "llm")]
    fn tokenize(&self, text: &str) -> Vec<String> {
        if let Some(tok) = self.tokenizer.as_ref() {
            // Best-effort: use external tokenizer vocabulary if present.
            // Any failures fall back to basic tokenization.
            if let Ok(encoding) = tok.encode(text, true) {
                if let Some(tokens) = encoding.get_tokens().get(0..) {
                    return tokens.iter().map(|t| t.to_string()).collect();
                }
            }
        }

        tokenize_basic(text)
    }

    #[cfg(feature = "llm")]
    fn embed_tokens(&self, tokens: &[String]) -> Vec<f32> {
        embed_hashed(tokens, self.embedding_dim)
    }

    #[cfg(feature = "llm")]
    fn classify_intent(&self, embedding: &[f32]) -> (IntentKind, f32) {
        if let Some(classifier) = self.classifier.as_ref() {
            if let Ok((kind, confidence)) = classifier.predict(embedding) {
                return (kind, confidence);
            }
        }
        classify_intent_similarity(embedding)
    }
}

#[cfg(feature = "llm")]
const EMBEDDING_DIM: usize = 384;

#[cfg(feature = "llm")]
#[derive(Debug, Clone)]
struct NeuralIntentClassifier {
    embedding_dim: usize,
    w: Tensor, // [num_classes, embedding_dim]
    b: Tensor, // [num_classes]
}

#[cfg(feature = "llm")]
#[derive(serde::Deserialize)]
struct IntentClassifierJson {
    embedding_dim: usize,
    // Row-major: weights[class][dim]
    weights: Vec<Vec<f32>>,
    bias: Vec<f32>,
}

#[cfg(feature = "llm")]
impl NeuralIntentClassifier {
    fn labels() -> &'static [IntentKind] {
        &[
            IntentKind::Create,
            IntentKind::Modify,
            IntentKind::Delete,
            IntentKind::Query,
            IntentKind::Navigate,
            IntentKind::Execute,
        ]
    }

    fn load_json(path: &std::path::Path, device: &Device) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let parsed: IntentClassifierJson = serde_json::from_str(&text)?;

        let num_classes = Self::labels().len();
        if parsed.weights.len() != num_classes {
            anyhow::bail!(
                "weights rows mismatch: expected {}, got {}",
                num_classes,
                parsed.weights.len()
            );
        }
        if parsed.bias.len() != num_classes {
            anyhow::bail!(
                "bias len mismatch: expected {}, got {}",
                num_classes,
                parsed.bias.len()
            );
        }
        for (i, row) in parsed.weights.iter().enumerate() {
            if row.len() != parsed.embedding_dim {
                anyhow::bail!(
                    "weights[{}] dim mismatch: expected {}, got {}",
                    i,
                    parsed.embedding_dim,
                    row.len()
                );
            }
        }

        let flat_w: Vec<f32> = parsed.weights.into_iter().flatten().collect();
        let w = Tensor::from_vec(flat_w, (num_classes, parsed.embedding_dim), device)?;
        let b = Tensor::from_vec(parsed.bias, (num_classes,), device)?;

        Ok(Self {
            embedding_dim: parsed.embedding_dim,
            w,
            b,
        })
    }

    #[allow(dead_code)]
    fn deterministic(device: &Device, embedding_dim: usize) -> anyhow::Result<Self> {
        // Deterministic "default weights" derived from per-class hashes.
        // This makes the neural path usable even without shipping weights.
        let num_classes = Self::labels().len();
        let mut flat_w = vec![0.0f32; num_classes * embedding_dim];
        let mut b = vec![0.0f32; num_classes];

        for (ci, kind) in Self::labels().iter().enumerate() {
            let seed = format!("rayos-intent-classifier:{kind:?}");
            let h = blake3::hash(seed.as_bytes());
            let bytes = h.as_bytes();
            // Bias derived from hash so classes don't tie constantly.
            b[ci] = (bytes[0] as f32 / 255.0) - 0.5;

            for j in 0..embedding_dim {
                let byte = bytes[(j + ci) % bytes.len()] as f32;
                let centered = (byte / 255.0) - 0.5;
                flat_w[ci * embedding_dim + j] = centered * 0.05;
            }
        }

        let w = Tensor::from_vec(flat_w, (num_classes, embedding_dim), device)?;
        let b = Tensor::from_vec(b, (num_classes,), device)?;
        Ok(Self {
            embedding_dim,
            w,
            b,
        })
    }

    fn predict(&self, embedding: &[f32]) -> anyhow::Result<(IntentKind, f32)> {
        if embedding.len() != self.embedding_dim {
            anyhow::bail!(
                "embedding dim mismatch: expected {}, got {}",
                self.embedding_dim,
                embedding.len()
            );
        }

        let x = Tensor::from_vec(embedding.to_vec(), (self.embedding_dim,), self.w.device())?
            .to_dtype(DType::F32)?;
        // logits = W*x + b  -> [num_classes]
        let logits = (self.w.matmul(&x.unsqueeze(1)?)?.squeeze(1)? + &self.b)?;
        let probs = candle_nn::ops::softmax(&logits, 0)?;
        let probs_vec = probs.to_vec1::<f32>()?;

        let mut best_i = 0usize;
        let mut best_p = -1.0f32;
        for (i, p) in probs_vec.iter().enumerate() {
            if *p > best_p {
                best_p = *p;
                best_i = i;
            }
        }

        let kind = Self::labels()[best_i];
        Ok((kind, best_p.clamp(0.0, 1.0)))
    }
}

#[cfg(feature = "llm")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntentKind {
    Create,
    Modify,
    Delete,
    Query,
    Navigate,
    Execute,
}

#[cfg(feature = "llm")]
fn tokenize_basic(text: &str) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;

    // Unicode-aware “word” tokenization.
    // This is intentionally simple and deterministic.
    text.unicode_words()
        .map(|w| w.to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

#[cfg(feature = "llm")]
fn embed_hashed(tokens: &[String], dim: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; dim];

    if tokens.is_empty() {
        return v;
    }

    for token in tokens {
        let h = blake3::hash(token.as_bytes());
        let b = h.as_bytes();

        // Feature hashing into a fixed-size vector.
        // This produces meaningful overlap for shared tokens.
        let idx1 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize % dim;
        let idx2 = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize % dim;
        let s1 = if (b[8] & 1) == 0 { 1.0 } else { -1.0 };
        let s2 = if (b[9] & 1) == 0 { 1.0 } else { -1.0 };

        v[idx1] += s1;
        v[idx2] += 0.5 * s2;
    }

    // Normalize.
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }

    v
}

#[cfg(feature = "llm")]
fn cosine_similarity_f32(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
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

#[cfg(feature = "llm")]
fn classify_intent_similarity(embedding: &[f32]) -> (IntentKind, f32) {
    // Prototype phrases per intent kind.
    // We embed these once per call; it’s cheap for this crate and keeps things
    // deterministic without shipping external weights.
    const CREATE: &[&str] = &["create file", "make new", "generate", "add", "new directory"];
    const MODIFY: &[&str] = &["rename", "move", "refactor", "optimize", "edit", "update"];
    const DELETE: &[&str] = &["delete", "remove", "erase", "destroy"];
    const QUERY: &[&str] = &["find", "search", "show me", "list", "where is", "which"];
    const NAVIGATE: &[&str] = &["go to", "open", "switch to", "navigate"];
    const EXECUTE: &[&str] = &["run", "execute", "start", "launch"];

    let prototypes: &[(IntentKind, &[&str])] = &[
        (IntentKind::Create, CREATE),
        (IntentKind::Modify, MODIFY),
        (IntentKind::Delete, DELETE),
        (IntentKind::Query, QUERY),
        (IntentKind::Navigate, NAVIGATE),
        (IntentKind::Execute, EXECUTE),
    ];

    let mut scored: Vec<(IntentKind, f32)> = Vec::with_capacity(prototypes.len());
    for (kind, phrases) in prototypes {
        let centroid = centroid_embedding(phrases);
        let sim = cosine_similarity_f32(embedding, &centroid);
        scored.push((*kind, sim));
    }

    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (best_kind, best) = scored[0];
    let second = scored.get(1).map(|x| x.1).unwrap_or(-1.0);

    // Confidence is a squashed margin between top-2.
    let margin = best - second;
    let confidence = (0.5 + margin).clamp(0.0, 1.0);
    (best_kind, confidence)
}

#[cfg(feature = "llm")]
fn centroid_embedding(phrases: &[&str]) -> Vec<f32> {
    let mut acc = vec![0.0f32; EMBEDDING_DIM];
    let mut count = 0usize;
    for p in phrases {
        let tokens = tokenize_basic(p);
        let emb = embed_hashed(&tokens, EMBEDDING_DIM);
        for (a, e) in acc.iter_mut().zip(emb.iter()) {
            *a += *e;
        }
        count += 1;
    }
    if count == 0 {
        return acc;
    }
    for a in &mut acc {
        *a /= count as f32;
    }
    let norm: f32 = acc.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut acc {
            *x /= norm;
        }
    }
    acc
}

#[cfg(feature = "llm")]
#[derive(Debug, Default, Clone)]
struct Entities {
    quoted: Vec<String>,
    filenames: Vec<String>,
    paths: Vec<String>,
    kv: HashMap<String, String>,
    numbers: Vec<f64>,
}

#[cfg(feature = "llm")]
fn extract_entities(input: &str) -> Entities {
    let mut out = Entities::default();

    let quoted_re = Regex::new(r#"\"([^\"]+)\"|'([^']+)'"#).unwrap();
    for caps in quoted_re.captures_iter(input) {
        if let Some(m) = caps.get(1).or_else(|| caps.get(2)) {
            out.quoted.push(m.as_str().to_string());
        }
    }

    let filename_re = Regex::new(r"(?i)\b[\w\-.]+\.[a-z0-9]{1,6}\b").unwrap();
    for m in filename_re.find_iter(input) {
        out.filenames.push(m.as_str().to_string());
    }

    let path_re = Regex::new(r"(?i)\b(?:\./|\../|/)[^\s]+\b").unwrap();
    for m in path_re.find_iter(input) {
        out.paths.push(m.as_str().to_string());
    }

    let kv_re = Regex::new(r"(?i)\b([a-z_][a-z0-9_\-]*)=([^\s]+)\b").unwrap();
    for caps in kv_re.captures_iter(input) {
        if let (Some(k), Some(v)) = (caps.get(1), caps.get(2)) {
            out.kv.insert(k.as_str().to_string(), v.as_str().to_string());
        }
    }

    let num_re = Regex::new(r"(?i)(?:^|\s)(-?\d+(?:\.\d+)?)(?:$|\s)").unwrap();
    for caps in num_re.captures_iter(input) {
        if let Some(m) = caps.get(1) {
            if let Ok(n) = m.as_str().parse::<f64>() {
                out.numbers.push(n);
            }
        }
    }

    out
}

#[cfg(feature = "llm")]
fn build_command(kind: IntentKind, input: &str, entities: &Entities, context: &Context) -> Command {
    let input_lower = input.to_lowercase();

    match kind {
        IntentKind::Create => {
            let mut properties = HashMap::new();
            properties.insert("source".to_string(), input.to_string());

            // Carry through any k=v pairs from entity extraction.
            for (k, v) in entities.kv.iter() {
                properties.insert(k.clone(), v.clone());
            }

            let name = entities
                .quoted
                .first()
                .cloned()
                .or_else(|| entities.filenames.first().cloned());
            if let Some(name) = name {
                properties.insert("name".to_string(), name);
            }

            let object_type = if input_lower.contains("dir") || input_lower.contains("folder") {
                "directory"
            } else if input_lower.contains("file") {
                "file"
            } else {
                "item"
            };

            Command::Create {
                object_type: object_type.to_string(),
                properties,
            }
        }
        IntentKind::Modify => {
            let target = extract_target(&input_lower, entities, context);
            let operation = if input_lower.contains("rename") {
                if let Some(new_name) = entities.quoted.first().cloned().or_else(|| entities.filenames.first().cloned()) {
                    Operation::Rename { new_name }
                } else {
                    Operation::Custom {
                        operation: "rename".to_string(),
                        params: HashMap::new(),
                    }
                }
            } else if input_lower.contains("move") {
                let destination = entities
                    .paths
                    .first()
                    .cloned()
                    .or_else(|| entities.quoted.first().cloned())
                    .unwrap_or_else(|| "./".to_string());
                Operation::Move {
                    destination: PathBuf::from(destination),
                }
            } else if input_lower.contains("optimize") {
                Operation::Optimize
            } else if input_lower.contains("refactor") {
                Operation::Refactor
            } else {
                Operation::Custom {
                    operation: "edit".to_string(),
                    params: HashMap::new(),
                }
            };

            Command::Modify { target, operation }
        }
        IntentKind::Delete => {
            let target = extract_target(&input_lower, entities, context);
            Command::Delete { target }
        }
        IntentKind::Query => Command::Query {
            query: input.to_string(),
            filters: vec![],
        },
        IntentKind::Navigate => {
            let destination = entities
                .paths
                .first()
                .cloned()
                .or_else(|| entities.quoted.first().cloned())
                .unwrap_or_else(|| input.to_string());
            Command::Navigate { destination }
        }
        IntentKind::Execute => {
            // Split “run X Y Z” into action + args.
            let mut parts = input.split_whitespace();
            let first = parts.next().unwrap_or("");
            let mut rest: Vec<String> = parts.map(|s| s.to_string()).collect();

            let (action, args) = if matches!(first.to_lowercase().as_str(), "run" | "execute" | "start" | "launch") {
                let action = rest.first().cloned().unwrap_or_else(|| "".to_string());
                if !rest.is_empty() {
                    rest.remove(0);
                }
                (action, rest)
            } else {
                (first.to_string(), rest)
            };

            Command::Execute { action, args }
        }
    }
}

#[cfg(feature = "llm")]
fn guess_intent_classifier_path(model_path: &std::path::Path) -> Option<PathBuf> {
    if model_path.is_dir() {
        let p = model_path.join("intent_classifier.json");
        if p.exists() {
            return Some(p);
        }
        return None;
    }

    let parent = model_path.parent()?;
    let p = parent.join("intent_classifier.json");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

#[cfg(feature = "llm")]
fn extract_target(input_lower: &str, entities: &Entities, context: &Context) -> Target {
    if input_lower.contains("that") || input_lower.contains("this") || input_lower.contains("it") {
        return Target::Deictic {
            gaze_position: context.gaze.as_ref().map(|g| g.position),
            object_id: context.gaze.as_ref().and_then(|g| g.focused_object.clone()),
        };
    }

    if let Some(p) = entities.paths.first().cloned() {
        return Target::Direct { path: PathBuf::from(p) };
    }
    if let Some(f) = entities.filenames.first().cloned() {
        return Target::Direct { path: PathBuf::from(f) };
    }
    if let Some(q) = entities.quoted.first().cloned() {
        return Target::Named { name: q };
    }

    Target::Named {
        name: "unknown".to_string(),
    }
}

#[cfg(feature = "llm")]
fn guess_tokenizer_path(model_path: &std::path::Path) -> Option<PathBuf> {
    if model_path.is_dir() {
        let p = model_path.join("tokenizer.json");
        if p.exists() {
            return Some(p);
        }
        return None;
    }

    let parent = model_path.parent()?;
    let p = parent.join("tokenizer.json");
    if p.exists() {
        Some(p)
    } else {
        None
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

    #[cfg(feature = "llm")]
    #[test]
    fn test_llm_pipeline_fallback_parse_create() {
        let mut connector = LLMConnector::new(None);
        connector.initialize().expect("initialize");

        let ctx = Context {
            gaze: None,
            audio: None,
            visual_objects: vec![],
            application: None,
            filesystem: None,
            system: SystemContext {
                cpu_usage: 10.0,
                memory_usage: 10.0,
                active_tasks: 0,
            },
        };

        let res = connector
            .parse("create file named \"hello.rs\"", &ctx)
            .expect("parse");

        match res.intent.command {
            Command::Create { object_type, properties } => {
                assert!(object_type.contains("file"));
                assert_eq!(properties.get("name").map(|s| s.as_str()), Some("hello.rs"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_llm_pipeline_fallback_parse_delete_deictic() {
        let mut connector = LLMConnector::new(None);
        connector.initialize().expect("initialize");

        let ctx = Context {
            gaze: Some(GazeContext {
                position: (100.0, 200.0),
                focused_object: Some("test.rs".to_string()),
                timestamp: std::time::Instant::now(),
            }),
            audio: None,
            visual_objects: vec![],
            application: None,
            filesystem: None,
            system: SystemContext {
                cpu_usage: 10.0,
                memory_usage: 10.0,
                active_tasks: 0,
            },
        };

        let res = connector.parse("delete that", &ctx).expect("parse");
        match res.intent.command {
            Command::Delete { target } => {
                assert!(matches!(target, Target::Deictic { .. }));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_llm_pipeline_optional_classifier_json_is_loaded() {
        use std::io::Write;

        let dir = tempfile::tempdir().expect("tempdir");
        let classifier_path = dir.path().join("intent_classifier.json");

        // Force a predictable outcome via biases: always pick Delete.
        // Keep weights all zeros so embedding content doesn't matter.
        let embedding_dim = EMBEDDING_DIM;
        let num_classes = NeuralIntentClassifier::labels().len();
        let weights: Vec<Vec<f32>> = (0..num_classes)
            .map(|_| vec![0.0f32; embedding_dim])
            .collect();
        let mut bias = vec![0.0f32; num_classes];
        // Delete is index 2 in labels().
        bias[2] = 10.0;

        let json = serde_json::json!({
            "embedding_dim": embedding_dim,
            "weights": weights,
            "bias": bias
        });

        let mut f = std::fs::File::create(&classifier_path).expect("create classifier json");
        writeln!(f, "{}", json.to_string()).expect("write classifier json");

        let mut connector = LLMConnector::new(Some(dir.path().to_path_buf()));
        connector.initialize().expect("initialize");
        assert!(connector.classifier.is_some());

        let ctx = Context {
            gaze: None,
            audio: None,
            visual_objects: vec![],
            application: None,
            filesystem: None,
            system: SystemContext {
                cpu_usage: 1.0,
                memory_usage: 1.0,
                active_tasks: 0,
            },
        };

        let res = connector.parse("create file named hello.rs", &ctx).expect("parse");
        assert!(matches!(res.intent.command, Command::Delete { .. }));
    }
}
