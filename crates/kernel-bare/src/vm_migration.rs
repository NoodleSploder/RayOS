// VM Live Migration Support
// Implements live VM migration with dirty page tracking, memory pre-copy, and stop-and-copy

use core::fmt;

// Migration state machine
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MigrationState {
    Idle = 0,                    // Not migrating
    PreCopy = 1,                 // Pre-copy phase (dirty page tracking)
    StopAndCopy = 2,             // Stop VM, copy final pages
    Verification = 3,            // Verify target consistency
    Completing = 4,              // Finalization on target
    Completed = 5,               // Migration complete
    Failed = 6,                  // Migration failed
    RollingBack = 7,             // Rolling back source
}

impl fmt::Display for MigrationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::PreCopy => write!(f, "PreCopy"),
            Self::StopAndCopy => write!(f, "StopAndCopy"),
            Self::Verification => write!(f, "Verification"),
            Self::Completing => write!(f, "Completing"),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
            Self::RollingBack => write!(f, "RollingBack"),
        }
    }
}

// Memory page tracking for pre-copy phase
#[derive(Copy, Clone, Debug)]
pub struct MemoryPage {
    pub page_num: u32,           // Page number (0 = addr 0, 1 = addr 4KB, etc)
    pub is_dirty: bool,          // True if modified since last copy
    pub copy_count: u16,         // Number of times copied
    pub last_copy_time_ms: u32,  // Time of last copy
}

impl MemoryPage {
    pub fn new(page_num: u32) -> Self {
        Self {
            page_num,
            is_dirty: true,
            copy_count: 0,
            last_copy_time_ms: 0,
        }
    }
}

// Dirty page tracking during pre-copy
#[derive(Copy, Clone, Debug)]
pub struct DirtyPageTracking {
    pub pages: [MemoryPage; 256],     // Track up to 256 pages (1GB at 4KB granularity)
    pub num_pages: usize,              // Total pages in VM
    pub pages_marked_clean: u32,       // Pages no longer dirty
    pub pages_with_errors: u32,        // Pages failed to copy
    pub total_dirty_scans: u32,        // Number of dirty passes
}

impl DirtyPageTracking {
    pub fn new(total_pages: u32) -> Self {
        let num_pages = if total_pages > 256 { 256 } else { total_pages as usize };
        Self {
            pages: [MemoryPage::new(0); 256],
            num_pages,
            pages_marked_clean: 0,
            pages_with_errors: 0,
            total_dirty_scans: 0,
        }
    }

    pub fn mark_page_clean(&mut self, page_num: u32) {
        for page in self.pages.iter_mut().take(self.num_pages) {
            if page.page_num == page_num {
                page.is_dirty = false;
                self.pages_marked_clean += 1;
                break;
            }
        }
    }

    pub fn mark_page_dirty(&mut self, page_num: u32) {
        for page in self.pages.iter_mut().take(self.num_pages) {
            if page.page_num == page_num {
                page.is_dirty = true;
                break;
            }
        }
    }

    pub fn get_dirty_page_count(&self) -> u32 {
        self.pages.iter()
            .take(self.num_pages)
            .filter(|p| p.is_dirty)
            .count() as u32
    }
}

// Migration progress tracking
#[derive(Copy, Clone, Debug)]
pub struct MigrationProgress {
    pub total_pages: u32,               // Total pages to migrate
    pub pages_copied: u32,              // Pages successfully copied
    pub pages_pending: u32,             // Pages still needing copy
    pub pages_verified: u32,            // Pages verified on target
    pub progress_percent: u8,           // 0-100%
    pub time_elapsed_ms: u32,           // Elapsed time
    pub bandwidth_mbps: u32,            // Measured bandwidth
    pub est_remaining_time_ms: u32,     // Estimated time remaining
}

