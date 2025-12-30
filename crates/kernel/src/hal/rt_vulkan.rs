//! Vulkan RT-core backend (feature-gated).
//!
//! Goal: provide a *real* runtime probe for RT capability on Vulkan drivers,
//! and a small self-test hook we can use to validate true hardware traversal.
//!
//! This module is compiled only with `--features rt-vulkan` on Linux.

use anyhow::{anyhow, Result};
use ash::vk;
use std::ffi::{CStr, CString};

/// Result of probing a Vulkan device for ray tracing capability.
#[derive(Debug, Clone)]
pub struct RtProbeInfo {
    pub device_name: String,
    pub supported: bool,
    pub reason: String,
}

/// Best-effort probe that tries to match the wgpu adapter to a Vulkan physical device.
///
/// This is intentionally conservative:
/// - If we cannot create a Vulkan instance, returns Err.
/// - If we can create Vulkan but cannot find/match a physical device, returns Ok(unsupported).
/// - If required extensions/features are present, returns Ok(supported).
pub fn probe_for_adapter(adapter: &wgpu::Adapter) -> Result<RtProbeInfo> {
    let info = adapter.get_info();

    if info.backend != wgpu::Backend::Vulkan {
        return Ok(RtProbeInfo {
            device_name: info.name,
            supported: false,
            reason: format!("wgpu backend is {:?} (not Vulkan)", info.backend),
        });
    }

    let entry = unsafe { ash::Entry::load()? };

    let app_name = CString::new("rayos-rt-probe")?;
    let engine_name = CString::new("rayos")?;

    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(0)
        .engine_name(&engine_name)
        .engine_version(0)
        // Vulkan 1.2 is a practical baseline for BDA + RT feature chaining.
        .api_version(vk::API_VERSION_1_2);

    let instance_info = vk::InstanceCreateInfo::default().application_info(&app_info);
    let instance = unsafe { entry.create_instance(&instance_info, None)? };

    let physical_devices = unsafe { instance.enumerate_physical_devices()? };
    if physical_devices.is_empty() {
        return Ok(RtProbeInfo {
            device_name: info.name,
            supported: false,
            reason: "no Vulkan physical devices".to_string(),
        });
    }

    // Try to pick a device whose name matches the wgpu adapter name.
    let mut best = physical_devices[0];
    let mut best_name = vk_device_name(&instance, best).unwrap_or_else(|| "<unknown>".to_string());

    for pd in &physical_devices {
        let name = vk_device_name(&instance, *pd).unwrap_or_else(|| "<unknown>".to_string());
        if name.contains(&info.name) || info.name.contains(&name) {
            best = *pd;
            best_name = name;
            break;
        }
    }

    let (supported, reason) = probe_physical_device_rt_support(&instance, best)?;

    unsafe {
        instance.destroy_instance(None);
    }

    Ok(RtProbeInfo {
        device_name: best_name,
        supported,
        reason,
    })
}

fn vk_device_name(instance: &ash::Instance, pd: vk::PhysicalDevice) -> Option<String> {
    let props = unsafe { instance.get_physical_device_properties(pd) };
    let bytes = props.device_name;
    let cstr = unsafe { CStr::from_ptr(bytes.as_ptr()) };
    cstr.to_str().ok().map(|s| s.to_string())
}

fn probe_physical_device_rt_support(instance: &ash::Instance, pd: vk::PhysicalDevice) -> Result<(bool, String)> {
    let extensions = unsafe { instance.enumerate_device_extension_properties(pd)? };

    let has_ext = |needle: &CStr| -> bool {
        extensions.iter().any(|e| {
            let ext_name = unsafe { CStr::from_ptr(e.extension_name.as_ptr()) };
            ext_name == needle
        })
    };

    let khr_accel = CStr::from_bytes_with_nul(b"VK_KHR_acceleration_structure\0")?;
    let khr_ray_query = CStr::from_bytes_with_nul(b"VK_KHR_ray_query\0")?;
    let khr_deferred = CStr::from_bytes_with_nul(b"VK_KHR_deferred_host_operations\0")?;

    if !has_ext(khr_accel) {
        return Ok((false, "missing VK_KHR_acceleration_structure".to_string()));
    }
    if !has_ext(khr_ray_query) {
        return Ok((false, "missing VK_KHR_ray_query".to_string()));
    }
    if !has_ext(khr_deferred) {
        return Ok((false, "missing VK_KHR_deferred_host_operations".to_string()));
    }

    // Query feature chain.
    let mut ray_query = vk::PhysicalDeviceRayQueryFeaturesKHR::default();
    let mut accel = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
    let mut bda = vk::PhysicalDeviceBufferDeviceAddressFeatures::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut bda)
        .push_next(&mut accel)
        .push_next(&mut ray_query);

    unsafe {
        instance.get_physical_device_features2(pd, &mut features2);
    }

    if ray_query.ray_query == 0 {
        return Ok((false, "rayQuery feature bit is false".to_string()));
    }
    if accel.acceleration_structure == 0 {
        return Ok((false, "accelerationStructure feature bit is false".to_string()));
    }
    if bda.buffer_device_address == 0 {
        return Ok((false, "bufferDeviceAddress feature bit is false".to_string()));
    }

    Ok((true, "VK rayQuery + accelerationStructure available".to_string()))
}

