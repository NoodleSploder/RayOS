// ===== RayOS System Update & Atomic Deployment (Phase 9B Task 5) =====
// A/B partition updates, compatibility checking, verification, rollback

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ===== Constants =====

const MAX_UPDATE_COMPONENTS: usize = 32;
const MAX_DEPENDENCIES: usize = 16;
const MAX_ROLLBACK_HISTORY: usize = 8;
const MAX_UPDATE_HOOKS: usize = 16;

// ===== Partition Slot =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PartitionSlot {
    /// Slot A (primary)
    SlotA,
    /// Slot B (secondary)
    SlotB,
}

impl PartitionSlot {
    pub fn other(&self) -> Self {
        match self {
            PartitionSlot::SlotA => PartitionSlot::SlotB,
            PartitionSlot::SlotB => PartitionSlot::SlotA,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PartitionSlot::SlotA => "slot-a",
            PartitionSlot::SlotB => "slot-b",
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            PartitionSlot::SlotA => "_a",
            PartitionSlot::SlotB => "_b",
        }
    }
}

// ===== Slot State =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SlotState {
    /// Slot is empty/uninitialized
    Empty,
    /// Slot has valid content
    Valid,
    /// Slot is marked as bootable
    Bootable,
    /// Slot is currently active
    Active,
    /// Slot failed boot verification
    BootFailed,
    /// Slot is being updated
    Updating,
    /// Slot marked for garbage collection
    Stale,
}

// ===== Partition Metadata =====

#[derive(Copy, Clone)]
pub struct PartitionMeta {
    /// Slot identifier
    pub slot: PartitionSlot,
    /// Slot state
    pub state: SlotState,
    /// Version string hash
    pub version_hash: u64,
    /// Build timestamp
    pub build_timestamp: u64,
    /// Boot attempt counter
    pub boot_attempts: u32,
    /// Maximum boot attempts before failover
    pub max_boot_attempts: u32,
    /// Successful boot flag
    pub boot_successful: bool,
    /// Priority (higher = preferred)
    pub priority: u8,
    /// Verified boot signature
    pub signature_valid: bool,
    /// dm-verity root hash
    pub verity_hash: [u8; 32],
}

impl PartitionMeta {
    pub fn new(slot: PartitionSlot) -> Self {
        PartitionMeta {
            slot,
            state: SlotState::Empty,
            version_hash: 0,
            build_timestamp: 0,
            boot_attempts: 0,
            max_boot_attempts: 3,
            boot_successful: false,
            priority: if slot == PartitionSlot::SlotA { 1 } else { 0 },
            signature_valid: false,
            verity_hash: [0u8; 32],
        }
    }

    pub fn mark_bootable(&mut self) {
        self.state = SlotState::Bootable;
        self.boot_attempts = 0;
        self.boot_successful = false;
    }

    pub fn mark_active(&mut self) {
        self.state = SlotState::Active;
    }

    pub fn increment_boot_attempt(&mut self) -> bool {
        self.boot_attempts += 1;
        self.boot_attempts <= self.max_boot_attempts
    }

    pub fn mark_boot_successful(&mut self) {
        self.boot_successful = true;
        self.boot_attempts = 0;
    }

    pub fn should_failover(&self) -> bool {
        self.boot_attempts >= self.max_boot_attempts && !self.boot_successful
    }
}

// ===== A/B Partition Manager =====

pub struct AbPartitionManager {
    slot_a: PartitionMeta,
    slot_b: PartitionMeta,
    current_slot: PartitionSlot,
    fallback_enabled: bool,
}

impl AbPartitionManager {
    pub fn new() -> Self {
        AbPartitionManager {
            slot_a: PartitionMeta::new(PartitionSlot::SlotA),
            slot_b: PartitionMeta::new(PartitionSlot::SlotB),
            current_slot: PartitionSlot::SlotA,
            fallback_enabled: true,
        }
    }

    pub fn current_slot(&self) -> PartitionSlot {
        self.current_slot
    }

    pub fn inactive_slot(&self) -> PartitionSlot {
        self.current_slot.other()
    }

    pub fn get_slot_meta(&self, slot: PartitionSlot) -> &PartitionMeta {
        match slot {
            PartitionSlot::SlotA => &self.slot_a,
            PartitionSlot::SlotB => &self.slot_b,
        }
    }

