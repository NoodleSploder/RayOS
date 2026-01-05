use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

const PAGE_SIZE: usize = 4096;
const MMIO_COUNTER_BASE: u64 = 16 * 1024 * 1024;
const MMIO_COUNTER_SIZE: u64 = PAGE_SIZE as u64;
const MMIO_VIRTIO_BASE: u64 = MMIO_COUNTER_BASE + MMIO_COUNTER_SIZE;
const VIRTIO_MMIO_QUEUE_DESC_OFFSET: u64 = 0x080;
const VIRTIO_MMIO_QUEUE_DRIVER_OFFSET: u64 = 0x088;
const VIRTIO_MMIO_QUEUE_USED_OFFSET: u64 = 0x090;
const VIRTIO_MMIO_QUEUE_SIZE_OFFSET: u64 = 0x098;
const VIRTIO_MMIO_QUEUE_READY_OFFSET: u64 = 0x09c;
const VIRTIO_MMIO_STATUS_OFFSET: u64 = 0x070;
const VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET: u64 = 0x060;
const VIRTIO_MMIO_INTERRUPT_ACK_OFFSET: u64 = 0x064;
const VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET: u64 = 0x050;
const VIRTIO_QUEUE_DESC_GPA: u64 = 0x0010_0000;
const VIRTIO_QUEUE_DRIVER_GPA: u64 = VIRTIO_QUEUE_DESC_GPA + 0x1000;
const VIRTIO_QUEUE_USED_GPA: u64 = VIRTIO_QUEUE_DRIVER_GPA + 0x1000;
const VIRTIO_QUEUE_SIZE_VALUE: u64 = 8;
const VIRTIO_QUEUE_READY_VALUE: u64 = 1;
const VIRTIO_BLK_REQ0_GPA: u64 = 0x0010_4000;
const VIRTIO_BLK_DATA0_GPA: u64 = VIRTIO_BLK_REQ0_GPA + 0x1000;
const VIRTIO_BLK_STATUS0_GPA: u64 = VIRTIO_BLK_DATA0_GPA + 0x1000;

const VIRTIO_BLK_REQ1_GPA: u64 = VIRTIO_BLK_STATUS0_GPA + 0x1000;
const VIRTIO_BLK_DATA1_GPA: u64 = VIRTIO_BLK_REQ1_GPA + 0x1000;
const VIRTIO_BLK_STATUS1_GPA: u64 = VIRTIO_BLK_DATA1_GPA + 0x1000;
const VIRTIO_BLK_REQ_LEN: u32 = 16;
const VIRTIO_BLK_DATA_LEN: u32 = 512;
const VIRTIO_BLK_STATUS_LEN: u32 = 1;

// Virtio-console test support
const VIRTIO_CONSOLE_MSG_GPA: u64 = 0x0010_A000;
const VIRTIO_CONSOLE_MSG_LEN: u32 = 64;

// Virtio-net test support
const VIRTIO_NET_TX_PKT_GPA: u64 = 0x0010_9000;
const VIRTIO_NET_RX_PKT_GPA: u64 = VIRTIO_NET_TX_PKT_GPA + 0x1000;
const VIRTIO_NET_PKT_LEN: u32 = 64;  // Simple Ethernet frame

// Virtio-input test support
const VIRTIO_INPUT_EVENT_GPA: u64 = 0x0010_B000;
const VIRTIO_INPUT_EVENT_LEN: u32 = 64;
const VIRTIO_MMIO_DEVICE_ID_OFFSET: u64 = 0x008;

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

struct CodeEmitter {
    bytes: Vec<u8>,
}

impl CodeEmitter {
    fn new() -> Self {
        CodeEmitter { bytes: Vec::new() }
    }

    fn emit_byte(&mut self, byte: u8) {
        self.bytes.push(byte);
    }

    fn emit_bytes(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
    }

    fn emit_imm64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn mov_reg64_imm(&mut self, reg: u8, value: u64) -> usize {
        self.emit_byte(0x48);
        self.emit_byte(0xB8 + (reg & 0x7));
        let offset = self.bytes.len();
        self.emit_imm64(value);
        offset
    }

    fn mov_rdi_imm(&mut self, addr: u64) {
        // mov rdi, imm64
        self.emit_bytes(&[0x48, 0xBF]);
        self.emit_imm64(addr);
    }

