/// System 1: The Reflex Engine - "The Subconscious"
///
/// This is the persistent compute shader that runs the megakernel loop.
/// Handles millisecond-level execution, the core event loop of RayOS.

pub mod megakernel;
pub mod ray_logic;

use crate::hal::HalManager;
use crate::hal::hive::HiveManager;
use crate::task_queue::{TaskQueue, TaskCompletion};
use crate::types::{LogicRay, TaskResult, SystemMetrics, KernelConfig};
use anyhow::Result;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use tokio::sync::oneshot;

struct GpuMegakernelState {
    executor: Arc<megakernel::MegakernelExecutor>,
    task_queue_buffer: Arc<wgpu::Buffer>,
    _output_buffer: Arc<wgpu::Buffer>,
    header_readback: Arc<wgpu::Buffer>,
    bind_group: Arc<wgpu::BindGroup>,
    capacity: u32,
}

/// System 1 - The Reflex Engine
pub struct ReflexEngine {
    /// Hardware abstraction layer
    hal: Arc<HalManager>,

    /// Hive manager for multi-GPU coordination
    hive: Arc<HiveManager>,

    /// Global task queue
    task_queue: Arc<TaskQueue<LogicRay>>,

    /// Is the engine running?
    running: Arc<AtomicBool>,

    /// Total rays processed
    rays_processed: Arc<AtomicU64>,

    latency_sum_us: Arc<AtomicU64>,
    latency_count: Arc<AtomicU64>,

    gpu_iteration_budget: Arc<AtomicU32>,
    gpu_max_dispatches: Arc<AtomicU32>,

    /// Configuration
    config: KernelConfig,

    gpu_megakernel: Arc<Mutex<Option<GpuMegakernelState>>>,
}

impl ReflexEngine {
    /// Create a new Reflex Engine
    pub async fn new(config: KernelConfig) -> Result<Self> {
        log::info!("Initializing System 1: Reflex Engine");

        let hal = Arc::new(HalManager::new().await?);
        let device_count = hal.device_count();

        let hive = Arc::new(HiveManager::new(device_count));

        // Best-effort GPU megakernel initialization (fallback to CPU simulation if it fails)
        let gpu_megakernel = {
            let device = hal.primary_device();
                let mut executor = megakernel::MegakernelExecutor::new();
                let init_res = executor.initialize(device).await;
            if let Err(e) = init_res {
                log::warn!("Megakernel GPU pipeline unavailable; using CPU simulation. Reason: {e}");
                Arc::new(Mutex::new(None))
            } else {
                    let executor = Arc::new(executor);
                let layout = executor
                    .bind_group_layout()
                    .ok_or_else(|| anyhow::anyhow!("Megakernel bind group layout missing after init"))?;

                // Capacity for in-flight rays per dispatch. Keep modest to avoid large writes.
                let capacity: u32 = 4096;
                let header_size: u64 = 16;
                let ray_size: u64 = std::mem::size_of::<LogicRay>() as u64;
                let task_queue_size = header_size + (capacity as u64) * ray_size;

                let task_queue_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Megakernel TaskQueue Buffer"),
                    size: task_queue_size,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }));

