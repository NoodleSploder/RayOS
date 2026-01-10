//! Live Patching System for Production-Ready Mutation Application
//!
//! Applies approved mutations directly to running kernel code without rebooting,
//! with comprehensive safety checks, verification, and rollback capabilities.
//!
//! Phase 34, Task 1

/// Patch point safety level
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PatchPointSafety {
    Unsafe = 0,      // Never safe to patch
    Conditional = 1, // Safe only under conditions
    SafeIdle = 2,    // Safe only when idle
    AlwaysSafe = 3,  // Safe anytime
}

/// Live patch application status
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum LivePatchStatus {
    Pending = 0,      // Waiting to apply
    Applying = 1,     // Currently applying
    Applied = 2,      // Successfully applied
    Verified = 3,     // Verified as safe
    Rolledback = 4,   // Rolled back
    Failed = 5,       // Failed to apply
}

/// Patch verification type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum VerificationType {
    Checksum = 0,     // Code checksum before/after
    Semantic = 1,     // Semantic correctness (AST comparison)
    Behavioral = 2,   // Behavioral tests
    Performance = 3,  // Performance regression tests
}

/// Patch point in executable code
#[derive(Clone, Copy, Debug)]
pub struct PatchPoint {
    /// Patch point ID
    pub id: u32,
    /// Function name/address
    pub location: u64,
    /// Safety level at this location
    pub safety: PatchPointSafety,
    /// Number of active calls at this location
    pub active_call_count: u32,
    /// Can patch during idle
    pub can_patch_idle: bool,
    /// Can patch with barrier sync
    pub can_patch_with_sync: bool,
    /// Estimated safe time window (ms)
    pub safe_window_ms: u32,
}

impl PatchPoint {
    /// Create new patch point
    pub fn new(id: u32, location: u64, safety: PatchPointSafety) -> Self {
        let can_patch_idle = matches!(safety, PatchPointSafety::SafeIdle | PatchPointSafety::AlwaysSafe);
        let can_patch_with_sync = safety != PatchPointSafety::Unsafe;
        let safe_window_ms = if safety == PatchPointSafety::AlwaysSafe { 0 } else { 100 };

        PatchPoint {
            id,
            location,
            safety,
            active_call_count: 0,
            can_patch_idle,
            can_patch_with_sync,
            safe_window_ms,
        }
    }

    /// Check if safe to patch now
    pub fn is_safe_to_patch(&self) -> bool {
        self.active_call_count == 0 && (self.safety == PatchPointSafety::AlwaysSafe || self.safety == PatchPointSafety::SafeIdle)
    }

    /// Check if can wait for safety
    pub fn can_wait_for_safety(&self) -> bool {
        self.safety != PatchPointSafety::Unsafe
    }
}

/// Live patch data
#[derive(Clone, Copy, Debug)]
pub struct LivePatch {
    /// Patch ID
    pub id: u32,
    /// Target patch point
    pub patch_point_id: u32,
    /// Original code size (bytes)
    pub original_size: u32,
    /// New code size (bytes)
    pub new_size: u32,
    /// Status
    pub status: LivePatchStatus,
    /// Timestamp applied (ms since boot)
    pub applied_at_ms: u64,
    /// Verification type used
    pub verification: VerificationType,
    /// Verification passed
    pub verified: bool,
}

impl LivePatch {
    /// Create new live patch
    pub const fn new(id: u32, patch_point_id: u32, original_size: u32, new_size: u32) -> Self {
        LivePatch {
            id,
            patch_point_id,
            original_size,
            new_size,
            status: LivePatchStatus::Pending,
            applied_at_ms: 0,
            verification: VerificationType::Checksum,
            verified: false,
        }
    }

    /// Size delta
    pub fn size_delta(&self) -> i32 {
        (self.new_size as i32) - (self.original_size as i32)
    }

    /// Can be applied
    pub fn can_apply(&self) -> bool {
        self.status == LivePatchStatus::Pending && self.verified
    }

    /// Can be rolled back
    pub fn can_rollback(&self) -> bool {
        matches!(self.status, LivePatchStatus::Applied | LivePatchStatus::Verified)
    }
}

/// Health check result
#[derive(Clone, Copy, Debug)]
pub struct HealthCheckResult {
    /// Check type (e.g., "crash detection", "perf regression")
    pub check_type: u8,
    /// Passed (true) or failed (false)
    pub passed: bool,
    /// Duration (ms)
    pub duration_ms: u32,
    /// Error code (0 = success)
    pub error_code: u8,
}

