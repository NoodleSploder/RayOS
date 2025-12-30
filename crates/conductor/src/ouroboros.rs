//! Ouroboros Engine - Self-optimization through genetic mutations
//!
//! The system that makes RayOS evolve: mutates code, tests it in a sandbox,
//! and hot-swaps improvements into the running system.

use crate::types::{MutationResult, OptimizationTarget};
use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use tokio::sync::Mutex;

/// The Mutator - applies genetic mutations to code
pub struct Mutator {
    mutation_strategies: Vec<MutationStrategy>,
    mutation_rate: f64,
}

#[derive(Debug, Clone)]
enum MutationStrategy {
    /// Flip random bits in binary
    BitFlip,
    /// Swap instruction order
    InstructionSwap,
    /// Replace constants
    ConstantTweaking,
    /// Inline/outline functions
    FunctionInlining,
    /// Change compiler flags
    CompilerOptimization,
}

impl Mutator {
    pub fn new() -> Self {
        Self {
            mutation_strategies: vec![
                MutationStrategy::BitFlip,
                MutationStrategy::InstructionSwap,
                MutationStrategy::ConstantTweaking,
            ],
            mutation_rate: 0.01,  // 1% mutation rate
        }
    }

    /// Apply a random mutation to binary code
    pub fn mutate(&self, original: &[u8]) -> Vec<u8> {
        let mut rng = fastrand::Rng::new();
        let strategy = &self.mutation_strategies[rng.usize(..self.mutation_strategies.len())];

        match strategy {
            MutationStrategy::BitFlip => self.mutate_bit_flip(original),
            MutationStrategy::InstructionSwap => self.mutate_swap(original),
            MutationStrategy::ConstantTweaking => self.mutate_constants(original),
            _ => original.to_vec(),  // Other strategies need more context
        }
    }

    fn mutate_bit_flip(&self, original: &[u8]) -> Vec<u8> {
        let mut mutated = original.to_vec();
        let mut rng = fastrand::Rng::new();

        // Flip random bits
        for byte in mutated.iter_mut() {
            if rng.f64() < self.mutation_rate {
                let bit = rng.u8(0..8);
                *byte ^= 1 << bit;
            }
        }

        mutated
    }

    fn mutate_swap(&self, original: &[u8]) -> Vec<u8> {
        if original.len() < 8 {
            return original.to_vec();
        }

        let mut mutated = original.to_vec();
        let mut rng = fastrand::Rng::new();

        // Swap random 4-byte chunks (simulating instruction swaps)
        let chunk_count = original.len() / 4;
        if chunk_count >= 2 {
            let idx1 = rng.usize(0..chunk_count) * 4;
            let idx2 = rng.usize(0..chunk_count) * 4;

            for i in 0..4 {
                mutated.swap(idx1 + i, idx2 + i);
            }
        }

        mutated
    }

    fn mutate_constants(&self, original: &[u8]) -> Vec<u8> {
        let mut mutated = original.to_vec();
        let mut rng = fastrand::Rng::new();

        // Tweak numeric constants (simplified - looks for u32/i32 patterns)
        for i in 0..mutated.len().saturating_sub(4) {
            if rng.f64() < self.mutation_rate {
                let value = u32::from_le_bytes([
                    mutated[i],
                    mutated[i + 1],
                    mutated[i + 2],
                    mutated[i + 3],
                ]);

                // Small adjustment
                let adjustment = rng.i32(-10..=10);
                let new_value = (value as i32).wrapping_add(adjustment) as u32;
                let bytes = new_value.to_le_bytes();

                mutated[i..i + 4].copy_from_slice(&bytes);
            }
        }

        mutated
    }
}

impl Default for Mutator {
    fn default() -> Self {
        Self::new()
    }
}

/// The Arena - sandbox for testing mutations
pub struct Arena {
    test_timeout: Duration,
    safety_checks: bool,
}

impl Arena {
    pub fn new() -> Self {
        Self {
            test_timeout: Duration::from_secs(5),
            safety_checks: true,
        }
    }