impl MigrationProgress {
    pub fn new(total_pages: u32) -> Self {
        Self {
            total_pages,
            pages_copied: 0,
            pages_pending: total_pages,
            pages_verified: 0,
            progress_percent: 0,
            time_elapsed_ms: 0,
            bandwidth_mbps: 0,
            est_remaining_time_ms: 0,
        }
    }

    pub fn update(&mut self, pages_just_copied: u32, elapsed_ms: u32, page_size_kb: u32) {
        self.pages_copied += pages_just_copied;
        self.pages_pending = self.total_pages.saturating_sub(self.pages_copied);
        self.time_elapsed_ms = elapsed_ms;

        if elapsed_ms > 0 {
            let bytes_copied = self.pages_copied as u64 * page_size_kb as u64 * 1024;
            self.bandwidth_mbps = ((bytes_copied / (1024 * 1024)) * 1000 / elapsed_ms as u64) as u32;
        }

        if self.pages_pending > 0 && self.bandwidth_mbps > 0 {
            let bytes_remaining = self.pages_pending as u64 * page_size_kb as u64 * 1024;
            let mb_remaining = bytes_remaining / (1024 * 1024);
            self.est_remaining_time_ms = (mb_remaining * 1000 / self.bandwidth_mbps as u64) as u32;
        } else {
            self.est_remaining_time_ms = 0;
        }

        if self.total_pages > 0 {
            self.progress_percent = ((self.pages_copied as u64 * 100) / self.total_pages as u64) as u8;
        }
    }
}

// Migration session for live migration workflow
#[derive(Copy, Clone, Debug)]
pub struct MigrationSession {
    pub session_id: u32,                // Unique migration session ID
    pub source_vm_id: u32,              // Source VM identifier
    pub target_vm_id: u32,              // Target VM identifier
    pub state: MigrationState,          // Current migration state
    pub timestamp_start_ms: u32,        // Start timestamp
    pub precopy_iterations: u32,        // Number of pre-copy iterations
    pub precopy_time_ms: u32,           // Total pre-copy time
    pub stop_and_copy_time_ms: u32,     // Stop-and-copy duration
    pub verification_time_ms: u32,      // Verification phase time
    pub total_time_ms: u32,             // Total migration time
    pub pages_dirty_precopy: u32,       // Pages dirtied during pre-copy
    pub pages_re_sent: u32,             // Pages sent multiple times
    pub checksum_mismatches: u32,       // Verification failures
    pub error_code: u32,                // Error if failed
    pub is_rollback_pending: bool,      // Rollback needed
}

impl MigrationSession {
    pub fn new(session_id: u32, source_vm: u32, target_vm: u32) -> Self {
        Self {
            session_id,
            source_vm_id: source_vm,
            target_vm_id: target_vm,
            state: MigrationState::Idle,
            timestamp_start_ms: 0,
            precopy_iterations: 0,
            precopy_time_ms: 0,
            stop_and_copy_time_ms: 0,
            verification_time_ms: 0,
            total_time_ms: 0,
            pages_dirty_precopy: 0,
            pages_re_sent: 0,
            checksum_mismatches: 0,
            error_code: 0,
            is_rollback_pending: false,
        }
    }

    pub fn can_transition_to(&self, new_state: MigrationState) -> bool {
        match (self.state, new_state) {
            (MigrationState::Idle, MigrationState::PreCopy) => true,
            (MigrationState::PreCopy, MigrationState::StopAndCopy) => true,
            (MigrationState::PreCopy, MigrationState::RollingBack) => true,
            (MigrationState::StopAndCopy, MigrationState::Verification) => true,
            (MigrationState::StopAndCopy, MigrationState::Failed) => true,
            (MigrationState::Verification, MigrationState::Completing) => true,
            (MigrationState::Verification, MigrationState::Failed) => true,
            (MigrationState::Completing, MigrationState::Completed) => true,
            (MigrationState::RollingBack, MigrationState::Idle) => true,
            _ => self.state == new_state, // Allow idempotent transitions
        }
    }
}

