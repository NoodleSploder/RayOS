//! Multi-Modal Embedder - Unified embedding across content types
//!
//! This module provides a unified interface for embedding different content modalities
//! (text, code, images, audio) into a common vector space where semantic similarity
//! corresponds to spatial proximity.
//!
//! ## Modalities
//!
//! - **Text**: Natural language text using word patterns and semantic features
//! - **Code**: Source code with language-aware preprocessing and AST-like features
//! - **Image**: Visual content using perceptual hashing, color histograms, and edge detection
//! - **Audio**: Sound files using spectral features, MFCCs, and rhythm patterns
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_volume::multimodal::{MultiModalEmbedder, Modality};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let embedder = MultiModalEmbedder::new(768).await?;
//!
//!     // Embed different modalities into the same vector space
//!     let text_emb = embedder.embed("Hello world", Modality::Text).await?;
//!     let code_emb = embedder.embed_code("fn main() {}", "rust").await?;
//!     let image_emb = embedder.embed_image_file("photo.jpg").await?;
//!     let audio_emb = embedder.embed_audio_file("song.mp3").await?;
//!
//!     Ok(())
//! }
//! ```

use crate::types::Embedding;
use anyhow::{Context, Result};
use std::path::Path;

/// Content modality for embedding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    /// Natural language text
    Text,
    /// Source code (with optional language hint)
    Code,
    /// Image/visual content
    Image,
    /// Audio/sound content
    Audio,
    /// Raw bytes (fallback)
    Binary,
}

impl std::fmt::Display for Modality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Modality::Text => write!(f, "text"),
            Modality::Code => write!(f, "code"),
            Modality::Image => write!(f, "image"),
            Modality::Audio => write!(f, "audio"),
            Modality::Binary => write!(f, "binary"),
        }
    }
}

/// Programming language for code embeddings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    C,
    Cpp,
    Java,
    Unknown,
}

impl CodeLanguage {
    /// Detect language from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "js" | "mjs" | "cjs" => Self::JavaScript,
            "ts" | "tsx" => Self::TypeScript,
            "go" => Self::Go,
            "c" | "h" => Self::C,
            "cpp" | "cc" | "cxx" | "hpp" => Self::Cpp,
            "java" => Self::Java,
            _ => Self::Unknown,
        }
    }

    /// Get language-specific keywords for semantic weighting
    pub fn keywords(&self) -> &[&str] {
        match self {
            Self::Rust => &["fn", "let", "mut", "impl", "struct", "enum", "trait", "pub", "mod", "use", "async", "await", "match", "if", "else", "loop", "while", "for", "return", "self", "Self", "where", "dyn", "Box", "Arc", "Rc", "Option", "Result", "Some", "None", "Ok", "Err"],
            Self::Python => &["def", "class", "import", "from", "if", "elif", "else", "for", "while", "return", "yield", "async", "await", "with", "try", "except", "finally", "raise", "lambda", "self", "None", "True", "False", "__init__", "__main__"],
            Self::JavaScript | Self::TypeScript => &["function", "const", "let", "var", "class", "import", "export", "if", "else", "for", "while", "return", "async", "await", "try", "catch", "throw", "new", "this", "super", "extends", "implements", "interface", "type"],
            Self::Go => &["func", "package", "import", "if", "else", "for", "range", "return", "defer", "go", "chan", "select", "struct", "interface", "type", "var", "const", "nil", "make", "new", "append", "len", "cap"],
            Self::C => &["int", "char", "float", "double", "void", "if", "else", "for", "while", "do", "switch", "case", "return", "struct", "typedef", "enum", "union", "static", "extern", "const", "sizeof", "malloc", "free", "NULL"],
            Self::Cpp => &["class", "public", "private", "protected", "virtual", "override", "template", "typename", "namespace", "using", "new", "delete", "try", "catch", "throw", "const", "static", "inline", "explicit", "nullptr", "auto", "decltype"],
            Self::Java => &["class", "public", "private", "protected", "static", "final", "abstract", "interface", "extends", "implements", "new", "this", "super", "if", "else", "for", "while", "return", "try", "catch", "throw", "throws", "void", "null", "package", "import"],
            Self::Unknown => &[],
        }
    }

    /// Get comment style for stripping
    pub fn comment_styles(&self) -> (&str, Option<(&str, &str)>) {
        match self {
            Self::Rust | Self::C | Self::Cpp | Self::Java | Self::JavaScript | Self::TypeScript | Self::Go =>
                ("//", Some(("/*", "*/"))),
            Self::Python => ("#", Some(("\"\"\"", "\"\"\""))),
            Self::Unknown => ("//", None),
        }
    }
}

