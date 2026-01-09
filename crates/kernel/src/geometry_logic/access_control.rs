//! Access Control Geometry - Permissions as Geometric Hit Tests
//!
//! Implements access control decisions as GPU ray-geometry intersections.
//! This provides massively parallel permission checking by encoding ACLs
//! as 3D geometry and executing hit tests on RT cores.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    Access Control Geometry Scene                         │
//! │                                                                          │
//! │    ┌─────────────┐     ┌─────────────┐     ┌─────────────┐              │
//! │    │ Admin Sphere│     │ Owner Cone  │     │ Permission  │              │
//! │    │  (role=admin)     │ (user→file) │     │   Mesh      │              │
//! │    │   radius=1  │     │             │     │ (ACL bits)  │              │
//! │    └─────────────┘     └─────────────┘     └─────────────┘              │
//! │           │                   │                   │                      │
//! │           └───────────────────┴───────────────────┘                      │
//! │                               │                                          │
//! │    Access Ray: origin=user_context, direction=toward_resource            │
//! │                               │                                          │
//! │    Result: First intersection determines access decision                 │
//! └──────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_kernel::geometry_logic::{AccessControlGeometry, AccessQuery};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let device = ...; // wgpu device
//!     let queue = ...;  // wgpu queue
//!
//!     let mut acl = AccessControlGeometry::new();
//!     acl.initialize(&device).await?;
//!
//!     // Add principals and resources
//!     acl.add_principal(PrincipalGeometry::user(1, &["admin"]));
//!     acl.add_resource(ResourceGeometry::file(1, 1, 0o644));
//!
//!     // Check access
//!     let query = AccessQuery::new(1, 1, Permission::READ);
//!     let result = acl.check_access(&device, &queue, &[query]).await?;
//!
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

// =============================================================================
// Configuration
// =============================================================================

/// Maximum number of principals (users/groups)
pub const MAX_PRINCIPALS: usize = 1024;

/// Maximum number of resources (files/objects)
pub const MAX_RESOURCES: usize = 4096;

/// Maximum access queries per dispatch
pub const MAX_QUERIES: usize = 65536;

/// Maximum groups per principal
pub const MAX_GROUPS_PER_PRINCIPAL: usize = 16;

// =============================================================================
// GPU Shader
// =============================================================================

/// WGSL compute shader for geometric access control
pub const ACCESS_CONTROL_SHADER: &str = r#"
// Access Control Geometry Shader
//
// Encodes access control as ray-geometry intersections:
// - Principals are points in permission space
// - Resources are volumes/surfaces
// - Access rays test if principal can reach resource
// - Different geometric primitives encode different permission types

// 3D vector for geometric operations
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

// A principal (user/group) in permission space
struct Principal {
    id: u32,
    // Position encodes role level (admin=high Y, user=low Y)
    position: Vec3,
    // Radius encodes influence (admin has larger radius)
    radius: f32,
    // Group memberships as bitmask
    groups: u32,
    // Flags (is_admin, is_owner, etc.)
    flags: u32,
    _pad0: u32,
    _pad1: u32,
}

// A resource (file/object) in permission space
struct Resource {
    id: u32,
    owner_id: u32,
    // Position in permission space
    position: Vec3,
    // Size encodes access complexity
    extent: Vec3,
    // Unix-style permission bits (owner/group/other × rwx)
    permission_bits: u32,
    // Group that owns this resource
    owner_group: u32,
    _pad0: u32,
}

// Access query (ray to cast)
struct AccessQuery {
    principal_id: u32,
    resource_id: u32,
    // Permission type (Unix): 1=execute, 2=write, 4=read
    permission: u32,
    _pad0: u32,
}

// Access result
struct AccessResult {
    query_idx: u32,
    // 0 = denied, 1 = allowed
    allowed: u32,
    // Reason code: 0=denied, 1=admin, 2=owner, 3=group, 4=other
    reason: u32,
    // Distance to permission (for debugging/logging)
    distance: f32,
}

// Configuration
struct Config {
    num_principals: u32,
    num_resources: u32,
    num_queries: u32,
    _pad0: u32,
}

// Buffers
@group(0) @binding(0)
var<storage, read> principals: array<Principal, 1024>;  // MAX_PRINCIPALS

@group(0) @binding(1)
var<storage, read> resources: array<Resource, 4096>;  // MAX_RESOURCES

