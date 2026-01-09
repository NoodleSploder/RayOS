//! RT Core Logic Encoding - Conditionals as Ray-Geometry Intersections
//!
//! This module implements the core "Logic as Geometry" paradigm where
//! logical operations are encoded as ray-geometry intersections for
//! massively parallel GPU execution.
//!
//! ## Core Concepts
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Logic Encoding Scene                            │
//! │                                                                         │
//! │   State Space (3D)                                                      │
//! │   ───────────────                                                       │
//! │                    ┌───────────┐                                        │
//! │                    │ Condition │   Hit = true                           │
//! │      ────────→     │  Sphere   │   Miss = false                         │
//! │      Logic Ray     │           │                                        │
//! │                    └───────────┘                                        │
//! │                                                                         │
//! │   Compound conditions:                                                  │
//! │   ┌─────┐                                                               │
//! │   │ AND │ = Intersection of geometry                                    │
//! │   ├─────┤                                                               │
//! │   │ OR  │ = Union of geometry (any hit)                                 │
//! │   ├─────┤                                                               │
//! │   │ NOT │ = Inverted geometry (miss = hit)                              │
//! │   └─────┘                                                               │
//! │                                                                         │
//! │   Ray encodes:                                                          │
//! │   - Origin = Current state                                              │
//! │   - Direction = Operation/query type                                    │
//! │   - Payload = Context data (variable values)                            │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_kernel::geometry_logic::logic_encoding::*;
//!
//! // Create a condition: x > 5
//! let cond = ConditionGeometry::threshold("x", ThresholdOp::Greater, 5.0);
//!
//! // Create a logic scene
//! let mut scene = LogicScene::new();
//! scene.add_condition(0, cond);
//!
//! // Cast logic rays
//! let ray = LogicRay::new([5.5, 0.0, 0.0]); // x=5.5
//! let results = scene.evaluate(&device, &queue, &[ray]).await?;
//! assert!(results[0].hit); // 5.5 > 5 is true
//! ```

use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

// =============================================================================
// Configuration
// =============================================================================

/// Maximum number of conditions in a scene
pub const MAX_CONDITIONS: usize = 4096;

/// Maximum number of logic rays per dispatch
pub const MAX_LOGIC_RAYS: usize = 65536;

/// Maximum depth for compound conditions (AND/OR trees)
pub const MAX_CONDITION_DEPTH: usize = 16;

/// Workgroup size for compute shader
pub const WORKGROUP_SIZE: u32 = 256;

// =============================================================================
// GPU Shader
// =============================================================================

/// WGSL compute shader for logic encoding
pub const LOGIC_ENCODING_SHADER: &str = r#"
// RT Core Logic Encoding Shader
//
// Encodes logical conditions as geometry and evaluates them via ray intersection.
// Each condition is a geometric primitive in 3D "state space" where:
// - X, Y, Z coordinates represent variable values
// - Rays cast through the scene test conditions
// - Hits indicate condition is true, misses indicate false

// 3D vector for state space
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

// Axis-aligned bounding box for spatial tests
struct AABB {
    min: Vec3,
    max: Vec3,
}

// Sphere for range/threshold tests
struct Sphere {
    center: Vec3,
    radius: f32,
}

// Condition types
const COND_NONE: u32 = 0u;
const COND_THRESHOLD_GT: u32 = 1u;   // value > threshold
const COND_THRESHOLD_GE: u32 = 2u;   // value >= threshold
const COND_THRESHOLD_LT: u32 = 3u;   // value < threshold
const COND_THRESHOLD_LE: u32 = 4u;   // value <= threshold
const COND_THRESHOLD_EQ: u32 = 5u;   // value == threshold (with epsilon)
const COND_RANGE: u32 = 6u;          // min <= value <= max
const COND_SPHERE: u32 = 7u;         // distance from center < radius
const COND_AABB: u32 = 8u;           // point inside box
const COND_PLANE: u32 = 9u;          // point on positive side of plane
const COND_AND: u32 = 10u;           // compound: both children true
const COND_OR: u32 = 11u;            // compound: either child true
const COND_NOT: u32 = 12u;           // compound: child is false
const COND_XOR: u32 = 13u;           // compound: exactly one child true
const COND_PATTERN: u32 = 14u;       // bitmask pattern match
const COND_LOOKUP: u32 = 15u;        // lookup table decision

