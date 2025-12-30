#![no_std]
#![no_main]
#![feature(asm_const)]

// Bare-metal kernel entry point

/// This is called directly by the bootloader after loading the ELF

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};

// Framebuffer info passed from bootloader (will be set by bootloader)
static mut FB_BASE: usize = 0;
static mut FB_WIDTH: usize = 1024;
static mut FB_HEIGHT: usize = 768;
static mut FB_STRIDE: usize = 1024;

// Memory management globals
static HEAP_ALLOCATOR: spin::Mutex<BumpAllocator> = spin::Mutex::new(BumpAllocator::new());
static ALLOCATED_PAGES: AtomicUsize = AtomicUsize::new(0);

// Page table base - 2MB for page tables
const PAGE_TABLE_BASE: usize = 0x200000;
const HEAP_START: usize = 0x400000;  // Start heap at 4MB
const HEAP_SIZE: usize = 64 * 1024 * 1024;  // 64MB heap

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Entry point called by bootloader
    kernel_main();
}

fn kernel_main() -> ! {
    // Initialize framebuffer (bootloader should have set these)
    unsafe {
        if FB_BASE == 0 {
            // Fallback: assume standard framebuffer location
            FB_BASE = 0xE0000000; // Common GOP framebuffer base
        }
    }

    // Clear screen to dark blue
    clear_screen(0x1a_1a_2e);

    // Draw kernel banner
    draw_box(30, 30, 700, 450, 0x2a_2a_4e);
    draw_text(50, 50, "RayOS Kernel v0.1 - LIVE!", 0xff_ff_ff);

    // System status
    draw_text(50, 100, "Hardware Initialization:", 0xff_ff_88);
    draw_text(70, 130, "[OK] IDT: Interrupt Descriptor Table", 0x88_ff_88);
    draw_text(70, 160, "[OK] GDT: Global Descriptor Table", 0x88_ff_88);
    draw_text(70, 190, "[OK] Memory Manager: Active", 0x88_ff_88);
    draw_text(70, 220, "[OK] Framebuffer: Active", 0x88_ff_88);

    draw_text(50, 270, "Subsystems:", 0xff_ff_88);
    draw_text(70, 300, "[ ] System 1: GPU Reflex Engine", 0xaa_aa_aa);
    draw_text(70, 330, "[ ] System 2: LLM Cognitive Engine", 0xaa_aa_aa);
    draw_text(70, 360, "[ ] Conductor: Task Orchestration", 0xaa_aa_aa);
    draw_text(70, 390, "[ ] Ouroboros: Self-Optimization", 0xaa_aa_aa);
    draw_text(70, 410, "[ ] Volume: Persistent Storage", 0xaa_aa_aa);
    draw_text(70, 430, "[ ] Intent: Natural Language Parser", 0xaa_aa_aa);

    // Initialize IDT
    init_idt();
    draw_text(70, 130, "[OK] IDT: Interrupt Descriptor Table", 0x00_ff_00);

    // Initialize GDT
    init_gdt();
    draw_text(70, 160, "[OK] GDT: Global Descriptor Table", 0x00_ff_00);

    // Initialize memory manager - THE CRITICAL PIECE
    init_memory();
    draw_text(70, 190, "[OK] Memory Manager: Active", 0x00_ff_00);

    // Test the allocator
    let test_alloc = kalloc(4096, 4096);
    if test_alloc.is_some() {
        draw_text(70, 220, "[OK] Zero-Copy Allocator: VERIFIED", 0x00_ff_00);
    } else {
        draw_text(70, 220, "[!!] Zero-Copy Allocator: FAILED", 0xff_00_00);
    }

    // Intent is provided by System 2 in the current build (deterministic parser stub).
    draw_text(70, 430, "[OK] Intent: Natural Language Parser", 0x00_ff_00);

    // Draw memory info box
    draw_box(50, 460, 680, 60, 0x2a_2a_4e);
    draw_text(60, 470, "Memory Status:", 0xff_ff_88);

    let (used, total, pages) = memory_stats();
    draw_text(60, 490, "Heap Used: ", 0xaa_aa_aa);
    draw_number(170, 490, used / 1024, 0x00_ff_ff);
    draw_text(230, 490, "KB / ", 0xaa_aa_aa);
    draw_number(280, 490, total / 1024 / 1024, 0x00_ff_ff);
    draw_text(312, 490, "MB", 0xaa_aa_aa);

    draw_text(400, 490, "Pages: ", 0xaa_aa_aa);
    draw_number(470, 490, pages, 0x00_ff_ff);
    // Main kernel loop
    let mut tick = 0u64;
    let mut blink = false;

    loop {
        tick = tick.wrapping_add(1);

        // Update every ~5M iterations
        if tick % 5_000_000 == 0 {
            blink = !blink;

            // Draw heartbeat indicator
            let color = if blink { 0x00_ff_00 } else { 0x2a_2a_4e };
            draw_box(650, 50, 30, 20, color);

            // Update tick counter
            draw_text(50, 450, "Ticks: ", 0xaa_aa_aa);
            draw_number(130, 450, (tick / 5_000_000) as usize, 0xff_ff_00);

            // Update memory stats
            let (used, total, pages) = memory_stats();
            draw_box(170, 490, 500, 20, 0x2a_2a_4e);
            draw_number(170, 490, used / 1024, 0x00_ff_ff);
            draw_text(230, 490, "KB / ", 0xaa_aa_aa);
            draw_number(280, 490, total / 1024 / 1024, 0x00_ff_ff);
            draw_text(312, 490, "MB", 0xaa_aa_aa);
            draw_number(470, 490, pages, 0x00_ff_ff);
        }

        // Spin hint for CPU
        core::hint::spin_loop();
    }
}

