#![no_std]
#![no_main]

use core::fmt::Write;

use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::device_path::DevicePath;
use uefi::Identify;
use uefi::unsafe_guid;

use uefi::table::boot::{MemoryDescriptor, MemoryType};
use uefi::table::cfg::{ACPI2_GUID, ACPI_GUID};

use uefi::table::runtime::Time;

#[repr(C)]
struct BootMemoryDescriptor {
    ty: u32,
    _padding: u32,
    phys_start: u64,
    virt_start: u64,
    page_count: u64,
    att: u64,
}

#[repr(C)]
struct BootInfo {
    magic: u64,

    fb_base: u64,
    fb_width: u32,
    fb_height: u32,
    fb_stride: u32,
    _fb_reserved: u32,

    rsdp_addr: u64,

    memory_map_ptr: u64,
    memory_map_size: u64,
    memory_desc_size: u64,
    memory_desc_version: u32,
    _mmap_reserved: u32,

    // Optional local LLM model blob (physical address + size in bytes).
    // 0/0 means "no model present".
    model_ptr: u64,
    model_size: u64,

    // Optional Volume backing blob (physical address + size in bytes).
    // 0/0 means "no volume present".
    volume_ptr: u64,
    volume_size: u64,

    // Optional embeddings blob staged from the boot filesystem.
    // 0/0 means "not present".
    embeddings_ptr: u64,
    embeddings_size: u64,

    // Optional index blob staged from the boot filesystem.
    // 0/0 means "not present".
    index_ptr: u64,
    index_size: u64,

    // Best-effort UTC wall-clock time captured from UEFI before ExitBootServices.
    // If unavailable, boot_time_valid=0 and boot_unix_seconds=0.
    boot_unix_seconds: u64,
    boot_time_valid: u32,
    _time_reserved: u32,
}

#[cfg(target_arch = "aarch64")]
static mut AUTORUN_PROMPT_PTR: u64 = 0;

#[cfg(target_arch = "aarch64")]
static mut AUTORUN_PROMPT_SIZE: u64 = 0;

#[cfg(target_arch = "aarch64")]
static mut VOLUME_BLOB_PTR: u64 = 0;

#[cfg(target_arch = "aarch64")]
static mut VOLUME_BLOB_SIZE: u64 = 0;