// A condition encoded as geometry
struct Condition {
    id: u32,
    cond_type: u32,
    // For thresholds: which axis (0=x, 1=y, 2=z) and threshold value
    axis: u32,
    threshold: f32,
    // For ranges/boxes: min/max bounds
    min_bound: Vec3,
    max_bound: Vec3,
    // For spheres: center and radius
    center: Vec3,
    radius: f32,
    // For compound conditions: child indices
    child_a: u32,
    child_b: u32,
    // Flags: 1=inverted, 2=enabled, 4=shortcut
    flags: u32,
    // For pattern matching: expected pattern and mask
    pattern: u32,
    mask: u32,
    _pad0: u32,
}

// A logic ray to evaluate conditions
struct LogicRay {
    id: u32,
    // State vector (point in state space)
    state: Vec3,
    // Optional: direction for ray-casting (for BVH traversal)
    direction: Vec3,
    // Optional: additional state as bitmask
    state_bits: u32,
    // Which condition to evaluate (0 = all root conditions)
    target_condition: u32,
    _pad0: u32,
    _pad1: u32,
}

// Result of logic evaluation
struct LogicResult {
    ray_id: u32,
    condition_id: u32,
    // 0 = false, 1 = true
    result: u32,
    // Distance to condition boundary (useful for fuzzy logic)
    distance: f32,
    // Number of conditions evaluated (for profiling)
    evaluations: u32,
    // Depth in condition tree
    depth: u32,
    _pad0: u32,
    _pad1: u32,
}