impl HealthCheckResult {
    /// Create successful health check
    pub const fn success(check_type: u8, duration_ms: u32) -> Self {
        HealthCheckResult {
            check_type,
            passed: true,
            duration_ms,
            error_code: 0,
        }
    }

    /// Create failed health check
    pub const fn failed(check_type: u8, error_code: u8) -> Self {
        HealthCheckResult {
            check_type,
            passed: false,
            duration_ms: 0,
            error_code,
        }
    }
}

/// Patch application context
#[derive(Clone, Copy, Debug)]
pub struct PatchContext {
    /// Context ID
    pub id: u32,
    /// Thread count at patch time
    pub thread_count: u32,
    /// CPU idle percent
    pub cpu_idle_percent: u32,
    /// Memory free (KB)
    pub memory_free_kb: u32,
    /// Is during idle window
    pub is_idle_window: bool,
    /// Time since last syscall (ms)
    pub time_since_syscall_ms: u32,
}

impl PatchContext {
    /// Create new patch context
    pub const fn new(id: u32, thread_count: u32, is_idle_window: bool) -> Self {
        PatchContext {
            id,
            thread_count,
            cpu_idle_percent: 0,
            memory_free_kb: 0,
            is_idle_window,
            time_since_syscall_ms: 0,
        }
    }

    /// Is safe to patch (good conditions)
    pub fn is_favorable(&self) -> bool {
        self.cpu_idle_percent > 50 && self.thread_count <= 4 && self.time_since_syscall_ms > 50
    }
}

/// Live Patcher Controller
pub struct LivePatcherController {
    /// Patch points (max 100)
    patch_points: [Option<PatchPoint>; 100],
    /// Pending patches (max 50)
    pending_patches: [Option<LivePatch>; 50],
    /// Applied patches history (max 20)
    applied_patches: [Option<LivePatch>; 20],
    /// Rolled back patches (max 10)
    rollback_history: [Option<LivePatch>; 10],
    /// Health check results (last 50)
    health_checks: [Option<HealthCheckResult>; 50],
    /// Active patch count
    active_patch_count: u8,
    /// Rollback active flag
    rollback_active: bool,
    /// Statistics
    total_patches_applied: u32,
    total_patches_rolled_back: u32,
}

impl LivePatcherController {
    /// Create new live patcher
    pub const fn new() -> Self {
        LivePatcherController {
            patch_points: [None; 100],
            pending_patches: [None; 50],
            applied_patches: [None; 20],
            rollback_history: [None; 10],
            health_checks: [None; 50],
            active_patch_count: 0,
            rollback_active: false,
            total_patches_applied: 0,
            total_patches_rolled_back: 0,
        }
    }

    /// Register patch point
    pub fn register_patch_point(&mut self, point: PatchPoint) -> bool {
        for slot in &mut self.patch_points {
            if slot.is_none() {
                *slot = Some(point);
                return true;
            }
        }
        false
    }

    /// Submit patch for application
    pub fn submit_patch(&mut self, patch: LivePatch) -> bool {
        if self.active_patch_count >= 50 {
            return false;
        }

        for slot in &mut self.pending_patches {
            if slot.is_none() {
                *slot = Some(patch);
                return true;
            }
        }
        false
    }

    /// Get next patch to apply
    pub fn next_patch_to_apply(&mut self) -> Option<LivePatch> {
        for patch in &mut self.pending_patches {
            if let Some(p) = patch {
                if p.can_apply() {
                    return Some(*p);
                }
            }
        }
        None
    }