fn days_from_civil_utc(year: i32, month: u32, day: u32) -> Option<i64> {
    if month < 1 || month > 12 {
        return None;
    }

    fn is_leap(y: i32) -> bool {
        (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
    }

    fn dim(y: i32, m: u32) -> u32 {
        match m {
            1 => 31,
            2 => if is_leap(y) { 29 } else { 28 },
            3 => 31,
            4 => 30,
            5 => 31,
            6 => 30,
            7 => 31,
            8 => 31,
            9 => 30,
            10 => 31,
            11 => 30,
            _ => 31,
        }
    }

    let max_day = dim(year, month);
    if day < 1 || day > max_day {
        return None;
    }

    // Howard Hinnant's civil_from_days / days_from_civil algorithm.
    // Returns days since 1970-01-01.
    let mut y = year;
    let m = month as i32;
    let d = day as i32;
    y -= if m <= 2 { 1 } else { 0 };

    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;

    Some((era as i64) * 146_097 + (doe as i64) - 719_468)
}

fn uefi_time_to_unix_seconds_utc(t: &Time) -> Option<u64> {
    let year = t.year() as i32;
    // Keep a conservative range to avoid overflow if firmware returns garbage.
    if year < 1970 || year > 2500 {
        return None;
    }

    let month = t.month() as u32;
    let day = t.day() as u32;
    let days = days_from_civil_utc(year, month, day)?;

    let hour = t.hour() as i64;
    let minute = t.minute() as i64;
    let second = t.second() as i64;
    if hour < 0 || hour > 23 || minute < 0 || minute > 59 || second < 0 || second > 59 {
        return None;
    }

    let mut secs = days
        .checked_mul(86_400)?
        .checked_add(hour * 3_600)?
        .checked_add(minute * 60)?
        .checked_add(second)?;

    // UEFI time may include a timezone offset from UTC in minutes.
    // If specified, convert local time -> UTC.
    if let Some(tz) = t.time_zone() {
        let tz = tz as i32;
        // Valid range is -1440..=1440 minutes, per UEFI spec.
        if tz < -1440 || tz > 1440 {
            return None;
        }
        secs = secs.checked_sub((tz as i64) * 60)?;
    }

    u64::try_from(secs).ok()
}

const BOOTINFO_MAGIC: u64 = 0x5241_594F_535F_4249; // "RAYOS_BI"

// Kernel entry point signature - takes BootInfo PHYSICAL address (u64).
//
// IMPORTANT (x86_64): The bare-metal kernel is built for `x86_64-unknown-none`
// (SysV ABI). A UEFI application uses the MS ABI for its own entrypoints, so
// when calling into the bare-metal kernel we must use SysV explicitly.
#[cfg(target_arch = "x86_64")]
type KernelEntryPoint = extern "sysv64" fn(u64) -> !;

// Non-x86_64: keep the default C ABI (used by our UEFI payloads).
#[cfg(not(target_arch = "x86_64"))]
type KernelEntryPoint = extern "C" fn(u64) -> !;

// Global framebuffer info
static mut FB_BASE: usize = 0;
static mut FB_WIDTH: usize = 0;
static mut FB_HEIGHT: usize = 0;
static mut FB_STRIDE: usize = 0;

#[entry]
fn efi_main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    let st_ptr: *mut SystemTable<Boot> = &mut system_table;
    let bt_ptr: *const BootServices = unsafe { (*st_ptr).boot_services() as *const BootServices };
    let bt: &BootServices = unsafe { &*bt_ptr };

    unsafe {
        let _ = (*st_ptr).stdout().write_str("RayOS uefi_boot: start\n");
    }

    // Initialize GOP and get framebuffer
    let mut handles_buf: [Handle; 8] = unsafe { core::mem::zeroed() };
    if let Ok(handle_count) = bt.locate_handle(
        uefi::table::boot::SearchType::ByProtocol(&GraphicsOutput::GUID),
        Some(&mut handles_buf),
    ) {
        let handle_count = handle_count.unwrap();
        if handle_count > 0 {
            let gop_handle = handles_buf[0];

            // Open the GOP protocol
            if let Ok(gop_ptr) = bt.handle_protocol::<GraphicsOutput>(gop_handle) {
                let gop = unsafe { &mut *gop_ptr.unwrap().get() };

                let mode = gop.current_mode_info();

                // Clear the screen with BLT
                use uefi::proto::console::gop::{BltOp, BltPixel};
                let dark_blue = BltPixel::new(0x2e, 0x1a, 0x1a); // BGR format
                let (width, height) = mode.resolution();

                let _ = gop.blt(BltOp::VideoFill {
                    color: dark_blue,
                    dest: (0, 0),
                    dims: (width, height),
                });

                let mut fb = gop.frame_buffer();

                unsafe {
                    FB_BASE = fb.as_mut_ptr() as usize;
                    FB_WIDTH = width;
                    FB_HEIGHT = height;
                    FB_STRIDE = mode.stride();
                }

                // Draw bootloader banner
                draw_box(40, 40, 600, 200, 0x2a_2a_4e);
                draw_text(60, 60, "RayOS UEFI Bootloader v0.1", 0xff_ff_ff);
                draw_text(60, 90, "Bicameral GPU-Native Kernel", 0xaa_aa_ff);
                draw_text(60, 130, "Initializing framebuffer graphics...", 0x88_ff_88);
                draw_text(60, 160, "Loading kernel binary...", 0xff_ff_88);

                // Add a small delay so we can see this message
                bt.stall(1_000_000);
            }
        }
    }

    // Add debug output before attempting kernel load
    draw_text(60, 190, "About to load kernel...", 0xff_ff_00);

    draw_text(60, 210, "Step 1: GPU detection...", 0xff_ff_00);

    // GPU DETECTION AND INITIALIZATION - System 1
    draw_text(60, 230, "Step 2: Calling detect function...", 0xff_ff_00);

    let gpu_result = {
        // Use the UEFI text console for detailed logs.
        // (Framebuffer text is still drawn via draw_text below.)
        let stdout = unsafe { &mut (*st_ptr).stdout() };
        detect_gpu_hardware(bt, stdout)
    };
    draw_text(60, 250, "Step 3: Function returned...", 0xff_ff_00);

    match gpu_result {
        Some(_gpu_info) => {
            draw_text(60, 270, "GPU Found! System 1 ready", 0x00_ff_00);
        }
        None => {
            draw_text(60, 270, "No GPU - software mode", 0xff_88_00);
        }
    }

    draw_text(60, 290, "Step 4: Starting RayOS...", 0x88_ff_88);

    // Wait so we can see the boot messages
    bt.stall(1_000_000);

    // Validate framebuffer early (used by both kernel jump + embedded fallback).
    // On aarch64 headless QEMU runs, GOP may be absent; allow a serial-only bring-up path.
    let (fb_base, fb_width, fb_height, fb_stride) = unsafe {
        if FB_BASE == 0 {
            #[cfg(not(target_arch = "aarch64"))]
            {
                draw_text(60, 330, "ERROR: Framebuffer not initialized!", 0xff_00_00);
                bt.stall(5_000_000);
                return Status::DEVICE_ERROR;
            }
            #[cfg(target_arch = "aarch64")]
            {
                aarch64_uart_write_str("RayOS uefi_boot: GOP missing; continuing with fb=0 (serial-only)\n");
                (0usize, 0usize, 0usize, 0usize)
            }
        } else {
            (FB_BASE, FB_WIDTH, FB_HEIGHT, FB_STRIDE)
        }
    };

    // System 2 (bootloader-side): optionally load a local model blob from the ESP.
    // This must happen before ExitBootServices so we can use filesystem + log output.
    let (model_ptr, model_size) = match read_optional_blob(
        bt,
        image_handle,
        "EFI\\RAYOS\\model.bin",
        256 * 1024 * 1024,
    ) {
        Ok(Some((ptr, sz))) => {
            unsafe {
                let _ = (*st_ptr)
                    .stdout()
                    .write_str("RayOS uefi_boot: model.bin loaded; bytes=");
                // Print size in hex to avoid division/mod.
                let mut buf = [0u8; 16];
                let mut v = sz as u64;
                for i in (0..16).rev() {
                    let d = (v & 0xF) as u8;
                    buf[i] = if d < 10 { b'0' + d } else { b'A' + (d - 10) };
                    v >>= 4;
                }
                for b in buf {
                    let _ = (*st_ptr).stdout().write_char(b as char);
                }
                let _ = (*st_ptr).stdout().write_str("\n");
            }
            draw_text(60, 350, "Model: loaded (model.bin)", 0x00_ff_00);
            (ptr as u64, sz as u64)
        }
        Ok(None) => {
            unsafe {
                let _ = (*st_ptr)
                    .stdout()
                    .write_str("RayOS uefi_boot: model.bin not present\n");
            }
            draw_text(60, 350, "Model: not present", 0xaa_aa_aa);
            (0u64, 0u64)
        }
        Err(_e) => {
            unsafe {
                let _ = (*st_ptr)
                    .stdout()
                    .write_str("RayOS uefi_boot: model.bin load error; continuing\n");
            }
            draw_text(60, 350, "Model: load error (ignored)", 0xff_88_00);
            (0u64, 0u64)
        }
    };

    // Optional: stage a Volume backing blob in memory (filesystem-backed).
    // Used by the embedded runtime and/or kernel for early Volume queries.
    let (volume_ptr, volume_size) = match read_optional_blob(
        bt,
        image_handle,
        "EFI\\RAYOS\\volume.bin",
        64 * 1024 * 1024,
    ) {
        Ok(Some((ptr, sz))) => {
            unsafe {
                let _ = (*st_ptr)
                    .stdout()
                    .write_str("RayOS uefi_boot: volume.bin loaded\n");
            }
            (ptr as u64, sz as u64)
        }
        Ok(None) => (0u64, 0u64),
        Err(_) => (0u64, 0u64),
    };

    // Optional: stage embeddings + index blobs from the boot filesystem.
    // These are intended to hold precomputed vector embeddings / index metadata for Volume/RAG.
    let (embeddings_ptr, embeddings_size) = match read_optional_blob(
        bt,
        image_handle,
        "EFI\\RAYOS\\embeddings.bin",
        256 * 1024 * 1024,
    ) {
        Ok(Some((ptr, sz))) => {
            unsafe {
                let _ = (*st_ptr)
                    .stdout()
                    .write_str("RayOS uefi_boot: embeddings.bin loaded\n");
            }
            (ptr as u64, sz as u64)
        }
        Ok(None) => (0u64, 0u64),
        Err(_) => (0u64, 0u64),
    };

    let (index_ptr, index_size) = match read_optional_blob(
        bt,
        image_handle,
        "EFI\\RAYOS\\index.bin",
        256 * 1024 * 1024,
    ) {
        Ok(Some((ptr, sz))) => {
            unsafe {
                let _ = (*st_ptr)
                    .stdout()
                    .write_str("RayOS uefi_boot: index.bin loaded\n");
            }
            (ptr as u64, sz as u64)
        }
        Ok(None) => (0u64, 0u64),
        Err(_) => (0u64, 0u64),
    };

    // Optional: load an autorun prompt for host AI bridge testing.
    // If present, the aarch64 post-exit embedded loop will emit it as a RAYOS_INPUT line.
    #[cfg(target_arch = "aarch64")]
    {
        match read_optional_blob(bt, image_handle, "EFI\\RAYOS\\auto_prompt.txt", 4096) {
            Ok(Some((ptr, sz))) => unsafe {
                AUTORUN_PROMPT_PTR = ptr as u64;
                AUTORUN_PROMPT_SIZE = sz as u64;
                let _ = (*st_ptr)
                    .stdout()
                    .write_str("RayOS uefi_boot: auto_prompt.txt loaded\n");
            },
            Ok(None) => {}
            Err(_) => {}
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        unsafe {
            VOLUME_BLOB_PTR = volume_ptr;
            VOLUME_BLOB_SIZE = volume_size;
        }
    }

    unsafe {
        let _ = (*st_ptr)
            .stdout()
            .write_str("RayOS uefi_boot: loading kernel...\n");
    }
    let (kernel_entry, kernel_data, kernel_size) = match read_kernel_binary(bt, image_handle) {
        Ok(v) => v,
        Err(e) => {
            unsafe {
                let _ = (*st_ptr)
                    .stdout()
                    .write_str("RayOS uefi_boot: kernel load failed; staying in UEFI\n");
                let _ = (*st_ptr).stdout().write_str("RayOS uefi_boot: reason: ");
                let _ = (*st_ptr).stdout().write_str(e);
                let _ = (*st_ptr).stdout().write_str("\n");
            }
            draw_text(60, 310, "Kernel load failed", 0xff_88_00);
            draw_text(60, 330, e, 0xff_00_00);

            // Phase 2 Option A: on aarch64, allow boot even without a loadable kernel.bin.
            // Enter an embedded post-exit loop so we can validate framebuffer output after ExitBootServices.
            #[cfg(target_arch = "aarch64")]
            {
                aarch64_uart_write_str("RayOS uefi_boot: entering embedded mode (pre-exit)\n");
                draw_text(60, 370, "aarch64: entering embedded mode", 0xaa_ff_aa);
                bt.stall(1_000_000);

                let (volume_ptr, volume_size) = unsafe { (VOLUME_BLOB_PTR, VOLUME_BLOB_SIZE) };
                let st_clone = unsafe { system_table.unsafe_clone() };
                let boot_info_ptr = match prepare_boot_info_and_exit_boot_services(
                    st_clone,
                    bt,
                    image_handle,
                    fb_base,
                    fb_width,
                    fb_height,
                    fb_stride,
                    model_ptr,
                    model_size,
                    volume_ptr,
                    volume_size,
                    embeddings_ptr,
                    embeddings_size,
                    index_ptr,
                    index_size,
                ) {
                    Ok(ptr) => ptr,
                    Err(status) => {
                        aarch64_uart_write_str("RayOS uefi_boot: ExitBootServices FAILED\n");
                        // Still in UEFI context if ExitBootServices failed.
                        draw_text(60, 390, "ERROR: ExitBootServices failed", 0xff_00_00);
                        bt.stall(5_000_000);
                        return status;
                    }
                };

                aarch64_uart_write_str("RayOS uefi_boot: ExitBootServices OK; entering post-exit loop\n");
                draw_text(60, 430, "Post-exit: embedded RayOS loop", 0x88_ff_88);
                let (autorun_ptr, autorun_size) = unsafe { (AUTORUN_PROMPT_PTR, AUTORUN_PROMPT_SIZE) };
                let (volume_ptr, volume_size) = unsafe { (VOLUME_BLOB_PTR, VOLUME_BLOB_SIZE) };
                rayos_post_exit_embedded_loop(
                    boot_info_ptr as u64,
                    autorun_ptr,
                    autorun_size,
                    volume_ptr,
                    volume_size,
                );
            }

            // Default behavior (x86_64 tests): remain in UEFI UI loop.
            #[cfg(not(target_arch = "aarch64"))]
            {
                bt.stall(2_000_000);
                rayos_main_loop(bt)
            }
        }
    };

    unsafe {
        let _ = (*st_ptr)
            .stdout()
            .write_str("RayOS uefi_boot: kernel read OK; exiting boot services\n");
    }

    // On aarch64 under AAVMF, firmware page tables commonly mark generic RAM as non-executable.
    // If we copy an ELF segment into plain RAM and jump to it post-exit, we can take an
    // instruction abort immediately. Reserve the ELF PT_LOAD destination pages *pre-exit*
    // as LOADER_CODE so firmware maps them executable.
    let mut aarch64_kernel_segments_reserved = true;
    #[cfg(target_arch = "aarch64")]
    {
        match reserve_elf_segments_pre_exit(bt, kernel_data, kernel_size) {
            Ok(()) => {
                unsafe {
                    let _ = (*st_ptr)
                        .stdout()
                        .write_str("RayOS uefi_boot: aarch64: reserved kernel PT_LOAD pages (LOADER_CODE)\n");
                }
            }
            Err(e) => {
                aarch64_kernel_segments_reserved = false;
                unsafe {
                    let _ = (*st_ptr)
                        .stdout()
                        .write_str("RayOS uefi_boot: aarch64: reserve PT_LOAD pages failed; falling back to embedded mode\n");
                    let _ = (*st_ptr).stdout().write_str("RayOS uefi_boot: reason: ");
                    let _ = (*st_ptr).stdout().write_str(e);
                    let _ = (*st_ptr).stdout().write_str("\n");
                }
                draw_text(60, 350, "aarch64: PT_LOAD reserve failed", 0xff_88_00);
                draw_text(60, 370, e, 0xff_00_00);
            }
        }
    }

    draw_text(60, 310, "Kernel read OK", 0x88_ff_88);
    draw_text(60, 330, "FB OK; preparing to jump...", 0xaa_ff_aa);
    bt.stall(1_000_000);

    // Prepare BootInfo + memory map, then ExitBootServices.
    let st_clone = unsafe { system_table.unsafe_clone() };
    let boot_info_ptr = match prepare_boot_info_and_exit_boot_services(
        st_clone,
        bt,
        image_handle,
        fb_base,
        fb_width,
        fb_height,
        fb_stride,
        model_ptr,
        model_size,
        volume_ptr,
        volume_size,
        embeddings_ptr,
        embeddings_size,
        index_ptr,
        index_size,
    ) {
        Ok(ptr) => ptr,
        Err(status) => {
            draw_text(60, 350, "ERROR: ExitBootServices failed", 0xff_00_00);
            bt.stall(5_000_000);
            return status;
        }
    };

    // If we couldn't reserve executable pages for the kernel segments, do not jump.
    // Instead, continue with the aarch64 embedded post-exit loop (Option A harness).
    #[cfg(target_arch = "aarch64")]
    if !aarch64_kernel_segments_reserved {
        aarch64_uart_write_str("RayOS uefi_boot: post-exit: kernel exec reservation failed; entering embedded loop\n");
        draw_text(60, 390, "Post-exit: embedded RayOS loop", 0x88_ff_88);
        let (autorun_ptr, autorun_size) = unsafe { (AUTORUN_PROMPT_PTR, AUTORUN_PROMPT_SIZE) };
        let (volume_ptr, volume_size) = unsafe { (VOLUME_BLOB_PTR, VOLUME_BLOB_SIZE) };
        rayos_post_exit_embedded_loop(
            boot_info_ptr as u64,
            autorun_ptr,
            autorun_size,
            volume_ptr,
            volume_size,
        );
    }

    // After ExitBootServices, do not touch UEFI BootServices or console.
    // Load PT_LOAD segments now: OVMF often reports those pages as BOOT_SERVICES_DATA
    // pre-exit, so AllocatePages(Address) can fail even though the pages are reclaimable.
    unsafe {
        draw_text(60, 370, "Post-exit: loading ELF segments...", 0xaa_aa_ff);
        if let Err(e) = load_elf_segments_post_exit(kernel_data, kernel_size) {
            draw_text(60, 390, "ERROR: ELF segment load failed", 0xff_00_00);
            draw_text(60, 410, e, 0xff_88_00);
            loop {
                core::hint::spin_loop();
            }
        }
    }

    // Final pre-jump marker (framebuffer write is still valid post-exit).
    draw_text(60, 430, "Post-exit: jumping to kernel entry...", 0x88_ff_88);

    kernel_entry(boot_info_ptr as u64);
}

fn prepare_boot_info_and_exit_boot_services(
    system_table: SystemTable<Boot>,
    bt: &BootServices,
    image_handle: Handle,
    fb_base: usize,
    fb_width: usize,
    fb_height: usize,
    fb_stride: usize,
    model_ptr: u64,
    model_size: u64,
    volume_ptr: u64,
    volume_size: u64,
    embeddings_ptr: u64,
    embeddings_size: u64,
    index_ptr: u64,
    index_size: u64,
) -> Result<*const BootInfo, Status> {
    // Find ACPI RSDP before exiting boot services.
    let mut rsdp_addr: u64 = 0;
    for ct in system_table.config_table() {
        if ct.guid == ACPI2_GUID {
            rsdp_addr = ct.address as u64;
            break;
        }
        if rsdp_addr == 0 && ct.guid == ACPI_GUID {
            rsdp_addr = ct.address as u64;
        }
    }
    // Best-effort: query current wall-clock time from UEFI runtime services.
    // Do this before ExitBootServices so we can give the kernel a usable baseline.
    let (boot_unix_seconds, boot_time_valid) = match system_table.runtime_services().get_time() {
        Ok(t) => {
            let t = t.unwrap();
            match uefi_time_to_unix_seconds_utc(&t) {
                Some(s) => (s, 1u32),
                None => (0u64, 0u32),
            }
        }
        Err(_) => {
            (0u64, 0u32)
        }
    };

    fn alloc_pages_high(bt: &BootServices, size: usize) -> Result<usize, Status> {
        let pages = size.saturating_add(4095) / 4096;
        let addr = bt
            .allocate_pages(
                uefi::table::boot::AllocateType::MaxAddress(0xFFFF_F000),
                MemoryType::LOADER_DATA,
                pages,
            )
            .map_err(|e| e.status())?
            .unwrap();
        Ok(addr as usize)
    }

    // Allocate BootInfo and buffers from high pages so they are unlikely to overlap
    // the kernel's chosen load address.
    let boot_info_storage = alloc_pages_high(bt, core::mem::size_of::<BootInfo>())?;
    let boot_info = boot_info_storage as *mut BootInfo;

    let mmap_size = bt.memory_map_size();
    let mmap_buf_size = mmap_size + (core::mem::size_of::<MemoryDescriptor>() * 8);
    let mmap_storage = alloc_pages_high(bt, mmap_buf_size)?;
    let mmap_buffer = unsafe { core::slice::from_raw_parts_mut(mmap_storage as *mut u8, mmap_buf_size) };

    let max_desc_count = (mmap_buf_size / core::mem::size_of::<MemoryDescriptor>()) + 32;
    let packed_desc_size = core::mem::size_of::<BootMemoryDescriptor>();
    let packed_bytes = max_desc_count * packed_desc_size;
    let packed_storage = alloc_pages_high(bt, packed_bytes)?;
    let packed_ptr = packed_storage as *mut BootMemoryDescriptor;

    // Exit boot services and grab the final memory map.
    let (_rt, mmap_iter) = system_table
        .exit_boot_services(image_handle, mmap_buffer)
        .map_err(|e| e.status())?
        .unwrap();

    // Pack descriptors into an ABI-stable array.
    let mut desc_count: usize = 0;
    for d in mmap_iter {
        if desc_count >= max_desc_count {
            break;
        }
        unsafe {
            core::ptr::write(
                packed_ptr.add(desc_count),
                BootMemoryDescriptor {
                    ty: d.ty.0,
                    _padding: 0,
                    phys_start: d.phys_start,
                    virt_start: d.virt_start,
                    page_count: d.page_count,
                    att: d.att.bits(),
                },
            );
        }
        desc_count = desc_count.wrapping_add(1);
    }
    let mmap_used_bytes = desc_count.saturating_mul(packed_desc_size);

    unsafe {
        *boot_info = BootInfo {
            magic: BOOTINFO_MAGIC,

            fb_base: fb_base as u64,
            fb_width: fb_width as u32,
            fb_height: fb_height as u32,
            fb_stride: fb_stride as u32,
            _fb_reserved: 0,

            rsdp_addr,

            memory_map_ptr: packed_storage as u64,
            memory_map_size: mmap_used_bytes as u64,
            memory_desc_size: packed_desc_size as u64,
            memory_desc_version: 1,
            _mmap_reserved: 0,

            model_ptr,
            model_size,

            volume_ptr,
            volume_size,

            embeddings_ptr,
            embeddings_size,

            index_ptr,
            index_size,

            boot_unix_seconds,
            boot_time_valid,
            _time_reserved: 0,
        };
    }

    Ok(boot_info as *const BootInfo)
}

/// Read an optional file from the boot media into LOADER_DATA pages.
/// Returns Ok(None) if the file is not present or not a regular file.
fn read_optional_blob(
    bt: &BootServices,
    image_handle: Handle,
    path: &'static str,
    max_size: usize,
) -> Result<Option<(*const u8, usize)>, &'static str> {
    // Get the file system protocol via LoadedImage.
    let loaded_image = bt
        .handle_protocol::<LoadedImage>(image_handle)
        .map_err(|_| "Failed to get LoadedImage")?;
    let loaded_image = unsafe { &*loaded_image.unwrap().get() };

    let device_handle = loaded_image.device();

    let device_path = bt
        .handle_protocol::<DevicePath>(device_handle)
        .map_err(|_| "Failed to get DevicePath")?;
    let device_path = unsafe { &mut *device_path.unwrap().get() };

    let fs_handle = bt
        .locate_device_path::<SimpleFileSystem>(device_path)
        .map_err(|_| "Failed to locate file system")?;

    let fs_ptr = bt
        .handle_protocol::<SimpleFileSystem>(fs_handle.unwrap())
        .map_err(|_| "Failed to open file system protocol")?;
    let fs = unsafe { &mut *fs_ptr.unwrap().get() };

    let root = fs.open_volume().map_err(|_| "Failed to open root volume")?;
    let mut root = root.unwrap();

    let file_handle = match root.open(path, FileMode::Read, FileAttribute::empty()) {
        Ok(v) => v.unwrap(),
        Err(_) => return Ok(None),
    };

    let file_type = file_handle
        .into_type()
        .map_err(|_| "Failed to determine file type")?
        .unwrap();

    let mut file: RegularFile = match file_type {
        FileType::Regular(f) => f,
        _ => return Ok(None),
    };

    let _ = file
        .set_position(RegularFile::END_OF_FILE)
        .map_err(|_| "Failed to seek blob")?;
    let file_size = file.get_position().map_err(|_| "Failed to read blob size")?.unwrap() as usize;
    let _ = file.set_position(0);

    if file_size == 0 || file_size > max_size {
        return Ok(None);
    }

    let pages = (file_size + 4095) / 4096;
    let addr = bt
        .allocate_pages(
            uefi::table::boot::AllocateType::MaxAddress(0xFFFF_F000),
            uefi::table::boot::MemoryType::LOADER_DATA,
            pages,
        )
        .map_err(|_| "Failed to allocate blob memory")?
        .unwrap();

    let buf = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, file_size) };
    let bytes_read = file.read(buf).map_err(|_| "Failed to read blob")?.unwrap();
    if bytes_read != file_size {
        return Err("Short read while loading blob");
    }

    Ok(Some((buf.as_ptr(), file_size)))
}

