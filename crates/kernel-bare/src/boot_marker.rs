//! RayOS Boot Markers & Recovery Policy
//!
//! Tracks kernel boot progress through deterministic markers and implements
//! golden state recovery for automatic fallback when boot fails.
//!
//! **Design**: Boot progresses through stages (Kernel_Loaded → Subsystems_Ready → Shell_Ready → Golden).
//! Each stage is marked. Three consecutive failures at any stage trigger recovery to the last golden state.

use core::cmp::min;

/// Boot marker stage
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BootMarkerStage {
    /// UEFI bootloader has loaded kernel
    KernelLoaded = 0,
    /// Kernel has initialized paging and memory
    MemoryReady = 1,
    /// Kernel has initialized all subsystems
    SubsystemsReady = 2,
    /// Interactive shell is ready
    ShellReady = 3,
    /// System has been running stably for boot timeout duration
    Golden = 4,
}

impl BootMarkerStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            BootMarkerStage::KernelLoaded => "KERNEL_LOADED",
            BootMarkerStage::MemoryReady => "MEMORY_READY",
            BootMarkerStage::SubsystemsReady => "SUBSYSTEMS_READY",
            BootMarkerStage::ShellReady => "SHELL_READY",
            BootMarkerStage::Golden => "GOLDEN",
        }
    }
}

/// Boot marker entry
#[derive(Clone, Copy)]
pub struct BootMarker {
    /// Stage reached
    pub stage: BootMarkerStage,
    /// Timestamp (boot-relative milliseconds)
    pub timestamp: u64,
    /// Sequential marker number
    pub sequence: u32,
    /// Stable hash of kernel state at this marker
    pub kernel_hash: u32,
}

impl BootMarker {
    pub fn new(stage: BootMarkerStage, timestamp: u64, sequence: u32, kernel_hash: u32) -> Self {
        BootMarker {
            stage,
            timestamp,
            sequence,
            kernel_hash,
        }
    }
}

/// Maximum boot markers to store
const MAX_BOOT_MARKERS: usize = 100;

/// Boot marker storage
pub struct MarkerStorage {
    /// Array of markers
    markers: [Option<BootMarker>; MAX_BOOT_MARKERS],
    /// Number of markers
    marker_count: u32,
    /// Current stage
    current_stage: BootMarkerStage,
    /// Number of times current stage has been reached
    stage_attempts: u32,
    /// Maximum attempts before recovery
    max_attempts_per_stage: u32,
}

impl MarkerStorage {
    pub fn new() -> Self {
        MarkerStorage {
            markers: [None; MAX_BOOT_MARKERS],
            marker_count: 0,
            current_stage: BootMarkerStage::KernelLoaded,
            stage_attempts: 0,
            max_attempts_per_stage: 3,
        }
    }

    /// Record a boot marker
    pub fn record(&mut self, stage: BootMarkerStage, timestamp: u64, kernel_hash: u32) -> Result<u32, &'static str> {
        if self.marker_count >= (MAX_BOOT_MARKERS as u32) {
            // Rotate: remove oldest marker
            for i in 0..(MAX_BOOT_MARKERS - 1) {
                self.markers[i] = self.markers[i + 1];
            }
            self.marker_count = (MAX_BOOT_MARKERS - 1) as u32;
        }

        let marker = BootMarker::new(stage, timestamp, self.marker_count, kernel_hash);
        self.markers[self.marker_count as usize] = Some(marker);
        self.marker_count += 1;

        // Update current stage
        if stage as u32 > self.current_stage as u32 {
            self.current_stage = stage;
            self.stage_attempts = 0; // Reset attempts on advancement
        } else if stage == self.current_stage {
            self.stage_attempts = self.stage_attempts.saturating_add(1);
        }