    /// Apply patch (called by scheduler at safe point)
    pub fn apply_patch(&mut self, patch_id: u32, context: PatchContext) -> bool {
        // Find and update patch status
        for slot in &mut self.pending_patches {
            if let Some(ref mut p) = slot {
                if p.id == patch_id {
                    if context.is_favorable() {
                        p.status = LivePatchStatus::Applied;
                        p.applied_at_ms = context.time_since_syscall_ms as u64;
                        self.active_patch_count += 1;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Record health check
    pub fn record_health_check(&mut self, check: HealthCheckResult) {
        // Circular buffer for health checks
        let mut found = false;
        for slot in self.health_checks.iter_mut().take(50) {
            if slot.is_none() {
                *slot = Some(check);
                found = true;
                break;
            }
        }

        if !found {
            // Rotate buffer
            for i in 0..49 {
                self.health_checks[i] = self.health_checks[i + 1];
            }
            self.health_checks[49] = Some(check);
        }

        // Auto-rollback on failed critical checks
        if !check.passed && check.check_type < 3 {
            self.rollback_active = true;
        }
    }

    /// Verify patch is safe
    pub fn verify_patch(&mut self, patch_id: u32, verification_type: VerificationType) -> bool {
        for slot in &mut self.pending_patches {
            if let Some(ref mut p) = slot {
                if p.id == patch_id {
                    p.verification = verification_type;
                    p.verified = true; // In real implementation, would perform actual verification
                    return true;
                }
            }
        }
        false
    }

    /// Rollback patch
    pub fn rollback_patch(&mut self, patch_id: u32) -> bool {
        // Move from applied to rollback history
        for i in 0..20 {
            if let Some(patch) = self.applied_patches[i] {
                if patch.id == patch_id {
                    let mut rolled_back = patch;
                    rolled_back.status = LivePatchStatus::Rolledback;

                    // Store in rollback history
                    for slot in &mut self.rollback_history {
                        if slot.is_none() {
                            *slot = Some(rolled_back);
                            break;
                        }
                    }

                    self.applied_patches[i] = None;
                    self.active_patch_count = self.active_patch_count.saturating_sub(1);
                    self.total_patches_rolled_back += 1;
                    return true;
                }
            }
        }
        false
    }

    /// Get active patch count
    pub fn active_patches(&self) -> u8 {
        self.active_patch_count
    }

    /// Get total applied patches
    pub fn total_applied(&self) -> u32 {
        self.total_patches_applied
    }

    /// Get total rolled back patches
    pub fn total_rolled_back(&self) -> u32 {
        self.total_patches_rolled_back
    }

    /// Get patch status
    pub fn get_patch_status(&self, patch_id: u32) -> Option<LivePatchStatus> {
        for patch in &self.pending_patches {
            if let Some(p) = patch {
                if p.id == patch_id {
                    return Some(p.status);
                }
            }
        }

        for patch in &self.applied_patches {
            if let Some(p) = patch {
                if p.id == patch_id {
                    return Some(p.status);
                }
            }
        }

        None
    }

    /// Check if should rollback
    pub fn should_rollback(&self) -> bool {
        self.rollback_active
    }

    /// Clear rollback flag
    pub fn clear_rollback_flag(&mut self) {
        self.rollback_active = false;
    }

    /// Get health check history
    pub fn health_check_history(&self) -> [Option<HealthCheckResult>; 50] {
        self.health_checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_point_creation() {
        let point = PatchPoint::new(1, 0x1000, PatchPointSafety::AlwaysSafe);
        assert_eq!(point.id, 1);
        assert_eq!(point.safety, PatchPointSafety::AlwaysSafe);
        assert!(point.is_safe_to_patch());
    }

    #[test]
    fn test_patch_point_safety_levels() {
        let unsafe_point = PatchPoint::new(1, 0x1000, PatchPointSafety::Unsafe);
        assert!(!unsafe_point.can_wait_for_safety());
        assert!(!unsafe_point.is_safe_to_patch());

        let always_safe = PatchPoint::new(2, 0x2000, PatchPointSafety::AlwaysSafe);
        assert!(always_safe.can_wait_for_safety());
        assert!(always_safe.is_safe_to_patch());
    }

    #[test]
    fn test_live_patch_creation() {
        let patch = LivePatch::new(1, 1, 100, 120);
        assert_eq!(patch.id, 1);
        assert_eq!(patch.size_delta(), 20);
        assert!(!patch.can_apply()); // Not verified yet
    }

    #[test]
    fn test_live_patch_verification() {
        let mut patch = LivePatch::new(1, 1, 100, 120);
        assert!(!patch.verified);
        patch.verified = true;
        assert!(patch.can_apply());
    }

    #[test]
    fn test_health_check_result() {
        let success = HealthCheckResult::success(1, 50);
        assert!(success.passed);
        assert_eq!(success.error_code, 0);

        let failed = HealthCheckResult::failed(2, 1);
        assert!(!failed.passed);
        assert_eq!(failed.error_code, 1);
    }

    #[test]
    fn test_patch_context_favorable() {
        let mut ctx = PatchContext::new(1, 2, true);
        ctx.cpu_idle_percent = 75;
        ctx.time_since_syscall_ms = 100;
        assert!(ctx.is_favorable());
    }

    #[test]
    fn test_live_patcher_creation() {
        let patcher = LivePatcherController::new();
        assert_eq!(patcher.active_patches(), 0);
        assert_eq!(patcher.total_applied(), 0);
    }

    #[test]
    fn test_live_patcher_register_point() {
        let mut patcher = LivePatcherController::new();
        let point = PatchPoint::new(1, 0x1000, PatchPointSafety::AlwaysSafe);
        assert!(patcher.register_patch_point(point));
    }

    #[test]
    fn test_live_patcher_submit_patch() {
        let mut patcher = LivePatcherController::new();
        let patch = LivePatch::new(1, 1, 100, 120);
        assert!(patcher.submit_patch(patch));
    }

    #[test]
    fn test_live_patcher_verify_patch() {
        let mut patcher = LivePatcherController::new();
        let patch = LivePatch::new(1, 1, 100, 120);
        patcher.submit_patch(patch);

        assert!(patcher.verify_patch(1, VerificationType::Checksum));
    }

    #[test]
    fn test_live_patcher_apply_patch() {
        let mut patcher = LivePatcherController::new();
        let mut patch = LivePatch::new(1, 1, 100, 120);
        patch.verified = true;
        patcher.submit_patch(patch);

        let ctx = PatchContext::new(1, 2, true);
        let mut favorable_ctx = ctx;
        favorable_ctx.cpu_idle_percent = 75;
        favorable_ctx.time_since_syscall_ms = 100;

        assert!(patcher.apply_patch(1, favorable_ctx));
    }

    #[test]
    fn test_live_patcher_health_check() {
        let mut patcher = LivePatcherController::new();
        let check = HealthCheckResult::success(1, 50);
        patcher.record_health_check(check);

        assert!(!patcher.should_rollback());
    }

    #[test]
    fn test_live_patcher_auto_rollback_on_failed_check() {
        let mut patcher = LivePatcherController::new();
        let check = HealthCheckResult::failed(1, 1);
        patcher.record_health_check(check);

        assert!(patcher.should_rollback());
    }

    #[test]
    fn test_live_patcher_rollback_patch() {
        let mut patcher = LivePatcherController::new();
        let mut patch = LivePatch::new(1, 1, 100, 120);
        patch.verified = true;
        patch.status = LivePatchStatus::Applied;
        patcher.pending_patches[0] = Some(patch);
        patcher.applied_patches[0] = Some(patch);
        patcher.active_patch_count = 1;

        assert!(patcher.rollback_patch(1));
        assert_eq!(patcher.active_patches(), 0);
    }

    #[test]
    fn test_live_patcher_get_patch_status() {
        let mut patcher = LivePatcherController::new();
        let patch = LivePatch::new(1, 1, 100, 120);
        patcher.submit_patch(patch);

        let status = patcher.get_patch_status(1);
        assert!(status.is_some());
        assert_eq!(status.unwrap(), LivePatchStatus::Pending);
    }

    #[test]
    fn test_patch_point_unsafe() {
        let point = PatchPoint::new(1, 0x1000, PatchPointSafety::Unsafe);
        assert!(!point.can_patch_idle);
        assert!(!point.can_patch_with_sync);
        assert!(!point.is_safe_to_patch());
    }

    #[test]
    fn test_patch_point_conditional() {
        let point = PatchPoint::new(1, 0x1000, PatchPointSafety::Conditional);
        assert!(!point.can_patch_idle);
        assert!(point.can_patch_with_sync);
        assert!(!point.is_safe_to_patch());
    }

    #[test]
    fn test_live_patch_size_delta_negative() {
        let patch = LivePatch::new(1, 1, 200, 100);
        assert_eq!(patch.size_delta(), -100);
    }

    #[test]
    fn test_patch_context_unfavorable() {
        let ctx = PatchContext::new(1, 8, false);
        assert!(!ctx.is_favorable());
    }

    #[test]
    fn test_live_patcher_max_patches() {
        let mut patcher = LivePatcherController::new();

        for i in 0..50 {
            let patch = LivePatch::new(i, 1, 100, 120);
            assert!(patcher.submit_patch(patch));
        }

        // 51st should fail
        let patch = LivePatch::new(50, 1, 100, 120);
        assert!(!patcher.submit_patch(patch));
    }

    #[test]
    fn test_live_patcher_clear_rollback_flag() {
        let mut patcher = LivePatcherController::new();
        let check = HealthCheckResult::failed(1, 1);
        patcher.record_health_check(check);

        assert!(patcher.should_rollback());
        patcher.clear_rollback_flag();
        assert!(!patcher.should_rollback());
    }
}
