//! Pipeline Core - Unified GPU compute infrastructure
//!
//! This module provides the core GPU compute infrastructure that all
//! pipeline stages share. It manages:
//! - GPU device and queue
//! - Compute pipeline compilation
//! - Buffer memory pools
//! - Dispatch scheduling
//! - Async result readback

use anyhow::{Context, Result};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "std-kernel")]
use wgpu;

// =============================================================================
// Configuration
// =============================================================================

/// Maximum pipeline stages that can execute concurrently
pub const MAX_CONCURRENT_STAGES: usize = 8;

/// Maximum buffer handles in the memory pool
pub const MAX_BUFFER_HANDLES: usize = 256;

/// Default workgroup size for compute shaders
pub const DEFAULT_WORKGROUP_SIZE: u32 = 256;

/// Maximum elements per dispatch
pub const MAX_ELEMENTS_PER_DISPATCH: u32 = 1 << 20; // 1M elements

// =============================================================================
// WGSL Shader: Unified Pipeline Dispatcher
// =============================================================================

/// WGSL shader for the unified dispatch coordinator
///
/// This shader coordinates execution across multiple stage types,
/// allowing a single dispatch to process perception, logic, and semantic
/// operations together.
pub const UNIFIED_PIPELINE_SHADER: &str = r#"
// Unified Perception/Logic Pipeline Shader
//
// This shader provides a unified dispatch mechanism for multiple computation types.
// Each thread is assigned to a specific stage based on its global ID.

// Pipeline header describing the dispatch configuration
struct PipelineHeader {
    // Total number of work items across all stages
    total_items: u32,
    // Number of perception items
    perception_count: u32,
    // Number of logic items
    logic_count: u32,
    // Number of semantic items
    semantic_count: u32,
    // Number of custom items
    custom_count: u32,
    // Timestamp for profiling
    timestamp: u32,
    // Flags (bit 0: debug mode)
    flags: u32,
    _padding: u32,
}

// Work item representing a single computation
struct WorkItem {
    // Stage type: 0=perception, 1=logic, 2=semantic, 3=custom
    stage_type: u32,
    // Input data offset in the input buffer
    input_offset: u32,
    // Output data offset in the output buffer
    output_offset: u32,
    // Item-specific parameter
    param: u32,
}

// Result of a single computation
struct WorkResult {
    // Whether the computation succeeded
    success: u32,
    // Stage type that produced this result
    stage_type: u32,
    // Primary result value (interpretation depends on stage)
    primary: f32,
    // Secondary result value
    secondary: f32,
}

@group(0) @binding(0)
var<storage, read> header: PipelineHeader;

@group(0) @binding(1)
var<storage, read> work_items: array<WorkItem>;

@group(0) @binding(2)
var<storage, read> input_data: array<f32>;

@group(0) @binding(3)
var<storage, read_write> output_data: array<WorkResult>;

// =============================================================================
// Perception Stage
// =============================================================================

// Process perception input (e.g., edge detection, motion detection)
fn process_perception(item: WorkItem) -> WorkResult {
    let offset = item.input_offset;
    
    // Simple perception: compute average intensity from input window
    var sum: f32 = 0.0;
    let window_size = min(item.param, 64u);
    
    for (var i: u32 = 0u; i < window_size; i = i + 1u) {
        sum = sum + input_data[offset + i];
    }
    
    let avg = sum / f32(max(window_size, 1u));
    
    // Motion detection: compare with expected baseline (stored after window)
    let baseline = input_data[offset + window_size];
    let motion = abs(avg - baseline);
    
    return WorkResult(
        1u,           // success
        0u,           // stage_type = perception
        avg,          // primary = average intensity
        motion        // secondary = motion magnitude
    );
}

// =============================================================================
// Logic Stage
// =============================================================================

