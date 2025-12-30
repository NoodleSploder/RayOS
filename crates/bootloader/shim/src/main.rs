
#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::fmt::Write;
use uefi::prelude::*;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::device_path::DevicePath;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{AllocateType, MemoryDescriptor, MemoryType};

fn get_image_file_system(
    bs: &uefi::table::boot::BootServices,
    image_handle: Handle,
) -> uefi::Result<&core::cell::UnsafeCell<SimpleFileSystem>> {
    let loaded_image = bs
        .handle_protocol::<LoadedImage>(image_handle)?
        .expect("Failed to retrieve LoadedImage protocol from image handle");
    let loaded_image = unsafe { &*loaded_image.get() };

    let device_handle = loaded_image.device();

    let device_path = bs
        .handle_protocol::<DevicePath>(device_handle)?
        .expect("Failed to retrieve DevicePath protocol from device handle");
    let mut device_path = unsafe { &*device_path.get() };

    let device_handle = bs
        .locate_device_path::<SimpleFileSystem>(&mut device_path)?
        .expect("Failed to locate SimpleFileSystem protocol on device path");

    bs.handle_protocol::<SimpleFileSystem>(device_handle)
}

#[entry]
fn efi_main(handle: Handle, mut st: SystemTable<Boot>) -> Status {
    let _ = st.stdout().reset(false);
    let _ = st.stdout().write_str("RayOS shim started\n");

    let bs = st.boot_services();

    // Disable the firmware watchdog (UEFI starts a 5-min timer per loaded image).
    // Watchdog codes 0..=0xffff are reserved by firmware; use > 0xffff.
    let _ = bs.set_watchdog_timer(0, 0x10000, None);

    // Reserve the ray-OS "Spine" memory regions so the kernel can immediately
    // initialize the Zero-Copy Allocator (UMA) and Cognitive Engine partition.
    const PAGE_SIZE: u64 = 4096;
    const UMA_POOL_BYTES: u64 = 16 * 1024 * 1024 * 1024; // 16 GiB
    const COGNITIVE_POOL_BYTES: u64 = 8 * 1024 * 1024 * 1024; // 8 GiB

    let uma_pages: usize = (UMA_POOL_BYTES / PAGE_SIZE) as usize;
    let cognitive_pages: usize = (COGNITIVE_POOL_BYTES / PAGE_SIZE) as usize;

    let uma_base_address = match bs.allocate_pages(
        AllocateType::AnyPages,
        MemoryType::RESERVED,
        uma_pages,
    ) {
        Ok(c) => c.unwrap(),
        Err(e) => {
            let _ = st.stdout().write_str("Failed to allocate UMA pool\n");
            return e.status();
        }
    };

    let cognitive_engine_base = match bs.allocate_pages(
        AllocateType::AnyPages,
        MemoryType::RUNTIME_SERVICES_DATA,
        cognitive_pages,
    ) {
        Ok(c) => c.unwrap(),
        Err(e) => {
            let _ = st.stdout().write_str("Failed to allocate Cognitive Engine pool\n");
            return e.status();
        }
    };

    // Open the filesystem the image was loaded from.
    let fs: &core::cell::UnsafeCell<SimpleFileSystem> = match get_image_file_system(bs, handle) {
        Ok(c) => c.unwrap(),
        Err(e) => {
            let _ = st.stdout().write_str("Failed to get image filesystem\n");
            return e.status();
        }
    };
    let fs = unsafe { &mut *fs.get() };
    let mut root = match fs.open_volume() {
        Ok(c) => c.unwrap(),
        Err(e) => {
            let _ = st.stdout().write_str("Failed to open filesystem volume\n");
            return e.status();
        }
    };

    // Load the kernel image.
    // Expected location on the ESP: \EFI\RAYOS\kernel.bin
    let kernel_handle = match root.open(
        "EFI\\RAYOS\\kernel.bin",
        FileMode::Read,
        FileAttribute::empty(),
    ) {
        Ok(c) => c.unwrap(),
        Err(e) => {
            let _ = st.stdout().write_str("Kernel not found: EFI\\RAYOS\\kernel.bin\n");
            return e.status();
        }
    };

    let kernel_file = match kernel_handle.into_type() {
        Ok(c) => c.unwrap(),
        Err(e) => {
            let _ = st.stdout().write_str("Failed to determine kernel file type\n");
            return e.status();
        }
    };

    let mut kernel_file: RegularFile = match kernel_file {
        FileType::Regular(f) => f,
        _ => {
            let _ = st.stdout().write_str("Kernel path is not a regular file\n");
            return Status::LOAD_ERROR;
        }
    };

    // Determine file size without needing an aligned FileInfo buffer.
    if kernel_file.set_position(RegularFile::END_OF_FILE).is_err() {
        let _ = st.stdout().write_str("Failed to seek kernel\n");
        return Status::LOAD_ERROR;
    }
    let kernel_size = match kernel_file.get_position() {
        Ok(c) => c.unwrap() as usize,
        Err(_) => {
            let _ = st.stdout().write_str("Failed to read kernel size\n");
            return Status::LOAD_ERROR;
        }
    };
    let _ = kernel_file.set_position(0);

    if kernel_size == 0 {
        let _ = st.stdout().write_str("Kernel size is zero\n");
        return Status::LOAD_ERROR;
    }

    let pages = (kernel_size + 4095) / 4096;
    let kernel_addr = match bs.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_CODE, pages)
    {
        Ok(c) => c.unwrap(),
        Err(e) => {
            let _ = st.stdout().write_str("Failed to allocate kernel pages\n");
            return e.status();
        }
    };

    let kernel_buf = unsafe { core::slice::from_raw_parts_mut(kernel_addr as *mut u8, kernel_size) };
    let mut read_total = 0usize;
    while read_total < kernel_size {
        match kernel_file.read(&mut kernel_buf[read_total..]) {
            Ok(c) => {
                let n = c.unwrap();
                if n == 0 {
                    break;
                }
                read_total += n;
            }
            Err(_) => {
                let _ = st.stdout().write_str("Failed while reading kernel\n");
                return Status::LOAD_ERROR;
            }
        }
    }

    if read_total != kernel_size {
        let _ = st.stdout().write_str("Short read while loading kernel\n");
        return Status::LOAD_ERROR;
    }

    // Allocate memory map buffer. Add some slack because allocations above changed the map.
    let mmap_size = bs.memory_map_size() + 8 * core::mem::size_of::<MemoryDescriptor>();
    let mmap_storage = match bs.allocate_pool(MemoryType::LOADER_DATA, mmap_size) {
        Ok(c) => c.unwrap(),
        Err(e) => {
            let _ = st.stdout().write_str("Failed to allocate memory map buffer\n");
            return e.status();
        }
    };
    let mmap_buf = unsafe { core::slice::from_raw_parts_mut(mmap_storage, mmap_size) };

    // Exit boot services (consumes the SystemTable<Boot>).
    let (_rt, _mmap_iter) = match st.exit_boot_services(handle, mmap_buf) {
        Ok(c) => c.unwrap(),
        Err(e) => {
            // Can't reliably print much if ExitBootServices failed repeatedly.
            return e.status();
        }
    };

    // Jump to the loaded kernel.
    let entry: extern "C" fn(u64, u64) -> ! = unsafe { core::mem::transmute(kernel_addr) };
    entry(uma_base_address, cognitive_engine_base)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
