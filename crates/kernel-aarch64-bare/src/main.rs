#![no_std]
#![no_main]

use core::arch::asm;

mod acpi;
mod pci;

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

    model_ptr: u64,
    model_size: u64,

    volume_ptr: u64,
    volume_size: u64,

    embeddings_ptr: u64,
    embeddings_size: u64,

    index_ptr: u64,
    index_size: u64,

    boot_unix_seconds: u64,
    boot_time_valid: u32,
    _time_reserved: u32,
}

const BOOTINFO_MAGIC: u64 = 0x5241_594F_535F_4249; // "RAYOS_BI"

const QEMU_VIRT_PL011_BASE: usize = 0x0900_0000;

fn uart_putc(byte: u8) {
    unsafe {
        let dr = (QEMU_VIRT_PL011_BASE + 0x00) as *mut u32;
        let fr = (QEMU_VIRT_PL011_BASE + 0x18) as *const u32;
        while core::ptr::read_volatile(fr) & (1 << 5) != 0 {
            core::hint::spin_loop();
        }
        core::ptr::write_volatile(dr, byte as u32);
    }
}

fn uart_try_getc() -> Option<u8> {
    unsafe {
        let dr = (QEMU_VIRT_PL011_BASE + 0x00) as *const u32;
        let fr = (QEMU_VIRT_PL011_BASE + 0x18) as *const u32;
        // FR bit4 = RXFE (receive FIFO empty)
        if core::ptr::read_volatile(fr) & (1 << 4) != 0 {
            return None;
        }
        Some((core::ptr::read_volatile(dr) & 0xFF) as u8)
    }
}

fn uart_write_str(s: &str) {
    for &b in s.as_bytes() {
        if b == b'\n' {
            uart_putc(b'\r');
        }
        uart_putc(b);
    }
}

fn uart_write_hex_u64(value: u64) {
    let hex = b"0123456789ABCDEF";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        let shift = (15 - i) * 4;
        buf[i] = hex[((value >> shift) & 0xF) as usize];
    }
    for &b in &buf {
        uart_putc(b);
    }
}

fn uart_write_u32_dec(mut value: u32) {
    // Division-based decimal is fine here (we have a panic handler), but keep it simple.
    // If this ever becomes a problem, swap to a division-free renderer like kernel-bare.
    let mut buf = [0u8; 10];
    let mut i = 0usize;
    if value == 0 {
        uart_putc(b'0');
        return;
    }
    while value != 0 {
        let digit = (value % 10) as u8;
        buf[i] = b'0' + digit;
        i += 1;
        value /= 10;
    }
    while i != 0 {
        i -= 1;
        uart_putc(buf[i]);
    }
}

fn uart_write_bytes(bytes: &[u8]) {
    for &b in bytes {
        uart_putc(b);
    }
}

fn parse_u32_prefix_and_rest(line: &[u8]) -> Option<(u32, &[u8])> {
    let mut i = 0usize;
    let mut id: u32 = 0;
    while i < line.len() {
        let c = line[i];
        if c == b':' {
            if i == 0 {
                return None;
            }
            return Some((id, &line[i + 1..]));
        }
        if c < b'0' || c > b'9' {
            return None;
        }
        id = id.saturating_mul(10).saturating_add((c - b'0') as u32);
        i += 1;
    }
    None
}

fn align_up4(v: usize) -> usize {
    (v + 3) & !3
}

// Minimal read-only Volume format for early bring-up.
// Header: [8]"RAYOSVOL" + u32 version(=1) + u32 entry_count
// Entry:  u16 key_len + u16 value_len + u32 reserved + key bytes + value bytes + pad to 4
fn volume_kv_find(buf: &[u8], key: &[u8]) -> Result<Option<(usize, usize)>, &'static str> {
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
            return Ok(Some((off + key_len, val_len)));
        }
        off = align_up4(end);
        if off > buf.len() {
            break;
        }
    }
    Ok(None)
}

fn spin_delay(iterations: u64) {
    let mut i = 0u64;
    while i < iterations {
        core::hint::spin_loop();
        i = i.wrapping_add(1);
    }
}