// Scene configuration
struct Config {
    num_conditions: u32,
    num_rays: u32,
    root_condition: u32,  // Starting condition for evaluation
    max_depth: u32,
    epsilon: f32,         // For floating point comparisons
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

// Buffers
@group(0) @binding(0) var<storage, read> conditions: array<Condition, 4096>;
@group(0) @binding(1) var<storage, read> rays: array<LogicRay, 65536>;
@group(0) @binding(2) var<storage, read> config: Config;
@group(0) @binding(3) var<storage, read_write> results: array<LogicResult, 65536>;

// Helper functions
fn vec3_new(x: f32, y: f32, z: f32) -> Vec3 {
    return Vec3(x, y, z);
}

fn get_axis_value(state: Vec3, axis: u32) -> f32 {
    switch (axis) {
        case 0u: { return state.x; }
        case 1u: { return state.y; }
        case 2u: { return state.z; }
        default: { return 0.0; }
    }
}

fn point_in_aabb(point: Vec3, min_b: Vec3, max_b: Vec3) -> bool {
    return point.x >= min_b.x && point.x <= max_b.x &&
           point.y >= min_b.y && point.y <= max_b.y &&
           point.z >= min_b.z && point.z <= max_b.z;
}

fn distance_to_sphere(point: Vec3, center: Vec3, radius: f32) -> f32 {
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    let dz = point.z - center.z;
    return sqrt(dx*dx + dy*dy + dz*dz) - radius;
}

fn plane_distance(point: Vec3, normal: Vec3, d: f32) -> f32 {
    return normal.x * point.x + normal.y * point.y + normal.z * point.z - d;
}

// Evaluate a single condition (non-recursive leaf)
fn evaluate_leaf(cond: Condition, state: Vec3, state_bits: u32, epsilon: f32) -> bool {
    switch (cond.cond_type) {
        case COND_THRESHOLD_GT: {
            let val = get_axis_value(state, cond.axis);
            return val > cond.threshold;
        }
        case COND_THRESHOLD_GE: {
            let val = get_axis_value(state, cond.axis);
            return val >= cond.threshold - epsilon;
        }
        case COND_THRESHOLD_LT: {
            let val = get_axis_value(state, cond.axis);
            return val < cond.threshold;
        }
        case COND_THRESHOLD_LE: {
            let val = get_axis_value(state, cond.axis);
            return val <= cond.threshold + epsilon;
        }
        case COND_THRESHOLD_EQ: {
            let val = get_axis_value(state, cond.axis);
            return abs(val - cond.threshold) <= epsilon;
        }
        case COND_RANGE: {
            let val = get_axis_value(state, cond.axis);
            return val >= cond.min_bound.x && val <= cond.max_bound.x;
        }
        case COND_SPHERE: {
            let dist = distance_to_sphere(state, cond.center, cond.radius);
            return dist <= 0.0;
        }
        case COND_AABB: {
            return point_in_aabb(state, cond.min_bound, cond.max_bound);
        }
        case COND_PLANE: {
            // Normal stored in center, d in radius
            let dist = plane_distance(state, cond.center, cond.radius);
            return dist >= 0.0;
        }
        case COND_PATTERN: {
            return (state_bits & cond.mask) == cond.pattern;
        }
        default: {
            return false;
        }
    }
}

// Evaluate condition tree (iterative with manual stack to avoid recursion limits)
fn evaluate_condition(start_idx: u32, state: Vec3, state_bits: u32, epsilon: f32, max_depth: u32) -> bool {
    // Manual stack for iterative tree traversal
    var stack_idx: array<u32, 16>;  // Condition indices
    var stack_state: array<u32, 16>;  // 0=need eval, 1=have left, 2=have both
    var stack_left: array<bool, 16>;  // Left result
    var stack_right: array<bool, 16>; // Right result
    var sp: u32 = 0u;

    stack_idx[0] = start_idx;
    stack_state[0] = 0u;

    var final_result = false;
    var evaluations = 0u;

    loop {
        if (sp >= max_depth || evaluations > 1000u) {
            break;
        }

        let idx = stack_idx[sp];
        let cond = conditions[idx];
        evaluations = evaluations + 1u;

        // Check if this is a compound condition
        let is_compound = cond.cond_type >= COND_AND && cond.cond_type <= COND_NOT;

        if (!is_compound) {
            // Leaf condition - evaluate directly
            var result = evaluate_leaf(cond, state, state_bits, epsilon);

            // Apply inversion if flagged
            if ((cond.flags & 1u) != 0u) {
                result = !result;
            }

            if (sp == 0u) {
                final_result = result;
                break;
            }

            // Pop and propagate result up
            sp = sp - 1u;
            if (stack_state[sp] == 0u) {
                stack_left[sp] = result;
                stack_state[sp] = 1u;
                // If we need right child, push it
                let parent = conditions[stack_idx[sp]];
                if (parent.cond_type != COND_NOT) {
                    sp = sp + 1u;
                    stack_idx[sp] = parent.child_b;
                    stack_state[sp] = 0u;
                } else {
                    // NOT only has one child, result is inverse
                    final_result = !result;
                    if (sp == 0u) {
                        break;
                    }
                }
            } else {
                stack_right[sp] = result;
                // Combine results based on parent operator
                let parent = conditions[stack_idx[sp]];
                var combined = false;
                switch (parent.cond_type) {
                    case COND_AND: { combined = stack_left[sp] && stack_right[sp]; }
                    case COND_OR: { combined = stack_left[sp] || stack_right[sp]; }
                    case COND_XOR: { combined = stack_left[sp] != stack_right[sp]; }
                    default: { combined = false; }
                }
                if ((parent.flags & 1u) != 0u) {
                    combined = !combined;
                }
                if (sp == 0u) {
                    final_result = combined;
                    break;
                }
                // Continue propagating up
                sp = sp - 1u;
                if (stack_state[sp] == 0u) {
                    stack_left[sp] = combined;
                    stack_state[sp] = 1u;
                } else {
                    stack_right[sp] = combined;
                }
            }
        } else {
            // Compound condition - push left child
            if (stack_state[sp] == 0u) {
                sp = sp + 1u;
                stack_idx[sp] = cond.child_a;
                stack_state[sp] = 0u;
            }
        }
    }

    return final_result;
}

// Main compute shader entry point
@compute @workgroup_size(256)
fn logic_eval_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let ray_idx = global_id.x;

    if (ray_idx >= config.num_rays) {
        return;
    }

    let ray = rays[ray_idx];

    // Determine which condition to evaluate
    var target = ray.target_condition;
    if (target == 0u) {
        target = config.root_condition;
    }

    // Evaluate the condition tree
    let result = evaluate_condition(target, ray.state, ray.state_bits, config.epsilon, config.max_depth);

    // Calculate distance to nearest boundary (for fuzzy logic / gradients)
    var distance = 0.0;
    let cond = conditions[target];
    if (cond.cond_type == COND_SPHERE) {
        distance = distance_to_sphere(ray.state, cond.center, cond.radius);
    } else if (cond.cond_type >= COND_THRESHOLD_GT && cond.cond_type <= COND_THRESHOLD_EQ) {
        let val = get_axis_value(ray.state, cond.axis);
        distance = val - cond.threshold;
    }

    results[ray_idx] = LogicResult(
        ray.id,
        target,
        select(0u, 1u, result),
        distance,
        1u,  // evaluations (simplified)
        1u,  // depth (simplified)
        0u,
        0u
    );
}
"#;

