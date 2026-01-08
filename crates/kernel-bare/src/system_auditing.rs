/// System Auditing & Logging
///
/// Comprehensive audit trail system for compliance, security monitoring,
/// and system event tracking with circular buffer storage and filtering.

use core::cmp::min;

const MAX_AUDIT_ENTRIES: usize = 8192;
const MAX_AUDIT_FILTERS: usize = 16;

/// Audit level enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AuditLevel {
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
    Critical = 4,
}

/// Audit event type enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AuditEventType {
    Security = 0,
    Performance = 1,
    System = 2,
    User = 3,
    Storage = 4,
    Network = 5,
}

/// Single audit entry
#[derive(Clone, Copy, Debug)]
pub struct AuditEntry {
    pub entry_id: u64,
    pub timestamp: u64,
    pub level: AuditLevel,
    pub event_type: AuditEventType,
    pub source_id: u32,
    pub target_id: u32,
    pub status_code: u32,
    pub message_hash: u32,
}

impl AuditEntry {
    pub fn new(
        entry_id: u64,
        timestamp: u64,
        level: AuditLevel,
        event_type: AuditEventType,
    ) -> Self {
        AuditEntry {
            entry_id,
            timestamp,
            level,
            event_type,
            source_id: 0,
            target_id: 0,
            status_code: 0,
            message_hash: 0,
        }
    }
}

/// Audit filter for event routing and filtering
#[derive(Clone, Copy, Debug)]
pub struct AuditFilter {
    pub filter_id: u32,
    pub event_type: AuditEventType,
    pub min_level: AuditLevel,
    pub source_id: u32,
    pub enabled: bool,
}

impl AuditFilter {
    pub fn new(filter_id: u32, event_type: AuditEventType, min_level: AuditLevel) -> Self {
        AuditFilter {
            filter_id,
            event_type,
            min_level,
            source_id: 0,
            enabled: true,
        }
    }

    pub fn matches(&self, entry: &AuditEntry) -> bool {
        if !self.enabled {
            return false;
        }
        if entry.event_type != self.event_type {
            return false;
        }
        if (entry.level as u32) < (self.min_level as u32) {
            return false;
        }
        if self.source_id > 0 && entry.source_id != self.source_id {
            return false;
        }
        true
    }
}

/// Compliance tracking
#[derive(Clone, Copy, Debug)]
pub struct ComplianceRecord {
    pub requirement_id: u32,
    pub status: u8,
    pub last_verified: u64,
    pub violation_count: u32,
}

impl ComplianceRecord {
    pub fn new(requirement_id: u32) -> Self {
        ComplianceRecord {
            requirement_id,
            status: 0,
            last_verified: 0,
            violation_count: 0,
        }
    }
}

/// System Auditing and Logging
pub struct AuditingSystem {
    entries: [Option<AuditEntry>; MAX_AUDIT_ENTRIES],
    filters: [Option<AuditFilter>; MAX_AUDIT_FILTERS],
    compliance: [Option<ComplianceRecord>; 32],
    buffer_head: usize,
    entry_count: u64,
    total_logged: u64,
    total_filtered: u64,
    filter_count: u32,
    compliance_count: u32,
}

impl AuditingSystem {
    pub fn new() -> Self {
        AuditingSystem {
            entries: [None; MAX_AUDIT_ENTRIES],
            filters: [None; MAX_AUDIT_FILTERS],
            compliance: [None; 32],
            buffer_head: 0,
            entry_count: 0,
            total_logged: 0,
            total_filtered: 0,
            filter_count: 0,
            compliance_count: 0,
        }
    }

    pub fn log_event(
        &mut self,
        timestamp: u64,
        level: AuditLevel,
        event_type: AuditEventType,
        source_id: u32,
        target_id: u32,
    ) -> u64 {
        let entry_id = self.entry_count;
        self.entry_count += 1;
        self.total_logged += 1;

        let mut entry = AuditEntry::new(entry_id, timestamp, level, event_type);
        entry.source_id = source_id;
        entry.target_id = target_id;

        self.entries[self.buffer_head] = Some(entry);
        self.buffer_head = (self.buffer_head + 1) % MAX_AUDIT_ENTRIES;

        entry_id
    }

    pub fn log_event_with_status(
        &mut self,
        timestamp: u64,
        level: AuditLevel,
        event_type: AuditEventType,
        source_id: u32,
        status_code: u32,
    ) -> u64 {
        let entry_id = self.entry_count;
        self.entry_count += 1;
        self.total_logged += 1;

        let mut entry = AuditEntry::new(entry_id, timestamp, level, event_type);
        entry.source_id = source_id;
        entry.status_code = status_code;

        self.entries[self.buffer_head] = Some(entry);
        self.buffer_head = (self.buffer_head + 1) % MAX_AUDIT_ENTRIES;

        entry_id
    }

    pub fn add_filter(&mut self, event_type: AuditEventType, min_level: AuditLevel) -> u32 {
        for i in 0..MAX_AUDIT_FILTERS {
            if self.filters[i].is_none() {
                let filter_id = i as u32 + 1;
                let filter = AuditFilter::new(filter_id, event_type, min_level);
                self.filters[i] = Some(filter);
                self.filter_count += 1;
                return filter_id;
            }
        }
        0
    }

    pub fn remove_filter(&mut self, filter_id: u32) -> bool {
        let idx = (filter_id as usize) - 1;
        if idx < MAX_AUDIT_FILTERS {
            if self.filters[idx].is_some() {
                self.filters[idx] = None;
                self.filter_count -= 1;
                return true;
            }
        }
        false
    }

