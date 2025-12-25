/// Hardware Abstraction Layer - "The Spine"
///
/// Provides unified access to heterogeneous hardware (APU + dGPUs)

pub mod allocator;
pub mod hive;

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

/// HAL Manager - coordinates all hardware resources
pub struct HalManager {
    instance: Instance,
    devices: Vec<(Device, Queue)>,
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

        let mut devices = Vec::new();

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

            devices.push((device, queue));
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
        &self.devices[self.primary_device_idx].0
    }

    /// Get the primary queue
    pub fn primary_queue(&self) -> &Queue {
        &self.devices[self.primary_device_idx].1
    }

    /// Get all devices for hive operations
    pub fn all_devices(&self) -> &[(Device, Queue)] {
        &self.devices
    }

    /// Number of available compute devices
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }
}
