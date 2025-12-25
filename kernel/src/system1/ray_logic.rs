/// Ray-Logic Unit (RLU) - "Logic as Geometry"
///
/// Converts if/else control flow into BVH (Bounding Volume Hierarchy)
/// structures that can be traversed by RT Cores.

use glam::Vec3;

/// A logic node in the BVH tree
#[derive(Debug, Clone)]
pub enum LogicNode {
    /// Branch condition (if/else)
    Branch {
        condition_id: u32,
        true_child: Box<LogicNode>,
        false_child: Box<LogicNode>,
    },
    /// Leaf node (action to execute)
    Leaf {
        action_id: u32,
    },
}

/// BVH representation of logic
#[derive(Debug)]
pub struct LogicBVH {
    /// Root node
    root: LogicNode,
    /// Unique ID for this logic tree
    pub tree_id: u32,
}

impl LogicBVH {
    /// Create a new logic BVH
    pub fn new(tree_id: u32, root: LogicNode) -> Self {
        Self { root, tree_id }
    }

    /// Convert a simple if/else into spatial geometry
    pub fn from_simple_branch(
        tree_id: u32,
        condition_id: u32,
        true_action: u32,
        false_action: u32,
    ) -> Self {
        let root = LogicNode::Branch {
            condition_id,
            true_child: Box::new(LogicNode::Leaf {
                action_id: true_action,
            }),
            false_child: Box::new(LogicNode::Leaf {
                action_id: false_action,
            }),
        };

        Self::new(tree_id, root)
    }

    /// Evaluate the BVH by "tracing" through it
    /// In the real implementation, this would be done by RT Cores
    pub fn trace(&self, state: &[f32]) -> u32 {
        self.trace_node(&self.root, state)
    }

    fn trace_node(&self, node: &LogicNode, state: &[f32]) -> u32 {
        match node {
            LogicNode::Branch {
                condition_id,
                true_child,
                false_child,
            } => {
                // Evaluate condition
                let condition_result = self.evaluate_condition(*condition_id, state);

                if condition_result {
                    self.trace_node(true_child, state)
                } else {
                    self.trace_node(false_child, state)
                }
            }
            LogicNode::Leaf { action_id } => *action_id,
        }
    }

    fn evaluate_condition(&self, condition_id: u32, state: &[f32]) -> bool {
        // Evaluate condition based on state vector
        // This maps to BVH AABB intersection logic

        if state.is_empty() {
            return false;
        }

        let idx = (condition_id as usize) % state.len();
        let value = state[idx];

        // Different condition types based on condition_id
        match condition_id % 8 {
            0 => value > 0.5,                    // Threshold test
            1 => value < 0.5,                    // Inverted threshold
            2 => value.abs() > 0.3,              // Magnitude test
            3 => {
                // Range test
                value > 0.3 && value < 0.7
            }
            4 => {
                // Multi-value AND condition
                let next_idx = (idx + 1) % state.len();
                value > 0.5 && state[next_idx] > 0.5
            }
            5 => {
                // Multi-value OR condition
                let next_idx = (idx + 1) % state.len();
                value > 0.5 || state[next_idx] > 0.5
            }
            6 => {
                // Comparison with average
                let avg: f32 = state.iter().sum::<f32>() / state.len() as f32;
                value > avg
            }
            _ => {
                // Pattern matching: check if value matches expected pattern
                let expected = (condition_id as f32) / 100.0;
                (value - expected).abs() < 0.2
            }
        }
    }
}

/// Builder for constructing logic BVHs
pub struct LogicBVHBuilder {
    next_tree_id: u32,
}

impl LogicBVHBuilder {
    pub fn new() -> Self {
        Self { next_tree_id: 0 }
    }

    /// Build a simple if/else BVH
    pub fn build_if_else(
        &mut self,
        condition_id: u32,
        true_action: u32,
        false_action: u32,
    ) -> LogicBVH {
        let tree_id = self.next_tree_id;
        self.next_tree_id += 1;

        LogicBVH::from_simple_branch(tree_id, condition_id, true_action, false_action)
    }

    /// Build a switch statement BVH (multiple branches)
    pub fn build_switch(
        &mut self,
        conditions: Vec<u32>,
        actions: Vec<u32>,
        default_action: u32,
    ) -> LogicBVH {
        let tree_id = self.next_tree_id;
        self.next_tree_id += 1;

        // Build nested branches
        let root = self.build_switch_node(&conditions, &actions, default_action);

        LogicBVH::new(tree_id, root)
    }

    fn build_switch_node(
        &self,
        conditions: &[u32],
        actions: &[u32],
        default_action: u32,
    ) -> LogicNode {
        if conditions.is_empty() {
            return LogicNode::Leaf {
                action_id: default_action,
            };
        }

        let condition = conditions[0];
        let action = actions[0];

        LogicNode::Branch {
            condition_id: condition,
            true_child: Box::new(LogicNode::Leaf { action_id: action }),
            false_child: Box::new(self.build_switch_node(
                &conditions[1..],
                &actions[1..],
                default_action,
            )),
        }
    }
}

impl Default for LogicBVHBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Converts a ray origin and direction into a spatial query
pub fn ray_to_spatial_query(origin: Vec3, direction: Vec3) -> (Vec3, Vec3) {
    // Normalize direction
    let direction = direction.normalize();

    (origin, direction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_bvh() {
        let bvh = LogicBVH::from_simple_branch(0, 0, 100, 200);

        // State where condition 0 is true
        let state_true = vec![1.0];
        assert_eq!(bvh.trace(&state_true), 100);

        // State where condition 0 is false
        let state_false = vec![0.0];
        assert_eq!(bvh.trace(&state_false), 200);
    }

    #[test]
    fn test_switch_bvh() {
        let mut builder = LogicBVHBuilder::new();
        let bvh = builder.build_switch(
            vec![0, 1, 2],
            vec![100, 200, 300],
            999,
        );

        // Test different conditions
        let state1 = vec![1.0, 0.0, 0.0];
        assert_eq!(bvh.trace(&state1), 100);

        let state2 = vec![0.0, 1.0, 0.0];
        assert_eq!(bvh.trace(&state2), 200);

        let state_default = vec![0.0, 0.0, 0.0];
        assert_eq!(bvh.trace(&state_default), 999);
    }
}
