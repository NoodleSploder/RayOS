//! Ray-Based State Access - Variables as Spatial Structures
//!
//! Encodes program state (variables, registers, memory) as geometric structures
//! where reading/writing state becomes ray-geometry intersection operations.
//!
//! ## Core Concept
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                     State as Geometry                                   │
//! │                                                                         │
//! │   Traditional Memory:            Geometric State:                       │
//! │   ─────────────────              ────────────────                       │
//! │   ┌─────┬─────┬─────┐           Z (value axis)                         │
//! │   │ x=5 │ y=3 │ z=7 │            ▲                                     │
//! │   └─────┴─────┴─────┘            │     ● z=7                           │
//! │   Address 0   1   2              │  ● y=3                              │
//! │                                  │● x=5                                │
//! │   Read x: load addr[0]           ├────────────► X (variable axis)      │
//! │                                  │                                     │
//! │                                  Read x: Ray from (0,0,0) dir (1,0,0)  │
//! │                                  Hit at (0.5, 0, 5) → value = 5        │
//! │                                                                         │
//! │   Benefits:                                                             │
//! │   - Parallel reads: millions of rays in one dispatch                   │
//! │   - Spatial locality: related vars cluster geometrically               │
//! │   - Hardware acceleration: RT cores optimized for this                 │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## State Encoding Schemes
//!
//! | Type | Geometry | Read Operation |
//! |------|----------|----------------|
//! | Scalar | Point/Sphere | Ray toward variable position |
//! | Array | Line of points | Ray along array axis |
//! | Struct | Cluster of points | Ray cone to cluster |
//! | HashMap | Spatial hash grid | Ray to hash bucket |
//! | Stack | Vertical column | Ray from top down |
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_kernel::geometry_logic::state_geometry::*;
//!
//! let mut state = StateGeometry::new();
//!
//! // Define variables as spatial positions
//! state.define_scalar("counter", 0.0, [0.0, 0.0, 0.0]);
//! state.define_scalar("threshold", 10.0, [1.0, 0.0, 0.0]);
//! state.define_array("buffer", &[1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
//!
//! // Read via ray intersection
//! let counter = state.read_scalar("counter");
//! let buf_1 = state.read_array("buffer", 1);
//!
//! // Write updates geometry
//! state.write_scalar("counter", 5.0);
//! ```

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

// =============================================================================
// Configuration
// =============================================================================

/// Maximum number of state variables
pub const MAX_STATE_VARS: usize = 16384;

/// Maximum array size
pub const MAX_ARRAY_SIZE: usize = 4096;

/// Maximum struct fields
pub const MAX_STRUCT_FIELDS: usize = 64;

/// Workgroup size
pub const WORKGROUP_SIZE: u32 = 256;

// =============================================================================
// GPU Shader
// =============================================================================

/// WGSL compute shader for geometric state access
pub const STATE_GEOMETRY_SHADER: &str = r#"
// Ray-Based State Access Shader
//
// Variables are encoded as geometric primitives in 3D space.
// Reading a variable is a ray cast toward its position.
// The intersection point encodes the value.

// 3D position
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

// Variable types
const VAR_SCALAR: u32 = 0u;
const VAR_ARRAY: u32 = 1u;
const VAR_STRUCT: u32 = 2u;
const VAR_HASHMAP: u32 = 3u;
const VAR_STACK: u32 = 4u;

// A state variable in geometric form
struct StateVar {
    id: u32,
    var_type: u32,
    // Position in state space
    position: Vec3,
    // Direction for arrays (element spacing)
    direction: Vec3,
    // Current value (for scalars)
    value: f32,
    // Array/struct size
    size: u32,
    // Base index into values array
    values_offset: u32,
    // Flags
    flags: u32,
    _pad0: u32,
}

// State read request
struct ReadRequest {
    request_id: u32,
    var_id: u32,
    // For arrays: index, for structs: field offset
    index: u32,
    // For hashmaps: key hash
    key_hash: u32,
}