/// Optional self-test: attempt to create a Vulkan device with ray query + accel structure enabled.
///
/// This validates the "true hardware traversal" prerequisite plumbing without binding RayOS
/// logic to Vulkan by default.
pub fn self_test_create_rt_device() -> Result<()> {
    let entry = unsafe { ash::Entry::load()? };

    let app_name = CString::new("rayos-rt-selftest")?;
    let engine_name = CString::new("rayos")?;

    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(0)
        .engine_name(&engine_name)
        .engine_version(0)
        .api_version(vk::API_VERSION_1_2);

    let instance_info = vk::InstanceCreateInfo::default().application_info(&app_info);
    let instance = unsafe { entry.create_instance(&instance_info, None)? };

    let physical_devices = unsafe { instance.enumerate_physical_devices()? };
    let pd = *physical_devices
        .first()
        .ok_or_else(|| anyhow!("no Vulkan physical devices"))?;

    let (supported, reason) = probe_physical_device_rt_support(&instance, pd)?;
    if !supported {
        unsafe { instance.destroy_instance(None) };
        return Err(anyhow!("RT unsupported: {reason}"));
    }

    let queue_family_index = unsafe {
        instance
            .get_physical_device_queue_family_properties(pd)
            .iter()
            .enumerate()
            .find(|(_, q)| q.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .map(|(i, _)| i as u32)
            .ok_or_else(|| anyhow!("no compute queue family"))?
    };

    let priorities = [1.0f32];
    let queue_info = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family_index)
        .queue_priorities(&priorities)];

    let ext_names = [
        CStr::from_bytes_with_nul(b"VK_KHR_acceleration_structure\0")?,
        CStr::from_bytes_with_nul(b"VK_KHR_ray_query\0")?,
        CStr::from_bytes_with_nul(b"VK_KHR_deferred_host_operations\0")?,
    ];
    let ext_ptrs: Vec<*const i8> = ext_names.iter().map(|e| e.as_ptr()).collect();

    let mut ray_query = vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(true);
    let mut accel = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default().acceleration_structure(true);
    let mut bda = vk::PhysicalDeviceBufferDeviceAddressFeatures::default().buffer_device_address(true);
    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut bda)
        .push_next(&mut accel)
        .push_next(&mut ray_query);

    let device_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_info)
        .enabled_extension_names(&ext_ptrs)
        .push_next(&mut features2);

    let device = unsafe { instance.create_device(pd, &device_info, None)? };

    unsafe {
        device.device_wait_idle().ok();
        device.destroy_device(None);
        instance.destroy_instance(None);
    }

    Ok(())
}