    pub fn get_slot_meta_mut(&mut self, slot: PartitionSlot) -> &mut PartitionMeta {
        match slot {
            PartitionSlot::SlotA => &mut self.slot_a,
            PartitionSlot::SlotB => &mut self.slot_b,
        }
    }

    /// Prepare inactive slot for update
    pub fn prepare_for_update(&mut self) -> PartitionSlot {
        let target = self.inactive_slot();
        let meta = self.get_slot_meta_mut(target);
        meta.state = SlotState::Updating;
        meta.boot_attempts = 0;
        meta.boot_successful = false;
        meta.signature_valid = false;
        target
    }

    /// Finalize update and mark slot as bootable
    pub fn finalize_update(&mut self, slot: PartitionSlot, version_hash: u64, timestamp: u64) {
        let meta = self.get_slot_meta_mut(slot);
        meta.version_hash = version_hash;
        meta.build_timestamp = timestamp;
        meta.mark_bootable();
        
        // Increase priority of new slot
        meta.priority = 2;
        
        // Decrease priority of old slot
        let other = slot.other();
        self.get_slot_meta_mut(other).priority = 1;
    }

    /// Select best slot for next boot
    pub fn select_boot_slot(&mut self) -> PartitionSlot {
        let a = &self.slot_a;
        let b = &self.slot_b;

        // Check if current slot failed too many times
        let current_meta = self.get_slot_meta(self.current_slot);
        if current_meta.should_failover() && self.fallback_enabled {
            let fallback = self.current_slot.other();
            let fallback_meta = self.get_slot_meta(fallback);
            if fallback_meta.state == SlotState::Bootable || fallback_meta.state == SlotState::Valid {
                return fallback;
            }
        }

        // Select by priority
        if a.priority > b.priority && matches!(a.state, SlotState::Bootable | SlotState::Valid | SlotState::Active) {
            PartitionSlot::SlotA
        } else if matches!(b.state, SlotState::Bootable | SlotState::Valid | SlotState::Active) {
            PartitionSlot::SlotB
        } else {
            self.current_slot
        }
    }

    /// Mark current boot as successful
    pub fn mark_boot_successful(&mut self) {
        let meta = self.get_slot_meta_mut(self.current_slot);
        meta.mark_boot_successful();
        meta.state = SlotState::Active;
    }

    /// Rollback to previous slot
    pub fn rollback(&mut self) -> Result<PartitionSlot, &'static str> {
        let target = self.current_slot.other();
        let meta = self.get_slot_meta(target);
        
        if !matches!(meta.state, SlotState::Valid | SlotState::Bootable) {
            return Err("No valid rollback target");
        }

        // Swap priorities
        let current = self.current_slot;
        self.get_slot_meta_mut(target).priority = 2;
        self.get_slot_meta_mut(current).priority = 1;
        self.get_slot_meta_mut(target).mark_bootable();

        Ok(target)
    }
}

// ===== Update Component =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ComponentType {
    /// Bootloader
    Bootloader,
    /// Kernel image
    Kernel,
    /// Initial ramdisk
    Initrd,
    /// Root filesystem
    Rootfs,
    /// System partition
    System,
    /// Vendor partition
    Vendor,
    /// OEM partition
    Oem,
    /// Recovery partition
    Recovery,
    /// Device tree blob
    Dtb,
    /// Firmware
    Firmware,
}

#[derive(Copy, Clone)]
pub struct UpdateComponent {
    pub component_type: ComponentType,
    /// Current version
    pub current_version: u64,
    /// Target version
    pub target_version: u64,
    /// Download size in bytes
    pub download_size: u64,
    /// Installed size in bytes
    pub installed_size: u64,
    /// SHA-256 hash of update payload
    pub payload_hash: [u8; 32],
    /// Is delta update
    pub is_delta: bool,
    /// Requires reboot
    pub requires_reboot: bool,
    /// Update progress (0-100)
    pub progress: u8,
    /// Update succeeded
    pub completed: bool,
}

impl UpdateComponent {
    pub fn new(component_type: ComponentType, current: u64, target: u64) -> Self {
        UpdateComponent {
            component_type,
            current_version: current,
            target_version: target,
            download_size: 0,
            installed_size: 0,
            payload_hash: [0u8; 32],
            is_delta: false,
            requires_reboot: true,
            progress: 0,
            completed: false,
        }
    }
}