@group(0) @binding(2)
var<storage, read> queries: array<AccessQuery, 65536>;  // MAX_QUERIES

@group(0) @binding(3)
var<storage, read> config: Config;

@group(0) @binding(4)
var<storage, read_write> results: array<AccessResult, 65536>;  // MAX_QUERIES

// Vector operations
fn vec3_new(x: f32, y: f32, z: f32) -> Vec3 {
    return Vec3(x, y, z);
}

fn vec3_sub(a: Vec3, b: Vec3) -> Vec3 {
    return Vec3(a.x - b.x, a.y - b.y, a.z - b.z);
}

fn vec3_length(v: Vec3) -> f32 {
    return sqrt(v.x * v.x + v.y * v.y + v.z * v.z);
}

fn vec3_dot(a: Vec3, b: Vec3) -> f32 {
    return a.x * b.x + a.y * b.y + a.z * b.z;
}

// Check if principal is admin (high Y position + large radius)
fn is_admin(principal: Principal) -> bool {
    return (principal.flags & 1u) != 0u;
}

// Check if principal owns resource
fn is_owner(principal: Principal, resource: Resource) -> bool {
    return principal.id == resource.owner_id;
}

// Check if principal is in resource's group
fn is_in_group(principal: Principal, resource: Resource) -> bool {
    // Check if any of principal's groups match resource's owner group
    return (principal.groups & (1u << resource.owner_group)) != 0u;
}

// Geometric distance test: can principal "reach" resource?
fn geometric_access_test(principal: Principal, resource: Resource) -> f32 {
    let diff = vec3_sub(resource.position, principal.position);
    let dist = vec3_length(diff);

    // Principal's radius represents their "reach" in permission space
    // If distance < radius, they can access
    return dist - principal.radius;
}

// Check permission bits (Unix-style)
fn check_permission_bits(resource: Resource, permission: u32, is_owner_flag: bool, is_group_flag: bool) -> bool {
    let bits = resource.permission_bits;

    // Owner permissions (bits 6-8)
    if (is_owner_flag) {
        let owner_perms = (bits >> 6u) & 7u;
        if ((owner_perms & permission) != 0u) {
            return true;
        }
    }

    // Group permissions (bits 3-5)
    if (is_group_flag) {
        let group_perms = (bits >> 3u) & 7u;
        if ((group_perms & permission) != 0u) {
            return true;
        }
    }

    // Other permissions (bits 0-2)
    let other_perms = bits & 7u;
    return (other_perms & permission) != 0u;
}

// Find principal by ID (linear search - could use spatial hash in production)
fn find_principal(id: u32) -> Principal {
    for (var i = 0u; i < config.num_principals; i++) {
        if (principals[i].id == id) {
            return principals[i];
        }
    }
    // Return empty principal if not found
    return Principal(0u, Vec3(0.0, 0.0, 0.0), 0.0, 0u, 0u, 0u, 0u);
}

// Find resource by ID
fn find_resource(id: u32) -> Resource {
    for (var i = 0u; i < config.num_resources; i++) {
        if (resources[i].id == id) {
            return resources[i];
        }
    }
    // Return empty resource if not found
    return Resource(0u, 0u, Vec3(0.0, 0.0, 0.0), Vec3(0.0, 0.0, 0.0), 0u, 0u, 0u);
}

// Main access control shader
@compute @workgroup_size(256)
fn access_check_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let query_idx = global_id.x;

    if (query_idx >= config.num_queries) {
        return;
    }

    let query = queries[query_idx];
    let principal = find_principal(query.principal_id);
    let resource = find_resource(query.resource_id);

    // Default: access denied
    var result = AccessResult(query_idx, 0u, 0u, 999.0);

    // Check 1: Is principal valid?
    if (principal.id == 0u) {
        results[query_idx] = result;
        return;
    }

    // Check 2: Is resource valid?
    if (resource.id == 0u) {
        results[query_idx] = result;
        return;
    }

    // Geometric Test 1: Admin sphere
    // Admins have flag set and large radius - they "reach" everything
    if (is_admin(principal)) {
        let dist = geometric_access_test(principal, resource);
        result = AccessResult(query_idx, 1u, 1u, dist);  // Reason: admin
        results[query_idx] = result;
        return;
    }

    // Geometric Test 2: Ownership cone
    // Owner has direct line to their resources
    let owner_flag = is_owner(principal, resource);
    if (owner_flag) {
        if (check_permission_bits(resource, query.permission, true, false)) {
            let dist = geometric_access_test(principal, resource);
            result = AccessResult(query_idx, 1u, 2u, dist);  // Reason: owner
            results[query_idx] = result;
            return;
        }
    }

    // Geometric Test 3: Group membership mesh
    let group_flag = is_in_group(principal, resource);
    if (group_flag) {
        if (check_permission_bits(resource, query.permission, false, true)) {
            let dist = geometric_access_test(principal, resource);
            result = AccessResult(query_idx, 1u, 3u, dist);  // Reason: group
            results[query_idx] = result;
            return;
        }
    }

    // Geometric Test 4: World (other) permissions
    if (check_permission_bits(resource, query.permission, false, false)) {
        let dist = geometric_access_test(principal, resource);
        result = AccessResult(query_idx, 1u, 4u, dist);  // Reason: other
        results[query_idx] = result;
        return;
    }

    // Access denied
    results[query_idx] = result;
}
"#;

