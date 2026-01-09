//! BVH Decision Trees - Decision Trees as Bounding Volume Hierarchies
//!
//! Maps traditional decision trees to BVH (Bounding Volume Hierarchy)
//! structures for massively parallel GPU execution on RT cores.
//!
//! ## Core Concept
//!
//! A decision tree is structurally identical to a BVH:
//! - Both are binary trees
//! - Both make branching decisions at each node
//! - Both terminate at leaf nodes
//!
//! By encoding decision boundaries as axis-aligned bounding boxes (AABBs),
//! we can use GPU ray-tracing hardware to evaluate decision trees in parallel.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    Decision Tree → BVH Mapping                          │
//! │                                                                         │
//! │   Traditional Decision Tree:        BVH Equivalent:                     │
//! │                                                                         │
//! │        [x > 5?]                     ┌─────────────┐                     │
//! │        /     \                      │  Root AABB  │                     │
//! │      Yes     No                     │ (full space)│                     │
//! │      /         \                    └──────┬──────┘                     │
//! │   [y > 3?]   [z > 2?]                     │                             │
//! │   /   \      /    \                ┌──────┴──────┐                      │
//! │  A     B    C      D              ┌┴┐          ┌─┴─┐                    │
//! │                                   │L│ x≤5      │R  │ x>5                │
//! │                                   └┬┘          └─┬─┘                    │
//! │  Leaves A,B,C,D become          ┌──┴──┐     ┌───┴───┐                   │
//! │  AABB regions in space          │LL  │LR   │RL     │RR                  │
//! │                                 │y≤3 │y>3  │z≤2    │z>2                 │
//! │                                 │=C  │=D   │=A     │=B                  │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## GPU Execution
//!
//! A ray is cast through the BVH with origin at the feature vector.
//! The ray direction is uniform (e.g., +X). The first leaf hit
//! determines the classification/decision.
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_kernel::geometry_logic::decision_tree::*;
//!
//! // Build a decision tree
//! let mut tree = DecisionTree::new(3); // 3 features
//!
//! // Add decision nodes
//! let root = tree.add_split(0, 5.0, SplitOp::Greater); // x > 5
//! let left = tree.add_split(1, 3.0, SplitOp::Greater);  // y > 3
//! let right = tree.add_split(2, 2.0, SplitOp::Greater); // z > 2
//!
//! // Add leaf nodes with class labels
//! tree.add_leaf(root, BranchDir::Left, left);
//! tree.add_leaf(root, BranchDir::Right, right);
//! tree.set_leaf_class(left, BranchDir::Left, 0);  // Class A
//! tree.set_leaf_class(left, BranchDir::Right, 1); // Class B
//!
//! // Convert to BVH
//! let bvh = tree.to_bvh();
//!
//! // Classify samples
//! let samples = vec![[6.0, 4.0, 1.0], [2.0, 1.0, 5.0]];
//! let classes = bvh.classify_cpu(&samples);
//! ```

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

// =============================================================================
// Configuration
// =============================================================================

/// Maximum tree depth
pub const MAX_TREE_DEPTH: usize = 32;

/// Maximum number of nodes
pub const MAX_NODES: usize = 65536;

/// Maximum samples per GPU dispatch
pub const MAX_SAMPLES: usize = 262144;

/// Workgroup size for compute shader
pub const WORKGROUP_SIZE: u32 = 256;

// =============================================================================
// GPU Shader
// =============================================================================

/// WGSL compute shader for BVH decision tree traversal
pub const BVH_DECISION_TREE_SHADER: &str = r#"
// BVH Decision Tree Shader
//
// Traverses a BVH to classify input samples. Each sample is a point
// in feature space, and the BVH encodes decision boundaries.

// 3D bounding box
struct AABB {
    min_x: f32,
    min_y: f32,
    min_z: f32,
    max_x: f32,
    max_y: f32,
    max_z: f32,
}

// Extended bounds for higher dimensions (up to 8 features)
struct ExtendedBounds {
    // Dimensions 0-2 are in AABB
    min_3: f32,
    max_3: f32,
    min_4: f32,
    max_4: f32,
    min_5: f32,
    max_5: f32,
    min_6: f32,
    max_6: f32,
    min_7: f32,
    max_7: f32,
}