// ===== Compatibility Check =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CompatibilityResult {
    /// Fully compatible
    Compatible,
    /// Compatible with warnings
    CompatibleWithWarnings,
    /// Incompatible - missing dependencies
    MissingDependency,
    /// Incompatible - version conflict
    VersionConflict,
    /// Incompatible - hardware not supported
    HardwareNotSupported,
    /// Incompatible - insufficient space
    InsufficientSpace,
    /// Incompatible - requires newer bootloader
    BootloaderTooOld,
}

#[derive(Copy, Clone)]
pub struct Dependency {
    /// Component type
    pub component: ComponentType,
    /// Minimum version required
    pub min_version: u64,
    /// Maximum version (0 = any)
    pub max_version: u64,
    /// Is hard requirement
    pub required: bool,
}

impl Dependency {
    pub fn new(component: ComponentType, min_version: u64) -> Self {
        Dependency {
            component,
            min_version,
            max_version: 0,
            required: true,
        }
    }

    pub fn check(&self, current_version: u64) -> bool {
        current_version >= self.min_version && 
        (self.max_version == 0 || current_version <= self.max_version)
    }
}

pub struct CompatibilityChecker {
    dependencies: [Dependency; MAX_DEPENDENCIES],
    dep_count: usize,
    required_space: u64,
    available_space: u64,
}

impl CompatibilityChecker {
    pub fn new() -> Self {
        CompatibilityChecker {
            dependencies: [Dependency::new(ComponentType::Kernel, 0); MAX_DEPENDENCIES],
            dep_count: 0,
            required_space: 0,
            available_space: 0,
        }
    }

    pub fn add_dependency(&mut self, dep: Dependency) {
        if self.dep_count < MAX_DEPENDENCIES {
            self.dependencies[self.dep_count] = dep;
            self.dep_count += 1;
        }
    }

    pub fn set_space_requirements(&mut self, required: u64, available: u64) {
        self.required_space = required;
        self.available_space = available;
    }

    pub fn check(&self, component_versions: &[(ComponentType, u64)]) -> CompatibilityResult {
        // Check space
        if self.required_space > self.available_space {
            return CompatibilityResult::InsufficientSpace;
        }

        // Check dependencies
        for i in 0..self.dep_count {
            let dep = &self.dependencies[i];
            let mut found = false;
            
            for (comp_type, version) in component_versions {
                if *comp_type == dep.component {
                    found = true;
                    if !dep.check(*version) {
                        return CompatibilityResult::VersionConflict;
                    }
                    break;
                }
            }

            if !found && dep.required {
                return CompatibilityResult::MissingDependency;
            }
        }

        CompatibilityResult::Compatible
    }
}

// ===== Update Verification =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VerificationResult {
    /// Verification passed
    Valid,
    /// Hash mismatch
    HashMismatch,
    /// Signature invalid
    SignatureInvalid,
    /// Certificate expired
    CertificateExpired,
    /// Payload corrupted
    PayloadCorrupted,
    /// Size mismatch
    SizeMismatch,
}

pub struct UpdateVerifier {
    /// Expected SHA-256 hash
    expected_hash: [u8; 32],
    /// Calculated hash (incremental)
    calculated_hash: [u8; 32],
    /// Bytes verified
    bytes_verified: u64,
    /// Expected total size
    expected_size: u64,
    /// Public key for signature verification
    public_key: [u8; 64],
    /// Has public key
    has_public_key: bool,
}

impl UpdateVerifier {
    pub fn new() -> Self {
        UpdateVerifier {
            expected_hash: [0u8; 32],
            calculated_hash: [0u8; 32],
            bytes_verified: 0,
            expected_size: 0,
            public_key: [0u8; 64],
            has_public_key: false,
        }
    }

    pub fn set_expected_hash(&mut self, hash: &[u8; 32]) {
        self.expected_hash = *hash;
    }

    pub fn set_expected_size(&mut self, size: u64) {
        self.expected_size = size;
    }

    pub fn set_public_key(&mut self, key: &[u8; 64]) {
        self.public_key = *key;
        self.has_public_key = true;
    }

    pub fn update(&mut self, data: &[u8]) {
        // Would update hash incrementally
        self.bytes_verified += data.len() as u64;
    }

    pub fn finalize(&self) -> VerificationResult {
        // Check size
        if self.bytes_verified != self.expected_size && self.expected_size > 0 {
            return VerificationResult::SizeMismatch;
        }

        // Check hash
        if self.calculated_hash != self.expected_hash {
            return VerificationResult::HashMismatch;
        }

        VerificationResult::Valid
    }