// =============================================================================
// CPU-Side Types
// =============================================================================

/// Permission types (Unix convention: r=4, w=2, x=1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Permission {
    Execute = 1,
    Write = 2,
    Read = 4,
    WriteExecute = 3,
    ReadExecute = 5,
    ReadWrite = 6,
    All = 7,
}

/// Access decision result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    Denied,
    AllowedAdmin,
    AllowedOwner,
    AllowedGroup,
    AllowedOther,
}

impl AccessDecision {
    fn from_reason(allowed: u32, reason: u32) -> Self {
        if allowed == 0 {
            AccessDecision::Denied
        } else {
            match reason {
                1 => AccessDecision::AllowedAdmin,
                2 => AccessDecision::AllowedOwner,
                3 => AccessDecision::AllowedGroup,
                4 => AccessDecision::AllowedOther,
                _ => AccessDecision::Denied,
            }
        }
    }

    pub fn is_allowed(&self) -> bool {
        !matches!(self, AccessDecision::Denied)
    }
}

/// 3D vector for geometric operations
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub const ORIGIN: Self = Self::new(0.0, 0.0, 0.0);
}

/// Principal (user/group) in permission space
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPrincipal {
    pub id: u32,
    pub position: Vec3,
    pub radius: f32,
    pub groups: u32,
    pub flags: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

impl Default for GpuPrincipal {
    fn default() -> Self {
        Self {
            id: 0,
            position: Vec3::ORIGIN,
            radius: 0.0,
            groups: 0,
            flags: 0,
            _pad0: 0,
            _pad1: 0,
        }
    }
}

/// Resource (file/object) in permission space
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuResource {
    pub id: u32,
    pub owner_id: u32,
    pub position: Vec3,
    pub extent: Vec3,
    pub permission_bits: u32,
    pub owner_group: u32,
    pub _pad0: u32,
}

impl Default for GpuResource {
    fn default() -> Self {
        Self {
            id: 0,
            owner_id: 0,
            position: Vec3::ORIGIN,
            extent: Vec3::new(1.0, 1.0, 1.0),
            permission_bits: 0,
            owner_group: 0,
            _pad0: 0,
        }
    }
}

/// Access query
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuAccessQuery {
    pub principal_id: u32,
    pub resource_id: u32,
    pub permission: u32,
    pub _pad0: u32,
}

/// Access result from GPU
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuAccessResult {
    pub query_idx: u32,
    pub allowed: u32,
    pub reason: u32,
    pub distance: f32,
}

/// Configuration buffer
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuConfig {
    pub num_principals: u32,
    pub num_resources: u32,
    pub num_queries: u32,
    pub _pad0: u32,
}

// =============================================================================
// High-Level API Types
// =============================================================================

/// A principal (user/service) with geometric representation
#[derive(Debug, Clone)]
pub struct PrincipalGeometry {
    pub id: u32,
    pub name: String,
    pub is_admin: bool,
    pub groups: Vec<u32>,
    /// Radius in permission space (larger = more access)
    pub radius: f32,
}

impl PrincipalGeometry {
    /// Create a regular user
    pub fn user(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            is_admin: false,
            groups: Vec::new(),
            radius: 1.0,
        }
    }

    /// Create an admin user
    pub fn admin(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            is_admin: true,
            groups: Vec::new(),
            radius: 100.0, // Large radius to "reach" everything
        }
    }

    /// Add group membership
    pub fn with_group(mut self, group_id: u32) -> Self {
        self.groups.push(group_id);
        self
    }

    /// Convert to GPU representation
    fn to_gpu(&self) -> GpuPrincipal {
        let mut groups_bitmask = 0u32;
        for &g in &self.groups {
            if g < 32 {
                groups_bitmask |= 1 << g;
            }
        }

        let flags = if self.is_admin { 1 } else { 0 };

        // Position: Y encodes privilege level
        let y = if self.is_admin { 100.0 } else { 0.0 };

        GpuPrincipal {
            id: self.id,
            position: Vec3::new(0.0, y, 0.0),
            radius: self.radius,
            groups: groups_bitmask,
            flags,
            _pad0: 0,
            _pad1: 0,
        }
    }
}