// BVH Node
struct BVHNode {
    // Bounding box for this node
    bounds: AABB,
    // Extended bounds for features 3-7
    extended: ExtendedBounds,
    // For internal nodes: index of left child (right = left + 1)
    // For leaf nodes: class label
    left_child_or_class: u32,
    // Node type: 0 = internal, 1 = leaf
    is_leaf: u32,
    // Split axis for internal nodes (0-7)
    split_axis: u32,
    // Split value for internal nodes
    split_value: f32,
    // Depth in tree (for debugging)
    depth: u32,
    // Parent index (for debugging)
    parent: u32,
    _pad0: u32,
    _pad1: u32,
}

// Sample to classify
struct Sample {
    id: u32,
    // Feature vector (up to 8 dimensions)
    f0: f32,
    f1: f32,
    f2: f32,
    f3: f32,
    f4: f32,
    f5: f32,
    f6: f32,
    f7: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

// Classification result
struct ClassResult {
    sample_id: u32,
    class_label: u32,
    confidence: f32,  // 1.0 for hard classification, can be soft
    leaf_index: u32,
    traversal_depth: u32,
    nodes_visited: u32,
    _pad0: u32,
    _pad1: u32,
}

// Configuration
struct Config {
    num_nodes: u32,
    num_samples: u32,
    num_features: u32,
    root_index: u32,
    num_classes: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

// Buffers
@group(0) @binding(0) var<storage, read> nodes: array<BVHNode, 65536>;
@group(0) @binding(1) var<storage, read> samples: array<Sample, 262144>;
@group(0) @binding(2) var<storage, read> config: Config;
@group(0) @binding(3) var<storage, read_write> results: array<ClassResult, 262144>;

// Get feature value from sample by index
fn get_feature(sample: Sample, idx: u32) -> f32 {
    switch (idx) {
        case 0u: { return sample.f0; }
        case 1u: { return sample.f1; }
        case 2u: { return sample.f2; }
        case 3u: { return sample.f3; }
        case 4u: { return sample.f4; }
        case 5u: { return sample.f5; }
        case 6u: { return sample.f6; }
        case 7u: { return sample.f7; }
        default: { return 0.0; }
    }
}

// Check if sample is inside node's bounding region
fn sample_in_bounds(sample: Sample, node: BVHNode, num_features: u32) -> bool {
    // Check first 3 dimensions (AABB)
    if (num_features >= 1u && (sample.f0 < node.bounds.min_x || sample.f0 > node.bounds.max_x)) {
        return false;
    }
    if (num_features >= 2u && (sample.f1 < node.bounds.min_y || sample.f1 > node.bounds.max_y)) {
        return false;
    }
    if (num_features >= 3u && (sample.f2 < node.bounds.min_z || sample.f2 > node.bounds.max_z)) {
        return false;
    }
    // Check extended dimensions
    if (num_features >= 4u && (sample.f3 < node.extended.min_3 || sample.f3 > node.extended.max_3)) {
        return false;
    }
    if (num_features >= 5u && (sample.f4 < node.extended.min_4 || sample.f4 > node.extended.max_4)) {
        return false;
    }
    if (num_features >= 6u && (sample.f5 < node.extended.min_5 || sample.f5 > node.extended.max_5)) {
        return false;
    }
    if (num_features >= 7u && (sample.f6 < node.extended.min_6 || sample.f6 > node.extended.max_6)) {
        return false;
    }
    if (num_features >= 8u && (sample.f7 < node.extended.min_7 || sample.f7 > node.extended.max_7)) {
        return false;
    }
    return true;
}

// Traverse BVH to classify sample
fn classify_sample(sample: Sample, num_features: u32) -> ClassResult {
    var result = ClassResult(sample.id, 0u, 1.0, 0u, 0u, 0u, 0u, 0u);

    var node_idx = config.root_index;
    var depth = 0u;
    var nodes_visited = 0u;

    // Traverse until we hit a leaf
    loop {
        if (depth >= 32u || node_idx >= config.num_nodes) {
            break;
        }

        let node = nodes[node_idx];
        nodes_visited = nodes_visited + 1u;

        // Check if we're at a leaf
        if (node.is_leaf == 1u) {
            result.class_label = node.left_child_or_class;
            result.leaf_index = node_idx;
            result.traversal_depth = depth;
            result.nodes_visited = nodes_visited;
            break;
        }

        // Internal node: decide which child to visit
        let feature_val = get_feature(sample, node.split_axis);

        if (feature_val <= node.split_value) {
            // Go left
            node_idx = node.left_child_or_class;
        } else {
            // Go right (left + 1)
            node_idx = node.left_child_or_class + 1u;
        }

        depth = depth + 1u;
    }

    return result;
}

// Main compute shader
@compute @workgroup_size(256)
fn bvh_classify_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let sample_idx = global_id.x;