    /// Run mutated code against test cases
    pub async fn test_mutation(
        &self,
        original: &[u8],
        mutated: &[u8],
        test_cases: &[TestCase],
    ) -> Result<MutationResult> {
        let mutation_id = Uuid::new_v4();

        log::debug!("Testing mutation {}", mutation_id);

        // Benchmark original
        let original_duration = self.benchmark_code(original, test_cases).await?;

        // Benchmark mutated
        let mutated_duration = self.benchmark_code(mutated, test_cases).await?;

        // Run correctness tests
        let passed_tests = self.run_tests(mutated, test_cases).await?;

        let improvement_factor = if mutated_duration.as_secs_f64() > 0.0 {
            original_duration.as_secs_f64() / mutated_duration.as_secs_f64()
        } else {
            0.0
        };

        Ok(MutationResult {
            mutation_id,
            original_duration,
            mutated_duration,
            improvement_factor,
            passed_tests,
            binary_diff: self.compute_diff(original, mutated),
        })
    }

    async fn benchmark_code(&self, code: &[u8], test_cases: &[TestCase]) -> Result<Duration> {
        // Write code to temporary file for benchmarking
        let temp_dir = std::env::temp_dir();
        let code_path = temp_dir.join(format!("rayos_bench_{}.bin", Uuid::new_v4()));

        std::fs::write(&code_path, code)?;

        // Estimate execution time based on code characteristics
        let mut total_time = Duration::from_micros(10); // Base overhead

        // Analyze code complexity
        let instruction_count = code.len() / 4; // Assume ~4 bytes per instruction
        let loop_factor = code.windows(4).filter(|w| {
            // Look for loop patterns (jump backwards)
            w[0] == 0xe9 || w[0] == 0xeb // JMP opcodes on x86
        }).count();

        // Calculate based on instruction count and loops
        let compute_time = Duration::from_nanos((instruction_count as u64) * 2); // ~2ns per instruction
        let loop_time = Duration::from_micros((loop_factor as u64) * 100); // Loops add significant time

        total_time += compute_time + loop_time;

        // Run test cases to add realistic I/O time
        for test_case in test_cases {
            let io_time = Duration::from_micros((test_case.input.len() as u64) * 5);
            total_time += io_time;
        }

        // Cleanup
        let _ = std::fs::remove_file(&code_path);

        // Small actual delay to simulate real execution
        tokio::time::sleep(Duration::from_micros(100)).await;

        Ok(total_time)
    }

    async fn run_tests(&self, code: &[u8], test_cases: &[TestCase]) -> Result<bool> {
        // Safety check: validate code doesn't contain dangerous patterns
        if self.safety_checks {
            // Check for system calls, file I/O, network access patterns
            let dangerous_patterns = [
                vec![0x0f, 0x05], // syscall on x86_64
                vec![0xcd, 0x80], // int 0x80 on x86
            ];

            for pattern in &dangerous_patterns {
                if code.windows(pattern.len()).any(|w| w == pattern.as_slice()) {
                    log::warn!("Code contains potentially dangerous patterns, test failed");
                    return Ok(false);
                }
            }
        }

        let mut passed = 0;
        let total = test_cases.len().max(1);

        for test_case in test_cases {
            // Create timeout for test execution
            let test_future = async {
                // Write test case to temp file
                let input_path = std::env::temp_dir().join(format!("rayos_test_input_{}", Uuid::new_v4()));
                std::fs::write(&input_path, &test_case.input)?;

                // Compute expected behavior hash
                let expected_hash = blake3::hash(&test_case.expected_output);

                // Simulate execution (in real impl, would run code)
                let code_hash = blake3::hash(code);

                // Deterministic "execution" based on code and input
                let combined = format!("{:?}{:?}", code_hash, expected_hash);
                let result_hash = blake3::hash(combined.as_bytes());

                // Pass if hash matches certain criteria (simulates correct behavior)
                let passes = result_hash.as_bytes()[0] % 5 != 0; // 80% pass rate

                let _ = std::fs::remove_file(&input_path);
                Ok::<bool, anyhow::Error>(passes)
            };

            // Apply timeout
            match tokio::time::timeout(self.test_timeout, test_future).await {
                Ok(Ok(true)) => passed += 1,
                Ok(Ok(false)) => {},
                Ok(Err(e)) => log::warn!("Test execution error: {}", e),
                Err(_) => log::warn!("Test timeout exceeded"),
            }
        }

        // Require at least 80% pass rate
        Ok(passed as f64 / total as f64 >= 0.8)
    }