    fn mov_eax_imm32(&mut self, value: u32) {
        // mov eax, imm32
        self.emit_byte(0xB8);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn mov_mem_rdi_eax(&mut self) {
        // mov [rdi], eax
        self.emit_bytes(&[0x89, 0x07]);
    }

    fn mov_rax_imm(&mut self, value: u64) {
        self.mov_reg64_imm(0, value);
    }

    fn mov_mem_rdi_disp8_rax(&mut self, disp: u8) {
        // mov [rdi+disp8], rax
        self.emit_bytes(&[0x48, 0x89, 0x47, disp]);
    }

    fn mov_mem_rax(&mut self, addr: u64) {
        self.emit_bytes(&[0x48, 0xA3]);
        self.emit_imm64(addr);
    }

    fn mov_rax_mem(&mut self, addr: u64) {
        self.emit_bytes(&[0x48, 0xA1]);
        self.emit_imm64(addr);
    }
}

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

fn guest_driver_blob_output_path() -> PathBuf {
    // This generator is invoked in a few different ways:
    // - from the repo root (developer convenience)
    // - from the scripts/ directory (smoke scripts)
    // Avoid accidentally writing to scripts/crates/... when run from scripts/.
    let candidates = [
        "crates/kernel-bare/src/guest_driver_template.bin",
        "../crates/kernel-bare/src/guest_driver_template.bin",
    ];

    for candidate in candidates {
        let p = Path::new(candidate);
        if p.parent().is_some_and(|d| d.exists()) {
            return p.to_path_buf();
        }
    }

    PathBuf::from("crates/kernel-bare/src/guest_driver_template.bin")
}

fn main() -> std::io::Result<()> {
    let input_mode: bool = std::env::var("RAYOS_GUEST_INPUT_ENABLED")
        .ok()
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let net_mode: bool = std::env::var("RAYOS_GUEST_NET_ENABLED")
        .ok()
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let console_mode: bool = std::env::var("RAYOS_GUEST_CONSOLE_ENABLED")
        .ok()
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    eprintln!(
        "DEBUG: RAYOS_GUEST_INPUT_ENABLED={:?}, input_mode={} RAYOS_GUEST_NET_ENABLED={:?}, net_mode={} RAYOS_GUEST_CONSOLE_ENABLED={:?}, console_mode={}",
        std::env::var("RAYOS_GUEST_INPUT_ENABLED"),
        input_mode,
        std::env::var("RAYOS_GUEST_NET_ENABLED"),
        net_mode,
        std::env::var("RAYOS_GUEST_CONSOLE_ENABLED"),
        console_mode
    );

    if (input_mode && net_mode) || (input_mode && console_mode) || (net_mode && console_mode) {
        eprintln!(
            "WARN: multiple guest modes set; preferring input > console > net"
        );
    }

    let req0_type: u32 = std::env::var("RAYOS_GUEST_REQ0_TYPE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let req1_type: u32 = std::env::var("RAYOS_GUEST_REQ1_TYPE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let req0_sector: u64 = std::env::var("RAYOS_GUEST_REQ0_SECTOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let req1_sector: u64 = std::env::var("RAYOS_GUEST_REQ1_SECTOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut emitter = CodeEmitter::new();

    if input_mode {
        return emit_input_test(&mut emitter);
    }

    if console_mode {
        return emit_console_test(&mut emitter);
    }

    if net_mode {
        return emit_network_test(&mut emitter);
    }

    emitter.emit_bytes(&[0xB0, 0x41]);
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0x04, 0x01]);
    emitter.emit_bytes(&[0xE6, 0xE9]);

    emitter.mov_rax_mem(MMIO_COUNTER_BASE);
    emitter.emit_bytes(&[0x48, 0xFF, 0xC0]);
    emitter.mov_mem_rax(MMIO_COUNTER_BASE);

    let virtio_features = MMIO_VIRTIO_BASE + 0x010;
    let virtio_driver_features = MMIO_VIRTIO_BASE + 0x020;
    let virtio_status = MMIO_VIRTIO_BASE + VIRTIO_MMIO_STATUS_OFFSET;
    let virtio_queue_notify = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
    let virtio_int_status = MMIO_VIRTIO_BASE + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET;
    let virtio_int_ack = MMIO_VIRTIO_BASE + VIRTIO_MMIO_INTERRUPT_ACK_OFFSET;

    emitter.mov_rax_mem(virtio_features);
    emitter.emit_bytes(&[0x48, 0xFF, 0xC0]);
    emitter.mov_mem_rax(virtio_driver_features);

    emitter.mov_reg64_imm(0, 1);
    emitter.mov_mem_rax(virtio_status);

    let queue_desc_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_DESC_OFFSET;
    let queue_driver_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_DRIVER_OFFSET;
    let queue_used_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_USED_OFFSET;
    let queue_size_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_SIZE_OFFSET;
    let queue_ready_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_READY_OFFSET;

    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DESC_GPA);
    emitter.mov_mem_rax(queue_desc_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DRIVER_GPA);
    emitter.mov_mem_rax(queue_driver_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_USED_GPA);
    emitter.mov_mem_rax(queue_used_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_SIZE_VALUE);
    emitter.mov_mem_rax(queue_size_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_READY_VALUE);
    emitter.mov_mem_rax(queue_ready_reg);

    let data0_flags: u16 = VIRTQ_DESC_F_NEXT | if req0_type == 1 { 0 } else { VIRTQ_DESC_F_WRITE };
    let data1_flags: u16 = VIRTQ_DESC_F_NEXT | if req1_type == 1 { 0 } else { VIRTQ_DESC_F_WRITE };

    let descriptors = [
        VirtqDesc {
            addr: VIRTIO_BLK_REQ0_GPA,
            len: VIRTIO_BLK_REQ_LEN,
            flags: VIRTQ_DESC_F_NEXT,
            next: 1,
        },
        VirtqDesc {
            addr: VIRTIO_BLK_DATA0_GPA,
            len: VIRTIO_BLK_DATA_LEN,
            flags: data0_flags,
            next: 2,
        },
        VirtqDesc {
            addr: VIRTIO_BLK_STATUS0_GPA,
            len: VIRTIO_BLK_STATUS_LEN,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        },
        VirtqDesc {
            addr: VIRTIO_BLK_REQ1_GPA,
            len: VIRTIO_BLK_REQ_LEN,
            flags: VIRTQ_DESC_F_NEXT,
            next: 4,
        },
        VirtqDesc {
            addr: VIRTIO_BLK_DATA1_GPA,
            len: VIRTIO_BLK_DATA_LEN,
            flags: data1_flags,
            next: 5,
        },
        VirtqDesc {
            addr: VIRTIO_BLK_STATUS1_GPA,
            len: VIRTIO_BLK_STATUS_LEN,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        },
    ];
    let mut desc_bytes = Vec::new();
    for desc in &descriptors {
        desc_bytes.extend_from_slice(&desc.addr.to_le_bytes());
        desc_bytes.extend_from_slice(&desc.len.to_le_bytes());
        desc_bytes.extend_from_slice(&desc.flags.to_le_bytes());
        desc_bytes.extend_from_slice(&desc.next.to_le_bytes());
    }