    if (sample_idx >= config.num_samples) {
        return;
    }

    let sample = samples[sample_idx];
    results[sample_idx] = classify_sample(sample, config.num_features);
}
"#;

// =============================================================================
// Rust Types
// =============================================================================

/// Split operation for decision nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOp {
    /// value <= threshold goes left, > goes right
    LessOrEqual,
    /// value < threshold goes left, >= goes right
    Less,
    /// value > threshold goes left, <= goes right
    Greater,
    /// value >= threshold goes left, < goes right
    GreaterOrEqual,
}

/// Branch direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchDir {
    Left,
    Right,
}

/// A node in the decision tree
#[derive(Debug, Clone)]
pub enum DecisionNode {
    /// Internal node with a split condition
    Split {
        /// Feature index to split on
        feature: usize,
        /// Threshold value
        threshold: f32,
        /// Split operation
        op: SplitOp,
        /// Left child index
        left: Option<usize>,
        /// Right child index
        right: Option<usize>,
    },
    /// Leaf node with class label
    Leaf {
        /// Class label
        class: u32,
        /// Optional probability distribution over classes
        probabilities: Option<Vec<f32>>,
    },
}

/// Axis-aligned bounding box in feature space
#[derive(Debug, Clone, Copy, Default)]
pub struct FeatureAABB {
    /// Minimum bounds for each feature
    pub min: [f32; 8],
    /// Maximum bounds for each feature
    pub max: [f32; 8],
}

impl FeatureAABB {
    /// Create a new AABB covering all of feature space
    pub fn full(num_features: usize) -> Self {
        let mut aabb = Self::default();
        for i in 0..num_features.min(8) {
            aabb.min[i] = f32::NEG_INFINITY;
            aabb.max[i] = f32::INFINITY;
        }
        aabb
    }

    /// Create an AABB with specific bounds
    pub fn new(min: [f32; 8], max: [f32; 8]) -> Self {
        Self { min, max }
    }

    /// Check if a point is inside the AABB
    pub fn contains(&self, point: &[f32], num_features: usize) -> bool {
        for i in 0..num_features.min(8).min(point.len()) {
            if point[i] < self.min[i] || point[i] > self.max[i] {
                return false;
            }
        }
        true
    }

    /// Split the AABB along an axis
    pub fn split(&self, axis: usize, value: f32) -> (Self, Self) {
        let mut left = *self;
        let mut right = *self;

        if axis < 8 {
            left.max[axis] = value;
            right.min[axis] = value;
        }

        (left, right)
    }
}

