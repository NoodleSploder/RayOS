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
//! - `decision_tree` - BVH-encoded decision trees (planned)
//! - `state_geometry` - Variables as spatial structures (planned)

pub mod access_control;

pub use access_control::{
    AccessControlGeometry, AccessQuery, AccessResult, AccessDecision,
    GeometricACL, PrincipalGeometry, ResourceGeometry, PermissionMesh,
    ACLScene, ACLRay, ACLHit, AccessStats,
};