// =============================================================================
// Rust Types
// =============================================================================

/// Threshold comparison operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdOp {
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Equal,
}

impl ThresholdOp {
    fn to_cond_type(&self) -> u32 {
        match self {
            ThresholdOp::Greater => 1,
            ThresholdOp::GreaterEqual => 2,
            ThresholdOp::Less => 3,
            ThresholdOp::LessEqual => 4,
            ThresholdOp::Equal => 5,
        }
    }
}

/// Compound logical operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    And,
    Or,
    Not,
    Xor,
}

impl CompoundOp {
    fn to_cond_type(&self) -> u32 {
        match self {
            CompoundOp::And => 10,
            CompoundOp::Or => 11,
            CompoundOp::Not => 12,
            CompoundOp::Xor => 13,
        }
    }
}

/// A point in 3D state space
#[derive(Debug, Clone, Copy, Default)]
pub struct StateVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl StateVec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn from_array(arr: [f32; 3]) -> Self {
        Self { x: arr[0], y: arr[1], z: arr[2] }
    }
}

/// Condition geometry for logical tests
#[derive(Debug, Clone)]
pub enum ConditionGeometry {
    /// Threshold test: axis_value op threshold
    Threshold {
        axis: usize,  // 0=x, 1=y, 2=z
        op: ThresholdOp,
        value: f32,
    },
    /// Range test: min <= axis_value <= max
    Range {
        axis: usize,
        min: f32,
        max: f32,
    },
    /// Sphere test: distance from center < radius
    Sphere {
        center: StateVec3,
        radius: f32,
    },
    /// AABB test: point inside box
    Box {
        min: StateVec3,
        max: StateVec3,
    },
    /// Plane test: point on positive side
    Plane {
        normal: StateVec3,
        distance: f32,
    },
    /// Pattern match: (bits & mask) == pattern
    Pattern {
        pattern: u32,
        mask: u32,
    },
    /// Compound: combines two conditions
    Compound {
        op: CompoundOp,
        left: Box<ConditionGeometry>,
        right: Option<Box<ConditionGeometry>>,  // None for NOT
    },
}

impl ConditionGeometry {
    /// Create a threshold condition
    pub fn threshold(axis: usize, op: ThresholdOp, value: f32) -> Self {
        ConditionGeometry::Threshold { axis, op, value }
    }

    /// Create x > value condition
    pub fn x_gt(value: f32) -> Self {
        Self::threshold(0, ThresholdOp::Greater, value)
    }

    /// Create y > value condition
    pub fn y_gt(value: f32) -> Self {
        Self::threshold(1, ThresholdOp::Greater, value)
    }

    /// Create z > value condition
    pub fn z_gt(value: f32) -> Self {
        Self::threshold(2, ThresholdOp::Greater, value)
    }

    /// Create a range condition
    pub fn range(axis: usize, min: f32, max: f32) -> Self {
        ConditionGeometry::Range { axis, min, max }
    }

    /// Create a sphere condition
    pub fn sphere(center: [f32; 3], radius: f32) -> Self {
        ConditionGeometry::Sphere {
            center: StateVec3::from_array(center),
            radius,
        }
    }

    /// Create a box condition
    pub fn aabb(min: [f32; 3], max: [f32; 3]) -> Self {
        ConditionGeometry::Box {
            min: StateVec3::from_array(min),
            max: StateVec3::from_array(max),
        }
    }

    /// Create an AND condition
    pub fn and(left: ConditionGeometry, right: ConditionGeometry) -> Self {
        ConditionGeometry::Compound {
            op: CompoundOp::And,
            left: Box::new(left),
            right: Some(Box::new(right)),
        }
    }

    /// Create an OR condition
    pub fn or(left: ConditionGeometry, right: ConditionGeometry) -> Self {
        ConditionGeometry::Compound {
            op: CompoundOp::Or,
            left: Box::new(left),
            right: Some(Box::new(right)),
        }
    }

    /// Create a NOT condition
    pub fn not(inner: ConditionGeometry) -> Self {
        ConditionGeometry::Compound {
            op: CompoundOp::Not,
            left: Box::new(inner),
            right: None,
        }
    }

    /// Create an XOR condition
    pub fn xor(left: ConditionGeometry, right: ConditionGeometry) -> Self {
        ConditionGeometry::Compound {
            op: CompoundOp::Xor,
            left: Box::new(left),
            right: Some(Box::new(right)),
        }
    }