/// A BVH node for GPU execution
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuBVHNode {
    // AABB bounds (first 3 dimensions)
    pub min_x: f32,
    pub min_y: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub max_z: f32,
    // Extended bounds (dimensions 3-7)
    pub min_3: f32,
    pub max_3: f32,
    pub min_4: f32,
    pub max_4: f32,
    pub min_5: f32,
    pub max_5: f32,
    pub min_6: f32,
    pub max_6: f32,
    pub min_7: f32,
    pub max_7: f32,
    // Node data
    pub left_child_or_class: u32,
    pub is_leaf: u32,
    pub split_axis: u32,
    pub split_value: f32,
    pub depth: u32,
    pub parent: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

impl GpuBVHNode {
    /// Create a leaf node
    pub fn leaf(bounds: &FeatureAABB, class: u32, depth: u32, parent: u32) -> Self {
        Self {
            min_x: bounds.min[0],
            min_y: bounds.min[1],
            min_z: bounds.min[2],
            max_x: bounds.max[0],
            max_y: bounds.max[1],
            max_z: bounds.max[2],
            min_3: bounds.min[3],
            max_3: bounds.max[3],
            min_4: bounds.min[4],
            max_4: bounds.max[4],
            min_5: bounds.min[5],
            max_5: bounds.max[5],
            min_6: bounds.min[6],
            max_6: bounds.max[6],
            min_7: bounds.min[7],
            max_7: bounds.max[7],
            left_child_or_class: class,
            is_leaf: 1,
            split_axis: 0,
            split_value: 0.0,
            depth,
            parent,
            _pad0: 0,
            _pad1: 0,
        }
    }

    /// Create an internal node
    pub fn internal(
        bounds: &FeatureAABB,
        left_child: u32,
        split_axis: u32,
        split_value: f32,
        depth: u32,
        parent: u32,
    ) -> Self {
        Self {
            min_x: bounds.min[0],
            min_y: bounds.min[1],
            min_z: bounds.min[2],
            max_x: bounds.max[0],
            max_y: bounds.max[1],
            max_z: bounds.max[2],
            min_3: bounds.min[3],
            max_3: bounds.max[3],
            min_4: bounds.min[4],
            max_4: bounds.max[4],
            min_5: bounds.min[5],
            max_5: bounds.max[5],
            min_6: bounds.min[6],
            max_6: bounds.max[6],
            min_7: bounds.min[7],
            max_7: bounds.max[7],
            left_child_or_class: left_child,
            is_leaf: 0,
            split_axis,
            split_value,
            depth,
            parent,
            _pad0: 0,
            _pad1: 0,
        }
    }
}

/// A sample to classify
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuSample {
    pub id: u32,
    pub f0: f32,
    pub f1: f32,
    pub f2: f32,
    pub f3: f32,
    pub f4: f32,
    pub f5: f32,
    pub f6: f32,
    pub f7: f32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

impl GpuSample {
    pub fn new(id: u32, features: &[f32]) -> Self {
        let mut sample = Self::default();
        sample.id = id;
        if features.len() > 0 { sample.f0 = features[0]; }
        if features.len() > 1 { sample.f1 = features[1]; }
        if features.len() > 2 { sample.f2 = features[2]; }
        if features.len() > 3 { sample.f3 = features[3]; }
        if features.len() > 4 { sample.f4 = features[4]; }
        if features.len() > 5 { sample.f5 = features[5]; }
        if features.len() > 6 { sample.f6 = features[6]; }
        if features.len() > 7 { sample.f7 = features[7]; }
        sample
    }
}

/// Classification result
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuClassResult {
    pub sample_id: u32,
    pub class_label: u32,
    pub confidence: f32,
    pub leaf_index: u32,
    pub traversal_depth: u32,
    pub nodes_visited: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

/// Classification result (Rust-friendly)
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// Sample ID
    pub sample_id: u32,
    /// Predicted class label
    pub class: u32,
    /// Confidence (1.0 for hard classification)
    pub confidence: f32,
    /// Index of leaf node reached
    pub leaf_index: u32,
    /// Depth traversed in tree
    pub depth: u32,
    /// Number of nodes visited
    pub nodes_visited: u32,
}

impl From<GpuClassResult> for ClassificationResult {
    fn from(r: GpuClassResult) -> Self {
        Self {
            sample_id: r.sample_id,
            class: r.class_label,
            confidence: r.confidence,
            leaf_index: r.leaf_index,
            depth: r.traversal_depth,
            nodes_visited: r.nodes_visited,
        }
    }
}

/// Statistics for BVH decision tree
#[derive(Debug, Default)]
pub struct DecisionTreeStats {
    pub total_classifications: AtomicU64,
    pub total_nodes_visited: AtomicU64,
    pub gpu_dispatches: AtomicU64,
}

/// A decision tree that can be converted to BVH
pub struct DecisionTree {
    /// Number of features
    num_features: usize,
    /// Number of classes
    num_classes: usize,
    /// Nodes in the tree
    nodes: Vec<DecisionNode>,
    /// Root node index
    root: Option<usize>,
}

impl DecisionTree {
    /// Create a new decision tree
    pub fn new(num_features: usize) -> Self {
        Self {
            num_features,
            num_classes: 2,
            nodes: Vec::new(),
            root: None,
        }
    }

    /// Set number of classes
    pub fn set_num_classes(&mut self, num_classes: usize) {
        self.num_classes = num_classes;
    }

    /// Add a split node
    pub fn add_split(&mut self, feature: usize, threshold: f32, op: SplitOp) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(DecisionNode::Split {
            feature,
            threshold,
            op,
            left: None,
            right: None,
        });
        if self.root.is_none() {
            self.root = Some(idx);
        }
        idx
    }

    /// Add a leaf node
    pub fn add_leaf(&mut self, class: u32) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(DecisionNode::Leaf {
            class,
            probabilities: None,
        });
        idx
    }

    /// Add a leaf with probability distribution
    pub fn add_leaf_with_probs(&mut self, class: u32, probs: Vec<f32>) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(DecisionNode::Leaf {
            class,
            probabilities: Some(probs),
        });
        idx
    }

    /// Set left child of a node
    pub fn set_left(&mut self, parent: usize, child: usize) {
        if let Some(DecisionNode::Split { left, .. }) = self.nodes.get_mut(parent) {
            *left = Some(child);
        }
    }

    /// Set right child of a node
    pub fn set_right(&mut self, parent: usize, child: usize) {
        if let Some(DecisionNode::Split { right, .. }) = self.nodes.get_mut(parent) {
            *right = Some(child);
        }
    }

    /// Set the root node
    pub fn set_root(&mut self, root: usize) {
        self.root = Some(root);
    }

    /// Number of nodes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Convert to BVH for GPU execution
    pub fn to_bvh(&self) -> BVHDecisionTree {
        let mut bvh = BVHDecisionTree::new(self.num_features, self.num_classes);

        if let Some(root_idx) = self.root {
            let bounds = FeatureAABB::full(self.num_features);
            self.build_bvh_recursive(root_idx, bounds, 0, 0, &mut bvh);
        }

        bvh.root_index = 0;
        bvh
    }

    fn build_bvh_recursive(
        &self,
        node_idx: usize,
        bounds: FeatureAABB,
        depth: u32,
        parent: u32,
        bvh: &mut BVHDecisionTree,
    ) -> u32 {
        let gpu_idx = bvh.nodes.len() as u32;

        match &self.nodes[node_idx] {
            DecisionNode::Leaf { class, .. } => {
                bvh.nodes.push(GpuBVHNode::leaf(&bounds, *class, depth, parent));
                gpu_idx
            }
            DecisionNode::Split { feature, threshold, op, left, right } => {
                // Reserve space for this internal node
                bvh.nodes.push(GpuBVHNode::default());

                // Determine split bounds based on operation
                let (left_bounds, right_bounds) = match op {
                    SplitOp::LessOrEqual | SplitOp::Less => bounds.split(*feature, *threshold),
                    SplitOp::Greater | SplitOp::GreaterOrEqual => {
                        let (r, l) = bounds.split(*feature, *threshold);
                        (l, r)
                    }
                };

                // Build children
                let left_idx = if let Some(l) = left {
                    self.build_bvh_recursive(*l, left_bounds, depth + 1, gpu_idx, bvh)
                } else {
                    // Create default leaf
                    let leaf_idx = bvh.nodes.len() as u32;
                    bvh.nodes.push(GpuBVHNode::leaf(&left_bounds, 0, depth + 1, gpu_idx));
                    leaf_idx
                };

                let _right_idx = if let Some(r) = right {
                    self.build_bvh_recursive(*r, right_bounds, depth + 1, gpu_idx, bvh)
                } else {
                    // Create default leaf
                    let leaf_idx = bvh.nodes.len() as u32;
                    bvh.nodes.push(GpuBVHNode::leaf(&right_bounds, 0, depth + 1, gpu_idx));
                    leaf_idx
                };

                // Update the internal node
                bvh.nodes[gpu_idx as usize] = GpuBVHNode::internal(
                    &bounds,
                    left_idx,
                    *feature as u32,
                    *threshold,
                    depth,
                    parent,
                );

                gpu_idx
            }
        }
    }

    /// Classify a single sample (CPU)
    pub fn classify(&self, features: &[f32]) -> Option<u32> {
        let mut current = self.root?;

        loop {
            match &self.nodes[current] {
                DecisionNode::Leaf { class, .. } => return Some(*class),
                DecisionNode::Split { feature, threshold, op, left, right } => {
                    let val = features.get(*feature).copied().unwrap_or(0.0);

                    let go_left = match op {
                        SplitOp::LessOrEqual => val <= *threshold,
                        SplitOp::Less => val < *threshold,
                        SplitOp::Greater => val > *threshold,
                        SplitOp::GreaterOrEqual => val >= *threshold,
                    };

                    current = if go_left {
                        (*left)?
                    } else {
                        (*right)?
                    };
                }
            }
        }
    }
}