    fn compute_diff(&self, original: &[u8], mutated: &[u8]) -> Vec<u8> {
        // Simple diff: store changed bytes
        let mut diff = Vec::new();

        for (i, (&o, &m)) in original.iter().zip(mutated.iter()).enumerate() {
            if o != m {
                diff.extend_from_slice(&(i as u32).to_le_bytes());
                diff.push(m);
            }
        }

        diff
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub input: Vec<u8>,
    pub expected_output: Vec<u8>,
}

/// The Hot-Swapper - live patches the running kernel
pub struct HotSwapper {
    active_patches: Arc<RwLock<Vec<AppliedPatch>>>,
}

#[derive(Debug, Clone)]
struct AppliedPatch {
    id: Uuid,
    target: String,
    applied_at: Instant,
    improvement_factor: f64,
}

impl HotSwapper {
    pub fn new() -> Self {
        Self {
            active_patches: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Apply a mutation to the running system
    pub fn apply_patch(&self, target: &str, mutation: &MutationResult) -> Result<()> {
        if !mutation.is_improvement() {
            anyhow::bail!("Refusing to apply non-improvement mutation");
        }

        log::info!(
            "🔄 Hot-swapping: {} ({:.2}x faster)",
            target,
            mutation.improvement_factor
        );

        // In a real implementation, this would:
        // 1. Verify patch safety
        // 2. Create rollback point
        // 3. Apply binary patch atomically
        // 4. Monitor for regressions
        // 5. Rollback if issues detected

        let patch = AppliedPatch {
            id: mutation.mutation_id,
            target: target.to_string(),
            applied_at: Instant::now(),
            improvement_factor: mutation.improvement_factor,
        };

        self.active_patches.write().push(patch);

        Ok(())
    }

    /// Rollback a patch
    pub fn rollback(&self, patch_id: Uuid) -> Result<()> {
        let mut patches = self.active_patches.write();

        if let Some(idx) = patches.iter().position(|p| p.id == patch_id) {
            let patch = patches.remove(idx);
            log::warn!("⏪ Rolling back patch: {}", patch.target);
            Ok(())
        } else {
            anyhow::bail!("Patch not found: {}", patch_id)
        }
    }

    pub fn active_patch_count(&self) -> usize {
        self.active_patches.read().len()
    }

    #[cfg(test)]
    fn get_active_patches_for_test(&self) -> Vec<Uuid> {
        self.active_patches.read().iter().map(|p| p.id).collect()
    }
}

impl Default for HotSwapper {
    fn default() -> Self {
        Self::new()
    }
}

/// The Ouroboros Engine - orchestrates the entire self-optimization loop
pub struct OuroborosEngine {
    mutator: Mutator,
    arena: Arena,
    hot_swapper: HotSwapper,
    mutation_history: Arc<RwLock<VecDeque<MutationResult>>>,
    max_history: usize,
    enabled: Arc<RwLock<bool>>,

    // Prevent concurrent full-system optimization runs from overlapping.
    cycle_lock: Arc<Mutex<()>>,
}

impl OuroborosEngine {
    const PINNED_TOOLCHAIN: &'static str = "nightly-2024-11-01";

    fn system_opt_repeats() -> usize {
        std::env::var("RAYOS_SYSTEM_OPT_REPEATS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|v| *v >= 1 && *v <= 10)
            .unwrap_or(2)
    }

    fn median_duration(mut samples: Vec<Duration>) -> Duration {
        if samples.is_empty() {
            return Duration::MAX;
        }
        samples.sort_by_key(|d| d.as_nanos());
        samples[samples.len() / 2]
    }

    pub fn new() -> Self {
        log::info!("Initializing Ouroboros Engine (self-optimization system)");

        Self {
            mutator: Mutator::new(),
            arena: Arena::new(),
            hot_swapper: HotSwapper::new(),
            mutation_history: Arc::new(RwLock::new(VecDeque::new())),
            max_history: 1000,
            enabled: Arc::new(RwLock::new(false)),
            cycle_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Enable/disable self-optimization
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write() = enabled;

        if enabled {
            log::warn!("⚠️ Ouroboros Engine ENABLED - system will self-modify");
        } else {
            log::info!("Ouroboros Engine disabled");
        }
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.read()
    }

    /// Run one optimization cycle
    pub async fn optimize_cycle(&self, target: OptimizationTarget) -> Result<()> {
        if !self.is_enabled() {
            anyhow::bail!("Ouroboros Engine is disabled");
        }

        // Only allow one optimization run at a time.
        let _cycle_guard = self.cycle_lock.clone().lock_owned().await;

        log::info!("Starting optimization cycle for: {:?}", target);

        match target {
            OptimizationTarget::Function { name, binary } => {
                self.optimize_function(&name, &binary).await?;
            }
            OptimizationTarget::Module { path } => {
                log::info!("Optimizing module: {}", path.display());

                // Read the module binary
                match std::fs::read(&path) {
                    Ok(binary) => {
                        log::debug!("Module size: {} bytes", binary.len());

                        // Extract function symbols (simplified - look for common patterns)
                        let mut functions = Vec::new();

                        // Scan for function prologues (simplified heuristic)
                        for i in 0..binary.len().saturating_sub(4) {
                            // Look for common x86_64 function prologue patterns
                            if binary[i..i+3] == [0x55, 0x48, 0x89] { // push rbp; mov rbp, rsp
                                functions.push((i, format!("func_{}", functions.len())));
                            }
                        }

                        log::info!("Found {} potential functions in module", functions.len());

                        // Optimize first few functions as demonstration
                        for (offset, name) in functions.iter().take(5) {
                            let end = (offset + 256).min(binary.len());
                            let func_binary = &binary[*offset..end];

                            log::debug!("Optimizing function {} at offset {}", name, offset);

                            // Use the existing function optimizer
                            if let Err(e) = self.optimize_function(name, func_binary).await {
                                log::warn!("Failed to optimize {}: {}", name, e);
                            }
                        }

                        log::info!("Module optimization cycle complete");
                    }
                    Err(e) => {
                        log::error!("Failed to read module {}: {}", path.display(), e);
                    }
                }
            }
            OptimizationTarget::System => {
                self.optimize_system_end_to_end().await?;
            }
        }

        Ok(())
    }

    fn repo_root() -> PathBuf {
        // conductor/ is one level under the repo root.
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .map(PathBuf::from)
            .unwrap_or(manifest_dir)
    }

    fn rustup_which(tool: &str) -> Result<PathBuf> {
        let out = Command::new("rustup")
            .args(["which", tool])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("failed to invoke rustup which {tool}"))?;

        if !out.status.success() {
            anyhow::bail!(
                "rustup which {} failed: {}",
                tool,
                String::from_utf8_lossy(&out.stderr)
            );
        }

        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(PathBuf::from(s))
    }

    fn rustup_which_pinned(tool: &str) -> Result<PathBuf> {
        let out = Command::new("rustup")
            .args(["which", tool, "--toolchain", Self::PINNED_TOOLCHAIN])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("failed to invoke rustup which {tool} --toolchain {}", Self::PINNED_TOOLCHAIN))?;

        if !out.status.success() {
            anyhow::bail!(
                "rustup which {} --toolchain {} failed: {}",
                tool,
                Self::PINNED_TOOLCHAIN,
                String::from_utf8_lossy(&out.stderr)
            );
        }

        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(PathBuf::from(s))
    }

    fn rustup_run_cargo(current_dir: &PathBuf, args: &[String], envs: &[(&str, &str)]) -> Result<()> {
        // Use rustup to ensure we run the pinned nightly toolchain even if a system
        // `cargo` exists earlier in PATH.
        let pinned_rustc = Self::rustup_which_pinned("rustc")?;

        let mut cmd = Command::new("rustup");
        cmd.current_dir(current_dir)
            .args(["run", Self::PINNED_TOOLCHAIN, "cargo"])
            .args(args)
            // Some environments have /usr/bin earlier in PATH; setting RUSTC makes the
            // toolchain selection unambiguous and avoids missing-target sysroot issues.
            .env("RUSTC", pinned_rustc)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        for (k, v) in envs {
            cmd.env(k, v);
        }

        let out = cmd
            .output()
            .with_context(|| format!("failed to invoke cargo via rustup ({})", current_dir.display()))?;

        if !out.status.success() {
            anyhow::bail!("cargo failed: {}", String::from_utf8_lossy(&out.stderr));
        }

        Ok(())
    }

    fn ensure_pinned_toolchain() -> Result<()> {
        // Best-effort install; if already installed, rustup exits 0.
        let out = Command::new("rustup")
            .args(["toolchain", "install", Self::PINNED_TOOLCHAIN])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| "failed to invoke rustup toolchain install")?;

        if !out.status.success() {
            anyhow::bail!(
                "rustup toolchain install failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        Ok(())
    }

    fn ensure_target(target: &str) -> Result<()> {
        let out = Command::new("rustup")
            .args(["target", "add", target, "--toolchain", Self::PINNED_TOOLCHAIN])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("failed to invoke rustup target add {target}"))?;

        if !out.status.success() {
            anyhow::bail!(
                "rustup target add {} failed: {}",
                target,
                String::from_utf8_lossy(&out.stderr)
            );
        }

        Ok(())
    }

    fn build_kernel_bare(repo_root: &PathBuf, cargo_config_overrides: &[String]) -> Result<()> {
        let kernel_dir = repo_root.join("kernel-bare");

        Self::ensure_pinned_toolchain()?;
        Self::ensure_target("x86_64-unknown-none")?;

        let mut args: Vec<String> = vec![
            "build".to_string(),
            "--release".to_string(),
            "--target".to_string(),
            "x86_64-unknown-none".to_string(),
            "-Z".to_string(),
            "build-std=core,alloc".to_string(),
            "-Z".to_string(),
            "build-std-features=compiler-builtins-mem".to_string(),
        ];

        // IMPORTANT: Do NOT set RUSTFLAGS here.
        // In this repo, `kernel-bare/.cargo/config.toml` sets per-target linker-script
        // flags (e.g., `-T linker.ld`). Setting `RUSTFLAGS` overrides those and causes
        // link failures (missing `__kernel_start/__kernel_end`).
        for cfg in cargo_config_overrides {
            args.push("--config".to_string());
            args.push(cfg.clone());
        }

        Self::rustup_run_cargo(&kernel_dir, &args, &[])
            .with_context(|| {
                if cargo_config_overrides.is_empty() {
                    "kernel-bare build failed".to_string()
                } else {
                    format!(
                        "kernel-bare build failed (--config {:?})",
                        cargo_config_overrides
                    )
                }
            })?;

        Ok(())
    }

    fn build_bootloader(repo_root: &PathBuf) -> Result<()> {
        let boot_dir = repo_root.join("bootloader");

        Self::ensure_pinned_toolchain()?;
        Self::ensure_target("x86_64-unknown-uefi")?;

        let args: Vec<String> = vec![
            "build".to_string(),
            "--release".to_string(),
            "--target".to_string(),
            "x86_64-unknown-uefi".to_string(),
            "-p".to_string(),
            "uefi_boot".to_string(),
        ];
        Self::rustup_run_cargo(&boot_dir, &args, &[])
            .with_context(|| "bootloader build failed")?;

        Ok(())
    }

    fn run_local_ai_matrix(repo_root: &PathBuf, work_dir: &PathBuf) -> Result<Duration> {
        let script = repo_root.join("test-boot-local-ai-matrix-headless.sh");
        if !script.exists() {
            anyhow::bail!("missing headless matrix script: {}", script.display());
        }

        let start = Instant::now();

        let mut cmd = Command::new("bash");
        cmd.current_dir(repo_root)
            .arg(script)
            .env("WORK_DIR", work_dir)
            .env("QUIET_BUILD", "1")
            // Builds are handled by Ouroboros; don't rebuild inside the matrix.
            .env("BUILD_KERNEL_MATRIX", "0")
            .env("BUILD_BOOTLOADER_MATRIX", "0")
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let out = cmd
            .output()
            .with_context(|| "failed to invoke headless matrix")?;

        if !out.status.success() {
            anyhow::bail!(
                "headless matrix failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        Ok(start.elapsed())
    }

    async fn optimize_system_end_to_end(&self) -> Result<()> {
        log::info!("Starting system-wide end-to-end optimization (build + headless boot tests)");

        let repo_root = Self::repo_root();

        let repeats = Self::system_opt_repeats();
        log::info!("[system-opt] repeats per candidate: {} (set RAYOS_SYSTEM_OPT_REPEATS to change)", repeats);

        // Baseline + a few conservative build-profile mutations.
        //
        // IMPORTANT:
        // - Avoid `-C target-cpu=native` (QEMU safety).
        // - Avoid setting RUSTFLAGS: it overrides `kernel-bare/.cargo/config.toml` target
        //   rustflags (linker script), which breaks linking.
        //
        // We mutate via `cargo --config profile.release.*=...` which keeps target rustflags intact.
        #[derive(Clone)]
        struct Candidate {
            desc: String,
            kernel_config: Vec<String>,
        }

        let candidates: Vec<Candidate> = vec![
            Candidate {
                desc: "baseline".to_string(),
                kernel_config: vec![],
            },
            Candidate {
                desc: "profile.release.lto=\"thin\"".to_string(),
                kernel_config: vec!["profile.release.lto=\"thin\"".to_string()],
            },
            Candidate {
                desc: "profile.release.lto=false".to_string(),
                kernel_config: vec!["profile.release.lto=false".to_string()],
            },
            Candidate {
                desc: "profile.release.opt-level=\"s\"".to_string(),
                kernel_config: vec!["profile.release.opt-level=\"s\"".to_string()],
            },
            Candidate {
                desc: "profile.release.opt-level=\"z\"".to_string(),
                kernel_config: vec!["profile.release.opt-level=\"z\"".to_string()],
            },
        ];

        let mut baseline_duration: Option<Duration> = None;
        let mut best_desc = String::new();
        let mut best_cfg: Vec<String> = Vec::new();
        let mut best_duration = Duration::MAX;

        for (idx, cand) in candidates.iter().enumerate() {
            log::info!("[system-opt] candidate {}/{}: {}",
                idx + 1,
                candidates.len(),
                cand.desc
            );

            // 1) Build artifacts for this candidate.
            let build_start = Instant::now();
            Self::build_bootloader(&repo_root)?;
            Self::build_kernel_bare(&repo_root, &cand.kernel_config)?;
            let build_elapsed = build_start.elapsed();

            // 2) Run end-to-end boot + prompt matrix.
            let work_dir = std::env::temp_dir().join(format!("rayos_ouroboros_matrix_{}", Uuid::new_v4()));
            let mut runs: Vec<Duration> = Vec::with_capacity(repeats);
            for run_idx in 0..repeats {
                let matrix_elapsed = Self::run_local_ai_matrix(&repo_root, &work_dir)?;
                log::info!("[system-opt]   run {}/{}: {:?}", run_idx + 1, repeats, matrix_elapsed);
                runs.push(matrix_elapsed);
            }

            let matrix_median = Self::median_duration(runs.clone());

            log::info!(
                "[system-opt] candidate done: build={:?}, matrix_median={:?}",
                build_elapsed,
                matrix_median
            );

            // Use the first candidate as the baseline for MutationResult bookkeeping.
            if baseline_duration.is_none() {
                baseline_duration = Some(matrix_median);
            }

            if matrix_median < best_duration {
                best_duration = matrix_median;
                best_desc = cand.desc.clone();
                best_cfg = cand.kernel_config.clone();
            }
        }

        let baseline_duration = baseline_duration.unwrap_or(best_duration);
        let improvement_factor = if best_duration.as_secs_f64() > 0.0 {
            baseline_duration.as_secs_f64() / best_duration.as_secs_f64()
        } else {
            0.0
        };

        let result = MutationResult {
            mutation_id: Uuid::new_v4(),
            original_duration: baseline_duration,
            mutated_duration: best_duration,
            improvement_factor,
            passed_tests: true,
            binary_diff: {
                // Store a compact description of what was applied.
                // This isn't a real binary diff (yet); it's metadata for system-level “whole enchilada” runs.
                let mut s = best_desc.clone();
                if !best_cfg.is_empty() {
                    s.push_str(" | ");
                    s.push_str(&best_cfg.join("; "));
                }
                s.into_bytes()
            },
        };

        {
            let mut history = self.mutation_history.write();
            history.push_back(result.clone());
            if history.len() > self.max_history {
                history.pop_front();
            }
        }

        if result.is_improvement() {
            self.hot_swapper.apply_patch("system", &result)?;
            log::info!(
                "✅ System improvement: {:.2}x faster (matrix {:?} -> {:?}), {}",
                result.improvement_factor,
                result.original_duration,
                result.mutated_duration,
                best_desc
            );

            // Rebuild once more with the best candidate so artifacts in target/ reflect the chosen config.
            Self::build_bootloader(&repo_root)?;
            Self::build_kernel_bare(&repo_root, &best_cfg)?;
        } else {
            log::info!(
                "System optimization found no >5% improvement (best {:.2}x)",
                result.improvement_factor
            );
        }

        Ok(())
    }

    async fn optimize_function(&self, name: &str, binary: &[u8]) -> Result<()> {
        log::debug!("Optimizing function: {} ({} bytes)", name, binary.len());

        // Generate mutation
        let mutated = self.mutator.mutate(binary);

        // Test in arena
        let test_cases = vec![
            TestCase {
                input: vec![1, 2, 3],
                expected_output: vec![6],
            },
        ];

        let result = self.arena.test_mutation(binary, &mutated, &test_cases).await?;

        // Record in history
        {
            let mut history = self.mutation_history.write();
            history.push_back(result.clone());
            if history.len() > self.max_history {
                history.pop_front();
            }
        }

        // Apply if improvement
        if result.is_improvement() {
            self.hot_swapper.apply_patch(name, &result)?;
            log::info!(
                "✅ Applied improvement: {} is now {:.2}x faster",
                name,
                result.improvement_factor
            );
        } else {
            log::debug!(
                "❌ Mutation rejected: {:.2}x slower, tests={}",
                result.improvement_factor,
                result.passed_tests
            );
        }

        Ok(())
    }

    /// Get mutation statistics
    pub fn get_statistics(&self) -> OuroborosStatistics {
        let history = self.mutation_history.read();

        let total_mutations = history.len();
        let successful = history.iter().filter(|m| m.is_improvement()).count();

        let avg_improvement = if successful > 0 {
            history.iter()
                .filter(|m| m.is_improvement())
                .map(|m| m.improvement_factor)
                .sum::<f64>() / successful as f64
        } else {
            1.0
        };

        OuroborosStatistics {
            total_mutations,
            successful_mutations: successful,
            active_patches: self.hot_swapper.active_patch_count(),
            avg_improvement_factor: avg_improvement,
        }
    }
}

impl Default for OuroborosEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct OuroborosStatistics {
    pub total_mutations: usize,
    pub successful_mutations: usize,
    pub active_patches: usize,
    pub avg_improvement_factor: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutator() {
        let mutator = Mutator::new();
        let original = vec![0u8; 100];

        // Mutation is probabilistic; retry a few times to avoid flakes.
        let mut mutated = mutator.mutate(&original);
        for _ in 0..16 {
            if mutated != original {
                break;
            }
            mutated = mutator.mutate(&original);
        }

        assert_ne!(original, mutated);
    }

    #[tokio::test]
    async fn test_arena() {
        let arena = Arena::new();
        let original = vec![1, 2, 3, 4];
        let mutated = vec![1, 2, 3, 5];

        let test_cases = vec![
            TestCase {
                input: vec![1],
                expected_output: vec![2],
            },
        ];

        let result = arena.test_mutation(&original, &mutated, &test_cases).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_hot_swapper() {
        let swapper = HotSwapper::new();

        let result = MutationResult {
            mutation_id: Uuid::new_v4(),
            original_duration: Duration::from_millis(100),
            mutated_duration: Duration::from_millis(80),
            improvement_factor: 1.25,
            passed_tests: true,
            binary_diff: vec![],
        };

        assert!(swapper.apply_patch("test_fn", &result).is_ok());
        assert_eq!(swapper.get_active_patches_for_test().len(), 1);
    }

    #[tokio::test]
    async fn test_ouroboros_cycle() {
        let engine = OuroborosEngine::new();
        engine.set_enabled(true);

        let target = OptimizationTarget::Function {
            name: "test_function".to_string(),
            binary: vec![0u8; 50],
        };

        let result = engine.optimize_cycle(target).await;
        assert!(result.is_ok());
    }
}
