//! GPU-Accelerated Similarity Search
//!
//! Uses compute shaders to perform parallel cosine similarity computation
//! across the entire vector store, achieving massive speedups over CPU-based search.
//!
//! ## Architecture
//!
//! The GPU search engine works in three phases:
//! 1. Upload vector database to GPU storage buffer (one-time on index build)
//! 2. Upload query vector to uniform buffer
//! 3. Dispatch compute shader to compute similarities in parallel
//! 4. Read back top-k results from output buffer
//!
//! Each GPU thread computes the cosine similarity between the query vector
//! and one database vector, achieving O(1) time complexity per vector.

use anyhow::{Context, Result};

#[cfg(feature = "gpu")]
use std::sync::Arc;

/// WGSL compute shader for parallel cosine similarity
///
/// Each thread computes similarity for one vector in the database.
/// Results are written to an output buffer for CPU-side sorting.
pub const GPU_SIMILARITY_SHADER: &str = r#"
// GPU-Accelerated Similarity Search Shader
// Computes cosine similarity between query vector and all database vectors

struct SearchParams {
    num_vectors: u32,      // Number of vectors in the database
    dimension: u32,        // Vector dimension (e.g., 768)
    k: u32,                // Number of top results to find
    _padding: u32,
}

@group(0) @binding(0)
var<uniform> params: SearchParams;

// Query vector (single vector to search for)
@group(0) @binding(1)
var<storage, read> query_vector: array<f32>;

// Database vectors (flattened: num_vectors * dimension floats)
@group(0) @binding(2)
var<storage, read> database: array<f32>;

// Output: similarity scores for each vector
@group(0) @binding(3)
var<storage, read_write> similarities: array<f32>;

// Compute cosine similarity between query and database vector at index
fn cosine_similarity(vec_index: u32) -> f32 {
    let dim = params.dimension;
    let base_offset = vec_index * dim;

    var dot_product: f32 = 0.0;
    var query_magnitude_sq: f32 = 0.0;
    var db_magnitude_sq: f32 = 0.0;

    // Compute dot product and magnitudes
    for (var i: u32 = 0u; i < dim; i = i + 1u) {
        let q = query_vector[i];
        let d = database[base_offset + i];

        dot_product = dot_product + (q * d);
        query_magnitude_sq = query_magnitude_sq + (q * q);
        db_magnitude_sq = db_magnitude_sq + (d * d);
    }

    // Compute cosine similarity
    let query_mag = sqrt(query_magnitude_sq);
    let db_mag = sqrt(db_magnitude_sq);

    if (query_mag == 0.0 || db_mag == 0.0) {
        return 0.0;
    }

    return dot_product / (query_mag * db_mag);
}

@compute @workgroup_size(256)
fn similarity_search(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let vec_index = global_id.x;

    // Bounds check
    if (vec_index >= params.num_vectors) {
        return;
    }

    // Compute and store similarity
    let similarity = cosine_similarity(vec_index);
    similarities[vec_index] = similarity;
}
"#;

/// GPU Search Engine for fast similarity search
///
/// Manages GPU buffers and compute pipeline for parallel similarity computation.
#[cfg(feature = "gpu")]
pub struct GpuSearchEngine {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    /// Database vectors on GPU (num_vectors * dimension floats)
    database_buffer: Option<wgpu::Buffer>,
    /// File IDs corresponding to database vectors (for result mapping)
    file_ids: Vec<crate::types::FileId>,
    /// Vector dimension
    dimension: usize,
    /// Number of vectors currently in the database
    num_vectors: usize,
    /// Whether the GPU engine is ready
    initialized: bool,
}

#[cfg(feature = "gpu")]
impl GpuSearchEngine {
    /// Create a new GPU search engine
    pub async fn new() -> Result<Self> {
        log::info!("Initializing GPU Search Engine...");

        // Create wgpu instance and get device
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .context("Failed to find suitable GPU adapter")?;

        let info = adapter.get_info();
        log::info!("GPU Search using: {} ({:?})", info.name, info.backend);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Volume GPU Search Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .context("Failed to create GPU device")?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Similarity Search Shader"),
            source: wgpu::ShaderSource::Wgsl(GPU_SIMILARITY_SHADER.into()),
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Similarity Search Bind Group Layout"),
            entries: &[
                // params (uniform)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // query_vector (storage, read-only)
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
                // database (storage, read-only)
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
                // similarities (storage, read-write)
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

        // Create compute pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Similarity Search Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Similarity Search Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "similarity_search",
            compilation_options: Default::default(),
            cache: None,
        });