/// BVH-encoded decision tree for GPU execution
pub struct BVHDecisionTree {
    /// Number of features
    pub num_features: usize,
    /// Number of classes
    pub num_classes: usize,
    /// BVH nodes in GPU format
    pub nodes: Vec<GpuBVHNode>,
    /// Root node index
    pub root_index: u32,
    /// Statistics
    pub stats: Arc<DecisionTreeStats>,
}

impl BVHDecisionTree {
    /// Create a new empty BVH
    pub fn new(num_features: usize, num_classes: usize) -> Self {
        Self {
            num_features,
            num_classes,
            nodes: Vec::new(),
            root_index: 0,
            stats: Arc::new(DecisionTreeStats::default()),
        }
    }

    /// Classify samples on CPU
    pub fn classify_cpu(&self, samples: &[impl AsRef<[f32]>]) -> Vec<ClassificationResult> {
        samples
            .iter()
            .enumerate()
            .map(|(id, features)| {
                let features = features.as_ref();
                let result = self.classify_single_cpu(features);
                self.stats.total_classifications.fetch_add(1, AtomicOrdering::Relaxed);
                self.stats.total_nodes_visited.fetch_add(result.nodes_visited as u64, AtomicOrdering::Relaxed);
                ClassificationResult {
                    sample_id: id as u32,
                    ..result
                }
            })
            .collect()
    }

