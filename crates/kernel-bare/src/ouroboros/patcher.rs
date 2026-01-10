//! Live Patcher: Hot-Swap Code Patching Without Reboot
//!
//! This module implements the hot-swap patching system that applies winning mutations
//! directly to live kernel code without requiring a system reboot. It manages patch
//! application, version tracking, and atomic rollback on failure.
//!
//! # Architecture
//!
//! The patching process involves:
//! 1. **Patch Preparation** - Bundle mutations with metadata
//! 2. **Atomicity Guarantee** - Lock critical sections before patching
//! 3. **Code Swap** - Replace old code with new code atomically
//! 4. **Verification** - Verify patch applied correctly
//! 5. **Rollback Log** - Track changes for potential rollback
//!
//! # Boot Markers
//!
//! - `RAYOS_OUROBOROS:PATCHED` - Mutation patch applied live
//! - `RAYOS_OUROBOROS:VERIFIED` - Patch verification passed
//! - `RAYOS_OUROBOROS:ROLLED_BACK` - Patch rollback executed

use crate::ouroboros::{EvolutionResult, Checkpoint, CheckpointData, Checkpointable};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum number of patches in a bundle
pub const MAX_PATCHES_PER_BUNDLE: usize = 32;

/// Maximum patch size (bytes)
pub const MAX_PATCH_SIZE: u32 = 65536; // 64 KB

/// Maximum rollback log entries
pub const MAX_ROLLBACK_ENTRIES: usize = 256;

/// Minimum free memory required for patching (bytes)
pub const MIN_FREE_MEMORY_FOR_PATCH: u64 = 1048576; // 1 MB

/// Patch timeout (milliseconds)
pub const PATCH_TIMEOUT_MS: u32 = 5000;

// ============================================================================
// PATCH OPERATIONS
// ============================================================================

/// A single patch operation (code replacement)
#[derive(Clone, Copy, Debug)]
pub struct PatchOperation {
    /// Unique patch ID
    pub id: u32,
    /// Target memory address
    pub target_address: u64,
    /// Old code (for rollback)
    pub old_code: [u8; 256],
    pub old_code_len: u16,
    /// New code (mutation)
    pub new_code: [u8; 256],
    pub new_code_len: u16,
    /// Status
    pub status: PatchStatus,
    /// Checksum of old code (verification)
    pub old_code_checksum: u32,
    /// Checksum of new code (verification)
    pub new_code_checksum: u32,
}

/// Patch operation status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PatchStatus {
    /// Created, ready to apply
    Created = 0,
    /// Memory locked
    MemoryLocked = 1,
    /// Code swapped
    Applied = 2,
    /// Verification passed
    Verified = 3,
    /// Verification failed
    VerificationFailed = 4,
    /// Rolled back
    RolledBack = 5,
}

impl PatchOperation {
    /// Create a new patch operation
    pub fn new(id: u32, target_address: u64) -> Self {
        Self {
            id,
            target_address,
            old_code: [0u8; 256],
            old_code_len: 0,
            new_code: [0u8; 256],
            new_code_len: 0,
            status: PatchStatus::Created,
            old_code_checksum: 0,
            new_code_checksum: 0,
        }
    }

    /// Set old code
    pub fn set_old_code(&mut self, code: &[u8]) -> Result<(), EvolutionResult> {
        if code.len() > 256 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.old_code[..code.len()].copy_from_slice(code);
        self.old_code_len = code.len() as u16;
        self.old_code_checksum = crc32(code);
        Ok(())
    }

    /// Set new code
    pub fn set_new_code(&mut self, code: &[u8]) -> Result<(), EvolutionResult> {
        if code.len() > 256 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.new_code[..code.len()].copy_from_slice(code);
        self.new_code_len = code.len() as u16;
        self.new_code_checksum = crc32(code);
        Ok(())
    }

    /// Mark as memory locked
    pub fn lock_memory(&mut self) {
        self.status = PatchStatus::MemoryLocked;
    }

    /// Mark as applied
    pub fn mark_applied(&mut self) {
        self.status = PatchStatus::Applied;
    }

    /// Mark as verified
    pub fn mark_verified(&mut self) {
        self.status = PatchStatus::Verified;
    }