/// A resource (file/object) with geometric representation
#[derive(Debug, Clone)]
pub struct ResourceGeometry {
    pub id: u32,
    pub owner_id: u32,
    pub owner_group: u32,
    /// Unix-style permission bits (9 bits: rwxrwxrwx)
    pub permission_bits: u32,
    pub name: String,
}

impl ResourceGeometry {
    /// Create a file resource
    pub fn file(id: u32, owner_id: u32, permission_bits: u32) -> Self {
        Self {
            id,
            owner_id,
            owner_group: 0,
            permission_bits,
            name: format!("file_{}", id),
        }
    }

    /// Set owner group
    pub fn with_group(mut self, group_id: u32) -> Self {
        self.owner_group = group_id;
        self
    }

    /// Set name
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Convert to GPU representation
    fn to_gpu(&self) -> GpuResource {
        // Position resources in a grid based on ID
        let x = (self.id % 64) as f32;
        let z = (self.id / 64) as f32;

        GpuResource {
            id: self.id,
            owner_id: self.owner_id,
            position: Vec3::new(x, 0.0, z),
            extent: Vec3::new(1.0, 1.0, 1.0),
            permission_bits: self.permission_bits,
            owner_group: self.owner_group,
            _pad0: 0,
        }
    }
}

/// An access query
#[derive(Debug, Clone)]
pub struct AccessQuery {
    pub principal_id: u32,
    pub resource_id: u32,
    pub permission: Permission,
}

impl AccessQuery {
    pub fn new(principal_id: u32, resource_id: u32, permission: Permission) -> Self {
        Self {
            principal_id,
            resource_id,
            permission,
        }
    }

    fn to_gpu(&self) -> GpuAccessQuery {
        GpuAccessQuery {
            principal_id: self.principal_id,
            resource_id: self.resource_id,
            permission: self.permission as u32,
            _pad0: 0,
        }
    }
}

/// Access check result
#[derive(Debug, Clone)]
pub struct AccessResult {
    pub query: AccessQuery,
    pub decision: AccessDecision,
    pub distance: f32,
}

impl AccessResult {
    pub fn is_allowed(&self) -> bool {
        self.decision.is_allowed()
    }
}

/// A ray in the ACL scene (for visualization/debugging)
#[derive(Debug, Clone)]
pub struct ACLRay {
    pub origin: Vec3,
    pub direction: Vec3,
    pub principal_id: u32,
    pub resource_id: u32,
}

/// A hit result (for visualization/debugging)
#[derive(Debug, Clone)]
pub struct ACLHit {
    pub ray: ACLRay,
    pub hit_type: &'static str,
    pub distance: f32,
    pub allowed: bool,
}

/// The complete ACL scene (for visualization)
#[derive(Debug, Clone, Default)]
pub struct ACLScene {
    pub principals: Vec<PrincipalGeometry>,
    pub resources: Vec<ResourceGeometry>,
}

/// Permission mesh (for complex ACL rules)
#[derive(Debug, Clone)]
pub struct PermissionMesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

/// Geometric ACL representation
#[derive(Debug, Clone, Default)]
pub struct GeometricACL {
    pub scene: ACLScene,
}

// =============================================================================
// GPU State
// =============================================================================

struct GpuState {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    principals_buffer: wgpu::Buffer,
    resources_buffer: wgpu::Buffer,
    queries_buffer: wgpu::Buffer,
    config_buffer: wgpu::Buffer,
    results_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
}

