
const MAX_SECURITY_RULES: usize = 256;
const MAX_CAPABILITIES: usize = 64;
const MAX_SECURITY_CONTEXTS: usize = 256;

/// Security level enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SecurityLevel {
    Public,
    Internal,
    Private,
    Isolated,
}

/// Access control policy enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AccessControlPolicy {
    Allow,
    Deny,
    Audit,
}

/// Security context for a process
#[derive(Clone, Copy, Debug)]
pub struct SecurityContext {
    pub context_id: u32,
    pub uid: u32,
    pub gid: u32,
    pub euid: u32,
    pub egid: u32,
    pub security_level: SecurityLevel,
    pub capabilities: u64,
}

impl SecurityContext {
    pub fn new(context_id: u32, uid: u32, gid: u32) -> Self {
        SecurityContext {
            context_id,
            uid,
            gid,
            euid: uid,
            egid: gid,
            security_level: SecurityLevel::Internal,
            capabilities: 0,
        }
    }

    pub fn has_capability(&self, cap_id: u32) -> bool {
        if cap_id >= MAX_CAPABILITIES as u32 {
            return false;
        }
        (self.capabilities & (1u64 << cap_id)) != 0
    }

    pub fn grant_capability(&mut self, cap_id: u32) {
        if cap_id < MAX_CAPABILITIES as u32 {
            self.capabilities |= 1u64 << cap_id;
        }
    }

    pub fn revoke_capability(&mut self, cap_id: u32) {
        if cap_id < MAX_CAPABILITIES as u32 {
            self.capabilities &= !(1u64 << cap_id);
        }
    }
}

/// Capability set for Linux-style capabilities
#[derive(Clone, Copy, Debug)]
pub struct CapabilitySet {
    pub effective_caps: u64,
    pub permitted_caps: u64,
    pub inheritable_caps: u64,
}

impl CapabilitySet {
    pub fn new() -> Self {
        CapabilitySet {
            effective_caps: 0,
            permitted_caps: 0,
            inheritable_caps: 0,
        }
    }

    pub fn grant_effective(&mut self, cap_id: u32) {
        if cap_id < MAX_CAPABILITIES as u32 {
            self.effective_caps |= 1u64 << cap_id;
        }
    }

    pub fn has_effective(&self, cap_id: u32) -> bool {
        if cap_id >= MAX_CAPABILITIES as u32 {
            return false;
        }
        (self.effective_caps & (1u64 << cap_id)) != 0
    }
}

/// Security policy rule
#[derive(Clone, Copy, Debug)]
pub struct SecurityPolicyRule {
    pub rule_id: u32,
    pub source_context: u32,
    pub target_resource: u32,
    pub action: AccessControlPolicy,
    pub enabled: bool,
    pub audit_flag: bool,
}

impl SecurityPolicyRule {
    pub fn new(rule_id: u32, source: u32, target: u32, action: AccessControlPolicy) -> Self {
        SecurityPolicyRule {
            rule_id,
            source_context: source,
            target_resource: target,
            action,
            enabled: true,
            audit_flag: false,
        }
    }
}

/// Security enforcer engine
pub struct SecurityEnforcer {
    rules: [Option<SecurityPolicyRule>; MAX_SECURITY_RULES],
    contexts: [Option<SecurityContext>; MAX_SECURITY_CONTEXTS],
    audit_events: [u32; 128],
    audit_index: usize,
    active_rule_count: u32,
    active_context_count: u32,
    rule_id_counter: u32,
    context_id_counter: u32,
    total_checks: u64,
    denied_count: u64,
}

impl SecurityEnforcer {
    pub fn new() -> Self {
        SecurityEnforcer {
            rules: [None; MAX_SECURITY_RULES],
            contexts: [None; MAX_SECURITY_CONTEXTS],
            audit_events: [0; 128],
            audit_index: 0,
            active_rule_count: 0,
            active_context_count: 0,
            rule_id_counter: 7000,
            context_id_counter: 8000,
            total_checks: 0,
            denied_count: 0,
        }
    }

    pub fn create_security_context(&mut self, uid: u32, gid: u32) -> u32 {
        for i in 0..MAX_SECURITY_CONTEXTS {
            if self.contexts[i].is_none() {
                let context_id = self.context_id_counter;
                self.context_id_counter += 1;
                let context = SecurityContext::new(context_id, uid, gid);
                self.contexts[i] = Some(context);
                self.active_context_count += 1;
                return context_id;
            }
        }
        0
    }

    pub fn get_context(&self, context_id: u32) -> Option<SecurityContext> {
        for i in 0..MAX_SECURITY_CONTEXTS {
            if let Some(ctx) = self.contexts[i] {
                if ctx.context_id == context_id {
                    return Some(ctx);
                }
            }
        }
        None
    }

    pub fn grant_capability(&mut self, context_id: u32, cap_id: u32) -> bool {
        for i in 0..MAX_SECURITY_CONTEXTS {
            if let Some(mut ctx) = self.contexts[i] {
                if ctx.context_id == context_id {
                    ctx.grant_capability(cap_id);
                    self.contexts[i] = Some(ctx);
                    return true;
                }
            }
        }
        false
    }