    /// Mark as verification failed
    pub fn mark_verification_failed(&mut self) {
        self.status = PatchStatus::VerificationFailed;
    }

    /// Mark as rolled back
    pub fn mark_rolled_back(&mut self) {
        self.status = PatchStatus::RolledBack;
    }

    /// Is patch successfully applied?
    pub fn is_applied(&self) -> bool {
        matches!(self.status, PatchStatus::Verified)
    }
}

// ============================================================================
// PATCH BUNDLE
// ============================================================================

/// Collection of related patches (atomic unit)
#[derive(Clone, Copy, Debug)]
pub struct PatchBundle {
    /// Bundle ID
    pub id: u64,
    /// Bundle status
    pub status: BundleStatus,
    /// Number of patches
    pub patch_count: u32,
    /// Target memory size (total bytes to modify)
    pub total_size: u32,
    /// Creation timestamp
    pub created_at: u64,
    /// Applied timestamp
    pub applied_at: u64,
    /// Bundle name
    pub name: [u8; 64],
    pub name_len: u8,
}

/// Bundle status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BundleStatus {
    /// Created, awaiting patches
    Created = 0,
    /// Ready to apply
    Ready = 1,
    /// Currently applying
    Applying = 2,
    /// Successfully applied
    Applied = 3,
    /// Application failed
    Failed = 4,
    /// Rolled back
    RolledBack = 5,
}

impl PatchBundle {
    /// Create a new patch bundle
    pub fn new(id: u64) -> Self {
        Self {
            id,
            status: BundleStatus::Created,
            patch_count: 0,
            total_size: 0,
            created_at: 0,
            applied_at: 0,
            name: [0u8; 64],
            name_len: 0,
        }
    }

    /// Set bundle name
    pub fn set_name(&mut self, name: &[u8]) -> Result<(), EvolutionResult> {
        if name.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.name[..name.len()].copy_from_slice(name);
        self.name_len = name.len() as u8;
        Ok(())
    }

    /// Add a patch to the bundle
    pub fn add_patch(&mut self, size: u32) -> Result<(), EvolutionResult> {
        if self.patch_count >= MAX_PATCHES_PER_BUNDLE as u32 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        if self.total_size.saturating_add(size) > MAX_PATCH_SIZE {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.patch_count = self.patch_count.saturating_add(1);
        self.total_size = self.total_size.saturating_add(size);
        Ok(())
    }

    /// Mark as ready
    pub fn mark_ready(&mut self) {
        self.status = BundleStatus::Ready;
    }

    /// Mark as applying
    pub fn mark_applying(&mut self) {
        self.status = BundleStatus::Applying;
    }

    /// Mark as applied
    pub fn mark_applied(&mut self, timestamp: u64) {
        self.status = BundleStatus::Applied;
        self.applied_at = timestamp;
    }

    /// Mark as failed
    pub fn mark_failed(&mut self) {
        self.status = BundleStatus::Failed;
    }

    /// Mark as rolled back
    pub fn mark_rolled_back(&mut self) {
        self.status = BundleStatus::RolledBack;
    }

    /// Is bundle ready?
    pub fn is_ready(&self) -> bool {
        self.status == BundleStatus::Ready
    }

    /// Is bundle successfully applied?
    pub fn is_applied(&self) -> bool {
        self.status == BundleStatus::Applied
    }
}

// ============================================================================
// ROLLBACK INFRASTRUCTURE
// ============================================================================

/// Single rollback log entry
#[derive(Clone, Copy, Debug)]
pub struct RollbackEntry {
    /// Entry ID
    pub id: u32,
    /// Bundle that was rolled back
    pub bundle_id: u64,
    /// Timestamp of rollback
    pub timestamp: u64,
    /// Reason for rollback
    pub reason: RollbackReason,
    /// Memory address affected
    pub address: u64,
    /// Data size
    pub size: u32,
}

/// Reasons for rollback
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RollbackReason {
    /// Patch verification failed
    VerificationFailed = 0,
    /// Timeout during application
    Timeout = 1,
    /// Memory error detected
    MemoryError = 2,
    /// Crash detected post-patch
    CrashDetected = 3,
    /// Performance regression
    PerformanceRegression = 4,
    /// Manual rollback
    Manual = 5,
}

impl RollbackEntry {
    /// Create a new rollback entry
    pub fn new(id: u32, bundle_id: u64, reason: RollbackReason, address: u64, size: u32) -> Self {
        Self {
            id,
            bundle_id,
            timestamp: 0,
            reason,
            address,
            size,
        }
    }
}

/// Log of all rollbacks (for audit trail)
#[derive(Clone, Copy, Debug)]
pub struct RollbackLog {
    /// Number of entries
    pub entry_count: u32,
    /// Total successful rollbacks
    pub successful_rollbacks: u32,
    /// Total failed rollback attempts
    pub failed_rollbacks: u32,
    /// Most recent rollback ID
    pub last_rollback_id: u32,
}

impl RollbackLog {
    /// Create a new rollback log
    pub fn new() -> Self {
        Self {
            entry_count: 0,
            successful_rollbacks: 0,
            failed_rollbacks: 0,
            last_rollback_id: 0,
        }
    }