    /// Create a pattern match condition
    pub fn pattern(pattern: u32, mask: u32) -> Self {
        ConditionGeometry::Pattern { pattern, mask }
    }

    /// Evaluate condition on CPU (for testing/fallback)
    pub fn evaluate(&self, state: &StateVec3, state_bits: u32) -> bool {
        match self {
            ConditionGeometry::Threshold { axis, op, value } => {
                let v = match axis {
                    0 => state.x,
                    1 => state.y,
                    2 => state.z,
                    _ => 0.0,
                };
                match op {
                    ThresholdOp::Greater => v > *value,
                    ThresholdOp::GreaterEqual => v >= *value,
                    ThresholdOp::Less => v < *value,
                    ThresholdOp::LessEqual => v <= *value,
                    ThresholdOp::Equal => (v - *value).abs() < 1e-6,
                }
            }
            ConditionGeometry::Range { axis, min, max } => {
                let v = match axis {
                    0 => state.x,
                    1 => state.y,
                    2 => state.z,
                    _ => 0.0,
                };
                v >= *min && v <= *max
            }
            ConditionGeometry::Sphere { center, radius } => {
                let dx = state.x - center.x;
                let dy = state.y - center.y;
                let dz = state.z - center.z;
                (dx * dx + dy * dy + dz * dz).sqrt() <= *radius
            }
            ConditionGeometry::Box { min, max } => {
                state.x >= min.x && state.x <= max.x &&
                state.y >= min.y && state.y <= max.y &&
                state.z >= min.z && state.z <= max.z
            }
            ConditionGeometry::Plane { normal, distance } => {
                let d = normal.x * state.x + normal.y * state.y + normal.z * state.z;
                d >= *distance
            }
            ConditionGeometry::Pattern { pattern, mask } => {
                (state_bits & mask) == *pattern
            }
            ConditionGeometry::Compound { op, left, right } => {
                let l = left.evaluate(state, state_bits);
                match op {
                    CompoundOp::Not => !l,
                    CompoundOp::And => l && right.as_ref().map(|r| r.evaluate(state, state_bits)).unwrap_or(true),
                    CompoundOp::Or => l || right.as_ref().map(|r| r.evaluate(state, state_bits)).unwrap_or(false),
                    CompoundOp::Xor => l ^ right.as_ref().map(|r| r.evaluate(state, state_bits)).unwrap_or(false),
                }
            }
        }
    }
}

/// A logic ray for evaluating conditions
#[derive(Debug, Clone)]
pub struct LogicRay {
    /// Unique ID
    pub id: u64,
    /// State vector (position in state space)
    pub state: StateVec3,
    /// Optional direction for ray-casting
    pub direction: StateVec3,
    /// Additional state as bitmask
    pub state_bits: u32,
    /// Target condition to evaluate (0 = root)
    pub target_condition: u32,
}

impl LogicRay {
    /// Create a new logic ray
    pub fn new(state: [f32; 3]) -> Self {
        Self {
            id: 0,
            state: StateVec3::from_array(state),
            direction: StateVec3::default(),
            state_bits: 0,
            target_condition: 0,
        }
    }

    /// Create with state bits
    pub fn with_bits(state: [f32; 3], bits: u32) -> Self {
        Self {
            id: 0,
            state: StateVec3::from_array(state),
            direction: StateVec3::default(),
            state_bits: bits,
            target_condition: 0,
        }
    }
}

/// Result of logic evaluation
#[derive(Debug, Clone)]
pub struct LogicResult {
    /// Ray ID
    pub ray_id: u64,
    /// Condition that was evaluated
    pub condition_id: u32,
    /// Whether condition was true
    pub result: bool,
    /// Distance to condition boundary (for gradients)
    pub distance: f32,
    /// Number of conditions evaluated
    pub evaluations: u32,
    /// Depth in condition tree
    pub depth: u32,
}

/// Statistics for logic evaluation
#[derive(Debug, Default)]
pub struct LogicStats {
    pub total_evaluations: AtomicU64,
    pub total_rays: AtomicU64,
    pub cache_hits: AtomicU64,
    pub gpu_dispatches: AtomicU64,
}

/// GPU-accelerated logic scene for parallel condition evaluation
pub struct LogicScene {
    /// Conditions in the scene
    conditions: Vec<ConditionGeometry>,
    /// Flattened GPU-compatible conditions
    gpu_conditions: Vec<GpuCondition>,
    /// Map from condition index to GPU index
    condition_map: HashMap<u32, u32>,
    /// Root condition index
    root_condition: u32,
    /// Statistics
    stats: Arc<LogicStats>,
    /// Epsilon for floating point comparisons
    epsilon: f32,
    /// Whether GPU buffers need rebuild
    dirty: bool,
}