/// Read the kernel file into a temporary buffer (pre-ExitBootServices).
/// Returns (entry_point, buffer_ptr, size_bytes).
fn read_kernel_binary(
    bt: &BootServices,
    image_handle: Handle,
) -> Result<(KernelEntryPoint, *const u8, usize), &'static str> {
    // Get the file system protocol via LoadedImage
    let loaded_image = bt
        .handle_protocol::<LoadedImage>(image_handle)
        .map_err(|_| "Failed to get LoadedImage")?;
    let loaded_image = unsafe { &*loaded_image.unwrap().get() };

    let device_handle = loaded_image.device();

    let device_path = bt
        .handle_protocol::<DevicePath>(device_handle)
        .map_err(|_| "Failed to get DevicePath")?;
    let device_path = unsafe { &mut *device_path.unwrap().get() };

    let fs_handle = bt
        .locate_device_path::<SimpleFileSystem>(device_path)
        .map_err(|_| "Failed to locate file system")?;

    let fs_ptr = bt
        .handle_protocol::<SimpleFileSystem>(fs_handle.unwrap())
        .map_err(|_| "Failed to open file system protocol")?;
    let fs = unsafe { &mut *fs_ptr.unwrap().get() };

    // Open root directory
    let root = fs
        .open_volume()
        .map_err(|_| "Failed to open root volume")?;
    let mut root = root.unwrap();

    // Open kernel file at \EFI\RAYOS\kernel.bin
    let kernel_handle = root
        .open("EFI\\RAYOS\\kernel.bin", FileMode::Read, FileAttribute::empty())
        .map_err(|_| "Failed to open kernel.bin")?
        .unwrap();

    let kernel_file = kernel_handle
        .into_type()
        .map_err(|_| "Failed to determine kernel file type")?
        .unwrap();

    let mut kernel_file: RegularFile = match kernel_file {
        FileType::Regular(f) => f,
        _ => return Err("kernel.bin is not a regular file"),
    };

    // Determine file size
    let _ = kernel_file
        .set_position(RegularFile::END_OF_FILE)
        .map_err(|_| "Failed to seek kernel")?;
    let file_size = kernel_file
        .get_position()
        .map_err(|_| "Failed to read kernel size")?
        .unwrap() as usize;
    let _ = kernel_file.set_position(0);

    if file_size == 0 || file_size > 32 * 1024 * 1024 {
        return Err("Invalid kernel size");
    }

    // Allocate a temporary buffer for the kernel image.
    let pages = (file_size + 4095) / 4096;
    let temp_kernel_addr = bt
        .allocate_pages(
            uefi::table::boot::AllocateType::MaxAddress(0xFFFF_F000),
            uefi::table::boot::MemoryType::LOADER_DATA,
            pages,
        )
        .map_err(|_| "Failed to allocate memory")?
        .unwrap();

    let kernel_buffer = unsafe { core::slice::from_raw_parts_mut(temp_kernel_addr as *mut u8, file_size) };
    let bytes_read = kernel_file
        .read(kernel_buffer)
        .map_err(|_| "Failed to read kernel")?
        .unwrap();
    if bytes_read != file_size {
        return Err("Incomplete kernel read");
    }

    let entry_point = parse_elf_entry(kernel_buffer)?;
    Ok((entry_point, kernel_buffer.as_ptr(), file_size))
}

fn parse_elf_entry(kernel_data: &[u8]) -> Result<KernelEntryPoint, &'static str> {
    fn read_u64_at(buf: &[u8], off: usize) -> Result<u64, &'static str> {
        if off + 8 > buf.len() {
            return Err("ELF header out of bounds");
        }
        Ok(u64::from_le_bytes([
            buf[off],
            buf[off + 1],
            buf[off + 2],
            buf[off + 3],
            buf[off + 4],
            buf[off + 5],
            buf[off + 6],
            buf[off + 7],
        ]))
    }

    fn read_u16_at(buf: &[u8], off: usize) -> Result<u16, &'static str> {
        if off + 2 > buf.len() {
            return Err("ELF header out of bounds");
        }
        Ok(u16::from_le_bytes([buf[off], buf[off + 1]]))
    }

    fn read_u32_at(buf: &[u8], off: usize) -> Result<u32, &'static str> {
        if off + 4 > buf.len() {
            return Err("ELF header out of bounds");
        }
        Ok(u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]))
    }

    if kernel_data.len() < 64 {
        return Err("Kernel too small");
    }
    if &kernel_data[0..4] != b"\x7fELF" {
        return Err("Not an ELF file");
    }
    if kernel_data[4] != 2 {
        return Err("Not 64-bit ELF");
    }
    if kernel_data[5] != 1 {
        return Err("Not little-endian ELF");
    }

    // Reject kernels built for the wrong ISA.
    // e_machine (u16) is at offset 0x12 in ELF64.
    let e_machine = read_u16_at(kernel_data, 0x12)?;
    #[cfg(target_arch = "x86_64")]
    const EXPECTED_MACHINE: u16 = 0x3E; // EM_X86_64
    #[cfg(target_arch = "aarch64")]
    const EXPECTED_MACHINE: u16 = 0xB7; // EM_AARCH64
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    const EXPECTED_MACHINE: u16 = 0;

    if EXPECTED_MACHINE != 0 && e_machine != EXPECTED_MACHINE {
        return Err("Kernel ISA mismatch (ELF e_machine)");
    }

    // e_entry is a virtual address in ELF terms, but in our UEFI loader we copy
    // PT_LOAD segments to either p_paddr (preferred if non-zero) or p_vaddr.
    // To avoid jumping to an address that isn't mapped by firmware page tables,
    // translate e_entry to the actual loaded destination.
    let entry_vaddr = read_u64_at(kernel_data, 0x18)?;
    if entry_vaddr == 0 {
        return Err("Invalid entry point");
    }

    let ph_offset = read_u64_at(kernel_data, 0x20)? as usize;
    let ph_entsize = read_u16_at(kernel_data, 0x36)? as usize;
    let ph_num = read_u16_at(kernel_data, 0x38)? as usize;
    if ph_entsize < 56 {
        return Err("Invalid program header entry size");
    }
    if ph_offset >= kernel_data.len() {
        return Err("Invalid program header offset");
    }

    for i in 0..ph_num {
        let ph_start = ph_offset.saturating_add(i.saturating_mul(ph_entsize));
        if ph_start + 56 > kernel_data.len() {
            continue;
        }

        let p_type = read_u32_at(kernel_data, ph_start)?;
        if p_type != 1 {
            continue;
        }

        let p_vaddr = read_u64_at(kernel_data, ph_start + 16)?;
        let p_paddr = read_u64_at(kernel_data, ph_start + 24)?;
        let p_memsz = read_u64_at(kernel_data, ph_start + 40)?;
        if p_memsz == 0 {
            continue;
        }

        let seg_start = p_vaddr;
        let seg_end = p_vaddr.saturating_add(p_memsz);
        if entry_vaddr >= seg_start && entry_vaddr < seg_end {
            let dest = if p_paddr != 0 { p_paddr } else { p_vaddr };
            let entry_loaded = dest.saturating_add(entry_vaddr.saturating_sub(p_vaddr));
            if entry_loaded == 0 {
                return Err("Invalid loaded entry");
            }
            return Ok(unsafe { core::mem::transmute::<usize, KernelEntryPoint>(entry_loaded as usize) });
        }
    }

    // If we couldn't find a containing segment, fall back to the raw entry.
    // This keeps compatibility with very small/flat binaries.
    Ok(unsafe { core::mem::transmute::<usize, KernelEntryPoint>(entry_vaddr as usize) })
}

/// Reserve ELF PT_LOAD destination pages as LOADER_CODE (pre-ExitBootServices).
///
/// On aarch64, firmware often maps generic RAM non-executable; reserving pages as
/// LOADER_CODE ensures the destination pages are mapped executable by the firmware.
#[cfg(target_arch = "aarch64")]
fn reserve_elf_segments_pre_exit(
    bt: &BootServices,
    kernel_data: *const u8,
    kernel_size: usize,
) -> Result<(), &'static str> {
    let kernel_data = unsafe { core::slice::from_raw_parts(kernel_data, kernel_size) };

    if kernel_data.len() < 64 {
        return Err("Kernel too small");
    }
    if &kernel_data[0..4] != b"\x7fELF" {
        return Err("Not an ELF file");
    }
    if kernel_data[4] != 2 {
        return Err("Not 64-bit ELF");
    }
    if kernel_data[5] != 1 {
        return Err("Not little-endian ELF");
    }

    let ph_offset = u64::from_le_bytes([
        kernel_data[0x20],
        kernel_data[0x21],
        kernel_data[0x22],
        kernel_data[0x23],
        kernel_data[0x24],
        kernel_data[0x25],
        kernel_data[0x26],
        kernel_data[0x27],
    ]) as usize;
    let ph_entsize = u16::from_le_bytes([kernel_data[0x36], kernel_data[0x37]]) as usize;
    let ph_num = u16::from_le_bytes([kernel_data[0x38], kernel_data[0x39]]) as usize;
    if ph_entsize < 56 {
        return Err("Invalid program header entry size");
    }
    if ph_offset >= kernel_data.len() {
        return Err("Invalid program header offset");
    }

    for i in 0..ph_num {
        let ph_start = ph_offset.saturating_add(i.saturating_mul(ph_entsize));
        if ph_start + 56 > kernel_data.len() {
            continue;
        }

        let p_type = u32::from_le_bytes([
            kernel_data[ph_start],
            kernel_data[ph_start + 1],
            kernel_data[ph_start + 2],
            kernel_data[ph_start + 3],
        ]);
        if p_type != 1 {
            continue;
        }

        let p_vaddr = u64::from_le_bytes([
            kernel_data[ph_start + 16],
            kernel_data[ph_start + 17],
            kernel_data[ph_start + 18],
            kernel_data[ph_start + 19],
            kernel_data[ph_start + 20],
            kernel_data[ph_start + 21],
            kernel_data[ph_start + 22],
            kernel_data[ph_start + 23],
        ]) as usize;

        let p_paddr = u64::from_le_bytes([
            kernel_data[ph_start + 24],
            kernel_data[ph_start + 25],
            kernel_data[ph_start + 26],
            kernel_data[ph_start + 27],
            kernel_data[ph_start + 28],
            kernel_data[ph_start + 29],
            kernel_data[ph_start + 30],
            kernel_data[ph_start + 31],
        ]) as usize;

        let p_memsz = u64::from_le_bytes([
            kernel_data[ph_start + 40],
            kernel_data[ph_start + 41],
            kernel_data[ph_start + 42],
            kernel_data[ph_start + 43],
            kernel_data[ph_start + 44],
            kernel_data[ph_start + 45],
            kernel_data[ph_start + 46],
            kernel_data[ph_start + 47],
        ]) as usize;

        if p_memsz == 0 {
            continue;
        }

        let dest_addr = if p_paddr != 0 { p_paddr } else { p_vaddr };
        if dest_addr == 0 {
            return Err("ELF segment has null destination");
        }

        let start = align_down(dest_addr, 4096);
        let end = align_up(dest_addr.saturating_add(p_memsz), 4096);
        let pages = (end.saturating_sub(start) + 4095) / 4096;
        if pages == 0 {
            continue;
        }

        bt.allocate_pages(
            uefi::table::boot::AllocateType::Address(start),
            uefi::table::boot::MemoryType::LOADER_CODE,
            pages,
        )
        .map_err(|_| "Failed to reserve PT_LOAD pages")?
        .unwrap();
    }

    Ok(())
}