// Process logic input (e.g., access control, reflex matching)
fn process_logic(item: WorkItem) -> WorkResult {
    let offset = item.input_offset;
    
    // Logic encoding:
    // input_data[offset + 0] = user_id (as f32)
    // input_data[offset + 1] = resource_id (as f32)
    // input_data[offset + 2] = permission_bits
    // input_data[offset + 3] = resource_owner_id
    // input_data[offset + 4] = resource_permission_bits
    
    let user_id = bitcast<u32>(input_data[offset + 0u]);
    let resource_id = bitcast<u32>(input_data[offset + 1u]);
    let requested_perm = bitcast<u32>(input_data[offset + 2u]);
    let owner_id = bitcast<u32>(input_data[offset + 3u]);
    let resource_perms = bitcast<u32>(input_data[offset + 4u]);
    
    // Access control logic as geometric hit test
    var granted: u32 = 0u;
    var confidence: f32 = 0.0;
    
    // Check 1: Owner always has full access
    if (user_id == owner_id) {
        granted = 1u;
        confidence = 1.0;
    }
    // Check 2: Permission bits match
    else if ((resource_perms & requested_perm) == requested_perm) {
        granted = 1u;
        confidence = 0.9;
    }
    // Check 3: Admin flag (user_id MSB set)
    else if ((user_id & 0x80000000u) != 0u) {
        granted = 1u;
        confidence = 0.95;
    }
    
    return WorkResult(
        1u,                    // success
        1u,                    // stage_type = logic
        f32(granted),          // primary = access granted
        confidence             // secondary = confidence
    );
}

// =============================================================================
// Semantic Stage
// =============================================================================

// Process semantic input (e.g., cosine similarity)
fn process_semantic(item: WorkItem) -> WorkResult {
    let offset = item.input_offset;
    let dim = item.param;  // Vector dimension
    
    // Compute cosine similarity between two vectors
    // Vector A: input_data[offset .. offset + dim]
    // Vector B: input_data[offset + dim .. offset + 2*dim]
    
    var dot_product: f32 = 0.0;
    var mag_a_sq: f32 = 0.0;
    var mag_b_sq: f32 = 0.0;
    
    for (var i: u32 = 0u; i < dim; i = i + 1u) {
        let a = input_data[offset + i];
        let b = input_data[offset + dim + i];
        
        dot_product = dot_product + (a * b);
        mag_a_sq = mag_a_sq + (a * a);
        mag_b_sq = mag_b_sq + (b * b);
    }
    
    let mag_a = sqrt(mag_a_sq);
    let mag_b = sqrt(mag_b_sq);
    
    var similarity: f32 = 0.0;
    if (mag_a > 0.0 && mag_b > 0.0) {
        similarity = dot_product / (mag_a * mag_b);
    }
    
    return WorkResult(
        1u,           // success
        2u,           // stage_type = semantic
        similarity,   // primary = cosine similarity
        dot_product   // secondary = raw dot product
    );
}

// =============================================================================
// Custom Stage
// =============================================================================

// Process custom input (user-defined computation)
fn process_custom(item: WorkItem) -> WorkResult {
    let offset = item.input_offset;
    let op_code = item.param;
    
    // Custom operations based on op_code
    var result: f32 = 0.0;
    var aux: f32 = 0.0;
    
    switch (op_code) {
        // Op 0: Sum
        case 0u: {
            let count = bitcast<u32>(input_data[offset]);
            for (var i: u32 = 1u; i <= count; i = i + 1u) {
                result = result + input_data[offset + i];
            }
        }
        // Op 1: Max
        case 1u: {
            let count = bitcast<u32>(input_data[offset]);
            result = input_data[offset + 1u];
            for (var i: u32 = 2u; i <= count; i = i + 1u) {
                result = max(result, input_data[offset + i]);
            }
        }
        // Op 2: Threshold
        case 2u: {
            let value = input_data[offset];
            let threshold = input_data[offset + 1u];
            result = select(0.0, 1.0, value > threshold);
            aux = value - threshold;
        }
        default: {
            return WorkResult(0u, 3u, 0.0, 0.0);  // Unknown op
        }
    }
    
    return WorkResult(
        1u,       // success
        3u,       // stage_type = custom
        result,   // primary
        aux       // secondary
    );
}

// =============================================================================
// Main Entry Point
// =============================================================================

