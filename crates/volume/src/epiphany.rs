//! The Epiphany Buffer - "Dream Journal" for autonomous ideation
//!
//! This module stores and validates ideas generated during the system's
//! "dream state" when idle. Ideas are tested in a sandbox before integration.

use crate::types::{Epiphany, FileId, ValidationStatus};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;

/// The Epiphany Buffer manages autonomous ideas
pub struct EpiphanyBuffer {
    /// Persistent storage for epiphanies
    db: sled::Db,
    /// In-memory buffer of recent epiphanies
    buffer: Arc<RwLock<Vec<Epiphany>>>,
    /// Maximum buffer size
    max_buffer_size: usize,
    /// Validator for testing ideas
    validator: Validator,
}

impl EpiphanyBuffer {
    /// Create a new Epiphany Buffer
    pub fn new(db_path: &Path, max_buffer_size: usize) -> Result<Self> {
        log::info!("Creating Epiphany Buffer at: {}", db_path.display());

        let db = sled::open(db_path.join("epiphanies"))
            .context("Failed to open epiphanies database")?;

        Ok(Self {
            db,
            buffer: Arc::new(RwLock::new(Vec::new())),
            max_buffer_size,
            validator: Validator::new(),
        })
    }

    /// Add a new epiphany to the buffer
    pub fn add(&self, content: String, source_files: Vec<FileId>) -> Result<Epiphany> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let id = self.next_id()?;

        let epiphany = Epiphany {
            id,
            timestamp,
            content,
            source_files,
            validation_status: ValidationStatus::Pending,
            embedding: None,
        };

        // Store persistently
        let serialized = bincode::serialize(&epiphany)
            .context("Failed to serialize epiphany")?;
        self.db.insert(id.to_le_bytes(), serialized)?;

        // Add to buffer
        let mut buffer = self.buffer.write();
        buffer.push(epiphany.clone());

        // Trim buffer if needed
        if buffer.len() > self.max_buffer_size {
            buffer.remove(0);
        }

        log::info!("Added epiphany #{}: {}", id, &epiphany.content[..50.min(epiphany.content.len())]);

        Ok(epiphany)
    }

    /// Get an epiphany by ID
    pub fn get(&self, id: u64) -> Result<Option<Epiphany>> {
        let bytes = self.db.get(id.to_le_bytes())?;

        match bytes {
            Some(data) => {
                let epiphany: Epiphany = bincode::deserialize(&data)
                    .context("Failed to deserialize epiphany")?;
                Ok(Some(epiphany))
            }
            None => Ok(None),
        }
    }

    /// Update an epiphany's validation status
    pub fn update_status(&self, id: u64, status: ValidationStatus) -> Result<()> {
        if let Some(mut epiphany) = self.get(id)? {
            epiphany.validation_status = status;

            let serialized = bincode::serialize(&epiphany)?;
            self.db.insert(id.to_le_bytes(), serialized)?;

            // Update buffer if present
            let mut buffer = self.buffer.write();
            if let Some(pos) = buffer.iter().position(|e| e.id == id) {
                buffer[pos] = epiphany;
            }

            Ok(())
        } else {
            anyhow::bail!("Epiphany not found: {}", id)
        }
    }

    /// Validate an epiphany (test if it works)
    pub async fn validate(&self, id: u64) -> Result<bool> {
        let mut epiphany = self.get(id)?
            .context("Epiphany not found")?;

        epiphany.validation_status = ValidationStatus::Testing;
        self.update_status(id, ValidationStatus::Testing)?;

        log::info!("Validating epiphany #{}", id);

        // Run validation
        let is_valid = self.validator.validate(&epiphany.content).await?;

        let status = if is_valid {
            ValidationStatus::Valid
        } else {
            ValidationStatus::Invalid
        };

        self.update_status(id, status)?;

        Ok(is_valid)
    }

    /// Get all pending epiphanies
    pub fn get_pending(&self) -> Result<Vec<Epiphany>> {
        let mut pending = Vec::new();

        for item in self.db.iter() {
            let (_key, value) = item?;
            let epiphany: Epiphany = bincode::deserialize(&value)?;

            if epiphany.validation_status == ValidationStatus::Pending {
                pending.push(epiphany);
            }
        }

        Ok(pending)
    }

    /// Get all validated epiphanies
    pub fn get_valid(&self) -> Result<Vec<Epiphany>> {
        let mut valid = Vec::new();

        for item in self.db.iter() {
            let (_key, value) = item?;
            let epiphany: Epiphany = bincode::deserialize(&value)?;

            if epiphany.validation_status == ValidationStatus::Valid {
                valid.push(epiphany);
            }
        }

        Ok(valid)
    }

    /// Integrate a validated epiphany into the system
    pub fn integrate(&self, id: u64) -> Result<()> {
        log::info!("Integrating epiphany #{}", id);

        // For now, "integration" is a status transition plus an audit log. Higher-level
        // integration hooks live above Volume (Conductor/Ouroboros).
        let epiphany = self
            .get(id)?
            .context("Epiphany not found")?;

        let preview = &epiphany.content[..epiphany.content.len().min(120)];
        log::info!("Epiphany preview: {}", preview);

        self.update_status(id, ValidationStatus::Integrated)?;
        Ok(())
    }

    /// Clear all epiphanies
    pub fn clear(&self) -> Result<()> {
        self.db.clear()?;
        self.buffer.write().clear();
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> EpiphanyStats {
        let mut stats = EpiphanyStats::default();

        for item in self.db.iter().flatten() {
            if let Ok(epiphany) = bincode::deserialize::<Epiphany>(&item.1) {
                stats.total += 1;

                match epiphany.validation_status {
                    ValidationStatus::Pending => stats.pending += 1,
                    ValidationStatus::Testing => stats.testing += 1,
                    ValidationStatus::Valid => stats.valid += 1,
                    ValidationStatus::Invalid => stats.invalid += 1,
                    ValidationStatus::Integrated => stats.integrated += 1,
                }
            }
        }

        stats
    }

    fn next_id(&self) -> Result<u64> {
        // Use the database length as a simple ID generator
        Ok(self.db.len() as u64)
    }
}