/// GPU-compatible condition representation (matches shader struct)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuCondition {
    pub id: u32,
    pub cond_type: u32,
    pub axis: u32,
    pub threshold: f32,
    pub min_bound: [f32; 3],
    pub _pad0: f32,
    pub max_bound: [f32; 3],
    pub _pad1: f32,
    pub center: [f32; 3],
    pub radius: f32,
    pub child_a: u32,
    pub child_b: u32,
    pub flags: u32,
    pub pattern: u32,
    pub mask: u32,
    pub _pad2: u32,
    pub _pad3: u32,
    pub _pad4: u32,
}

impl LogicScene {
    /// Create a new empty logic scene
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
            gpu_conditions: Vec::new(),
            condition_map: HashMap::new(),
            root_condition: 0,
            stats: Arc::new(LogicStats::default()),
            epsilon: 1e-6,
            dirty: true,
        }
    }

    /// Set the epsilon for floating point comparisons
    pub fn set_epsilon(&mut self, epsilon: f32) {
        self.epsilon = epsilon;
    }

    /// Add a condition to the scene
    pub fn add_condition(&mut self, id: u32, condition: ConditionGeometry) -> u32 {
        let idx = self.conditions.len() as u32;
        self.conditions.push(condition);
        self.condition_map.insert(id, idx);
        self.dirty = true;
        idx
    }

    /// Set the root condition for evaluation
    pub fn set_root(&mut self, condition_id: u32) {
        if let Some(&idx) = self.condition_map.get(&condition_id) {
            self.root_condition = idx;
        }
    }

    /// Flatten conditions for GPU
    fn flatten_conditions(&mut self) {
        self.gpu_conditions.clear();

        for (idx, cond) in self.conditions.iter().enumerate() {
            let gpu_cond = self.condition_to_gpu(idx as u32, cond);
            self.gpu_conditions.push(gpu_cond);
        }

        self.dirty = false;
    }

    fn condition_to_gpu(&self, id: u32, cond: &ConditionGeometry) -> GpuCondition {
        let mut gpu = GpuCondition::default();
        gpu.id = id;

        match cond {
            ConditionGeometry::Threshold { axis, op, value } => {
                gpu.cond_type = op.to_cond_type();
                gpu.axis = *axis as u32;
                gpu.threshold = *value;
            }
            ConditionGeometry::Range { axis, min, max } => {
                gpu.cond_type = 6; // COND_RANGE
                gpu.axis = *axis as u32;
                gpu.min_bound[0] = *min;
                gpu.max_bound[0] = *max;
            }
            ConditionGeometry::Sphere { center, radius } => {
                gpu.cond_type = 7; // COND_SPHERE
                gpu.center = [center.x, center.y, center.z];
                gpu.radius = *radius;
            }
            ConditionGeometry::Box { min, max } => {
                gpu.cond_type = 8; // COND_AABB
                gpu.min_bound = [min.x, min.y, min.z];
                gpu.max_bound = [max.x, max.y, max.z];
            }
            ConditionGeometry::Plane { normal, distance } => {
                gpu.cond_type = 9; // COND_PLANE
                gpu.center = [normal.x, normal.y, normal.z];
                gpu.radius = *distance;
            }
            ConditionGeometry::Pattern { pattern, mask } => {
                gpu.cond_type = 14; // COND_PATTERN
                gpu.pattern = *pattern;
                gpu.mask = *mask;
            }
            ConditionGeometry::Compound { op, left, right } => {
                gpu.cond_type = op.to_cond_type();
                // Note: for compound conditions, children need to be added separately
                // and their indices stored here
                gpu.child_a = 0; // Would be set during full flattening
                gpu.child_b = 0;
            }
        }

        gpu
    }

    /// Evaluate conditions on CPU (for testing or when GPU unavailable)
    pub fn evaluate_cpu(&self, rays: &[LogicRay]) -> Vec<LogicResult> {
        rays.iter()
            .map(|ray| {
                let cond_idx = if ray.target_condition == 0 {
                    self.root_condition as usize
                } else {
                    ray.target_condition as usize
                };

                let result = if cond_idx < self.conditions.len() {
                    self.conditions[cond_idx].evaluate(&ray.state, ray.state_bits)
                } else {
                    false
                };

                LogicResult {
                    ray_id: ray.id,
                    condition_id: cond_idx as u32,
                    result,
                    distance: 0.0,
                    evaluations: 1,
                    depth: 1,
                }
            })
            .collect()
    }

    /// Get statistics
    pub fn stats(&self) -> &LogicStats {
        &self.stats
    }

    /// Number of conditions in the scene
    pub fn len(&self) -> usize {
        self.conditions.len()
    }

    /// Is the scene empty?
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }
}