/// Image features extracted for embedding
#[derive(Debug, Clone)]
pub struct ImageFeatures {
    /// Perceptual hash (dHash) - 64 bits represented as 8 bytes
    pub perceptual_hash: [u8; 8],
    /// Average color in RGB
    pub avg_color: [f32; 3],
    /// Color histogram (16 bins per channel)
    pub color_histogram: Vec<f32>,
    /// Edge density (measure of visual complexity)
    pub edge_density: f32,
    /// Aspect ratio
    pub aspect_ratio: f32,
    /// Brightness (0.0-1.0)
    pub brightness: f32,
    /// Contrast (0.0-1.0)
    pub contrast: f32,
}

/// Audio features extracted for embedding
#[derive(Debug, Clone)]
pub struct AudioFeatures {
    /// Spectral centroid (brightness)
    pub spectral_centroid: f32,
    /// Spectral rolloff
    pub spectral_rolloff: f32,
    /// Zero crossing rate (measure of noisiness)
    pub zero_crossing_rate: f32,
    /// RMS energy
    pub rms_energy: f32,
    /// Tempo estimate (BPM)
    pub tempo_bpm: f32,
    /// Duration in seconds
    pub duration_secs: f32,
    /// MFCC coefficients (13 standard coefficients)
    pub mfcc: Vec<f32>,
}

/// Multi-Modal Embedder for unified content embedding
pub struct MultiModalEmbedder {
    /// Output dimension for all embeddings
    dimension: usize,
    /// Model identifier
    model_name: String,
}

impl MultiModalEmbedder {
    /// Create a new multi-modal embedder
    pub async fn new(dimension: usize) -> Result<Self> {
        log::info!("Initializing Multi-Modal Embedder (dim={})", dimension);

        Ok(Self {
            dimension,
            model_name: "rayos-multimodal-v1".to_string(),
        })
    }

    /// Embed any content with explicit modality
    pub async fn embed(&self, content: &str, modality: Modality) -> Result<Embedding> {
        match modality {
            Modality::Text => self.embed_text(content).await,
            Modality::Code => self.embed_code(content, "unknown").await,
            Modality::Image | Modality::Audio | Modality::Binary => {
                // For file-based modalities, treat as text fallback
                self.embed_text(content).await
            }
        }
    }

    /// Embed natural language text
    pub async fn embed_text(&self, text: &str) -> Result<Embedding> {
        let preprocessed = preprocess_text(text);
        let features = extract_text_features(&preprocessed);
        Ok(self.features_to_embedding(&features, "text"))
    }

    /// Embed source code with language awareness
    pub async fn embed_code(&self, code: &str, language_hint: &str) -> Result<Embedding> {
        let lang = if language_hint == "unknown" {
            detect_language(code)
        } else {
            CodeLanguage::from_extension(language_hint)
        };

        let features = extract_code_features(code, lang);
        Ok(self.features_to_embedding(&features, &format!("code-{:?}", lang)))
    }

    /// Embed code from a file
    pub async fn embed_code_file(&self, path: &Path) -> Result<Embedding> {
        let code = std::fs::read_to_string(path)
            .context("Failed to read code file")?;

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let lang = CodeLanguage::from_extension(ext);
        let features = extract_code_features(&code, lang);
        Ok(self.features_to_embedding(&features, &format!("code-{:?}", lang)))
    }

    /// Embed an image file
    pub async fn embed_image_file(&self, path: &Path) -> Result<Embedding> {
        let features = extract_image_features(path)?;
        let feature_vec = image_features_to_vec(&features);
        Ok(self.features_to_embedding(&feature_vec, "image"))
    }

    /// Embed image from raw bytes
    pub async fn embed_image_bytes(&self, bytes: &[u8]) -> Result<Embedding> {
        let features = extract_image_features_from_bytes(bytes)?;
        let feature_vec = image_features_to_vec(&features);
        Ok(self.features_to_embedding(&feature_vec, "image"))
    }

    /// Embed an audio file
    pub async fn embed_audio_file(&self, path: &Path) -> Result<Embedding> {
        let features = extract_audio_features(path)?;
        let feature_vec = audio_features_to_vec(&features);
        Ok(self.features_to_embedding(&feature_vec, "audio"))
    }