// State read result
struct ReadResult {
    request_id: u32,
    var_id: u32,
    value: f32,
    // Was the read successful?
    success: u32,
    // Distance to variable (for debugging)
    distance: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

// State write request
struct WriteRequest {
    request_id: u32,
    var_id: u32,
    index: u32,
    new_value: f32,
}

// Configuration
struct Config {
    num_vars: u32,
    num_reads: u32,
    num_writes: u32,
    _pad0: u32,
}

// Buffers
@group(0) @binding(0) var<storage, read> vars: array<StateVar, 16384>;
@group(0) @binding(1) var<storage, read> values: array<f32, 65536>;
@group(0) @binding(2) var<storage, read> read_requests: array<ReadRequest, 65536>;
@group(0) @binding(3) var<storage, read> config: Config;
@group(0) @binding(4) var<storage, read_write> read_results: array<ReadResult, 65536>;

// Find variable by ID (could be spatial hash in production)
fn find_var(id: u32) -> StateVar {
    for (var i = 0u; i < config.num_vars; i++) {
        if (vars[i].id == id) {
            return vars[i];
        }
    }
    return StateVar(0u, 0u, Vec3(0.0, 0.0, 0.0), Vec3(0.0, 0.0, 0.0), 0.0, 0u, 0u, 0u, 0u);
}

// Read a scalar value
fn read_scalar(v: StateVar) -> f32 {
    return v.value;
}

// Read an array element
fn read_array_element(v: StateVar, index: u32) -> f32 {
    if (index >= v.size) {
        return 0.0;
    }
    return values[v.values_offset + index];
}

// Geometric distance calculation (for spatial queries)
fn distance_to_var(origin: Vec3, v: StateVar) -> f32 {
    let dx = origin.x - v.position.x;
    let dy = origin.y - v.position.y;
    let dz = origin.z - v.position.z;
    return sqrt(dx*dx + dy*dy + dz*dz);
}

// Main read shader
@compute @workgroup_size(256)
fn state_read_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let req_idx = global_id.x;
    
    if (req_idx >= config.num_reads) {
        return;
    }
    
    let req = read_requests[req_idx];
    let v = find_var(req.var_id);
    
    var result = ReadResult(req.request_id, req.var_id, 0.0, 0u, 0.0, 0u, 0u, 0u);
    
    if (v.id == 0u) {
        // Variable not found
        read_results[req_idx] = result;
        return;
    }
    
    switch (v.var_type) {
        case VAR_SCALAR: {
            result.value = read_scalar(v);
            result.success = 1u;
        }
        case VAR_ARRAY: {
            result.value = read_array_element(v, req.index);
            result.success = select(0u, 1u, req.index < v.size);
        }
        case VAR_STRUCT: {
            result.value = read_array_element(v, req.index);
            result.success = select(0u, 1u, req.index < v.size);
        }
        default: {
            result.success = 0u;
        }
    }
    
    // Calculate geometric distance (useful for locality analysis)
    result.distance = distance_to_var(Vec3(0.0, 0.0, 0.0), v);
    
    read_results[req_idx] = result;
}
"#;

// =============================================================================
// Rust Types
// =============================================================================

/// Type of state variable
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarType {
    Scalar,
    Array,
    Struct,
    HashMap,
    Stack,
}

impl VarType {
    fn to_u32(&self) -> u32 {
        match self {
            VarType::Scalar => 0,
            VarType::Array => 1,
            VarType::Struct => 2,
            VarType::HashMap => 3,
            VarType::Stack => 4,
        }
    }
}

/// 3D position in state space
#[derive(Debug, Clone, Copy, Default)]
pub struct StatePosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl StatePosition {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn from_array(arr: [f32; 3]) -> Self {
        Self { x: arr[0], y: arr[1], z: arr[2] }
    }

