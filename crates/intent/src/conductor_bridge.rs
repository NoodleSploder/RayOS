//! Conductor bridge (Phase 5 → Phase 4)
//!
//! Converts Intent intents/policies into Conductor tasks.
//!
//! This module is feature-gated to avoid pulling the Conductor dependency
//! unless explicitly requested.

use crate::types::{Command, Intent, Policy, Priority};

use std::time::Duration;

/// Convert an Intent policy priority into a Conductor priority.
pub fn map_priority(priority: Priority) -> rayos_conductor::Priority {
    match priority {
        Priority::Realtime => rayos_conductor::Priority::Critical,
        Priority::Interactive => rayos_conductor::Priority::High,
        Priority::Normal => rayos_conductor::Priority::Normal,
        Priority::Low => rayos_conductor::Priority::Low,
        Priority::Idle => rayos_conductor::Priority::Dream,
    }
}

/// Convert a parsed intent into a Conductor task.
///
/// Returns `None` when the intent is inherently not actionable (e.g. ambiguous).
pub fn task_from_intent(intent: &Intent, policy: &Policy) -> Option<rayos_conductor::Task> {
    let priority = map_priority(policy.priority);

    let payload = match &intent.command {
        Command::Query { query, .. } => {
            if is_conversational_query(query) {
                return None;
            }
            rayos_conductor::TaskPayload::Search {
                query: query.clone(),
                limit: 25,
            }
        }

        Command::Create { object_type, properties } => {
            // Best-effort: if it's a file create and we have a name, index it.
            if object_type.to_lowercase().contains("file") {
                if let Some(name) = properties.get("name") {
                    return Some(rayos_conductor::Task::new(
                        priority,
                        rayos_conductor::TaskPayload::IndexFile {
                            path: name.into(),
                        },
                    ));
                }
            }

            rayos_conductor::TaskPayload::Compute {
                name: format!("intent:create:{}", object_type),
                estimated_duration: Duration::from_millis(150),
            }
        }

        Command::Modify { .. } => rayos_conductor::TaskPayload::Compute {
            name: "intent:modify".to_string(),
            estimated_duration: Duration::from_millis(250),
        },

        Command::Delete { .. } => rayos_conductor::TaskPayload::Maintenance {
            task_type: rayos_conductor::MaintenanceType::GarbageCollection,
        },

        Command::Navigate { destination } => rayos_conductor::TaskPayload::Compute {
            name: format!("intent:navigate:{}", destination),
            estimated_duration: Duration::from_millis(50),
        },

        Command::Execute { action, args } => {
            // Chat-like commands are not actionable tasks.
            // (Handled by the host AI reply path instead.)
            if action.trim().eq_ignore_ascii_case("chat") {
                return None;
            }

            rayos_conductor::TaskPayload::Compute {
                name: if args.is_empty() {
                    format!("intent:exec:{}", action)
                } else {
                    format!("intent:exec:{} {}", action, args.join(" "))
                },
                estimated_duration: Duration::from_millis(500),
            }
        }

        Command::Configure { component, .. } => rayos_conductor::TaskPayload::Maintenance {
            task_type: match component.to_lowercase().as_str() {
                "cache" => rayos_conductor::MaintenanceType::CacheFlush,
                "metrics" => rayos_conductor::MaintenanceType::MetricsExport,
                _ => rayos_conductor::MaintenanceType::CacheFlush,
            },
        },

        Command::Sequence { steps } => rayos_conductor::TaskPayload::Compute {
            name: format!("intent:sequence:{}", steps.len()),
            estimated_duration: Duration::from_millis(750),
        },

        Command::Ambiguous { .. } => return None,
    };

    Some(rayos_conductor::Task::new(priority, payload))
}

fn is_conversational_query(query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }

    // Only filter questions about the assistant/system itself.
    q.contains("are you")
        || q.contains("who are you")
        || q.contains("what are you")
        || q.contains("what are you doing")
        || q.contains("status")
        || q.contains("help")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Context, IntentId, SystemContext};
    use std::time::Instant;

    fn empty_context() -> Context {
        Context {
            gaze: None,
            audio: None,
            visual_objects: vec![],
            application: None,
            filesystem: None,
            system: SystemContext {
                cpu_usage: 0.0,
                memory_usage: 0.0,
                active_tasks: 0,
            },
        }
    }

    #[test]
    fn test_priority_mapping() {
        assert_eq!(map_priority(Priority::Realtime), rayos_conductor::Priority::Critical);
        assert_eq!(map_priority(Priority::Interactive), rayos_conductor::Priority::High);
        assert_eq!(map_priority(Priority::Normal), rayos_conductor::Priority::Normal);
        assert_eq!(map_priority(Priority::Low), rayos_conductor::Priority::Low);
        assert_eq!(map_priority(Priority::Idle), rayos_conductor::Priority::Dream);
    }

    #[test]
    fn test_ambiguous_is_not_actionable() {
        let intent = Intent {
            id: IntentId::new(),
            command: Command::Ambiguous {
                possibilities: vec![],
                question: "clarify".to_string(),
            },
            confidence: 0.2,
            context: empty_context(),
            timestamp: Instant::now(),
        };

        let policy = Policy {
            priority: Priority::Interactive,
            resource_limits: Default::default(),
            constraints: vec![],
        };

        assert!(task_from_intent(&intent, &policy).is_none());
    }

    // Keep a small smoke test that touches TaskPayload mapping.
    #[test]
    fn test_query_maps_to_search_task() {
        let intent = Intent {
            id: IntentId::new(),
            command: Command::Query {
                query: "rust files".to_string(),
                filters: vec![],
            },
            confidence: 0.9,
            context: empty_context(),
            timestamp: Instant::now(),
        };

        let policy = Policy {
            priority: Priority::Interactive,
            resource_limits: Default::default(),
            constraints: vec![],
        };

        let task = task_from_intent(&intent, &policy).expect("query should be actionable");
        assert!(matches!(task.payload, rayos_conductor::TaskPayload::Search { .. }));
    }

    #[test]
    fn test_conversational_query_is_not_actionable() {
        let intent = Intent {
            id: IntentId::new(),
            command: Command::Query {
                query: "what are you doing".to_string(),
                filters: vec![],
            },
            confidence: 0.9,
            context: empty_context(),
            timestamp: Instant::now(),
        };

        let policy = Policy {
            priority: Priority::Interactive,
            resource_limits: Default::default(),
            constraints: vec![],
        };

        assert!(task_from_intent(&intent, &policy).is_none());
    }

    #[test]
    fn test_chat_execute_is_not_actionable() {
        let intent = Intent {
            id: IntentId::new(),
            command: Command::Execute {
                action: "chat".to_string(),
                args: vec!["what are you doing".to_string()],
            },
            confidence: 0.9,
            context: empty_context(),
            timestamp: Instant::now(),
        };

        let policy = Policy {
            priority: Priority::Interactive,
            resource_limits: Default::default(),
            constraints: vec![],
        };

        assert!(task_from_intent(&intent, &policy).is_none());
    }
}