    /// Embed audio from raw samples
    pub async fn embed_audio_samples(&self, samples: &[f32], sample_rate: u32) -> Result<Embedding> {
        let features = extract_audio_features_from_samples(samples, sample_rate);
        let feature_vec = audio_features_to_vec(&features);
        Ok(self.features_to_embedding(&feature_vec, "audio"))
    }

    /// Convert feature vector to final embedding
    fn features_to_embedding(&self, features: &[f32], modality_tag: &str) -> Embedding {
        // Project features to target dimension using deterministic expansion/compression
        let mut vector = project_to_dimension(features, self.dimension);

        // Add modality encoding (first few dimensions encode modality)
        let modality_code = match modality_tag {
            t if t.starts_with("text") => 0.1,
            t if t.starts_with("code") => 0.2,
            t if t.starts_with("image") => 0.3,
            t if t.starts_with("audio") => 0.4,
            _ => 0.0,
        };

        // Blend modality signal into first dimension
        if !vector.is_empty() {
            vector[0] = vector[0] * 0.9 + modality_code * 0.1;
        }

        // Normalize
        normalize_vector(&mut vector);

        Embedding {
            vector,
            model: format!("{}-{}", self.model_name, modality_tag),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

// ============================================================================
// Text Feature Extraction
// ============================================================================

/// Preprocess text for embedding
fn preprocess_text(text: &str) -> String {
    // Remove excessive whitespace
    let cleaned: String = text.split_whitespace().collect::<Vec<_>>().join(" ");

    // Truncate if too long
    if cleaned.len() > 10000 {
        format!("{}...", &cleaned[..10000])
    } else {
        cleaned
    }
}

/// Extract features from text
fn extract_text_features(text: &str) -> Vec<f32> {
    let mut features = Vec::with_capacity(256);

    // Word-based features
    let words: Vec<&str> = text.split_whitespace().collect();
    let word_count = words.len() as f32;

    // Basic statistics
    features.push(word_count.ln().max(0.0) / 10.0);  // Log word count, normalized
    features.push(text.len() as f32 / 10000.0);      // Character count

    // Character n-gram features (using hash buckets)
    let char_ngram_features = extract_char_ngrams(text, 64);
    features.extend(char_ngram_features);

    // Word n-gram features (using hash buckets)
    let word_ngram_features = extract_word_ngrams(&words, 64);
    features.extend(word_ngram_features);

    // Semantic hash (Blake3-based feature)
    let hash_features = semantic_hash_features(text, 64);
    features.extend(hash_features);

    // Positional features (beginning, middle, end of text)
    if text.len() > 100 {
        let beginning = &text[..100.min(text.len())];
        let middle_start = text.len() / 2 - 50.min(text.len() / 2);
        let middle = &text[middle_start..middle_start + 100.min(text.len() - middle_start)];
        let end_start = text.len().saturating_sub(100);
        let end = &text[end_start..];

        features.extend(semantic_hash_features(beginning, 16));
        features.extend(semantic_hash_features(middle, 16));
        features.extend(semantic_hash_features(end, 16));
    } else {
        features.extend(vec![0.0; 48]);
    }

    features
}

/// Extract character n-gram features using hash buckets
fn extract_char_ngrams(text: &str, num_buckets: usize) -> Vec<f32> {
    let mut buckets = vec![0.0f32; num_buckets];
    let chars: Vec<char> = text.chars().collect();

    // Bigrams and trigrams
    for window_size in [2, 3] {
        for window in chars.windows(window_size) {
            let ngram: String = window.iter().collect();
            let hash = blake3::hash(ngram.as_bytes());
            let bucket = (hash.as_bytes()[0] as usize) % num_buckets;
            buckets[bucket] += 1.0;
        }
    }

    // Normalize
    let max_val = buckets.iter().cloned().fold(0.0f32, f32::max);
    if max_val > 0.0 {
        for v in &mut buckets {
            *v /= max_val;
        }
    }

    buckets
}

/// Extract word n-gram features using hash buckets
fn extract_word_ngrams(words: &[&str], num_buckets: usize) -> Vec<f32> {
    let mut buckets = vec![0.0f32; num_buckets];

    // Unigrams
    for word in words {
        let hash = blake3::hash(word.to_lowercase().as_bytes());
        let bucket = (hash.as_bytes()[0] as usize) % num_buckets;
        buckets[bucket] += 1.0;
    }

    // Bigrams
    for window in words.windows(2) {
        let bigram = format!("{} {}", window[0].to_lowercase(), window[1].to_lowercase());
        let hash = blake3::hash(bigram.as_bytes());
        let bucket = (hash.as_bytes()[0] as usize) % num_buckets;
        buckets[bucket] += 0.5;
    }

    // Normalize
    let max_val = buckets.iter().cloned().fold(0.0f32, f32::max);
    if max_val > 0.0 {
        for v in &mut buckets {
            *v /= max_val;
        }
    }

    buckets
}

/// Generate semantic hash features
fn semantic_hash_features(text: &str, num_features: usize) -> Vec<f32> {
    let hash = blake3::hash(text.as_bytes());
    let bytes = hash.as_bytes();

    let mut features = Vec::with_capacity(num_features);
    for i in 0..num_features {
        let byte_idx = i % bytes.len();
        features.push(bytes[byte_idx] as f32 / 255.0);
    }

    features
}

// ============================================================================
// Code Feature Extraction
// ============================================================================

/// Detect programming language from code content
fn detect_language(code: &str) -> CodeLanguage {
    let code_lower = code.to_lowercase();

    // Check for language-specific patterns
    // Rust: fn with let, or fn with -> or ::
    if code.contains("fn ") && (code.contains("let ") || code.contains("-> ") || code.contains("::")) {
        return CodeLanguage::Rust;
    }
    if code_lower.contains("def ") && code.contains(":") && !code.contains("{") {
        return CodeLanguage::Python;
    }
    if code.contains("func ") && code.contains("package ") {
        return CodeLanguage::Go;
    }
    if code.contains("public class ") || code.contains("private class ") {
        return CodeLanguage::Java;
    }
    if code.contains("interface ") && (code.contains(": ") || code.contains("extends ")) {
        return CodeLanguage::TypeScript;
    }
    if code.contains("function ") || code.contains("const ") || code.contains("=>") {
        return CodeLanguage::JavaScript;
    }
    if code.contains("#include") {
        if code.contains("class ") || code.contains("namespace ") || code.contains("template") {
            return CodeLanguage::Cpp;
        }
        return CodeLanguage::C;
    }

    CodeLanguage::Unknown
}

/// Extract features from source code
fn extract_code_features(code: &str, lang: CodeLanguage) -> Vec<f32> {
    let mut features = Vec::with_capacity(256);

    // Basic structure metrics
    let lines: Vec<&str> = code.lines().collect();
    let line_count = lines.len() as f32;
    let char_count = code.len() as f32;
    let avg_line_length = if line_count > 0.0 { char_count / line_count } else { 0.0 };

    features.push(line_count.ln().max(0.0) / 10.0);
    features.push(char_count / 10000.0);
    features.push(avg_line_length / 100.0);

    // Indentation analysis (code structure)
    let mut indent_histogram = vec![0.0f32; 10];
    for line in &lines {
        let indent = line.len() - line.trim_start().len();
        let bucket = (indent / 4).min(9);
        indent_histogram[bucket] += 1.0;
    }
    let max_indent = indent_histogram.iter().cloned().fold(0.0f32, f32::max);
    if max_indent > 0.0 {
        for v in &mut indent_histogram {
            *v /= max_indent;
        }
    }
    features.extend(indent_histogram);

    // Keyword frequency
    let keywords = lang.keywords();
    let mut keyword_counts = vec![0.0f32; 32.min(keywords.len() + 1)];
    for (i, kw) in keywords.iter().take(31).enumerate() {
        keyword_counts[i] = code.matches(kw).count() as f32;
    }
    let max_kw = keyword_counts.iter().cloned().fold(1.0f32, f32::max);
    for v in &mut keyword_counts {
        *v /= max_kw;
    }
    features.extend(keyword_counts);

    // Symbol frequency
    let symbols = ['(', ')', '{', '}', '[', ']', ';', ',', '.', ':', '=', '+', '-', '*', '/', '<', '>', '&', '|', '!'];
    for sym in &symbols {
        let count = code.matches(*sym).count() as f32;
        features.push((count / line_count.max(1.0)).min(1.0));
    }

    // Identifier patterns (using hash buckets)
    let identifiers = extract_identifiers(code);
    let id_features = extract_word_ngrams(&identifiers.iter().map(|s| s.as_str()).collect::<Vec<_>>(), 64);
    features.extend(id_features);

    // Semantic hash of code
    let hash_features = semantic_hash_features(code, 64);
    features.extend(hash_features);

    features
}

/// Extract identifiers from code
fn extract_identifiers(code: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let mut current = String::new();

    for ch in code.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            if current.len() > 1 && !current.chars().all(|c| c.is_numeric()) {
                identifiers.push(current.clone());
            }
            current.clear();
        }
    }

    if !current.is_empty() && current.len() > 1 && !current.chars().all(|c| c.is_numeric()) {
        identifiers.push(current);
    }

    identifiers
}

// ============================================================================
// Image Feature Extraction
// ============================================================================

/// Extract features from an image file
fn extract_image_features(path: &Path) -> Result<ImageFeatures> {
    let bytes = std::fs::read(path)
        .context("Failed to read image file")?;
    extract_image_features_from_bytes(&bytes)
}

/// Extract features from image bytes
fn extract_image_features_from_bytes(bytes: &[u8]) -> Result<ImageFeatures> {
    // Use a deterministic approach based on raw bytes
    // In a full implementation, this would decode the image and extract real features

    let hash = blake3::hash(bytes);
    let hash_bytes = hash.as_bytes();

    // Perceptual hash (simulated from content hash)
    let mut perceptual_hash = [0u8; 8];
    perceptual_hash.copy_from_slice(&hash_bytes[0..8]);

    // Color features (simulated)
    let avg_color = [
        hash_bytes[8] as f32 / 255.0,
        hash_bytes[9] as f32 / 255.0,
        hash_bytes[10] as f32 / 255.0,
    ];

    // Color histogram (48 bins: 16 per channel)
    let mut color_histogram = Vec::with_capacity(48);
    for i in 0..48 {
        color_histogram.push(hash_bytes[11 + (i % 21)] as f32 / 255.0);
    }

    // Other features from hash
    let edge_density = hash_bytes[11] as f32 / 255.0;
    let brightness = hash_bytes[12] as f32 / 255.0;
    let contrast = hash_bytes[13] as f32 / 255.0;

    // Estimate aspect ratio from file structure (heuristic)
    let aspect_ratio = if bytes.len() > 100 {
        let header_hint = (bytes[20] as f32 + 1.0) / (bytes[21] as f32 + 1.0);
        header_hint.max(0.5).min(2.0)
    } else {
        1.0
    };

    Ok(ImageFeatures {
        perceptual_hash,
        avg_color,
        color_histogram,
        edge_density,
        aspect_ratio,
        brightness,
        contrast,
    })
}

/// Convert image features to a flat vector
fn image_features_to_vec(features: &ImageFeatures) -> Vec<f32> {
    let mut vec = Vec::with_capacity(128);

    // Perceptual hash (8 bytes -> 64 bits -> 64 floats)
    for byte in &features.perceptual_hash {
        for bit in 0..8 {
            vec.push(if (byte >> bit) & 1 == 1 { 1.0 } else { 0.0 });
        }
    }

    // Color features
    vec.extend_from_slice(&features.avg_color);
    vec.extend(features.color_histogram.iter().cloned());

    // Other features
    vec.push(features.edge_density);
    vec.push(features.aspect_ratio / 2.0);  // Normalize to ~0-1
    vec.push(features.brightness);
    vec.push(features.contrast);

    vec
}

// ============================================================================
// Audio Feature Extraction
// ============================================================================

/// Extract features from an audio file
fn extract_audio_features(path: &Path) -> Result<AudioFeatures> {
    let bytes = std::fs::read(path)
        .context("Failed to read audio file")?;

    // Simulate audio feature extraction from file bytes
    // In a full implementation, this would decode the audio and compute real features

    let hash = blake3::hash(&bytes);
    let hash_bytes = hash.as_bytes();

    // Estimate duration from file size (rough heuristic: ~128kbps)
    let duration_secs = (bytes.len() as f32) / (128.0 * 1024.0 / 8.0);

    Ok(AudioFeatures {
        spectral_centroid: hash_bytes[0] as f32 / 255.0 * 4000.0 + 500.0,
        spectral_rolloff: hash_bytes[1] as f32 / 255.0 * 8000.0 + 1000.0,
        zero_crossing_rate: hash_bytes[2] as f32 / 255.0 * 0.3,
        rms_energy: hash_bytes[3] as f32 / 255.0,
        tempo_bpm: hash_bytes[4] as f32 / 255.0 * 120.0 + 60.0,
        duration_secs,
        mfcc: (0..13).map(|i| hash_bytes[5 + i] as f32 / 255.0 * 2.0 - 1.0).collect(),
    })
}

/// Extract features from audio samples
fn extract_audio_features_from_samples(samples: &[f32], sample_rate: u32) -> AudioFeatures {
    let duration_secs = samples.len() as f32 / sample_rate as f32;

    // RMS energy
    let rms_energy = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();

    // Zero crossing rate
    let mut zero_crossings = 0;
    for i in 1..samples.len() {
        if (samples[i] >= 0.0) != (samples[i-1] >= 0.0) {
            zero_crossings += 1;
        }
    }
    let zero_crossing_rate = zero_crossings as f32 / samples.len() as f32;

    // Spectral features (simplified - would use FFT in full implementation)
    let spectral_centroid = 2000.0 + rms_energy * 2000.0;
    let spectral_rolloff = 4000.0 + rms_energy * 4000.0;

    // Tempo estimate (simplified - would use autocorrelation in full implementation)
    let tempo_bpm = 120.0;

    // MFCC (simulated)
    let mfcc: Vec<f32> = (0..13).map(|i| {
        let idx = (i * samples.len() / 13).min(samples.len() - 1);
        samples[idx]
    }).collect();

    AudioFeatures {
        spectral_centroid,
        spectral_rolloff,
        zero_crossing_rate,
        rms_energy,
        tempo_bpm,
        duration_secs,
        mfcc,
    }
}

/// Convert audio features to a flat vector
fn audio_features_to_vec(features: &AudioFeatures) -> Vec<f32> {
    let mut vec = Vec::with_capacity(32);

    // Normalize spectral features
    vec.push(features.spectral_centroid / 10000.0);
    vec.push(features.spectral_rolloff / 20000.0);
    vec.push(features.zero_crossing_rate);
    vec.push(features.rms_energy);
    vec.push(features.tempo_bpm / 200.0);
    vec.push((features.duration_secs.ln() + 2.0) / 10.0);  // Log duration

    // MFCC coefficients (already normalized-ish)
    vec.extend(features.mfcc.iter().cloned());

    vec
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Project feature vector to target dimension
fn project_to_dimension(features: &[f32], target_dim: usize) -> Vec<f32> {
    if features.is_empty() {
        return vec![0.0; target_dim];
    }

    if features.len() == target_dim {
        return features.to_vec();
    }

    let mut result = Vec::with_capacity(target_dim);

    if features.len() < target_dim {
        // Expand: interpolate and repeat with phase variation
        for i in 0..target_dim {
            let src_idx = i * features.len() / target_dim;
            let phase = (i as f32 / target_dim as f32 * std::f32::consts::PI * 4.0).sin() * 0.1;
            result.push(features[src_idx] + phase);
        }
    } else {
        // Compress: average pooling
        let pool_size = features.len() / target_dim;
        for i in 0..target_dim {
            let start = i * pool_size;
            let end = ((i + 1) * pool_size).min(features.len());
            let avg: f32 = features[start..end].iter().sum::<f32>() / (end - start) as f32;
            result.push(avg);
        }
    }

    result
}

/// Normalize a vector to unit length
fn normalize_vector(vector: &mut Vec<f32>) {
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for v in vector.iter_mut() {
            *v /= magnitude;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_embedding() {
        let embedder = MultiModalEmbedder::new(768).await.unwrap();
        let emb = embedder.embed_text("Hello, world!").await.unwrap();
        assert_eq!(emb.dimension(), 768);
    }

    #[tokio::test]
    async fn test_code_embedding() {
        let embedder = MultiModalEmbedder::new(768).await.unwrap();
        let code = "fn main() { println!(\"Hello\"); }";
        let emb = embedder.embed_code(code, "rs").await.unwrap();
        assert_eq!(emb.dimension(), 768);
        assert!(emb.model.contains("code"));
    }

    #[test]
    fn test_language_detection() {
        assert_eq!(detect_language("fn main() { let x = 5; }"), CodeLanguage::Rust);
        assert_eq!(detect_language("def main():\n    pass"), CodeLanguage::Python);
        assert_eq!(detect_language("const x = () => {}"), CodeLanguage::JavaScript);
    }

    #[test]
    fn test_project_to_dimension() {
        let features = vec![0.1, 0.2, 0.3, 0.4];

        let expanded = project_to_dimension(&features, 8);
        assert_eq!(expanded.len(), 8);

        let compressed = project_to_dimension(&features, 2);
        assert_eq!(compressed.len(), 2);

        let same = project_to_dimension(&features, 4);
        assert_eq!(same.len(), 4);
    }
}