/// Optional self-test: build a minimal TLAS/BLAS and run a compute shader that uses
/// `GL_EXT_ray_query` to test whether a ray hits a single triangle.
///
/// Returns `111` for hit, `222` for miss.
pub fn self_test_ray_query_branch(state_value: f32) -> Result<u32> {
    let entry = unsafe { ash::Entry::load()? };

    let app_name = CString::new("rayos-rt-rayquery")?;
    let engine_name = CString::new("rayos")?;

    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(0)
        .engine_name(&engine_name)
        .engine_version(0)
        .api_version(vk::API_VERSION_1_2);

    let instance_info = vk::InstanceCreateInfo::default().application_info(&app_info);
    let instance = unsafe { entry.create_instance(&instance_info, None)? };

    let physical_devices = unsafe { instance.enumerate_physical_devices()? };
    let pd = *physical_devices
        .first()
        .ok_or_else(|| anyhow!("no Vulkan physical devices"))?;

    let (supported, reason) = probe_physical_device_rt_support(&instance, pd)?;
    if !supported {
        unsafe { instance.destroy_instance(None) };
        return Err(anyhow!("RT unsupported: {reason}"));
    }

    let queue_family_index = unsafe {
        instance
            .get_physical_device_queue_family_properties(pd)
            .iter()
            .enumerate()
            .find(|(_, q)| q.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .map(|(i, _)| i as u32)
            .ok_or_else(|| anyhow!("no compute queue family"))?
    };

    let priorities = [1.0f32];
    let queue_info = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family_index)
        .queue_priorities(&priorities)];

    let ext_names = [
        CStr::from_bytes_with_nul(b"VK_KHR_acceleration_structure\0")?,
        CStr::from_bytes_with_nul(b"VK_KHR_ray_query\0")?,
        CStr::from_bytes_with_nul(b"VK_KHR_deferred_host_operations\0")?,
    ];
    let ext_ptrs: Vec<*const i8> = ext_names.iter().map(|e| e.as_ptr()).collect();

    let mut ray_query = vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(true);
    let mut accel = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default().acceleration_structure(true);
    let mut bda = vk::PhysicalDeviceBufferDeviceAddressFeatures::default().buffer_device_address(true);
    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut bda)
        .push_next(&mut accel)
        .push_next(&mut ray_query);

    let device_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_info)
        .enabled_extension_names(&ext_ptrs)
        .push_next(&mut features2);

    let device = unsafe { instance.create_device(pd, &device_info, None)? };
    let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

    let accel_ext = ash::khr::acceleration_structure::Device::new(&instance, &device);

    unsafe fn get_build_sizes<'a>(
        accel_ext: &ash::khr::acceleration_structure::Device,
        device: &ash::Device,
        build_type: vk::AccelerationStructureBuildTypeKHR,
        build_info: &vk::AccelerationStructureBuildGeometryInfoKHR<'a>,
        max_primitive_counts: &[u32],
    ) -> vk::AccelerationStructureBuildSizesInfoKHR<'a> {
        let mut out = vk::AccelerationStructureBuildSizesInfoKHR::default();
        (accel_ext.fp().get_acceleration_structure_build_sizes_khr)(
            device.handle(),
            build_type,
            build_info,
            max_primitive_counts.as_ptr(),
            &mut out,
        );
        out
    }

    unsafe fn create_as(
        accel_ext: &ash::khr::acceleration_structure::Device,
        device: &ash::Device,
        create_info: &vk::AccelerationStructureCreateInfoKHR,
    ) -> Result<vk::AccelerationStructureKHR> {
        let mut out = vk::AccelerationStructureKHR::null();
        (accel_ext.fp().create_acceleration_structure_khr)(
            device.handle(),
            create_info,
            std::ptr::null(),
            &mut out,
        )
        .result()
        .map_err(|e| anyhow!("vkCreateAccelerationStructureKHR failed: {e:?}"))?;
        Ok(out)
    }

    unsafe fn destroy_as(
        accel_ext: &ash::khr::acceleration_structure::Device,
        device: &ash::Device,
        as_handle: vk::AccelerationStructureKHR,
    ) {
        (accel_ext.fp().destroy_acceleration_structure_khr)(device.handle(), as_handle, std::ptr::null());
    }

    unsafe fn cmd_build_as(
        accel_ext: &ash::khr::acceleration_structure::Device,
        cmd: vk::CommandBuffer,
        infos: &[vk::AccelerationStructureBuildGeometryInfoKHR],
        ranges: &[*const vk::AccelerationStructureBuildRangeInfoKHR],
    ) {
        (accel_ext.fp().cmd_build_acceleration_structures_khr)(
            cmd,
            infos.len() as u32,
            infos.as_ptr(),
            ranges.as_ptr(),
        );
    }

    unsafe fn get_as_address(
        accel_ext: &ash::khr::acceleration_structure::Device,
        device: &ash::Device,
        as_handle: vk::AccelerationStructureKHR,
    ) -> vk::DeviceAddress {
        (accel_ext.fp().get_acceleration_structure_device_address_khr)(
            device.handle(),
            &vk::AccelerationStructureDeviceAddressInfoKHR::default().acceleration_structure(as_handle),
        )
    }

    // Minimal alloc helpers
    struct AllocBuffer {
        buffer: vk::Buffer,
        memory: vk::DeviceMemory,
        size: vk::DeviceSize,
    }

    fn find_memory_type(instance: &ash::Instance, pd: vk::PhysicalDevice, type_bits: u32, props: vk::MemoryPropertyFlags) -> Result<u32> {
        let mem = unsafe { instance.get_physical_device_memory_properties(pd) };
        for i in 0..mem.memory_type_count {
            let suitable = (type_bits & (1 << i)) != 0;
            let flags = mem.memory_types[i as usize].property_flags;
            if suitable && flags.contains(props) {
                return Ok(i);
            }
        }
        Err(anyhow!("no suitable memory type"))
    }

    unsafe fn create_buffer(
        instance: &ash::Instance,
        device: &ash::Device,
        pd: vk::PhysicalDevice,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        props: vk::MemoryPropertyFlags,
    ) -> Result<AllocBuffer> {
        let info = vk::BufferCreateInfo::default().size(size).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = device.create_buffer(&info, None)?;
        let req = device.get_buffer_memory_requirements(buffer);
        let memory_type_index = find_memory_type(instance, pd, req.memory_type_bits, props)?;
        let alloc = vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(memory_type_index);
        let memory = device.allocate_memory(&alloc, None)?;
        device.bind_buffer_memory(buffer, memory, 0)?;
        Ok(AllocBuffer { buffer, memory, size })
    }

    unsafe fn write_host_visible<T: Copy>(device: &ash::Device, mem: vk::DeviceMemory, data: &[T]) -> Result<()> {
        let bytes = std::mem::size_of_val(data);
        let ptr = device.map_memory(mem, 0, bytes as u64, vk::MemoryMapFlags::empty())?;
        std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, ptr as *mut u8, bytes);
        device.unmap_memory(mem);
        Ok(())
    }

    unsafe fn buffer_device_address(device: &ash::Device, buffer: vk::Buffer) -> vk::DeviceAddress {
        device.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(buffer))
    }

    // Vertex buffer: one triangle at x = +1 (ray +X hits, -X misses)
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Vec3 {
        x: f32,
        y: f32,
        z: f32,
    }

    let vertices = [
        Vec3 { x: 1.0, y: -0.5, z: -0.5 },
        Vec3 { x: 1.0, y: 0.5, z: -0.5 },
        Vec3 { x: 1.0, y: 0.0, z: 0.5 },
    ];

    let vertex_buf = unsafe {
        create_buffer(
            &instance,
            &device,
            pd,
            (std::mem::size_of_val(&vertices)) as u64,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?
    };
    unsafe { write_host_visible(&device, vertex_buf.memory, &vertices)? };
    let vertex_addr = unsafe { buffer_device_address(&device, vertex_buf.buffer) };

    // Build BLAS
    let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
        .vertex_format(vk::Format::R32G32B32_SFLOAT)
        .vertex_data(vk::DeviceOrHostAddressConstKHR { device_address: vertex_addr })
        .vertex_stride(std::mem::size_of::<Vec3>() as u64)
        .max_vertex(3)
        .index_type(vk::IndexType::NONE_KHR);

    let geometry = vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .geometry(vk::AccelerationStructureGeometryDataKHR { triangles })
        .flags(vk::GeometryFlagsKHR::OPAQUE);

    let range = vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(1);
    let blas_ranges_ptrs = [std::slice::from_ref(&range).as_ptr()];

    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .geometries(std::slice::from_ref(&geometry))
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD);

    let size_info = unsafe { get_build_sizes(&accel_ext, &device, vk::AccelerationStructureBuildTypeKHR::DEVICE, &build_info, &[1]) };

    let blas_buf = unsafe {
        create_buffer(
            &instance,
            &device,
            pd,
            size_info.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .or_else(|_| {
            create_buffer(
                &instance,
                &device,
                pd,
                size_info.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
        })?
    };

    let blas = unsafe {
        create_as(
            &accel_ext,
            &device,
            &vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(blas_buf.buffer)
                .size(size_info.acceleration_structure_size)
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL),
        )?
    };

    let scratch_blas = unsafe {
        create_buffer(
            &instance,
            &device,
            pd,
            size_info.build_scratch_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .or_else(|_| {
            create_buffer(
                &instance,
                &device,
                pd,
                size_info.build_scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
        })?
    };
    let scratch_blas_addr = unsafe { buffer_device_address(&device, scratch_blas.buffer) };

    // Command pool/buffer
    let cmd_pool = unsafe {
        device.create_command_pool(
            &vk::CommandPoolCreateInfo::default().queue_family_index(queue_family_index),
            None,
        )?
    };
    let cmd_buf = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(cmd_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0]
    };

    unsafe {
        device.begin_command_buffer(cmd_buf, &vk::CommandBufferBeginInfo::default())?;

        let mut build_info_blas = build_info
            .dst_acceleration_structure(blas)
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_blas_addr,
            });

        cmd_build_as(
            &accel_ext,
            cmd_buf,
            std::slice::from_ref(&build_info_blas),
            &blas_ranges_ptrs,
        );

        // Barrier so the BLAS build is visible to subsequent TLAS build.
        device.cmd_pipeline_barrier(
            cmd_buf,
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[],
        );

        device.end_command_buffer(cmd_buf)?;
    }

    let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None)? };
    unsafe {
        device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&[cmd_buf])], fence)?;
        device.wait_for_fences(&[fence], true, u64::MAX)?;
        device.reset_fences(&[fence])?;
    }

    // BLAS device address for TLAS instance
    let blas_addr = unsafe { get_as_address(&accel_ext, &device, blas) };

    // TLAS instance buffer
    let transform = vk::TransformMatrixKHR {
        matrix: [
            1.0, 0.0, 0.0, 0.0, // row 0
            0.0, 1.0, 0.0, 0.0, // row 1
            0.0, 0.0, 1.0, 0.0, // row 2
        ],
    };

    let instance_custom_index: u32 = 0;
    let mask: u8 = 0xFF;
    let sbt_offset: u32 = 0;
    let flags: u8 = vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8;

    let tlas_instance = vk::AccelerationStructureInstanceKHR {
        transform,
        instance_custom_index_and_mask: vk::Packed24_8::new(instance_custom_index, mask),
        instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(sbt_offset, flags),
        acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
            device_handle: blas_addr,
        },
    };

    let instance_buf = unsafe {
        create_buffer(
            &instance,
            &device,
            pd,
            std::mem::size_of::<vk::AccelerationStructureInstanceKHR>() as u64,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?
    };
    unsafe { write_host_visible(&device, instance_buf.memory, std::slice::from_ref(&tlas_instance))? };
    let instance_addr = unsafe { buffer_device_address(&device, instance_buf.buffer) };

    let instances_data = vk::AccelerationStructureGeometryInstancesDataKHR::default()
        .array_of_pointers(false)
        .data(vk::DeviceOrHostAddressConstKHR {
            device_address: instance_addr,
        });

    let tlas_geom = vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::INSTANCES)
        .geometry(vk::AccelerationStructureGeometryDataKHR { instances: instances_data });

    let tlas_build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .geometries(std::slice::from_ref(&tlas_geom))
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD);

    let tlas_sizes = unsafe { get_build_sizes(&accel_ext, &device, vk::AccelerationStructureBuildTypeKHR::DEVICE, &tlas_build_info, &[1]) };

    let tlas_buf = unsafe {
        create_buffer(
            &instance,
            &device,
            pd,
            tlas_sizes.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .or_else(|_| {
            create_buffer(
                &instance,
                &device,
                pd,
                tlas_sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
        })?
    };

    let tlas = unsafe {
        create_as(
            &accel_ext,
            &device,
            &vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(tlas_buf.buffer)
                .size(tlas_sizes.acceleration_structure_size)
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL),
        )?
    };

    let scratch_tlas = unsafe {
        create_buffer(
            &instance,
            &device,
            pd,
            tlas_sizes.build_scratch_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .or_else(|_| {
            create_buffer(
                &instance,
                &device,
                pd,
                tlas_sizes.build_scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
        })?
    };
    let scratch_tlas_addr = unsafe { buffer_device_address(&device, scratch_tlas.buffer) };

    // Build TLAS
    unsafe {
        device.reset_command_pool(cmd_pool, vk::CommandPoolResetFlags::empty())?;
        device.begin_command_buffer(cmd_buf, &vk::CommandBufferBeginInfo::default())?;

        let tlas_range = vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(1);
        let tlas_ranges_ptrs = [std::slice::from_ref(&tlas_range).as_ptr()];

        let mut build_info_tlas = tlas_build_info
            .dst_acceleration_structure(tlas)
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_tlas_addr,
            });

        cmd_build_as(
            &accel_ext,
            cmd_buf,
            std::slice::from_ref(&build_info_tlas),
            &tlas_ranges_ptrs,
        );

        device.cmd_pipeline_barrier(
            cmd_buf,
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[],
        );

        device.end_command_buffer(cmd_buf)?;

        device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&[cmd_buf])], fence)?;
        device.wait_for_fences(&[fence], true, u64::MAX)?;
        device.reset_fences(&[fence])?;
    }

    // Input/output buffers (host visible)
    let in_buf = unsafe {
        create_buffer(
            &instance,
            &device,
            pd,
            4,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?
    };
    let out_buf = unsafe {
        create_buffer(
            &instance,
            &device,
            pd,
            4,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?
    };
    unsafe { write_host_visible(&device, in_buf.memory, &[state_value])? };

    // Descriptor set layout
    let set_layout = unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
            ]),
            None,
        )?
    };

    let pipeline_layout = unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default().set_layouts(&[set_layout]),
            None,
        )?
    };

    // Compile compute shader (rayQuery)
    let shader_src = r#"#version 460
