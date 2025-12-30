use wgpu::{Adapter, Device};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RayTracingSupport {
    Supported,
    Unsupported,
    Unknown,
}

/// Conservative probe for ray tracing support.
///
/// wgpu 0.19 does not expose cross-backend, stable RT-core capability queries,
/// so we keep this minimal and safe: we never claim support unless we can
/// prove it with stable API.
pub fn detect_ray_tracing_support(_adapter: &Adapter, _device: &Device) -> RayTracingSupport {
    #[cfg(all(feature = "rt-vulkan", target_os = "linux"))]
    {
        // If we're running on a Vulkan-capable system, do an actual extension/feature probe.
        // This is intentionally best-effort and never panics.
        if let Ok(info) = crate::hal::rt_vulkan::probe_for_adapter(_adapter) {
            return if info.supported {
                RayTracingSupport::Supported
            } else {
                RayTracingSupport::Unsupported
            };
        }
    }

    // wgpu 0.19 does not expose stable, cross-backend RT capability queries.
    RayTracingSupport::Unknown
}