                // Shader writes into output indexed by (task_id & 0xFFFF)
                let output_len: u64 = 65536;
                let output_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Megakernel Output Buffer"),
                    size: output_len * 4,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }));

                let header_readback = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Megakernel TaskQueue Header Readback"),
                    size: 8,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));

                let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Megakernel Bind Group"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: task_queue_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: output_buffer.as_entire_binding(),
                        },
                    ],
                }));

                Arc::new(Mutex::new(Some(GpuMegakernelState {
                    executor,
                    task_queue_buffer,
                    _output_buffer: output_buffer,
                    header_readback,
                    bind_group,
                    capacity,
                })))
            }
        };

        Ok(Self {
            hal,
            hive,
            task_queue: Arc::new(TaskQueue::new()),
            running: Arc::new(AtomicBool::new(false)),
            rays_processed: Arc::new(AtomicU64::new(0)),
            latency_sum_us: Arc::new(AtomicU64::new(0)),
            latency_count: Arc::new(AtomicU64::new(0)),
            gpu_iteration_budget: Arc::new(AtomicU32::new(256)),
            gpu_max_dispatches: Arc::new(AtomicU32::new(64)),
            config,
            gpu_megakernel,
        })
    }

    pub fn gpu_tuning(&self) -> (u32, u32) {
        (
            self.gpu_iteration_budget.load(Ordering::Relaxed),
            self.gpu_max_dispatches.load(Ordering::Relaxed),
        )
    }

    pub fn set_gpu_tuning(&self, iteration_budget: u32, max_dispatches: u32) {
        let iteration_budget = iteration_budget.clamp(1, 4096);
        let max_dispatches = max_dispatches.clamp(1, 512);
        self.gpu_iteration_budget
            .store(iteration_budget, Ordering::Relaxed);
        self.gpu_max_dispatches.store(max_dispatches, Ordering::Relaxed);
    }

    /// Start the megakernel loop
    pub async fn start(&self) -> Result<()> {
        if self.running.load(Ordering::Relaxed) {
            log::warn!("Reflex Engine already running");
            return Ok(());
        }

        log::info!("Starting Reflex Engine megakernel loop");
        self.running.store(true, Ordering::Relaxed);

        // Spawn the main loop
        let engine = self.clone_arc();
        tokio::spawn(async move {
            if let Err(e) = engine.megakernel_loop().await {
                log::error!("Megakernel loop error: {}", e);
            }
        });

        Ok(())
    }

    /// Stop the megakernel loop
    pub fn stop(&self) {
        log::info!("Stopping Reflex Engine");
        self.running.store(false, Ordering::Relaxed);
    }

    /// Submit a task (ray) for execution
    pub fn submit_ray(&self, ray: LogicRay) -> Result<()> {
        if self.task_queue.pending_len() >= self.config.max_queue_size {
            anyhow::bail!("Task queue full! Backpressure applied.");
        }

        self.task_queue.submit(ray);
        Ok(())
    }

    /// Submit multiple rays in batch
    pub fn submit_ray_batch(&self, rays: Vec<LogicRay>) -> Result<()> {
        if self.task_queue.pending_len() + rays.len() >= self.config.max_queue_size {
            anyhow::bail!("Task queue would overflow!");
        }

        for ray in rays {
            self.task_queue.submit(ray);
        }

        Ok(())
    }

    /// Drain completed task results (best-effort, non-blocking).
    pub fn drain_completions(&self, max: usize) -> Vec<TaskCompletion> {
        let mut out = Vec::new();
        for _ in 0..max {
            if let Some(c) = self.task_queue.try_pop_completion() {
                out.push(c);
            } else {
                break;
            }
        }
        out
    }

    /// The infinite megakernel loop (while true)
    async fn megakernel_loop(&self) -> Result<()> {
        log::info!("Entering megakernel loop (infinite execution)");

        let mut frame_count = 0u64;
        let start_time = std::time::Instant::now();

        while self.running.load(Ordering::Relaxed) {
            let frame_start = std::time::Instant::now();

            // Process rays from global queue
            let mut rays_this_frame = 0;

            // Dequeue and distribute to workers
            while let Some(ray) = self.task_queue.pop_for_dispatch() {
                if let Err(e) = self.hive.submit_task(ray) {
                    log::error!("Failed to submit task to hive: {}", e);
                    break;
                }
                rays_this_frame += 1;

                // Limit per frame to maintain real-time performance
                if rays_this_frame >= 10000 {
                    break;
                }
            }

            // Execute rays on workers (simulate for now)
            self.execute_worker_tasks().await?;

            // Maintain frame rate
            let frame_time = frame_start.elapsed();
            let target_frame_time = std::time::Duration::from_micros(
                self.config.target_frame_time_us
            );

            if frame_time < target_frame_time {
                tokio::time::sleep(target_frame_time - frame_time).await;
            }

            frame_count += 1;

            // Log stats every second
            if frame_count % 60 == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rays_per_sec = self.rays_processed.load(Ordering::Relaxed) as f64 / elapsed;

                log::debug!(
                    "Frame {}: {:.0} rays/sec, Queue depth: {}",
                    frame_count,
                    rays_per_sec,
                    self.task_queue.pending_len()
                );
            }

            // Load balancing
            self.hive.balance_load();
        }

        log::info!("Megakernel loop terminated");
        Ok(())
    }

    /// Execute tasks from worker queues
    async fn execute_worker_tasks(&self) -> Result<()> {
        // Prefer actual GPU dispatch when available; otherwise fall back to CPU simulation.
        let gpu = {
            let guard = self.gpu_megakernel.lock();
            guard.as_ref().map(|state| {
                (
                    Arc::clone(&state.executor),
                    Arc::clone(&state.task_queue_buffer),
                    Arc::clone(&state.header_readback),
                    Arc::clone(&state.bind_group),
                    state.capacity,
                )
            })
        };

        if let Some((executor, task_queue_buffer, header_readback, bind_group, queue_capacity)) = gpu {
            let mut batch: Vec<LogicRay> = Vec::with_capacity(queue_capacity as usize);

            // Drain tasks from workers into a single dispatch batch.
            let stats = self.hive.aggregate_stats();
            for (worker_id, worker_stats) in stats.iter().enumerate() {
                if worker_stats.queue_depth == 0 {
                    continue;
                }

                if let Some(worker) = self.hive.worker(worker_id) {
                    while batch.len() < queue_capacity as usize {
                        if let Some(ray) = worker.pop_task() {
                            batch.push(ray);
                        } else {
                            break;
                        }
                    }
                }

                if batch.len() >= queue_capacity as usize {
                    break;
                }
            }

            if batch.is_empty() {
                return Ok(());
            }

            // Layout: [head u32][tail u32][capacity u32][pad u32][rays...]
            let header_size = 16usize;
            let ray_size = std::mem::size_of::<LogicRay>();
            let total_size = header_size + (queue_capacity as usize) * ray_size;
            let mut bytes = vec![0u8; total_size];

            let head: u32 = 0;
            let tail: u32 = batch.len() as u32;
            let capacity: u32 = queue_capacity;
            // Watchdog-safe chunking: cap per-dispatch iterations and rely on re-dispatch.
            // Since `tail` is fixed to this batch, the shader will also exit once the queue empties.
            let iteration_budget: u32 = self.gpu_iteration_budget.load(Ordering::Relaxed).clamp(1, 4096);

            bytes[0..4].copy_from_slice(&head.to_le_bytes());
            bytes[4..8].copy_from_slice(&tail.to_le_bytes());
            bytes[8..12].copy_from_slice(&capacity.to_le_bytes());
            bytes[12..16].copy_from_slice(&iteration_budget.to_le_bytes());

            let rays_bytes = bytemuck::cast_slice(&batch);
            let rays_dst_end = header_size + rays_bytes.len();
            bytes[header_size..rays_dst_end].copy_from_slice(rays_bytes);

            let device = self.hal.primary_device();
            let queue = self.hal.primary_queue();
            queue.write_buffer(&*task_queue_buffer, 0, &bytes);

            let workgroup_count = ((batch.len() as u32) + 255) / 256;
            // Dispatch/resubmit until the shader reports head == tail (bounded).
            // This keeps watchdog-safe chunking while ensuring real completion signals.
            let mut completed_until: u32 = 0;
            let tail = batch.len() as u32;
            let max_dispatches: u32 = self.gpu_max_dispatches.load(Ordering::Relaxed).clamp(1, 512);

            for _ in 0..max_dispatches {
                executor.dispatch(device, queue, &*bind_group, workgroup_count.max(1));

                // Ensure the submitted work is finished before interpreting `head` as completed.
                // `head` increments when a task is *claimed*; once the GPU work is done, all
                // claimed tasks have completed execution.
                wait_for_queue_idle(device, queue).await?;

                let (head, observed_tail) =
                    read_queue_head_tail(device, queue, &*task_queue_buffer, &*header_readback)
                        .await?;

                // tail should remain stable; treat mismatches defensively.
                let effective_tail = observed_tail.min(tail);
                let effective_head = head.min(effective_tail);

                if effective_head > completed_until {
                    for ray in batch
                        .iter()
                        .take(effective_head as usize)
                        .skip(completed_until as usize)
                    {
                        let latency_us = self.task_queue.complete(ray.task_id, TaskResult::Success);
                        self.latency_sum_us.fetch_add(latency_us, Ordering::Relaxed);
                        self.latency_count.fetch_add(1, Ordering::Relaxed);
                        self.rays_processed.fetch_add(1, Ordering::Relaxed);
                    }
                    completed_until = effective_head;
                }

                if completed_until >= effective_tail {
                    break;
                }
            }

            // If some tasks were not completed within our bounded dispatch loop, requeue them.
            if completed_until < tail {
                for ray in batch.iter().skip(completed_until as usize) {
                    self.task_queue.requeue(*ray);
                }
            }
            return Ok(());
        }

        // CPU simulation fallback: for each worker, pop and execute tasks
        let stats = self.hive.aggregate_stats();
        for (worker_id, worker_stats) in stats.iter().enumerate() {
            if worker_stats.queue_depth == 0 {
                continue;
            }

            if let Some(worker) = self.hive.worker(worker_id) {
                if let Some(ray) = worker.pop_task() {
                    log::debug!("Executing ray {} on worker {}", ray.task_id, worker_id);
                    let result = self.execute_ray(ray).await?;
                    // TaskQueue computes end-to-end latency from submit() timestamps.
                    let latency_us = self.task_queue.complete(ray.task_id, result);
                    self.latency_sum_us.fetch_add(latency_us, Ordering::Relaxed);
                    self.latency_count.fetch_add(1, Ordering::Relaxed);
                    self.rays_processed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        Ok(())
    }

    /// Execute a single ray using RT Core logic tree traversal
    async fn execute_ray(&self, ray: LogicRay) -> Result<TaskResult> {
        // Implement RT Core-style BVH traversal simulation
        // In real hardware, this would be done by GPU RT cores

        let start = std::time::Instant::now();

        // Simulate BVH tree traversal
        // Each ray traverses a logic tree structure
        let tree_depth = (ray.logic_tree_id as usize % 10) + 5;  // 5-15 levels deep
        let mut nodes_visited = 0;

        for depth in 0..tree_depth {
            // Simulate AABB intersection test (what RT cores do)
            let intersects = (ray.task_id + depth as u64) % 3 != 0;  // 2/3 hit rate

            if intersects {
                nodes_visited += 1;
                // Simulate work at this node
                tokio::time::sleep(std::time::Duration::from_micros(1)).await;
            } else {
                // Early exit (ray missed)
                break;
            }
        }

        // Track latency (for logs only; metrics use TaskQueue end-to-end latency)
        let elapsed = start.elapsed().as_micros() as u64;
        log::trace!("Ray {} traversed {} nodes in {} μs", ray.task_id, nodes_visited, elapsed);

        Ok(TaskResult::Success)
    }

    /// Get current system metrics
    pub fn metrics(&self) -> SystemMetrics {
        let queue_depth = self.hive.total_queue_depth();
        let active_rays = self.task_queue.pending_len() + queue_depth;
        let rays_processed = self.rays_processed.load(Ordering::Relaxed);

        // Calculate entropy based on queue depth and activity
        // Higher queue depth = higher entropy (more disorder/unpredictability)
        let entropy = if rays_processed > 0 {
            (active_rays as f32 / (rays_processed as f32 + 1.0)).min(1.0)
        } else {
            0.0
        };

        // Real average latency (from TaskQueue end-to-end completion timestamps).
        let count = self.latency_count.load(Ordering::Relaxed);
        let avg_latency_us = if count == 0 {
            0
        } else {
            self.latency_sum_us.load(Ordering::Relaxed) / count
        };

        // User presence would be connected to Cortex gaze/vision system
        // For now, assume present if we have active rays
        let user_present = active_rays > 0 || queue_depth > 0;

        SystemMetrics {
            active_rays,
            queue_depth,
            user_present,
            entropy,
            avg_latency_us,
        }
    }

    /// Helper to clone Arc references for async tasks
    fn clone_arc(&self) -> Self {
        Self {
            hal: Arc::clone(&self.hal),
            hive: Arc::clone(&self.hive),
            task_queue: Arc::clone(&self.task_queue),
            running: Arc::clone(&self.running),
            rays_processed: Arc::clone(&self.rays_processed),
            latency_sum_us: Arc::clone(&self.latency_sum_us),
            latency_count: Arc::clone(&self.latency_count),
            gpu_iteration_budget: Arc::clone(&self.gpu_iteration_budget),
            gpu_max_dispatches: Arc::clone(&self.gpu_max_dispatches),
            config: self.config.clone(),
            gpu_megakernel: Arc::clone(&self.gpu_megakernel),
        }
    }
}

async fn wait_for_queue_idle(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    queue.on_submitted_work_done(move || {
        let _ = tx.send(());
    });
    device.poll(wgpu::Maintain::Wait);
    rx.await.map_err(|_| anyhow::anyhow!("queue completion canceled"))?;
    Ok(())
}

async fn read_queue_head_tail(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src_task_queue: &wgpu::Buffer,
    dst_readback: &wgpu::Buffer,
) -> Result<(u32, u32)> {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Megakernel Header Readback Encoder"),
    });

    encoder.copy_buffer_to_buffer(src_task_queue, 0, dst_readback, 0, 8);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = dst_readback.slice(..8);
    let (tx, rx) = oneshot::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });

    device.poll(wgpu::Maintain::Wait);
    rx.await
        .map_err(|_| anyhow::anyhow!("readback canceled"))?
        .map_err(|e| anyhow::anyhow!("readback map failed: {e:?}"))?;

    let data = slice.get_mapped_range();
    let head = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let tail = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    drop(data);
    dst_readback.unmap();

    Ok((head, tail))
}