        Ok(self.marker_count - 1)
    }

    /// Get last marker at or before given stage
    pub fn get_marker_at_stage(&self, stage: BootMarkerStage) -> Option<BootMarker> {
        for i in (0..self.marker_count as usize).rev() {
            if let Some(marker) = self.markers[i] {
                if marker.stage <= stage {
                    return Some(marker);
                }
            }
        }
        None
    }

    /// Get latest marker
    pub fn latest(&self) -> Option<BootMarker> {
        if self.marker_count > 0 {
            self.markers[(self.marker_count - 1) as usize]
        } else {
            None
        }
    }

    /// Get current boot stage
    pub fn current_stage(&self) -> BootMarkerStage {
        self.current_stage
    }

    /// Get number of attempts at current stage
    pub fn stage_attempts(&self) -> u32 {
        self.stage_attempts
    }

    /// Check if stage is failing repeatedly
    pub fn is_stage_stuck(&self) -> bool {
        self.stage_attempts >= self.max_attempts_per_stage
    }

    /// Clear all markers
    pub fn clear(&mut self) {
        self.marker_count = 0;
        self.current_stage = BootMarkerStage::KernelLoaded;
        self.stage_attempts = 0;
    }

    /// Get total markers recorded
    pub fn total(&self) -> u32 {
        self.marker_count
    }
}

/// Recovery snapshot of a known-good golden state
#[derive(Clone, Copy)]
pub struct RecoverySnapshot {
    /// Boot stage this snapshot represents
    pub stage: BootMarkerStage,
    /// Timestamp of snapshot
    pub timestamp: u64,
    /// Kernel version/hash
    pub kernel_hash: u32,
    /// Partition LBA for kernel image
    pub kernel_lba: u64,
    /// Is this snapshot valid
    pub is_valid: bool,
}

impl RecoverySnapshot {
    pub fn new(stage: BootMarkerStage, kernel_hash: u32, kernel_lba: u64) -> Self {
        RecoverySnapshot {
            stage,
            timestamp: 0,
            kernel_hash,
            kernel_lba,
            is_valid: false,
        }
    }

    /// Validate snapshot (mark as golden)
    pub fn validate(&mut self) {
        self.is_valid = true;
    }
}

/// Recovery policy state machine
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryState {
    /// No recovery needed
    Idle = 0,
    /// Detecting failure
    Detecting = 1,
    /// Preparing recovery
    Preparing = 2,
    /// Loading golden state
    Loading = 3,
    /// Booting recovered kernel
    Booting = 4,
    /// Recovery succeeded
    Succeeded = 5,
    /// Recovery failed (gave up)
    Failed = 6,
}

/// Maximum recovery snapshots
const MAX_RECOVERY_SNAPSHOTS: usize = 5;

/// Recovery policy engine
pub struct RecoveryPolicy {
    /// Current recovery state
    state: RecoveryState,
    /// Boot marker storage
    markers: MarkerStorage,
    /// Recovery snapshots (multiple fallback levels)
    snapshots: [Option<RecoverySnapshot>; MAX_RECOVERY_SNAPSHOTS],
    /// Number of snapshots
    snapshot_count: u32,
    /// Failure counter
    failure_count: u32,
    /// Maximum failures before giving up
    max_failures: u32,
    /// Current snapshot being recovered
    current_snapshot_index: Option<u32>,
}

impl RecoveryPolicy {
    pub fn new() -> Self {
        RecoveryPolicy {
            state: RecoveryState::Idle,
            markers: MarkerStorage::new(),
            snapshots: [None; MAX_RECOVERY_SNAPSHOTS],
            snapshot_count: 0,
            failure_count: 0,
            max_failures: 3,
            current_snapshot_index: None,
        }
    }

    /// Record boot marker
    pub fn record_marker(&mut self, stage: BootMarkerStage, timestamp: u64, kernel_hash: u32) -> Result<(), &'static str> {
        self.markers.record(stage, timestamp, kernel_hash)?;

        // Auto-promote to golden after Shell_Ready
        if stage == BootMarkerStage::ShellReady && self.snapshot_count == 0 {
            let snapshot = RecoverySnapshot::new(stage, kernel_hash, 0x2000);
            self.add_snapshot(snapshot)?;
        }