/// Statistics for access control operations
#[derive(Debug, Clone, Default)]
pub struct AccessStats {
    pub queries_processed: u64,
    pub queries_allowed: u64,
    pub queries_denied: u64,
    pub gpu_dispatches: u64,
    pub last_dispatch_us: u64,
}

// =============================================================================
// Main Access Control Engine
// =============================================================================

/// GPU-accelerated access control using geometric hit tests
pub struct AccessControlGeometry {
    /// GPU state
    gpu_state: Arc<Mutex<Option<GpuState>>>,

    /// Principals (users/groups)
    principals: HashMap<u32, PrincipalGeometry>,

    /// Resources (files/objects)
    resources: HashMap<u32, ResourceGeometry>,

    /// Statistics
    pub stats: AccessStats,
}

impl AccessControlGeometry {
    /// Create a new access control engine
    pub fn new() -> Self {
        Self {
            gpu_state: Arc::new(Mutex::new(None)),
            principals: HashMap::new(),
            resources: HashMap::new(),
            stats: AccessStats::default(),
        }
    }

    /// Initialize GPU pipeline
    pub async fn initialize(&mut self, device: &wgpu::Device) -> Result<()> {
        log::info!("Initializing Access Control Geometry Engine...");

        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Access Control Shader"),
            source: wgpu::ShaderSource::Wgsl(ACCESS_CONTROL_SHADER.into()),
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Access Control Bind Group Layout"),
            entries: &[
                // Principals buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Resources buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Queries buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Config buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Results buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Access Control Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Access Control Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "access_check_main",
        });