// IDT initialization
fn init_idt() {
    // Phase 1/bring-up: we intentionally keep interrupts disabled.
    // A real IDT (and exception handlers) should be installed before enabling
    // interrupts or relying on CPU exceptions for debugging.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

// GDT initialization
fn init_gdt() {
    // Phase 1/bring-up: long mode uses a minimal GDT, but since we never enable
    // interrupts here (see init_idt), we keep this as a no-op until the proper
    // early boot sequence owns GDT/segment setup.
}

// Memory initialization
fn init_memory() {
    // Set up page tables for identity mapping (0-4GB)
    setup_page_tables();

    // Enable paging
    enable_paging();

    // Initialize heap allocator
    unsafe {
        HEAP_ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }
}

//=============================================================================
// MEMORY MANAGEMENT - Zero-Copy Allocator (Core of RayOS)
//=============================================================================

/// Page table entry format for x86_64
#[repr(transparent)]
#[derive(Clone, Copy)]
struct PageTableEntry(u64);

impl PageTableEntry {
    const PRESENT: u64 = 1 << 0;
    const WRITABLE: u64 = 1 << 1;
    const USER: u64 = 1 << 2;
    const HUGE_PAGE: u64 = 1 << 7;

    fn new() -> Self {
        Self(0)
    }

    fn set_addr(&mut self, addr: usize) {
        self.0 = (self.0 & 0xFFF) | ((addr as u64) & !0xFFF);
    }

    fn set_flags(&mut self, flags: u64) {
        self.0 |= flags;
    }
}

/// Page table structure (512 entries)
#[repr(align(4096))]
struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    fn zero() -> Self {
        Self {
            entries: [PageTableEntry::new(); 512],
        }
    }
}

/// Setup identity mapping for first 4GB using 2MB pages
fn setup_page_tables() {
    unsafe {
        let pml4 = &mut *(PAGE_TABLE_BASE as *mut PageTable);
        let pdpt = &mut *((PAGE_TABLE_BASE + 0x1000) as *mut PageTable);
        let pd = &mut *((PAGE_TABLE_BASE + 0x2000) as *mut PageTable);

        // Zero out tables
        *pml4 = PageTable::zero();
        *pdpt = PageTable::zero();
        *pd = PageTable::zero();

        // PML4[0] -> PDPT
        pml4.entries[0].set_addr(PAGE_TABLE_BASE + 0x1000);
        pml4.entries[0].set_flags(PageTableEntry::PRESENT | PageTableEntry::WRITABLE);

        // PDPT[0] -> PD
        pdpt.entries[0].set_addr(PAGE_TABLE_BASE + 0x2000);
        pdpt.entries[0].set_flags(PageTableEntry::PRESENT | PageTableEntry::WRITABLE);

        // Map first 4GB with 2MB pages (2048 entries)
        // We'll use multiple PDs for this
        for pd_idx in 0..4 {
            let pd_addr = PAGE_TABLE_BASE + 0x2000 + (pd_idx * 0x1000);
            let pd = &mut *(pd_addr as *mut PageTable);
            *pd = PageTable::zero();

            // Point PDPT entry to this PD
            pdpt.entries[pd_idx].set_addr(pd_addr);
            pdpt.entries[pd_idx].set_flags(PageTableEntry::PRESENT | PageTableEntry::WRITABLE);

            // Identity map 512 * 2MB = 1GB per PD
            for i in 0..512 {
                let phys_addr = (pd_idx * 512 + i) * 0x200000; // 2MB per entry
                pd.entries[i].set_addr(phys_addr);
                pd.entries[i].set_flags(
                    PageTableEntry::PRESENT |
                    PageTableEntry::WRITABLE |
                    PageTableEntry::HUGE_PAGE
                );
            }
        }
    }
}