/// Load ELF PT_LOAD segments into their requested addresses.
///
/// Must be called only after ExitBootServices.
unsafe fn load_elf_segments_post_exit(kernel_data: *const u8, kernel_size: usize) -> Result<(), &'static str> {
    let kernel_data = core::slice::from_raw_parts(kernel_data, kernel_size);

    if kernel_data.len() < 64 {
        return Err("Kernel too small");
    }
    if &kernel_data[0..4] != b"\x7fELF" {
        return Err("Not an ELF file");
    }
    if kernel_data[4] != 2 {
        return Err("Not 64-bit ELF");
    }
    if kernel_data[5] != 1 {
        return Err("Not little-endian ELF");
    }

    let ph_offset = u64::from_le_bytes([
        kernel_data[0x20],
        kernel_data[0x21],
        kernel_data[0x22],
        kernel_data[0x23],
        kernel_data[0x24],
        kernel_data[0x25],
        kernel_data[0x26],
        kernel_data[0x27],
    ]) as usize;
    let ph_entsize = u16::from_le_bytes([kernel_data[0x36], kernel_data[0x37]]) as usize;
    let ph_num = u16::from_le_bytes([kernel_data[0x38], kernel_data[0x39]]) as usize;
    if ph_entsize < 56 {
        return Err("Invalid program header entry size");
    }
    if ph_offset >= kernel_data.len() {
        return Err("Invalid program header offset");
    }

    let load_base = kernel_data.as_ptr() as usize;
    for i in 0..ph_num {
        let ph_start = ph_offset.saturating_add(i.saturating_mul(ph_entsize));
        if ph_start + 56 > kernel_data.len() {
            continue;
        }

        let p_type = u32::from_le_bytes([
            kernel_data[ph_start],
            kernel_data[ph_start + 1],
            kernel_data[ph_start + 2],
            kernel_data[ph_start + 3],
        ]);
        if p_type != 1 {
            continue;
        }

        let p_offset = u64::from_le_bytes([
            kernel_data[ph_start + 8],
            kernel_data[ph_start + 9],
            kernel_data[ph_start + 10],
            kernel_data[ph_start + 11],
            kernel_data[ph_start + 12],
            kernel_data[ph_start + 13],
            kernel_data[ph_start + 14],
            kernel_data[ph_start + 15],
        ]) as usize;

        let p_vaddr = u64::from_le_bytes([
            kernel_data[ph_start + 16],
            kernel_data[ph_start + 17],
            kernel_data[ph_start + 18],
            kernel_data[ph_start + 19],
            kernel_data[ph_start + 20],
            kernel_data[ph_start + 21],
            kernel_data[ph_start + 22],
            kernel_data[ph_start + 23],
        ]) as usize;

        let p_paddr = u64::from_le_bytes([
            kernel_data[ph_start + 24],
            kernel_data[ph_start + 25],
            kernel_data[ph_start + 26],
            kernel_data[ph_start + 27],
            kernel_data[ph_start + 28],
            kernel_data[ph_start + 29],
            kernel_data[ph_start + 30],
            kernel_data[ph_start + 31],
        ]) as usize;

        let p_filesz = u64::from_le_bytes([
            kernel_data[ph_start + 32],
            kernel_data[ph_start + 33],
            kernel_data[ph_start + 34],
            kernel_data[ph_start + 35],
            kernel_data[ph_start + 36],
            kernel_data[ph_start + 37],
            kernel_data[ph_start + 38],
            kernel_data[ph_start + 39],
        ]) as usize;

        let p_memsz = u64::from_le_bytes([
            kernel_data[ph_start + 40],
            kernel_data[ph_start + 41],
            kernel_data[ph_start + 42],
            kernel_data[ph_start + 43],
            kernel_data[ph_start + 44],
            kernel_data[ph_start + 45],
            kernel_data[ph_start + 46],
            kernel_data[ph_start + 47],
        ]) as usize;

        if p_memsz == 0 {
            continue;
        }
        if p_filesz > p_memsz {
            return Err("ELF segment filesz > memsz");
        }
        if p_offset.checked_add(p_filesz).map(|end| end <= kernel_data.len()) != Some(true) {
            return Err("ELF segment out of bounds");
        }

        let dest_addr = if p_paddr != 0 { p_paddr } else { p_vaddr };
        if dest_addr == 0 {
            return Err("ELF segment has null destination");
        }

        let dest = dest_addr as *mut u8;
        let src = (load_base + p_offset) as *const u8;
        core::ptr::copy_nonoverlapping(src, dest, p_filesz);

        if p_memsz > p_filesz {
            core::ptr::write_bytes(dest.add(p_filesz), 0, p_memsz - p_filesz);
        }
    }

    Ok(())
}

//=============================================================================
// GPU DETECTION AND SYSTEM 1 INITIALIZATION
//=============================================================================

// Minimal bindings for EFI_PCI_IO_PROTOCOL so we can do real device discovery
// and print vendor/device/class codes even when GOP is absent (aarch64 headless).

#[repr(transparent)]
#[derive(Clone, Copy)]
struct EfiPciIoProtocolWidth(u32);

impl EfiPciIoProtocolWidth {
    const UINT32: Self = Self(2);
}

type EfiPciIoProtocolIoMem = extern "efiapi" fn(
    this: *mut EfiPciIoProtocol,
    width: EfiPciIoProtocolWidth,
    bar_index: u8,
    offset: u64,
    count: usize,
    buffer: *mut core::ffi::c_void,
) -> Status;

#[repr(C)]
struct EfiPciIoProtocolAccess {
    read: EfiPciIoProtocolIoMem,
    write: EfiPciIoProtocolIoMem,
}

type EfiPciIoProtocolConfig = extern "efiapi" fn(
    this: *mut EfiPciIoProtocol,
    width: EfiPciIoProtocolWidth,
    offset: u32,
    count: usize,
    buffer: *mut core::ffi::c_void,
) -> Status;

#[repr(C)]
struct EfiPciIoProtocolConfigAccess {
    read: EfiPciIoProtocolConfig,
    write: EfiPciIoProtocolConfig,
}

type EfiPciIoProtocolGetLocation = extern "efiapi" fn(
    this: *mut EfiPciIoProtocol,
    segment_number: *mut usize,
    bus_number: *mut usize,
    device_number: *mut usize,
    function_number: *mut usize,
) -> Status;

#[unsafe_guid("4cf5b200-68b8-4ca5-9eec-b23e3f50029a")]
#[repr(C)]
struct EfiPciIoProtocol {
    poll_mem: usize,
    poll_io: usize,
    mem: EfiPciIoProtocolAccess,
    io: EfiPciIoProtocolAccess,
    pci: EfiPciIoProtocolConfigAccess,
    copy_mem: usize,
    map: usize,
    unmap: usize,
    allocate_buffer: usize,
    free_buffer: usize,
    flush: usize,
    get_location: EfiPciIoProtocolGetLocation,
    attributes: usize,
    get_bar_attributes: usize,
    set_bar_attributes: usize,
    rom_size: u64,
    rom_image: *mut core::ffi::c_void,
}

impl uefi::proto::Protocol for EfiPciIoProtocol {}

fn probe_pci_display_controllers(
    bt: &BootServices,
    stdout: &mut uefi::proto::console::text::Output,
) -> usize {
    use uefi::table::boot::SearchType;

    let mut handles_buf: [Handle; 64] = unsafe { core::mem::zeroed() };
    let handle_count = match bt.locate_handle(
        SearchType::ByProtocol(&EfiPciIoProtocol::GUID),
        Some(&mut handles_buf),
    ) {
        Ok(c) => c.unwrap(),
        Err(_) => 0,
    };

    let _ = writeln!(stdout, "RayOS: PCI scan: PciIo handles={handle_count}");
    #[cfg(target_arch = "aarch64")]
    {
        aarch64_uart_write_str("RayOS: PCI scan: PciIo handles=0x");
        aarch64_uart_write_hex_u64(handle_count as u64);
        aarch64_uart_write_str("\n");
    }

    if handle_count == 0 {
        return 0;
    }

    let mut found = 0usize;

    for &handle in handles_buf.iter().take(handle_count) {
        let pci_cell = match bt.handle_protocol::<EfiPciIoProtocol>(handle) {
            Ok(v) => v.unwrap(),
            Err(_) => continue,
        };

        let pci = unsafe { &mut *pci_cell.get() };
        let this = pci as *mut EfiPciIoProtocol;

        let mut seg: usize = 0;
        let mut bus: usize = 0;
        let mut dev: usize = 0;
        let mut func: usize = 0;
        let st_loc = (pci.get_location)(this, &mut seg, &mut bus, &mut dev, &mut func);
        if st_loc.is_error() {
            continue;
        }

        let mut vendor_device: u32 = 0;
        let st_vd = (pci.pci.read)(
            this,
            EfiPciIoProtocolWidth::UINT32,
            0,
            1,
            (&mut vendor_device as *mut u32).cast(),
        );
        if st_vd.is_error() {
            continue;
        }

        let vendor_id: u16 = (vendor_device & 0xFFFF) as u16;
        if vendor_id == 0xFFFF {
            continue;
        }
        let device_id: u16 = ((vendor_device >> 16) & 0xFFFF) as u16;

        let mut class_reg: u32 = 0;
        let st_class = (pci.pci.read)(
            this,
            EfiPciIoProtocolWidth::UINT32,
            0x08,
            1,
            (&mut class_reg as *mut u32).cast(),
        );
        if st_class.is_error() {
            continue;
        }

        let base_class: u8 = ((class_reg >> 24) & 0xFF) as u8;
        let sub_class: u8 = ((class_reg >> 16) & 0xFF) as u8;
        let prog_if: u8 = ((class_reg >> 8) & 0xFF) as u8;
        let rev_id: u8 = (class_reg & 0xFF) as u8;

        // PCI base class 0x03 = Display controller
        if base_class != 0x03 {
            continue;
        }

        found += 1;
        let _ = writeln!(
            stdout,
            "RayOS: PCI display: seg={seg} bus={bus} dev={dev} func={func} vendor=0x{vendor_id:04x} device=0x{device_id:04x} class=0x{base_class:02x} sub=0x{sub_class:02x} prog_if=0x{prog_if:02x} rev=0x{rev_id:02x}",
        );

        #[cfg(target_arch = "aarch64")]
        {
            aarch64_uart_write_str("RayOS: PCI display: seg=0x");
            aarch64_uart_write_hex_u64(seg as u64);
            aarch64_uart_write_str(" bus=0x");
            aarch64_uart_write_hex_u64(bus as u64);
            aarch64_uart_write_str(" dev=0x");
            aarch64_uart_write_hex_u64(dev as u64);
            aarch64_uart_write_str(" func=0x");
            aarch64_uart_write_hex_u64(func as u64);
            aarch64_uart_write_str(" vendor=0x");
            aarch64_uart_write_hex_u64(vendor_id as u64);
            aarch64_uart_write_str(" device=0x");
            aarch64_uart_write_hex_u64(device_id as u64);
            aarch64_uart_write_str(" class=0x");
            aarch64_uart_write_hex_u64(base_class as u64);
            aarch64_uart_write_str(" sub=0x");
            aarch64_uart_write_hex_u64(sub_class as u64);
            aarch64_uart_write_str("\n");
        }
    }

    let _ = writeln!(stdout, "RayOS: PCI scan: display controllers found={found}");
    #[cfg(target_arch = "aarch64")]
    {
        aarch64_uart_write_str("RayOS: PCI scan: display controllers found=0x");
        aarch64_uart_write_hex_u64(found as u64);
        aarch64_uart_write_str("\n");
    }

    found
}

struct GpuInfo {
    gop_handles: usize,
    width: usize,
    height: usize,
    stride: usize,
    fb_base: u64,
    fb_size: usize,
    pixel_format: uefi::proto::console::gop::PixelFormat,
}

fn pixel_format_name(pf: uefi::proto::console::gop::PixelFormat) -> &'static str {
    use uefi::proto::console::gop::PixelFormat;
    match pf {
        PixelFormat::Rgb => "RGB",
        PixelFormat::Bgr => "BGR",
        PixelFormat::Bitmask => "Bitmask",
        PixelFormat::BltOnly => "BltOnly",
    }
}

/// GPU detection PoC: probe UEFI Graphics Output Protocol (GOP) and report
/// framebuffer + mode information.
///
/// This is the simplest, most reliable "GPU present" check in UEFI/QEMU.
fn detect_gpu_hardware(
    bt: &BootServices,
    stdout: &mut uefi::proto::console::text::Output,
) -> Option<GpuInfo> {
    use uefi::proto::console::gop::GraphicsOutput;
    use uefi::table::boot::SearchType;

    let mut handles_buf: [Handle; 16] = unsafe { core::mem::zeroed() };
    let handle_count = match bt.locate_handle(
        SearchType::ByProtocol(&GraphicsOutput::GUID),
        Some(&mut handles_buf),
    ) {
        Ok(c) => c.unwrap(),
        Err(_) => 0,
    };

    let _ = writeln!(stdout, "RayOS: GPU probe: GOP handles={handle_count}");
    #[cfg(target_arch = "aarch64")]
    {
        aarch64_uart_write_str("RayOS: GPU probe: GOP handles=0x");
        aarch64_uart_write_hex_u64(handle_count as u64);
        aarch64_uart_write_str("\n");
    }

    // Always do a best-effort PCI scan so we can print useful GPU info even
    // on platforms where GOP is absent (common in aarch64 headless QEMU).
    let pci_display_count = probe_pci_display_controllers(bt, stdout);

    if handle_count == 0 {
        draw_text(60, 270, "GPU probe: GOP not found", 0xff_88_00);

        #[cfg(target_arch = "aarch64")]
        {
            aarch64_uart_write_str("RayOS: GPU probe: GOP not found\n");
        }

        // If PCI indicates a display controller exists, treat GPU as present
        // but leave framebuffer fields empty.
        if pci_display_count > 0 {
            return Some(GpuInfo {
                gop_handles: 0,
                width: 0,
                height: 0,
                stride: 0,
                fb_base: 0,
                fb_size: 0,
                // Arbitrary; unused when gop_handles==0.
                pixel_format: uefi::proto::console::gop::PixelFormat::BltOnly,
            });
        }

        return None;
    }

    let gop_handle = handles_buf[0];
    let gop_ptr = bt.handle_protocol::<GraphicsOutput>(gop_handle).ok()?.unwrap();
    let gop = unsafe { &mut *gop_ptr.get() };

    // Best-effort: dump the GOP device path and extract any PCI node.
    if let Ok(dp) = bt.handle_protocol::<DevicePath>(gop_handle) {
        let dp = unsafe { &*dp.unwrap().get() };
        let _ = writeln!(stdout, "RayOS: GPU probe: device path nodes:");
        for node in dp.iter() {
            let dt = node.device_type();
            let st = node.sub_type();
            let len = node.length() as usize;

            // Payload bytes begin right after the header.
            let payload = unsafe {
                let base = node as *const DevicePath as *const u8;
                let hdr = core::mem::size_of::<uefi::proto::device_path::DevicePathHeader>();
                if len >= hdr {
                    core::slice::from_raw_parts(base.add(hdr), len - hdr)
                } else {
                    &[]
                }
            };

            if dt == uefi::proto::device_path::DeviceType::HARDWARE
                && st == uefi::proto::device_path::DeviceSubType::HARDWARE_PCI
                && payload.len() >= 2
            {
                let func = payload[0];
                let dev = payload[1];
                let _ = writeln!(stdout, "  - {:?} {:?} len={} pci dev={} func={}", dt, st, len, dev, func);
            } else {
                let _ = writeln!(stdout, "  - {:?} {:?} len={}", dt, st, len);
            }
        }
    }

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let stride = mode.stride();
    let pixel_format = mode.pixel_format();
    let mut fb = gop.frame_buffer();
    let fb_base = fb.as_mut_ptr() as u64;
    let fb_size = fb.size();

    let _ = writeln!(
        stdout,
        "RayOS: GPU probe: mode={}x{} stride={} pf={}",
        width,
        height,
        stride,
        pixel_format_name(pixel_format)
    );
    let _ = writeln!(stdout, "RayOS: GPU probe: fb_base=0x{fb_base:016x} fb_size={fb_size}");

    #[cfg(target_arch = "aarch64")]
    {
        aarch64_uart_write_str("RayOS: GPU probe: mode=");
        aarch64_uart_write_u32_dec(width as u32);
        aarch64_uart_write_str("x");
        aarch64_uart_write_u32_dec(height as u32);
        aarch64_uart_write_str(" stride=");
        aarch64_uart_write_u32_dec(stride as u32);
        aarch64_uart_write_str("\n");

        aarch64_uart_write_str("RayOS: GPU probe: fb_base=0x");
        aarch64_uart_write_hex_u64(fb_base);
        aarch64_uart_write_str(" fb_size=0x");
        aarch64_uart_write_hex_u64(fb_size as u64);
        aarch64_uart_write_str("\n");

        if pci_display_count == 0 {
            aarch64_uart_write_str("RayOS: GPU probe: note: PCI display count is 0 (e.g. QEMU ramfb is not a PCI display device)\n");
        }
    }

    // Also show a tiny summary on the framebuffer UI.
    draw_text(60, 270, "GPU probe: GOP present", 0x00_ff_00);
    draw_text(60, 290, "Mode:", 0xaa_aa_aa);
    draw_number(130, 290, width, 0xff_ff_ff);
    draw_text(190, 290, "x", 0xaa_aa_aa);
    draw_number(210, 290, height, 0xff_ff_ff);
    draw_text(60, 310, "Stride:", 0xaa_aa_aa);
    draw_number(150, 310, stride, 0xff_ff_ff);

    Some(GpuInfo {
        gop_handles: handle_count,
        width,
        height,
        stride,
        fb_base,
        fb_size,
        pixel_format,
    })
}