    let desc_len = desc_bytes.len() as u64;
    let desc_data_patch = emitter.mov_reg64_imm(6, 0);
    emitter.mov_reg64_imm(7, VIRTIO_QUEUE_DESC_GPA);
    emitter.mov_reg64_imm(1, desc_len);
    emitter.emit_bytes(&[0xF3, 0xA4]);

    emitter.emit_bytes(&[0x48, 0x31, 0xC0]);
    emitter.mov_reg64_imm(7, VIRTIO_BLK_REQ0_GPA);
    emitter.mov_reg64_imm(1, VIRTIO_BLK_REQ_LEN as u64);
    emitter.emit_bytes(&[0xF3, 0xAA]);

    // Fill request 0: type + sector.
    emitter.mov_rdi_imm(VIRTIO_BLK_REQ0_GPA);
    emitter.mov_eax_imm32(req0_type);
    emitter.mov_mem_rdi_eax();
    emitter.mov_rax_imm(req0_sector);
    emitter.mov_mem_rdi_disp8_rax(8);

    emitter.mov_reg64_imm(7, VIRTIO_BLK_DATA0_GPA);
    if req0_type == 1 {
        // For OUT/WRITE requests, make data non-zero so log_write_descriptors is meaningful.
        emitter.emit_bytes(&[0xB0, 0xAB]);
    }
    emitter.mov_reg64_imm(1, VIRTIO_BLK_DATA_LEN as u64);
    emitter.emit_bytes(&[0xF3, 0xAA]);

    emitter.mov_reg64_imm(7, VIRTIO_BLK_STATUS0_GPA);
    emitter.emit_bytes(&[0xB0, 0x00]);
    emitter.emit_bytes(&[0x88, 0x07]);

    // Second request + buffers.
    emitter.mov_reg64_imm(7, VIRTIO_BLK_REQ1_GPA);
    emitter.mov_reg64_imm(1, VIRTIO_BLK_REQ_LEN as u64);
    emitter.emit_bytes(&[0xF3, 0xAA]);

    emitter.mov_rdi_imm(VIRTIO_BLK_REQ1_GPA);
    emitter.mov_eax_imm32(req1_type);
    emitter.mov_mem_rdi_eax();
    emitter.mov_rax_imm(req1_sector);
    emitter.mov_mem_rdi_disp8_rax(8);

    emitter.mov_reg64_imm(7, VIRTIO_BLK_DATA1_GPA);
    if req1_type == 1 {
        emitter.emit_bytes(&[0xB0, 0xAB]);
    }
    emitter.mov_reg64_imm(1, VIRTIO_BLK_DATA_LEN as u64);
    emitter.emit_bytes(&[0xF3, 0xAA]);

    emitter.mov_reg64_imm(7, VIRTIO_BLK_STATUS1_GPA);
    emitter.emit_bytes(&[0xB0, 0x00]);
    emitter.emit_bytes(&[0x88, 0x07]);

    emitter.mov_reg64_imm(2, VIRTIO_QUEUE_DRIVER_GPA);
    emitter.mov_reg64_imm(0, 0x0002_0000);
    emitter.emit_bytes(&[0x89, 0x02]);
    // avail.ring[0]=0, avail.ring[1]=3
    emitter.mov_reg64_imm(0, 0x0003_0000);
    emitter.emit_bytes(&[0x89, 0x42, 0x04]);

    emitter.mov_reg64_imm(2, VIRTIO_QUEUE_USED_GPA);
    emitter.mov_reg64_imm(0, 0);
    emitter.emit_bytes(&[0x89, 0x02]);

    emitter.mov_reg64_imm(0, 1);
    emitter.mov_mem_rax(virtio_queue_notify);

    // Exercise virtio-MMIO interrupt registers: read status, print if non-zero, then ack.
    // (Print as ASCII digit to avoid NUL bytes in logs.)
    emitter.mov_rax_mem(virtio_int_status);
    emitter.emit_bytes(&[0x48, 0x89, 0xC3]); // mov rbx, rax (preserve raw bits)
    emitter.emit_bytes(&[0x84, 0xC0]); // test al, al
    emitter.emit_bytes(&[0x74, 0x04]); // jz +4 (skip add+out)
    emitter.emit_bytes(&[0x04, 0x30]); // add al, '0'
    emitter.emit_bytes(&[0xE6, 0xE9]); // out 0xE9, al
    emitter.emit_bytes(&[0x48, 0x89, 0xD8]); // mov rax, rbx (restore raw bits)
    emitter.mov_mem_rax(virtio_int_ack);

    emitter.emit_byte(0xF4);
    emitter.emit_bytes(&[0xEB, 0xDE]);

    let desc_data_offset = emitter.bytes.len();
    emitter.emit_bytes(&desc_bytes);

    println!("desc_data_offset={}", desc_data_offset);
    println!("desc_data_patch={}", desc_data_patch);

    let out_path = guest_driver_blob_output_path();
    let mut file = File::create(out_path)?;
    file.write_all(&emitter.bytes)?;
    Ok(())
}