@compute @workgroup_size(256)
fn unified_dispatch(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    
    // Bounds check
    if (idx >= header.total_items) {
        return;
    }
    
    let item = work_items[idx];
    var result: WorkResult;
    
    // Dispatch to appropriate stage handler
    switch (item.stage_type) {
        case 0u: {
            result = process_perception(item);
        }
        case 1u: {
            result = process_logic(item);
        }
        case 2u: {
            result = process_semantic(item);
        }
        case 3u: {
            result = process_custom(item);
        }
        default: {
            result = WorkResult(0u, item.stage_type, 0.0, 0.0);
        }
    }
    
    // Write result
    output_data[item.output_offset] = result;
}
"#;

// =============================================================================
// Types
// =============================================================================

/// Pipeline configuration
#[derive(Clone, Debug)]
pub struct PipelineConfig {
    /// Maximum concurrent work items
    pub max_work_items: u32,
    /// Enable debug mode
    pub debug_mode: bool,
    /// Input buffer size in bytes
    pub input_buffer_size: u64,
    /// Output buffer size in bytes
    pub output_buffer_size: u64,
    /// Enable async readback
    pub async_readback: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_work_items: MAX_ELEMENTS_PER_DISPATCH,
            debug_mode: false,
            input_buffer_size: 64 * 1024 * 1024,  // 64 MB
            output_buffer_size: 16 * 1024 * 1024, // 16 MB
            async_readback: true,
        }
    }
}

/// Pipeline execution statistics
#[derive(Clone, Debug, Default)]
pub struct PipelineStats {
    /// Total dispatches executed
    pub total_dispatches: u64,
    /// Total work items processed
    pub total_items: u64,
    /// Items by stage type
    pub items_by_stage: [u64; 4],
    /// Average dispatch latency in microseconds
    pub avg_latency_us: f64,
    /// Peak throughput (items per second)
    pub peak_throughput: f64,
    /// GPU memory used in bytes
    pub gpu_memory_bytes: u64,
}

/// Pipeline hardware capabilities
#[derive(Clone, Debug)]
pub struct PipelineCapabilities {
    /// GPU device name
    pub device_name: String,
    /// Maximum workgroup size
    pub max_workgroup_size: u32,
    /// Maximum buffer size
    pub max_buffer_size: u64,
    /// Supports async compute
    pub async_compute: bool,
    /// Number of compute units
    pub compute_units: u32,
}

impl Default for PipelineCapabilities {
    fn default() -> Self {
        Self {
            device_name: "Unknown".to_string(),
            max_workgroup_size: 256,
            max_buffer_size: 1 << 30, // 1 GB
            async_compute: false,
            compute_units: 1,
        }
    }
}

/// Handle to a buffer in the memory pool
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferHandle(pub u64);

/// Memory pool for efficient buffer allocation
pub struct MemoryPool {
    /// Next handle ID
    next_id: AtomicU64,
    /// Buffer registry
    #[cfg(feature = "std-kernel")]
    buffers: Mutex<HashMap<u64, Arc<wgpu::Buffer>>>,
    #[cfg(not(feature = "std-kernel"))]
    buffers: Mutex<HashMap<u64, Vec<u8>>>,
    /// Total allocated bytes
    allocated_bytes: AtomicU64,
}

impl Default for MemoryPool {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryPool {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            buffers: Mutex::new(HashMap::new()),
            allocated_bytes: AtomicU64::new(0),
        }
    }

    /// Allocate a new buffer
    #[cfg(feature = "std-kernel")]
    pub fn allocate(&self, device: &wgpu::Device, size: u64, usage: wgpu::BufferUsages) -> BufferHandle {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("MemoryPool Buffer {}", id)),
            size,
            usage,
            mapped_at_creation: false,
        });
        self.buffers.lock().insert(id, Arc::new(buffer));
        self.allocated_bytes.fetch_add(size, Ordering::Relaxed);
        BufferHandle(id)
    }

    /// Get a buffer by handle
    #[cfg(feature = "std-kernel")]
    pub fn get(&self, handle: BufferHandle) -> Option<Arc<wgpu::Buffer>> {
        self.buffers.lock().get(&handle.0).cloned()
    }

    /// Free a buffer
    pub fn free(&self, handle: BufferHandle) {
        self.buffers.lock().remove(&handle.0);
    }

    /// Get total allocated bytes
    pub fn allocated_bytes(&self) -> u64 {
        self.allocated_bytes.load(Ordering::Relaxed)
    }
}