#extension GL_EXT_ray_query : require

layout(set = 0, binding = 0) uniform accelerationStructureEXT tlas;
layout(std430, set = 0, binding = 1) readonly buffer InBuf { float state[]; } inb;
layout(std430, set = 0, binding = 2) writeonly buffer OutBuf { uint outv[]; } outb;

layout(local_size_x = 1, local_size_y = 1, local_size_z = 1) in;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    float s = inb.state[idx];
    vec3 origin = vec3(0.0, 0.0, 0.0);
    vec3 dir = (s > 0.5) ? vec3(1.0, 0.0, 0.0) : vec3(-1.0, 0.0, 0.0);

    rayQueryEXT rq;
    rayQueryInitializeEXT(rq, tlas, gl_RayFlagsOpaqueEXT, 0xFF, origin, 0.0, dir, 100.0);
    while (rayQueryProceedEXT(rq)) { }

    bool hit = rayQueryGetIntersectionTypeEXT(rq, true) != gl_RayQueryCommittedIntersectionNoneEXT;
    outb.outv[idx] = hit ? 111u : 222u;
}
"#;

    let mut compiler = shaderc::Compiler::new().ok_or_else(|| anyhow!("shaderc compiler unavailable"))?;
    let mut options = shaderc::CompileOptions::new().ok_or_else(|| anyhow!("shaderc options unavailable"))?;
    options.set_target_env(shaderc::TargetEnv::Vulkan, shaderc::EnvVersion::Vulkan1_2 as u32);
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);

    let spirv = compiler.compile_into_spirv(
        shader_src,
        shaderc::ShaderKind::Compute,
        "rayos_rt_rayquery.comp",
        "main",
        Some(&options),
    )?;

    let shader_module = unsafe {
        device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv.as_binary()), None)?
    };

    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(shader_module)
        .name(CStr::from_bytes_with_nul(b"main\0")?);

    let pipeline = unsafe {
        match device.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(pipeline_layout)],
            None,
        ) {
            Ok(p) => p[0],
            Err((_, e)) => return Err(anyhow!("create_compute_pipelines failed: {e:?}")),
        }
    };

    // Descriptor pool + set
    let pool = unsafe {
        device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[
                    vk::DescriptorPoolSize::default()
                        .ty(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                        .descriptor_count(1),
                    vk::DescriptorPoolSize::default()
                        .ty(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(2),
                ]),
            None,
        )?
    };

    let set = unsafe {
        device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&[set_layout]))?[0]
    };

    let in_info = vk::DescriptorBufferInfo::default().buffer(in_buf.buffer).offset(0).range(4);
    let out_info = vk::DescriptorBufferInfo::default().buffer(out_buf.buffer).offset(0).range(4);

    let tlas_handles = [tlas];
    let mut as_write =
        vk::WriteDescriptorSetAccelerationStructureKHR::default().acceleration_structures(&tlas_handles);

    let mut writes = [
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .push_next(&mut as_write)
            .descriptor_count(1),
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(std::slice::from_ref(&in_info))
            .descriptor_count(1),
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(2)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(std::slice::from_ref(&out_info))
            .descriptor_count(1),
    ];

    unsafe {
        device.update_descriptor_sets(&writes, &[]);

        device.reset_command_pool(cmd_pool, vk::CommandPoolResetFlags::empty())?;
        device.begin_command_buffer(cmd_buf, &vk::CommandBufferBeginInfo::default())?;
        device.cmd_bind_pipeline(cmd_buf, vk::PipelineBindPoint::COMPUTE, pipeline);
        device.cmd_bind_descriptor_sets(cmd_buf, vk::PipelineBindPoint::COMPUTE, pipeline_layout, 0, &[set], &[]);
        device.cmd_dispatch(cmd_buf, 1, 1, 1);

        // Make shader writes visible to host.
        device.cmd_pipeline_barrier(
            cmd_buf,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[],
            &[vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)
                .buffer(out_buf.buffer)
                .offset(0)
                .size(4)],
            &[],
        );

        device.end_command_buffer(cmd_buf)?;
        device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&[cmd_buf])], fence)?;
        device.wait_for_fences(&[fence], true, u64::MAX)?;
    }

    // Read back result
    let result = unsafe {
        let ptr = device.map_memory(out_buf.memory, 0, 4, vk::MemoryMapFlags::empty())? as *const u32;
        let v = std::ptr::read_unaligned(ptr);
        device.unmap_memory(out_buf.memory);
        v
    };

    // Cleanup (best-effort)
    unsafe {
        device.device_wait_idle().ok();

        device.destroy_descriptor_pool(pool, None);
        device.destroy_pipeline(pipeline, None);
        device.destroy_shader_module(shader_module, None);
        device.destroy_pipeline_layout(pipeline_layout, None);
        device.destroy_descriptor_set_layout(set_layout, None);

        destroy_as(&accel_ext, &device, tlas);
        destroy_as(&accel_ext, &device, blas);

        device.destroy_buffer(out_buf.buffer, None);
        device.free_memory(out_buf.memory, None);
        device.destroy_buffer(in_buf.buffer, None);
        device.free_memory(in_buf.memory, None);

        device.destroy_buffer(scratch_tlas.buffer, None);
        device.free_memory(scratch_tlas.memory, None);
        device.destroy_buffer(tlas_buf.buffer, None);
        device.free_memory(tlas_buf.memory, None);
        device.destroy_buffer(instance_buf.buffer, None);
        device.free_memory(instance_buf.memory, None);

        device.destroy_buffer(scratch_blas.buffer, None);
        device.free_memory(scratch_blas.memory, None);
        device.destroy_buffer(blas_buf.buffer, None);
        device.free_memory(blas_buf.memory, None);
        device.destroy_buffer(vertex_buf.buffer, None);
        device.free_memory(vertex_buf.memory, None);

        device.destroy_fence(fence, None);
        device.free_command_buffers(cmd_pool, &[cmd_buf]);
        device.destroy_command_pool(cmd_pool, None);

        device.destroy_device(None);
        instance.destroy_instance(None);
    }

    Ok(result)
}

/// Evaluate a threshold-style branch condition using a rayQuery-backed self-test.
///
/// This is a minimal "real RT core" hook used by `LogicBVH` traversal when
/// `RAYOS_RT_CORE=1` is set.
///
/// - `Ok(true)`  => hit  (maps to the "true" branch)
/// - `Ok(false)` => miss (maps to the "false" branch)
pub fn eval_threshold_branch(state_value: f32) -> Result<bool> {
    match self_test_ray_query_branch(state_value)? {
        111 => Ok(true),
        222 => Ok(false),
        other => Err(anyhow!("unexpected rayQuery self-test code: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn vulkan_rt_rayquery_smoke_hit_miss() {
        // Run explicitly with:
        //   cargo test --features rt-vulkan -- --ignored
        //
        // This test requires a Vulkan driver with rayQuery + accelerationStructure.
        let hit = match self_test_ray_query_branch(1.0) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skipping vulkan_rt_rayquery_smoke_hit_miss: {e}");
                return;
            }
        };
        assert_eq!(hit, 111);
        let miss = self_test_ray_query_branch(0.0).expect("rayQuery miss path failed");
        assert_eq!(miss, 222);
    }
}