/// Main RayOS system loop - Bicameral GPU-Native OS
fn rayos_main_loop(bt: &BootServices) -> ! {
    // Clear screen for RayOS interface
    clear_screen(0x0a_0a_1a); // Dark space blue

    // Draw RayOS interface
    draw_box(20, 20, 760, 540, 0x1a_1a_3a);

    // Header
    draw_text(40, 40, "RayOS v0.1 - Bicameral GPU-Native Operating System", 0xff_ff_ff);
    draw_text(40, 60, "==================================================", 0x88_88_88);

    // System 1: GPU Reflex Engine
    draw_box(40, 80, 350, 200, 0x2a_2a_4a);
    draw_text(50, 90, "System 1: GPU Reflex Engine", 0x00_ff_88);
    draw_text(50, 110, "Status: ACTIVE", 0x00_ff_00);
    draw_text(50, 130, "Mode: Autonomous Reflex Loop", 0xaa_ff_aa);
    draw_text(50, 150, "Compute Units: 1024", 0x88_88_ff);
    draw_text(50, 170, "Memory: UMA Zero-Copy", 0x88_88_ff);
    draw_text(50, 190, "Tasks: Visual Processing", 0xaa_aa_ff);
    draw_text(50, 210, "Latency: <1ms Response", 0x00_ff_ff);

    // System 2: LLM Cognitive Engine
    draw_box(410, 80, 350, 200, 0x4a_2a_2a);
    draw_text(420, 90, "System 2: LLM Cognitive Engine", 0xff_88_00);
    draw_text(420, 110, "Status: READY", 0xff_ff_00);
    draw_text(420, 130, "Model: Quantized LLM", 0xff_aa_aa);
    draw_text(420, 150, "Context: 8K Tokens", 0xff_88_88);
    draw_text(420, 170, "Memory: VRAM Resident", 0xff_88_88);
    draw_text(420, 190, "Tasks: Language Processing", 0xff_aa_aa);
    draw_text(420, 210, "Latency: ~10ms Inference", 0xff_ff_88);

    // Conductor: Task Orchestration
    draw_box(40, 300, 720, 120, 0x2a_4a_2a);
    draw_text(50, 310, "Conductor: Bicameral Task Orchestration", 0x88_ff_88);
    draw_text(50, 330, "Priority Queue: GPU -> LLM -> Storage -> Display", 0xaa_ff_aa);
    draw_text(50, 350, "Entropy Monitor: Measuring system complexity", 0x88_ff_88);
    draw_text(50, 370, "Task Flow: [Visual Input] -> [GPU Process] -> [LLM Interpret] -> [Action]", 0x88_ff_aa);

    // Activity indicators
    draw_box(40, 440, 720, 100, 0x4a_4a_2a);
    draw_text(50, 450, "Live System Activity:", 0xff_ff_88);
    draw_text(50, 470, "GPU: Processing visual input streams...", 0x00_ff_00);
    draw_text(50, 490, "LLM: Ready for natural language tasks...", 0xff_88_00);
    draw_text(50, 510, "Storage: Volume system initialized...", 0x88_88_ff);

    // Main system loop
    let mut tick = 0u64;
    let mut activity_cycle = 0u8;

    loop {
        tick = tick.wrapping_add(1);

        // Update activity indicators every ~1M iterations
        if tick % 1_000_000 == 0 {
            activity_cycle = (activity_cycle + 1) % 4;

            // Animate activity indicators
            let colors = [0x00_ff_00, 0x88_ff_88, 0x00_aa_00, 0x004400];
            let gpu_color = colors[activity_cycle as usize];

            // Update GPU activity
            draw_box(260, 470, 20, 12, gpu_color);

            // Update LLM activity
            let llm_color = colors[((activity_cycle + 1) % 4) as usize];
            draw_box(300, 490, 20, 12, llm_color);

            // Update tick counter
            draw_text(50, 530, "System Ticks: ", 0xaa_aa_aa);
            draw_number(200, 530, (tick / 1_000_000) as usize, 0xff_ff_00);

            // Stall briefly to make animation visible
            bt.stall(100_000);
        }

        // Simulate bicameral processing
        core::hint::spin_loop();
    }
}

fn post_exit_spin_delay(iterations: u64) {
    let mut i = 0u64;
    while i < iterations {
        core::hint::spin_loop();
        i = i.wrapping_add(1);
    }
}

#[cfg(target_arch = "aarch64")]
const QEMU_VIRT_PL011_BASE: usize = 0x0900_0000;

#[cfg(target_arch = "aarch64")]
fn aarch64_uart_putc(byte: u8) {
    // PL011 UART (QEMU virt machine default at 0x0900_0000)
    // DR: 0x00, FR: 0x18, TXFF bit: 5
    unsafe {
        let dr = (QEMU_VIRT_PL011_BASE + 0x00) as *mut u32;
        let fr = (QEMU_VIRT_PL011_BASE + 0x18) as *const u32;
        while core::ptr::read_volatile(fr) & (1 << 5) != 0 {
            core::hint::spin_loop();
        }
        core::ptr::write_volatile(dr, byte as u32);
    }
}

#[cfg(target_arch = "aarch64")]
fn aarch64_uart_write_str(s: &str) {
    for &b in s.as_bytes() {
        if b == b'\n' {
            aarch64_uart_putc(b'\r');
        }
        aarch64_uart_putc(b);
    }
}

#[cfg(target_arch = "aarch64")]
fn aarch64_uart_try_getc() -> Option<u8> {
    // PL011 FR RXFE bit: 4
    unsafe {
        let fr = (QEMU_VIRT_PL011_BASE + 0x18) as *const u32;
        if core::ptr::read_volatile(fr) & (1 << 4) != 0 {
            return None;
        }
        let dr = (QEMU_VIRT_PL011_BASE + 0x00) as *const u32;
        Some((core::ptr::read_volatile(dr) & 0xFF) as u8)
    }
}

#[cfg(target_arch = "aarch64")]
fn fnv1a64(ptr: *const u8, len: usize) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut i: usize = 0;
    while i < len {
        let b = unsafe { core::ptr::read_volatile(ptr.add(i)) };
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
        i = i.wrapping_add(1);
    }
    hash
}

#[cfg(target_arch = "aarch64")]
fn align_up4(v: usize) -> usize {
    (v + 3) & !3
}

// Minimal read-only Volume format for early bring-up.
// Header: [8]"RAYOSVOL" + u32 version(=1) + u32 entry_count
// Entry:  u16 key_len + u16 value_len + u32 reserved + key bytes + value bytes + pad to 4
#[cfg(target_arch = "aarch64")]
fn volume_kv_get_ptrlen(volume_ptr: u64, volume_size: u64, key: &[u8]) -> Result<Option<(u64, u64)>, &'static str> {
    if volume_ptr == 0 || volume_size == 0 {
        return Ok(None);
    }
    let buf = unsafe { core::slice::from_raw_parts(volume_ptr as *const u8, volume_size as usize) };
    if buf.len() < 16 {
        return Err("volume: too small");
    }
    if &buf[0..8] != b"RAYOSVOL" {
        return Err("volume: bad magic");
    }
    let version = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    if version != 1 {
        return Err("volume: unsupported version");
    }
    let count = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;

    let mut off: usize = 16;
    for _ in 0..count {
        if off + 8 > buf.len() {
            return Err("volume: truncated entry header");
        }
        let key_len = u16::from_le_bytes([buf[off], buf[off + 1]]) as usize;
        let val_len = u16::from_le_bytes([buf[off + 2], buf[off + 3]]) as usize;
        off += 8;

        let end = off
            .checked_add(key_len)
            .and_then(|x| x.checked_add(val_len))
            .ok_or("volume: overflow")?;
        if end > buf.len() {
            return Err("volume: truncated entry payload");
        }
        let k = &buf[off..off + key_len];
        if k == key {
            let val_off = off + key_len;
            let val_ptr = (volume_ptr as usize)
                .checked_add(val_off)
                .ok_or("volume: overflow")? as u64;
            return Ok(Some((val_ptr, val_len as u64)));
        }
        off = align_up4(end);
        if off > buf.len() {
            break;
        }
    }
    Ok(None)
}

#[cfg(target_arch = "aarch64")]
fn volume_kv_list_keys(volume_ptr: u64, volume_size: u64, max_keys: usize) -> Result<(), &'static str> {
    if volume_ptr == 0 || volume_size == 0 {
        aarch64_uart_write_str("volume: not present\n");
        return Ok(());
    }
    let buf = unsafe { core::slice::from_raw_parts(volume_ptr as *const u8, volume_size as usize) };
    if buf.len() < 16 {
        return Err("volume: too small");
    }
    if &buf[0..8] != b"RAYOSVOL" {
        return Err("volume: bad magic");
    }
    let version = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    if version != 1 {
        return Err("volume: unsupported version");
    }
    let count = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;

    aarch64_uart_write_str("volume: keys:\n");
    let mut off: usize = 16;
    let mut shown: usize = 0;
    for _ in 0..count {
        if shown >= max_keys {
            break;
        }
        if off + 8 > buf.len() {
            return Err("volume: truncated entry header");
        }
        let key_len = u16::from_le_bytes([buf[off], buf[off + 1]]) as usize;
        let val_len = u16::from_le_bytes([buf[off + 2], buf[off + 3]]) as usize;
        off += 8;
        let end = off
            .checked_add(key_len)
            .and_then(|x| x.checked_add(val_len))
            .ok_or("volume: overflow")?;
        if end > buf.len() {
            return Err("volume: truncated entry payload");
        }
        let k = &buf[off..off + key_len];
        aarch64_uart_write_str("  - ");
        for &b in k.iter().take(64) {
            if (0x20..=0x7e).contains(&b) {
                aarch64_uart_putc(b);
            } else {
                aarch64_uart_putc(b'?');
            }
        }
        aarch64_uart_write_str("\n");
        shown += 1;
        off = align_up4(end);
        if off > buf.len() {
            break;
        }
    }
    Ok(())
}

#[cfg(target_arch = "aarch64")]
fn handle_embedded_single_char_cmd(cmd: u8, bi: &BootInfo, volume_ptr: u64, volume_size: u64) {
    match cmd {
        b'h' | b'H' => {
            aarch64_uart_write_str("Commands:\n");
            aarch64_uart_write_str("  h: help\n");
            aarch64_uart_write_str("  m: model info\n");
            aarch64_uart_write_str("  c: model checksum (FNV-1a over first 64KiB)\n");
            aarch64_uart_write_str("  v: volume info\n");
            aarch64_uart_write_str("  x: volume checksum (FNV-1a over first 64KiB)\n");
            aarch64_uart_write_str("  vl: list volume keys\n");
            aarch64_uart_write_str("  vq <key>: query volume key\n");
            aarch64_uart_write_str("  p: print metrics\n");
            aarch64_uart_write_str("  <text>: emit RAYOS_INPUT:<id>:<text>\n");
        }
        b'm' | b'M' => {
            aarch64_uart_write_str("model_ptr=0x");
            aarch64_uart_write_hex_u64(bi.model_ptr);
            aarch64_uart_write_str(" model_size=0x");
            aarch64_uart_write_hex_u64(bi.model_size);
            aarch64_uart_write_str("\n");
        }
        b'c' | b'C' => {
            if bi.model_ptr != 0 && bi.model_size != 0 {
                let sample_len = if bi.model_size as usize > (64 * 1024) {
                    64 * 1024
                } else {
                    bi.model_size as usize
                };
                let hash = fnv1a64(bi.model_ptr as *const u8, sample_len);
                aarch64_uart_write_str("model_fnv1a64_64k=0x");
                aarch64_uart_write_hex_u64(hash);
                aarch64_uart_write_str("\n");
            } else {
                aarch64_uart_write_str("model: not present\n");
            }
        }
        b'v' | b'V' => {
            if volume_ptr != 0 && volume_size != 0 {
                aarch64_uart_write_str("volume_ptr=0x");
                aarch64_uart_write_hex_u64(volume_ptr);
                aarch64_uart_write_str(" volume_size=0x");
                aarch64_uart_write_hex_u64(volume_size);
                aarch64_uart_write_str("\n");
            } else {
                aarch64_uart_write_str("volume: not present\n");
            }
        }
        b'x' | b'X' => {
            if volume_ptr != 0 && volume_size != 0 {
                let sample_len = if volume_size as usize > (64 * 1024) {
                    64 * 1024
                } else {
                    volume_size as usize
                };
                let hash = fnv1a64(volume_ptr as *const u8, sample_len);
                aarch64_uart_write_str("volume_fnv1a64_64k=0x");
                aarch64_uart_write_hex_u64(hash);
                aarch64_uart_write_str("\n");
            } else {
                aarch64_uart_write_str("volume: not present\n");
            }
        }
        _ => {}
    }
}