    pub fn enable_filter(&mut self, filter_id: u32, enabled: bool) -> bool {
        let idx = (filter_id as usize) - 1;
        if idx < MAX_AUDIT_FILTERS {
            if let Some(mut filter) = self.filters[idx] {
                filter.enabled = enabled;
                self.filters[idx] = Some(filter);
                if enabled {
                    self.total_filtered += 1;
                }
                return true;
            }
        }
        false
    }

    pub fn add_compliance_requirement(&mut self, requirement_id: u32) -> bool {
        for i in 0..32 {
            if self.compliance[i].is_none() {
                let record = ComplianceRecord::new(requirement_id);
                self.compliance[i] = Some(record);
                self.compliance_count += 1;
                return true;
            }
        }
        false
    }

    pub fn update_compliance_status(
        &mut self,
        requirement_id: u32,
        status: u8,
        timestamp: u64,
    ) -> bool {
        for i in 0..32 {
            if let Some(mut record) = self.compliance[i] {
                if record.requirement_id == requirement_id {
                    record.status = status;
                    record.last_verified = timestamp;
                    self.compliance[i] = Some(record);
                    return true;
                }
            }
        }
        false
    }

    pub fn record_compliance_violation(&mut self, requirement_id: u32) -> bool {
        for i in 0..32 {
            if let Some(mut record) = self.compliance[i] {
                if record.requirement_id == requirement_id {
                    record.violation_count += 1;
                    self.compliance[i] = Some(record);
                    return true;
                }
            }
        }
        false
    }

    pub fn get_entry(&self, index: usize) -> Option<AuditEntry> {
        if index < MAX_AUDIT_ENTRIES {
            self.entries[index]
        } else {
            None
        }
    }

    pub fn count_entries_by_level(&self, level: AuditLevel) -> u32 {
        let mut count = 0;
        for i in 0..MAX_AUDIT_ENTRIES {
            if let Some(entry) = self.entries[i] {
                if entry.level == level {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn count_entries_by_type(&self, event_type: AuditEventType) -> u32 {
        let mut count = 0;
        for i in 0..MAX_AUDIT_ENTRIES {
            if let Some(entry) = self.entries[i] {
                if entry.event_type == event_type {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn get_total_logged(&self) -> u64 {
        self.total_logged
    }

    pub fn get_total_filtered(&self) -> u64 {
        self.total_filtered
    }

    pub fn get_filter_count(&self) -> u32 {
        self.filter_count
    }

    pub fn get_compliance_count(&self) -> u32 {
        self.compliance_count
    }

    pub fn get_violations(&self, requirement_id: u32) -> u32 {
        for i in 0..32 {
            if let Some(record) = self.compliance[i] {
                if record.requirement_id == requirement_id {
                    return record.violation_count;
                }
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_event() {
        let mut auditing = AuditingSystem::new();
        let entry_id = auditing.log_event(100, AuditLevel::Info, AuditEventType::Security, 1, 2);
        assert!(entry_id >= 0);
        assert_eq!(auditing.get_total_logged(), 1);
    }

    #[test]
    fn test_log_multiple_events() {
        let mut auditing = AuditingSystem::new();
        for i in 0..10 {
            auditing.log_event(
                100 + i,
                AuditLevel::Info,
                AuditEventType::Security,
                i as u32,
                i as u32 + 1,
            );
        }
        assert_eq!(auditing.get_total_logged(), 10);
    }

    #[test]
    fn test_add_filter() {
        let mut auditing = AuditingSystem::new();
        let filter_id = auditing.add_filter(AuditEventType::Security, AuditLevel::Warning);
        assert!(filter_id > 0);
        assert_eq!(auditing.get_filter_count(), 1);
    }

    #[test]
    fn test_remove_filter() {
        let mut auditing = AuditingSystem::new();
        let filter_id = auditing.add_filter(AuditEventType::Security, AuditLevel::Warning);
        assert!(auditing.remove_filter(filter_id));
        assert_eq!(auditing.get_filter_count(), 0);
    }

    #[test]
    fn test_count_entries_by_level() {
        let mut auditing = AuditingSystem::new();
        auditing.log_event(100, AuditLevel::Info, AuditEventType::Security, 1, 2);
        auditing.log_event(101, AuditLevel::Warning, AuditEventType::Security, 1, 2);
        auditing.log_event(102, AuditLevel::Info, AuditEventType::Security, 1, 2);

        assert_eq!(auditing.count_entries_by_level(AuditLevel::Info), 2);
        assert_eq!(auditing.count_entries_by_level(AuditLevel::Warning), 1);
    }

    #[test]
    fn test_add_compliance_requirement() {
        let mut auditing = AuditingSystem::new();
        assert!(auditing.add_compliance_requirement(100));
        assert_eq!(auditing.get_compliance_count(), 1);
    }

    #[test]
    fn test_record_violation() {
        let mut auditing = AuditingSystem::new();
        auditing.add_compliance_requirement(100);
        auditing.record_compliance_violation(100);
        assert_eq!(auditing.get_violations(100), 1);
    }

    #[test]
    fn test_update_compliance_status() {
        let mut auditing = AuditingSystem::new();
        auditing.add_compliance_requirement(100);
        assert!(auditing.update_compliance_status(100, 1, 500));
    }
}