        Ok(())
    }

    /// Add a recovery snapshot
    pub fn add_snapshot(&mut self, mut snapshot: RecoverySnapshot) -> Result<u32, &'static str> {
        if self.snapshot_count >= (MAX_RECOVERY_SNAPSHOTS as u32) {
            return Err("Max snapshots reached");
        }

        snapshot.validate();
        self.snapshots[self.snapshot_count as usize] = Some(snapshot);
        self.snapshot_count += 1;
        Ok(self.snapshot_count - 1)
    }

    /// Detect failure and prepare recovery
    pub fn detect_failure(&mut self) -> Result<(), &'static str> {
        if self.markers.is_stage_stuck() {
            self.state = RecoveryState::Detecting;
            self.failure_count = self.failure_count.saturating_add(1);

            if self.failure_count >= self.max_failures {
                self.state = RecoveryState::Failed;
                return Err("Max failures reached, recovery abandoned");
            }

            return Ok(());
        }

        Ok(())
    }

    /// Execute recovery: load last golden snapshot
    pub fn attempt_recovery(&mut self) -> Result<RecoverySnapshot, &'static str> {
        if self.snapshot_count == 0 {
            self.state = RecoveryState::Failed;
            return Err("No recovery snapshot available");
        }

        self.state = RecoveryState::Loading;

        // Get most recent snapshot
        if let Some(snapshot) = self.snapshots[(self.snapshot_count - 1) as usize] {
            if snapshot.is_valid {
                self.current_snapshot_index = Some(self.snapshot_count - 1);
                self.state = RecoveryState::Booting;
                return Ok(snapshot);
            }
        }

        self.state = RecoveryState::Failed;
        Err("No valid recovery snapshot")
    }

    /// Report recovery success
    pub fn recovery_succeeded(&mut self) {
        self.state = RecoveryState::Succeeded;
        self.failure_count = 0;
        self.markers.clear();
    }

    /// Report recovery failure
    pub fn recovery_failed(&mut self) {
        self.state = RecoveryState::Failed;
    }

    /// Get current recovery state
    pub fn state(&self) -> RecoveryState {
        self.state
    }

    /// Get latest boot marker
    pub fn latest_marker(&self) -> Option<BootMarker> {
        self.markers.latest()
    }

    /// Get current boot stage
    pub fn current_stage(&self) -> BootMarkerStage {
        self.markers.current_stage()
    }

    /// Get failure count
    pub fn failure_count(&self) -> u32 {
        self.failure_count
    }

    /// Check if system is in recovery
    pub fn in_recovery(&self) -> bool {
        self.state != RecoveryState::Idle && self.state != RecoveryState::Succeeded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_marker() {
        let marker = BootMarker::new(BootMarkerStage::KernelLoaded, 100, 0, 0x12345678);
        assert_eq!(marker.stage, BootMarkerStage::KernelLoaded);
    }

    #[test]
    fn test_marker_storage() {
        let mut storage = MarkerStorage::new();

        storage.record(BootMarkerStage::KernelLoaded, 100, 0x11111111).unwrap();
        storage.record(BootMarkerStage::MemoryReady, 200, 0x22222222).unwrap();
        storage.record(BootMarkerStage::SubsystemsReady, 300, 0x33333333).unwrap();

        assert_eq!(storage.total(), 3);
        assert_eq!(storage.current_stage(), BootMarkerStage::SubsystemsReady);
    }

    #[test]
    fn test_recovery_snapshot() {
        let mut snapshot = RecoverySnapshot::new(BootMarkerStage::ShellReady, 0x44444444, 0x2000);
        assert!(!snapshot.is_valid);

        snapshot.validate();
        assert!(snapshot.is_valid);
    }

    #[test]
    fn test_recovery_policy() {
        let mut policy = RecoveryPolicy::new();

        policy.record_marker(BootMarkerStage::KernelLoaded, 100, 0x11111111).unwrap();
        policy.record_marker(BootMarkerStage::SubsystemsReady, 200, 0x22222222).unwrap();
        policy.record_marker(BootMarkerStage::ShellReady, 300, 0x33333333).unwrap();

        // Shell_Ready should auto-create snapshot
        assert_eq!(policy.snapshot_count, 1);

        // Simulate failure
        policy.detect_failure().ok();
        let recovery = policy.attempt_recovery();
        assert!(recovery.is_ok());
    }

    #[test]
    fn test_stage_stuck_detection() {
        let mut storage = MarkerStorage::new();
        storage.max_attempts_per_stage = 3;

        storage.record(BootMarkerStage::KernelLoaded, 100, 0x11111111).unwrap();

        // Keep recording same stage
        for _ in 0..3 {
            storage.record(BootMarkerStage::KernelLoaded, 100, 0x11111111).ok();
        }

        assert!(storage.is_stage_stuck());
    }
}