    pub fn verify_signature(&self, _signature: &[u8]) -> VerificationResult {
        if !self.has_public_key {
            return VerificationResult::SignatureInvalid;
        }
        // Would verify Ed25519/ECDSA signature
        VerificationResult::Valid
    }
}

// ===== Update State Machine =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UpdatePhase {
    /// Idle, no update in progress
    Idle,
    /// Checking for updates
    Checking,
    /// Update available
    Available,
    /// Downloading update
    Downloading,
    /// Verifying download
    Verifying,
    /// Preparing target partition
    Preparing,
    /// Installing update
    Installing,
    /// Finalizing update
    Finalizing,
    /// Pending reboot
    PendingReboot,
    /// Update complete
    Complete,
    /// Update failed
    Failed,
    /// Rolling back
    RollingBack,
}

#[derive(Debug, Copy, Clone)]
pub enum UpdateError {
    /// Download failed
    DownloadFailed,
    /// Verification failed
    VerificationFailed,
    /// Incompatible update
    Incompatible,
    /// Insufficient space
    InsufficientSpace,
    /// Installation failed
    InstallFailed,
    /// Already in progress
    AlreadyInProgress,
    /// Network error
    NetworkError,
    /// Server error
    ServerError,
    /// Cancelled by user
    Cancelled,
}

pub struct UpdateStateMachine {
    phase: UpdatePhase,
    error: Option<UpdateError>,
    progress_percent: u8,
    bytes_downloaded: u64,
    bytes_total: u64,
    current_component: usize,
    total_components: usize,
}

impl UpdateStateMachine {
    pub fn new() -> Self {
        UpdateStateMachine {
            phase: UpdatePhase::Idle,
            error: None,
            progress_percent: 0,
            bytes_downloaded: 0,
            bytes_total: 0,
            current_component: 0,
            total_components: 0,
        }
    }

    pub fn phase(&self) -> UpdatePhase {
        self.phase
    }

    pub fn error(&self) -> Option<UpdateError> {
        self.error
    }

    pub fn progress(&self) -> u8 {
        self.progress_percent
    }

    pub fn transition(&mut self, next: UpdatePhase) -> Result<(), &'static str> {
        // Validate transitions
        let valid = match (self.phase, next) {
            (UpdatePhase::Idle, UpdatePhase::Checking) => true,
            (UpdatePhase::Checking, UpdatePhase::Available) => true,
            (UpdatePhase::Checking, UpdatePhase::Idle) => true,
            (UpdatePhase::Available, UpdatePhase::Downloading) => true,
            (UpdatePhase::Available, UpdatePhase::Idle) => true,
            (UpdatePhase::Downloading, UpdatePhase::Verifying) => true,
            (UpdatePhase::Downloading, UpdatePhase::Failed) => true,
            (UpdatePhase::Verifying, UpdatePhase::Preparing) => true,
            (UpdatePhase::Verifying, UpdatePhase::Failed) => true,
            (UpdatePhase::Preparing, UpdatePhase::Installing) => true,
            (UpdatePhase::Preparing, UpdatePhase::Failed) => true,
            (UpdatePhase::Installing, UpdatePhase::Finalizing) => true,
            (UpdatePhase::Installing, UpdatePhase::Failed) => true,
            (UpdatePhase::Finalizing, UpdatePhase::PendingReboot) => true,
            (UpdatePhase::Finalizing, UpdatePhase::Complete) => true,
            (UpdatePhase::Failed, UpdatePhase::RollingBack) => true,
            (UpdatePhase::Failed, UpdatePhase::Idle) => true,
            (UpdatePhase::RollingBack, UpdatePhase::Idle) => true,
            (UpdatePhase::Complete, UpdatePhase::Idle) => true,
            (UpdatePhase::PendingReboot, UpdatePhase::Idle) => true,
            _ => false,
        };

        if valid {
            self.phase = next;
            if next == UpdatePhase::Idle {
                self.reset();
            }
            Ok(())
        } else {
            Err("Invalid state transition")
        }
    }

    pub fn set_error(&mut self, error: UpdateError) {
        self.error = Some(error);
        self.phase = UpdatePhase::Failed;
    }

    pub fn set_progress(&mut self, downloaded: u64, total: u64) {
        self.bytes_downloaded = downloaded;
        self.bytes_total = total;
        if total > 0 {
            self.progress_percent = ((downloaded * 100) / total) as u8;
        }
    }

    pub fn reset(&mut self) {
        self.phase = UpdatePhase::Idle;
        self.error = None;
        self.progress_percent = 0;
        self.bytes_downloaded = 0;
        self.bytes_total = 0;
        self.current_component = 0;
        self.total_components = 0;
    }
}