// =============================================================================
// GPU Pipeline State
// =============================================================================

#[cfg(feature = "std-kernel")]
struct GpuState {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    header_buffer: wgpu::Buffer,
    work_items_buffer: wgpu::Buffer,
    input_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
}

// =============================================================================
// Unified Pipeline
// =============================================================================

/// The Unified Perception/Logic Pipeline
///
/// Provides a single GPU compute interface for perception, logic, and semantic operations.
pub struct UnifiedPipeline {
    /// Configuration
    config: PipelineConfig,
    /// Execution statistics
    stats: RwLock<PipelineStats>,
    /// Hardware capabilities
    capabilities: RwLock<PipelineCapabilities>,
    /// Memory pool
    memory_pool: Arc<MemoryPool>,
    /// GPU state
    #[cfg(feature = "std-kernel")]
    gpu_state: Mutex<Option<GpuState>>,
    /// Initialization flag
    initialized: std::sync::atomic::AtomicBool,
}

impl Default for UnifiedPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl UnifiedPipeline {
    /// Create a new unified pipeline
    pub fn new() -> Self {
        Self::with_config(PipelineConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: PipelineConfig) -> Self {
        Self {
            config,
            stats: RwLock::new(PipelineStats::default()),
            capabilities: RwLock::new(PipelineCapabilities::default()),
            memory_pool: Arc::new(MemoryPool::new()),
            #[cfg(feature = "std-kernel")]
            gpu_state: Mutex::new(None),
            initialized: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Initialize the GPU pipeline
    #[cfg(feature = "std-kernel")]
    pub async fn initialize(&self, device: &wgpu::Device) -> Result<()> {
        use std::sync::atomic::Ordering;

        log::info!("Initializing Unified Perception/Logic Pipeline");

        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Unified Pipeline Shader"),
            source: wgpu::ShaderSource::Wgsl(UNIFIED_PIPELINE_SHADER.into()),
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Unified Pipeline Bind Group Layout"),
            entries: &[
                // Header buffer
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
                // Work items buffer
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
                // Input data buffer
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
                // Output buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
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

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Unified Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create compute pipeline
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Unified Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "unified_dispatch",
        });

        // Create buffers
        let header_size = 32u64; // 8 u32s
        let work_items_size = self.config.max_work_items as u64 * 16; // 4 u32s per item
        let output_size = self.config.max_work_items as u64 * 16; // WorkResult size

        let header_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pipeline Header Buffer"),
            size: header_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let work_items_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pipeline Work Items Buffer"),
            size: work_items_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pipeline Input Buffer"),
            size: self.config.input_buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pipeline Output Buffer"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pipeline Readback Buffer"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Store GPU state
        *self.gpu_state.lock() = Some(GpuState {
            pipeline,
            bind_group_layout,
            header_buffer,
            work_items_buffer,
            input_buffer,
            output_buffer,
            readback_buffer,
        });

        self.initialized.store(true, Ordering::Release);

        log::info!("Unified Pipeline initialized (max {} items)", self.config.max_work_items);

        Ok(())
    }

    /// Check if the pipeline is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Get pipeline statistics
    pub fn stats(&self) -> PipelineStats {
        self.stats.read().clone()
    }

    /// Get hardware capabilities
    pub fn capabilities(&self) -> PipelineCapabilities {
        self.capabilities.read().clone()
    }

    /// Get the memory pool
    pub fn memory_pool(&self) -> Arc<MemoryPool> {
        Arc::clone(&self.memory_pool)
    }

    /// Execute a batch of work items
    #[cfg(feature = "std-kernel")]
    pub async fn execute(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        work_items: &[WorkItem],
        input_data: &[f32],
    ) -> Result<Vec<WorkResult>> {
        use std::sync::atomic::Ordering;

        if !self.initialized.load(Ordering::Acquire) {
            anyhow::bail!("Pipeline not initialized");
        }

        let gpu_state = self.gpu_state.lock();
        let state = gpu_state.as_ref().context("GPU state missing")?;

        let num_items = work_items.len() as u32;
        if num_items == 0 {
            return Ok(Vec::new());
        }
        if num_items > self.config.max_work_items {
            anyhow::bail!("Too many work items: {} > {}", num_items, self.config.max_work_items);
        }

        // Count items by stage type
        let mut perception_count = 0u32;
        let mut logic_count = 0u32;
        let mut semantic_count = 0u32;
        let mut custom_count = 0u32;

        for item in work_items {
            match item.stage_type {
                0 => perception_count += 1,
                1 => logic_count += 1,
                2 => semantic_count += 1,
                3 => custom_count += 1,
                _ => {}
            }
        }

        // Prepare header
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u32)
            .unwrap_or(0);

        let header = PipelineHeader {
            total_items: num_items,
            perception_count,
            logic_count,
            semantic_count,
            custom_count,
            timestamp,
            flags: if self.config.debug_mode { 1 } else { 0 },
            _padding: 0,
        };

        // Upload data
        queue.write_buffer(&state.header_buffer, 0, bytemuck::bytes_of(&header));
        queue.write_buffer(&state.work_items_buffer, 0, bytemuck::cast_slice(work_items));
        queue.write_buffer(&state.input_buffer, 0, bytemuck::cast_slice(input_data));

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Pipeline Bind Group"),
            layout: &state.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: state.header_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: state.work_items_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: state.input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: state.output_buffer.as_entire_binding(),
                },
            ],
        });

        // Encode and submit
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Pipeline Encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Unified Pipeline Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&state.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let workgroups = (num_items + DEFAULT_WORKGROUP_SIZE - 1) / DEFAULT_WORKGROUP_SIZE;
            pass.dispatch_workgroups(workgroups, 1, 1);
        }

        // Copy output to readback buffer
        let output_bytes = (num_items as u64) * std::mem::size_of::<WorkResult>() as u64;
        encoder.copy_buffer_to_buffer(
            &state.output_buffer,
            0,
            &state.readback_buffer,
            0,
            output_bytes,
        );

        queue.submit(std::iter::once(encoder.finish()));

        // Map and read results
        let readback_slice = state.readback_buffer.slice(0..output_bytes);
        let (sender, receiver) = tokio::sync::oneshot::channel();
        readback_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device.poll(wgpu::Maintain::Wait);
        receiver.await.context("GPU readback channel closed")?.context("GPU readback failed")?;

        let data = readback_slice.get_mapped_range();
        let results: Vec<WorkResult> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        state.readback_buffer.unmap();

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_dispatches += 1;
            stats.total_items += num_items as u64;
            stats.items_by_stage[0] += perception_count as u64;
            stats.items_by_stage[1] += logic_count as u64;
            stats.items_by_stage[2] += semantic_count as u64;
            stats.items_by_stage[3] += custom_count as u64;
        }

        Ok(results)
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        *self.stats.write() = PipelineStats::default();
    }
}

