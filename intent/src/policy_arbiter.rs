//! Policy Arbiter - Dynamic Resource Allocation
//!
//! Makes decisions about resource allocation based on:
//! - System load (from Conductor)
//! - Intent priority
//! - User context (interactive vs background)
//! - Resource constraints

use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;

/// Policy arbiter manages resource allocation decisions
pub struct PolicyArbiter {
    config: IntentConfig,
    policies: Arc<RwLock<HashMap<String, Policy>>>,
    system_state: Arc<RwLock<SystemState>>,
}

/// Current system state for policy decisions
#[derive(Debug, Clone)]
struct SystemState {
    cpu_usage: f32,
    memory_usage: f32,
    gpu_usage: Option<f32>,
    active_intents: usize,
    load_factor: f32,  // 0.0 = idle, 1.0 = saturated
}

impl Default for SystemState {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            gpu_usage: None,
            active_intents: 0,
            load_factor: 0.0,
        }
    }
}

impl PolicyArbiter {
    /// Create new policy arbiter
    pub fn new(config: IntentConfig) -> Self {
        Self {
            config,
            policies: Arc::new(RwLock::new(HashMap::new())),
            system_state: Arc::new(RwLock::new(SystemState::default())),
        }
    }

    /// Determine resource allocation for an intent
    pub fn allocate(&self, intent: &Intent) -> Policy {
        let system = self.system_state.read();

        // Determine priority based on command type and context
        let priority = self.determine_priority(intent, &system);

        // Calculate resource limits based on priority and system load
        let resource_limits = self.calculate_limits(priority, &system);

        // Generate constraints
        let constraints = self.generate_constraints(intent, priority);

        Policy {
            priority,
            resource_limits,
            constraints,
        }
    }

    /// Determine priority for intent
    fn determine_priority(&self, intent: &Intent, system: &SystemState) -> Priority {
        // User-facing commands are realtime/interactive
        match &intent.command {
            Command::Navigate { .. } => Priority::Realtime,  // UI navigation

            Command::Query { .. } => {
                // Queries are interactive if user-initiated
                if system.active_intents < 5 {
                    Priority::Interactive
                } else {
                    Priority::Normal
                }
            }

            Command::Create { .. } | Command::Execute { .. } => {
                // Creation/execution depends on system load
                if system.load_factor < 0.5 {
                    Priority::Interactive
                } else {
                    Priority::Normal
                }
            }

            Command::Modify { .. } => Priority::Interactive,
            Command::Delete { .. } => Priority::Interactive,

            Command::Configure { .. } => Priority::Normal,

            Command::Sequence { .. } => {
                // Compound commands are lower priority
                if system.load_factor < 0.3 {
                    Priority::Normal
                } else {
                    Priority::Low
                }
            }

            Command::Ambiguous { .. } => Priority::Interactive,  // Need user input
        }
    }

    /// Calculate resource limits based on priority and load
    fn calculate_limits(&self, priority: Priority, system: &SystemState) -> ResourceLimits {
        let base_limits = self.config.default_limits.clone();

        // Adjust based on priority
        let priority_multiplier = match priority {
            Priority::Realtime => 1.5,      // More generous for user-facing
            Priority::Interactive => 1.2,
            Priority::Normal => 1.0,
            Priority::Low => 0.7,
            Priority::Idle => 0.3,
        };

        // Adjust based on system load
        let load_multiplier = if system.load_factor > 0.8 {
            0.5  // Constrain heavily when loaded
        } else if system.load_factor > 0.6 {
            0.75
        } else {
            1.0
        };

        let combined_multiplier = priority_multiplier * load_multiplier;

        ResourceLimits {
            max_cpu_percent: (base_limits.max_cpu_percent * combined_multiplier).min(100.0),
            max_memory_mb: (base_limits.max_memory_mb * combined_multiplier).min(4096.0),
            max_gpu_percent: base_limits.max_gpu_percent.map(|g| (g * combined_multiplier).min(100.0)),
            max_duration_ms: base_limits.max_duration_ms.map(|d| {
                // Lower priority = longer allowed duration
                match priority {
                    Priority::Realtime => d / 2,     // 2.5s
                    Priority::Interactive => d,      // 5s
                    Priority::Normal => d * 2,       // 10s
                    Priority::Low => d * 4,          // 20s
                    Priority::Idle => d * 10,        // 50s
                }
            }),
        }
    }

    /// Generate constraints for intent execution
    fn generate_constraints(&self, intent: &Intent, priority: Priority) -> Vec<Constraint> {
        let mut constraints = Vec::new();

        // Realtime and Interactive tasks should complete quickly
        if priority <= Priority::Interactive {
            let deadline = intent.timestamp + std::time::Duration::from_millis(
                match priority {
                    Priority::Realtime => 16,    // 1 frame
                    Priority::Interactive => 100,
                    _ => 1000,
                }
            );
            constraints.push(Constraint::Deadline { timestamp: deadline });
        }

        // Destructive operations should be reversible
        if matches!(intent.command, Command::Delete { .. } | Command::Modify { .. }) {
            constraints.push(Constraint::Reversible);
        }

        // Untrusted or ambiguous commands should be sandboxed
        if intent.confidence < 0.7 || matches!(intent.command, Command::Ambiguous { .. }) {
            constraints.push(Constraint::Sandboxed);
        }

        // Check for resource requirements
        match &intent.command {
            Command::Execute { action, .. } => {
                // GPU-heavy operations
                if action.contains("render") || action.contains("compute") {
                    constraints.push(Constraint::RequiresResource {
                        resource: "gpu".to_string(),
                    });
                }
            }
            Command::Query { query, .. } => {
                // Large searches might need index
                if query.len() > 100 {
                    constraints.push(Constraint::RequiresResource {
                        resource: "index".to_string(),
                    });
                }
            }
            _ => {}
        }

        constraints
    }