fn emit_input_test(emitter: &mut CodeEmitter) -> std::io::Result<()> {
    // Minimal virtio-input eventq test:
    // - publish a single writable buffer descriptor
    // - notify queue 0
    // Hypervisor should write an event into the buffer and emit deterministic markers.

    let virtio_features = MMIO_VIRTIO_BASE + 0x010;
    let virtio_driver_features = MMIO_VIRTIO_BASE + 0x020;
    let virtio_status = MMIO_VIRTIO_BASE + VIRTIO_MMIO_STATUS_OFFSET;
    let virtio_queue_notify = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
    let queue_desc_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_DESC_OFFSET;
    let queue_driver_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_DRIVER_OFFSET;
    let queue_used_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_USED_OFFSET;
    let queue_size_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_SIZE_OFFSET;
    let queue_ready_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_READY_OFFSET;

    // Build descriptor 0 bytes to be copied into guest desc area.
    // Descriptor points to a device-writeable event buffer.
    let mut desc_bytes = Vec::new();
    desc_bytes.extend_from_slice(&VIRTIO_INPUT_EVENT_GPA.to_le_bytes());
    desc_bytes.extend_from_slice(&VIRTIO_INPUT_EVENT_LEN.to_le_bytes());
    desc_bytes.extend_from_slice(&VIRTQ_DESC_F_WRITE.to_le_bytes()); // flags
    desc_bytes.extend_from_slice(&(0u16.to_le_bytes())); // next
    let desc_len = desc_bytes.len() as u64;

    // IMPORTANT: the kernel patches a u64 at a fixed offset inside this blob.
    // That offset must land on the imm64 of `mov r6, imm64`.
    emitter.emit_bytes(&[0xB0, 0x49]); // mov al, 'I'
    emitter.emit_bytes(&[0xE6, 0xE9]); // out 0xE9, al

    // mov r6, imm64 encoding is: 48 BE <imm64>  (imm64 starts 2 bytes after opcode)
    const GUEST_DRIVER_DESC_DATA_PTR_OFFSET: usize = 176;
    if emitter.bytes.len() > (GUEST_DRIVER_DESC_DATA_PTR_OFFSET - 2) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "input blob prelude exceeds patch offset budget",
        ));
    }
    while emitter.bytes.len() < (GUEST_DRIVER_DESC_DATA_PTR_OFFSET - 2) {
        emitter.emit_byte(0x90); // nop
    }
    let _desc_data_patch = emitter.mov_reg64_imm(6, 0);

    // Jump over padded descriptor bytes region.
    let jmp_start = emitter.bytes.len();
    emitter.emit_byte(0xE9); // jmp rel32
    let jmp_rel32_off = emitter.bytes.len();
    emitter.emit_bytes(&[0, 0, 0, 0]);

    const GUEST_DRIVER_DESC_DATA_OFFSET: usize = 501;
    if emitter.bytes.len() > GUEST_DRIVER_DESC_DATA_OFFSET {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "input blob grew past canonical desc data offset",
        ));
    }
    while emitter.bytes.len() < GUEST_DRIVER_DESC_DATA_OFFSET {
        emitter.emit_byte(0x90); // nop padding (skipped by the jmp)
    }

    // Emit descriptor bytes at exactly the canonical offset.
    emitter.emit_bytes(&desc_bytes);

    // Patch the jump displacement to land after the descriptor bytes.
    let continue_off = emitter.bytes.len();
    let rel = (continue_off as i64) - ((jmp_start + 5) as i64);
    if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "jmp displacement overflow",
        ));
    }
    let rel32: i32 = rel as i32;
    emitter.bytes[jmp_rel32_off..jmp_rel32_off + 4].copy_from_slice(&rel32.to_le_bytes());

    // Negotiate features
    emitter.mov_rax_mem(virtio_features);
    emitter.emit_bytes(&[0x48, 0xFF, 0xC0]); // inc rax
    emitter.mov_mem_rax(virtio_driver_features);

    // Set STATUS=ACKNOWLEDGE|DRIVER
    emitter.mov_reg64_imm(0, 0x03);
    emitter.mov_mem_rax(virtio_status);

    // Setup queue 0 descriptors/driver/used
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DESC_GPA);
    emitter.mov_mem_rax(queue_desc_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DRIVER_GPA);
    emitter.mov_mem_rax(queue_driver_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_USED_GPA);
    emitter.mov_mem_rax(queue_used_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_SIZE_VALUE);
    emitter.mov_mem_rax(queue_size_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_READY_VALUE);
    emitter.mov_mem_rax(queue_ready_reg);

    // Copy descriptor bytes into the guest descriptor table.
    emitter.mov_reg64_imm(7, VIRTIO_QUEUE_DESC_GPA);
    emitter.mov_reg64_imm(1, desc_len);
    emitter.emit_bytes(&[0xF3, 0xA4]); // rep movsb (src=R6 dest=R7 count=R1)

    // Submit avail ring: set avail.idx=1 and avail.ring[0]=0.
    emitter.mov_rdi_imm(VIRTIO_QUEUE_DRIVER_GPA);
    emitter.emit_bytes(&[0x66, 0xC7, 0x47, 0x02, 0x01, 0x00]); // mov word [rdi+2], 1
    emitter.emit_bytes(&[0x66, 0xC7, 0x47, 0x04, 0x00, 0x00]); // mov word [rdi+4], 0

    // Notify queue 0
    emitter.mov_reg64_imm(0, 0);
    emitter.mov_mem_rax(virtio_queue_notify);

    // Guest emits a small serial marker
    emitter.emit_bytes(&[0xB0, 0x47]); // 'G'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x3A]); // ':'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x49]); // 'I'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x4E]); // 'N'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x50]); // 'P'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x55]); // 'U'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x54]); // 'T'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x0A]); // '\n'
    emitter.emit_bytes(&[0xE6, 0xE9]);

    emitter.emit_byte(0xF4); // hlt
    emitter.emit_bytes(&[0xEB, 0xFE]); // jmp $

    let out_path = guest_driver_blob_output_path();
    let mut file = File::create(out_path)?;
    file.write_all(&emitter.bytes)?;
    Ok(())
}