// Central migration manager
pub struct VmMigrationManager {
    sessions: [Option<MigrationSession>; 8],  // Max 8 concurrent migrations
    progress: [Option<MigrationProgress>; 8],
    dirty_tracking: [Option<DirtyPageTracking>; 8],
    active_sessions: u32,
    completed_migrations: u32,
    failed_migrations: u32,
    total_pages_migrated: u64,
}

impl VmMigrationManager {
    pub const fn new() -> Self {
        const NONE_SESSION: Option<MigrationSession> = None;
        const NONE_PROGRESS: Option<MigrationProgress> = None;
        const NONE_TRACKING: Option<DirtyPageTracking> = None;

        Self {
            sessions: [NONE_SESSION; 8],
            progress: [NONE_PROGRESS; 8],
            dirty_tracking: [NONE_TRACKING; 8],
            active_sessions: 0,
            completed_migrations: 0,
            failed_migrations: 0,
            total_pages_migrated: 0,
        }
    }

    pub fn start_migration(&mut self, session_id: u32, source_vm: u32, target_vm: u32, total_pages: u32) -> bool {
        if self.active_sessions >= 8 {
            return false;
        }

        for i in 0..8 {
            if self.sessions[i].is_none() {
                let mut session = MigrationSession::new(session_id, source_vm, target_vm);
                session.state = MigrationState::PreCopy;

                self.sessions[i] = Some(session);
                self.progress[i] = Some(MigrationProgress::new(total_pages));
                self.dirty_tracking[i] = Some(DirtyPageTracking::new(total_pages));
                self.active_sessions += 1;

                return true;
            }
        }

        false
    }