impl Default for LogicScene {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Builder Pattern for Complex Conditions
// =============================================================================

/// Builder for constructing complex logic conditions
pub struct ConditionBuilder {
    condition: Option<ConditionGeometry>,
}

impl ConditionBuilder {
    pub fn new() -> Self {
        Self { condition: None }
    }

    /// Start with a threshold condition
    pub fn threshold(axis: usize, op: ThresholdOp, value: f32) -> Self {
        Self {
            condition: Some(ConditionGeometry::threshold(axis, op, value)),
        }
    }

    /// Start with x > value
    pub fn x_gt(value: f32) -> Self {
        Self::threshold(0, ThresholdOp::Greater, value)
    }

    /// Start with x < value
    pub fn x_lt(value: f32) -> Self {
        Self::threshold(0, ThresholdOp::Less, value)
    }

    /// Start with a range
    pub fn range(axis: usize, min: f32, max: f32) -> Self {
        Self {
            condition: Some(ConditionGeometry::range(axis, min, max)),
        }
    }

    /// Start with a sphere
    pub fn sphere(center: [f32; 3], radius: f32) -> Self {
        Self {
            condition: Some(ConditionGeometry::sphere(center, radius)),
        }
    }

    /// AND with another condition
    pub fn and(self, other: ConditionBuilder) -> Self {
        match (self.condition, other.condition) {
            (Some(left), Some(right)) => Self {
                condition: Some(ConditionGeometry::and(left, right)),
            },
            (Some(c), None) | (None, Some(c)) => Self { condition: Some(c) },
            (None, None) => Self { condition: None },
        }
    }

    /// OR with another condition
    pub fn or(self, other: ConditionBuilder) -> Self {
        match (self.condition, other.condition) {
            (Some(left), Some(right)) => Self {
                condition: Some(ConditionGeometry::or(left, right)),
            },
            (Some(c), None) | (None, Some(c)) => Self { condition: Some(c) },
            (None, None) => Self { condition: None },
        }
    }

    /// NOT the current condition
    pub fn not(self) -> Self {
        Self {
            condition: self.condition.map(ConditionGeometry::not),
        }
    }

    /// Build the condition
    pub fn build(self) -> Option<ConditionGeometry> {
        self.condition
    }
}

impl Default for ConditionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold_greater() {
        let cond = ConditionGeometry::x_gt(5.0);
        let state = StateVec3::new(6.0, 0.0, 0.0);
        assert!(cond.evaluate(&state, 0));

        let state2 = StateVec3::new(4.0, 0.0, 0.0);
        assert!(!cond.evaluate(&state2, 0));
    }

    #[test]
    fn test_threshold_less() {
        let cond = ConditionGeometry::threshold(0, ThresholdOp::Less, 5.0);
        let state = StateVec3::new(4.0, 0.0, 0.0);
        assert!(cond.evaluate(&state, 0));

        let state2 = StateVec3::new(6.0, 0.0, 0.0);
        assert!(!cond.evaluate(&state2, 0));
    }

    #[test]
    fn test_range() {
        let cond = ConditionGeometry::range(0, 0.0, 10.0);

        assert!(cond.evaluate(&StateVec3::new(5.0, 0.0, 0.0), 0));
        assert!(cond.evaluate(&StateVec3::new(0.0, 0.0, 0.0), 0));
        assert!(cond.evaluate(&StateVec3::new(10.0, 0.0, 0.0), 0));
        assert!(!cond.evaluate(&StateVec3::new(-1.0, 0.0, 0.0), 0));
        assert!(!cond.evaluate(&StateVec3::new(11.0, 0.0, 0.0), 0));
    }

    #[test]
    fn test_sphere() {
        let cond = ConditionGeometry::sphere([0.0, 0.0, 0.0], 5.0);

        assert!(cond.evaluate(&StateVec3::new(0.0, 0.0, 0.0), 0));
        assert!(cond.evaluate(&StateVec3::new(3.0, 4.0, 0.0), 0)); // distance = 5
        assert!(!cond.evaluate(&StateVec3::new(10.0, 0.0, 0.0), 0));
    }

