//! Lightweight statistical intent classifier (non-LLM, non-keyword-heuristic).
//!
//! Goal: replace hardcoded `str::contains(...)` intent parsing with a real
//! scoring model that can run everywhere.
//!
//! Approach:
//! - tokenize text into words
//! - vectorize via bag-of-words counts
//! - score using a tiny multinomial Naive Bayes model trained from an
//!   embedded, repo-owned phrase set
//!
//! This is intentionally small, deterministic, and dependency-free.

use super::IntentClass;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StatIntentModel {
    vocab: HashMap<String, usize>,
    // log P(token | class), flattened [num_classes * vocab_size]
    w: Vec<f32>,
    // log P(class)
    b: [f32; 5],
    vocab_size: usize,
}

impl StatIntentModel {
    pub fn new() -> Self {
        // Small, repo-owned training phrases.
        // Keep it compact but cover common paraphrases.
        let samples: &[(&str, IntentClass)] = &[
            // Open
            ("open settings", IntentClass::Open),
            ("launch the app", IntentClass::Open),
            ("show preferences", IntentClass::Open),
            ("start the program", IntentClass::Open),
            // Close
            ("close the window", IntentClass::Close),
            ("exit now", IntentClass::Close),
            ("quit the program", IntentClass::Close),
            ("terminate the app", IntentClass::Close),
            // Search
            ("search for logs", IntentClass::Search),
            ("find the config", IntentClass::Search),
            ("look up documentation", IntentClass::Search),
            ("locate the file", IntentClass::Search),
            // Create
            ("create a new file", IntentClass::Create),
            ("write a note", IntentClass::Create),
            ("make a document", IntentClass::Create),
            ("generate a report", IntentClass::Create),
            // Generic
            ("optimize the rendering pipeline", IntentClass::Generic),
            ("help", IntentClass::Generic),
            ("status", IntentClass::Generic),
            ("do the thing", IntentClass::Generic),
        ];

        // Build vocabulary.
        let mut vocab: HashMap<String, usize> = HashMap::new();
        for (text, _) in samples {
            for tok in tokenize(text) {
                if !vocab.contains_key(&tok) {
                    let next = vocab.len();
                    vocab.insert(tok, next);
                }
            }
        }
        let vocab_size = vocab.len().max(1);

        // Count tokens per class.
        let mut class_doc_counts = [0u32; 5];
        let mut class_token_counts = vec![0u32; 5 * vocab_size];
        let mut class_total_tokens = [0u32; 5];

        for (text, cls) in samples {
            let c = cls.index();
            class_doc_counts[c] += 1;
            for tok in tokenize(text) {
                if let Some(&ti) = vocab.get(&tok) {
                    let idx = c * vocab_size + ti;
                    class_token_counts[idx] += 1;
                    class_total_tokens[c] += 1;
                }
            }
        }

        // Naive Bayes with Laplace smoothing.
        let alpha = 1.0f32;
        let total_docs: f32 = class_doc_counts.iter().sum::<u32>() as f32;

        let mut b = [0f32; 5];
        for c in 0..5 {
            // P(class)
            let pc = (class_doc_counts[c] as f32 + alpha) / (total_docs + 5.0 * alpha);
            b[c] = pc.ln();
        }

        let mut w = vec![0f32; 5 * vocab_size];
        for c in 0..5 {
            let denom = (class_total_tokens[c] as f32) + alpha * (vocab_size as f32);
            for ti in 0..vocab_size {
                let count = class_token_counts[c * vocab_size + ti] as f32;
                let p = (count + alpha) / denom;
                w[c * vocab_size + ti] = p.ln();
            }
        }

        Self {
            vocab,
            w,
            b,
            vocab_size,
        }
    }

    pub fn classify(&self, text: &str) -> IntentClass {
        let mut scores = self.b;
        let mut any = false;

        for tok in tokenize(text) {
            if let Some(&ti) = self.vocab.get(&tok) {
                any = true;
                for c in 0..5 {
                    scores[c] += self.w[c * self.vocab_size + ti];
                }
            }
        }

        // If we saw nothing in-vocab, stay deterministic.
        if !any {
            return IntentClass::Generic;
        }

        let mut best = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for (i, v) in scores.iter().copied().enumerate() {
            if v > best_v {
                best_v = v;
                best = i;
            }
        }

        IntentClass::from_index(best)
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}