    pub fn add_policy_rule(
        &mut self,
        source: u32,
        target: u32,
        action: AccessControlPolicy,
    ) -> u32 {
        for i in 0..MAX_SECURITY_RULES {
            if self.rules[i].is_none() {
                let rule_id = self.rule_id_counter;
                self.rule_id_counter += 1;
                let rule = SecurityPolicyRule::new(rule_id, source, target, action);
                self.rules[i] = Some(rule);
                self.active_rule_count += 1;
                return rule_id;
            }
        }
        0
    }

    pub fn check_access(&mut self, source_context: u32, target_resource: u32) -> bool {
        self.total_checks += 1;

        for i in 0..MAX_SECURITY_RULES {
            if let Some(rule) = self.rules[i] {
                if rule.enabled && rule.source_context == source_context
                    && rule.target_resource == target_resource
                {
                    match rule.action {
                        AccessControlPolicy::Allow => return true,
                        AccessControlPolicy::Deny => {
                            self.denied_count += 1;
                            self.record_audit_event(rule.rule_id);
                            return false;
                        }
                        AccessControlPolicy::Audit => {
                            self.record_audit_event(rule.rule_id);
                            return true;
                        }
                    }
                }
            }
        }

        true
    }

    pub fn remove_policy_rule(&mut self, rule_id: u32) -> bool {
        for i in 0..MAX_SECURITY_RULES {
            if let Some(rule) = self.rules[i] {
                if rule.rule_id == rule_id {
                    self.rules[i] = None;
                    self.active_rule_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    fn record_audit_event(&mut self, event_id: u32) {
        self.audit_events[self.audit_index] = event_id;
        self.audit_index = (self.audit_index + 1) % 128;
    }

    pub fn enforce_mandatory_access_control(
        &self,
        source_level: SecurityLevel,
        target_level: SecurityLevel,
    ) -> bool {
        match (source_level, target_level) {
            (SecurityLevel::Isolated, _) => false,
            (_, SecurityLevel::Isolated) => false,
            (SecurityLevel::Private, SecurityLevel::Public) => false,
            (SecurityLevel::Internal, SecurityLevel::Private) => false,
            _ => true,
        }
    }

    pub fn get_active_rule_count(&self) -> u32 {
        self.active_rule_count
    }

    pub fn get_active_context_count(&self) -> u32 {
        self.active_context_count
    }

    pub fn get_total_checks(&self) -> u64 {
        self.total_checks
    }

    pub fn get_denied_count(&self) -> u64 {
        self.denied_count
    }

    pub fn get_denial_rate(&self) -> u32 {
        if self.total_checks == 0 {
            0
        } else {
            ((self.denied_count * 100) / self.total_checks) as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_security_context() {
        let mut enforcer = SecurityEnforcer::new();
        let ctx_id = enforcer.create_security_context(1000, 1000);
        assert!(ctx_id > 0);
        assert_eq!(enforcer.get_active_context_count(), 1);
    }

    #[test]
    fn test_capability_management() {
        let mut enforcer = SecurityEnforcer::new();
        let ctx_id = enforcer.create_security_context(1000, 1000);

        assert!(enforcer.grant_capability(ctx_id, 1));

        let ctx = enforcer.get_context(ctx_id);
        assert!(ctx.is_some());
        assert!(ctx.unwrap().has_capability(1));
        assert!(!ctx.unwrap().has_capability(2));
    }

    #[test]
    fn test_add_policy_rule() {
        let mut enforcer = SecurityEnforcer::new();
        let rule_id = enforcer.add_policy_rule(1, 100, AccessControlPolicy::Allow);
        assert!(rule_id > 0);
        assert_eq!(enforcer.get_active_rule_count(), 1);
    }

    #[test]
    fn test_access_check_allow() {
        let mut enforcer = SecurityEnforcer::new();
        enforcer.add_policy_rule(1, 100, AccessControlPolicy::Allow);
        assert!(enforcer.check_access(1, 100));
    }

    #[test]
    fn test_access_check_deny() {
        let mut enforcer = SecurityEnforcer::new();
        enforcer.add_policy_rule(1, 100, AccessControlPolicy::Deny);
        assert!(!enforcer.check_access(1, 100));
        assert!(enforcer.get_denied_count() > 0);
    }

    #[test]
    fn test_remove_policy_rule() {
        let mut enforcer = SecurityEnforcer::new();
        let rule_id = enforcer.add_policy_rule(1, 100, AccessControlPolicy::Deny);
        assert!(enforcer.remove_policy_rule(rule_id));
        assert_eq!(enforcer.get_active_rule_count(), 0);
    }

    #[test]
    fn test_mandatory_access_control() {
        let enforcer = SecurityEnforcer::new();

        assert!(!enforcer.enforce_mandatory_access_control(
            SecurityLevel::Isolated,
            SecurityLevel::Public
        ));

        assert!(!enforcer.enforce_mandatory_access_control(
            SecurityLevel::Private,
            SecurityLevel::Public
        ));

        assert!(enforcer.enforce_mandatory_access_control(
            SecurityLevel::Public,
            SecurityLevel::Public
        ));
    }

    #[test]
    fn test_denial_rate() {
        let mut enforcer = SecurityEnforcer::new();
        enforcer.add_policy_rule(1, 100, AccessControlPolicy::Deny);

        enforcer.check_access(1, 100);
        enforcer.check_access(2, 100);
        enforcer.check_access(3, 100);

        assert!(enforcer.get_denial_rate() > 0);
    }
}
