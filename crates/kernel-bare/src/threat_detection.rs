//! Threat Detection & Prevention
//!
//! Real-time anomaly detection, behavioral analysis, and intrusion prevention.
//! 16 detection rules with configurable thresholds and response actions.

#![no_std]

/// Detection rule type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectionRule {
    PrivilegeEscalation,
    MemoryExploit,
    UnauthorizedFileAccess,
    SuspiciousProcessSpawn,
    NetworkAnomalyDetected,
    BufferOverflow,
    UseAfterFree,
    RaceCondition,
    InvalidSyscall,
    PrivilegeAbuse,
    ResourceExhaustion,
    SuspiciousLibLoading,
    StackSmashing,
    FormatStringAttack,
    CommandInjection,
    TimingAttack,
}

/// Threat severity level
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Detection event
#[derive(Clone, Copy)]
pub struct DetectionEvent {
    pub rule: DetectionRule,
    pub severity: Severity,
    pub process_id: u32,
    pub timestamp: u64,
    pub confidence: u8,
}

/// Response action
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseAction {
    Log,
    Alert,
    Isolate,
    Kill,
    Quarantine,
}

/// Behavioral profile
#[derive(Clone, Copy)]
pub struct BehavioralProfile {
    pub process_id: u32,
    pub syscall_count: u32,
    pub memory_allocations: u32,
    pub file_operations: u32,
    pub network_connections: u32,
    pub privilege_level: u8,
    pub anomaly_score: u16,
}

/// Anomaly indicator
#[derive(Clone, Copy)]
pub struct AnomalyIndicator {
    pub indicator_type: u32,
    pub process_id: u32,
    pub value: u32,
    pub threshold: u32,
}

/// Threat detection engine
pub struct ThreatDetector {
    events: [DetectionEvent; 512],
    event_count: u16,
    
    profiles: [BehavioralProfile; 256],
    profile_count: u16,
    
    rules_enabled: [bool; 16],
    rule_thresholds: [u32; 16],
    rule_actions: [ResponseAction; 16],
    
    indicators: [AnomalyIndicator; 128],
    indicator_count: u8,
    
    alerts_generated: u16,
    threats_mitigated: u16,
}

impl ThreatDetector {
    /// Create new threat detector
    pub fn new() -> Self {
        let mut detector = ThreatDetector {
            events: [DetectionEvent {
                rule: DetectionRule::InvalidSyscall,
                severity: Severity::Low,
                process_id: 0,
                timestamp: 0,
                confidence: 0,
            }; 512],
            event_count: 0,
            
            profiles: [BehavioralProfile {
                process_id: 0,
                syscall_count: 0,
                memory_allocations: 0,
                file_operations: 0,
                network_connections: 0,
                privilege_level: 0,
                anomaly_score: 0,
            }; 256],
            profile_count: 0,
            
            rules_enabled: [true; 16],
            rule_thresholds: [100; 16],
            rule_actions: [ResponseAction::Log; 16],
            
            indicators: [AnomalyIndicator {
                indicator_type: 0,
                process_id: 0,
                value: 0,
                threshold: 0,
            }; 128],
            indicator_count: 0,
            
            alerts_generated: 0,
            threats_mitigated: 0,
        };
        
        // Configure rules
        detector.rule_actions[0] = ResponseAction::Kill;  // Privilege escalation
        detector.rule_actions[1] = ResponseAction::Alert; // Memory exploit
        detector.rule_actions[2] = ResponseAction::Alert; // File access
        detector.rule_actions[3] = ResponseAction::Log;   // Process spawn
        detector.rule_actions[4] = ResponseAction::Alert; // Network anomaly
        detector.rule_actions[5] = ResponseAction::Kill;  // Buffer overflow
        detector.rule_actions[6] = ResponseAction::Kill;  // Use after free
        detector.rule_actions[7] = ResponseAction::Isolate; // Race condition
        detector.rule_actions[8] = ResponseAction::Log;   // Invalid syscall
        detector.rule_actions[9] = ResponseAction::Alert; // Privilege abuse
        detector.rule_actions[10] = ResponseAction::Log;  // Resource exhaustion
        detector.rule_actions[11] = ResponseAction::Alert; // Suspicious lib
        detector.rule_actions[12] = ResponseAction::Kill; // Stack smashing
        detector.rule_actions[13] = ResponseAction::Alert; // Format string
        detector.rule_actions[14] = ResponseAction::Kill; // Command injection
        detector.rule_actions[15] = ResponseAction::Alert; // Timing attack
        
        detector
    }
    