    fn classify_single_cpu(&self, features: &[f32]) -> ClassificationResult {
        let mut node_idx = self.root_index as usize;
        let mut depth = 0u32;
        let mut nodes_visited = 0u32;

        loop {
            if node_idx >= self.nodes.len() || depth >= MAX_TREE_DEPTH as u32 {
                break;
            }

            let node = &self.nodes[node_idx];
            nodes_visited += 1;

            if node.is_leaf == 1 {
                return ClassificationResult {
                    sample_id: 0,
                    class: node.left_child_or_class,
                    confidence: 1.0,
                    leaf_index: node_idx as u32,
                    depth,
                    nodes_visited,
                };
            }

            // Get feature value
            let feature_val = features.get(node.split_axis as usize).copied().unwrap_or(0.0);

            // Decide direction
            if feature_val <= node.split_value {
                node_idx = node.left_child_or_class as usize;
            } else {
                node_idx = (node.left_child_or_class + 1) as usize;
            }

            depth += 1;
        }

        // Default result if traversal fails
        ClassificationResult {
            sample_id: 0,
            class: 0,
            confidence: 0.0,
            leaf_index: 0,
            depth,
            nodes_visited,
        }
    }

    /// Number of nodes in the BVH
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get tree depth
    pub fn depth(&self) -> u32 {
        self.nodes.iter().map(|n| n.depth).max().unwrap_or(0) + 1
    }

    /// Get number of leaf nodes
    pub fn num_leaves(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_leaf == 1).count()
    }
}

// =============================================================================
// Builder for Decision Trees
// =============================================================================

/// Builder for constructing decision trees from training data
pub struct DecisionTreeBuilder {
    max_depth: usize,
    min_samples_split: usize,
    min_samples_leaf: usize,
    num_features: usize,
}

impl DecisionTreeBuilder {
    /// Create a new builder
    pub fn new(num_features: usize) -> Self {
        Self {
            max_depth: 10,
            min_samples_split: 2,
            min_samples_leaf: 1,
            num_features,
        }
    }

    /// Set maximum tree depth
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set minimum samples to split
    pub fn min_samples_split(mut self, min: usize) -> Self {
        self.min_samples_split = min;
        self
    }

    /// Set minimum samples per leaf
    pub fn min_samples_leaf(mut self, min: usize) -> Self {
        self.min_samples_leaf = min;
        self
    }

    /// Build a decision tree from training data using CART algorithm
    pub fn build(&self, features: &[Vec<f32>], labels: &[u32]) -> DecisionTree {
        let mut tree = DecisionTree::new(self.num_features);

        if features.is_empty() || labels.is_empty() {
            return tree;
        }

        let num_classes = *labels.iter().max().unwrap_or(&0) as usize + 1;
        tree.set_num_classes(num_classes);

        // Collect indices
        let indices: Vec<usize> = (0..features.len()).collect();

        // Build tree recursively
        if let Some(root) = self.build_node(&mut tree, features, labels, &indices, 0) {
            tree.set_root(root);
        }

        tree
    }