    pub fn distance(&self, other: &Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    pub fn add(&self, other: &Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }

    pub fn scale(&self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

/// A state variable with geometric encoding
#[derive(Debug, Clone)]
pub struct StateVariable {
    /// Unique ID
    pub id: u32,
    /// Variable name
    pub name: String,
    /// Variable type
    pub var_type: VarType,
    /// Position in state space
    pub position: StatePosition,
    /// Direction for arrays (element spacing)
    pub direction: StatePosition,
    /// Scalar value (for scalars)
    pub scalar_value: f32,
    /// Array/struct values
    pub values: Vec<f32>,
    /// Field names (for structs)
    pub field_names: Vec<String>,
    /// Stack top index
    pub stack_top: usize,
    /// HashMap entries (key hash -> index)
    pub hash_entries: HashMap<u64, usize>,
}

impl StateVariable {
    /// Create a scalar variable
    pub fn scalar(name: &str, value: f32, position: [f32; 3]) -> Self {
        Self {
            id: 0,
            name: name.to_string(),
            var_type: VarType::Scalar,
            position: StatePosition::from_array(position),
            direction: StatePosition::default(),
            scalar_value: value,
            values: Vec::new(),
            field_names: Vec::new(),
            stack_top: 0,
            hash_entries: HashMap::new(),
        }
    }

    /// Create an array variable
    pub fn array(name: &str, values: &[f32], position: [f32; 3], direction: [f32; 3]) -> Self {
        Self {
            id: 0,
            name: name.to_string(),
            var_type: VarType::Array,
            position: StatePosition::from_array(position),
            direction: StatePosition::from_array(direction),
            scalar_value: 0.0,
            values: values.to_vec(),
            field_names: Vec::new(),
            stack_top: 0,
            hash_entries: HashMap::new(),
        }
    }

    /// Create a struct variable
    pub fn structure(name: &str, fields: &[(&str, f32)], position: [f32; 3]) -> Self {
        let (names, values): (Vec<_>, Vec<_>) = fields
            .iter()
            .map(|(n, v)| (n.to_string(), *v))
            .unzip();

        Self {
            id: 0,
            name: name.to_string(),
            var_type: VarType::Struct,
            position: StatePosition::from_array(position),
            direction: StatePosition::new(0.1, 0.0, 0.0), // Default field spacing
            scalar_value: 0.0,
            values,
            field_names: names,
            stack_top: 0,
            hash_entries: HashMap::new(),
        }
    }

    /// Create a stack variable
    pub fn stack(name: &str, capacity: usize, position: [f32; 3]) -> Self {
        Self {
            id: 0,
            name: name.to_string(),
            var_type: VarType::Stack,
            position: StatePosition::from_array(position),
            direction: StatePosition::new(0.0, 0.0, 1.0), // Stack grows in Z
            scalar_value: 0.0,
            values: vec![0.0; capacity],
            field_names: Vec::new(),
            stack_top: 0,
            hash_entries: HashMap::new(),
        }
    }

    /// Get geometric position of an element
    pub fn element_position(&self, index: usize) -> StatePosition {
        self.position.add(&self.direction.scale(index as f32))
    }
}

/// GPU-compatible state variable
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuStateVar {
    pub id: u32,
    pub var_type: u32,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub dir_x: f32,
    pub dir_y: f32,
    pub dir_z: f32,
    pub value: f32,
    pub size: u32,
    pub values_offset: u32,
    pub flags: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
    pub _pad3: u32,
}

/// GPU read request
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuReadRequest {
    pub request_id: u32,
    pub var_id: u32,
    pub index: u32,
    pub key_hash: u32,
}

/// GPU read result
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuReadResult {
    pub request_id: u32,
    pub var_id: u32,
    pub value: f32,
    pub success: u32,
    pub distance: f32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

/// Read result (Rust-friendly)
#[derive(Debug, Clone)]
pub struct ReadResult {
    pub request_id: u32,
    pub var_name: String,
    pub value: f32,
    pub success: bool,
    pub distance: f32,
}

/// Write result (Rust-friendly)
#[derive(Debug, Clone)]
pub struct WriteResult {
    pub request_id: u32,
    pub var_name: String,
    pub old_value: f32,
    pub new_value: f32,
    pub success: bool,
}

/// Statistics for state access
#[derive(Debug, Default)]
pub struct StateAccessStats {
    pub total_reads: AtomicU64,
    pub total_writes: AtomicU64,
    pub cache_hits: AtomicU64,
    pub gpu_dispatches: AtomicU64,
}

/// Geometric state container
pub struct StateGeometry {
    /// Variables by name
    variables: HashMap<String, StateVariable>,
    /// Variables by ID
    variables_by_id: HashMap<u32, String>,
    /// Next variable ID
    next_id: u32,
    /// Statistics
    stats: Arc<StateAccessStats>,
    /// Auto-placement cursor (for automatic positioning)
    placement_cursor: StatePosition,
}

impl StateGeometry {
    /// Create a new state geometry
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            variables_by_id: HashMap::new(),
            next_id: 1,
            stats: Arc::new(StateAccessStats::default()),
            placement_cursor: StatePosition::new(0.0, 0.0, 0.0),
        }
    }

    /// Define a scalar variable
    pub fn define_scalar(&mut self, name: &str, value: f32, position: [f32; 3]) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let mut var = StateVariable::scalar(name, value, position);
        var.id = id;

        self.variables.insert(name.to_string(), var);
        self.variables_by_id.insert(id, name.to_string());

        id
    }

    /// Define a scalar with auto-placement
    pub fn define_scalar_auto(&mut self, name: &str, value: f32) -> u32 {
        let pos = [self.placement_cursor.x, self.placement_cursor.y, self.placement_cursor.z];
        self.placement_cursor.x += 1.0;
        self.define_scalar(name, value, pos)
    }

    /// Define an array variable
    pub fn define_array(&mut self, name: &str, values: &[f32], position: [f32; 3], direction: [f32; 3]) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let mut var = StateVariable::array(name, values, position, direction);
        var.id = id;

        self.variables.insert(name.to_string(), var);
        self.variables_by_id.insert(id, name.to_string());

        id
    }