fn fb_try_draw_test_pattern(bi: &BootInfo) {
    if bi.fb_base == 0 || bi.fb_width == 0 || bi.fb_height == 0 {
        return;
    }

    // Assume 32bpp linear framebuffer; paint a small corner pattern.
    let base = bi.fb_base as *mut u32;
    let stride = bi.fb_stride as usize;

    for y in 0..32usize {
        for x in 0..64usize {
            let color = if x < 32 { 0x00FF00 } else { 0xFFAA00 };
            unsafe {
                core::ptr::write_volatile(base.add(y * stride + x), color);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn _start(boot_info_phys: u64) -> ! {
    unsafe {
        // Mask interrupts.
        asm!("msr daifset, #0xf", options(nomem, nostack, preserves_flags));
    }

    uart_write_str("RayOS kernel-aarch64-bare: _start\n");
    uart_write_str("  boot_info=0x");
    uart_write_hex_u64(boot_info_phys);
    uart_write_str("\n");

    let bi = unsafe { &*(boot_info_phys as *const BootInfo) };
    if bi.magic != BOOTINFO_MAGIC {
        uart_write_str("  ERROR: bad BootInfo magic\n");
    } else {
        uart_write_str("  BootInfo OK fb_base=0x");
        uart_write_hex_u64(bi.fb_base);
        uart_write_str(" model_ptr=0x");
        uart_write_hex_u64(bi.model_ptr);
        uart_write_str("\n");
    }

    fb_try_draw_test_pattern(bi);

    if bi.rsdp_addr != 0 {
        uart_write_str("  RSDP found at 0x");
        uart_write_hex_u64(bi.rsdp_addr);
        uart_write_str("\n");

        if let Some(mcfg) = acpi::find_mcfg(bi.rsdp_addr) {
            pci::enumerate_pci(mcfg);
        }
    }


    // Prove volume is queryable from Option B.
    uart_write_str("  volume_ptr=0x");
    uart_write_hex_u64(bi.volume_ptr);
    uart_write_str(" volume_size=0x");
    uart_write_hex_u64(bi.volume_size);
    uart_write_str("\n");

    uart_write_str("  embeddings_ptr=0x");
    uart_write_hex_u64(bi.embeddings_ptr);
    uart_write_str(" embeddings_size=0x");
    uart_write_hex_u64(bi.embeddings_size);
    uart_write_str("\n");

    uart_write_str("  index_ptr=0x");
    uart_write_hex_u64(bi.index_ptr);
    uart_write_str(" index_size=0x");
    uart_write_hex_u64(bi.index_size);
    uart_write_str("\n");
    if bi.volume_ptr != 0 && bi.volume_size != 0 {
        let buf = unsafe { core::slice::from_raw_parts(bi.volume_ptr as *const u8, bi.volume_size as usize) };
        match volume_kv_find(buf, b"greeting") {
            Ok(Some((val_off, val_len))) => {
                uart_write_str("volume: greeting = ");
                for &b in buf[val_off..val_off + val_len].iter().take(256) {
                    if (0x20..=0x7e).contains(&b) {
                        uart_putc(b);
                    } else {
                        uart_putc(b'.');
                    }
                }
                uart_write_str("\n");
            }
            Ok(None) => uart_write_str("volume: greeting not found\n"),
            Err(e) => {
                uart_write_str(e);
                uart_write_str("\n");
            }
        }
    } else {
        uart_write_str("volume: not present\n");
    }

    uart_write_str("RayOS aarch64 kernel loop ready\n");

    // Option B bring-up: prove we can use the host-side ai_bridge protocol from the kernel.
    // Emit a deterministic request that ai_bridge can answer without an external backend.
    let request_id: u32 = 1;
    spin_delay(2_000_000);
    uart_write_str("RAYOS_INPUT:");
    uart_write_u32_dec(request_id);
    uart_write_str(":what time is it?\n");

    // Read and echo AI responses until we see AI_END for our id.
    let mut line_buf = [0u8; 256];
    let mut line_len: usize = 0;
    let mut saw_end = false;

    let mut ticks: u64 = 0;
    loop {
        ticks = ticks.wrapping_add(1);

        if let Some(ch) = uart_try_getc() {
            if ch == b'\r' {
                // ignore
            } else if ch == b'\n' {
                let line = &line_buf[..line_len];
                // Minimal parsing: look for AI_END:<id>
                if line.starts_with(b"AI_END:") {
                    if let Some((id, _rest)) = parse_u32_prefix_and_rest(&line[b"AI_END:".len()..]) {
                        if id == request_id {
                            uart_write_str("RayOS kernel-aarch64-bare: got AI_END for id=");
                            uart_write_u32_dec(id);
                            uart_write_str("\n");
                            saw_end = true;
                        }
                    }
                } else if line.starts_with(b"AI:") {
                    // Echo a short marker for visibility.
                    if let Some((id, rest)) = parse_u32_prefix_and_rest(&line[b"AI:".len()..]) {
                        if id == request_id {
                            uart_write_str("RayOS kernel-aarch64-bare: AI chunk id=");
                            uart_write_u32_dec(id);
                            uart_write_str(" bytes=");
                            uart_write_u32_dec(rest.len() as u32);
                            uart_write_str("\n");
                        }
                    }
                }

                line_len = 0;
            } else {
                if line_len + 1 < line_buf.len() {
                    line_buf[line_len] = ch;
                    line_len += 1;
                } else {
                    // Drop overly long line.
                    line_len = 0;
                }
            }
        }

        if (ticks & ((1u64 << 22) - 1)) == 0 {
            uart_write_str("tick=0x");
            uart_write_hex_u64(ticks);
            uart_write_str(" ai_done=");
            uart_write_bytes(if saw_end { b"1" } else { b"0" });
            uart_write_str("\n");
        }

        spin_delay(80_000);
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    uart_write_str("RayOS kernel-aarch64-bare: panic\n");
    loop {
        core::hint::spin_loop();
    }
}