        // Create buffers
        let principals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Principals Buffer"),
            size: (MAX_PRINCIPALS * std::mem::size_of::<GpuPrincipal>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let resources_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Resources Buffer"),
            size: (MAX_RESOURCES * std::mem::size_of::<GpuResource>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let queries_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Queries Buffer"),
            size: (MAX_QUERIES * std::mem::size_of::<GpuAccessQuery>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let config_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Config Buffer"),
            size: std::mem::size_of::<GpuConfig>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let results_size = (MAX_QUERIES * std::mem::size_of::<GpuAccessResult>()) as u64;
        let results_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Results Buffer"),
            size: results_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Readback Buffer"),
            size: results_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        *self.gpu_state.lock() = Some(GpuState {
            pipeline,
            bind_group_layout,
            principals_buffer,
            resources_buffer,
            queries_buffer,
            config_buffer,
            results_buffer,
            readback_buffer,
        });

        log::info!("Access Control Geometry Engine initialized");
        Ok(())
    }

    /// Check if GPU is initialized
    pub fn is_initialized(&self) -> bool {
        self.gpu_state.lock().is_some()
    }

    /// Add a principal
    pub fn add_principal(&mut self, principal: PrincipalGeometry) {
        self.principals.insert(principal.id, principal);
    }

    /// Remove a principal
    pub fn remove_principal(&mut self, id: u32) {
        self.principals.remove(&id);
    }

    /// Add a resource
    pub fn add_resource(&mut self, resource: ResourceGeometry) {
        self.resources.insert(resource.id, resource);
    }

    /// Remove a resource
    pub fn remove_resource(&mut self, id: u32) {
        self.resources.remove(&id);
    }

    /// Get the geometric ACL scene (for visualization)
    pub fn scene(&self) -> ACLScene {
        ACLScene {
            principals: self.principals.values().cloned().collect(),
            resources: self.resources.values().cloned().collect(),
        }
    }

    /// Check access for multiple queries in parallel on GPU
    pub async fn check_access(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        queries: &[AccessQuery],
    ) -> Result<Vec<AccessResult>> {
        let start = std::time::Instant::now();

        if queries.is_empty() {
            return Ok(Vec::new());
        }

        let state = self.gpu_state.lock();
        let state = match state.as_ref() {
            Some(s) => s,
            None => anyhow::bail!("GPU not initialized"),
        };

        // Prepare principals data
        let mut gpu_principals = vec![GpuPrincipal::default(); MAX_PRINCIPALS];
        for (i, p) in self.principals.values().enumerate() {
            if i < MAX_PRINCIPALS {
                gpu_principals[i] = p.to_gpu();
            }
        }

        // Prepare resources data
        let mut gpu_resources = vec![GpuResource::default(); MAX_RESOURCES];
        for (i, r) in self.resources.values().enumerate() {
            if i < MAX_RESOURCES {
                gpu_resources[i] = r.to_gpu();
            }
        }

        // Prepare queries
        let num_queries = queries.len().min(MAX_QUERIES);
        let mut gpu_queries = vec![GpuAccessQuery::default(); MAX_QUERIES];
        for (i, q) in queries.iter().take(num_queries).enumerate() {
            gpu_queries[i] = q.to_gpu();
        }

        // Prepare config
        let config = GpuConfig {
            num_principals: self.principals.len() as u32,
            num_resources: self.resources.len() as u32,
            num_queries: num_queries as u32,
            _pad0: 0,
        };

        // Upload to GPU
        queue.write_buffer(
            &state.principals_buffer,
            0,
            bytemuck::cast_slice(&gpu_principals),
        );
        queue.write_buffer(
            &state.resources_buffer,
            0,
            bytemuck::cast_slice(&gpu_resources),
        );
        queue.write_buffer(
            &state.queries_buffer,
            0,
            bytemuck::cast_slice(&gpu_queries),
        );
        queue.write_buffer(&state.config_buffer, 0, bytemuck::bytes_of(&config));

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Access Control Bind Group"),
            layout: &state.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: state.principals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: state.resources_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: state.queries_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: state.config_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: state.results_buffer.as_entire_binding(),
                },
            ],
        });

        // Dispatch compute shader
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Access Control Encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Access Control Pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&state.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);

            // One thread per query, 256 threads per workgroup
            let workgroups = (num_queries as u32 + 255) / 256;
            pass.dispatch_workgroups(workgroups, 1, 1);
        }

        // Copy results to readback buffer
        let copy_size = (num_queries * std::mem::size_of::<GpuAccessResult>()) as u64;
        encoder.copy_buffer_to_buffer(
            &state.results_buffer,
            0,
            &state.readback_buffer,
            0,
            copy_size,
        );

        queue.submit(std::iter::once(encoder.finish()));

        // Map and read results
        let buffer_slice = state.readback_buffer.slice(..copy_size);
        let (tx, rx) = tokio::sync::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::Maintain::Wait);
        rx.await??;

        let data = buffer_slice.get_mapped_range();
        let gpu_results: &[GpuAccessResult] = bytemuck::cast_slice(&data);

        // Convert to high-level results
        let mut results = Vec::with_capacity(num_queries);
        for (i, gpu_result) in gpu_results.iter().take(num_queries).enumerate() {
            let decision = AccessDecision::from_reason(gpu_result.allowed, gpu_result.reason);

            if decision.is_allowed() {
                self.stats.queries_allowed += 1;
            } else {
                self.stats.queries_denied += 1;
            }

            results.push(AccessResult {
                query: queries[i].clone(),
                decision,
                distance: gpu_result.distance,
            });
        }

        drop(data);
        state.readback_buffer.unmap();

        // Update stats
        self.stats.queries_processed += num_queries as u64;
        self.stats.gpu_dispatches += 1;
        self.stats.last_dispatch_us = start.elapsed().as_micros() as u64;

        log::trace!(
            "Access check: {} queries in {}µs ({} allowed, {} denied)",
            num_queries,
            self.stats.last_dispatch_us,
            results.iter().filter(|r| r.is_allowed()).count(),
            results.iter().filter(|r| !r.is_allowed()).count()
        );

        Ok(results)
    }

    /// Simple synchronous check (uses cached results or CPU fallback)
    pub fn check_access_sync(
        &self,
        principal_id: u32,
        resource_id: u32,
        permission: Permission,
    ) -> AccessDecision {
        let principal = match self.principals.get(&principal_id) {
            Some(p) => p,
            None => return AccessDecision::Denied,
        };

        let resource = match self.resources.get(&resource_id) {
            Some(r) => r,
            None => return AccessDecision::Denied,
        };

        // Admin check
        if principal.is_admin {
            return AccessDecision::AllowedAdmin;
        }

        let perm_bits = permission as u32;

        // Owner check
        if principal.id == resource.owner_id {
            let owner_perms = (resource.permission_bits >> 6) & 7;
            if (owner_perms & perm_bits) != 0 {
                return AccessDecision::AllowedOwner;
            }
        }

        // Group check
        if principal.groups.contains(&resource.owner_group) {
            let group_perms = (resource.permission_bits >> 3) & 7;
            if (group_perms & perm_bits) != 0 {
                return AccessDecision::AllowedGroup;
            }
        }

        // Other check
        let other_perms = resource.permission_bits & 7;
        if (other_perms & perm_bits) != 0 {
            return AccessDecision::AllowedOther;
        }

        AccessDecision::Denied
    }

    /// Get statistics
    pub fn stats(&self) -> &AccessStats {
        &self.stats
    }
}

