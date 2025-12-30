#![no_std]
#![no_main]

use core::mem;
use uefi::prelude::*;
use uefi::table::boot::{AllocateType, MemoryType, SearchType};
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};

#[entry]
fn main(_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    // 1. Initialize UEFI services for hardware access
    uefi_services::init(&mut system_table).unwrap();
    let bt = system_table.boot_services();

    // Initialize Graphics Output Protocol for display
    let gop_handle = bt.get_handle_for_protocol::<GraphicsOutput>()
        .expect("Failed to get GOP handle");
    let mut gop = bt.open_protocol_exclusive::<GraphicsOutput>(gop_handle)
        .expect("Failed to open Graphics Output Protocol");

    // Get framebuffer info
    let mode_info = gop.current_mode_info();
    let mut framebuffer = gop.frame_buffer();
    let (width, height) = mode_info.resolution();
    let stride = mode_info.stride();

    // Save framebuffer pointer and info for after exit_boot_services
    let fb_base = framebuffer.as_mut_ptr() as usize;
    let fb_size = framebuffer.size();

    // Clear screen to dark blue background
    clear_screen(&mut framebuffer, width, height, stride, 0x001a1a2e);

    // Draw boot banner
    draw_banner(&mut framebuffer, width, height, stride);

    // 2. PILLAR 1.3.2: ZERO-COPY ALLOCATOR INITIALIZATION
    // ray-OS requires mandatory Unified Memory Architecture (UMA) [5].
    // We must reserve a large block of RAM that both CPU and GPU can access.
    let uma_pool_size_pages = 4194304; // 16GB (calculated as size / 4096)
    let uma_base_addr = bt.allocate_pages(
        AllocateType::AnyPages,
        MemoryType::RESERVED, // Reserved so the kernel knows this is for UMA
        uma_pool_size_pages,
    ).expect("Failed to allocate UMA pool for Zero-Copy Allocator");

    // 3. SYSTEM 2: COGNITIVE ENGINE PARTITIONING
    // Reserve VRAM/RAM for the Resident LLM (the Frontal Cortex) [3, 6].
    let llm_partition_pages = 2097152; // 8GB for quantized Llama-3-8B
    let llm_base_addr = bt.allocate_pages(
        AllocateType::AnyPages,
        MemoryType::RUNTIME_SERVICES_DATA, // Persistent for the Cognitive Engine
        llm_partition_pages,
    ).expect("Failed to reserve memory for System 2 (Cognitive Engine)");

    // 4. LOAD PILLAR 1.2.1: THE MEGAKERNEL (SYSTEM 1)
    // The Reflex Engine runs as a Persistent Compute Shader [3, 4].
    // We load the compiled Rust/SPIR-V binary from the EFI System Partition.
    let mut fs = bt.get_image_file_system(_handle)
        .expect("Failed to access EFI file system");
    let mut root = fs.open_volume().expect("Failed to open volume");

    let mut kernel_file = root.open(
        cstr16!("\\EFI\\RAYOS\\kernel.bin"),
        FileMode::Read,
        FileAttribute::empty(),
    ).expect("Kernel binary not found at \\EFI\\RAYOS\\kernel.bin")
    .into_regular_file()
    .expect("Invalid kernel file");

    // Get kernel file size
    use uefi::proto::media::file::FileInfo;
    let mut info_buffer = [0u8; 256];
    let info = kernel_file.get_info::<FileInfo>(&mut info_buffer)
        .expect("Failed to get kernel file info");
    let kernel_size = info.file_size() as usize;
    let kernel_pages = (kernel_size + 4095) / 4096; // Round up to page boundary

    // Allocate buffer and load kernel into memory
    let kernel_buffer_addr = bt.allocate_pages(
        AllocateType::AnyPages,
        MemoryType::LOADER_CODE,
        kernel_pages,
    ).expect("Failed to allocate buffer for kernel");

    let kernel_buffer = unsafe {
        core::slice::from_raw_parts_mut(kernel_buffer_addr as *mut u8, kernel_size)
    };
    let bytes_read = kernel_file.read(kernel_buffer)
        .expect("Failed to read kernel into memory");

    if bytes_read != kernel_size {
        panic!("Incomplete kernel read: {} != {}", bytes_read, kernel_size);
    }

    // Display progress messages BEFORE exiting boot services
    draw_text(&mut framebuffer, width, stride, 100, 220, "Initializing framebuffer graphics...", 0x0050fa7b);
    draw_text(&mut framebuffer, width, stride, 100, 250, "Loading kernel binary...", 0x00ffff00);
    draw_text(&mut framebuffer, width, stride, 100, 280, "Kernel loaded successfully", 0x0050fa7b);
    draw_text(&mut framebuffer, width, stride, 100, 330, "Memory allocated:", 0x00aaaaaa);
    draw_text(&mut framebuffer, width, stride, 120, 360, "- UMA Pool: 16 GB", 0x0088ff88);
    draw_text(&mut framebuffer, width, stride, 120, 390, "- LLM Partition: 8 GB", 0x0088ff88);

    // Stall before exiting boot services so user can see the display
    bt.stall(2_000_000); // 2 seconds

    // 5. PHASE 1: THE PULSE (WATCHDOG BYPASS)
    // The Megakernel is an infinite while(true) loop on the GPU [2-4].
    // To prevent a hardware reset/TDR, we must disable the UEFI watchdog.
    bt.set_watchdog_timer(0, 0x10000, None).expect("Failed to bypass watchdog");

    // 6. HANDOFF TO CONTINUOUS SIMULATION MODEL
    // Exit Boot Services to transition from Von Neumann to ray-OS [4, 7].
    let max_mmap_size = bt.memory_map_size().map_size;
    let mmap_storage = bt.allocate_pool(MemoryType::LOADER_DATA, max_mmap_size)
        .expect("Failed to allocate mmap storage");
    let mmap_buffer = unsafe { core::slice::from_raw_parts_mut(mmap_storage, max_mmap_size) };

    let (_rt, _mmap_iter) = system_table
        .exit_boot_services(_handle, mmap_buffer)
        .expect("Failed to exit boot services and start the Pulse");

    // After exit_boot_services, we can no longer use UEFI boot services
    // Draw one final message directly to framebuffer
    let fb_ptr = fb_base as *mut u8;
    draw_text_raw(fb_ptr, stride, 100, 450, "Starting RayOS kernel...", 0x00ffaa00);

    // Simple delay loop (no more UEFI stall available)
    for _ in 0..100_000_000 {
        unsafe { core::ptr::read_volatile(&fb_base); }
    }

    // JUMP TO KERNEL ENTRY POINT
    // Cast the kernel buffer to a function pointer and call it
    // The kernel entry point has signature: extern "C" fn(fb_base: usize, fb_size: usize, width: usize, height: usize, stride: usize) -> !
    type KernelEntry = extern "C" fn(usize, usize, usize, usize, usize) -> !;
    let kernel_entry: KernelEntry = unsafe { mem::transmute(kernel_buffer_addr) };

    // Jump to kernel - this should never return
    kernel_entry(fb_base, fb_size, width, height, stride);
}