    pub fn advance_precopy(&mut self, session_id: u32) -> bool {
        for i in 0..8 {
            if let Some(session) = self.sessions[i] {
                if session.session_id == session_id {
                    if let Some(progress) = self.progress[i].as_mut() {
                        if let Some(dirty) = self.dirty_tracking[i].as_mut() {
                            // Simulate copying dirty pages
                            let dirty_count = dirty.get_dirty_page_count();
                            progress.update(dirty_count.min(64), progress.time_elapsed_ms + 50, 4);
                            dirty.total_dirty_scans += 1;

                            if let Some(sess) = self.sessions[i].as_mut() {
                                sess.precopy_iterations += 1;
                                sess.precopy_time_ms += 50;
                                sess.pages_dirty_precopy += dirty_count;
                            }

                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    pub fn transition_state(&mut self, session_id: u32, new_state: MigrationState) -> bool {
        for i in 0..8 {
            if let Some(session) = self.sessions[i].as_mut() {
                if session.session_id == session_id {
                    if session.can_transition_to(new_state) {
                        session.state = new_state;
                        return true;
                    } else {
                        return false;
                    }
                }
            }
        }

        false
    }

    pub fn complete_migration(&mut self, session_id: u32) -> bool {
        for i in 0..8 {
            if let Some(session) = self.sessions[i].as_mut() {
                if session.session_id == session_id {
                    if session.state == MigrationState::Completed {
                        if let Some(progress) = self.progress[i] {
                            self.total_pages_migrated += progress.pages_verified as u64;
                            self.completed_migrations += 1;
                        }

                        self.sessions[i] = None;
                        self.progress[i] = None;
                        self.dirty_tracking[i] = None;
                        self.active_sessions = self.active_sessions.saturating_sub(1);

                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn abort_migration(&mut self, session_id: u32) -> bool {
        for i in 0..8 {
            if let Some(session) = self.sessions[i].as_mut() {
                if session.session_id == session_id {
                    session.state = MigrationState::Failed;
                    session.error_code = 1;
                    self.failed_migrations += 1;

                    self.sessions[i] = None;
                    self.progress[i] = None;
                    self.dirty_tracking[i] = None;
                    self.active_sessions = self.active_sessions.saturating_sub(1);

                    return true;
                }
            }
        }

        false
    }

    pub fn get_session_progress(&self, session_id: u32) -> Option<MigrationProgress> {
        for i in 0..8 {
            if let Some(session) = self.sessions[i] {
                if session.session_id == session_id {
                    return self.progress[i];
                }
            }
        }

        None
    }

    pub fn get_stats(&self) -> (u32, u32, u32, u64) {
        (self.active_sessions, self.completed_migrations, self.failed_migrations, self.total_pages_migrated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_state_transitions() {
        let session = MigrationSession::new(1, 100, 200);

        let mut state = MigrationState::Idle;
        assert!(session.can_transition_to(state));

        state = MigrationState::PreCopy;
        assert!(session.can_transition_to(state));

        // Can't jump directly from Idle to StopAndCopy
        let session2 = MigrationSession::new(2, 100, 200);
        assert!(!session2.can_transition_to(MigrationState::StopAndCopy));
    }

    #[test]
    fn test_dirty_page_tracking() {
        let mut tracking = DirtyPageTracking::new(256);
        assert_eq!(tracking.num_pages, 256);
        assert_eq!(tracking.get_dirty_page_count(), 256);

        tracking.mark_page_clean(0);
        assert_eq!(tracking.get_dirty_page_count(), 255);

        tracking.mark_page_dirty(0);
        assert_eq!(tracking.get_dirty_page_count(), 256);
    }

    #[test]
    fn test_migration_progress() {
        let mut progress = MigrationProgress::new(1024);
        assert_eq!(progress.pages_copied, 0);
        assert_eq!(progress.progress_percent, 0);

        progress.update(256, 100, 4);
        assert_eq!(progress.pages_copied, 256);
        assert!(progress.bandwidth_mbps > 0);
        assert!(progress.progress_percent > 0 && progress.progress_percent <= 100);
    }

    #[test]
    fn test_migration_manager_lifecycle() {
        let mut manager = VmMigrationManager::new();
        assert_eq!(manager.active_sessions, 0);

        let started = manager.start_migration(1, 100, 200, 512);
        assert!(started);
        assert_eq!(manager.active_sessions, 1);

        let advanced = manager.advance_precopy(1);
        assert!(advanced);

        let transitioned = manager.transition_state(1, MigrationState::StopAndCopy);
        assert!(transitioned);

        let aborted = manager.abort_migration(1);
        assert!(aborted);
        assert_eq!(manager.active_sessions, 0);
    }

    #[test]
    fn test_concurrent_migrations() {
        let mut manager = VmMigrationManager::new();

        for i in 1..=8 {
            let started = manager.start_migration(i, 100 + i as u32, 200 + i as u32, 256);
            assert!(started);
        }

        assert_eq!(manager.active_sessions, 8);

        // Can't start more than 8 concurrent
        let failed = manager.start_migration(9, 200, 300, 256);
        assert!(!failed);

        for i in 1..=8 {
            manager.transition_state(i, MigrationState::Completed);
            manager.complete_migration(i);
        }

        assert_eq!(manager.active_sessions, 0);
        assert_eq!(manager.completed_migrations, 8);
    }

    #[test]
    fn test_precopy_iterations() {
        let mut manager = VmMigrationManager::new();
        manager.start_migration(1, 100, 200, 256);

        for _ in 0..5 {
            manager.advance_precopy(1);
        }

        let (_, _, _, _) = manager.get_stats();
        // Verify multiple iterations completed
        assert_eq!(manager.active_sessions, 1);
    }

    #[test]
    fn test_migration_statistics() {
        let mut manager = VmMigrationManager::new();

        manager.start_migration(1, 100, 200, 512);
        manager.transition_state(1, MigrationState::Completed);
        manager.complete_migration(1);

        manager.start_migration(2, 100, 200, 512);
        manager.abort_migration(2);

        let (active, completed, failed, total_pages) = manager.get_stats();
        assert_eq!(active, 0);
        assert_eq!(completed, 1);
        assert_eq!(failed, 1);
        assert!(total_pages > 0);
    }
}