    /// Record rollback attempt
    pub fn record_rollback(&mut self, success: bool) {
        self.entry_count = self.entry_count.saturating_add(1);
        if success {
            self.successful_rollbacks = self.successful_rollbacks.saturating_add(1);
        } else {
            self.failed_rollbacks = self.failed_rollbacks.saturating_add(1);
        }
    }

    /// Get rollback success rate
    pub fn success_rate(&self) -> f32 {
        if self.entry_count == 0 {
            return 0.0;
        }
        (self.successful_rollbacks as f32 / self.entry_count as f32) * 100.0
    }
}

impl Default for RollbackLog {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ATOMIC SWAP
// ============================================================================

/// Atomic memory swap operation (compare-and-swap pattern)
#[derive(Clone, Copy, Debug)]
pub struct AtomicSwap {
    /// Swap ID
    pub id: u64,
    /// Source address
    pub source_address: u64,
    /// Destination address
    pub dest_address: u64,
    /// Size to swap
    pub size: u32,
    /// Expected value (for verification)
    pub expected_value: [u8; 64],
    pub expected_value_len: u8,
    /// Was swap successful?
    pub success: bool,
}

impl AtomicSwap {
    /// Create a new atomic swap
    pub fn new(id: u64, source: u64, dest: u64, size: u32) -> Self {
        Self {
            id,
            source_address: source,
            dest_address: dest,
            size,
            expected_value: [0u8; 64],
            expected_value_len: 0,
            success: false,
        }
    }

    /// Set expected value for verification
    pub fn set_expected_value(&mut self, value: &[u8]) -> Result<(), EvolutionResult> {
        if value.len() > 64 {
            return Err(EvolutionResult::ResourceLimitExceeded);
        }
        self.expected_value[..value.len()].copy_from_slice(value);
        self.expected_value_len = value.len() as u8;
        Ok(())
    }

    /// Mark as successful
    pub fn mark_success(&mut self) {
        self.success = true;
    }

    /// Mark as failed
    pub fn mark_failure(&mut self) {
        self.success = false;
    }
}

// ============================================================================
// VERSION REGISTRY
// ============================================================================

/// Tracks versions of patched code
#[derive(Clone, Copy, Debug)]
pub struct CodeVersion {
    /// Version number (incremented with each patch)
    pub version: u32,
    /// Code hash (SHA256, stored as first 32 bits)
    pub code_hash: u32,
    /// Patch ID that created this version
    pub patch_id: u32,
    /// Timestamp
    pub timestamp: u64,
    /// Is this version stable?
    pub stable: bool,
}

impl CodeVersion {
    /// Create a new code version
    pub fn new(version: u32, code_hash: u32, patch_id: u32) -> Self {
        Self {
            version,
            code_hash,
            patch_id,
            timestamp: 0,
            stable: false,
        }
    }

    /// Mark as stable
    pub fn mark_stable(&mut self) {
        self.stable = true;
    }
}

/// Registry of all code versions
#[derive(Clone, Copy, Debug)]
pub struct VersionRegistry {
    /// Current version
    pub current_version: u32,
    /// Previous stable version
    pub previous_stable_version: u32,
    /// Total versions
    pub total_versions: u32,
    /// Last patch ID applied
    pub last_patch_id: u32,
}

impl VersionRegistry {
    /// Create a new version registry
    pub fn new() -> Self {
        Self {
            current_version: 1,
            previous_stable_version: 0,
            total_versions: 1,
            last_patch_id: 0,
        }
    }