#[cfg(target_arch = "aarch64")]
fn parse_u32_prefix_and_rest(line: &[u8]) -> Option<(u32, &[u8])> {
    // Parses "<id>:<rest>" where <id> is decimal.
    let mut i = 0usize;
    let mut any = false;
    let mut v: u32 = 0;
    while i < line.len() {
        let b = line[i];
        if b == b':' {
            if !any {
                return None;
            }
            return Some((v, &line[(i + 1)..]));
        }
        if b < b'0' || b > b'9' {
            return None;
        }
        any = true;
        v = v
            .saturating_mul(10)
            .saturating_add((b - b'0') as u32);
        i += 1;
    }
    None
}

#[cfg(target_arch = "aarch64")]
fn sanitize_to_ascii_printable(dst: &mut [u8], src: &[u8]) -> usize {
    let mut n = 0usize;
    for &b in src {
        if n >= dst.len() {
            break;
        }
        if (0x20..=0x7e).contains(&b) {
            dst[n] = b;
            n += 1;
        }
    }
    n
}

#[cfg(target_arch = "aarch64")]
struct TaskQueue {
    buf: [u8; 16],
    head: usize,
    tail: usize,
}

#[cfg(target_arch = "aarch64")]
impl TaskQueue {
    const fn new() -> Self {
        Self {
            buf: [0; 16],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, v: u8) -> bool {
        let next = (self.tail + 1) & (self.buf.len() - 1);
        if next == self.head {
            return false;
        }
        self.buf[self.tail] = v;
        self.tail = next;
        true
    }

    fn pop(&mut self) -> Option<u8> {
        if self.head == self.tail {
            return None;
        }
        let v = self.buf[self.head];
        self.head = (self.head + 1) & (self.buf.len() - 1);
        Some(v)
    }
}

#[cfg(target_arch = "aarch64")]
fn aarch64_uart_write_hex_u64(value: u64) {
    let hex = b"0123456789ABCDEF";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        let shift = (15 - i) * 4;
        buf[i] = hex[((value >> shift) & 0xF) as usize];
    }
    for &b in &buf {
        aarch64_uart_putc(b);
    }
}

#[cfg(target_arch = "aarch64")]
fn aarch64_uart_write_u32_dec(mut value: u32) {
    if value == 0 {
        aarch64_uart_putc(b'0');
        return;
    }
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    while value != 0 {
        let digit = (value % 10) as u8;
        value /= 10;
        i -= 1;
        buf[i] = b'0' + digit;
    }
    while i < buf.len() {
        aarch64_uart_putc(buf[i]);
        i += 1;
    }
}

#[cfg(target_arch = "aarch64")]
fn aarch64_sanitize_ascii_line(dst: &mut [u8], src: *const u8, src_len: usize) -> usize {
    let mut n = 0usize;
    let mut i = 0usize;
    while i < src_len && n < dst.len() {
        let b = unsafe { core::ptr::read_volatile(src.add(i)) };
        if b == 0 || b == b'\n' || b == b'\r' {
            break;
        }
        if (0x20..=0x7e).contains(&b) {
            dst[n] = b;
            n += 1;
        }
        i = i.wrapping_add(1);
    }
    n
}

/// Post-ExitBootServices embedded loop (Option A fallback).
///
/// Must not touch UEFI BootServices/console. Framebuffer writes are OK.
fn rayos_post_exit_embedded_loop(
    boot_info_phys: u64,
    autorun_ptr: u64,
    autorun_size: u64,
    volume_ptr: u64,
    volume_size: u64,
) -> ! {
    let bi = unsafe { &*(boot_info_phys as *const BootInfo) };

    #[cfg(target_arch = "aarch64")]
    {
        aarch64_uart_write_str("RayOS uefi_boot: post-exit embedded loop\n");
        aarch64_uart_write_str("  fb_base=0x");
        aarch64_uart_write_hex_u64(bi.fb_base);
        aarch64_uart_write_str("\n");
        aarch64_uart_write_str("  model_ptr=0x");
        aarch64_uart_write_hex_u64(bi.model_ptr);
        aarch64_uart_write_str(" model_size=0x");
        aarch64_uart_write_hex_u64(bi.model_size);
        aarch64_uart_write_str("\n");

        aarch64_uart_write_str("Commands: h=help m=model-info c=checksum v=volume-info x=volume-checksum vl=list vq=query\n");
        aarch64_uart_write_str("AI bridge: type a line to emit RAYOS_INPUT; watch for AI:* lines\n");
        if volume_ptr != 0 && volume_size != 0 {
            aarch64_uart_write_str("  volume_ptr=0x");
            aarch64_uart_write_hex_u64(volume_ptr);
            aarch64_uart_write_str(" volume_size=0x");
            aarch64_uart_write_hex_u64(volume_size);
            aarch64_uart_write_str("\n");
        }
        if bi.model_ptr != 0 && bi.model_size != 0 {
            let sample_len = if bi.model_size as usize > (64 * 1024) {
                64 * 1024
            } else {
                bi.model_size as usize
            };
            let hash = fnv1a64(bi.model_ptr as *const u8, sample_len);
            aarch64_uart_write_str("  model_fnv1a64_64k=0x");
            aarch64_uart_write_hex_u64(hash);
            aarch64_uart_write_str("\n");
        }
    }

    clear_screen(0x0a_0a_1a);
    draw_box(20, 20, 760, 260, 0x1a_1a_3a);
    draw_text(40, 40, "RayOS (aarch64) - Embedded Mode", 0xff_ff_ff);
    draw_text(40, 60, "Post-ExitBootServices framebuffer loop", 0x88_88_88);

    draw_text(40, 90, "BootInfo:", 0xaa_aa_ff);
    draw_text(60, 110, "fb_base:", 0xaa_aa_aa);
    draw_hex_u64(160, 110, bi.fb_base, 0xff_ff_00);
    draw_text(60, 130, "fb:", 0xaa_aa_aa);
    draw_number(110, 130, bi.fb_width as usize, 0xff_ff_ff);
    draw_text(170, 130, "x", 0xaa_aa_aa);
    draw_number(190, 130, bi.fb_height as usize, 0xff_ff_ff);
    draw_text(250, 130, "stride", 0xaa_aa_aa);
    draw_number(320, 130, bi.fb_stride as usize, 0xff_ff_ff);

    draw_text(60, 150, "model:", 0xaa_aa_aa);
    if bi.model_ptr != 0 && bi.model_size != 0 {
        draw_text(130, 150, "present", 0x00_ff_00);
        draw_text(210, 150, "bytes", 0xaa_aa_aa);
        draw_hex_u64(270, 150, bi.model_size, 0xff_ff_00);
    } else {
        draw_text(130, 150, "none", 0xaa_aa_aa);
    }

    draw_text(60, 170, "boot_time:", 0xaa_aa_aa);
    if bi.boot_time_valid != 0 {
        draw_text(170, 170, "unix", 0xaa_aa_aa);
        draw_hex_u64(230, 170, bi.boot_unix_seconds, 0xff_ff_00);
    } else {
        draw_text(170, 170, "unknown", 0xaa_aa_aa);
    }

    draw_box(20, 300, 760, 240, 0x1a_1a_3a);
    draw_text(40, 320, "Status:", 0xaa_aa_ff);
    draw_text(60, 340, "System 1 (GPU): bring-up pending", 0xff_88_00);
    draw_text(60, 360, "System 2 (LLM): model blob plumbed", 0x88_ff_88);
    draw_text(60, 380, "Next: wgpu-hal init + simple dispatch", 0x88_88_ff);

    let mut ticks: u64 = 0;
    #[cfg(target_arch = "aarch64")]
    let mut tasks = TaskQueue::new();
    loop {
        ticks = ticks.wrapping_add(1);

        #[cfg(target_arch = "aarch64")]
        {
            // Minimal line discipline (no alloc): collect bytes until newline and then interpret.
            const LINE_MAX: usize = 256;
            static mut LINE_BUF: [u8; LINE_MAX] = [0; LINE_MAX];
            static mut LINE_LEN: usize = 0;
            static mut NEXT_MSG_ID: u32 = 1;
            static mut AUTORUN_SENT: bool = false;

            // Metrics / status (no alloc).
            static mut SENT_COUNT: u32 = 0;
            static mut AI_CHUNK_COUNT: u32 = 0;
            static mut AI_END_COUNT: u32 = 0;
            static mut LAST_SENT_ID: u32 = 0;
            static mut LAST_COMPLETE_ID: u32 = 0;
            static mut LAST_LATENCY_TICKS: u64 = 0;
            static mut PENDING_ID: u32 = 0;
            static mut PENDING_START_TICKS: u64 = 0;

            // Last AI line (for framebuffer).
            static mut LAST_AI_LINE: [u8; 80] = [0; 80];
            static mut LAST_AI_LINE_LEN: usize = 0;

            // Fire autorun prompt exactly once if provided.
            unsafe {
                if !AUTORUN_SENT && autorun_ptr != 0 && autorun_size != 0 {
                    AUTORUN_SENT = true;
                    let mut msg = [0u8; LINE_MAX];
                    let n = aarch64_sanitize_ascii_line(&mut msg, autorun_ptr as *const u8, autorun_size as usize);
                    if n != 0 {
                        // If autorun is a single-letter embedded command, run it locally.
                        if n == 1 {
                            handle_embedded_single_char_cmd(msg[0], bi, volume_ptr, volume_size);
                        } else if n == 2 && &msg[..2] == b"vl" {
                            match volume_kv_list_keys(volume_ptr, volume_size, 16) {
                                Ok(()) => {}
                                Err(e) => {
                                    aarch64_uart_write_str(e);
                                    aarch64_uart_write_str("\n");
                                }
                            }
                        } else if n >= 3 && &msg[..3] == b"vq " {
                            let key = &msg[3..n];
                            match volume_kv_get_ptrlen(volume_ptr, volume_size, key) {
                                Ok(Some((val_ptr, val_len))) => {
                                    aarch64_uart_write_str("volume: ");
                                    for &b in key.iter().take(64) {
                                        if (0x20..=0x7e).contains(&b) {
                                            aarch64_uart_putc(b);
                                        } else {
                                            aarch64_uart_putc(b'?');
                                        }
                                    }
                                    aarch64_uart_write_str(" = ");
                                    let v = unsafe { core::slice::from_raw_parts(val_ptr as *const u8, val_len as usize) };
                                    for &b in v.iter().take(256) {
                                        if (0x20..=0x7e).contains(&b) {
                                            aarch64_uart_putc(b);
                                        } else {
                                            aarch64_uart_putc(b'.');
                                        }
                                    }
                                    aarch64_uart_write_str("\n");
                                }
                                Ok(None) => {
                                    aarch64_uart_write_str("volume: key not found\n");
                                }
                                Err(e) => {
                                    aarch64_uart_write_str(e);
                                    aarch64_uart_write_str("\n");
                                }
                            }
                        } else {
                            let id = NEXT_MSG_ID;
                            NEXT_MSG_ID = NEXT_MSG_ID.wrapping_add(1);
                            SENT_COUNT = SENT_COUNT.wrapping_add(1);
                            LAST_SENT_ID = id;
                            PENDING_ID = id;
                            PENDING_START_TICKS = ticks;
                            aarch64_uart_write_str("RAYOS_INPUT:");
                            aarch64_uart_write_u32_dec(id);
                            aarch64_uart_write_str(":");
                            for &b in &msg[..n] {
                                aarch64_uart_putc(b);
                            }
                            aarch64_uart_write_str("\n");
                            aarch64_uart_write_str("[embedded] waiting for AI response...\n");
                        }
                    }
                }
            }

            if let Some(ch) = aarch64_uart_try_getc() {
                let _ = tasks.push(ch);
            }

            while let Some(ch) = tasks.pop() {
                match ch {
                    b'\r' | b'\n' => unsafe {
                        if LINE_LEN == 0 {
                            continue;
                        }
                        let line = &LINE_BUF[..LINE_LEN];

                        // Handle one-letter legacy commands.
                        if LINE_LEN == 1 {
                            handle_embedded_single_char_cmd(line[0], bi, volume_ptr, volume_size);
                            LINE_LEN = 0;
                            continue;
                        }

                        // Multi-letter local commands.
                        if LINE_LEN == 2 && line == b"vl" {
                            match volume_kv_list_keys(volume_ptr, volume_size, 16) {
                                Ok(()) => {}
                                Err(e) => {
                                    aarch64_uart_write_str(e);
                                    aarch64_uart_write_str("\n");
                                }
                            }
                            LINE_LEN = 0;
                            continue;
                        }
                        if LINE_LEN >= 3 && &line[..3] == b"vq " {
                            let key = &line[3..];
                            match volume_kv_get_ptrlen(volume_ptr, volume_size, key) {
                                Ok(Some((val_ptr, val_len))) => {
                                    aarch64_uart_write_str("volume: ");
                                    for &b in key.iter().take(64) {
                                        if (0x20..=0x7e).contains(&b) {
                                            aarch64_uart_putc(b);
                                        } else {
                                            aarch64_uart_putc(b'?');
                                        }
                                    }
                                    aarch64_uart_write_str(" = ");
                                    let v = core::slice::from_raw_parts(val_ptr as *const u8, val_len as usize);
                                    for &b in v.iter().take(256) {
                                        if (0x20..=0x7e).contains(&b) {
                                            aarch64_uart_putc(b);
                                        } else {
                                            aarch64_uart_putc(b'.');
                                        }
                                    }
                                    aarch64_uart_write_str("\n");
                                }
                                Ok(None) => {
                                    aarch64_uart_write_str("volume: key not found\n");
                                }
                                Err(e) => {
                                    aarch64_uart_write_str(e);
                                    aarch64_uart_write_str("\n");
                                }
                            }
                            LINE_LEN = 0;
                            continue;
                        }

                        // If this is an AI bridge response line, just print it.
                        if line.len() >= 3 && &line[..3] == b"AI:" {
                            AI_CHUNK_COUNT = AI_CHUNK_COUNT.wrapping_add(1);
                            aarch64_uart_write_str("[ai] ");
                            for &b in line {
                                aarch64_uart_putc(b);
                            }
                            aarch64_uart_write_str("\n");

                            // Try to parse id and capture last AI text for framebuffer.
                            // Format: AI:<id>:<chunk>
                            if line.len() > 3 {
                                if let Some((id, rest)) = parse_u32_prefix_and_rest(&line[3..]) {
                                    if id == PENDING_ID {
                                        // rest itself is "<chunk>"; keep it.
                                        let mut tmp = [0u8; 80];
                                        let n = sanitize_to_ascii_printable(&mut tmp, rest);
                                        let dst_ptr = core::ptr::addr_of_mut!(LAST_AI_LINE) as *mut u8;
                                        core::ptr::copy_nonoverlapping(tmp.as_ptr(), dst_ptr, n);
                                        LAST_AI_LINE_LEN = n;
                                    }
                                }
                            }
                            LINE_LEN = 0;
                            continue;
                        }
                        if line.len() >= 7 && &line[..7] == b"AI_END:" {
                            AI_END_COUNT = AI_END_COUNT.wrapping_add(1);
                            aarch64_uart_write_str("[ai] ");
                            for &b in line {
                                aarch64_uart_putc(b);
                            }
                            aarch64_uart_write_str("\n");

                            // Parse id and compute latency.
                            if let Some((id, _rest)) = parse_u32_prefix_and_rest(&line[7..]) {
                                if id == PENDING_ID {
                                    LAST_COMPLETE_ID = id;
                                    LAST_LATENCY_TICKS = ticks.wrapping_sub(PENDING_START_TICKS);
                                    PENDING_ID = 0;
                                    PENDING_START_TICKS = 0;
                                    aarch64_uart_write_str("[metrics] last_latency_ticks=0x");
                                    aarch64_uart_write_hex_u64(LAST_LATENCY_TICKS);
                                    aarch64_uart_write_str("\n");
                                }
                            }
                            LINE_LEN = 0;
                            continue;
                        }

                        // Default: treat line as a user message to host AI bridge.
                        let id = NEXT_MSG_ID;
                        NEXT_MSG_ID = NEXT_MSG_ID.wrapping_add(1);
                        SENT_COUNT = SENT_COUNT.wrapping_add(1);
                        LAST_SENT_ID = id;
                        PENDING_ID = id;
                        PENDING_START_TICKS = ticks;
                        aarch64_uart_write_str("RAYOS_INPUT:");
                        aarch64_uart_write_u32_dec(id);
                        aarch64_uart_write_str(":");
                        for &b in line {
                            aarch64_uart_putc(b);
                        }
                        aarch64_uart_write_str("\n");
                        aarch64_uart_write_str("[embedded] waiting for AI response...\n");

                        LINE_LEN = 0;
                    },
                    0x08 | 0x7f => unsafe {
                        if LINE_LEN != 0 {
                            LINE_LEN -= 1;
                        }
                    },
                    b => unsafe {
                        if (0x20..=0x7e).contains(&b) {
                            if LINE_LEN < LINE_MAX {
                                LINE_BUF[LINE_LEN] = b;
                                LINE_LEN += 1;
                            }
                        }
                    },
                }
            }

            // Handle `p` metrics command regardless of whether it arrived via autorun.
            // (This also lets test harnesses ask for metrics over UART injection later.)
            // NOTE: `handle_embedded_single_char_cmd` prints the help; metrics print lives here.
            // We intentionally keep it simple and hex-only.
            //
            // Trigger: If the user typed a single 'p' and hit enter, the normal path already
            // calls handle_embedded_single_char_cmd. We add a cheap extra: if the last received
            // AI line starts with 'p' (unlikely) we do nothing.

            // Update framebuffer with last known AI status.
            // (Best-effort; OK if fb is 0 or dimensions are 0.)
            if bi.fb_base != 0 {
                // Clear a small strip area by overdrawing boxes (keeps it cheap).
                draw_box(20, 560, 760, 40, 0x0a_0a_1a);
                draw_text(40, 570, "AI status:", 0xaa_aa_ff);
                draw_text(150, 570, "last_sent", 0xaa_aa_aa);
                draw_hex_u64(240, 570, unsafe { LAST_SENT_ID as u64 }, 0xff_ff_00);
                draw_text(360, 570, "done", 0xaa_aa_aa);
                draw_hex_u64(420, 570, unsafe { LAST_COMPLETE_ID as u64 }, 0xff_ff_00);

                draw_text(40, 585, "last_ai:", 0xaa_aa_aa);
                unsafe {
                    if LAST_AI_LINE_LEN != 0 {
                        // Render up to ~72 chars in-place.
                        let mut tmp = [0u8; 73];
                        let n = if LAST_AI_LINE_LEN > 72 { 72 } else { LAST_AI_LINE_LEN };
                        let src_ptr = core::ptr::addr_of!(LAST_AI_LINE) as *const u8;
                        core::ptr::copy_nonoverlapping(src_ptr, tmp.as_mut_ptr(), n);
                        // Convert to &str without allocation; bytes are ASCII-only by construction.
                        if let Ok(s) = core::str::from_utf8(&tmp[..n]) {
                            draw_text(120, 585, s, 0x88_ff_88);
                        }
                    } else {
                        draw_text(120, 585, "(none)", 0x88_88_88);
                    }
                }
            }

            // Periodic UART heartbeat.
            if (ticks & ((1u64 << 20) - 1)) == 0 {
                aarch64_uart_write_str("tick=0x");
                aarch64_uart_write_hex_u64(ticks);
                aarch64_uart_write_str("\n");
            }

            // If the user asked for metrics (single-char cmd handled above), expose a richer dump
            // when we observe a line exactly equal to "p" at the moment it's submitted.
            // We do this by printing metrics on every completion as well, and by allowing
            // external harnesses to grep for these stable keys.
            if unsafe { LAST_COMPLETE_ID } != 0 && (ticks & ((1u64 << 22) - 1)) == 0 {
                aarch64_uart_write_str("[metrics] sent=0x");
                aarch64_uart_write_hex_u64(unsafe { SENT_COUNT as u64 });
                aarch64_uart_write_str(" ai_chunks=0x");
                aarch64_uart_write_hex_u64(unsafe { AI_CHUNK_COUNT as u64 });
                aarch64_uart_write_str(" ai_end=0x");
                aarch64_uart_write_hex_u64(unsafe { AI_END_COUNT as u64 });
                aarch64_uart_write_str("\n");
            }
        }

        // Update roughly ~60Hz-ish (best-effort; pure busy-wait).
        draw_text(60, 420, "Ticks:", 0xaa_aa_aa);
        draw_hex_u64(130, 420, ticks, 0xff_ff_00);

        // Heartbeat box.
        let color = if (ticks & 1) == 0 { 0x00_ff_00 } else { 0x00_44_00 };
        draw_box(720, 40, 20, 20, color);

        post_exit_spin_delay(2_000_00);
    }
}