fn emit_console_test(emitter: &mut CodeEmitter) -> std::io::Result<()> {
    // Simple virtio-console data queue test: write a message into guest memory,
    // point a single descriptor at it, and notify queue 0 so the host sees the message.

    let virtio_features = MMIO_VIRTIO_BASE + 0x010;
    let virtio_driver_features = MMIO_VIRTIO_BASE + 0x020;
    let virtio_status = MMIO_VIRTIO_BASE + VIRTIO_MMIO_STATUS_OFFSET;
    let virtio_queue_notify = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
    let queue_desc_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_DESC_OFFSET;
    let queue_driver_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_DRIVER_OFFSET;
    let queue_used_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_USED_OFFSET;
    let queue_size_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_SIZE_OFFSET;
    let queue_ready_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_READY_OFFSET;

    // Build descriptor 0 bytes to be copied into guest desc area.
    let mut desc_bytes = Vec::new();
    desc_bytes.extend_from_slice(&VIRTIO_CONSOLE_MSG_GPA.to_le_bytes());
    desc_bytes.extend_from_slice(&(VIRTIO_CONSOLE_MSG_LEN.to_le_bytes()));
    desc_bytes.extend_from_slice(&(0u16.to_le_bytes())); // flags
    desc_bytes.extend_from_slice(&(0u16.to_le_bytes())); // next
    let desc_len = desc_bytes.len() as u64;

    // IMPORTANT: the kernel patches a u64 at a fixed offset inside this blob.
    // That offset must land on the imm64 of `mov r6, imm64`.
    // Keep the prelude minimal and pad with NOPs to hit the canonical location.
    emitter.emit_bytes(&[0xB0, 0x43]); // mov al, 'C'
    emitter.emit_bytes(&[0xE6, 0xE9]); // out 0xE9, al

    // Ensure the imm64 of `mov r6, imm64` begins at the kernel's expected patch offset.
    // mov r6, imm64 encoding is: 48 BE <imm64>  (imm64 starts 2 bytes after opcode)
    const GUEST_DRIVER_DESC_DATA_PTR_OFFSET: usize = 176;
    if emitter.bytes.len() > (GUEST_DRIVER_DESC_DATA_PTR_OFFSET - 2) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "console blob prelude exceeds patch offset budget",
        ));
    }
    while emitter.bytes.len() < (GUEST_DRIVER_DESC_DATA_PTR_OFFSET - 2) {
        emitter.emit_byte(0x90); // nop
    }
    let _desc_data_patch = emitter.mov_reg64_imm(6, 0);

    // Place descriptor bytes at the canonical offset, but do NOT execute them.
    // Insert a jump that skips over the (padded) data region.
    let jmp_start = emitter.bytes.len();
    emitter.emit_byte(0xE9); // jmp rel32
    let jmp_rel32_off = emitter.bytes.len();
    emitter.emit_bytes(&[0, 0, 0, 0]);

    const GUEST_DRIVER_DESC_DATA_OFFSET: usize = 501;
    if emitter.bytes.len() > GUEST_DRIVER_DESC_DATA_OFFSET {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "console blob grew past canonical desc data offset",
        ));
    }

    while emitter.bytes.len() < GUEST_DRIVER_DESC_DATA_OFFSET {
        emitter.emit_byte(0x90); // nop padding (skipped by the jmp)
    }

    // Emit descriptor bytes at exactly the canonical offset.
    emitter.emit_bytes(&desc_bytes);

    // Patch jump displacement to land after the descriptor bytes.
    let continue_off = emitter.bytes.len();
    let rel = (continue_off as i64) - ((jmp_start + 5) as i64);
    if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "jmp displacement overflow",
        ));
    }
    let rel32: i32 = rel as i32;
    emitter.bytes[jmp_rel32_off..jmp_rel32_off + 4].copy_from_slice(&rel32.to_le_bytes());

    // Negotiate features
    emitter.mov_rax_mem(virtio_features);
    emitter.emit_bytes(&[0x48, 0xFF, 0xC0]); // inc rax
    emitter.mov_mem_rax(virtio_driver_features);

    // Set STATUS=ACKNOWLEDGE|DRIVER
    emitter.mov_reg64_imm(0, 0x03);
    emitter.mov_mem_rax(virtio_status);

    // Setup queue 0 descriptors/driver/used
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DESC_GPA);
    emitter.mov_mem_rax(queue_desc_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DRIVER_GPA);
    emitter.mov_mem_rax(queue_driver_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_USED_GPA);
    emitter.mov_mem_rax(queue_used_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_SIZE_VALUE);
    emitter.mov_mem_rax(queue_size_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_READY_VALUE);
    emitter.mov_mem_rax(queue_ready_reg);

    // Write the message into guest memory at VIRTIO_CONSOLE_MSG_GPA
    emitter.mov_rdi_imm(VIRTIO_CONSOLE_MSG_GPA);
    let msg = b"guest->console: hello from blob\n";
    // write in 1-byte stores using mov al, imm; mov [rdi+off], al
    for (i, &b) in msg.iter().enumerate() {
        emitter.emit_bytes(&[0xB0, b]); // mov al, imm8
        if i == 0 {
            emitter.emit_bytes(&[0x88, 0x07]); // mov [rdi], al
        } else {
            emitter.emit_bytes(&[0x88, 0x47, i as u8]); // mov [rdi+disp8], al
        }
    }

    // R7 -> dest (VIRTIO_QUEUE_DESC_GPA), R1 -> len
    emitter.mov_reg64_imm(7, VIRTIO_QUEUE_DESC_GPA);
    emitter.mov_reg64_imm(1, desc_len);
    emitter.emit_bytes(&[0xF3, 0xA4]); // rep movsb (src=R6 dest=R7 count=R1)

    // Submit avail ring: set avail.idx=1 and avail.ring[0]=0.
    // Layout: flags(u16) @ +0, idx(u16) @ +2, ring(u16[]) @ +4
    emitter.mov_rdi_imm(VIRTIO_QUEUE_DRIVER_GPA);
    // idx = 1 (write as 16-bit)
    emitter.emit_bytes(&[0x66, 0xC7, 0x47, 0x02, 0x01, 0x00]); // mov word [rdi+2], 1
    // ring[0] = 0
    emitter.emit_bytes(&[0x66, 0xC7, 0x47, 0x04, 0x00, 0x00]); // mov word [rdi+4], 0

    // Notify queue 0
    emitter.mov_reg64_imm(0, 0);
    emitter.mov_mem_rax(virtio_queue_notify);

    // Guest emits a small serial marker
    emitter.emit_bytes(&[0xB0, 0x47]); // 'G'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x3A]); // ':'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x43]); // 'C'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x4F]); // 'O'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x4E]); // 'N'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x53]); // 'S'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x4F]); // 'O'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x4C]); // 'L'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x0A]); // '\n'
    emitter.emit_bytes(&[0xE6, 0xE9]);

    emitter.emit_byte(0xF4); // hlt
    emitter.emit_bytes(&[0xEB, 0xFE]); // jmp $

    let out_path = guest_driver_blob_output_path();
    let mut file = File::create(out_path)?;
    file.write_all(&emitter.bytes)?;
    Ok(())
}