// =============================================================================
// Work Item and Result Types (for GPU)
// =============================================================================

/// Work item for GPU dispatch
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "std-kernel", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct WorkItem {
    /// Stage type: 0=perception, 1=logic, 2=semantic, 3=custom
    pub stage_type: u32,
    /// Input data offset
    pub input_offset: u32,
    /// Output data offset
    pub output_offset: u32,
    /// Item-specific parameter
    pub param: u32,
}

impl WorkItem {
    /// Create a perception work item
    pub fn perception(input_offset: u32, output_offset: u32, window_size: u32) -> Self {
        Self {
            stage_type: 0,
            input_offset,
            output_offset,
            param: window_size,
        }
    }

    /// Create a logic work item
    pub fn logic(input_offset: u32, output_offset: u32) -> Self {
        Self {
            stage_type: 1,
            input_offset,
            output_offset,
            param: 0,
        }
    }

    /// Create a semantic work item
    pub fn semantic(input_offset: u32, output_offset: u32, vector_dim: u32) -> Self {
        Self {
            stage_type: 2,
            input_offset,
            output_offset,
            param: vector_dim,
        }
    }

    /// Create a custom work item
    pub fn custom(input_offset: u32, output_offset: u32, op_code: u32) -> Self {
        Self {
            stage_type: 3,
            input_offset,
            output_offset,
            param: op_code,
        }
    }
}