    #[test]
    fn test_aabb() {
        let cond = ConditionGeometry::aabb([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);

        assert!(cond.evaluate(&StateVec3::new(5.0, 5.0, 5.0), 0));
        assert!(cond.evaluate(&StateVec3::new(0.0, 0.0, 0.0), 0));
        assert!(!cond.evaluate(&StateVec3::new(-1.0, 5.0, 5.0), 0));
    }

    #[test]
    fn test_pattern() {
        let cond = ConditionGeometry::pattern(0b1010, 0b1111);

        assert!(cond.evaluate(&StateVec3::default(), 0b1010));
        assert!(cond.evaluate(&StateVec3::default(), 0b11111010)); // mask filters
        assert!(!cond.evaluate(&StateVec3::default(), 0b1011));
    }

    #[test]
    fn test_compound_and() {
        let cond = ConditionGeometry::and(
            ConditionGeometry::x_gt(0.0),
            ConditionGeometry::threshold(0, ThresholdOp::Less, 10.0),
        );

        assert!(cond.evaluate(&StateVec3::new(5.0, 0.0, 0.0), 0));
        assert!(!cond.evaluate(&StateVec3::new(-1.0, 0.0, 0.0), 0));
        assert!(!cond.evaluate(&StateVec3::new(15.0, 0.0, 0.0), 0));
    }

    #[test]
    fn test_compound_or() {
        let cond = ConditionGeometry::or(
            ConditionGeometry::threshold(0, ThresholdOp::Less, 0.0),
            ConditionGeometry::x_gt(10.0),
        );

        assert!(cond.evaluate(&StateVec3::new(-5.0, 0.0, 0.0), 0));
        assert!(cond.evaluate(&StateVec3::new(15.0, 0.0, 0.0), 0));
        assert!(!cond.evaluate(&StateVec3::new(5.0, 0.0, 0.0), 0));
    }

    #[test]
    fn test_compound_not() {
        let cond = ConditionGeometry::not(ConditionGeometry::x_gt(5.0));

        assert!(cond.evaluate(&StateVec3::new(4.0, 0.0, 0.0), 0));
        assert!(!cond.evaluate(&StateVec3::new(6.0, 0.0, 0.0), 0));
    }

    #[test]
    fn test_builder() {
        let cond = ConditionBuilder::x_gt(0.0)
            .and(ConditionBuilder::x_lt(10.0))
            .build()
            .unwrap();

        assert!(cond.evaluate(&StateVec3::new(5.0, 0.0, 0.0), 0));
        assert!(!cond.evaluate(&StateVec3::new(-5.0, 0.0, 0.0), 0));
    }

    #[test]
    fn test_logic_scene_cpu() {
        let mut scene = LogicScene::new();

        // x > 5 AND x < 15
        let cond1 = ConditionGeometry::x_gt(5.0);
        let cond2 = ConditionGeometry::threshold(0, ThresholdOp::Less, 15.0);
        let compound = ConditionGeometry::and(cond1, cond2);

        scene.add_condition(1, compound);
        scene.set_root(1);

        let rays = vec![
            LogicRay::new([10.0, 0.0, 0.0]),  // Should be true
            LogicRay::new([3.0, 0.0, 0.0]),   // Should be false
            LogicRay::new([20.0, 0.0, 0.0]),  // Should be false
        ];

        let results = scene.evaluate_cpu(&rays);

        assert!(results[0].result);
        assert!(!results[1].result);
        assert!(!results[2].result);
    }

    #[test]
    fn test_logic_ray() {
        let ray = LogicRay::new([1.0, 2.0, 3.0]);
        assert_eq!(ray.state.x, 1.0);
        assert_eq!(ray.state.y, 2.0);
        assert_eq!(ray.state.z, 3.0);

        let ray_bits = LogicRay::with_bits([0.0, 0.0, 0.0], 0b1010);
        assert_eq!(ray_bits.state_bits, 0b1010);
    }

    #[test]
    fn test_shader_compiles() {
        // Verify shader syntax is valid
        assert!(LOGIC_ENCODING_SHADER.contains("@compute @workgroup_size(256)"));
        assert!(LOGIC_ENCODING_SHADER.contains("fn logic_eval_main"));
        assert!(LOGIC_ENCODING_SHADER.contains("struct Condition"));
    }
}