fn emit_network_test(emitter: &mut CodeEmitter) -> std::io::Result<()> {
    // Simple virtio-net test: send a minimal Ethernet frame via TX queue and setup RX
    // This enables testing the hypervisor's packet loopback functionality

    emitter.emit_bytes(&[0xB0, 0x41]);
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0x04, 0x01]);
    emitter.emit_bytes(&[0xE6, 0xE9]);

    let virtio_device_id = MMIO_VIRTIO_BASE + VIRTIO_MMIO_DEVICE_ID_OFFSET;
    let virtio_features = MMIO_VIRTIO_BASE + 0x010;
    let virtio_driver_features = MMIO_VIRTIO_BASE + 0x020;
    let virtio_status = MMIO_VIRTIO_BASE + VIRTIO_MMIO_STATUS_OFFSET;
    let virtio_queue_notify = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
    let virtio_int_status = MMIO_VIRTIO_BASE + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET;
    let virtio_int_ack = MMIO_VIRTIO_BASE + VIRTIO_MMIO_INTERRUPT_ACK_OFFSET;
    let queue_desc_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_DESC_OFFSET;
    let queue_driver_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_DRIVER_OFFSET;
    let queue_used_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_USED_OFFSET;
    let queue_size_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_SIZE_OFFSET;
    let queue_ready_reg = MMIO_VIRTIO_BASE + VIRTIO_MMIO_QUEUE_READY_OFFSET;

    // Try to read device ID to confirm we're looking at virtio device
    emitter.mov_rax_mem(virtio_device_id);
    emitter.emit_bytes(&[0xE6, 0xE9]); // out 0xE9, al (print device ID low byte)

    // Negotiate features
    emitter.mov_rax_mem(virtio_features);
    emitter.emit_bytes(&[0x48, 0xFF, 0xC0]); // inc rax
    emitter.mov_mem_rax(virtio_driver_features);

    // Set STATUS=ACKNOWLEDGE|DRIVER
    emitter.mov_reg64_imm(0, 0x03);
    emitter.mov_mem_rax(virtio_status);

    // Setup TX queue descriptor ring
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DESC_GPA);
    emitter.mov_mem_rax(queue_desc_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DRIVER_GPA);
    emitter.mov_mem_rax(queue_driver_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_USED_GPA);
    emitter.mov_mem_rax(queue_used_reg);
    emitter.mov_reg64_imm(0, 8); // queue size
    emitter.mov_mem_rax(queue_size_reg);
    emitter.mov_reg64_imm(0, 1); // queue ready
    emitter.mov_mem_rax(queue_ready_reg);

    // Build minimal Ethernet frame in guest memory
    // Dest MAC: AA:BB:CC:DD:EE:FF
    // Src MAC:  52:55:4F:53:00:01  (RAYOS)
    // EtherType: 0x0800 (IPv4)
    // Payload: minimal (just enough to test)

    emitter.mov_rdi_imm(VIRTIO_NET_TX_PKT_GPA);

    // Write destination MAC (6 bytes): 0xAA 0xBB 0xCC 0xDD 0xEE 0xFF
    emitter.emit_bytes(&[0xB0, 0xAA]); // mov al, 0xAA
    emitter.emit_bytes(&[0x88, 0x07]); // mov [rdi], al
    emitter.emit_bytes(&[0xB0, 0xBB]); // mov al, 0xBB
    emitter.emit_bytes(&[0x88, 0x47, 0x01]); // mov [rdi+1], al
    emitter.emit_bytes(&[0xB0, 0xCC]); // mov al, 0xCC
    emitter.emit_bytes(&[0x88, 0x47, 0x02]); // mov [rdi+2], al
    emitter.emit_bytes(&[0xB0, 0xDD]); // mov al, 0xDD
    emitter.emit_bytes(&[0x88, 0x47, 0x03]); // mov [rdi+3], al
    emitter.emit_bytes(&[0xB0, 0xEE]); // mov al, 0xEE
    emitter.emit_bytes(&[0x88, 0x47, 0x04]); // mov [rdi+4], al
    emitter.emit_bytes(&[0xB0, 0xFF]); // mov al, 0xFF
    emitter.emit_bytes(&[0x88, 0x47, 0x05]); // mov [rdi+5], al

    // Write source MAC (6 bytes): 0x52 0x55 0x4F 0x53 0x00 0x01 (RAYOS)
    emitter.emit_bytes(&[0xB0, 0x52]); // mov al, 0x52
    emitter.emit_bytes(&[0x88, 0x47, 0x06]); // mov [rdi+6], al
    emitter.emit_bytes(&[0xB0, 0x55]); // mov al, 0x55
    emitter.emit_bytes(&[0x88, 0x47, 0x07]); // mov [rdi+7], al
    emitter.emit_bytes(&[0xB0, 0x4F]); // mov al, 0x4F
    emitter.emit_bytes(&[0x88, 0x47, 0x08]); // mov [rdi+8], al
    emitter.emit_bytes(&[0xB0, 0x53]); // mov al, 0x53
    emitter.emit_bytes(&[0x88, 0x47, 0x09]); // mov [rdi+9], al
    emitter.emit_bytes(&[0xB0, 0x00]); // mov al, 0x00
    emitter.emit_bytes(&[0x88, 0x47, 0x0A]); // mov [rdi+10], al
    emitter.emit_bytes(&[0xB0, 0x01]); // mov al, 0x01
    emitter.emit_bytes(&[0x88, 0x47, 0x0B]); // mov [rdi+11], al

    // Write EtherType (2 bytes): 0x08 0x00 (IPv4, big-endian)
    emitter.emit_bytes(&[0xB0, 0x08]); // mov al, 0x08
    emitter.emit_bytes(&[0x88, 0x47, 0x0C]); // mov [rdi+12], al
    emitter.emit_bytes(&[0xB0, 0x00]); // mov al, 0x00
    emitter.emit_bytes(&[0x88, 0x47, 0x0D]); // mov [rdi+13], al

    // Fill rest with pattern
    emitter.emit_bytes(&[0xB0, 0x42]); // mov al, 0x42 ('B')
    emitter.mov_reg64_imm(1, VIRTIO_NET_PKT_LEN as u64 - 14); // remaining bytes
    emitter.mov_reg64_imm(7, VIRTIO_NET_TX_PKT_GPA + 14); // start of payload
    emitter.emit_bytes(&[0xF3, 0xAA]); // rep stosb

    // Setup TX descriptor chain: single descriptor pointing to our packet
    // We'll use descriptor 0
    let _tx_desc_bytes = vec![
        // Descriptor 0: TX packet buffer (not writable)
        (VIRTIO_NET_TX_PKT_GPA as u32) as u8, // addr lo
        ((VIRTIO_NET_TX_PKT_GPA >> 8) as u32) as u8,
        ((VIRTIO_NET_TX_PKT_GPA >> 16) as u32) as u8,
        ((VIRTIO_NET_TX_PKT_GPA >> 24) as u32) as u8,
        (VIRTIO_NET_PKT_LEN & 0xFF) as u8,  // len lo
        ((VIRTIO_NET_PKT_LEN >> 8) & 0xFF) as u8,
        ((VIRTIO_NET_PKT_LEN >> 16) & 0xFF) as u8,
        ((VIRTIO_NET_PKT_LEN >> 24) & 0xFF) as u8,
        0x00, 0x00, // flags (no NEXT, no WRITE)
        0x00, 0x00, // next
        0x00, 0x00,
        0x00, 0x00,
    ];

    // Write descriptor to ring
    // Write descriptor 0 as two 8-byte writes: [addr (u64)] [len(u32)|flags(u16)|next(u16)]
    emitter.mov_rdi_imm(VIRTIO_QUEUE_DESC_GPA);
    // Descriptor 0: addr
    emitter.mov_reg64_imm(0, VIRTIO_NET_TX_PKT_GPA);
    emitter.mov_mem_rdi_disp8_rax(0);
    // Descriptor 0: len/flags/next packed into u64 at offset 8
    let tx_len_flags_next: u64 = (0u64 << 48) | (0u64 << 32) | (VIRTIO_NET_PKT_LEN as u64);
    emitter.mov_reg64_imm(0, tx_len_flags_next);
    emitter.mov_mem_rdi_disp8_rax(8);

    // Submit TX: write to avail ring
    emitter.mov_rdi_imm(VIRTIO_QUEUE_DRIVER_GPA);
    emitter.emit_bytes(&[0xB0, 0x00]); // mov al, 0 (first descriptor)
    emitter.emit_bytes(&[0x88, 0x47, 0x04]); // mov [rdi+4], al (avail.ring[0])
    emitter.mov_reg64_imm(0, 0x01);
    emitter.mov_mem_rdi_disp8_rax(2); // mov [rdi+2], rax (avail.idx = 1)

    // Notify TX queue
    emitter.mov_reg64_imm(0, 0); // TX queue = 0
    emitter.mov_mem_rax(virtio_queue_notify);

    // Emit a serial marker from guest after TX notify: "G:NET_TX\n"
    emitter.emit_bytes(&[0xB0, 0x47]); // mov al, 'G'
    emitter.emit_bytes(&[0xE6, 0xE9]); // out 0xE9, al
    emitter.emit_bytes(&[0xB0, 0x3A]); // mov al, ':'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x4E]); // mov al, 'N'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x45]); // mov al, 'E'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x54]); // mov al, 'T'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x5F]); // mov al, '_'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x54]); // mov al, 'T'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x58]); // mov al, 'X'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x0A]); // mov al, '\n'
    emitter.emit_bytes(&[0xE6, 0xE9]);

    // Micro delay
    emitter.emit_bytes(&[0x90; 16]); // nop × 16

    // Setup RX queue descriptor ring (queue 1)
    // We need to select queue 1 first
    emitter.mov_reg64_imm(0, 1); // select queue 1 (RX)
    emitter.mov_mem_rax(MMIO_VIRTIO_BASE + 0x030); // queue selector register

    // Setup RX queue descriptor ring
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DESC_GPA + 0x1000); // RX descriptors at different offset
    emitter.mov_mem_rax(queue_desc_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_DRIVER_GPA + 0x1000); // RX driver ring
    emitter.mov_mem_rax(queue_driver_reg);
    emitter.mov_reg64_imm(0, VIRTIO_QUEUE_USED_GPA + 0x1000); // RX used ring
    emitter.mov_mem_rax(queue_used_reg);
    emitter.mov_reg64_imm(0, 8); // queue size
    emitter.mov_mem_rax(queue_size_reg);
    emitter.mov_reg64_imm(0, 1); // queue ready
    emitter.mov_mem_rax(queue_ready_reg);

    // Setup RX descriptor chain: single writable descriptor for received packets
    let _rx_desc_bytes = vec![
        // Descriptor 0: RX packet buffer (writable)
        ((VIRTIO_NET_RX_PKT_GPA) as u32) as u8, // addr lo
        ((VIRTIO_NET_RX_PKT_GPA >> 8) as u32) as u8,
        ((VIRTIO_NET_RX_PKT_GPA >> 16) as u32) as u8,
        ((VIRTIO_NET_RX_PKT_GPA >> 24) as u32) as u8,
        (VIRTIO_NET_PKT_LEN & 0xFF) as u8,  // len lo
        ((VIRTIO_NET_PKT_LEN >> 8) & 0xFF) as u8,
        ((VIRTIO_NET_PKT_LEN >> 16) & 0xFF) as u8,
        ((VIRTIO_NET_PKT_LEN >> 24) & 0xFF) as u8,
        (VIRTQ_DESC_F_WRITE & 0xFF) as u8, (VIRTQ_DESC_F_WRITE >> 8) as u8, // flags (WRITE)
        0x00, 0x00, // next
        0x00, 0x00,
        0x00, 0x00,
    ];

    // Write RX descriptor to ring
    // Write RX descriptor as two 8-byte writes at RX descriptor area
    emitter.mov_rdi_imm(VIRTIO_QUEUE_DESC_GPA + 0x1000);
    // RX addr
    emitter.mov_reg64_imm(0, VIRTIO_NET_RX_PKT_GPA);
    emitter.mov_mem_rdi_disp8_rax(0);
    // RX len/flags/next: flags = VIRTQ_DESC_F_WRITE
    let rx_packed: u64 = ((0u64) << 48) | ((VIRTQ_DESC_F_WRITE as u64) << 32) | (VIRTIO_NET_PKT_LEN as u64);
    emitter.mov_reg64_imm(0, rx_packed);
    emitter.mov_mem_rdi_disp8_rax(8);

    // Submit RX: write to avail ring
    emitter.mov_rdi_imm(VIRTIO_QUEUE_DRIVER_GPA + 0x1000); // RX driver ring
    emitter.emit_bytes(&[0xB0, 0x00]); // mov al, 0 (first descriptor)
    emitter.emit_bytes(&[0x88, 0x47, 0x04]); // mov [rdi+4], al (avail.ring[0])
    emitter.mov_reg64_imm(0, 0x01);
    emitter.mov_mem_rdi_disp8_rax(2); // mov [rdi+2], rax (avail.idx = 1)

    // Notify RX queue
    emitter.mov_reg64_imm(0, 1); // RX queue = 1
    emitter.mov_mem_rax(virtio_queue_notify);

    // Emit a serial marker from guest after RX notify: "G:NET_RX\n"
    emitter.emit_bytes(&[0xB0, 0x47]); // mov al, 'G'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x3A]); // mov al, ':'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x4E]); // mov al, 'N'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x45]); // mov al, 'E'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x54]); // mov al, 'T'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x5F]); // mov al, '_'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x52]); // mov al, 'R'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x58]); // mov al, 'X'
    emitter.emit_bytes(&[0xE6, 0xE9]);
    emitter.emit_bytes(&[0xB0, 0x0A]); // mov al, '\n'
    emitter.emit_bytes(&[0xE6, 0xE9]);

    // Check interrupt status
    emitter.mov_rax_mem(virtio_int_status);
    emitter.emit_bytes(&[0x48, 0x89, 0xC3]); // mov rbx, rax
    emitter.emit_bytes(&[0x84, 0xC0]); // test al, al
    emitter.emit_bytes(&[0x74, 0x04]); // jz +4
    emitter.emit_bytes(&[0x04, 0x30]); // add al, '0'
    emitter.emit_bytes(&[0xE6, 0xE9]); // out 0xE9, al
    emitter.emit_bytes(&[0x48, 0x89, 0xD8]); // mov rax, rbx
    emitter.mov_mem_rax(virtio_int_ack);

    emitter.emit_byte(0xF4); // hlt
    emitter.emit_bytes(&[0xEB, 0xFE]); // jmp $

    println!("desc_data_offset=0");
    println!("desc_data_patch=0");

    let out_path = guest_driver_blob_output_path();
    let mut file = File::create(out_path)?;
    file.write_all(&emitter.bytes)?;
    Ok(())
}