    fn build_node(
        &self,
        tree: &mut DecisionTree,
        features: &[Vec<f32>],
        labels: &[u32],
        indices: &[usize],
        depth: usize,
    ) -> Option<usize> {
        if indices.is_empty() {
            return None;
        }

        // Check stopping conditions
        let unique_labels: std::collections::HashSet<_> = indices.iter().map(|&i| labels[i]).collect();

        if unique_labels.len() == 1 || depth >= self.max_depth || indices.len() < self.min_samples_split {
            // Create leaf with majority class
            let class = self.majority_class(labels, indices);
            return Some(tree.add_leaf(class));
        }

        // Find best split
        if let Some((best_feature, best_threshold, left_indices, right_indices)) =
            self.find_best_split(features, labels, indices)
        {
            if left_indices.len() < self.min_samples_leaf || right_indices.len() < self.min_samples_leaf {
                // Can't split further, create leaf
                let class = self.majority_class(labels, indices);
                return Some(tree.add_leaf(class));
            }

            // Create split node
            let node_idx = tree.add_split(best_feature, best_threshold, SplitOp::LessOrEqual);

            // Build children
            if let Some(left) = self.build_node(tree, features, labels, &left_indices, depth + 1) {
                tree.set_left(node_idx, left);
            }
            if let Some(right) = self.build_node(tree, features, labels, &right_indices, depth + 1) {
                tree.set_right(node_idx, right);
            }

            Some(node_idx)
        } else {
            // No valid split found, create leaf
            let class = self.majority_class(labels, indices);
            Some(tree.add_leaf(class))
        }
    }

    fn majority_class(&self, labels: &[u32], indices: &[usize]) -> u32 {
        let mut counts: HashMap<u32, usize> = HashMap::new();
        for &i in indices {
            *counts.entry(labels[i]).or_insert(0) += 1;
        }
        counts.into_iter().max_by_key(|(_, count)| *count).map(|(class, _)| class).unwrap_or(0)
    }

    fn find_best_split(
        &self,
        features: &[Vec<f32>],
        labels: &[u32],
        indices: &[usize],
    ) -> Option<(usize, f32, Vec<usize>, Vec<usize>)> {
        let mut best_gini = f32::INFINITY;
        let mut best_result = None;

        for feature_idx in 0..self.num_features {
            // Get unique values for this feature
            let mut values: Vec<f32> = indices
                .iter()
                .filter_map(|&i| features.get(i).and_then(|f| f.get(feature_idx).copied()))
                .collect();
            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            values.dedup();

            // Try each threshold
            for i in 0..values.len().saturating_sub(1) {
                let threshold = (values[i] + values[i + 1]) / 2.0;

                let (left, right): (Vec<_>, Vec<_>) = indices
                    .iter()
                    .copied()
                    .partition(|&idx| {
                        features.get(idx)
                            .and_then(|f| f.get(feature_idx).copied())
                            .unwrap_or(0.0) <= threshold
                    });

                if left.is_empty() || right.is_empty() {
                    continue;
                }

                let gini = self.weighted_gini(labels, &left, &right);
                if gini < best_gini {
                    best_gini = gini;
                    best_result = Some((feature_idx, threshold, left, right));
                }
            }
        }

        best_result
    }

    fn weighted_gini(&self, labels: &[u32], left: &[usize], right: &[usize]) -> f32 {
        let total = (left.len() + right.len()) as f32;
        let left_gini = self.gini_impurity(labels, left);
        let right_gini = self.gini_impurity(labels, right);
        (left.len() as f32 / total) * left_gini + (right.len() as f32 / total) * right_gini
    }