/// Result from GPU computation
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "std-kernel", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct WorkResult {
    /// Whether the computation succeeded
    pub success: u32,
    /// Stage type that produced this result
    pub stage_type: u32,
    /// Primary result value
    pub primary: f32,
    /// Secondary result value
    pub secondary: f32,
}

impl WorkResult {
    /// Check if the computation succeeded
    pub fn is_success(&self) -> bool {
        self.success != 0
    }

    /// Get the stage type name
    pub fn stage_name(&self) -> &'static str {
        match self.stage_type {
            0 => "perception",
            1 => "logic",
            2 => "semantic",
            3 => "custom",
            _ => "unknown",
        }
    }
}

/// Pipeline header for GPU dispatch
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "std-kernel", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct PipelineHeader {
    pub total_items: u32,
    pub perception_count: u32,
    pub logic_count: u32,
    pub semantic_count: u32,
    pub custom_count: u32,
    pub timestamp: u32,
    pub flags: u32,
    pub _padding: u32,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.max_work_items, MAX_ELEMENTS_PER_DISPATCH);
        assert!(!config.debug_mode);
        assert!(config.async_readback);
    }

    #[test]
    fn test_work_item_creation() {
        let perception = WorkItem::perception(0, 0, 64);
        assert_eq!(perception.stage_type, 0);
        assert_eq!(perception.param, 64);

        let logic = WorkItem::logic(100, 1);
        assert_eq!(logic.stage_type, 1);

        let semantic = WorkItem::semantic(200, 2, 768);
        assert_eq!(semantic.stage_type, 2);
        assert_eq!(semantic.param, 768);

        let custom = WorkItem::custom(300, 3, 2);
        assert_eq!(custom.stage_type, 3);
        assert_eq!(custom.param, 2);
    }

    #[test]
    fn test_work_result_methods() {
        let result = WorkResult {
            success: 1,
            stage_type: 2,
            primary: 0.95,
            secondary: 1.5,
        };
        assert!(result.is_success());
        assert_eq!(result.stage_name(), "semantic");

        let failed = WorkResult::default();
        assert!(!failed.is_success());
        assert_eq!(failed.stage_name(), "perception");
    }

    #[test]
    fn test_memory_pool() {
        let pool = MemoryPool::new();
        assert_eq!(pool.allocated_bytes(), 0);
    }

    #[test]
    fn test_pipeline_stats_default() {
        let stats = PipelineStats::default();
        assert_eq!(stats.total_dispatches, 0);
        assert_eq!(stats.total_items, 0);
        assert_eq!(stats.items_by_stage, [0, 0, 0, 0]);
    }

    #[test]
    fn test_pipeline_capabilities_default() {
        let caps = PipelineCapabilities::default();
        assert_eq!(caps.max_workgroup_size, 256);
        assert!(!caps.async_compute);
    }

    #[test]
    fn test_unified_pipeline_new() {
        let pipeline = UnifiedPipeline::new();
        assert!(!pipeline.is_initialized());
        assert_eq!(pipeline.stats().total_dispatches, 0);
    }

    #[test]
    fn test_unified_pipeline_with_config() {
        let config = PipelineConfig {
            max_work_items: 1024,
            debug_mode: true,
            ..Default::default()
        };
        let pipeline = UnifiedPipeline::with_config(config.clone());
        assert!(!pipeline.is_initialized());
    }

    #[test]
    fn test_pipeline_header_size() {
        assert_eq!(std::mem::size_of::<PipelineHeader>(), 32);
    }

    #[test]
    fn test_work_item_size() {
        assert_eq!(std::mem::size_of::<WorkItem>(), 16);
    }

    #[test]
    fn test_work_result_size() {
        assert_eq!(std::mem::size_of::<WorkResult>(), 16);
    }
}