    /// Detect threat based on behavior
    pub fn detect_threat(&mut self, rule: DetectionRule, process_id: u32, 
                        confidence: u8) -> Option<ResponseAction> {
        if !self.rules_enabled[rule as usize] {
            return None;
        }
        
        let severity = match rule {
            DetectionRule::PrivilegeEscalation => Severity::Critical,
            DetectionRule::BufferOverflow => Severity::Critical,
            DetectionRule::UseAfterFree => Severity::Critical,
            DetectionRule::StackSmashing => Severity::Critical,
            DetectionRule::CommandInjection => Severity::Critical,
            DetectionRule::MemoryExploit => Severity::High,
            DetectionRule::RaceCondition => Severity::High,
            DetectionRule::PrivilegeAbuse => Severity::High,
            DetectionRule::SuspiciousLibLoading => Severity::High,
            DetectionRule::FormatStringAttack => Severity::High,
            DetectionRule::UnauthorizedFileAccess => Severity::Medium,
            DetectionRule::SuspiciousProcessSpawn => Severity::Medium,
            DetectionRule::NetworkAnomalyDetected => Severity::Medium,
            DetectionRule::InvalidSyscall => Severity::Medium,
            DetectionRule::ResourceExhaustion => Severity::Low,
            DetectionRule::TimingAttack => Severity::Low,
        };
        
        if self.event_count < 512 {
            self.events[self.event_count as usize] = DetectionEvent {
                rule,
                severity,
                process_id,
                timestamp: 0,
                confidence,
            };
            self.event_count += 1;
        }
        
        if severity >= Severity::High {
            self.alerts_generated += 1;
        }
        
        let action = self.rule_actions[rule as usize];
        if action != ResponseAction::Log {
            self.threats_mitigated += 1;
        }
        
        Some(action)
    }
    
    /// Monitor process behavior
    pub fn monitor_process(&mut self, process_id: u32) -> Option<BehavioralProfile> {
        for i in 0..self.profile_count as usize {
            if self.profiles[i].process_id == process_id {
                return Some(self.profiles[i]);
            }
        }
        
        if (self.profile_count as usize) < 256 {
            let profile = BehavioralProfile {
                process_id,
                syscall_count: 0,
                memory_allocations: 0,
                file_operations: 0,
                network_connections: 0,
                privilege_level: 0,
                anomaly_score: 0,
            };
            self.profiles[self.profile_count as usize] = profile;
            self.profile_count += 1;
            Some(profile)
        } else {
            None
        }
    }
    
    /// Record anomaly indicator
    pub fn record_indicator(&mut self, indicator_type: u32, process_id: u32, 
                           value: u32, threshold: u32) -> bool {
        if self.indicator_count >= 128 {
            return false;
        }
        
        self.indicators[self.indicator_count as usize] = AnomalyIndicator {
            indicator_type,
            process_id,
            value,
            threshold,
        };
        self.indicator_count += 1;
        
        // Check if anomalous
        if value > threshold {
            // Update behavioral profile
            for i in 0..self.profile_count as usize {
                if self.profiles[i].process_id == process_id {
                    self.profiles[i].anomaly_score += ((value - threshold) / 10) as u16;
                    break;
                }
            }
        }
        
        true
    }
    
    /// Check if process is suspicious
    pub fn is_process_suspicious(&self, process_id: u32) -> bool {
        for i in 0..self.profile_count as usize {
            if self.profiles[i].process_id == process_id {
                return self.profiles[i].anomaly_score > 500;
            }
        }
        false
    }
    
    /// Update syscall count for process
    pub fn record_syscall(&mut self, process_id: u32) {
        for i in 0..self.profile_count as usize {
            if self.profiles[i].process_id == process_id {
                self.profiles[i].syscall_count += 1;
                break;
            }
        }
    }
    
    /// Update memory operation count
    pub fn record_memory_operation(&mut self, process_id: u32) {
        for i in 0..self.profile_count as usize {
            if self.profiles[i].process_id == process_id {
                self.profiles[i].memory_allocations += 1;
                break;
            }
        }
    }
    
    /// Update file operation count
    pub fn record_file_operation(&mut self, process_id: u32) {
        for i in 0..self.profile_count as usize {
            if self.profiles[i].process_id == process_id {
                self.profiles[i].file_operations += 1;
                break;
            }
        }
    }
    
    /// Update network operation count
    pub fn record_network_operation(&mut self, process_id: u32) {
        for i in 0..self.profile_count as usize {
            if self.profiles[i].process_id == process_id {
                self.profiles[i].network_connections += 1;
                break;
            }
        }
    }
    
    /// Set rule enabled/disabled
    pub fn set_rule_enabled(&mut self, rule: DetectionRule, enabled: bool) {
        self.rules_enabled[rule as usize] = enabled;
    }
    
    /// Get detection event count
    pub fn get_event_count(&self) -> u16 {
        self.event_count
    }
    
    /// Get alerts generated
    pub fn get_alerts_generated(&self) -> u16 {
        self.alerts_generated
    }
    
    /// Get threats mitigated
    pub fn get_threats_mitigated(&self) -> u16 {
        self.threats_mitigated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detector_creation() {
        let detector = ThreatDetector::new();
        assert_eq!(detector.get_event_count(), 0);
    }
    
    #[test]
    fn test_detect_threat() {
        let mut detector = ThreatDetector::new();
        let action = detector.detect_threat(DetectionRule::BufferOverflow, 1001, 95);
        assert!(action.is_some());
    }
    
    #[test]
    fn test_monitor_process() {
        let mut detector = ThreatDetector::new();
        let profile = detector.monitor_process(2001);
        assert!(profile.is_some());
    }
}