// ===== Rollback History =====

#[derive(Copy, Clone)]
pub struct RollbackEntry {
    /// Version hash
    pub version_hash: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Slot used
    pub slot: PartitionSlot,
    /// Was automatic rollback
    pub automatic: bool,
    /// Reason code
    pub reason: u32,
}

impl RollbackEntry {
    pub fn new() -> Self {
        RollbackEntry {
            version_hash: 0,
            timestamp: 0,
            slot: PartitionSlot::SlotA,
            automatic: false,
            reason: 0,
        }
    }
}

pub struct RollbackHistory {
    entries: [RollbackEntry; MAX_ROLLBACK_HISTORY],
    count: usize,
    head: usize,
}

impl RollbackHistory {
    pub fn new() -> Self {
        RollbackHistory {
            entries: [RollbackEntry::new(); MAX_ROLLBACK_HISTORY],
            count: 0,
            head: 0,
        }
    }

    pub fn record(&mut self, entry: RollbackEntry) {
        self.entries[self.head] = entry;
        self.head = (self.head + 1) % MAX_ROLLBACK_HISTORY;
        if self.count < MAX_ROLLBACK_HISTORY {
            self.count += 1;
        }
    }

    pub fn last(&self) -> Option<&RollbackEntry> {
        if self.count == 0 {
            None
        } else {
            let idx = if self.head == 0 { MAX_ROLLBACK_HISTORY - 1 } else { self.head - 1 };
            Some(&self.entries[idx])
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

// ===== Update Hooks =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HookPhase {
    PreDownload,
    PostDownload,
    PreInstall,
    PostInstall,
    PreReboot,
    PostReboot,
    OnRollback,
}

pub type HookFn = fn(phase: HookPhase, context: u64) -> bool;

pub struct UpdateHooks {
    hooks: [(HookPhase, HookFn); MAX_UPDATE_HOOKS],
    count: usize,
}

impl UpdateHooks {
    pub fn new() -> Self {
        fn noop(_: HookPhase, _: u64) -> bool { true }
        UpdateHooks {
            hooks: [(HookPhase::PreDownload, noop as HookFn); MAX_UPDATE_HOOKS],
            count: 0,
        }
    }

    pub fn register(&mut self, phase: HookPhase, hook: HookFn) -> bool {
        if self.count >= MAX_UPDATE_HOOKS {
            return false;
        }
        self.hooks[self.count] = (phase, hook);
        self.count += 1;
        true
    }

    pub fn run(&self, phase: HookPhase, context: u64) -> bool {
        for i in 0..self.count {
            if self.hooks[i].0 == phase {
                if !self.hooks[i].1(phase, context) {
                    return false;
                }
            }
        }
        true
    }
}

// ===== Atomic Update Controller =====

pub struct AtomicUpdateController {
    pub partition_manager: AbPartitionManager,
    pub state_machine: UpdateStateMachine,
    pub verifier: UpdateVerifier,
    pub compatibility: CompatibilityChecker,
    pub rollback_history: RollbackHistory,
    pub hooks: UpdateHooks,
    components: [UpdateComponent; MAX_UPDATE_COMPONENTS],
    component_count: usize,
}

impl AtomicUpdateController {
    pub fn new() -> Self {
        AtomicUpdateController {
            partition_manager: AbPartitionManager::new(),
            state_machine: UpdateStateMachine::new(),
            verifier: UpdateVerifier::new(),
            compatibility: CompatibilityChecker::new(),
            rollback_history: RollbackHistory::new(),
            hooks: UpdateHooks::new(),
            components: [UpdateComponent::new(ComponentType::Kernel, 0, 0); MAX_UPDATE_COMPONENTS],
            component_count: 0,
        }
    }

    pub fn add_component(&mut self, component: UpdateComponent) -> bool {
        if self.component_count >= MAX_UPDATE_COMPONENTS {
            return false;
        }
        self.components[self.component_count] = component;
        self.component_count += 1;
        true
    }

    pub fn check_for_update(&mut self) -> Result<bool, UpdateError> {
        self.state_machine.transition(UpdatePhase::Checking)
            .map_err(|_| UpdateError::AlreadyInProgress)?;
        
        // Would query update server
        // For now, return no update
        let _ = self.state_machine.transition(UpdatePhase::Idle);
        Ok(false)
    }

    pub fn start_update(&mut self) -> Result<(), UpdateError> {
        if self.state_machine.phase() != UpdatePhase::Available {
            return Err(UpdateError::AlreadyInProgress);
        }

        // Run pre-download hooks
        if !self.hooks.run(HookPhase::PreDownload, 0) {
            return Err(UpdateError::Cancelled);
        }

        // Prepare inactive slot
        let target_slot = self.partition_manager.prepare_for_update();
        
        let _ = self.state_machine.transition(UpdatePhase::Downloading);
        
        // Download would happen here
        
        let _ = self.state_machine.transition(UpdatePhase::Verifying);
        
        // Verify
        let result = self.verifier.finalize();
        if result != VerificationResult::Valid {
            self.state_machine.set_error(UpdateError::VerificationFailed);
            return Err(UpdateError::VerificationFailed);
        }

        // Run post-download hooks
        self.hooks.run(HookPhase::PostDownload, 0);

        let _ = self.state_machine.transition(UpdatePhase::Preparing);
        let _ = self.state_machine.transition(UpdatePhase::Installing);

        // Install components
        for i in 0..self.component_count {
            self.components[i].completed = true;
        }

        let _ = self.state_machine.transition(UpdatePhase::Finalizing);

        // Finalize partition
        self.partition_manager.finalize_update(target_slot, 0, 0);

        // Run post-install hooks
        self.hooks.run(HookPhase::PostInstall, 0);

        let _ = self.state_machine.transition(UpdatePhase::PendingReboot);

        Ok(())
    }

    pub fn rollback(&mut self) -> Result<(), UpdateError> {
        match self.partition_manager.rollback() {
            Ok(slot) => {
                let entry = RollbackEntry {
                    version_hash: self.partition_manager.get_slot_meta(slot).version_hash,
                    timestamp: 0,
                    slot,
                    automatic: false,
                    reason: 0,
                };
                self.rollback_history.record(entry);
                self.hooks.run(HookPhase::OnRollback, 0);
                Ok(())
            }
            Err(_) => Err(UpdateError::InstallFailed),
        }
    }

    pub fn mark_boot_successful(&mut self) {
        self.partition_manager.mark_boot_successful();
        self.hooks.run(HookPhase::PostReboot, 0);
    }
}

// ===== Tests =====

pub fn test_ab_partition_manager() -> bool {
    let mut manager = AbPartitionManager::new();

    // Initial state
    if manager.current_slot() != PartitionSlot::SlotA {
        return false;
    }

    // Prepare for update
    let target = manager.prepare_for_update();
    if target != PartitionSlot::SlotB {
        return false;
    }

    // Finalize
    manager.finalize_update(PartitionSlot::SlotB, 12345, 100);

    // Check priority changed
    if manager.get_slot_meta(PartitionSlot::SlotB).priority != 2 {
        return false;
    }

    // Select boot slot should prefer B now
    let selected = manager.select_boot_slot();
    if selected != PartitionSlot::SlotB {
        return false;
    }

    true
}

pub fn test_compatibility_checker() -> bool {
    let mut checker = CompatibilityChecker::new();

    // Add dependencies
    checker.add_dependency(Dependency::new(ComponentType::Kernel, 100));
    checker.set_space_requirements(1000, 5000);

    // Check compatible
    let versions = [(ComponentType::Kernel, 150u64)];
    if checker.check(&versions) != CompatibilityResult::Compatible {
        return false;
    }

    // Check version conflict
    let old_versions = [(ComponentType::Kernel, 50u64)];
    if checker.check(&old_versions) != CompatibilityResult::VersionConflict {
        return false;
    }

    // Check insufficient space
    checker.set_space_requirements(10000, 5000);
    if checker.check(&versions) != CompatibilityResult::InsufficientSpace {
        return false;
    }

    true
}

pub fn test_update_state_machine() -> bool {
    let mut sm = UpdateStateMachine::new();

    // Valid transitions
    if sm.transition(UpdatePhase::Checking).is_err() {
        return false;
    }
    if sm.transition(UpdatePhase::Available).is_err() {
        return false;
    }
    if sm.transition(UpdatePhase::Downloading).is_err() {
        return false;
    }

    // Invalid transition
    if sm.transition(UpdatePhase::Complete).is_ok() {
        return false;
    }

    true
}