    /// Record new version
    pub fn new_version(&mut self, patch_id: u32) {
        self.previous_stable_version = self.current_version;
        self.current_version = self.current_version.saturating_add(1);
        self.total_versions = self.total_versions.saturating_add(1);
        self.last_patch_id = patch_id;
    }

    /// Rollback to previous version
    pub fn rollback_version(&mut self) {
        self.current_version = self.previous_stable_version;
    }

    /// Get version age (versions since current)
    pub fn version_age(&self) -> u32 {
        self.current_version.saturating_sub(self.previous_stable_version)
    }
}

impl Default for VersionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// LIVE PATCHER
// ============================================================================

/// Main live patching orchestrator
pub struct LivePatcher {
    /// Patcher ID
    pub id: u64,
    /// Total patches applied
    pub patches_applied: u32,
    /// Total patches failed
    pub patches_failed: u32,
    /// Total rollbacks performed
    pub rollbacks_performed: u32,
    /// Version registry
    pub version_registry: VersionRegistry,
    /// Rollback log
    pub rollback_log: RollbackLog,
    /// Is patching enabled?
    pub enabled: bool,
}

impl LivePatcher {
    /// Create a new live patcher
    pub fn new(id: u64) -> Self {
        Self {
            id,
            patches_applied: 0,
            patches_failed: 0,
            rollbacks_performed: 0,
            version_registry: VersionRegistry::new(),
            rollback_log: RollbackLog::new(),
            enabled: true,
        }
    }

    /// Enable/disable patching
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Apply a patch operation
    pub fn apply_patch(&mut self, patch: &PatchOperation) -> Result<(), EvolutionResult> {
        if !self.enabled {
            return Err(EvolutionResult::InternalError);
        }

        // Verify checksums match
        if patch.old_code_checksum != crc32(&patch.old_code[..patch.old_code_len as usize]) {
            return Err(EvolutionResult::RegressionDetected);
        }

        // This is a stub - actual implementation would:
        // 1. Lock memory region
        // 2. Perform atomic swap
        // 3. Flush CPU caches
        // 4. Verify patch applied
        // 5. Update version registry

        self.patches_applied = self.patches_applied.saturating_add(1);
        self.version_registry.new_version(patch.id);

        Ok(())
    }

    /// Rollback a patch
    pub fn rollback_patch(&mut self, _bundle_id: u64, _reason: RollbackReason) -> Result<(), EvolutionResult> {
        if !self.enabled {
            return Err(EvolutionResult::InternalError);
        }

        // This is a stub - actual implementation would:
        // 1. Restore old code from rollback entry
        // 2. Flush CPU caches
        // 3. Verify rollback successful
        // 4. Update version registry

        self.rollbacks_performed = self.rollbacks_performed.saturating_add(1);
        self.rollback_log.record_rollback(true);
        self.version_registry.rollback_version();

        Ok(())
    }

    /// Get success rate
    pub fn success_rate(&self) -> f32 {
        let total = self.patches_applied as f32 + self.patches_failed as f32;
        if total == 0.0 {
            return 0.0;
        }
        (self.patches_applied as f32 / total) * 100.0
    }

    /// Get current code version
    pub fn current_version(&self) -> u32 {
        self.version_registry.current_version
    }
}

impl Checkpointable for LivePatcher {
    fn checkpoint(&self) -> Result<Checkpoint, EvolutionResult> {
        // Store patcher state - use simple binary format instead of format!
        let mut data = CheckpointData::new();
        let mut bytes = [0u8; 32];

        // Encode as 4 u32 values in binary: applied, failed, rollbacks, version
        bytes[0..4].copy_from_slice(&self.patches_applied.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.patches_failed.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.rollbacks_performed.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.version_registry.current_version.to_le_bytes());

        data.set(&bytes[..16])?;
        Ok(Checkpoint {
            id: self.id,
            timestamp: 0,
            data,
        })
    }

    fn restore(&mut self, _checkpoint: &Checkpoint) -> Result<(), EvolutionResult> {
        // Stub implementation - would parse checkpoint data and restore state
        Ok(())
    }
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Simple CRC32 checksum for verification
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;

    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }

