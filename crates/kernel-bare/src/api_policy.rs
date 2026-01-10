//! Policy Engine & Rule Evaluation
//!
//! Declarative governance rules for API access control.



/// Policy type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyType {
    RateLimit,
    Quota,
    Authentication,
    Authorization,
    Transformation,
}

/// Policy condition
#[derive(Clone, Copy)]
pub struct Condition {
    pub service_id: u32,
    pub user_role: u8,      // 0=Guest, 1=User, 2=ServiceAccount, 3=Admin
    pub time_window: u8,    // 0=always, 1=business hours, 2=night
    pub source_ip: u32,     // 0=any
}

/// Policy action
#[derive(Clone, Copy)]
pub struct Action {
    pub action_type: u8,    // 0=allow, 1=deny, 2=throttle, 3=log, 4=alert
    pub priority: u8,
    pub enabled: bool,
    pub audit_log: bool,
}

/// Policy rule
#[derive(Clone, Copy)]
pub struct PolicyRule {
    pub rule_id: u32,
    pub policy_type: PolicyType,
    pub condition: Condition,
    pub action: Action,
    pub priority: u8,
}

/// Policy decision
#[derive(Clone, Copy)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: u32,        // policy_id that made decision
    pub enforcement_action: u8,
    pub audit_id: u32,
}

/// Policy context
#[derive(Clone, Copy)]
pub struct PolicyContext {
    pub request_id: u32,
    pub service_id: u32,
    pub user_id: u32,
    pub user_role: u8,
    pub source_ip: u32,
}

/// Policy engine
pub struct PolicyEngine {
    rules: [PolicyRule; 128],
    rule_count: u8,

    policies: [u32; 256],  // policy_id mapping

    total_evaluations: u32,
    allow_decisions: u32,
    deny_decisions: u16,
    alerts_triggered: u16,
}

impl PolicyEngine {
    /// Create new policy engine
    pub fn new() -> Self {
        PolicyEngine {
            rules: [PolicyRule {
                rule_id: 0,
                policy_type: PolicyType::Authentication,
                condition: Condition {
                    service_id: 0,
                    user_role: 0,
                    time_window: 0,
                    source_ip: 0,
                },
                action: Action {
                    action_type: 0,
                    priority: 1,
                    enabled: true,
                    audit_log: false,
                },
                priority: 1,
            }; 128],
            rule_count: 0,

            policies: [0; 256],

            total_evaluations: 0,
            allow_decisions: 0,
            deny_decisions: 0,
            alerts_triggered: 0,
        }
    }

    /// Add a policy rule
    pub fn add_policy(&mut self, policy_type: PolicyType, condition: Condition, action: Action) -> Option<u32> {
        if (self.rule_count as usize) >= 128 {
            return None;
        }

        let rule_id = self.rule_count as u32;
        self.rules[self.rule_count as usize] = PolicyRule {
            rule_id,
            policy_type,
            condition,
            action,
            priority: action.priority,
        };

        self.rule_count += 1;
        Some(rule_id)
    }

    /// Remove a policy
    pub fn remove_policy(&mut self, rule_id: u32) -> bool {
        for i in 0..(self.rule_count as usize) {
            if self.rules[i].rule_id == rule_id {
                self.rules.copy_within((i + 1).., i);
                self.rule_count -= 1;
                return true;
            }
        }
        false
    }

    /// Evaluate request through policy chain
    pub fn evaluate_request(&mut self, context: PolicyContext) -> PolicyDecision {
        self.total_evaluations += 1;

        // Find matching rules in priority order
        let mut matching_rule_id = None;
        let mut matching_action = None;

        for i in 0..(self.rule_count as usize) {
            let rule = &self.rules[i];

            // Skip if disabled
            if !rule.action.enabled {
                continue;
            }

            // Check if condition matches
            if self.match_condition(&rule.condition, &context) {
                matching_rule_id = Some(rule.rule_id);
                matching_action = Some(rule.action);
                break;
            }
        }

        if let Some(rule_id) = matching_rule_id {
            if let Some(action) = matching_action {
                return self.apply_action(&action, rule_id);
            }
        }

        // Default allow
        self.allow_decisions += 1;
        PolicyDecision {
            allowed: true,
            reason: 0,
            enforcement_action: 0,
            audit_id: 0,
        }
    }

    /// Test if condition matches
    fn match_condition(&self, condition: &Condition, context: &PolicyContext) -> bool {
        // Match service ID
        if condition.service_id != 0 && condition.service_id != context.service_id {
            return false;
        }

        // Match user role
        if condition.user_role != 0 && condition.user_role != context.user_role {
            return false;
        }

        true
    }

    /// Apply policy action
    fn apply_action(&mut self, action: &Action, rule_id: u32) -> PolicyDecision {
        match action.action_type {
            0 => {  // allow
                self.allow_decisions += 1;
                PolicyDecision {
                    allowed: true,
                    reason: rule_id,
                    enforcement_action: 0,
                    audit_id: 0,
                }
            },
            1 => {  // deny
                self.deny_decisions += 1;
                PolicyDecision {
                    allowed: false,
                    reason: rule_id,
                    enforcement_action: 1,
                    audit_id: 0,
                }
            },
            4 => {  // alert
                self.alerts_triggered += 1;
                PolicyDecision {
                    allowed: true,
                    reason: rule_id,
                    enforcement_action: 4,
                    audit_id: 0,
                }
            },
            _ => {
                PolicyDecision {
                    allowed: true,
                    reason: rule_id,
                    enforcement_action: action.action_type,
                    audit_id: 0,
                }
            }
        }
    }

    /// Get policy statistics
    pub fn get_policy_stats(&self) -> (u32, u32, u16, u16) {
        (self.total_evaluations, self.allow_decisions, self.deny_decisions, self.alerts_triggered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_creation() {
        let mut pe = PolicyEngine::new();
        let condition = Condition {
            service_id: 1,
            user_role: 0,
            time_window: 0,
            source_ip: 0,
        };
        let action = Action {
            action_type: 0,
            priority: 1,
            enabled: true,
            audit_log: false,
        };
        let policy_id = pe.add_policy(PolicyType::Authentication, condition, action);
        assert!(policy_id.is_some());
    }

    #[test]
    fn test_condition_matching() {
        let mut pe = PolicyEngine::new();
        let condition = Condition {
            service_id: 1,
            user_role: 0,
            time_window: 0,
            source_ip: 0,
        };
        let action = Action {
            action_type: 0,
            priority: 1,
            enabled: true,
            audit_log: false,
        };
        pe.add_policy(PolicyType::Authentication, condition, action);

        let context = PolicyContext {
            request_id: 1,
            service_id: 1,
            user_id: 1,
            user_role: 1,
            source_ip: 0,
        };
        let decision = pe.evaluate_request(context);
        assert!(decision.allowed);
    }

    #[test]
    fn test_action_enforcement() {
        let mut pe = PolicyEngine::new();
        let condition = Condition {
            service_id: 1,
            user_role: 0,
            time_window: 0,
            source_ip: 0,
        };
        let action = Action {
            action_type: 1,  // deny
            priority: 1,
            enabled: true,
            audit_log: false,
        };
        pe.add_policy(PolicyType::Authorization, condition, action);

        let context = PolicyContext {
            request_id: 1,
            service_id: 1,
            user_id: 1,
            user_role: 0,
            source_ip: 0,
        };
        let decision = pe.evaluate_request(context);
        assert!(!decision.allowed);
    }
}