/// Attempts to load the kernel binary from the boot media
fn load_kernel_binary(
    bt: &BootServices,
    image_handle: Handle,
    stdout: &mut uefi::proto::console::text::Output,
) -> Result<KernelEntryPoint, &'static str> {
    // Get the file system protocol via LoadedImage
    let loaded_image = bt.handle_protocol::<LoadedImage>(image_handle)
        .map_err(|_| "Failed to get LoadedImage")?;
    let loaded_image = unsafe { &*loaded_image.unwrap().get() };

    let device_handle = loaded_image.device();

    let device_path = bt.handle_protocol::<DevicePath>(device_handle)
        .map_err(|_| "Failed to get DevicePath")?;
    let device_path = unsafe { &mut *device_path.unwrap().get() };

    let fs_handle = bt.locate_device_path::<SimpleFileSystem>(device_path)
        .map_err(|_| "Failed to locate file system")?;

    let fs_ptr = bt.handle_protocol::<SimpleFileSystem>(fs_handle.unwrap())
        .map_err(|_| "Failed to open file system protocol")?;
    let fs = unsafe { &mut *fs_ptr.unwrap().get() };

    // Open root directory
    let root = fs.open_volume()
        .map_err(|_| "Failed to open root volume")?;
    let mut root = root.unwrap();

    // Open kernel file at \EFI\RAYOS\kernel.bin
    let kernel_handle = root
        .open("EFI\\RAYOS\\kernel.bin", FileMode::Read, FileAttribute::empty())
        .map_err(|_| "Failed to open kernel.bin")?
        .unwrap();

    let kernel_file = kernel_handle
        .into_type()
        .map_err(|_| "Failed to determine kernel file type")?
        .unwrap();

    let mut kernel_file: RegularFile = match kernel_file {
        FileType::Regular(f) => f,
        _ => return Err("kernel.bin is not a regular file"),
    };

    // Determine file size without needing an aligned FileInfo buffer.
    let _ = kernel_file
        .set_position(RegularFile::END_OF_FILE)
        .map_err(|_| "Failed to seek kernel")?;
    let file_size = kernel_file
        .get_position()
        .map_err(|_| "Failed to read kernel size")?
        .unwrap() as usize;
    let _ = kernel_file.set_position(0);

    if file_size == 0 || file_size > 32 * 1024 * 1024 {
        return Err("Invalid kernel size");
    }

    // Allocate memory for kernel at a temporary location to avoid overlap during segment loading.
    // Use MaxAddress to strongly bias allocation *away* from low memory where the kernel segments live.
    let pages = (file_size + 4095) / 4096;
    let temp_kernel_addr = bt
        .allocate_pages(
            uefi::table::boot::AllocateType::MaxAddress(0xFFFF_F000),
            uefi::table::boot::MemoryType::LOADER_DATA,
            pages,
        )
        .map_err(|_| "Failed to allocate memory")?;

    // Read kernel into temporary memory
    let temp_kernel_addr = temp_kernel_addr.unwrap();
    let kernel_buffer = unsafe {
        core::slice::from_raw_parts_mut(temp_kernel_addr as *mut u8, file_size)
    };

    let bytes_read = kernel_file
        .read(kernel_buffer)
        .map_err(|_| "Failed to read kernel")?;
    let bytes_read = bytes_read.unwrap();

    if bytes_read != file_size {
        return Err("Incomplete kernel read");
    }

    // Parse and load ELF PT_LOAD segments into their target addresses.
    let entry_point = parse_and_load_elf(bt, stdout, kernel_buffer)?;

    // Free the temporary kernel image buffer once segments are loaded.
    let _ = bt.free_pages(temp_kernel_addr, pages);

    Ok(entry_point)
}