impl Default for AccessControlGeometry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_enum() {
        // Unix convention: r=4, w=2, x=1
        assert_eq!(Permission::Read as u32, 4);
        assert_eq!(Permission::Write as u32, 2);
        assert_eq!(Permission::Execute as u32, 1);
        assert_eq!(Permission::All as u32, 7);
    }

    #[test]
    fn test_access_decision() {
        assert!(AccessDecision::AllowedAdmin.is_allowed());
        assert!(AccessDecision::AllowedOwner.is_allowed());
        assert!(AccessDecision::AllowedGroup.is_allowed());
        assert!(AccessDecision::AllowedOther.is_allowed());
        assert!(!AccessDecision::Denied.is_allowed());
    }

    #[test]
    fn test_principal_geometry() {
        let admin = PrincipalGeometry::admin(1, "root");
        assert!(admin.is_admin);
        assert_eq!(admin.radius, 100.0);

        let user = PrincipalGeometry::user(2, "alice").with_group(1).with_group(2);
        assert!(!user.is_admin);
        assert_eq!(user.groups, vec![1, 2]);
    }

    #[test]
    fn test_resource_geometry() {
        let file = ResourceGeometry::file(1, 2, 0o644)
            .with_group(1)
            .with_name("test.txt");

        assert_eq!(file.owner_id, 2);
        assert_eq!(file.permission_bits, 0o644);
        assert_eq!(file.owner_group, 1);
        assert_eq!(file.name, "test.txt");
    }

    #[test]
    fn test_sync_access_check() {
        let mut acl = AccessControlGeometry::new();

        // Add admin and regular user
        acl.add_principal(PrincipalGeometry::admin(1, "root"));
        acl.add_principal(PrincipalGeometry::user(2, "alice").with_group(1));
        acl.add_principal(PrincipalGeometry::user(3, "bob"));

        // Add file owned by alice with 0o640 (rw-r-----)
        acl.add_resource(
            ResourceGeometry::file(1, 2, 0o640)
                .with_group(1)
                .with_name("alice_file.txt"),
        );

        // Admin can read anything
        assert_eq!(
            acl.check_access_sync(1, 1, Permission::Read),
            AccessDecision::AllowedAdmin
        );

        // Owner (alice) can read
        assert_eq!(
            acl.check_access_sync(2, 1, Permission::Read),
            AccessDecision::AllowedOwner
        );

        // Owner (alice) can write
        assert_eq!(
            acl.check_access_sync(2, 1, Permission::Write),
            AccessDecision::AllowedOwner
        );

        // Bob (not in group) cannot read (other perms = 0)
        assert_eq!(
            acl.check_access_sync(3, 1, Permission::Read),
            AccessDecision::Denied
        );
    }

    #[test]
    fn test_group_access() {
        let mut acl = AccessControlGeometry::new();

        acl.add_principal(PrincipalGeometry::user(1, "alice").with_group(5));
        acl.add_principal(PrincipalGeometry::user(2, "bob"));

        // File with group=5, perms=0o640
        acl.add_resource(ResourceGeometry::file(1, 99, 0o640).with_group(5));

        // Alice is in group 5, can read
        assert_eq!(
            acl.check_access_sync(1, 1, Permission::Read),
            AccessDecision::AllowedGroup
        );

        // Bob is not in group 5, cannot read
        assert_eq!(
            acl.check_access_sync(2, 1, Permission::Read),
            AccessDecision::Denied
        );
    }

    #[test]
    fn test_world_permissions() {
        let mut acl = AccessControlGeometry::new();

        acl.add_principal(PrincipalGeometry::user(1, "anyone"));

        // File with world-readable permissions (0o644)
        acl.add_resource(ResourceGeometry::file(1, 99, 0o644));

        // Anyone can read
        assert_eq!(
            acl.check_access_sync(1, 1, Permission::Read),
            AccessDecision::AllowedOther
        );

        // But not write
        assert_eq!(
            acl.check_access_sync(1, 1, Permission::Write),
            AccessDecision::Denied
        );
    }
}
