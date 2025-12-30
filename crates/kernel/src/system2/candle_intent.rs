//! Candle-backed intent inference.
//!
//! This is a deliberately minimal “real inference” path: tokenize -> vectorize ->
//! linear classifier (matmul + bias) -> argmax.
//!
//! It’s feature-gated so environments without Candle can still build.

use anyhow::Result;
use candle_core::{DType, Device, Tensor};

use super::IntentClass;
use std::collections::HashMap;

pub struct CandleIntentModel {
    device: Device,
    vocab: HashMap<String, usize>,
    // [num_classes, vocab_size]
    w: Tensor,
    // [num_classes]
    b: Tensor,
}

impl CandleIntentModel {
    pub fn new() -> Result<Self> {
        let device = Device::Cpu;

        // Small, repo-owned training phrases (mirrors stat_intent).
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

        // Build vocab.
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
                    class_token_counts[c * vocab_size + ti] += 1;
                    class_total_tokens[c] += 1;
                }
            }
        }

        // Naive Bayes weights: log P(token|class), bias=log P(class)
        let alpha = 1.0f32;
        let total_docs: f32 = class_doc_counts.iter().sum::<u32>() as f32;

        let mut b_vec = vec![0f32; 5];
        for c in 0..5 {
            let pc = (class_doc_counts[c] as f32 + alpha) / (total_docs + 5.0 * alpha);
            b_vec[c] = pc.ln();
        }

        let mut w_vec = vec![0f32; 5 * vocab_size];
        for c in 0..5 {
            let denom = (class_total_tokens[c] as f32) + alpha * (vocab_size as f32);
            for ti in 0..vocab_size {
                let count = class_token_counts[c * vocab_size + ti] as f32;
                let p = (count + alpha) / denom;
                w_vec[c * vocab_size + ti] = p.ln();
            }
        }

        let w = Tensor::from_slice(&w_vec, (5usize, vocab_size), &device)?.to_dtype(DType::F32)?;
        let b = Tensor::from_slice(&b_vec, (5usize,), &device)?.to_dtype(DType::F32)?;

        Ok(Self { device, vocab, w, b })
    }

    pub fn classify(&self, text: &str) -> Result<IntentClass> {
        let x = self.vectorize(text)?; // [vocab_size]
        let logits = self.w.matmul(&x.unsqueeze(1)?)?.squeeze(1)?;
        let logits = (logits + &self.b)?.to_vec1::<f32>()?;

        let mut best_i = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for (i, v) in logits.iter().copied().enumerate() {
            if v > best_v {
                best_v = v;
                best_i = i;
            }
        }

        Ok(match best_i {
            0 => IntentClass::Open,
            1 => IntentClass::Close,
            2 => IntentClass::Search,
            3 => IntentClass::Create,
            _ => IntentClass::Generic,
        })
    }

    fn vectorize(&self, text: &str) -> Result<Tensor> {
        let tokens = tokenize(text);
        let mut vec = vec![0f32; self.vocab.len()];
        for tok in tokens {
            if let Some(&i) = self.vocab.get(tok.as_str()) {
                vec[i] = 1.0;
            }
        }
        Ok(Tensor::from_slice(&vec, (self.vocab.len(),), &self.device)?.to_dtype(DType::F32)?)
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
}