/// Clear the screen to a solid color
fn clear_screen(framebuffer: &mut uefi::proto::console::gop::FrameBuffer, width: usize, height: usize, stride: usize, color: u32) {
    let fb_slice = framebuffer.as_mut_ptr();
    for y in 0..height {
        for x in 0..width {
            let offset = (y * stride + x) * 4;
            unsafe {
                *fb_slice.add(offset) = (color & 0xff) as u8;         // B
                *fb_slice.add(offset + 1) = ((color >> 8) & 0xff) as u8;  // G
                *fb_slice.add(offset + 2) = ((color >> 16) & 0xff) as u8; // R
                *fb_slice.add(offset + 3) = 0xff;                     // A
            }
        }
    }
}

/// Draw a banner with title
fn draw_banner(framebuffer: &mut uefi::proto::console::gop::FrameBuffer, width: usize, height: usize, stride: usize) {
    // Draw title bar
    draw_rect(framebuffer, stride, 50, 50, width - 100, 150, 0x00282a36);
    draw_rect(framebuffer, stride, 52, 52, width - 104, 146, 0x0044475a);

    // Draw text (ASCII art style title)
    draw_text(framebuffer, width, stride, 100, 80, "RayOS UEFI Bootloader v0.1", 0x00ff79c6);
    draw_text(framebuffer, width, stride, 100, 120, "Bicameral GPU-Native Kernel", 0x008be9fd);
    draw_text(framebuffer, width, stride, 100, 160, "Phase 1: The Skeleton", 0x0050fa7b);
}