    /// Update system state (called periodically by monitoring)
    pub fn update_system_state(&self, cpu: f32, memory: f32, gpu: Option<f32>, active: usize) {
        let mut state = self.system_state.write();
        state.cpu_usage = cpu;
        state.memory_usage = memory;
        state.gpu_usage = gpu;
        state.active_intents = active;

        // Calculate load factor (0.0 - 1.0)
        let cpu_load = cpu / 100.0;
        let mem_load = memory / 100.0;
        let task_load = (active as f32 / 20.0).min(1.0);  // 20+ tasks = saturated

        state.load_factor = (cpu_load + mem_load + task_load) / 3.0;
    }

    /// Register a custom policy for specific command patterns
    pub fn register_policy(&self, pattern: String, policy: Policy) {
        let mut policies = self.policies.write();
        policies.insert(pattern, policy);
    }

    /// Check if intent should be executed based on policy
    pub fn should_execute(&self, intent: &Intent, policy: &Policy) -> bool {
        // Check confidence threshold
        if intent.confidence < self.config.confidence_threshold {
            return false;  // Needs clarification
        }

        // Check if policy enforcement is enabled
        if !self.config.enforce_policy {
            return true;  // Allow everything
        }

        // Check constraints
        for constraint in &policy.constraints {
            match constraint {
                Constraint::Deadline { timestamp } => {
                    // Can we meet the deadline?
                    let remaining = timestamp.saturating_duration_since(intent.timestamp);
                    if remaining < std::time::Duration::from_millis(10) {
                        return false;  // Too late
                    }
                }
                Constraint::RequiresResource { resource } => {
                    // Check if resource is available
                    if !self.is_resource_available(resource) {
                        return false;
                    }
                }
                _ => {}
            }
        }

        // Check system load
        let system = self.system_state.read();
        if system.load_factor > 0.95 && policy.priority > Priority::Interactive {
            return false;  // System overloaded, only critical tasks
        }

        true
    }

    /// Check if a resource is available
    fn is_resource_available(&self, resource: &str) -> bool {
        let system = self.system_state.read();

        match resource {
            "cpu" => system.cpu_usage < 90.0,
            "memory" => system.memory_usage < 90.0,
            "gpu" => system.gpu_usage.map(|u| u < 90.0).unwrap_or(true),
            "index" => true,  // Assume always available
            _ => true,
        }
    }

    /// Get current system load factor
    pub fn get_load_factor(&self) -> f32 {
        self.system_state.read().load_factor
    }

    /// Get current active intent count
    pub fn get_active_count(&self) -> usize {
        self.system_state.read().active_intents
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn make_intent(command: Command, confidence: f32) -> Intent {
        Intent {
            id: IntentId::new(),
            command,
            context: Context {
                gaze: None,
                audio: None,
                visual_objects: vec![],
                application: None,
                filesystem: None,
                system: SystemContext {
                    cpu_usage: 50.0,
                    memory_usage: 60.0,
                    active_tasks: 10,
                },
            },
            confidence,
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn test_priority_realtime() {
        let arbiter = PolicyArbiter::new(IntentConfig::default());

        let intent = make_intent(
            Command::Navigate {
                destination: "home".to_string(),
            },
            0.95,
        );

        let policy = arbiter.allocate(&intent);
        assert_eq!(policy.priority, Priority::Realtime);
    }

    #[test]
    fn test_resource_limits_scaling() {
        let arbiter = PolicyArbiter::new(IntentConfig::default());

        // Low load
        arbiter.update_system_state(20.0, 30.0, None, 2);
        let intent = make_intent(Command::Query {
            query: "test".to_string(),
            filters: vec![],
        }, 0.9);
        let policy = arbiter.allocate(&intent);

        assert!(policy.resource_limits.max_cpu_percent > 50.0);

        // High load
        arbiter.update_system_state(90.0, 85.0, None, 15);
        let policy2 = arbiter.allocate(&intent);

        assert!(policy2.resource_limits.max_cpu_percent < policy.resource_limits.max_cpu_percent);
    }

    #[test]
    fn test_reversible_constraint() {
        let arbiter = PolicyArbiter::new(IntentConfig::default());

        let intent = make_intent(
            Command::Delete {
                target: Target::Direct {
                    path: "test.rs".into(),
                },
            },
            0.95,
        );

        let policy = arbiter.allocate(&intent);
        assert!(policy.constraints.iter().any(|c| matches!(c, Constraint::Reversible)));
    }

    #[test]
    fn test_low_confidence_sandboxed() {
        let arbiter = PolicyArbiter::new(IntentConfig::default());

        let intent = make_intent(
            Command::Execute {
                action: "unknown".to_string(),
                args: vec![],
            },
            0.5,  // Low confidence
        );

        let policy = arbiter.allocate(&intent);
        assert!(policy.constraints.iter().any(|c| matches!(c, Constraint::Sandboxed)));
    }

    #[test]
    fn test_should_execute_low_confidence() {
        let mut config = IntentConfig::default();
        config.confidence_threshold = 0.8;
        let arbiter = PolicyArbiter::new(config);

        let intent = make_intent(
            Command::Query {
                query: "test".to_string(),
                filters: vec![],
            },
            0.6,  // Below threshold
        );

        let policy = arbiter.allocate(&intent);
        assert!(!arbiter.should_execute(&intent, &policy));
    }

    #[test]
    fn test_load_factor_calculation() {
        let arbiter = PolicyArbiter::new(IntentConfig::default());

        arbiter.update_system_state(50.0, 60.0, Some(40.0), 10);
        let load = arbiter.get_load_factor();

        assert!(load > 0.3 && load < 0.7);
    }
}
