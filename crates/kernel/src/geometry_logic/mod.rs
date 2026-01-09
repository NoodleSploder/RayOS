//! Logic as Geometry - GPU Ray Tracing for Decision Making
//!
//! This module implements the "Logic as Geometry" paradigm where logical
//! operations are encoded as ray-geometry intersections and executed on
//! GPU RT cores.
//!
//! ## Core Concepts
//!
//! | Traditional CPU Logic | RayOS Geometric Logic |
//! |-----------------------|----------------------|
//! | Instructions (opcodes) | Rays (origin + direction) |
//! | Conditionals (if/else) | Intersections (hit tests) |
//! | Branch prediction | BVH traversal |
//! | Call stack | Ray recursion depth |
//! | Memory access | Texture/buffer sampling at hit points |
//!
//! ## Modules
//!
//! - `access_control` - Permissions as geometric hit tests
//! - `logic_encoding` - RT Core logic: conditionals as ray-geometry intersections
//! - `decision_tree` - BVH-encoded decision trees for GPU classification
//! - `state_geometry` - Ray-based state access: variables as spatial structures

pub mod access_control;
pub mod logic_encoding;
pub mod decision_tree;
pub mod state_geometry;

pub use access_control::{
    AccessControlGeometry, AccessQuery, AccessResult, AccessDecision,
    GeometricACL, PrincipalGeometry, ResourceGeometry, PermissionMesh,
    ACLScene, ACLRay, ACLHit, AccessStats,
};

pub use logic_encoding::{
    // Core types
    ConditionGeometry, LogicRay, LogicResult, LogicScene, LogicStats,
    // Condition types
    ThresholdOp, CompoundOp, StateVec3,
    // GPU types
    GpuCondition,
    // Builder
    ConditionBuilder,
    // Constants
    MAX_CONDITIONS, MAX_LOGIC_RAYS, MAX_CONDITION_DEPTH,
    // Shader
    LOGIC_ENCODING_SHADER,
};

pub use decision_tree::{
    // Core types
    DecisionTree, DecisionNode, BVHDecisionTree, DecisionTreeStats,
    // Split operations
    SplitOp, BranchDir,
    // Geometry types
    FeatureAABB, GpuBVHNode, GpuSample, GpuClassResult,
    // Results
    ClassificationResult,
    // Builder
    DecisionTreeBuilder,
    // Constants
    MAX_TREE_DEPTH, MAX_NODES, MAX_SAMPLES,
    // Shader
    BVH_DECISION_TREE_SHADER,
};

pub use state_geometry::{
    // Core types
    StateGeometry, StateVariable, StatePosition, VarType,
    // GPU types
    GpuStateVar, GpuReadRequest, GpuReadResult,
    // Results
    ReadResult, WriteResult,
    // Builder
    StateLayoutBuilder, LayoutStrategy,
    // Stats
    StateAccessStats,
    // Constants
    MAX_STATE_VARS, MAX_ARRAY_SIZE, MAX_STRUCT_FIELDS, WORKGROUP_SIZE,
    // Shader
    STATE_GEOMETRY_SHADER,
};