/// Draw a rectangle
fn draw_rect(framebuffer: &mut uefi::proto::console::gop::FrameBuffer, stride: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    let fb_slice = framebuffer.as_mut_ptr();
    for py in y..y+h {
        for px in x..x+w {
            let offset = (py * stride + px) * 4;
            unsafe {
                *fb_slice.add(offset) = (color & 0xff) as u8;
                *fb_slice.add(offset + 1) = ((color >> 8) & 0xff) as u8;
                *fb_slice.add(offset + 2) = ((color >> 16) & 0xff) as u8;
                *fb_slice.add(offset + 3) = 0xff;
            }
        }
    }
}

/// Draw text using a simple 8x8 bitmap font
fn draw_text(framebuffer: &mut uefi::proto::console::gop::FrameBuffer, _width: usize, stride: usize, x: usize, y: usize, text: &str, color: u32) {
    let fb_slice = framebuffer.as_mut_ptr();
    draw_text_raw(fb_slice, stride, x, y, text, color);
}

/// Draw text directly to a raw framebuffer pointer (can be used after exit_boot_services)
fn draw_text_raw(fb_ptr: *mut u8, stride: usize, x: usize, y: usize, text: &str, color: u32) {
    for (i, ch) in text.chars().enumerate() {
        let glyph = get_glyph(ch);
        for py in 0..8 {
            for px in 0..8 {
                if (glyph[py] >> (7 - px)) & 1 == 1 {
                    let screen_x = x + i * 9 + px;
                    let screen_y = y + py;
                    let offset = (screen_y * stride + screen_x) * 4;
                    unsafe {
                        if offset + 3 < stride * 1080 * 4 { // Bounds check
                            *fb_ptr.add(offset) = (color & 0xff) as u8;
                            *fb_ptr.add(offset + 1) = ((color >> 8) & 0xff) as u8;
                            *fb_ptr.add(offset + 2) = ((color >> 16) & 0xff) as u8;
                            *fb_ptr.add(offset + 3) = 0xff;
                        }
                    }
                }
            }
        }
    }
}

/// Simple 8x8 bitmap font (very basic ASCII)
fn get_glyph(ch: char) -> [u8; 8] {
    match ch {
        'A' => [0x18, 0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x00],
        'B' => [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
        'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        'D' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        'E' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00],
        'F' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'G' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3E, 0x00],
        'H' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'I' => [0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'K' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        'L' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        'N' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'P' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'R' => [0x7C, 0x66, 0x66, 0x7C, 0x78, 0x6C, 0x66, 0x00],
        'S' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'V' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'W' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        'Y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        'a' | 'e' | 'i' | 'o' | 'u' => get_glyph(ch.to_ascii_uppercase()),
        'b' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'c' => [0x00, 0x00, 0x3C, 0x60, 0x60, 0x60, 0x3C, 0x00],
        'd' => [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'f' => [0x0C, 0x18, 0x18, 0x3E, 0x18, 0x18, 0x18, 0x00],
        'g' => [0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C, 0x00],
        'h' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'k' => [0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
        'l' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x0C, 0x00],
        'm' => [0x00, 0x00, 0x76, 0x7F, 0x6B, 0x63, 0x63, 0x00],
        'n' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'p' => [0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x00],
        'r' => [0x00, 0x00, 0x7C, 0x66, 0x60, 0x60, 0x60, 0x00],
        's' => [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
        't' => [0x18, 0x18, 0x7E, 0x18, 0x18, 0x18, 0x0E, 0x00],
        'v' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'w' => [0x00, 0x00, 0x63, 0x6B, 0x7F, 0x36, 0x36, 0x00],
        'x' => [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
        'y' => [0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x7C, 0x00],
        '0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        '1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        '2' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        '3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        '4' => [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
        '5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        '6' => [0x3C, 0x60, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
        '?' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x00, 0x18, 0x00],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '/' => [0x00, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x00, 0x00],
        '(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        ')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // Unknown char
    }
}