    crc ^ 0xFFFFFFFF
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_operation_creation() {
        let po = PatchOperation::new(1, 0x1000);
        assert_eq!(po.id, 1);
        assert_eq!(po.target_address, 0x1000);
        assert_eq!(po.status, PatchStatus::Created);
    }

    #[test]
    fn test_patch_operation_set_code() {
        let mut po = PatchOperation::new(1, 0x1000);
        po.set_old_code(&[0x48, 0x89, 0xC3]).unwrap();
        assert_eq!(po.old_code_len, 3);
        assert!(po.old_code_checksum != 0);
    }

    #[test]
    fn test_patch_operation_status_transitions() {
        let mut po = PatchOperation::new(1, 0x1000);
        assert_eq!(po.status, PatchStatus::Created);

        po.lock_memory();
        assert_eq!(po.status, PatchStatus::MemoryLocked);

        po.mark_applied();
        assert_eq!(po.status, PatchStatus::Applied);

        po.mark_verified();
        assert_eq!(po.status, PatchStatus::Verified);
        assert!(po.is_applied());
    }

    #[test]
    fn test_patch_bundle_creation() {
        let pb = PatchBundle::new(1);
        assert_eq!(pb.id, 1);
        assert_eq!(pb.status, BundleStatus::Created);
        assert_eq!(pb.patch_count, 0);
    }

    #[test]
    fn test_patch_bundle_set_name() {
        let mut pb = PatchBundle::new(1);
        pb.set_name(b"optimization_bundle").unwrap();
        assert_eq!(pb.name_len, 19);
    }

    #[test]
    fn test_patch_bundle_add_patches() {
        let mut pb = PatchBundle::new(1);
        pb.add_patch(128).unwrap();
        pb.add_patch(256).unwrap();
        assert_eq!(pb.patch_count, 2);
        assert_eq!(pb.total_size, 384);
    }

    #[test]
    fn test_patch_bundle_status_transitions() {
        let mut pb = PatchBundle::new(1);
        pb.mark_ready();
        assert!(pb.is_ready());

        pb.mark_applying();
        assert_eq!(pb.status, BundleStatus::Applying);

        pb.mark_applied(1000);
        assert!(pb.is_applied());
        assert_eq!(pb.applied_at, 1000);
    }

    #[test]
    fn test_rollback_entry_creation() {
        let re = RollbackEntry::new(1, 1, RollbackReason::VerificationFailed, 0x1000, 256);
        assert_eq!(re.id, 1);
        assert_eq!(re.bundle_id, 1);
        assert_eq!(re.reason, RollbackReason::VerificationFailed);
    }

    #[test]
    fn test_rollback_log() {
        let mut log = RollbackLog::new();
        assert_eq!(log.entry_count, 0);

        log.record_rollback(true);
        log.record_rollback(true);
        log.record_rollback(false);

        assert_eq!(log.entry_count, 3);
        assert_eq!(log.successful_rollbacks, 2);
        assert_eq!(log.failed_rollbacks, 1);
        assert!((log.success_rate() - 66.66).abs() < 1.0);
    }

    #[test]
    fn test_atomic_swap_creation() {
        let swap = AtomicSwap::new(1, 0x1000, 0x2000, 256);
        assert_eq!(swap.id, 1);
        assert_eq!(swap.source_address, 0x1000);
        assert_eq!(swap.dest_address, 0x2000);
        assert!(!swap.success);
    }

    #[test]
    fn test_atomic_swap_expected_value() {
        let mut swap = AtomicSwap::new(1, 0x1000, 0x2000, 256);
        swap.set_expected_value(&[0xAA, 0xBB]).unwrap();
        assert_eq!(swap.expected_value_len, 2);
    }

    #[test]
    fn test_atomic_swap_success() {
        let mut swap = AtomicSwap::new(1, 0x1000, 0x2000, 256);
        assert!(!swap.success);
        swap.mark_success();
        assert!(swap.success);
    }

    #[test]
    fn test_code_version_creation() {
        let cv = CodeVersion::new(1, 0x12345678, 100);
        assert_eq!(cv.version, 1);
        assert_eq!(cv.code_hash, 0x12345678);
        assert_eq!(cv.patch_id, 100);
        assert!(!cv.stable);
    }