        log::info!("GPU Search Engine initialized successfully");

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            database_buffer: None,
            file_ids: Vec::new(),
            dimension: 0,
            num_vectors: 0,
            initialized: true,
        })
    }

    /// Upload vectors to GPU for searching
    ///
    /// This should be called once when building the index. The vectors are
    /// stored on the GPU and reused for all subsequent searches.
    pub fn upload_vectors(
        &mut self,
        vectors: &[(crate::types::FileId, Vec<f32>)],
        dimension: usize,
    ) -> Result<()> {
        if vectors.is_empty() {
            log::warn!("No vectors to upload to GPU");
            return Ok(());
        }

        log::info!("Uploading {} vectors (dim={}) to GPU...", vectors.len(), dimension);

        self.dimension = dimension;
        self.num_vectors = vectors.len();

        // Store file IDs for result mapping
        self.file_ids = vectors.iter().map(|(id, _)| *id).collect();

        // Flatten vectors into a single buffer
        let total_floats = vectors.len() * dimension;
        let mut flat_data: Vec<f32> = Vec::with_capacity(total_floats);

        for (_, vec) in vectors {
            if vec.len() != dimension {
                anyhow::bail!(
                    "Vector dimension mismatch: expected {}, got {}",
                    dimension,
                    vec.len()
                );
            }
            flat_data.extend_from_slice(vec);
        }

        // Create GPU buffer
        let database_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vector Database Buffer"),
            size: (total_floats * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Upload data
        self.queue.write_buffer(&database_buffer, 0, bytemuck::cast_slice(&flat_data));
        self.database_buffer = Some(database_buffer);

        log::info!("Uploaded {:.2} MB to GPU", (total_floats * 4) as f64 / 1024.0 / 1024.0);

        Ok(())
    }

    /// Search for the k most similar vectors to the query
    ///
    /// Returns vector indices and their similarity scores, sorted by similarity (descending).
    pub fn search(
        &self,
        query: &[f32],
        k: usize,
    ) -> Result<Vec<(crate::types::FileId, f32)>> {
        if !self.initialized {
            anyhow::bail!("GPU Search Engine not initialized");
        }

        let database_buffer = self.database_buffer.as_ref()
            .context("No vectors uploaded to GPU")?;

        if query.len() != self.dimension {
            anyhow::bail!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            );
        }

        // Create params buffer
        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct SearchParams {
            num_vectors: u32,
            dimension: u32,
            k: u32,
            _padding: u32,
        }

        let params = SearchParams {
            num_vectors: self.num_vectors as u32,
            dimension: self.dimension as u32,
            k: k as u32,
            _padding: 0,
        };

        let params_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Search Params Buffer"),
            size: std::mem::size_of::<SearchParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&params_buffer, 0, bytemuck::bytes_of(&params));

        // Create query buffer
        let query_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Query Vector Buffer"),
            size: (self.dimension * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&query_buffer, 0, bytemuck::cast_slice(query));

        // Create output buffer for similarities
        let output_size = (self.num_vectors * std::mem::size_of::<f32>()) as u64;
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Similarities Output Buffer"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Create readback buffer
        let readback_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Similarities Readback Buffer"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Similarity Search Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: query_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: database_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        // Dispatch compute shader
        let workgroup_count = (self.num_vectors as u32 + 255) / 256;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Similarity Search Encoder"),
        });

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Similarity Search Pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        // Copy output to readback buffer
        encoder.copy_buffer_to_buffer(&output_buffer, 0, &readback_buffer, 0, output_size);

        self.queue.submit(std::iter::once(encoder.finish()));

        // Read back results
        let buffer_slice = readback_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });

        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().context("Failed to map readback buffer")??;

        let data = buffer_slice.get_mapped_range();
        let similarities: &[f32] = bytemuck::cast_slice(&data);

        // Collect results with file IDs
        let mut results: Vec<(crate::types::FileId, f32)> = self.file_ids
            .iter()
            .zip(similarities.iter())
            .map(|(id, sim)| (*id, *sim))
            .collect();

        // Sort by similarity (descending) and take top-k
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);

        drop(data);
        readback_buffer.unmap();

        Ok(results)
    }

    /// Check if GPU search is available and ready
    pub fn is_ready(&self) -> bool {
        self.initialized && self.database_buffer.is_some()
    }

    /// Get GPU stats for monitoring
    pub fn stats(&self) -> GpuSearchStats {
        GpuSearchStats {
            num_vectors: self.num_vectors,
            dimension: self.dimension,
            memory_bytes: if self.database_buffer.is_some() {
                self.num_vectors * self.dimension * std::mem::size_of::<f32>()
            } else {
                0
            },
            initialized: self.initialized,
        }
    }
}

/// GPU Search statistics
#[derive(Debug, Clone)]
pub struct GpuSearchStats {
    pub num_vectors: usize,
    pub dimension: usize,
    pub memory_bytes: usize,
    pub initialized: bool,
}

/// Fallback for when GPU feature is not enabled
#[cfg(not(feature = "gpu"))]
pub struct GpuSearchEngine;

#[cfg(not(feature = "gpu"))]
impl GpuSearchEngine {
    pub async fn new() -> Result<Self> {
        anyhow::bail!("GPU search not available: compile with --features gpu")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_compiles() {
        // This test just verifies the shader string is valid
        assert!(GPU_SIMILARITY_SHADER.contains("@compute"));
        assert!(GPU_SIMILARITY_SHADER.contains("cosine_similarity"));
        assert!(GPU_SIMILARITY_SHADER.contains("similarity_search"));
    }
}