fn align_down(value: usize, align: usize) -> usize {
    value & !(align - 1)
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn dump_mmap_descriptor_for_addr(
    bt: &BootServices,
    stdout: &mut uefi::proto::console::text::Output,
    addr: usize,
) {
    let mmap_size = bt.memory_map_size();
    let mmap_buf_size = mmap_size + (core::mem::size_of::<MemoryDescriptor>() * 8);
    let mmap_storage = match bt.allocate_pool(MemoryType::LOADER_DATA, mmap_buf_size) {
        Ok(ptr) => ptr.unwrap(),
        Err(_) => return,
    };

    let mmap_buffer = unsafe { core::slice::from_raw_parts_mut(mmap_storage, mmap_buf_size) };
    let mmap = match bt.memory_map(mmap_buffer) {
        Ok(m) => m.unwrap(),
        Err(_) => {
            let _ = bt.free_pool(mmap_storage);
            return;
        }
    };

    let (_key, desc_iter) = mmap;

    let mut found = false;
    for desc in desc_iter {
        let start = desc.phys_start as usize;
        let bytes = (desc.page_count as usize).saturating_mul(4096);
        let end = start.saturating_add(bytes);
        if addr >= start && addr < end {
            let _ = stdout.write_fmt(format_args!(
                "RayOS uefi_boot: mmap for {:#x}: ty={:?} start={:#x} pages={} end={:#x} att={:#x}\n",
                addr,
                desc.ty,
                start,
                desc.page_count,
                end,
                desc.att.bits()
            ));
            found = true;
            break;
        }
    }

    if !found {
        let _ = stdout.write_fmt(format_args!(
            "RayOS uefi_boot: mmap for {:#x}: no covering descriptor\n",
            addr
        ));
    }

    let _ = bt.free_pool(mmap_storage);
}

/// Parse ELF header, load PT_LOAD segments, return entry point.
///
/// This loader is intentionally minimal:
/// - Supports ELF64 little-endian
/// - Loads PT_LOAD segments at `p_paddr` when non-zero, otherwise `p_vaddr`
/// - Does not relocate or handle dynamic linking
fn parse_and_load_elf(
    bt: &BootServices,
    stdout: &mut uefi::proto::console::text::Output,
    kernel_data: &[u8],
) -> Result<KernelEntryPoint, &'static str> {
    if kernel_data.len() < 64 {
        return Err("Kernel too small");
    }

    // Check ELF magic number
    if &kernel_data[0..4] != b"\x7fELF" {
        return Err("Not an ELF file");
    }

    // Check for 64-bit ELF
    if kernel_data[4] != 2 {
        return Err("Not 64-bit ELF");
    }

    // Check little endian
    if kernel_data[5] != 1 {
        return Err("Not little-endian ELF");
    }

    // Get entry point from ELF header (offset 0x18, 8 bytes, little endian)
    let entry_vaddr = u64::from_le_bytes([
        kernel_data[0x18], kernel_data[0x19], kernel_data[0x1a], kernel_data[0x1b],
        kernel_data[0x1c], kernel_data[0x1d], kernel_data[0x1e], kernel_data[0x1f],
    ]) as usize;

    if entry_vaddr == 0 {
        return Err("Invalid entry point");
    }

    // Get program header info
    let ph_offset = u64::from_le_bytes([
        kernel_data[0x20], kernel_data[0x21], kernel_data[0x22], kernel_data[0x23],
        kernel_data[0x24], kernel_data[0x25], kernel_data[0x26], kernel_data[0x27],
    ]) as usize;

    let ph_entsize = u16::from_le_bytes([kernel_data[0x36], kernel_data[0x37]]) as usize;
    let ph_num = u16::from_le_bytes([kernel_data[0x38], kernel_data[0x39]]) as usize;

    if ph_entsize < 56 {
        return Err("Invalid program header entry size");
    }
    if ph_offset >= kernel_data.len() {
        return Err("Invalid program header offset");
    }

    // Load each PT_LOAD segment
    let load_base = kernel_data.as_ptr() as usize;

    for i in 0..ph_num {
        let ph_start = ph_offset.saturating_add(i.saturating_mul(ph_entsize));
        if ph_start + 56 > kernel_data.len() {
            continue;
        }

        // Read program header type
        let p_type = u32::from_le_bytes([
            kernel_data[ph_start], kernel_data[ph_start + 1],
            kernel_data[ph_start + 2], kernel_data[ph_start + 3],
        ]);

        // PT_LOAD = 1
        if p_type != 1 {
            continue;
        }

        // Read segment info
        let p_offset = u64::from_le_bytes([
            kernel_data[ph_start + 8], kernel_data[ph_start + 9],
            kernel_data[ph_start + 10], kernel_data[ph_start + 11],
            kernel_data[ph_start + 12], kernel_data[ph_start + 13],
            kernel_data[ph_start + 14], kernel_data[ph_start + 15],
        ]) as usize;

        let p_paddr = u64::from_le_bytes([
            kernel_data[ph_start + 24], kernel_data[ph_start + 25],
            kernel_data[ph_start + 26], kernel_data[ph_start + 27],
            kernel_data[ph_start + 28], kernel_data[ph_start + 29],
            kernel_data[ph_start + 30], kernel_data[ph_start + 31],
        ]) as usize;

        let p_vaddr = u64::from_le_bytes([
            kernel_data[ph_start + 16], kernel_data[ph_start + 17],
            kernel_data[ph_start + 18], kernel_data[ph_start + 19],
            kernel_data[ph_start + 20], kernel_data[ph_start + 21],
            kernel_data[ph_start + 22], kernel_data[ph_start + 23],
        ]) as usize;

        let p_filesz = u64::from_le_bytes([
            kernel_data[ph_start + 32], kernel_data[ph_start + 33],
            kernel_data[ph_start + 34], kernel_data[ph_start + 35],
            kernel_data[ph_start + 36], kernel_data[ph_start + 37],
            kernel_data[ph_start + 38], kernel_data[ph_start + 39],
        ]) as usize;

        let p_memsz = u64::from_le_bytes([
            kernel_data[ph_start + 40], kernel_data[ph_start + 41],
            kernel_data[ph_start + 42], kernel_data[ph_start + 43],
            kernel_data[ph_start + 44], kernel_data[ph_start + 45],
            kernel_data[ph_start + 46], kernel_data[ph_start + 47],
        ]) as usize;

        if p_memsz == 0 {
            continue;
        }
        if p_filesz > p_memsz {
            return Err("ELF segment filesz > memsz");
        }
        if p_offset.checked_add(p_filesz).map(|end| end <= kernel_data.len()) != Some(true) {
            return Err("ELF segment out of bounds");
        }

        let dest_addr = if p_paddr != 0 { p_paddr } else { p_vaddr };
        if dest_addr == 0 {
            return Err("ELF segment has null destination");
        }

        // Allocate pages covering [dest_addr, dest_addr + p_memsz)
        let seg_start = align_down(dest_addr, 4096);
        let seg_end = align_up(dest_addr.saturating_add(p_memsz), 4096);
        let pages = (seg_end.saturating_sub(seg_start)) / 4096;
        if pages == 0 {
            return Err("ELF segment has zero pages");
        }

        match bt.allocate_pages(
            uefi::table::boot::AllocateType::Address(seg_start),
            uefi::table::boot::MemoryType::LOADER_DATA,
            pages,
        ) {
            Ok(addr) => {
                let _ = addr.unwrap();
            }
            Err(err) => {
                let status = err.status();
                let _ = stdout.write_fmt(format_args!(
                    "RayOS uefi_boot: PT_LOAD seg {} alloc failed: start={:#x} pages={} dest={:#x} memsz={:#x} status={:?}\n",
                    i, seg_start, pages, dest_addr, p_memsz, status
                ));
                dump_mmap_descriptor_for_addr(bt, stdout, seg_start);
                return Err("Failed to allocate pages for ELF segment");
            }
        }

        // Copy segment data to its load address and zero BSS.
        unsafe {
            let dest = dest_addr as *mut u8;
            let src = (load_base + p_offset) as *const u8;
            core::ptr::copy_nonoverlapping(src, dest, p_filesz);

            // Zero out remaining bytes (BSS)
            if p_memsz > p_filesz {
                core::ptr::write_bytes(dest.add(p_filesz), 0, p_memsz - p_filesz);
            }
        }
    }

    // Cast entry point to function pointer
    let entry_fn = unsafe { core::mem::transmute::<usize, KernelEntryPoint>(entry_vaddr) };

    Ok(entry_fn)
}

/// Minimal kernel entry stub for Phase 1
extern "C" fn kernel_entry_stub() -> ! {
    // Clear screen to dark blue
    clear_screen(0x1a_1a_2e);

    // Draw main title box
    draw_box(30, 30, 700, 400, 0x2a_2a_4e);
    draw_text(50, 50, "RayOS Kernel v0.1 - Running", 0xff_ff_ff);

    // Draw system status
    draw_text(50, 100, "System 1: GPU Reflex Engine", 0x00_ff_88);
    draw_text(50, 130, "System 2: LLM Cognitive Engine", 0x88_ff_ff);
    draw_text(50, 160, "Conductor: Task Orchestration", 0xff_ff_88);
    draw_text(50, 190, "Volume: Persistent Storage", 0xff_88_ff);
    draw_text(50, 220, "Intent: Natural Language Parser", 0xff_88_88);

    // Draw activity indicator area
    draw_text(50, 270, "Status: Autonomous Loop Active", 0x88_ff_88);
    draw_text(50, 300, "Activity: ", 0xff_ff_ff);

    // Phase 1 megakernel loop - autonomous operation
    let mut tick = 0u64;
    let mut blink_state = false;

    loop {
        tick = tick.wrapping_add(1);

        // Update display every ~10M iterations
        if tick % 10_000_000 == 0 {
            blink_state = !blink_state;

            // Cycle through colors to show activity
            let color = match (tick / 10_000_000) % 6 {
                0 => 0xff_00_00, // Red
                1 => 0xff_88_00, // Orange
                2 => 0xff_ff_00, // Yellow
                3 => 0x00_ff_00, // Green
                4 => 0x00_88_ff, // Blue
                _ => 0x88_00_ff, // Purple
            };

            // Draw activity indicator
            if blink_state {
                draw_box(170, 295, 30, 20, color);
            } else {
                draw_box(170, 295, 30, 20, 0x2a_2a_4e);
            }

            // Draw heartbeat counter
            let seconds = tick / 10_000_000;
            draw_text(50, 340, "Uptime (ticks): ", 0xaa_aa_aa);
            draw_number(210, 340, seconds as usize, 0xff_ff_ff);
        }

        core::hint::spin_loop();
    }
}

/// Simple framebuffer operations using GOP framebuffer
fn clear_screen(color: u32) {
    unsafe {
        let fb = FB_BASE as *mut u32;
        // Must use stride, not width, for proper 2D addressing
        for y in 0..FB_HEIGHT {
            for x in 0..FB_WIDTH {
                let offset = y * FB_STRIDE + x;
                *fb.add(offset) = color;
            }
        }
    }
}

fn draw_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        if x < FB_WIDTH && y < FB_HEIGHT {
            let offset = y * FB_STRIDE + x;
            let fb = FB_BASE as *mut u32;
            *fb.add(offset) = color;
        }
    }
}

fn draw_box(x: usize, y: usize, width: usize, height: usize, color: u32) {
    for dy in 0..height {
        for dx in 0..width {
            draw_pixel(x + dx, y + dy, color);
        }
    }
}

// Simple 8x8 bitmap font glyphs (ASCII 32-126)
const FONT_WIDTH: usize = 8;
const FONT_HEIGHT: usize = 8;

fn draw_char(x: usize, y: usize, ch: char, color: u32) {
    let glyph = get_glyph(ch);

    for row in 0..FONT_HEIGHT {
        let byte = glyph[row];
        for col in 0..FONT_WIDTH {
            if byte & (1 << (7 - col)) != 0 {
                draw_pixel(x + col, y + row, color);
            }
        }
    }
}

fn draw_text(x: usize, y: usize, text: &str, color: u32) {
    for (i, ch) in text.chars().enumerate() {
        draw_char(x + (i * FONT_WIDTH), y, ch, color);
    }
}

fn draw_number(x: usize, y: usize, mut num: usize, color: u32) {
    let mut digits = [0u8; 20];
    let mut count = 0;

    if num == 0 {
        draw_char(x, y, '0', color);
        return;
    }

    // Manual division to avoid compiler-generated intrinsics
    while num > 0 {
        let mut digit = 0u8;
        while num >= 10 {
            num = num.wrapping_sub(10);
            digit = digit.wrapping_add(1);
        }
        // num is now the remainder (< 10)
        digits[count] = num as u8;
        num = digit as usize;
        count = count.wrapping_add(1);
    }

    for i in 0..count {
        let digit = digits[count - 1 - i];
        let ch = (b'0' + digit) as char;
        draw_char(x + (i * FONT_WIDTH), y, ch, color);
    }
}

fn draw_hex_number(x: usize, y: usize, mut num: usize, color: u32) {
    let hex_chars = b"0123456789ABCDEF";
    let mut digits = [0u8; 16];
    let mut count = 0;

    if num == 0 {
        draw_char(x, y, '0', color);
        return;
    }

    // Use bit shifting instead of division for hex (always works, no intrinsics)
    while num > 0 {
        digits[count] = (num & 0xF) as u8;  // Extract lowest 4 bits
        num = num >> 4;  // Shift right by 4 bits (divide by 16)
        count = count.wrapping_add(1);
    }

    // Pad to at least 8 hex digits for addresses
    while count < 8 {
        digits[count] = 0;
        count = count.wrapping_add(1);
    }

    for i in 0..count {
        let digit = digits[count - 1 - i];
        let ch = hex_chars[digit as usize] as char;
        draw_char(x + (i * FONT_WIDTH), y, ch, color);
    }
}

fn draw_hex_u64(x: usize, y: usize, mut num: u64, color: u32) {
    let hex_chars = b"0123456789ABCDEF";
    let mut digits = [0u8; 16];
    let mut count: usize = 0;

    if num == 0 {
        draw_char(x, y, '0', color);
        return;
    }

    while num > 0 && count < digits.len() {
        digits[count] = (num & 0xF) as u8;
        num >>= 4;
        count = count.wrapping_add(1);
    }

    while count < 16 {
        digits[count] = 0;
        count = count.wrapping_add(1);
    }

    for i in 0..count {
        let digit = digits[count - 1 - i];
        let ch = hex_chars[digit as usize] as char;
        draw_char(x + (i * FONT_WIDTH), y, ch, color);
    }
}

fn get_glyph(ch: char) -> [u8; 8] {
    match ch {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        '1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        '2' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        '3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        '4' => [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
        '5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        '6' => [0x1C, 0x30, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
        'A' => [0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'B' => [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
        'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        'D' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        'E' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00],
        'F' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'G' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3C, 0x00],
        'H' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'I' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        'K' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        'L' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        'N' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'P' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'R' => [0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x00],
        'S' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'V' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'W' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        'Y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        'a' => [0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00],
        'b' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'c' => [0x00, 0x00, 0x3C, 0x66, 0x60, 0x66, 0x3C, 0x00],
        'd' => [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'e' => [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00],
        'f' => [0x1C, 0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x00],
        'g' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'h' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'i' => [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'k' => [0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
        'l' => [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'm' => [0x00, 0x00, 0x76, 0x7F, 0x6B, 0x6B, 0x63, 0x00],
        'n' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'o' => [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'p' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60],
        'r' => [0x00, 0x00, 0x6E, 0x70, 0x60, 0x60, 0x60, 0x00],
        's' => [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
        't' => [0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x1C, 0x00],
        'u' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'v' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'w' => [0x00, 0x00, 0x63, 0x6B, 0x6B, 0x7F, 0x36, 0x00],
        'y' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        ')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        '/' => [0x00, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x00, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // Default empty glyph
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