    #[test]
    fn test_code_version_stable() {
        let mut cv = CodeVersion::new(1, 0x12345678, 100);
        cv.mark_stable();
        assert!(cv.stable);
    }

    #[test]
    fn test_version_registry_creation() {
        let vr = VersionRegistry::new();
        assert_eq!(vr.current_version, 1);
        assert_eq!(vr.previous_stable_version, 0);
        assert_eq!(vr.total_versions, 1);
    }

    #[test]
    fn test_version_registry_new_version() {
        let mut vr = VersionRegistry::new();
        vr.new_version(1);
        assert_eq!(vr.current_version, 2);
        assert_eq!(vr.previous_stable_version, 1);
        assert_eq!(vr.total_versions, 2);
    }

    #[test]
    fn test_version_registry_rollback() {
        let mut vr = VersionRegistry::new();
        vr.new_version(1);
        vr.new_version(2);
        assert_eq!(vr.current_version, 3);

        vr.rollback_version();
        assert_eq!(vr.current_version, 2);
    }

    #[test]
    fn test_version_registry_age() {
        let mut vr = VersionRegistry::new();
        assert_eq!(vr.version_age(), 1);

        vr.new_version(1);
        assert_eq!(vr.version_age(), 1);

        vr.new_version(2);
        assert_eq!(vr.version_age(), 1);
    }

    #[test]
    fn test_live_patcher_creation() {
        let patcher = LivePatcher::new(1);
        assert_eq!(patcher.id, 1);
        assert_eq!(patcher.patches_applied, 0);
        assert!(patcher.enabled);
    }

    #[test]
    fn test_live_patcher_enable_disable() {
        let mut patcher = LivePatcher::new(1);
        patcher.set_enabled(false);
        assert!(!patcher.enabled);
        patcher.set_enabled(true);
        assert!(patcher.enabled);
    }

    #[test]
    fn test_live_patcher_apply_patch() {
        let mut patcher = LivePatcher::new(1);
        let mut patch = PatchOperation::new(1, 0x1000);
        patch.set_old_code(&[0x48, 0x89, 0xC3]).unwrap();
        patch.set_new_code(&[0x49, 0x89, 0xC3]).unwrap();

        patcher.apply_patch(&patch).unwrap();
        assert_eq!(patcher.patches_applied, 1);
        assert_eq!(patcher.current_version(), 2);
    }

    #[test]
    fn test_live_patcher_disabled() {
        let mut patcher = LivePatcher::new(1);
        patcher.set_enabled(false);

        let patch = PatchOperation::new(1, 0x1000);
        let result = patcher.apply_patch(&patch);
        assert!(result.is_err());
    }

    #[test]
    fn test_live_patcher_rollback() {
        let mut patcher = LivePatcher::new(1);

        // Apply a patch first
        let mut patch = PatchOperation::new(1, 0x1000);
        patch.set_old_code(&[0x48, 0x89, 0xC3]).unwrap();
        patch.set_new_code(&[0x49, 0x89, 0xC3]).unwrap();
        patcher.apply_patch(&patch).unwrap();

        // Rollback
        patcher.rollback_patch(1, RollbackReason::VerificationFailed).unwrap();
        assert_eq!(patcher.rollbacks_performed, 1);
        assert_eq!(patcher.current_version(), 1);
    }

    #[test]
    fn test_live_patcher_success_rate() {
        let mut patcher = LivePatcher::new(1);
        patcher.patches_applied = 75;
        patcher.patches_failed = 25;
        assert!((patcher.success_rate() - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_crc32_checksum() {
        let data1 = [0x48, 0x89, 0xC3];
        let data2 = [0x48, 0x89, 0xC3];
        let data3 = [0x49, 0x89, 0xC3];

        let crc1 = crc32(&data1);
        let crc2 = crc32(&data2);
        let crc3 = crc32(&data3);

        assert_eq!(crc1, crc2);
        assert_ne!(crc1, crc3);
    }

    #[test]
    fn test_live_patcher_checkpoint() {
        let patcher = LivePatcher::new(1);
        let checkpoint = patcher.checkpoint().unwrap();
        assert!(!checkpoint.as_slice().is_empty());
    }
}