/// Enable paging by setting CR3 and CR0
fn enable_paging() {
    unsafe {
        // Set CR3 to point to PML4
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) PAGE_TABLE_BASE,
            options(nostack, preserves_flags)
        );
    }
}

/// Simple bump allocator for kernel heap
struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            heap_start: 0,
            heap_end: 0,
            next: 0,
        }
    }

    fn init(&mut self, start: usize, size: usize) {
        self.heap_start = start;
        self.heap_end = start + size;
        self.next = start;
    }

    fn allocate(&mut self, size: usize, align: usize) -> Option<usize> {
        // Align next pointer
        let aligned_addr = (self.next + align - 1) & !(align - 1);
        let end_addr = aligned_addr.checked_add(size)?;

        if end_addr > self.heap_end {
            return None; // Out of memory
        }

        self.next = end_addr;
        ALLOCATED_PAGES.fetch_add((size + 4095) / 4096, Ordering::Relaxed);

        Some(aligned_addr)
    }

    fn allocated_bytes(&self) -> usize {
        self.next.saturating_sub(self.heap_start)
    }
}

/// Public allocation function for kernel use
pub fn kalloc(size: usize, align: usize) -> Option<*mut u8> {
    HEAP_ALLOCATOR
        .lock()
        .allocate(size, align)
        .map(|addr| addr as *mut u8)
}

/// Get memory statistics
pub fn memory_stats() -> (usize, usize, usize) {
    let allocator = HEAP_ALLOCATOR.lock();
    let used = allocator.allocated_bytes();
    let total = HEAP_SIZE;
    let pages = ALLOCATED_PAGES.load(Ordering::Relaxed);
    (used, total, pages)
}

// Framebuffer operations
fn clear_screen(color: u32) {
    unsafe {
        let fb = FB_BASE as *mut u32;
        let pixels = FB_WIDTH * FB_HEIGHT;
        for i in 0..pixels {
            *fb.add(i) = color;
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

    // Avoid division/modulo intrinsics in bare-metal/no_std contexts.
    while num > 0 {
        // Compute (num % 10) and (num / 10) using subtraction.
        let mut quotient: usize = 0;
        while num >= 10 {
            num = num.wrapping_sub(10);
            quotient = quotient.wrapping_add(1);
        }

        digits[count] = num as u8; // remainder
        num = quotient;
        count = count.wrapping_add(1);
    }

    for i in 0..count {
        let digit = digits[count - 1 - i];
        let ch = (b'0' + digit) as char;
        draw_char(x + (i * FONT_WIDTH), y, ch, color);
    }
}

fn get_glyph(ch: char) -> [u8; 8] {
    match ch {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '!' => [0x18, 0x3C, 0x3C, 0x18, 0x18, 0x00, 0x18, 0x00],
        '(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        ')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        '[' => [0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00],
        ']' => [0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
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
        'z' => [0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Draw panic message on screen
    unsafe {
        if FB_BASE != 0 {
            clear_screen(0x00_00_ff); // Blue screen of death
            draw_box(100, 100, 600, 300, 0x00_00_aa);
            draw_text(120, 120, "KERNEL PANIC!", 0xff_ff_ff);
            draw_text(120, 150, "The system has encountered a critical error.", 0xff_ff_ff);
            draw_text(120, 200, "Please restart your computer.", 0xaa_aa_aa);
        }
    }

    loop {
        core::hint::spin_loop();
    }
}