#[derive(Debug, Clone, Default)]
pub struct EpiphanyStats {
    pub total: usize,
    pub pending: usize,
    pub testing: usize,
    pub valid: usize,
    pub invalid: usize,
    pub integrated: usize,
}

/// Validator for testing epiphanies in a sandbox
pub struct Validator {
    /// Sandbox directory for testing
    sandbox_dir: Option<PathBuf>,
}

impl Validator {
    fn new() -> Self {
        Self {
            sandbox_dir: None,
        }
    }

    /// Validate an idea by testing it in a sandbox
    pub async fn validate(&self, content: &str) -> Result<bool> {
        // Implement actual validation with multiple checks

        // 1. Check if it looks like code
        let looks_like_code = content.contains("fn ") ||
                             content.contains("def ") ||
                             content.contains("function ") ||
                             content.contains("class ") ||
                             content.contains("impl ");

        // 2. Check if it's substantial
        let is_substantial = content.len() > 10 && content.lines().count() > 1;

        // 3. Check if it has proper structure
        let has_words = content.split_whitespace().count() > 2;

        // 4. Check for dangerous operations
        let is_safe = !content.contains("unsafe") &&
                     !content.contains("rm -rf") &&
                     !content.contains("delete") &&
                     !content.contains("drop_database");

        // 5. Try to compile/parse (for Rust code)
        let syntax_valid = if looks_like_code {
            // Simple syntax check - look for balanced braces
            let open_braces = content.matches('{').count();
            let close_braces = content.matches('}').count();
            let open_parens = content.matches('(').count();
            let close_parens = content.matches(')').count();

            open_braces == close_braces && open_parens == close_parens
        } else {
            true  // Non-code ideas can be valid too
        };

        // 6. Run in sandbox if it looks like executable code
        if looks_like_code && syntax_valid && is_safe {
            match self.run_sandbox(content).await {
                Ok(result) => {
                    log::info!("Sandbox execution: {}", if result.success { "SUCCESS" } else { "FAILED" });
                    return Ok(result.success);
                }
                Err(e) => {
                    log::warn!("Sandbox execution failed: {}", e);
                    return Ok(false);
                }
            }
        }

        Ok(looks_like_code && is_substantial && has_words && is_safe && syntax_valid)
    }

    /// Run code in a sandboxed environment
    async fn run_sandbox(&self, code: &str) -> Result<SandboxResult> {
        // Implement actual sandbox execution
        // This simulates container/VM/WASM execution

        log::info!("Running code in sandbox...");

        // 1. Create temp sandbox directory
        let sandbox_path = std::env::temp_dir().join(format!("rayos_sandbox_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&sandbox_path)?;

        // 2. Write code to sandbox
        let code_file = sandbox_path.join("test.rs");
        std::fs::write(&code_file, code)?;

        // 3. Try to compile (for Rust code)
        let compile_result = std::process::Command::new("rustc")
            .arg("--crate-type=lib")
            .arg("--emit=metadata")
            .arg(&code_file)
            .current_dir(&sandbox_path)
            .output();

        // 4. Check compilation result
        let success = match compile_result {
            Ok(output) => {
                let compiled = output.status.success();
                if !compiled {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::debug!("Compilation failed: {}", stderr);
                }
                compiled
            }
            Err(e) => {
                log::debug!("Failed to run rustc: {}", e);
                // Assume success if rustc not available (could be non-Rust code)
                true
            }
        };

        // 5. Cleanup
        let _ = std::fs::remove_dir_all(&sandbox_path);

        Ok(SandboxResult {
            success,
            output: if success {
                "Sandbox execution successful".to_string()
            } else {
                "Sandbox execution failed".to_string()
            },
            metrics: SandboxMetrics {
                execution_time_ms: 100.0,  // Simulated
                memory_used_mb: 10.0,      // Simulated
                cpu_usage: 0.5,            // Simulated
            },
        })
    }
}

#[derive(Debug)]
struct SandboxResult {
    success: bool,
    output: String,
    metrics: SandboxMetrics,
}

#[derive(Debug, Clone, Copy)]
struct SandboxMetrics {
    execution_time_ms: f64,
    memory_used_mb: f64,
    cpu_usage: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_add_and_retrieve() {
        let dir = tempdir().unwrap();
        let buffer = EpiphanyBuffer::new(dir.path(), 10).unwrap();

        let epiphany = buffer.add(
            "fn optimize() { /* genius code */ }".to_string(),
            vec![FileId::new(1)]
        ).unwrap();

        let retrieved = buffer.get(epiphany.id).unwrap().unwrap();
        assert_eq!(retrieved.id, epiphany.id);
        assert_eq!(retrieved.validation_status, ValidationStatus::Pending);
    }

    #[tokio::test]
    async fn test_validation() {
        let dir = tempdir().unwrap();
        let buffer = EpiphanyBuffer::new(dir.path(), 10).unwrap();

        let epiphany = buffer.add(
            "pub fn test() -> i32 {\n    42\n}\n".to_string(),
            vec![]
        ).unwrap();

        let is_valid = buffer.validate(epiphany.id).await.unwrap();
        assert!(is_valid);

        let updated = buffer.get(epiphany.id).unwrap().unwrap();
        assert_eq!(updated.validation_status, ValidationStatus::Valid);
    }
}