    fn gini_impurity(&self, labels: &[u32], indices: &[usize]) -> f32 {
        if indices.is_empty() {
            return 0.0;
        }

        let mut counts: HashMap<u32, usize> = HashMap::new();
        for &i in indices {
            *counts.entry(labels[i]).or_insert(0) += 1;
        }

        let total = indices.len() as f32;
        let sum_sq: f32 = counts.values().map(|&c| (c as f32 / total).powi(2)).sum();
        1.0 - sum_sq
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tree() {
        let mut tree = DecisionTree::new(2);

        // x > 5: class 1, else class 0
        let root = tree.add_split(0, 5.0, SplitOp::Greater);
        let left = tree.add_leaf(1);  // x > 5
        let right = tree.add_leaf(0); // x <= 5

        tree.set_left(root, left);
        tree.set_right(root, right);

        assert_eq!(tree.classify(&[6.0, 0.0]), Some(1));
        assert_eq!(tree.classify(&[4.0, 0.0]), Some(0));
    }

    #[test]
    fn test_deep_tree() {
        let mut tree = DecisionTree::new(3);

        // Root: x > 5
        let root = tree.add_split(0, 5.0, SplitOp::Greater);

        // Left (x > 5): y > 3
        let left = tree.add_split(1, 3.0, SplitOp::Greater);

        // Right (x <= 5): z > 2
        let right = tree.add_split(2, 2.0, SplitOp::Greater);

        // Leaves
        let leaf_a = tree.add_leaf(0); // x > 5, y > 3
        let leaf_b = tree.add_leaf(1); // x > 5, y <= 3
        let leaf_c = tree.add_leaf(2); // x <= 5, z > 2
        let leaf_d = tree.add_leaf(3); // x <= 5, z <= 2

        tree.set_left(root, left);
        tree.set_right(root, right);
        tree.set_left(left, leaf_a);
        tree.set_right(left, leaf_b);
        tree.set_left(right, leaf_c);
        tree.set_right(right, leaf_d);

        assert_eq!(tree.classify(&[6.0, 4.0, 0.0]), Some(0)); // A
        assert_eq!(tree.classify(&[6.0, 2.0, 0.0]), Some(1)); // B
        assert_eq!(tree.classify(&[4.0, 0.0, 3.0]), Some(2)); // C
        assert_eq!(tree.classify(&[4.0, 0.0, 1.0]), Some(3)); // D
    }

    #[test]
    fn test_bvh_conversion() {
        let mut tree = DecisionTree::new(2);

        let root = tree.add_split(0, 5.0, SplitOp::LessOrEqual);
        let left = tree.add_leaf(0);
        let right = tree.add_leaf(1);

        tree.set_left(root, left);
        tree.set_right(root, right);

        let bvh = tree.to_bvh();

        assert!(!bvh.is_empty());
        assert_eq!(bvh.num_leaves(), 2);
    }

    #[test]
    fn test_bvh_classify_cpu() {
        let mut tree = DecisionTree::new(2);

        let root = tree.add_split(0, 5.0, SplitOp::LessOrEqual);
        let left = tree.add_leaf(0);
        let right = tree.add_leaf(1);

        tree.set_left(root, left);
        tree.set_right(root, right);

        let bvh = tree.to_bvh();

        let samples = vec![
            vec![3.0, 0.0], // <= 5, should be class 0
            vec![7.0, 0.0], // > 5, should be class 1
            vec![5.0, 0.0], // = 5, should be class 0
        ];

        let results = bvh.classify_cpu(&samples);

        assert_eq!(results[0].class, 0);
        assert_eq!(results[1].class, 1);
        assert_eq!(results[2].class, 0);
    }

    #[test]
    fn test_feature_aabb() {
        let aabb = FeatureAABB::new(
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0],
        );

        assert!(aabb.contains(&[5.0, 5.0, 5.0], 3));
        assert!(!aabb.contains(&[15.0, 5.0, 5.0], 3));

        let (left, right) = aabb.split(0, 5.0);
        assert!(left.contains(&[3.0, 5.0, 5.0], 3));
        assert!(!left.contains(&[7.0, 5.0, 5.0], 3));
        assert!(!right.contains(&[3.0, 5.0, 5.0], 3));
        assert!(right.contains(&[7.0, 5.0, 5.0], 3));
    }

    #[test]
    fn test_tree_builder() {
        let features = vec![
            vec![1.0, 2.0],
            vec![2.0, 3.0],
            vec![3.0, 4.0],
            vec![8.0, 9.0],
            vec![9.0, 10.0],
            vec![10.0, 11.0],
        ];
        let labels = vec![0, 0, 0, 1, 1, 1];

        let tree = DecisionTreeBuilder::new(2)
            .max_depth(5)
            .build(&features, &labels);

        // Should correctly classify training data
        for (i, f) in features.iter().enumerate() {
            assert_eq!(tree.classify(f), Some(labels[i]));
        }
    }

    #[test]
    fn test_gpu_node_size() {
        // Verify GPU struct is properly sized for alignment
        assert_eq!(std::mem::size_of::<GpuBVHNode>() % 16, 0);
    }

    #[test]
    fn test_shader_compiles() {
        // Verify shader syntax
        assert!(BVH_DECISION_TREE_SHADER.contains("@compute @workgroup_size(256)"));
        assert!(BVH_DECISION_TREE_SHADER.contains("fn bvh_classify_main"));
        assert!(BVH_DECISION_TREE_SHADER.contains("struct BVHNode"));
    }
}