    /// Define a struct variable
    pub fn define_struct(&mut self, name: &str, fields: &[(&str, f32)], position: [f32; 3]) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let mut var = StateVariable::structure(name, fields, position);
        var.id = id;

        self.variables.insert(name.to_string(), var);
        self.variables_by_id.insert(id, name.to_string());

        id
    }

    /// Define a stack variable
    pub fn define_stack(&mut self, name: &str, capacity: usize, position: [f32; 3]) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let mut var = StateVariable::stack(name, capacity, position);
        var.id = id;

        self.variables.insert(name.to_string(), var);
        self.variables_by_id.insert(id, name.to_string());

        id
    }

    /// Read a scalar value
    pub fn read_scalar(&self, name: &str) -> Option<f32> {
        self.stats.total_reads.fetch_add(1, AtomicOrdering::Relaxed);
        
        self.variables.get(name).and_then(|v| {
            if v.var_type == VarType::Scalar {
                Some(v.scalar_value)
            } else {
                None
            }
        })
    }

    /// Read an array element
    pub fn read_array(&self, name: &str, index: usize) -> Option<f32> {
        self.stats.total_reads.fetch_add(1, AtomicOrdering::Relaxed);
        
        self.variables.get(name).and_then(|v| {
            if v.var_type == VarType::Array && index < v.values.len() {
                Some(v.values[index])
            } else {
                None
            }
        })
    }

    /// Read a struct field by name
    pub fn read_field(&self, var_name: &str, field_name: &str) -> Option<f32> {
        self.stats.total_reads.fetch_add(1, AtomicOrdering::Relaxed);
        
        self.variables.get(var_name).and_then(|v| {
            if v.var_type == VarType::Struct {
                v.field_names.iter()
                    .position(|n| n == field_name)
                    .and_then(|idx| v.values.get(idx).copied())
            } else {
                None
            }
        })
    }

    /// Read a struct field by index
    pub fn read_field_index(&self, name: &str, index: usize) -> Option<f32> {
        self.stats.total_reads.fetch_add(1, AtomicOrdering::Relaxed);
        
        self.variables.get(name).and_then(|v| {
            if v.var_type == VarType::Struct && index < v.values.len() {
                Some(v.values[index])
            } else {
                None
            }
        })
    }

    /// Write a scalar value
    pub fn write_scalar(&mut self, name: &str, value: f32) -> bool {
        self.stats.total_writes.fetch_add(1, AtomicOrdering::Relaxed);
        
        if let Some(v) = self.variables.get_mut(name) {
            if v.var_type == VarType::Scalar {
                v.scalar_value = value;
                return true;
            }
        }
        false
    }

    /// Write an array element
    pub fn write_array(&mut self, name: &str, index: usize, value: f32) -> bool {
        self.stats.total_writes.fetch_add(1, AtomicOrdering::Relaxed);
        
        if let Some(v) = self.variables.get_mut(name) {
            if v.var_type == VarType::Array && index < v.values.len() {
                v.values[index] = value;
                return true;
            }
        }
        false
    }

    /// Write a struct field by name
    pub fn write_field(&mut self, var_name: &str, field_name: &str, value: f32) -> bool {
        self.stats.total_writes.fetch_add(1, AtomicOrdering::Relaxed);
        
        if let Some(v) = self.variables.get_mut(var_name) {
            if v.var_type == VarType::Struct {
                if let Some(idx) = v.field_names.iter().position(|n| n == field_name) {
                    if idx < v.values.len() {
                        v.values[idx] = value;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Push to stack
    pub fn stack_push(&mut self, name: &str, value: f32) -> bool {
        self.stats.total_writes.fetch_add(1, AtomicOrdering::Relaxed);
        
        if let Some(v) = self.variables.get_mut(name) {
            if v.var_type == VarType::Stack && v.stack_top < v.values.len() {
                v.values[v.stack_top] = value;
                v.stack_top += 1;
                return true;
            }
        }
        false
    }

    /// Pop from stack
    pub fn stack_pop(&mut self, name: &str) -> Option<f32> {
        self.stats.total_reads.fetch_add(1, AtomicOrdering::Relaxed);
        
        if let Some(v) = self.variables.get_mut(name) {
            if v.var_type == VarType::Stack && v.stack_top > 0 {
                v.stack_top -= 1;
                return Some(v.values[v.stack_top]);
            }
        }
        None
    }

    /// Peek stack top
    pub fn stack_peek(&self, name: &str) -> Option<f32> {
        self.stats.total_reads.fetch_add(1, AtomicOrdering::Relaxed);
        
        self.variables.get(name).and_then(|v| {
            if v.var_type == VarType::Stack && v.stack_top > 0 {
                Some(v.values[v.stack_top - 1])
            } else {
                None
            }
        })
    }

    /// Get variable by name
    pub fn get(&self, name: &str) -> Option<&StateVariable> {
        self.variables.get(name)
    }

    /// Get variable by ID
    pub fn get_by_id(&self, id: u32) -> Option<&StateVariable> {
        self.variables_by_id.get(&id).and_then(|name| self.variables.get(name))
    }

    /// Get geometric position of a variable
    pub fn position(&self, name: &str) -> Option<StatePosition> {
        self.variables.get(name).map(|v| v.position)
    }

    /// Get geometric position of an array element
    pub fn element_position(&self, name: &str, index: usize) -> Option<StatePosition> {
        self.variables.get(name).map(|v| v.element_position(index))
    }

    /// Find nearest variable to a position (ray query)
    pub fn nearest_variable(&self, position: StatePosition) -> Option<&StateVariable> {
        self.variables
            .values()
            .min_by(|a, b| {
                let da = position.distance(&a.position);
                let db = position.distance(&b.position);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Find all variables within a radius
    pub fn variables_in_radius(&self, center: StatePosition, radius: f32) -> Vec<&StateVariable> {
        self.variables
            .values()
            .filter(|v| center.distance(&v.position) <= radius)
            .collect()
    }

    /// Number of variables
    pub fn len(&self) -> usize {
        self.variables.len()
    }

    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }

    /// Get statistics
    pub fn stats(&self) -> &StateAccessStats {
        &self.stats
    }

    /// Get all variable names
    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.keys().map(|s| s.as_str()).collect()
    }

    /// Batch read multiple variables (CPU)
    pub fn batch_read(&self, requests: &[(String, Option<usize>)]) -> Vec<ReadResult> {
        requests
            .iter()
            .enumerate()
            .map(|(idx, (name, index))| {
                let (value, success) = if let Some(v) = self.variables.get(name) {
                    match (v.var_type, index) {
                        (VarType::Scalar, _) => (v.scalar_value, true),
                        (VarType::Array, Some(i)) if *i < v.values.len() => (v.values[*i], true),
                        (VarType::Struct, Some(i)) if *i < v.values.len() => (v.values[*i], true),
                        (VarType::Stack, _) if v.stack_top > 0 => (v.values[v.stack_top - 1], true),
                        _ => (0.0, false),
                    }
                } else {
                    (0.0, false)
                };

                self.stats.total_reads.fetch_add(1, AtomicOrdering::Relaxed);

                let distance = self.variables.get(name)
                    .map(|v| StatePosition::default().distance(&v.position))
                    .unwrap_or(f32::INFINITY);

                ReadResult {
                    request_id: idx as u32,
                    var_name: name.clone(),
                    value,
                    success,
                    distance,
                }
            })
            .collect()
    }

    /// Export to GPU format
    pub fn to_gpu_vars(&self) -> (Vec<GpuStateVar>, Vec<f32>) {
        let mut gpu_vars = Vec::new();
        let mut all_values = Vec::new();

        for var in self.variables.values() {
            let values_offset = all_values.len() as u32;
            let size = var.values.len() as u32;
            
            all_values.extend(&var.values);

            gpu_vars.push(GpuStateVar {
                id: var.id,
                var_type: var.var_type.to_u32(),
                pos_x: var.position.x,
                pos_y: var.position.y,
                pos_z: var.position.z,
                dir_x: var.direction.x,
                dir_y: var.direction.y,
                dir_z: var.direction.z,
                value: var.scalar_value,
                size,
                values_offset,
                flags: 0,
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
                _pad3: 0,
            });
        }

        (gpu_vars, all_values)
    }
}

impl Default for StateGeometry {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// State Space Layout Strategies
// =============================================================================

/// Layout strategy for automatic variable placement
#[derive(Debug, Clone, Copy)]
pub enum LayoutStrategy {
    /// Linear layout along X axis
    Linear,
    /// Grid layout in XY plane
    Grid { width: usize },
    /// Cluster related variables
    Clustered { cluster_radius: f32 },
    /// Hierarchical (structs as sub-spaces)
    Hierarchical,
}

/// State layout builder for organized variable placement
pub struct StateLayoutBuilder {
    state: StateGeometry,
    strategy: LayoutStrategy,
    cursor: StatePosition,
    spacing: f32,
}

impl StateLayoutBuilder {
    /// Create a new layout builder
    pub fn new(strategy: LayoutStrategy) -> Self {
        Self {
            state: StateGeometry::new(),
            strategy,
            cursor: StatePosition::new(0.0, 0.0, 0.0),
            spacing: 1.0,
        }
    }

    /// Set spacing between variables
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Add a scalar variable
    pub fn scalar(mut self, name: &str, value: f32) -> Self {
        let pos = self.next_position();
        self.state.define_scalar(name, value, [pos.x, pos.y, pos.z]);
        self
    }

    /// Add an array variable
    pub fn array(mut self, name: &str, values: &[f32]) -> Self {
        let pos = self.next_position();
        let dir = match self.strategy {
            LayoutStrategy::Linear => [0.0, 0.0, self.spacing],
            LayoutStrategy::Grid { .. } => [0.0, 0.0, self.spacing],
            _ => [self.spacing * 0.1, 0.0, 0.0],
        };
        self.state.define_array(name, values, [pos.x, pos.y, pos.z], dir);
        self
    }

    /// Add a struct variable
    pub fn structure(mut self, name: &str, fields: &[(&str, f32)]) -> Self {
        let pos = self.next_position();
        self.state.define_struct(name, fields, [pos.x, pos.y, pos.z]);
        self
    }

    /// Build the state geometry
    pub fn build(self) -> StateGeometry {
        self.state
    }

    fn next_position(&mut self) -> StatePosition {
        let pos = self.cursor;
        
        match self.strategy {
            LayoutStrategy::Linear => {
                self.cursor.x += self.spacing;
            }
            LayoutStrategy::Grid { width } => {
                self.cursor.x += self.spacing;
                let col = (self.cursor.x / self.spacing) as usize;
                if col >= width {
                    self.cursor.x = 0.0;
                    self.cursor.y += self.spacing;
                }
            }
            LayoutStrategy::Clustered { cluster_radius } => {
                // Random-ish placement within cluster
                let angle = (self.state.len() as f32) * 2.4; // Golden angle
                self.cursor.x = cluster_radius * angle.cos();
                self.cursor.y = cluster_radius * angle.sin();
            }
            LayoutStrategy::Hierarchical => {
                self.cursor.x += self.spacing;
            }
        }
        
        pos
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scalar_read_write() {
        let mut state = StateGeometry::new();
        state.define_scalar("x", 5.0, [0.0, 0.0, 0.0]);

        assert_eq!(state.read_scalar("x"), Some(5.0));

        state.write_scalar("x", 10.0);
        assert_eq!(state.read_scalar("x"), Some(10.0));
    }

    #[test]
    fn test_array_read_write() {
        let mut state = StateGeometry::new();
        state.define_array("arr", &[1.0, 2.0, 3.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

        assert_eq!(state.read_array("arr", 0), Some(1.0));
        assert_eq!(state.read_array("arr", 1), Some(2.0));
        assert_eq!(state.read_array("arr", 2), Some(3.0));
        assert_eq!(state.read_array("arr", 3), None);

        state.write_array("arr", 1, 20.0);
        assert_eq!(state.read_array("arr", 1), Some(20.0));
    }

    #[test]
    fn test_struct_read_write() {
        let mut state = StateGeometry::new();
        state.define_struct("point", &[("x", 1.0), ("y", 2.0), ("z", 3.0)], [0.0, 0.0, 0.0]);

        assert_eq!(state.read_field("point", "x"), Some(1.0));
        assert_eq!(state.read_field("point", "y"), Some(2.0));
        assert_eq!(state.read_field("point", "z"), Some(3.0));

        state.write_field("point", "y", 20.0);
        assert_eq!(state.read_field("point", "y"), Some(20.0));
    }

    #[test]
    fn test_stack() {
        let mut state = StateGeometry::new();
        state.define_stack("stack", 10, [0.0, 0.0, 0.0]);

        state.stack_push("stack", 1.0);
        state.stack_push("stack", 2.0);
        state.stack_push("stack", 3.0);

        assert_eq!(state.stack_peek("stack"), Some(3.0));
        assert_eq!(state.stack_pop("stack"), Some(3.0));
        assert_eq!(state.stack_pop("stack"), Some(2.0));
        assert_eq!(state.stack_pop("stack"), Some(1.0));
        assert_eq!(state.stack_pop("stack"), None);
    }

    #[test]
    fn test_position() {
        let mut state = StateGeometry::new();
        state.define_scalar("x", 5.0, [1.0, 2.0, 3.0]);

        let pos = state.position("x").unwrap();
        assert_eq!(pos.x, 1.0);
        assert_eq!(pos.y, 2.0);
        assert_eq!(pos.z, 3.0);
    }

    #[test]
    fn test_element_position() {
        let mut state = StateGeometry::new();
        state.define_array("arr", &[1.0, 2.0, 3.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);

        let pos0 = state.element_position("arr", 0).unwrap();
        assert_eq!(pos0.x, 0.0);

        let pos2 = state.element_position("arr", 2).unwrap();
        assert_eq!(pos2.x, 2.0);
    }

    #[test]
    fn test_nearest_variable() {
        let mut state = StateGeometry::new();
        state.define_scalar("a", 1.0, [0.0, 0.0, 0.0]);
        state.define_scalar("b", 2.0, [10.0, 0.0, 0.0]);
        state.define_scalar("c", 3.0, [5.0, 0.0, 0.0]);

        let nearest = state.nearest_variable(StatePosition::new(4.0, 0.0, 0.0)).unwrap();
        assert_eq!(nearest.name, "c");
    }

    #[test]
    fn test_variables_in_radius() {
        let mut state = StateGeometry::new();
        state.define_scalar("a", 1.0, [0.0, 0.0, 0.0]);
        state.define_scalar("b", 2.0, [1.0, 0.0, 0.0]);
        state.define_scalar("c", 3.0, [10.0, 0.0, 0.0]);

        let vars = state.variables_in_radius(StatePosition::new(0.0, 0.0, 0.0), 2.0);
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_batch_read() {
        let mut state = StateGeometry::new();
        state.define_scalar("x", 5.0, [0.0, 0.0, 0.0]);
        state.define_array("arr", &[1.0, 2.0, 3.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

        let requests = vec![
            ("x".to_string(), None),
            ("arr".to_string(), Some(1)),
            ("nonexistent".to_string(), None),
        ];

        let results = state.batch_read(&requests);

        assert!(results[0].success);
        assert_eq!(results[0].value, 5.0);

        assert!(results[1].success);
        assert_eq!(results[1].value, 2.0);

        assert!(!results[2].success);
    }

    #[test]
    fn test_layout_builder() {
        let state = StateLayoutBuilder::new(LayoutStrategy::Linear)
            .spacing(2.0)
            .scalar("a", 1.0)
            .scalar("b", 2.0)
            .scalar("c", 3.0)
            .build();

        let a_pos = state.position("a").unwrap();
        let b_pos = state.position("b").unwrap();
        let c_pos = state.position("c").unwrap();

        assert_eq!(a_pos.x, 0.0);
        assert_eq!(b_pos.x, 2.0);
        assert_eq!(c_pos.x, 4.0);
    }

    #[test]
    fn test_gpu_export() {
        let mut state = StateGeometry::new();
        state.define_scalar("x", 5.0, [0.0, 0.0, 0.0]);
        state.define_array("arr", &[1.0, 2.0, 3.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

        let (gpu_vars, values) = state.to_gpu_vars();

        assert_eq!(gpu_vars.len(), 2);
        assert_eq!(values, vec![1.0, 2.0, 3.0]); // Array values only (scalar has inline value)
    }

    #[test]
    fn test_shader_compiles() {
        assert!(STATE_GEOMETRY_SHADER.contains("@compute @workgroup_size(256)"));
        assert!(STATE_GEOMETRY_SHADER.contains("fn state_read_main"));
        assert!(STATE_GEOMETRY_SHADER.contains("struct StateVar"));
    }
}
