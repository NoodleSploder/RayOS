/// Hardware Abstraction Layer - "The Spine"
///
/// Provides unified access to heterogeneous hardware (APU + dGPUs)

pub mod allocator;
pub mod hive;
pub mod rt_core;

#[cfg(all(feature = "rt-vulkan", target_os = "linux"))]
pub mod rt_vulkan;

use anyhow::Result;
use wgpu::{Device, Queue, Instance};

/// Hardware capabilities detection
#[derive(Debug, Clone)]
pub struct HardwareInfo {
    pub device_name: String,
    pub backend: String,
    pub supports_ray_tracing: bool,
    pub unified_memory: bool,
    pub vram_size: u64,
}

struct GpuDevice {
    device: Device,
    queue: Queue,
    info: HardwareInfo,
}

/// HAL Manager - coordinates all hardware resources
pub struct HalManager {
    instance: Instance,
    devices: Vec<GpuDevice>,
    primary_device_idx: usize,
}

impl HalManager {
    /// Initialize the HAL with all available GPUs
    pub async fn new() -> Result<Self> {
        log::info!("Initializing Hardware Abstraction Layer...");

        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let mut devices: Vec<GpuDevice> = Vec::new();

        // Enumerate all adapters
        for adapter in instance.enumerate_adapters(wgpu::Backends::all()) {
            let info = adapter.get_info();
            log::info!(
                "Found GPU: {} ({:?})",
                info.name,
                info.backend
            );
            // Request device
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some(&format!("Device: {}", info.name)),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                    },
                    None,
                )
                .await?;

            let rt = rt_core::detect_ray_tracing_support(&adapter, &device);
            let (supports_ray_tracing, rt_reason) = match rt {
                rt_core::RayTracingSupport::Supported => (true, "supported"),
                rt_core::RayTracingSupport::Unsupported => (false, "unsupported"),
                rt_core::RayTracingSupport::Unknown => (false, "unknown"),
            };

            if supports_ray_tracing {
                log::info!("  RT cores: available ({rt_reason})");
            } else {
                log::info!("  RT cores: unavailable ({rt_reason})");
            }

            let unified_memory = matches!(info.device_type, wgpu::DeviceType::IntegratedGpu);
            devices.push(GpuDevice {
                device,
                queue,
                info: HardwareInfo {
                    device_name: info.name,
                    backend: format!("{:?}", info.backend),
                    supports_ray_tracing,
                    unified_memory,
                    vram_size: 0,
                },
            });
        }

        if devices.is_empty() {
            anyhow::bail!("No compatible GPU devices found!");
        }

        log::info!("HAL initialized with {} device(s)", devices.len());

        Ok(Self {
            instance,
            devices,
            primary_device_idx: 0,
        })
    }

    /// Get the primary compute device (APU or first GPU)
    pub fn primary_device(&self) -> &Device {
        &self.devices[self.primary_device_idx].device
    }

    /// Get the primary queue
    pub fn primary_queue(&self) -> &Queue {
        &self.devices[self.primary_device_idx].queue
    }

    /// Get all devices for hive operations
    pub fn all_devices(&self) -> Vec<(&Device, &Queue)> {
        self.devices
            .iter()
            .map(|d| (&d.device, &d.queue))
            .collect()
    }

    pub fn primary_hardware_info(&self) -> &HardwareInfo {
        &self.devices[self.primary_device_idx].info
    }

    /// Number of available compute devices
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }
}
