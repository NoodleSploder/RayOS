#![no_std]
#![no_main]

use core::cell::UnsafeCell;
use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use libm::{expf, sqrtf};

const KERNEL_BASE: u64 = 0xffff_ffff_8000_0000;
static KERNEL_PHYS_START_ALIGNED: AtomicU64 = AtomicU64::new(0);
static KERNEL_PHYS_END_ALIGNED: AtomicU64 = AtomicU64::new(0);
static KERNEL_VIRT_DELTA: AtomicU64 = AtomicU64::new(0);

static CURRENT_PML4_PHYS: AtomicU64 = AtomicU64::new(0);

fn cpu_enable_x87_sse() {
    // On x86_64, the ABI expects SSE registers for floating point.
    // When building with a modern `core` via `-Zbuild-std`, we must ensure the
    // CPU has x87/SSE enabled before any code paths might execute SSE.
    unsafe {
        let mut cr0: u64;
        asm!("mov {0}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        // Clear EM (x87 emulation) and TS (task switched), set MP (monitor coprocessor).
        cr0 &= !(1 << 2);
        cr0 &= !(1 << 3);
        cr0 |= 1 << 1;
        asm!("mov cr0, {0}", in(reg) cr0, options(nomem, nostack, preserves_flags));

        let mut cr4: u64;
        asm!("mov {0}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        // Enable FXSAVE/FXRSTOR + unmasked SIMD FP exceptions support.
        cr4 |= 1 << 9; // OSFXSR
        cr4 |= 1 << 10; // OSXMMEXCPT
        asm!("mov cr4, {0}", in(reg) cr4, options(nomem, nostack, preserves_flags));

        // Initialize the x87 FPU state.
        asm!("fninit", options(nomem, nostack));
    }
}

//=============================================================================
// Minimal serial output (COM1) for early bring-up
//=============================================================================

const COM1_PORT: u16 = 0x3F8;

#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let mut value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}

#[inline(always)]
unsafe fn outw(port: u16, value: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inw(port: u16) -> u16 {
    let mut value: u16;
    asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags));
    value
}

#[inline(always)]
unsafe fn outl(port: u16, value: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inl(port: u16) -> u32 {
    let mut value: u32;
    asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack, preserves_flags));
    value
}

fn serial_init() {
    unsafe {
        // Disable interrupts
        outb(COM1_PORT + 1, 0x00);
        // Enable DLAB
        outb(COM1_PORT + 3, 0x80);
        // Set divisor to 1 (115200 baud)
        outb(COM1_PORT + 0, 0x01);
        outb(COM1_PORT + 1, 0x00);
        // 8 bits, no parity, one stop bit
        outb(COM1_PORT + 3, 0x03);
        // Enable FIFO, clear them, with 14-byte threshold
        outb(COM1_PORT + 2, 0xC7);
        // IRQs enabled, RTS/DSR set
        outb(COM1_PORT + 4, 0x0B);
    }
}

fn serial_write_byte(byte: u8) {
    unsafe {
        // Wait for transmit holding register empty
        while (inb(COM1_PORT + 5) & 0x20) == 0 {
            core::hint::spin_loop();
        }
        outb(COM1_PORT, byte);
    }
}

fn serial_write_str(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            serial_write_byte(b'\r');
        }
        serial_write_byte(b);
    }
}

fn serial_write_hex_u64(mut value: u64) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut buf = [0u8; 16];
    for i in (0..16).rev() {
        buf[i] = HEX[(value & 0xF) as usize];
        value >>= 4;
    }
    for b in buf {
        serial_write_byte(b);
    }
}

fn serial_try_read_byte() -> Option<u8> {
    // LSR bit0 = Data Ready
    unsafe {
        if (inb(COM1_PORT + 5) & 0x01) == 0 {
            return None;
        }
        Some(inb(COM1_PORT))
    }
}

fn serial_write_tagged_input(msg_id: u32, line: &[u8]) {
    serial_write_str("RAYOS_INPUT:");
    // Decimal message id so the host can correlate responses.
    // Keep it ASCII-only for simplicity.
    let mut tmp = [0u8; 10];
    let mut n = 0usize;
    let mut v = msg_id;
    if v == 0 {
        tmp[0] = b'0';
        n = 1;
    } else {
        while v != 0 && n < tmp.len() {
            tmp[n] = b'0' + (v % 10) as u8;
            v /= 10;
            n += 1;
        }
        // Reverse.
        for i in 0..(n / 2) {
            let j = n - 1 - i;
            let t = tmp[i];
            tmp[i] = tmp[j];
            tmp[j] = t;
        }
    }
    for i in 0..n {
        serial_write_byte(tmp[i]);
    }
    serial_write_byte(b':');

    for &b in line {
        if b >= 0x20 && b <= 0x7E {
            serial_write_byte(b);
        }
    }
    serial_write_str("\n");
}

//=============================================================================
// Simple chat transcript (fixed-size, heap-free)
//=============================================================================

const CHAT_MAX_LINES: usize = 10;
const CHAT_MAX_COLS: usize = 78;

struct ChatLog {
    lines: [[u8; CHAT_MAX_COLS]; CHAT_MAX_LINES],
    lens: [usize; CHAT_MAX_LINES],
    head: usize,
    count: usize,
}

impl ChatLog {
    const fn new() -> Self {
        Self {
            lines: [[0u8; CHAT_MAX_COLS]; CHAT_MAX_LINES],
            lens: [0usize; CHAT_MAX_LINES],
            head: 0,
            count: 0,
        }
    }

    fn push_line(&mut self, prefix: &[u8], text: &[u8]) {
        let idx = self.head;
        self.head = (self.head + 1) % CHAT_MAX_LINES;
        if self.count < CHAT_MAX_LINES {
            self.count += 1;
        }

        // Clear
        for b in self.lines[idx].iter_mut() {
            *b = 0;
        }

        let mut out = 0usize;
        for &b in prefix.iter() {
            if out >= CHAT_MAX_COLS {
                break;
            }
            if b >= 0x20 && b <= 0x7E {
                self.lines[idx][out] = b;
                out += 1;
            }
        }
        for &b in text.iter() {
            if out >= CHAT_MAX_COLS {
                break;
            }
            if b >= 0x20 && b <= 0x7E {
                self.lines[idx][out] = b;
                out += 1;
            }
        }
        self.lens[idx] = out;
    }

    fn replace_last_line(&mut self, prefix: &[u8], text: &[u8]) {
        if self.count == 0 {
            self.push_line(prefix, text);
            return;
        }
        // Last written line is head-1.
        let idx = (self.head + CHAT_MAX_LINES - 1) % CHAT_MAX_LINES;

        for b in self.lines[idx].iter_mut() {
            *b = 0;
        }

        let mut out = 0usize;
        for &b in prefix.iter() {
            if out >= CHAT_MAX_COLS {
                break;
            }
            if b >= 0x20 && b <= 0x7E {
                self.lines[idx][out] = b;
                out += 1;
            }
        }
        for &b in text.iter() {
            if out >= CHAT_MAX_COLS {
                break;
            }
            if b >= 0x20 && b <= 0x7E {
                self.lines[idx][out] = b;
                out += 1;
            }
        }
        self.lens[idx] = out;
    }

    fn get_nth_oldest(&self, n: usize) -> Option<(&[u8], usize)> {
        if n >= self.count {
            return None;
        }
        let start = if self.count < CHAT_MAX_LINES {
            0
        } else {
            self.head
        };
        let idx = (start + n) % CHAT_MAX_LINES;
        Some((&self.lines[idx], self.lens[idx]))
    }
}

//=============================================================================
// Local (in-guest) "LLM" responder (tiny heuristic language model)
//=============================================================================

// Feature-gated so "LLM inside RayOS" is the default, but the host bridge can
// still be enabled for richer replies.
const HOST_AI_ENABLED: bool = cfg!(feature = "host_ai");
// Avoid double replies if host_ai is enabled.
const LOCAL_AI_ENABLED: bool = cfg!(feature = "local_ai") && !HOST_AI_ENABLED;
const LOCAL_LLM_ENABLED: bool = cfg!(feature = "local_llm") && !HOST_AI_ENABLED;

const RAYOS_VERSION_TEXT: &[u8] = b"RayOS Kernel v0.1";
const PIT_HZ: u64 = 100;

//=============================================================================
// Local learned model (TinyLM) - optional model.bin
//=============================================================================

static MODEL_PHYS: AtomicU64 = AtomicU64::new(0);
static MODEL_SIZE: AtomicU64 = AtomicU64::new(0);

#[repr(C)]
struct TinyLmHeader {
    magic: [u8; 8],   // b"RAYTLM01"
    version: u32,     // 1
    vocab: u32,       // expected 95 (printable ASCII 0x20..0x7E)
    ctx: u32,         // context window (chars)
    top_k: u32,       // recommended top-k
    rows: u32,        // vocab
    cols: u32,        // vocab
    _reserved: [u32; 2],
    // Followed by rows*cols u16 bigram weights.
}

fn tinylm_available() -> bool {
    MODEL_PHYS.load(Ordering::Relaxed) != 0 && MODEL_SIZE.load(Ordering::Relaxed) >= core::mem::size_of::<TinyLmHeader>() as u64
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalModelKind {
    None,
    TinyLm,
    RayGpt,
}

fn model_ptr_and_len() -> Option<(*const u8, usize)> {
    let phys = MODEL_PHYS.load(Ordering::Relaxed);
    if phys == 0 {
        return None;
    }
    if phys >= hhdm_phys_limit() {
        return None;
    }
    let len = MODEL_SIZE.load(Ordering::Relaxed) as usize;
    Some((phys_as_ptr::<u8>(phys), len))
}

fn local_model_kind() -> LocalModelKind {
    let (base, len) = match model_ptr_and_len() {
        Some(v) => v,
        None => return LocalModelKind::None,
    };
    if len < 8 {
        return LocalModelKind::None;
    }
    let mut magic = [0u8; 8];
    unsafe {
        for i in 0..8 {
            magic[i] = *base.add(i);
        }
    }
    if &magic == b"RAYGPT01" || &magic == b"RAYGPT02" {
        LocalModelKind::RayGpt
    } else if &magic == b"RAYTLM01" {
        LocalModelKind::TinyLm
    } else {
        LocalModelKind::None
    }
}

fn local_model_available() -> bool {
    local_model_kind() != LocalModelKind::None
}

fn tinylm_reply(prompt: &[u8], out: &mut [u8; CHAT_MAX_COLS]) -> usize {
    // Char-level bigram sampler trained offline and loaded as model.bin.
    // This is intentionally tiny but is *not* a programmed response.
    for b in out.iter_mut() {
        *b = 0;
    }

    let (base, len) = match model_ptr_and_len() {
        Some(v) => v,
        None => return copy_ascii(out, b"Local LLM: model missing (EFI/RAYOS/model.bin)."),
    };
    if len < core::mem::size_of::<TinyLmHeader>() {
        return copy_ascii(out, b"Local LLM: model invalid (too small).");
    }

    let hdr = unsafe { &*(base as *const TinyLmHeader) };
    if &hdr.magic != b"RAYTLM01" {
        return copy_ascii(out, b"Local LLM: model invalid (bad magic)." );
    }
    if hdr.version != 1 || hdr.vocab != 95 || hdr.rows != 95 || hdr.cols != 95 {
        return copy_ascii(out, b"Local LLM: model unsupported." );
    }

    let table_off = core::mem::size_of::<TinyLmHeader>();
    let table_bytes = (hdr.rows as usize)
        .saturating_mul(hdr.cols as usize)
        .saturating_mul(core::mem::size_of::<u16>());
    if len < table_off + table_bytes {
        return copy_ascii(out, b"Local LLM: model truncated." );
    }
    let table_ptr = unsafe { base.add(table_off) as *const u16 };

    // Seed RNG from prompt + timer ticks.
    let mut seed = fnv1a64(0xcbf29ce484222325, prompt);
    seed ^= TIMER_TICKS.load(Ordering::Relaxed).wrapping_mul(0x9e3779b97f4a7c15);
    let mut rng = seed;

    let ctx = if hdr.ctx == 0 { 64 } else { hdr.ctx as usize };
    let max_gen = 80usize;

    // Find last printable char in prompt as starting token.
    let mut last_tok: usize = 0; // ' '
    let mut seen = 0usize;
    let mut i = prompt.len();
    while i > 0 && seen < ctx {
        i -= 1;
        let b = prompt[i];
        if b >= 0x20 && b <= 0x7E {
            last_tok = (b - 0x20) as usize;
            break;
        }
        seen += 1;
    }

    // Prefix to make it obvious it's model-driven.
    let mut out_i = 0usize;
    out_append(out, &mut out_i, b"LLM: ");

    let top_k = if hdr.top_k == 0 { 8usize } else { hdr.top_k as usize };
    for _step in 0..max_gen {
        // Collect top-k candidates from row = last_tok
        let row_base = last_tok * 95;
        let mut best_ids = [0usize; 16];
        let mut best_w = [0u16; 16];
        let k = if top_k > best_ids.len() { best_ids.len() } else { top_k };

        for j in 0..k {
            best_ids[j] = j;
            best_w[j] = unsafe { *table_ptr.add(row_base + j) };
        }
        for cand in 0..95usize {
            let w = unsafe { *table_ptr.add(row_base + cand) };
            // find current smallest in top-k
            let mut min_idx = 0usize;
            let mut min_w = best_w[0];
            for t in 1..k {
                if best_w[t] < min_w {
                    min_w = best_w[t];
                    min_idx = t;
                }
            }
            if w > min_w {
                best_w[min_idx] = w;
                best_ids[min_idx] = cand;
            }
        }

        // Weighted sample among the selected candidates.
        let mut sum: u32 = 0;
        for t in 0..k {
            // Ensure every candidate has a non-zero chance.
            sum = sum.wrapping_add(best_w[t] as u32 + 1);
        }
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let mut r = (rng as u32) % (if sum == 0 { 1 } else { sum });
        let mut chosen = best_ids[0];
        for t in 0..k {
            let w = best_w[t] as u32 + 1;
            if r < w {
                chosen = best_ids[t];
                break;
            }
            r -= w;
        }

        let ch = (chosen as u8).wrapping_add(0x20);
        if out_i >= CHAT_MAX_COLS {
            break;
        }
        out[out_i] = ch;
        out_i += 1;
        last_tok = chosen;

        // Stop on sentence-ish boundary.
        if ch == b'.' || ch == b'!' || ch == b'?' {
            break;
        }
    }
    out_i
}

//=============================================================================
// Local learned model (RayGPT)
// - RAYGPT01: vocab=95, single-layer attention
// - RAYGPT02: vocab=256 (95 chars + learned bigrams), multi-layer attention
//=============================================================================

#[repr(C)]
struct RayGptHeader {
    magic: [u8; 8],
    version: u32,   // 1
    vocab: u32,     // 95
    ctx: u32,       // <= 64
    d_model: u32,   // 64
    n_layers: u32,  // 1
    n_heads: u32,   // 4
    d_ff: u32,      // 0 (unused)
    top_k: u32,     // recommended top-k
    _reserved: [u32; 3],
    // Followed by f32 weights in a fixed layout (see tools/gen_raygpt_model.py)
}

#[repr(C)]
struct RayGptV2Header {
    magic: [u8; 8],
    version: u32,      // 2
    vocab: u32,        // 256
    ctx: u32,          // <= 64
    d_model: u32,      // 64
    n_layers: u32,     // 1..4
    n_heads: u32,      // 4
    d_ff: u32,         // 0 (unused)
    top_k: u32,        // recommended top-k
    bigram_count: u32, // expected 161 (=256-95)
    _reserved: [u32; 2],
    // Followed by:
    // - bigram_map[95*95] u8 (0xFF => none, else index into bigram_pairs notice)
    // - bigram_pairs[bigram_count] of 2 bytes each (printable chars)
    // - padding to 4-byte alignment
    // - f32 weights (see v2 layout below)
}

const GPT1_VOCAB: usize = 95;
const GPT_CTX: usize = 64;
const GPT_D_MODEL: usize = 64;
const GPT_HEADS: usize = 4;
const GPT_DH: usize = GPT_D_MODEL / GPT_HEADS;

const GPT2_VOCAB: usize = 256;
const GPT2_BIGRAM_BASE: usize = 95;
const GPT2_BIGRAM_MAX: usize = GPT2_VOCAB - GPT2_BIGRAM_BASE; // 161
const GPT2_BIGRAM_MAP_LEN: usize = GPT1_VOCAB * GPT1_VOCAB; // 9025
const GPT_MAX_LAYERS: usize = 4;

static mut GPT_K_CACHE: [[[f32; GPT_D_MODEL]; GPT_CTX]; GPT_MAX_LAYERS] =
    [[[0.0; GPT_D_MODEL]; GPT_CTX]; GPT_MAX_LAYERS];
static mut GPT_V_CACHE: [[[f32; GPT_D_MODEL]; GPT_CTX]; GPT_MAX_LAYERS] =
    [[[0.0; GPT_D_MODEL]; GPT_CTX]; GPT_MAX_LAYERS];

#[inline(always)]
fn gpt_tok_from_ascii(b: u8) -> usize {
    if b >= 0x20 && b <= 0x7E {
        (b - 0x20) as usize
    } else {
        0
    }
}

#[inline(always)]
fn gpt_ascii_from_tok(t: usize) -> u8 {
    (t as u8).wrapping_add(0x20)
}

#[inline(always)]
fn align_up_4(x: usize) -> usize {
    (x + 3) & !3usize
}

#[inline(always)]
unsafe fn gpt_read_f32(p: *const f32, idx: usize) -> f32 {
    *p.add(idx)
}

fn raygpt_ptr_and_header() -> Option<(*const u8, usize, &'static RayGptHeader)> {
    let (base, len) = model_ptr_and_len()?;
    if len < core::mem::size_of::<RayGptHeader>() {
        return None;
    }
    let hdr = unsafe { &*(base as *const RayGptHeader) };
    if &hdr.magic != b"RAYGPT01" {
        return None;
    }
    Some((base, len, hdr))
}

fn raygpt_v2_ptr_and_header() -> Option<(*const u8, usize, &'static RayGptV2Header)> {
    let (base, len) = model_ptr_and_len()?;
    if len < core::mem::size_of::<RayGptV2Header>() {
        return None;
    }
    let hdr = unsafe { &*(base as *const RayGptV2Header) };
    if &hdr.magic != b"RAYGPT02" {
        return None;
    }
    Some((base, len, hdr))
}

fn raygpt_v1_available() -> bool {
    let (_base, len, hdr) = match raygpt_ptr_and_header() {
        Some(v) => v,
        None => return false,
    };

    if hdr.version != 1
        || hdr.vocab as usize != GPT1_VOCAB
        || hdr.ctx as usize > GPT_CTX
        || hdr.d_model as usize != GPT_D_MODEL
        || hdr.n_layers != 1
        || hdr.n_heads as usize != GPT_HEADS
        || hdr.d_ff != 0
    {
        return false;
    }

    let floats_needed: usize =
        (GPT1_VOCAB * GPT_D_MODEL) + (GPT_CTX * GPT_D_MODEL)
        + 4 * (GPT_D_MODEL * GPT_D_MODEL + GPT_D_MODEL)
        + (GPT_D_MODEL * GPT1_VOCAB) + GPT1_VOCAB;
    let bytes_needed = core::mem::size_of::<RayGptHeader>() + floats_needed * core::mem::size_of::<f32>();
    len >= bytes_needed
}

fn raygpt_v2_available() -> bool {
    let (_base, len, hdr) = match raygpt_v2_ptr_and_header() {
        Some(v) => v,
        None => return false,
    };

    if hdr.version != 2
        || hdr.vocab as usize != GPT2_VOCAB
        || hdr.ctx as usize > GPT_CTX
        || hdr.d_model as usize != GPT_D_MODEL
        || (hdr.n_layers as usize) == 0
        || (hdr.n_layers as usize) > GPT_MAX_LAYERS
        || hdr.n_heads as usize != GPT_HEADS
        || hdr.d_ff != 0
        || hdr.bigram_count as usize != GPT2_BIGRAM_MAX
    {
        return false;
    }

    let tables_bytes = GPT2_BIGRAM_MAP_LEN + (GPT2_BIGRAM_MAX * 2);
    let weights_off = align_up_4(core::mem::size_of::<RayGptV2Header>() + tables_bytes);
    let layers = hdr.n_layers as usize;
    let floats_needed: usize =
        // token_emb[vocab,d] + pos_emb[ctx,d]
        (GPT2_VOCAB * GPT_D_MODEL) + (GPT_CTX * GPT_D_MODEL)
        // per-layer: Wq/bq, Wk/bk, Wv/bv, Wo/bo
        + layers * (4 * (GPT_D_MODEL * GPT_D_MODEL + GPT_D_MODEL))
        // output
        + (GPT_D_MODEL * GPT2_VOCAB) + GPT2_VOCAB;
    let bytes_needed = weights_off + floats_needed * core::mem::size_of::<f32>();
    len >= bytes_needed
}

fn gpt_softmax_in_place(scores: &mut [f32; GPT_CTX], n: usize) {
    // Numerically stable softmax.
    let mut maxv = scores[0];
    for i in 1..n {
        if scores[i] > maxv {
            maxv = scores[i];
        }
    }
    let mut sum = 0.0f32;
    for i in 0..n {
        let e = expf(scores[i] - maxv);
        scores[i] = e;
        sum += e;
    }
    if sum <= 0.0 {
        let inv = 1.0f32 / (n as f32);
        for i in 0..n {
            scores[i] = inv;
        }
        return;
    }
    let inv = 1.0f32 / sum;
    for i in 0..n {
        scores[i] *= inv;
    }
}

fn gpt_matvec64(w: *const f32, x: &[f32; GPT_D_MODEL], b: *const f32, out: &mut [f32; GPT_D_MODEL]) {
    // Training uses y = x @ W + b where W is [in,out] (row-major).
    // So y[c] = sum_r x[r] * W[r,c] + b[c].
    for c in 0..GPT_D_MODEL {
        let mut acc = unsafe { gpt_read_f32(b, c) };
        for r in 0..GPT_D_MODEL {
            let wrc = unsafe { gpt_read_f32(w, r * GPT_D_MODEL + c) };
            acc += x[r] * wrc;
        }
        out[c] = acc;
    }
}

fn gpt_logits95(wout: *const f32, bout: *const f32, x: &[f32; GPT_D_MODEL], logits: &mut [f32; GPT1_VOCAB]) {
    // wout is row-major [d, vocab]
    for v in 0..GPT1_VOCAB {
        let mut acc = unsafe { gpt_read_f32(bout, v) };
        for d in 0..GPT_D_MODEL {
            let w = unsafe { gpt_read_f32(wout, d * GPT1_VOCAB + v) };
            acc += w * x[d];
        }
        logits[v] = acc;
    }
}

fn raygpt_forward_step(
    weights: *const f32,
    pos: usize,
    tok: usize,
    logits_out: &mut [f32; GPT1_VOCAB],
) {
    // Weight layout (all f32), immediately after header:
    // token_emb[vocab,d], pos_emb[ctx,d], Wq[d,d],bq[d], Wk,bk, Wv,bv, Wo,bo, Wout[d,vocab], bout[vocab]
    let mut off = 0usize;
    let token_emb = unsafe { weights.add(off) };
    off += GPT1_VOCAB * GPT_D_MODEL;
    let pos_emb = unsafe { weights.add(off) };
    off += GPT_CTX * GPT_D_MODEL;

    let wq = unsafe { weights.add(off) };
    off += GPT_D_MODEL * GPT_D_MODEL;
    let bq = unsafe { weights.add(off) };
    off += GPT_D_MODEL;

    let wk = unsafe { weights.add(off) };
    off += GPT_D_MODEL * GPT_D_MODEL;
    let bk = unsafe { weights.add(off) };
    off += GPT_D_MODEL;

    let wv = unsafe { weights.add(off) };
    off += GPT_D_MODEL * GPT_D_MODEL;
    let bv = unsafe { weights.add(off) };
    off += GPT_D_MODEL;

    let wo = unsafe { weights.add(off) };
    off += GPT_D_MODEL * GPT_D_MODEL;
    let bo = unsafe { weights.add(off) };
    off += GPT_D_MODEL;

    let wout = unsafe { weights.add(off) };
    off += GPT_D_MODEL * GPT1_VOCAB;
    let bout = unsafe { weights.add(off) };

    // x = token_emb[tok] + pos_emb[pos]
    let mut x = [0.0f32; GPT_D_MODEL];
    let te = tok * GPT_D_MODEL;
    let pe = pos * GPT_D_MODEL;
    for i in 0..GPT_D_MODEL {
        x[i] = unsafe { gpt_read_f32(token_emb, te + i) } + unsafe { gpt_read_f32(pos_emb, pe + i) };
    }

    // q,k,v
    let mut q = [0.0f32; GPT_D_MODEL];
    let mut k = [0.0f32; GPT_D_MODEL];
    let mut v = [0.0f32; GPT_D_MODEL];
    gpt_matvec64(wq, &x, bq, &mut q);
    gpt_matvec64(wk, &x, bk, &mut k);
    gpt_matvec64(wv, &x, bv, &mut v);

    unsafe {
        if pos < GPT_CTX {
            GPT_K_CACHE[0][pos] = k;
            GPT_V_CACHE[0][pos] = v;
        }
    }

    // Attention: per-head softmax over [0..pos]
    let inv_sqrt = 1.0f32 / sqrtf(GPT_DH as f32);
    let mut attn_out = [0.0f32; GPT_D_MODEL];

    for h in 0..GPT_HEADS {
        let h0 = h * GPT_DH;
        let mut scores = [0.0f32; GPT_CTX];
        let n = (pos + 1).min(GPT_CTX);

        for t in 0..n {
            let mut dot = 0.0f32;
            unsafe {
                for j in 0..GPT_DH {
                    dot += q[h0 + j] * GPT_K_CACHE[0][t][h0 + j];
                }
            }
            scores[t] = dot * inv_sqrt;
        }

        gpt_softmax_in_place(&mut scores, n);

        for t in 0..n {
            let w = scores[t];
            unsafe {
                for j in 0..GPT_DH {
                    attn_out[h0 + j] += w * GPT_V_CACHE[0][t][h0 + j];
                }
            }
        }
    }

    // y = Wo*attn_out + bo ; x2 = x + y
    let mut y = [0.0f32; GPT_D_MODEL];
    gpt_matvec64(wo, &attn_out, bo, &mut y);
    for i in 0..GPT_D_MODEL {
        x[i] += y[i];
    }

    gpt_logits95(wout, bout, &x, logits_out);
}

fn raygpt2_forward_step(
    weights: *const f32,
    layers: usize,
    pos: usize,
    tok: usize,
    logits_out: &mut [f32; GPT2_VOCAB],
) {
    // Weight layout (all f32), at weights pointer:
    // token_emb[256,64]
    // pos_emb[64,64]
    // repeated for each layer L:
    //   Wq[64,64], bq[64]
    //   Wk[64,64], bk[64]
    //   Wv[64,64], bv[64]
    //   Wo[64,64], bo[64]
    // Wout[64,256], bout[256]

    let mut off = 0usize;
    let token_emb = unsafe { weights.add(off) };
    off += GPT2_VOCAB * GPT_D_MODEL;
    let pos_emb = unsafe { weights.add(off) };
    off += GPT_CTX * GPT_D_MODEL;

    // x0 = token_emb[tok] + pos_emb[pos]
    let mut x = [0.0f32; GPT_D_MODEL];
    let te = tok * GPT_D_MODEL;
    let pe = pos * GPT_D_MODEL;
    for i in 0..GPT_D_MODEL {
        x[i] = unsafe { gpt_read_f32(token_emb, te + i) } + unsafe { gpt_read_f32(pos_emb, pe + i) };
    }

    let inv_sqrt = 1.0f32 / sqrtf(GPT_DH as f32);

    for layer in 0..layers {
        let wq = unsafe { weights.add(off) };
        off += GPT_D_MODEL * GPT_D_MODEL;
        let bq = unsafe { weights.add(off) };
        off += GPT_D_MODEL;

        let wk = unsafe { weights.add(off) };
        off += GPT_D_MODEL * GPT_D_MODEL;
        let bk = unsafe { weights.add(off) };
        off += GPT_D_MODEL;

        let wv = unsafe { weights.add(off) };
        off += GPT_D_MODEL * GPT_D_MODEL;
        let bv = unsafe { weights.add(off) };
        off += GPT_D_MODEL;

        let wo = unsafe { weights.add(off) };
        off += GPT_D_MODEL * GPT_D_MODEL;
        let bo = unsafe { weights.add(off) };
        off += GPT_D_MODEL;

        let mut q = [0.0f32; GPT_D_MODEL];
        let mut k = [0.0f32; GPT_D_MODEL];
        let mut v = [0.0f32; GPT_D_MODEL];
        gpt_matvec64(wq, &x, bq, &mut q);
        gpt_matvec64(wk, &x, bk, &mut k);
        gpt_matvec64(wv, &x, bv, &mut v);

        unsafe {
            if pos < GPT_CTX {
                GPT_K_CACHE[layer][pos] = k;
                GPT_V_CACHE[layer][pos] = v;
            }
        }

        let mut attn_out = [0.0f32; GPT_D_MODEL];
        for h in 0..GPT_HEADS {
            let h0 = h * GPT_DH;
            let mut scores = [0.0f32; GPT_CTX];
            let n = (pos + 1).min(GPT_CTX);

            for t in 0..n {
                let mut dot = 0.0f32;
                unsafe {
                    for j in 0..GPT_DH {
                        dot += q[h0 + j] * GPT_K_CACHE[layer][t][h0 + j];
                    }
                }
                scores[t] = dot * inv_sqrt;
            }
            gpt_softmax_in_place(&mut scores, n);
            for t in 0..n {
                let w = scores[t];
                unsafe {
                    for j in 0..GPT_DH {
                        attn_out[h0 + j] += w * GPT_V_CACHE[layer][t][h0 + j];
                    }
                }
            }
        }

        let mut y = [0.0f32; GPT_D_MODEL];
        gpt_matvec64(wo, &attn_out, bo, &mut y);
        for i in 0..GPT_D_MODEL {
            x[i] += y[i];
        }
    }

    // Output projection pointers are after per-layer weights.
    let wout = unsafe { weights.add(off) };
    off += GPT_D_MODEL * GPT2_VOCAB;
    let bout = unsafe { weights.add(off) };
    gpt_logits_256(wout, bout, &x, logits_out);
}

fn gpt_logits_256(wout: *const f32, bout: *const f32, x: &[f32; GPT_D_MODEL], logits: &mut [f32; GPT2_VOCAB]) {
    for v in 0..GPT2_VOCAB {
        let mut acc = unsafe { gpt_read_f32(bout, v) };
        for d in 0..GPT_D_MODEL {
            let w = unsafe { gpt_read_f32(wout, d * GPT2_VOCAB + v) };
            acc += w * x[d];
        }
        logits[v] = acc;
    }
}

fn gpt2_tokenize_prompt(
    prompt: &[u8],
    bigram_map: *const u8,
    out_tokens: &mut [usize; GPT_CTX],
    max_tokens: usize,
) -> usize {
    // Greedy bigram tokenization over printable ASCII.
    let mut tmp = [0usize; GPT_CTX * 2];
    let mut n = 0usize;

    let mut i = 0usize;
    while i < prompt.len() && n < tmp.len() {
        let b = prompt[i];
        if b < 0x20 || b > 0x7E {
            i += 1;
            continue;
        }
        let t1 = (b - 0x20) as usize;
        // find next printable
        let mut j = i + 1;
        while j < prompt.len() {
            let b2 = prompt[j];
            if b2 >= 0x20 && b2 <= 0x7E {
                let t2 = (b2 - 0x20) as usize;
                let idx = t1 * GPT1_VOCAB + t2;
                let bi = unsafe { *bigram_map.add(idx) };
                if bi != 0xFF {
                    tmp[n] = GPT2_BIGRAM_BASE + (bi as usize);
                    n += 1;
                    i = j + 1;
                    break;
                }
                // no bigram
                tmp[n] = t1;
                n += 1;
                i += 1;
                break;
            }
            j += 1;
        }
        if j >= prompt.len() {
            tmp[n] = t1;
            n += 1;
            break;
        }
    }

    if n == 0 {
        out_tokens[0] = 0;
        return 1;
    }

    let take = if n > max_tokens { max_tokens } else { n };
    let start = n - take;
    for k in 0..take {
        out_tokens[k] = tmp[start + k];
    }
    take
}

fn gpt2_decode_token(tok: usize, pairs: *const u8, out: &mut [u8; CHAT_MAX_COLS], out_i: &mut usize) {
    if *out_i >= CHAT_MAX_COLS {
        return;
    }
    if tok < GPT2_BIGRAM_BASE {
        out[*out_i] = gpt_ascii_from_tok(tok);
        *out_i += 1;
        return;
    }
    let bi = tok - GPT2_BIGRAM_BASE;
    if bi >= GPT2_BIGRAM_MAX {
        return;
    }
    let a = unsafe { *pairs.add(bi * 2) };
    let b = unsafe { *pairs.add(bi * 2 + 1) };
    if *out_i < CHAT_MAX_COLS {
        out[*out_i] = a;
        *out_i += 1;
    }
    if *out_i < CHAT_MAX_COLS {
        out[*out_i] = b;
        *out_i += 1;
    }
}

fn raygpt_reply(prompt: &[u8], out: &mut [u8; CHAT_MAX_COLS]) -> usize {
    for b in out.iter_mut() {
        *b = 0;
    }

    let (base, len, hdr) = match raygpt_ptr_and_header() {
        Some(v) => v,
        None => return copy_ascii(out, b"Local LLM: model missing/invalid (EFI/RAYOS/model.bin)."),
    };
    if hdr.version != 1 {
        return copy_ascii(out, b"Local LLM: model unsupported." );
    }
    let ctx = (hdr.ctx as usize).min(GPT_CTX);
    let top_k = if hdr.top_k == 0 { 12usize } else { (hdr.top_k as usize).min(GPT1_VOCAB) };

    // Weights start right after header.
    let weights_off = core::mem::size_of::<RayGptHeader>();
    if len < weights_off {
        return copy_ascii(out, b"Local LLM: model truncated." );
    }
    let weights = unsafe { base.add(weights_off) as *const f32 };

    // Seed RNG from prompt + timer ticks.
    let mut seed = fnv1a64(0xcbf29ce484222325, prompt);
    seed ^= TIMER_TICKS.load(Ordering::Relaxed).wrapping_mul(0x9e3779b97f4a7c15);
    let mut rng = seed;

    // Extract up to ctx printable ASCII tokens from the end of the prompt.
    let mut tokens = [0usize; GPT_CTX];
    let mut tlen = 0usize;
    let mut i = prompt.len();
    while i > 0 && tlen < ctx {
        i -= 1;
        let b = prompt[i];
        if b >= 0x20 && b <= 0x7E {
            tokens[ctx - 1 - tlen] = gpt_tok_from_ascii(b);
            tlen += 1;
        }
    }
    // Left-align.
    let start = ctx - tlen;
    for j in 0..tlen {
        tokens[j] = tokens[start + j];
    }

    // Reset KV cache.
    unsafe {
        for p in 0..GPT_CTX {
            for d in 0..GPT_D_MODEL {
                GPT_K_CACHE[0][p][d] = 0.0;
                GPT_V_CACHE[0][p][d] = 0.0;
            }
        }
    }

    // Prime the cache with prompt tokens.
    let mut logits = [0.0f32; GPT1_VOCAB];
    if tlen == 0 {
        tokens[0] = 0; // space
        tlen = 1;
    }
    let mut pos = 0usize;
    while pos < tlen && pos < ctx {
        raygpt_forward_step(weights, pos, tokens[pos], &mut logits);
        pos += 1;
    }

    // Prefix.
    let mut out_i = 0usize;
    out_append(out, &mut out_i, b"GPT: ");

    // Generate (cap so we stay within ctx; keep it short for UI stability).
    let max_gen = 48usize;
    let mut cur_tok = tokens[(tlen - 1).min(ctx - 1)];
    let mut gen_pos = tlen.min(ctx) - 1;

    for _ in 0..max_gen {
        // Predict next token from current token at current position.
        raygpt_forward_step(weights, gen_pos, cur_tok, &mut logits);

        // Top-k sampling from softmax(logits).
        // Find top-k indices by logit.
        let mut best_ids = [0usize; 16];
        let mut best_vals = [-1.0e30f32; 16];
        let k = if top_k > best_ids.len() { best_ids.len() } else { top_k };
        for i in 0..k {
            best_ids[i] = i;
            best_vals[i] = logits[i];
        }
        for cand in 0..GPT1_VOCAB {
            let v = logits[cand];
            let mut min_i = 0usize;
            let mut min_v = best_vals[0];
            for j in 1..k {
                if best_vals[j] < min_v {
                    min_v = best_vals[j];
                    min_i = j;
                }
            }
            if v > min_v {
                best_vals[min_i] = v;
                best_ids[min_i] = cand;
            }
        }

        // Softmax over top-k.
        let mut maxv = best_vals[0];
        for j in 1..k {
            if best_vals[j] > maxv {
                maxv = best_vals[j];
            }
        }
        let mut probs = [0.0f32; 16];
        let mut sum = 0.0f32;
        for j in 0..k {
            let e = expf(best_vals[j] - maxv);
            probs[j] = e;
            sum += e;
        }
        if sum <= 0.0 {
            probs[0] = 1.0;
            sum = 1.0;
        }
        let inv = 1.0f32 / sum;
        for j in 0..k {
            probs[j] *= inv;
        }

        // Sample.
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let r01 = ((rng as u32) as f32) / (u32::MAX as f32);
        let mut acc = 0.0f32;
        let mut chosen = best_ids[0];
        for j in 0..k {
            acc += probs[j];
            if r01 <= acc {
                chosen = best_ids[j];
                break;
            }
        }

        let ch = gpt_ascii_from_tok(chosen);
        if out_i >= CHAT_MAX_COLS {
            break;
        }
        out[out_i] = ch;
        out_i += 1;

        cur_tok = chosen;
        if gen_pos + 1 < ctx {
            gen_pos += 1;
        }

        if ch == b'.' || ch == b'!' || ch == b'?' {
            break;
        }
    }

    out_i
}

fn raygpt2_reply(prompt: &[u8], out: &mut [u8; CHAT_MAX_COLS]) -> usize {
    for b in out.iter_mut() {
        *b = 0;
    }

    let (base, len, hdr) = match raygpt_v2_ptr_and_header() {
        Some(v) => v,
        None => return copy_ascii(out, b"Local LLM: model missing/invalid (EFI/RAYOS/model.bin)."),
    };
    if !raygpt_v2_available() {
        return copy_ascii(out, b"Local LLM: model unsupported." );
    }

    let ctx = (hdr.ctx as usize).min(GPT_CTX);
    let layers = hdr.n_layers as usize;
    let top_k = if hdr.top_k == 0 { 24usize } else { (hdr.top_k as usize).min(GPT2_VOCAB) };

    let tables_off = core::mem::size_of::<RayGptV2Header>();
    let bigram_map = unsafe { base.add(tables_off) as *const u8 };
    let pairs_off = tables_off + GPT2_BIGRAM_MAP_LEN;
    let bigram_pairs = unsafe { base.add(pairs_off) as *const u8 };
    let weights_off = align_up_4(pairs_off + (GPT2_BIGRAM_MAX * 2));
    if len < weights_off {
        return copy_ascii(out, b"Local LLM: model truncated." );
    }
    let weights = unsafe { base.add(weights_off) as *const f32 };

    // Reset KV cache.
    unsafe {
        for l in 0..GPT_MAX_LAYERS {
            for p in 0..GPT_CTX {
                for d in 0..GPT_D_MODEL {
                    GPT_K_CACHE[l][p][d] = 0.0;
                    GPT_V_CACHE[l][p][d] = 0.0;
                }
            }
        }
    }

    // Tokenize prompt.
    let mut tokens = [0usize; GPT_CTX];
    let tlen = gpt2_tokenize_prompt(prompt, bigram_map, &mut tokens, ctx);

    // Seed RNG.
    let mut seed = fnv1a64(0xcbf29ce484222325, prompt);
    seed ^= TIMER_TICKS.load(Ordering::Relaxed).wrapping_mul(0x9e3779b97f4a7c15);
    let mut rng = seed;

    // Prime cache.
    let mut logits = [0.0f32; GPT2_VOCAB];
    let mut pos = 0usize;
    while pos < tlen && pos < ctx {
        let tok = tokens[pos];
        raygpt2_forward_step(weights, layers, pos, tok, &mut logits);
        pos += 1;
    }

    let mut out_i = 0usize;
    out_append(out, &mut out_i, b"GPT: ");

    let max_gen = 48usize;
    let mut cur_tok = tokens[(tlen - 1).min(ctx - 1)];
    let mut gen_pos = tlen.min(ctx) - 1;

    for _ in 0..max_gen {
        raygpt2_forward_step(weights, layers, gen_pos, cur_tok, &mut logits);

        // Top-k by logits.
        let mut best_ids = [0usize; 32];
        let mut best_vals = [-1.0e30f32; 32];
        let k = if top_k > best_ids.len() { best_ids.len() } else { top_k };
        for i in 0..k {
            best_ids[i] = i;
            best_vals[i] = logits[i];
        }
        for cand in 0..GPT2_VOCAB {
            let v = logits[cand];
            let mut min_i = 0usize;
            let mut min_v = best_vals[0];
            for j in 1..k {
                if best_vals[j] < min_v {
                    min_v = best_vals[j];
                    min_i = j;
                }
            }
            if v > min_v {
                best_vals[min_i] = v;
                best_ids[min_i] = cand;
            }
        }

        // Softmax over top-k.
        let mut maxv = best_vals[0];
        for j in 1..k {
            if best_vals[j] > maxv {
                maxv = best_vals[j];
            }
        }
        let mut probs = [0.0f32; 32];
        let mut sum = 0.0f32;
        for j in 0..k {
            let e = expf(best_vals[j] - maxv);
            probs[j] = e;
            sum += e;
        }
        if sum <= 0.0 {
            probs[0] = 1.0;
            sum = 1.0;
        }
        let inv = 1.0f32 / sum;
        for j in 0..k {
            probs[j] *= inv;
        }

        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let r01 = ((rng as u32) as f32) / (u32::MAX as f32);
        let mut acc = 0.0f32;
        let mut chosen = best_ids[0];
        for j in 0..k {
            acc += probs[j];
            if r01 <= acc {
                chosen = best_ids[j];
                break;
            }
        }

        gpt2_decode_token(chosen, bigram_pairs, out, &mut out_i);

        cur_tok = chosen;
        if gen_pos + 1 < ctx {
            gen_pos += 1;
        }

        // Stop on sentence-ish boundary if the last emitted char is punctuation.
        if out_i > 0 {
            let last = out[out_i - 1];
            if last == b'.' || last == b'!' || last == b'?' {
                break;
            }
        }
        if out_i >= CHAT_MAX_COLS {
            break;
        }
    }

    out_i
}

fn local_model_reply(prompt: &[u8], out: &mut [u8; CHAT_MAX_COLS]) -> usize {
    match local_model_kind() {
        LocalModelKind::RayGpt => {
            if raygpt_v2_available() {
                raygpt2_reply(prompt, out)
            } else if raygpt_v1_available() {
                raygpt_reply(prompt, out)
            } else {
                copy_ascii(out, b"Local LLM: RayGPT model invalid/unsupported.")
            }
        }
        LocalModelKind::TinyLm => tinylm_reply(prompt, out),
        LocalModelKind::None => copy_ascii(out, b"Local LLM: model missing (EFI/RAYOS/model.bin)."),
    }
}

fn out_append(out: &mut [u8; CHAT_MAX_COLS], idx: &mut usize, text: &[u8]) {
    for &b in text.iter() {
        if *idx >= CHAT_MAX_COLS {
            return;
        }
        if b >= 0x20 && b <= 0x7E {
            out[*idx] = b;
            *idx += 1;
        }
    }
}

fn out_append_u64_dec(out: &mut [u8; CHAT_MAX_COLS], idx: &mut usize, mut value: u64) {
    // ASCII decimal, no allocations.
    let mut tmp = [0u8; 20];
    let mut n = 0usize;
    if value == 0 {
        out_append(out, idx, b"0");
        return;
    }
    while value != 0 && n < tmp.len() {
        let d = (value % 10) as u8;
        tmp[n] = b'0' + d;
        n += 1;
        value /= 10;
    }
    while n > 0 {
        n -= 1;
        if *idx >= CHAT_MAX_COLS {
            return;
        }
        out[*idx] = tmp[n];
        *idx += 1;
    }
}

fn first_word_lower(input: &[u8]) -> (u8, usize) {
    let mut i = 0usize;
    while i < input.len() && input[i] == b' ' {
        i += 1;
    }
    if i >= input.len() {
        return (0, 0);
    }
    (ascii_lower(input[i]), i)
}

fn starts_with_word(input: &[u8], word: &[u8]) -> bool {
    if input.len() < word.len() {
        return false;
    }
    for i in 0..word.len() {
        if ascii_lower(input[i]) != word[i] {
            return false;
        }
    }
    true
}

fn normalize_ascii_for_match(input: &[u8], out: &mut [u8]) -> usize {
    // Lowercase and collapse all non-alnum into single spaces.
    // This makes matching robust to punctuation and repeated whitespace.
    let mut n = 0usize;
    let mut prev_space = true;
    for &b in input.iter() {
        let c = ascii_lower(b);
        let is_alnum = (b'a'..=b'z').contains(&c) || (b'0'..=b'9').contains(&c);
        if is_alnum {
            if n >= out.len() {
                break;
            }
            out[n] = c;
            n += 1;
            prev_space = false;
        } else {
            if !prev_space {
                if n >= out.len() {
                    break;
                }
                out[n] = b' ';
                n += 1;
                prev_space = true;
            }
        }
    }
    while n > 0 && out[n - 1] == b' ' {
        n -= 1;
    }
    n
}

fn local_ai_reply(input: &[u8], out: &mut [u8; CHAT_MAX_COLS]) -> usize {
    // Very small, deterministic responder that runs entirely inside RayOS.
    // Output is kept ASCII-only and single-line.
    for b in out.iter_mut() {
        *b = 0;
    }

    // Trim.
    let mut start = 0usize;
    let mut end = input.len();
    while start < end && input[start] == b' ' {
        start += 1;
    }
    while end > start && input[end - 1] == b' ' {
        end -= 1;
    }
    let s = &input[start..end];
    if s.is_empty() {
        return copy_ascii(out, b"Say something.");
    }

    // Normalize for matching (lowercase + collapsed spaces/punctuation).
    let mut norm = [0u8; CHAT_MAX_COLS];
    let norm_len = normalize_ascii_for_match(s, &mut norm);
    let sn = &norm[..norm_len];

    // Greetings / small talk.
    let (first, _) = first_word_lower(sn);
    if first == b'h' {
        if starts_with_word(sn, b"hi") || starts_with_word(sn, b"hello") || starts_with_word(sn, b"hey") {
            return copy_ascii(out, b"Hi. Local AI is online.");
        }
    }
    if first == b't' {
        if starts_with_word(sn, b"thanks") || starts_with_word(sn, b"thank") || starts_with_word(sn, b"thx") {
            return copy_ascii(out, b"You're welcome.");
        }
    }
    if first == b'b' {
        if starts_with_word(sn, b"bye") {
            return copy_ascii(out, b"OK. Standing by.");
        }
    }

    // Help / capabilities.
    if first == b'h' {
        if starts_with_word(sn, b"help") {
            return copy_ascii(out, b"Local AI: chat + guidance. For shell commands, type :help.");
        }
    }
    if first == b'w' {
        if bytes_contains_ci(sn, b"what can you do") {
            return copy_ascii(out, b"I can answer questions and guide debugging. Type :help for shell.");
        }
    }
    if first == b'c' {
        if bytes_contains_ci(sn, b"capabilities") {
            return copy_ascii(out, b"Capabilities: chat, basic troubleshooting, and guidance. Try :help.");
        }
    }

    // Basic questions answered with *real* kernel state.
    // This makes the local AI feel alive even before a full local LLM runtime exists.
    if bytes_contains_ci(sn, b"how old are you")
        || bytes_contains_ci(sn, b"when did you boot")
        || bytes_contains_ci(sn, b"when did you boot up")
        || bytes_contains_ci(sn, b"uptime")
        || bytes_contains_ci(sn, b"up time")
        || bytes_contains_ci(sn, b"how long")
    {
        let ticks = TIMER_TICKS.load(Ordering::Relaxed);
        let secs = ticks / PIT_HZ;
        let mut i = 0usize;
        out_append(out, &mut i, b"Uptime ~");
        out_append_u64_dec(out, &mut i, secs);
        out_append(out, &mut i, b"s (ticks=");
        out_append_u64_dec(out, &mut i, ticks);
        out_append(out, &mut i, b").");
        return i;
    }
    if bytes_contains_ci(sn, b"version") || bytes_contains_ci(sn, b"what are you") || bytes_contains_ci(sn, b"who are you") {
        let mut i = 0usize;
        out_append(out, &mut i, RAYOS_VERSION_TEXT);
        out_append(out, &mut i, b" local AI (in-guest). Type help.");
        return i;
    }

    // Status / telemetry.
    if bytes_contains_ci(sn, b"status") || bytes_contains_ci(sn, b"health") {
        let ticks = TIMER_TICKS.load(Ordering::Relaxed);
        let secs = ticks / PIT_HZ;
        let s1_run = if SYSTEM1_RUNNING.load(Ordering::Relaxed) { 1u64 } else { 0u64 };
        let qd = rayq_depth() as u64;
        let done = SYSTEM1_PROCESSED.load(Ordering::Relaxed);
        let op = SYSTEM2_LAST_OP.load(Ordering::Relaxed);
        let pr = SYSTEM2_LAST_PRIO.load(Ordering::Relaxed);
        let vol = if VOLUME_READY.load(Ordering::Relaxed) { 1u64 } else { 0u64 };

        let mut i = 0usize;
        out_append(out, &mut i, b"Status: up~");
        out_append_u64_dec(out, &mut i, secs);
        out_append(out, &mut i, b"s s1=");
        out_append_u64_dec(out, &mut i, s1_run);
        out_append(out, &mut i, b" q=");
        out_append_u64_dec(out, &mut i, qd);
        out_append(out, &mut i, b" done=");
        out_append_u64_dec(out, &mut i, done);
        out_append(out, &mut i, b" s2(op=");
        out_append_u64_dec(out, &mut i, op);
        out_append(out, &mut i, b" pr=");
        out_append_u64_dec(out, &mut i, pr);
        out_append(out, &mut i, b") vol=");
        out_append_u64_dec(out, &mut i, vol);
        return i;
    }

    // Calendar/time questions (UEFI provides wall-clock; we keep time using uptime).
    if bytes_contains_ci(sn, b"today")
        || bytes_contains_ci(sn, b"date")
        || bytes_contains_ci(sn, b"what day is it")
        || bytes_contains_ci(sn, b"day of the week")
        || bytes_contains_ci(sn, b"weekday")
        || bytes_contains_ci(sn, b"time")
        || bytes_contains_ci(sn, b"clock")
    {
        if let Some(now) = current_unix_seconds_utc() {
            let days = (now / 86_400) as i64;
            let sod = (now % 86_400) as u32;
            let hour = sod / 3_600;
            let min = (sod / 60) % 60;
            let sec = sod % 60;

            let (year, month, day) = civil_from_days(days);
            let wd = weekday_index_sun0(days);

            let mut i = 0usize;
            out_append(out, &mut i, b"UTC ");
            append_4digits(out, &mut i, year as u32);
            out_append(out, &mut i, b"-");
            append_2digits(out, &mut i, month);
            out_append(out, &mut i, b"-");
            append_2digits(out, &mut i, day);
            out_append(out, &mut i, b" ");
            append_2digits(out, &mut i, hour);
            out_append(out, &mut i, b":");
            append_2digits(out, &mut i, min);
            out_append(out, &mut i, b":");
            append_2digits(out, &mut i, sec);
            out_append(out, &mut i, b" Weekday ");
            out_append(out, &mut i, weekday_name(wd));
            return i;
        }

        // Fallback if UEFI time is unavailable.
        let ticks = TIMER_TICKS.load(Ordering::Relaxed);
        let secs = ticks / PIT_HZ;
        let mut i = 0usize;
        out_append(out, &mut i, b"Time unavailable. Uptime about ");
        out_append_u64_dec(out, &mut i, secs);
        out_append(out, &mut i, b"s.");
        return i;
    }

    if bytes_contains_ci(sn, b"memory") || bytes_contains_ci(sn, b"heap") {
        let heap_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed) as u64;
        let heap_kb = heap_bytes / 1024;
        let mut i = 0usize;
        out_append(out, &mut i, b"Memory: heap_used~");
        out_append_u64_dec(out, &mut i, heap_kb);
        out_append(out, &mut i, b"KB (");
        out_append_u64_dec(out, &mut i, heap_bytes);
        out_append(out, &mut i, b" bytes). Type :mem for details.");
        return i;
    }

    // Hardware / device inventory (grounded).
    if bytes_contains_ci(sn, b"devices") || bytes_contains_ci(sn, b"device") || bytes_contains_ci(sn, b"hardware") {
        let fb_ok = unsafe { FB_BASE } != 0 && unsafe { FB_WIDTH } != 0 && unsafe { FB_HEIGHT } != 0;
        let lapic_ok = unsafe { LAPIC_MMIO } != 0;
        let ioapic_ok = unsafe { IOAPIC_MMIO } != 0;
        let vol_ok = VOLUME_READY.load(Ordering::Relaxed);

        let mut i = 0usize;
        out_append(out, &mut i, b"Devices: serial=ok pit=ok kbd=ok fb=");
        out_append(out, &mut i, if fb_ok { b"ok" } else { b"none" });
        out_append(out, &mut i, b" apic=");
        out_append(out, &mut i, if lapic_ok { b"lapic" } else { b"none" });
        out_append(out, &mut i, b"/");
        out_append(out, &mut i, if ioapic_ok { b"ioapic" } else { b"none" });
        out_append(out, &mut i, b" storage=");
        out_append(out, &mut i, if vol_ok { b"virtio-blk" } else { b"missing" });
        out_append(out, &mut i, b".");
        return i;
    }

    // Volume Q&A (grounded in actual device detection).
    if bytes_contains_ci(sn, b"volume") {
        if !VOLUME_READY.load(Ordering::Relaxed) {
            return copy_ascii(out, b"Volume is missing: no virtio-blk device detected in this boot.");
        }
        let cap = VOLUME_CAPACITY_SECTORS.load(Ordering::Relaxed);
        let mut i = 0usize;
        out_append(out, &mut i, b"Volume ready: capacity=");
        out_append_u64_dec(out, &mut i, cap);
        out_append(out, &mut i, b" sectors.");
        return i;
    }

    // Files / storage questions (answer honestly and with current state).
    // RayOS doesn't have a filesystem layer exposed here yet, but we can ground the answer
    // in the actual volume detection status.
    if bytes_contains_ci(sn, b"file") || bytes_contains_ci(sn, b"files") {
        if !VOLUME_READY.load(Ordering::Relaxed) {
            return copy_ascii(
                out,
                b"Files: I can't count files yet (no filesystem layer), and this boot has no volume (virtio-blk not detected).",
            );
        }
        let cap = VOLUME_CAPACITY_SECTORS.load(Ordering::Relaxed);
        let mut i = 0usize;
        out_append(out, &mut i, b"Files: I can't count files yet (no filesystem layer). Volume is present; capacity=");
        out_append_u64_dec(out, &mut i, cap);
        out_append(out, &mut i, b" sectors.");
        return i;
    }

    // Tiny local FAQ for subsystem concepts (non-generative but helpful).
    if bytes_contains_ci(sn, b"system 1") {
        return copy_ascii(out, b"System 1: fast loop that processes logic rays each tick (reflex engine)." );
    }
    if bytes_contains_ci(sn, b"system 2") {
        return copy_ascii(out, b"System 2: parses your text into logic rays (cognitive engine stub)." );
    }
    if bytes_contains_ci(sn, b"conductor") {
        return copy_ascii(out, b"Conductor: orchestrates work by feeding System 2 when idle." );
    }
    if bytes_contains_ci(sn, b"intent") {
        return copy_ascii(out, b"Intent: lightweight parser that classifies your input (chat vs task)." );
    }

    // Frustration marker.
    if bytes_contains_ci(sn, b"sucks") || bytes_contains_ci(sn, b"broken") || bytes_contains_ci(sn, b"wtf") {
        return copy_ascii(out, b"Got it. What did you expect, and what happened instead?");
    }

    // Task-like keywords we can't execute locally (yet).
    if starts_with_word(sn, b"search ") || starts_with_word(sn, b"find ") {
        return copy_ascii(out, b"Local AI can't search files yet. Tell me what to look for.");
    }
    if starts_with_word(sn, b"index ") {
        return copy_ascii(out, b"Local AI can't index yet. Tell me your goal, and I'll suggest steps.");
    }

    // Default: if a learned model is available, use it for a non-canned reply.
    if LOCAL_LLM_ENABLED && local_model_available() {
        // Provide a stable chat-style prompt prefix to help the small model.
        // Keep it ASCII-only and bounded.
        const MODEL_PROMPT_MAX: usize = 192;
        let mut buf = [0u8; MODEL_PROMPT_MAX];
        let mut n = 0usize;

        for &b in b"YOU: ".iter() {
            if n >= buf.len() {
                break;
            }
            buf[n] = b;
            n += 1;
        }
        for &b in sn.iter() {
            if n >= buf.len() {
                break;
            }
            // sn is already normalized to printable-ish; keep it strictly printable.
            if b >= 0x20 && b <= 0x7E {
                buf[n] = b;
                n += 1;
            }
        }
        for &b in b" | AI: ".iter() {
            if n >= buf.len() {
                break;
            }
            buf[n] = b;
            n += 1;
        }

        return local_model_reply(&buf[..n], out);
    }

    // Fallback: short OS-like prompt.
    copy_ascii(out, b"OK. Ask a question or describe the issue. Type help for options.")
}

fn copy_ascii(out: &mut [u8; CHAT_MAX_COLS], text: &[u8]) -> usize {
    let mut n = 0usize;
    for &b in text.iter() {
        if n >= CHAT_MAX_COLS {
            break;
        }
        if b >= 0x20 && b <= 0x7E {
            out[n] = b;
            n += 1;
        }
    }
    n
}

//=============================================================================
// Framebuffer state (provided by the UEFI bootloader)
//=============================================================================

#[repr(C)]
pub struct BootInfo {
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

    // Best-effort UTC wall-clock baseline captured by the UEFI bootloader.
    // If unavailable, boot_time_valid=0 and boot_unix_seconds=0.
    boot_unix_seconds: u64,
    boot_time_valid: u32,
    _time_reserved: u32,
}

const BOOTINFO_MAGIC: u64 = 0x5241_594F_535F_4249; // "RAYOS_BI"

static mut FB_BASE: usize = 0;
static mut FB_WIDTH: usize = 0;
static mut FB_HEIGHT: usize = 0;
static mut FB_STRIDE: usize = 0;

static BOOT_INFO_PHYS: AtomicU64 = AtomicU64::new(0);

static BOOT_UNIX_SECONDS_AT_BOOT: AtomicU64 = AtomicU64::new(0);
static BOOT_TIME_VALID: AtomicU64 = AtomicU64::new(0);

static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    // Howard Hinnant's civil_from_days algorithm.
    // Input: days since 1970-01-01. Output: (year, month, day) in Gregorian calendar.
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i32) + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (mp + if mp < 10 { 3 } else { -9 }) as i32;
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m as u32, d)
}

fn weekday_index_sun0(days_since_epoch: i64) -> u32 {
    // 1970-01-01 was a Thursday.
    // Return index where 0=Sun, 1=Mon, ... 6=Sat.
    let mut w = (days_since_epoch + 4) % 7;
    if w < 0 {
        w += 7;
    }
    w as u32
}

fn weekday_name(w: u32) -> &'static [u8] {
    match w {
        0 => b"Sun",
        1 => b"Mon",
        2 => b"Tue",
        3 => b"Wed",
        4 => b"Thu",
        5 => b"Fri",
        _ => b"Sat",
    }
}

fn current_unix_seconds_utc() -> Option<u64> {
    if BOOT_TIME_VALID.load(Ordering::Relaxed) == 0 {
        return None;
    }
    let base = BOOT_UNIX_SECONDS_AT_BOOT.load(Ordering::Relaxed);
    let ticks = TIMER_TICKS.load(Ordering::Relaxed);
    let secs = ticks / PIT_HZ;
    base.checked_add(secs)
}

fn append_2digits(out: &mut [u8], i: &mut usize, v: u32) {
    let tens = (v / 10) % 10;
    let ones = v % 10;
    if *i < out.len() {
        out[*i] = b'0' + tens as u8;
        *i += 1;
    }
    if *i < out.len() {
        out[*i] = b'0' + ones as u8;
        *i += 1;
    }
}

fn append_4digits(out: &mut [u8], i: &mut usize, v: u32) {
    let a = (v / 1000) % 10;
    let b = (v / 100) % 10;
    let c = (v / 10) % 10;
    let d = v % 10;
    for digit in [a, b, c, d] {
        if *i < out.len() {
            out[*i] = b'0' + digit as u8;
            *i += 1;
        }
    }
}

static IRQ_TIMER_COUNT: AtomicU64 = AtomicU64::new(0);
static IRQ_KBD_COUNT: AtomicU64 = AtomicU64::new(0);

static LAST_SCANCODE: AtomicU64 = AtomicU64::new(0);
static LAST_ASCII: AtomicU64 = AtomicU64::new(0);

static SHIFT_DOWN: AtomicU64 = AtomicU64::new(0);
static CAPS_LOCK: AtomicU64 = AtomicU64::new(0);

const KBD_BUF_SIZE: usize = 256;
static KBD_BUF_HEAD: AtomicUsize = AtomicUsize::new(0);
static KBD_BUF_TAIL: AtomicUsize = AtomicUsize::new(0);
static mut KBD_BUF: [u8; KBD_BUF_SIZE] = [0; KBD_BUF_SIZE];

static mut LAPIC_MMIO: u64 = 0;
static mut IOAPIC_MMIO: u64 = 0;
static mut IRQ0_GSI: u32 = 0;
static mut IRQ0_FLAGS: u16 = 0;
static mut IRQ1_GSI: u32 = 1;
static mut IRQ1_FLAGS: u16 = 0;

//=============================================================================
// System 1 / System 2 (minimal, testable stubs)
//=============================================================================

#[repr(C)]
#[derive(Clone, Copy)]
struct LogicRay {
    id: u64,
    op: u8,
    priority: u8,
    _reserved: u16,
    arg: u64,
}

impl LogicRay {
    const fn empty() -> Self {
        Self {
            id: 0,
            op: 0,
            priority: 0,
            _reserved: 0,
            arg: 0,
        }
    }
}

const RAY_QUEUE_SIZE: usize = 256;
static RAYQ_HEAD: AtomicUsize = AtomicUsize::new(0);
static RAYQ_TAIL: AtomicUsize = AtomicUsize::new(0);
static mut RAYQ: [LogicRay; RAY_QUEUE_SIZE] = [LogicRay::empty(); RAY_QUEUE_SIZE];

static SYSTEM1_RUNNING: AtomicBool = AtomicBool::new(false);
static SYSTEM1_ENQUEUED: AtomicU64 = AtomicU64::new(0);
static SYSTEM1_DROPPED: AtomicU64 = AtomicU64::new(0);
static SYSTEM1_PROCESSED: AtomicU64 = AtomicU64::new(0);
static SYSTEM1_LAST_RAY_ID: AtomicU64 = AtomicU64::new(0);
static SYSTEM1_LAST_OP: AtomicU64 = AtomicU64::new(0);
static SYSTEM1_LAST_PRIO: AtomicU64 = AtomicU64::new(0);
static SYSTEM1_LAST_ARG: AtomicU64 = AtomicU64::new(0);

static SYSTEM2_LAST_HASH: AtomicU64 = AtomicU64::new(0);
static SYSTEM2_LAST_OP: AtomicU64 = AtomicU64::new(0);
static SYSTEM2_LAST_PRIO: AtomicU64 = AtomicU64::new(0);
static SYSTEM2_LAST_COUNT: AtomicU64 = AtomicU64::new(0);
static SYSTEM2_ENQUEUED: AtomicU64 = AtomicU64::new(0);
static SYSTEM2_DROPPED: AtomicU64 = AtomicU64::new(0);

static CONDUCTOR_RUNNING: AtomicBool = AtomicBool::new(false);
static CONDUCTOR_SUBMITTED: AtomicU64 = AtomicU64::new(0);
static CONDUCTOR_DROPPED: AtomicU64 = AtomicU64::new(0);
static CONDUCTOR_LAST_TICK: AtomicU64 = AtomicU64::new(0);

// Host-bridge (Conductor/ai_bridge) presence indicator.
// We consider the bridge "connected" once we have received at least one AI reply line over COM1.
static HOST_BRIDGE_CONNECTED: AtomicBool = AtomicBool::new(false);

const CONDUCTOR_TARGET_DEPTH: usize = 8;
const CONDUCTOR_MAX_SUBMITS_PER_TICK: usize = 2;

const TASKQ_SIZE: usize = 32;
const TASKQ_MAX_BYTES: usize = 96;

static TASKQ_HEAD: AtomicUsize = AtomicUsize::new(0);
static TASKQ_TAIL: AtomicUsize = AtomicUsize::new(0);
static mut TASKQ_LEN: [u8; TASKQ_SIZE] = [0; TASKQ_SIZE];
static mut TASKQ: [[u8; TASKQ_MAX_BYTES]; TASKQ_SIZE] = [[0; TASKQ_MAX_BYTES]; TASKQ_SIZE];

#[inline(always)]
fn taskq_depth() -> usize {
    let head = TASKQ_HEAD.load(Ordering::Acquire);
    let tail = TASKQ_TAIL.load(Ordering::Acquire);
    head.wrapping_sub(tail) & (TASKQ_SIZE - 1)
}

fn taskq_push(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let head = TASKQ_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) & (TASKQ_SIZE - 1);
    let tail = TASKQ_TAIL.load(Ordering::Acquire);
    if next == tail {
        return false;
    }

    let mut len = bytes.len();
    if len > TASKQ_MAX_BYTES {
        len = TASKQ_MAX_BYTES;
    }

    unsafe {
        TASKQ_LEN[head] = len as u8;
        let slot = &mut TASKQ[head];
        for i in 0..len {
            slot[i] = bytes[i];
        }
        // NUL-pad remainder for deterministic debug prints.
        for i in len..TASKQ_MAX_BYTES {
            slot[i] = 0;
        }
    }

    TASKQ_HEAD.store(next, Ordering::Release);
    true
}

fn taskq_pop(dst: &mut [u8; TASKQ_MAX_BYTES]) -> Option<usize> {
    let tail = TASKQ_TAIL.load(Ordering::Relaxed);
    let head = TASKQ_HEAD.load(Ordering::Acquire);
    if tail == head {
        return None;
    }

    let len = unsafe { TASKQ_LEN[tail] as usize };
    unsafe {
        let slot = &TASKQ[tail];
        for i in 0..TASKQ_MAX_BYTES {
            dst[i] = slot[i];
        }
    }

    let next = (tail + 1) & (TASKQ_SIZE - 1);
    TASKQ_TAIL.store(next, Ordering::Release);
    Some(len.min(TASKQ_MAX_BYTES))
}

fn conductor_enqueue(bytes: &[u8]) -> bool {
    let ok = taskq_push(bytes);
    if !ok {
        CONDUCTOR_DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    ok
}

fn conductor_tick(tick: u64) {
    if !CONDUCTOR_RUNNING.load(Ordering::Relaxed) {
        return;
    }

    // Throttle: only consider submitting once per tick value.
    let last = CONDUCTOR_LAST_TICK.load(Ordering::Relaxed);
    if tick == last {
        return;
    }
    CONDUCTOR_LAST_TICK.store(tick, Ordering::Relaxed);

    let mut submits = 0usize;
    let mut buf = [0u8; TASKQ_MAX_BYTES];

    while submits < CONDUCTOR_MAX_SUBMITS_PER_TICK {
        if rayq_depth() >= CONDUCTOR_TARGET_DEPTH {
            break;
        }

        let Some(len) = taskq_pop(&mut buf) else {
            break;
        };
        if len == 0 {
            continue;
        }

        let _ = system2_submit_text(&buf[..len]);
        CONDUCTOR_SUBMITTED.fetch_add(1, Ordering::Relaxed);
        submits += 1;
    }
}

#[inline(always)]
fn rayq_depth() -> usize {
    let head = RAYQ_HEAD.load(Ordering::Acquire);
    let tail = RAYQ_TAIL.load(Ordering::Acquire);
    head.wrapping_sub(tail) & (RAY_QUEUE_SIZE - 1)
}

fn rayq_push(ray: LogicRay) -> bool {
    let head = RAYQ_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) & (RAY_QUEUE_SIZE - 1);
    let tail = RAYQ_TAIL.load(Ordering::Acquire);
    if next == tail {
        return false;
    }
    unsafe {
        RAYQ[head] = ray;
    }
    RAYQ_HEAD.store(next, Ordering::Release);
    true
}

fn rayq_pop() -> Option<LogicRay> {
    let tail = RAYQ_TAIL.load(Ordering::Relaxed);
    let head = RAYQ_HEAD.load(Ordering::Acquire);
    if tail == head {
        return None;
    }
    let ray = unsafe { RAYQ[tail] };
    let next = (tail + 1) & (RAY_QUEUE_SIZE - 1);
    RAYQ_TAIL.store(next, Ordering::Release);
    Some(ray)
}

fn system1_process_budget(mut budget: usize) {
    if !SYSTEM1_RUNNING.load(Ordering::Relaxed) {
        return;
    }

    while budget != 0 {
        let Some(ray) = rayq_pop() else {
            break;
        };
        SYSTEM1_LAST_RAY_ID.store(ray.id, Ordering::Relaxed);
        SYSTEM1_LAST_OP.store(ray.op as u64, Ordering::Relaxed);
        SYSTEM1_LAST_PRIO.store(ray.priority as u64, Ordering::Relaxed);
        SYSTEM1_LAST_ARG.store(ray.arg, Ordering::Relaxed);
        SYSTEM1_PROCESSED.fetch_add(1, Ordering::Relaxed);
        // Minimal deterministic “work” placeholder.
        core::hint::spin_loop();
        budget -= 1;
    }
}

fn system2_submit_text(input: &[u8]) -> (usize, u64, u8, u8, u64) {
    let mut rays = [LogicRay::empty(); 4];
    let count = system2_parse_to_rays(input, &mut rays);

    let mut pushed = 0u64;
    for ri in 0..count {
        let ok = rayq_push(rays[ri]);
        if ok {
            pushed += 1;
            SYSTEM1_ENQUEUED.fetch_add(1, Ordering::Relaxed);
            SYSTEM2_ENQUEUED.fetch_add(1, Ordering::Relaxed);
        } else {
            SYSTEM1_DROPPED.fetch_add(1, Ordering::Relaxed);
            SYSTEM2_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Save System 2 “decision” metadata for UI.
    let base_hash = fnv1a64(0xcbf2_9ce4_8422_2325, input);
    SYSTEM2_LAST_HASH.store(base_hash, Ordering::Relaxed);
    SYSTEM2_LAST_OP.store(rays[0].op as u64, Ordering::Relaxed);
    SYSTEM2_LAST_PRIO.store(rays[0].priority as u64, Ordering::Relaxed);
    SYSTEM2_LAST_COUNT.store(count as u64, Ordering::Relaxed);

    // Persist System 2 inputs if Volume is available.
    volume_log_s2(input);

    (count, pushed, rays[0].op, rays[0].priority, base_hash)
}

//=============================================================================
// Volume: PCI + virtio-blk (legacy) + append-only log
//=============================================================================

static VOLUME_READY: AtomicBool = AtomicBool::new(false);
static VOLUME_CAPACITY_SECTORS: AtomicU64 = AtomicU64::new(0);
static VOLUME_LOG_WRITE_IDX: AtomicU64 = AtomicU64::new(0);

static mut VIRTIO_BLK_IO_BASE: u16 = 0;
static mut VIRTIO_BLK_Q_SIZE: u16 = 0;
static mut VIRTIO_BLK_Q_MEM_PHYS: u64 = 0;
static mut VIRTIO_BLK_REQ_PHYS: u64 = 0;
static mut VIRTIO_BLK_LAST_USED_IDX: u16 = 0;

const VOLUME_SECTOR_SIZE: usize = 512;
const VOLUME_SUPER_LBA: u64 = 0;
const VOLUME_LOG_BASE_LBA: u64 = 1;

const RVOL_MAGIC: u64 = 0x315F4C4F565F5941; // "AY_VOL_1" (little-endian-ish marker)
const RVOL_REC_MAGIC: u32 = 0x4C4F5652; // 'RVOL'
const RVOL_KIND_S2: u8 = 1;

#[repr(C)]
struct VolSuper {
    magic: u64,
    write_idx: u64,
    capacity_sectors: u64,
    _reserved: [u8; 512 - 24],
}

#[repr(C)]
struct VolRecHdr {
    magic: u32,
    kind: u8,
    _rsv: u8,
    len: u16,
}

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 8],
    used_event: u16,
}

#[repr(C)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 8],
    avail_event: u16,
}

#[repr(C)]
struct VirtioBlkReq {
    type_: u32,
    reserved: u32,
    sector: u64,
}

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;

// Legacy virtio-pci I/O register offsets
const VIO_HOST_FEATURES: u16 = 0x00;
const VIO_GUEST_FEATURES: u16 = 0x04;
const VIO_QUEUE_PFN: u16 = 0x08;
const VIO_QUEUE_NUM: u16 = 0x0C;
const VIO_QUEUE_SEL: u16 = 0x0E;
const VIO_QUEUE_NOTIFY: u16 = 0x10;
const VIO_STATUS: u16 = 0x12;
const VIO_ISR: u16 = 0x13;
const VIO_DEVICE_CFG: u16 = 0x14;

const VIRTIO_STATUS_ACK: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_FAILED: u8 = 0x80;

fn pci_cfg_addr(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC)
}

fn pci_read32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    unsafe {
        outl(0xCF8, pci_cfg_addr(bus, dev, func, off));
        inl(0xCFC)
    }
}

fn pci_write32(bus: u8, dev: u8, func: u8, off: u8, value: u32) {
    unsafe {
        outl(0xCF8, pci_cfg_addr(bus, dev, func, off));
        outl(0xCFC, value);
    }
}

fn pci_probe_display_controller_bus0() -> Option<(u16, u16, u8, u8)> {
    // Scan bus 0 only (Q35 typically enumerates devices on bus 0).
    for dev in 0u8..32 {
        for func in 0u8..8 {
            let id = pci_read32(0, dev, func, 0x00);
            let vendor = (id & 0xFFFF) as u16;
            if vendor == 0xFFFF {
                continue;
            }
            let device = ((id >> 16) & 0xFFFF) as u16;

            // Class code register: 0x08 => [31:24]=class, [23:16]=subclass
            let class_reg = pci_read32(0, dev, func, 0x08);
            let class = ((class_reg >> 24) & 0xFF) as u8;
            let subclass = ((class_reg >> 16) & 0xFF) as u8;

            // 0x03 = Display controller
            if class == 0x03 {
                return Some((vendor, device, class, subclass));
            }
        }
    }
    None
}

fn volume_probe_virtio_legacy_blk() -> bool {
    // Scan bus 0 only (Q35 typically enumerates virtio on bus 0).
    for dev in 0u8..32 {
        for func in 0u8..8 {
            let id = pci_read32(0, dev, func, 0x00);
            let vendor = (id & 0xFFFF) as u16;
            if vendor == 0xFFFF {
                continue;
            }
            let device = ((id >> 16) & 0xFFFF) as u16;
            if vendor != 0x1AF4 {
                continue;
            }
            // VirtIO block (legacy)
            if device != 0x1001 {
                continue;
            }

            // Enable IO space + bus mastering.
            let cmdsts = pci_read32(0, dev, func, 0x04);
            let mut cmd = (cmdsts & 0xFFFF) as u16;
            cmd |= 0x1; // IO space
            cmd |= 0x4; // bus master
            let new_cmdsts = (cmdsts & 0xFFFF_0000) | (cmd as u32);
            pci_write32(0, dev, func, 0x04, new_cmdsts);

            let bar0 = pci_read32(0, dev, func, 0x10);
            if (bar0 & 0x1) == 0 {
                continue;
            }
            let io_base = (bar0 & 0xFFF0) as u16;
            unsafe {
                VIRTIO_BLK_IO_BASE = io_base;
            }
            return true;
        }
    }
    false
}

fn virtio_read8(off: u16) -> u8 {
    unsafe { inb(unsafe { VIRTIO_BLK_IO_BASE } + off) }
}

fn virtio_write8(off: u16, v: u8) {
    unsafe { outb(unsafe { VIRTIO_BLK_IO_BASE } + off, v) }
}

fn virtio_read16(off: u16) -> u16 {
    unsafe { inw(unsafe { VIRTIO_BLK_IO_BASE } + off) }
}

fn virtio_write16(off: u16, v: u16) {
    unsafe { outw(unsafe { VIRTIO_BLK_IO_BASE } + off, v) }
}

fn virtio_read32(off: u16) -> u32 {
    unsafe { inl(unsafe { VIRTIO_BLK_IO_BASE } + off) }
}

fn virtio_write32(off: u16, v: u32) {
    unsafe { outl(unsafe { VIRTIO_BLK_IO_BASE } + off, v) }
}

fn virtio_blk_init() -> bool {
    // Reset
    virtio_write8(VIO_STATUS, 0);

    // Acknowledge + driver
    virtio_write8(VIO_STATUS, VIRTIO_STATUS_ACK);
    virtio_write8(VIO_STATUS, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER);

    // Feature negotiation (minimal)
    let _host_features = virtio_read32(VIO_HOST_FEATURES);
    virtio_write32(VIO_GUEST_FEATURES, 0);
    virtio_write8(VIO_STATUS, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);
    let st = virtio_read8(VIO_STATUS);
    if (st & VIRTIO_STATUS_FEATURES_OK) == 0 {
        virtio_write8(VIO_STATUS, st | VIRTIO_STATUS_FAILED);
        return false;
    }

    // Set up queue 0.
    virtio_write16(VIO_QUEUE_SEL, 0);
    let qnum = virtio_read16(VIO_QUEUE_NUM);
    if qnum == 0 {
        virtio_write8(VIO_STATUS, virtio_read8(VIO_STATUS) | VIRTIO_STATUS_FAILED);
        return false;
    }
    // Legacy virtio-pci does NOT provide a way to set queue size; we must use the
    // device-provided size.
    let qsize = qnum;
    if qsize > 256 {
        // Keep the implementation bounded.
        virtio_write8(VIO_STATUS, virtio_read8(VIO_STATUS) | VIRTIO_STATUS_FAILED);
        return false;
    }
    unsafe {
        VIRTIO_BLK_Q_SIZE = qsize;
    }

    let desc_size = (core::mem::size_of::<VirtqDesc>() * qsize as usize) as u64;
    let avail_size = (4 + (2 * qsize as u64) + 2) as u64;
    let avail_off = desc_size;
    let used_off = align_up(avail_off + avail_size, 4096);
    let used_size = (4 + (8 * qsize as u64) + 2) as u64;
    let total = align_up(used_off + used_size, 4096);

    let qmem_phys = match phys_alloc_bytes(total as usize, 4096) {
        Some(p) => p,
        None => return false,
    };
    unsafe {
        VIRTIO_BLK_Q_MEM_PHYS = qmem_phys;
    }
    // Zero queue memory
    unsafe {
        let p = phys_as_mut_ptr::<u8>(qmem_phys);
        for i in 0..(total as usize) {
            core::ptr::write_volatile(p.add(i), 0);
        }
    }

    // Tell device PFN
    virtio_write32(VIO_QUEUE_PFN, (qmem_phys >> 12) as u32);

    // Allocate a single request page (header + one sector + status)
    let req_phys = match phys_alloc_page() {
        Some(p) => p,
        None => return false,
    };
    unsafe {
        VIRTIO_BLK_REQ_PHYS = req_phys;
        VIRTIO_BLK_LAST_USED_IDX = 0;
    }

    virtio_write8(VIO_STATUS, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK);

    // Read capacity from device-specific config (u64 sectors at offset 0)
    let cap_lo = virtio_read32(VIO_DEVICE_CFG);
    let cap_hi = virtio_read32(VIO_DEVICE_CFG + 4);
    let cap = (cap_hi as u64) << 32 | (cap_lo as u64);
    VOLUME_CAPACITY_SECTORS.store(cap, Ordering::Relaxed);
    true
}

fn virtio_blk_rw_sector(lba: u64, write: bool, data: &mut [u8; VOLUME_SECTOR_SIZE]) -> bool {
    unsafe {
        if VIRTIO_BLK_IO_BASE == 0 || VIRTIO_BLK_Q_MEM_PHYS == 0 || VIRTIO_BLK_REQ_PHYS == 0 {
            return false;
        }
    }

    // Layout within request page
    let req_phys = unsafe { VIRTIO_BLK_REQ_PHYS };
    let hdr_off = 0u64;
    let data_off = 64u64;
    let status_off = data_off + VOLUME_SECTOR_SIZE as u64;

    // Write header
    let hdr_ptr = unsafe { phys_as_mut_ptr::<VirtioBlkReq>(req_phys + hdr_off) };
    unsafe {
        core::ptr::write_volatile(
            hdr_ptr,
            VirtioBlkReq {
                type_: if write { VIRTIO_BLK_T_OUT } else { VIRTIO_BLK_T_IN },
                reserved: 0,
                sector: lba,
            },
        );
    }

    // Data buffer copy
    let data_ptr = unsafe { phys_as_mut_ptr::<u8>(req_phys + data_off) };
    if write {
        for i in 0..VOLUME_SECTOR_SIZE {
            unsafe { core::ptr::write_volatile(data_ptr.add(i), data[i]) };
        }
    }

    // Status byte
    let st_ptr = unsafe { phys_as_mut_ptr::<u8>(req_phys + status_off) };
    unsafe { core::ptr::write_volatile(st_ptr, 0xFF) };

    // Queue pointers
    let qphys = unsafe { VIRTIO_BLK_Q_MEM_PHYS };
    let qsize = unsafe { VIRTIO_BLK_Q_SIZE } as usize;
    if qsize == 0 {
        return false;
    }

    let desc_ptr = unsafe { phys_as_mut_ptr::<VirtqDesc>(qphys) };
    let desc_bytes = core::mem::size_of::<VirtqDesc>() * qsize;
    let avail_off = desc_bytes as u64;
    let avail_size = (4 + (2 * qsize) + 2) as u64;
    let used_off = align_up(avail_off + avail_size, 4096);

    // Dynamic ring access (avoids fixed-size structs).
    let avail_flags_ptr = unsafe { phys_as_mut_ptr::<u16>(qphys + avail_off) };
    let avail_idx_ptr = unsafe { phys_as_mut_ptr::<u16>(qphys + avail_off + 2) };
    let avail_ring_ptr = unsafe { phys_as_mut_ptr::<u16>(qphys + avail_off + 4) };
    let used_idx_ptr = unsafe { phys_as_mut_ptr::<u16>(qphys + used_off + 2) };

    // Fill three descriptors at 0,1,2
    unsafe {
        core::ptr::write_volatile(
            desc_ptr.add(0),
            VirtqDesc {
                addr: req_phys + hdr_off,
                len: core::mem::size_of::<VirtioBlkReq>() as u32,
                flags: VIRTQ_DESC_F_NEXT,
                next: 1,
            },
        );
        core::ptr::write_volatile(
            desc_ptr.add(1),
            VirtqDesc {
                addr: req_phys + data_off,
                len: VOLUME_SECTOR_SIZE as u32,
                flags: VIRTQ_DESC_F_NEXT | if write { 0 } else { VIRTQ_DESC_F_WRITE },
                next: 2,
            },
        );
        core::ptr::write_volatile(
            desc_ptr.add(2),
            VirtqDesc {
                addr: req_phys + status_off,
                len: 1,
                flags: VIRTQ_DESC_F_WRITE,
                next: 0,
            },
        );

        // Add head to avail
        let aidx = core::ptr::read_volatile(avail_idx_ptr);
        core::ptr::write_volatile(avail_ring_ptr.add((aidx as usize) % qsize), 0);
        core::sync::atomic::fence(Ordering::SeqCst);
        core::ptr::write_volatile(avail_idx_ptr, aidx.wrapping_add(1));
        core::sync::atomic::fence(Ordering::SeqCst);
    }

    // Notify queue 0
    virtio_write16(VIO_QUEUE_NOTIFY, 0);

    // Poll used
    let mut spins = 0u32;
    loop {
        let used_idx = unsafe { core::ptr::read_volatile(used_idx_ptr) };
        let last = unsafe { VIRTIO_BLK_LAST_USED_IDX };
        if used_idx != last {
            unsafe { VIRTIO_BLK_LAST_USED_IDX = last.wrapping_add(1) };
            break;
        }
        spins = spins.wrapping_add(1);
        if spins > 5_000_000 {
            return false;
        }
        core::hint::spin_loop();
    }

    let st = unsafe { core::ptr::read_volatile(st_ptr) };
    if st != 0 {
        return false;
    }

    if !write {
        for i in 0..VOLUME_SECTOR_SIZE {
            data[i] = unsafe { core::ptr::read_volatile(data_ptr.add(i)) };
        }
    }
    true
}

fn volume_read_sector(lba: u64, out: &mut [u8; VOLUME_SECTOR_SIZE]) -> bool {
    virtio_blk_rw_sector(lba, false, out)
}

fn volume_write_sector(lba: u64, data: &mut [u8; VOLUME_SECTOR_SIZE]) -> bool {
    virtio_blk_rw_sector(lba, true, data)
}

fn volume_format() -> bool {
    if !VOLUME_READY.load(Ordering::Relaxed) {
        return false;
    }
    let cap = VOLUME_CAPACITY_SECTORS.load(Ordering::Relaxed);
    let superblk = VolSuper {
        magic: RVOL_MAGIC,
        write_idx: 0,
        capacity_sectors: cap,
        _reserved: [0u8; 512 - 24],
    };

    let mut sector = [0u8; VOLUME_SECTOR_SIZE];
    unsafe {
        let src = &superblk as *const VolSuper as *const u8;
        for i in 0..VOLUME_SECTOR_SIZE {
            sector[i] = core::ptr::read_volatile(src.add(i));
        }
    }
    let ok = volume_write_sector(VOLUME_SUPER_LBA, &mut sector);
    if ok {
        VOLUME_LOG_WRITE_IDX.store(0, Ordering::Relaxed);
    }
    ok
}

fn volume_mount_or_format() -> bool {
    let mut sector = [0u8; VOLUME_SECTOR_SIZE];
    if !volume_read_sector(VOLUME_SUPER_LBA, &mut sector) {
        return false;
    }

    let mut magic = 0u64;
    for i in 0..8 {
        magic |= (sector[i] as u64) << (8 * i);
    }
    if magic != RVOL_MAGIC {
        return volume_format();
    }

    let mut write_idx = 0u64;
    for i in 0..8 {
        write_idx |= (sector[8 + i] as u64) << (8 * i);
    }
    VOLUME_LOG_WRITE_IDX.store(write_idx, Ordering::Relaxed);
    true
}

fn volume_init() -> bool {
    if !volume_probe_virtio_legacy_blk() {
        return false;
    }
    if !virtio_blk_init() {
        return false;
    }
    VOLUME_READY.store(true, Ordering::Relaxed);
    // Mount log (or format if missing)
    volume_mount_or_format()
}

fn volume_log_append(kind: u8, bytes: &[u8]) {
    if !VOLUME_READY.load(Ordering::Relaxed) {
        return;
    }

    // Monotonic sequence number for log appends.
    let mut write_seq = VOLUME_LOG_WRITE_IDX.load(Ordering::Relaxed);
    let cap = VOLUME_CAPACITY_SECTORS.load(Ordering::Relaxed);
    if cap <= VOLUME_LOG_BASE_LBA {
        return;
    }

    let log_cap = cap - VOLUME_LOG_BASE_LBA;
    if log_cap == 0 {
        return;
    }

    // One record per sector, truncate to fit.
    let max_payload = VOLUME_SECTOR_SIZE - core::mem::size_of::<VolRecHdr>();
    let mut len = bytes.len();
    if len > max_payload {
        len = max_payload;
    }
    let slot = write_seq % log_cap;

    let mut sector = [0u8; VOLUME_SECTOR_SIZE];
    // Header
    sector[0] = (RVOL_REC_MAGIC & 0xFF) as u8;
    sector[1] = ((RVOL_REC_MAGIC >> 8) & 0xFF) as u8;
    sector[2] = ((RVOL_REC_MAGIC >> 16) & 0xFF) as u8;
    sector[3] = ((RVOL_REC_MAGIC >> 24) & 0xFF) as u8;
    sector[4] = kind;
    sector[5] = 0;
    sector[6] = (len & 0xFF) as u8;
    sector[7] = ((len >> 8) & 0xFF) as u8;
    for i in 0..len {
        sector[8 + i] = bytes[i];
    }

    let _ = volume_write_sector(VOLUME_LOG_BASE_LBA + slot, &mut sector);
    write_seq = write_seq.wrapping_add(1);
    VOLUME_LOG_WRITE_IDX.store(write_seq, Ordering::Relaxed);

    // Update superblock with new write_idx.
    let mut supersec = [0u8; VOLUME_SECTOR_SIZE];
    if volume_read_sector(VOLUME_SUPER_LBA, &mut supersec) {
        // write_idx at bytes 8..16
        for i in 0..8 {
            supersec[8 + i] = ((write_seq >> (8 * i)) & 0xFF) as u8;
        }
        let _ = volume_write_sector(VOLUME_SUPER_LBA, &mut supersec);
    }
}

fn volume_log_s2(input: &[u8]) {
    // Keep log deterministic: store printable bytes only.
    let mut tmp = [0u8; 128];
    let mut n = 0usize;
    for &b in input {
        if n >= tmp.len() {
            break;
        }
        if b >= 0x20 && b <= 0x7E {
            tmp[n] = b;
            n += 1;
        }
    }
    if n != 0 {
        volume_log_append(RVOL_KIND_S2, &tmp[..n]);
    }
}

fn parse_u64_dec(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut v: u64 = 0;
    for &b in bytes {
        if b < b'0' || b > b'9' {
            return None;
        }
        v = v.checked_mul(10)?;
        v = v.checked_add((b - b'0') as u64)?;
    }
    Some(v)
}

#[inline(always)]
fn ascii_lower(b: u8) -> u8 {
    if (b'A'..=b'Z').contains(&b) {
        b + 32
    } else {
        b
    }
}

fn bytes_contains_ci(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    for i in 0..=(haystack.len() - needle.len()) {
        let mut ok = true;
        for j in 0..needle.len() {
            if ascii_lower(haystack[i + j]) != needle[j] {
                ok = false;
                break;
            }
        }
        if ok {
            return true;
        }
    }
    false
}

fn fnv1a64(mut h: u64, bytes: &[u8]) -> u64 {
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

//=============================================================================
// Minimal RAG retrieval over staged embeddings/index blobs
//=============================================================================

// embeddings.bin format (little-endian, no compression):
//   [4]  magic = "EMB0"
//   u32  version (=1)
//   u32  dim (currently expected 8)
//   u32  count
//   repeated count times:
//     u32 text_len
//     [text_len] text bytes (ASCII/UTF-8, printable preferred)
//     [dim] f32 embedding

const RAG_EMB_MAGIC: &[u8; 4] = b"EMB0";
const RAG_EMB_VERSION: u32 = 1;
const RAG_DIM: usize = 8;

const RAG_IDX_MAGIC: &[u8; 4] = b"HNS0";
const RAG_IDX_VERSION: u32 = 1;
const RAG_MAX_DOCS: usize = 256;

fn read_u32_le(buf: &[u8], off: &mut usize) -> Option<u32> {
    if *off + 4 > buf.len() {
        return None;
    }
    let v = u32::from_le_bytes([buf[*off], buf[*off + 1], buf[*off + 2], buf[*off + 3]]);
    *off += 4;
    Some(v)
}

fn read_f32_le(buf: &[u8], off: &mut usize) -> Option<f32> {
    if *off + 4 > buf.len() {
        return None;
    }
    let v = f32::from_le_bytes([buf[*off], buf[*off + 1], buf[*off + 2], buf[*off + 3]]);
    *off += 4;
    Some(v)
}

fn embed_text_8d(text: &[u8]) -> [f32; RAG_DIM] {
    // Deterministic tiny embedder: hashed bag-of-tokens into 8 dims.
    // This is intentionally simple (no heap, no unicode) but stable.
    let mut v = [0f32; RAG_DIM];
    let mut i = 0usize;
    while i < text.len() {
        while i < text.len() && (text[i] == b' ' || text[i] == b'\t') {
            i += 1;
        }
        if i >= text.len() {
            break;
        }
        let start = i;
        while i < text.len() && text[i] != b' ' && text[i] != b'\t' {
            i += 1;
        }
        let tok = &text[start..i];
        if tok.is_empty() {
            continue;
        }

        // Lowercase into a fixed scratch buffer.
        let mut tmp = [0u8; 32];
        let mut n = 0usize;
        for &b in tok.iter() {
            if n >= tmp.len() {
                break;
            }
            if b >= 0x20 && b <= 0x7E {
                tmp[n] = ascii_lower(b);
                n += 1;
            }
        }
        if n == 0 {
            continue;
        }
        let h = fnv1a64(0xcbf2_9ce4_8422_2325, &tmp[..n]);
        let idx = (h as usize) % RAG_DIM;
        let sign = if (h >> 63) != 0 { -1.0f32 } else { 1.0f32 };
        v[idx] += sign;
    }

    // L2 normalize
    let mut ss = 0f32;
    for x in v {
        ss += x * x;
    }
    if ss > 0.0 {
        let inv = 1.0 / sqrtf(ss);
        for j in 0..RAG_DIM {
            v[j] *= inv;
        }
    }
    v
}

fn dot8(a: &[f32; RAG_DIM], b: &[f32; RAG_DIM]) -> f32 {
    let mut s = 0f32;
    for i in 0..RAG_DIM {
        s += a[i] * b[i];
    }
    s
}

fn rag_try_get_embeddings_blob() -> Option<&'static [u8]> {
    let bi = bootinfo_ref()?;
    if bi.magic != BOOTINFO_MAGIC {
        return None;
    }
    if bi.embeddings_ptr == 0 || bi.embeddings_size == 0 {
        return None;
    }
    if bi.embeddings_ptr >= hhdm_phys_limit() {
        return None;
    }
    let size = bi.embeddings_size as usize;
    Some(unsafe { core::slice::from_raw_parts(phys_as_ptr::<u8>(bi.embeddings_ptr), size) })
}

fn rag_try_get_index_blob() -> Option<&'static [u8]> {
    let bi = bootinfo_ref()?;
    if bi.magic != BOOTINFO_MAGIC {
        return None;
    }
    if bi.index_ptr == 0 || bi.index_size == 0 {
        return None;
    }
    if bi.index_ptr >= hhdm_phys_limit() {
        return None;
    }
    let size = bi.index_size as usize;
    Some(unsafe { core::slice::from_raw_parts(phys_as_ptr::<u8>(bi.index_ptr), size) })
}

fn rag_scan_embeddings(
    blob: &[u8],
    text_offs: &mut [usize; RAG_MAX_DOCS],
    text_lens: &mut [usize; RAG_MAX_DOCS],
    vec_offs: &mut [usize; RAG_MAX_DOCS],
) -> Result<usize, &'static str> {
    if blob.len() < 16 || &blob[0..4] != RAG_EMB_MAGIC {
        return Err("embeddings bad magic");
    }
    let mut off = 4usize;
    let Some(version) = read_u32_le(blob, &mut off) else {
        return Err("embeddings truncated");
    };
    if version != RAG_EMB_VERSION {
        return Err("embeddings unsupported version");
    }
    let Some(dim) = read_u32_le(blob, &mut off) else {
        return Err("embeddings truncated");
    };
    if dim as usize != RAG_DIM {
        return Err("embeddings unsupported dim");
    }
    let Some(count_u32) = read_u32_le(blob, &mut off) else {
        return Err("embeddings truncated");
    };
    let mut count = count_u32 as usize;
    if count > RAG_MAX_DOCS {
        count = RAG_MAX_DOCS;
    }

    for i in 0..count {
        let Some(text_len_u32) = read_u32_le(blob, &mut off) else {
            return Err("embeddings truncated");
        };
        let text_len = text_len_u32 as usize;
        if off + text_len > blob.len() {
            return Err("embeddings truncated");
        }
        let text_off = off;
        off += text_len;
        let vec_off = off;
        let vec_bytes = RAG_DIM * 4;
        if off + vec_bytes > blob.len() {
            return Err("embeddings truncated");
        }
        off += vec_bytes;

        text_offs[i] = text_off;
        text_lens[i] = text_len;
        vec_offs[i] = vec_off;
    }

    Ok(count)
}

fn rag_read_vec8(blob: &[u8], vec_off: usize) -> Option<[f32; RAG_DIM]> {
    if vec_off + (RAG_DIM * 4) > blob.len() {
        return None;
    }
    let mut out = [0f32; RAG_DIM];
    let mut off = vec_off;
    for i in 0..RAG_DIM {
        out[i] = read_f32_le(blob, &mut off)?;
    }
    Some(out)
}

fn rag_insert_topk(
    kk: usize,
    score: f32,
    text_off: usize,
    text_len: usize,
    best_score: &mut [f32; 3],
    best_text_off: &mut [usize; 3],
    best_text_len: &mut [usize; 3],
) {
    let mut pos = kk;
    for i in 0..kk {
        if score > best_score[i] {
            pos = i;
            break;
        }
    }
    if pos < kk {
        for j in (pos + 1..kk).rev() {
            best_score[j] = best_score[j - 1];
            best_text_off[j] = best_text_off[j - 1];
            best_text_len[j] = best_text_len[j - 1];
        }
        best_score[pos] = score;
        best_text_off[pos] = text_off;
        best_text_len[pos] = text_len;
    }
}

fn rag_try_hnsw_search_topk(
    emb: &[u8],
    count: usize,
    text_offs: &[usize; RAG_MAX_DOCS],
    text_lens: &[usize; RAG_MAX_DOCS],
    vec_offs: &[usize; RAG_MAX_DOCS],
    qv: &[f32; RAG_DIM],
    kk: usize,
    best_score: &mut [f32; 3],
    best_text_off: &mut [usize; 3],
    best_text_len: &mut [usize; 3],
) -> bool {
    let Some(idx) = rag_try_get_index_blob() else {
        return false;
    };
    if idx.len() < 20 || &idx[0..4] != RAG_IDX_MAGIC {
        return false;
    }
    let mut off = 4usize;
    let Some(version) = read_u32_le(idx, &mut off) else {
        return false;
    };
    if version != RAG_IDX_VERSION {
        return false;
    }
    let Some(idx_count_u32) = read_u32_le(idx, &mut off) else {
        return false;
    };
    let idx_count = idx_count_u32 as usize;
    if idx_count != count {
        return false;
    }
    let Some(m_u32) = read_u32_le(idx, &mut off) else {
        return false;
    };
    let m = m_u32 as usize;
    if m == 0 || m > 16 {
        return false;
    }
    let Some(entry_u32) = read_u32_le(idx, &mut off) else {
        return false;
    };
    let entry = entry_u32 as usize;
    if entry >= count {
        return false;
    }

    let needed = count.checked_mul(m).and_then(|x| x.checked_mul(4)).unwrap_or(usize::MAX);
    if off + needed > idx.len() {
        return false;
    }
    let neigh_start = off;

    let mut visited = [0u8; RAG_MAX_DOCS];
    let mut cand_idx = [0u16; 32];
    let mut cand_score = [-1.0e30f32; 32];
    let mut cand_len = 0usize;

    let mut score_cache = [-1.0e30f32; RAG_MAX_DOCS];
    let mut score_valid = [0u8; RAG_MAX_DOCS];

    let mut score_of = |di: usize| -> f32 {
        if score_valid[di] != 0 {
            return score_cache[di];
        }
        let Some(dv) = rag_read_vec8(emb, vec_offs[di]) else {
            return -1.0e30f32;
        };
        let s = dot8(qv, &dv);
        score_cache[di] = s;
        score_valid[di] = 1;
        s
    };

    let s0 = score_of(entry);
    cand_idx[0] = entry as u16;
    cand_score[0] = s0;
    cand_len = 1;

    let mut expansions = 0usize;
    while cand_len != 0 && expansions < 64 {
        // Pop best-scoring candidate.
        let mut best_pos = 0usize;
        let mut best_s = cand_score[0];
        for i in 1..cand_len {
            if cand_score[i] > best_s {
                best_s = cand_score[i];
                best_pos = i;
            }
        }
        let cur = cand_idx[best_pos] as usize;
        let cur_s = cand_score[best_pos];
        cand_len -= 1;
        cand_idx[best_pos] = cand_idx[cand_len];
        cand_score[best_pos] = cand_score[cand_len];

        if visited[cur] != 0 {
            continue;
        }
        visited[cur] = 1;
        expansions += 1;

        rag_insert_topk(
            kk,
            cur_s,
            text_offs[cur],
            text_lens[cur],
            best_score,
            best_text_off,
            best_text_len,
        );

        // Expand neighbors
        let base = neigh_start + cur * m * 4;
        for j in 0..m {
            let mut noff = base + j * 4;
            let Some(n_u32) = read_u32_le(idx, &mut noff) else {
                continue;
            };
            if n_u32 == 0xFFFF_FFFF {
                continue;
            }
            let ni = n_u32 as usize;
            if ni >= count || visited[ni] != 0 {
                continue;
            }
            let ns = score_of(ni);
            if cand_len < cand_idx.len() {
                cand_idx[cand_len] = ni as u16;
                cand_score[cand_len] = ns;
                cand_len += 1;
            } else {
                // Replace the worst candidate if better.
                let mut worst_pos = 0usize;
                let mut worst_s = cand_score[0];
                for i in 1..cand_idx.len() {
                    if cand_score[i] < worst_s {
                        worst_s = cand_score[i];
                        worst_pos = i;
                    }
                }
                if ns > worst_s {
                    cand_idx[worst_pos] = ni as u16;
                    cand_score[worst_pos] = ns;
                }
            }
        }
    }

    true
}

fn rag_print_topk(query: &[u8], k: usize) {
    let Some(blob) = rag_try_get_embeddings_blob() else {
        serial_write_str("RAG: no embeddings\n");
        return;
    };

    let mut text_offs = [0usize; RAG_MAX_DOCS];
    let mut text_lens = [0usize; RAG_MAX_DOCS];
    let mut vec_offs = [0usize; RAG_MAX_DOCS];
    let count = match rag_scan_embeddings(blob, &mut text_offs, &mut text_lens, &mut vec_offs) {
        Ok(c) => c,
        Err(e) => {
            serial_write_str("RAG: ");
            serial_write_str(e);
            serial_write_str("\n");
            return;
        }
    };

    let qv = embed_text_8d(query);

    // Track top-K in fixed arrays.
    let kk = if k == 0 { 1 } else { if k > 3 { 3 } else { k } };
    let mut best_score = [-1.0e30f32; 3];
    let mut best_text_off = [0usize; 3];
    let mut best_text_len = [0usize; 3];

    // Prefer the HNSW-like index if present/valid; fall back to brute force.
    let used_index = rag_try_hnsw_search_topk(
        blob,
        count,
        &text_offs,
        &text_lens,
        &vec_offs,
        &qv,
        kk,
        &mut best_score,
        &mut best_text_off,
        &mut best_text_len,
    );

    if !used_index {
        for di in 0..count {
            let Some(dv) = rag_read_vec8(blob, vec_offs[di]) else {
                continue;
            };
            let score = dot8(&qv, &dv);
            rag_insert_topk(
                kk,
                score,
                text_offs[di],
                text_lens[di],
                &mut best_score,
                &mut best_text_off,
                &mut best_text_len,
            );
        }
    }

    serial_write_str("RAG: top=0x");
    serial_write_hex_u64(kk as u64);
    serial_write_str(" index=0x");
    serial_write_hex_u64(if used_index { 1 } else { 0 });
    serial_write_str("\n");
    for i in 0..kk {
        if best_text_len[i] == 0 {
            continue;
        }
        serial_write_str("RAG[");
        serial_write_hex_u64(i as u64);
        serial_write_str("] score=0x");
        // Quantize score into a pseudo-fixed-point hex for deterministic printing.
        let q = (best_score[i] * 65536.0) as i32;
        serial_write_hex_u64(q as u64);
        serial_write_str(" text=");
        let start = best_text_off[i];
        let end = start + best_text_len[i];
        for &b in &blob[start..end] {
            if b >= 0x20 && b <= 0x7E {
                serial_write_byte(b);
            }
        }
        serial_write_str("\n");
    }
}

fn system2_parse_to_rays(input: &[u8], out: &mut [LogicRay; 4]) -> usize {
    // ASCII-only intent parsing stub (deterministic; no heap; no regex).
    let mut buf = [0u8; 128];
    let mut len = 0usize;
    for &b in input {
        if b >= 0x20 && b <= 0x7E {
            if len < buf.len() {
                buf[len] = b;
                len += 1;
            } else {
                break;
            }
        }
    }
    let input = &buf[..len];

    let mut priority: u8 = 1; // normal
    if bytes_contains_ci(&input, b"urgent") || bytes_contains_ci(&input, b"now") {
        priority = 2;
    } else if bytes_contains_ci(&input, b"later") || bytes_contains_ci(&input, b"eventually") {
        priority = 0;
    }

    let mut op: u8 = 0;
    if bytes_contains_ci(&input, b"open") || bytes_contains_ci(&input, b"launch") {
        op = 1;
    } else if bytes_contains_ci(&input, b"close") || bytes_contains_ci(&input, b"exit") {
        op = 2;
    } else if bytes_contains_ci(&input, b"search") || bytes_contains_ci(&input, b"find") {
        op = 3;
    } else if bytes_contains_ci(&input, b"write") || bytes_contains_ci(&input, b"create") {
        op = 4;
    }

    let base_hash = fnv1a64(0xcbf2_9ce4_8422_2325, &input);

    let count = if op == 3 { 3 } else { 1 };
    for i in 0..count {
        let id = fnv1a64(base_hash ^ (i as u64), &[i as u8]);
        out[i] = LogicRay {
            id,
            op,
            priority,
            _reserved: 0,
            arg: i as u64,
        };
    }
    count
}

const TIMER_VECTOR: u8 = 32;
const KEYBOARD_VECTOR: u8 = 33;
const SPURIOUS_VECTOR: u8 = 0xFF;

// CPU exception vectors we care about for fault containment.
const DF_VECTOR: u8 = 8;
const UD_VECTOR: u8 = 6;
const GP_VECTOR: u8 = 13;
const PF_VECTOR: u8 = 14;

const DF_IST_INDEX: u8 = 1;

const HHDM_OFFSET: u64 = 0xffff_8000_0000_0000;
const DEFAULT_HHDM_PHYS_LIMIT: u64 = 0x1_0000_0000; // 4GiB

static HHDM_PHYS_LIMIT: AtomicU64 = AtomicU64::new(DEFAULT_HHDM_PHYS_LIMIT);

#[inline(always)]
fn hhdm_phys_limit() -> u64 {
    HHDM_PHYS_LIMIT.load(Ordering::Relaxed)
}

fn set_hhdm_phys_limit(new_limit: u64) {
    // Must be a multiple of 1GiB for the current 2MiB/PD-per-GiB mapper.
    const ONE_GIB: u64 = 0x4000_0000;
    let mut limit = align_up(new_limit, ONE_GIB);
    if limit < DEFAULT_HHDM_PHYS_LIMIT {
        limit = DEFAULT_HHDM_PHYS_LIMIT;
    }
    // PDPT has 512 entries => 512GiB coverage.
    let max_limit = 512 * ONE_GIB;
    if limit > max_limit {
        limit = max_limit;
    }
    HHDM_PHYS_LIMIT.store(limit, Ordering::Relaxed);
}

#[inline(always)]
fn phys_to_virt(phys: u64) -> u64 {
    phys + HHDM_OFFSET
}

#[inline(always)]
fn virt_to_phys(virt: u64) -> u64 {
    virt - HHDM_OFFSET
}

#[inline(always)]
fn pml4_index(virt: u64) -> usize {
    ((virt >> 39) & 0x1ff) as usize
}

#[inline(always)]
fn pdpt_index(virt: u64) -> usize {
    ((virt >> 30) & 0x1ff) as usize
}

#[inline(always)]
fn pd_index(virt: u64) -> usize {
    ((virt >> 21) & 0x1ff) as usize
}

#[inline(always)]
fn pt_index(virt: u64) -> usize {
    ((virt >> 12) & 0x1ff) as usize
}

#[inline(always)]
fn phys_as_ptr<T>(phys: u64) -> *const T {
    phys_to_virt(phys) as *const T
}

#[inline(always)]
fn phys_as_mut_ptr<T>(phys: u64) -> *mut T {
    phys_to_virt(phys) as *mut T
}

fn bootinfo_ref() -> Option<&'static BootInfo> {
    let phys = BOOT_INFO_PHYS.load(Ordering::Relaxed);
    if phys == 0 {
        return None;
    }
    if phys >= hhdm_phys_limit() {
        return None;
    }
    Some(unsafe { &*phys_as_ptr::<BootInfo>(phys) })
}

#[inline(always)]
fn halt_forever() -> ! {
    loop {
        halt_spin();
    }
}

// Minimal interrupt stub for a no-error-code interrupt.
global_asm!(
    r#"
    .global isr_timer
isr_timer:
    // Align stack for the Rust call (SysV wants 16-byte alignment)
    sub rsp, 8
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11

    call timer_interrupt_handler

    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    add rsp, 8
    iretq
"#
);

// Exception stub (no-error-code): #UD invalid opcode.
global_asm!(
    r#"
    .global isr_invalid_opcode
isr_invalid_opcode:
    // Stack: RIP, CS, RFLAGS, (optional) RSP, SS
    mov rdi, [rsp + 0]    // RIP
    sub rsp, 8
    push rax
    push rcx
    push rdx
    push rsi
    push r8
    push r9
    push r10
    push r11

    call invalid_opcode_handler

    pop r11
    pop r10
    pop r9
    pop r8
    pop rsi
    pop rdx
    pop rcx
    pop rax
    add rsp, 8
    iretq
"#
);

// Exception stubs (error-code exceptions). We don't return; we print and halt.
global_asm!(
    r#"
    .global isr_page_fault
isr_page_fault:
    mov rdi, [rsp + 0]    // error code
    mov rsi, [rsp + 8]    // RIP
    mov rdx, cr2          // faulting address
    call page_fault_handler
    iretq
"#
);

global_asm!(
    r#"
    .global isr_general_protection
isr_general_protection:
    mov rdi, [rsp + 0]    // error code
    mov rsi, [rsp + 8]    // RIP
    call general_protection_handler
    iretq
"#
);

global_asm!(
    r#"
    .global isr_double_fault
isr_double_fault:
    mov rdi, [rsp + 0]    // error code (always 0)
    mov rsi, [rsp + 8]    // RIP
    call double_fault_handler
    iretq
"#
);

global_asm!(
    r#"
    .global isr_keyboard
isr_keyboard:
    sub rsp, 8
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11

    call keyboard_interrupt_handler

    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    add rsp, 8
    iretq
"#
);

extern "C" {
    fn isr_timer();
    fn isr_keyboard();
    fn isr_page_fault();
    fn isr_general_protection();
    fn isr_double_fault();
    fn isr_invalid_opcode();
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry {
    offset_low: 0,
    selector: 0,
    ist: 0,
    type_attr: 0,
    offset_mid: 0,
    offset_high: 0,
    zero: 0,
}; 256];

fn read_cs() -> u16 {
    let cs: u16;
    unsafe { asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags)) };
    cs
}

unsafe fn idt_set_gate(vector: u8, handler: u64) {
    idt_set_gate_ist(vector, handler, 0);
}

unsafe fn idt_set_gate_ist(vector: u8, handler: u64, ist_index: u8) {
    let selector = read_cs();
    let type_attr: u8 = 0x8E; // present, DPL=0, interrupt gate
    IDT[vector as usize] = IdtEntry {
        offset_low: handler as u16,
        selector,
        ist: ist_index & 0x7,
        type_attr,
        offset_mid: (handler >> 16) as u16,
        offset_high: (handler >> 32) as u32,
        zero: 0,
    };
}

unsafe fn lidt() {
    let ptr = IdtPointer {
        limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
        // The kernel is effectively "relocated" by paging (phys -> virt). Symbol
        // addresses themselves are still physical. Point IDTR at a *mapped* VA.
        base: phys_to_virt((&raw const IDT) as u64),
    };
    asm!("lidt [{0}]", in(reg) &ptr, options(nostack, preserves_flags));
}

fn sti() {
    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
}

fn cli() {
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };
}

fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | (low as u64)
}

//=============================================================================
// GDT + TSS (needed for IST-based double-fault recovery)
//=============================================================================

#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct Tss {
    _reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    _reserved1: u64,
    ist1: u64,
    ist2: u64,
    ist3: u64,
    ist4: u64,
    ist5: u64,
    ist6: u64,
    ist7: u64,
    _reserved2: u64,
    _reserved3: u16,
    iomap_base: u16,
}

static mut TSS: Tss = Tss {
    _reserved0: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    _reserved1: 0,
    ist1: 0,
    ist2: 0,
    ist3: 0,
    ist4: 0,
    ist5: 0,
    ist6: 0,
    ist7: 0,
    _reserved2: 0,
    _reserved3: 0,
    iomap_base: core::mem::size_of::<Tss>() as u16,
};

// A dedicated stack for IST1 (double fault). Size doesn't need to be huge.
const IST_STACK_SIZE: usize = 16 * 1024;
#[repr(align(16))]
struct IstStack([u8; IST_STACK_SIZE]);
static mut IST1_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);

// GDT layout (match the selectors we inherit from OVMF/UEFI):
//  0x00: null
//  ...  : unused (0x08..0x28)
//  0x30: data (index 6)
//  0x38: code (index 7)
//  0x40: TSS  (index 8 + 9)
static mut GDT: [u64; 10] = [0; 10];

const GDT_SEL_DATA: u16 = 0x30;
const GDT_SEL_CODE: u16 = 0x38;
const GDT_SEL_TSS: u16 = 0x40;

fn gdt_make_code64() -> u64 {
    // 64-bit code segment: base=0 limit=0, L=1, D=0, P=1, S=1, type=Execute/Read.
    0x00AF9A000000FFFFu64
}

fn gdt_make_data64() -> u64 {
    // Data segment (mostly ignored in long mode but keep sane descriptors).
    0x00AF92000000FFFFu64
}

fn gdt_make_tss_descriptor(tss_addr: u64, tss_size: u32) -> (u64, u64) {
    // 64-bit TSS descriptor (available 0x9).
    let limit = (tss_size - 1) as u64;
    let base = tss_addr;

    let mut low: u64 = 0;
    low |= (limit & 0xFFFF) << 0;
    low |= (base & 0xFFFF) << 16;
    low |= ((base >> 16) & 0xFF) << 32;
    low |= (0x9u64) << 40; // type
    low |= (1u64) << 47; // present
    low |= ((limit >> 16) & 0xF) << 48;
    low |= ((base >> 24) & 0xFF) << 56;

    let high: u64 = base >> 32;
    (low, high)
}

fn init_gdt() {
    static GDT_DONE: AtomicBool = AtomicBool::new(false);
    if GDT_DONE.swap(true, Ordering::AcqRel) {
        return;
    }

    unsafe {
        // Set up TSS IST1.
        let ist1_top = (&raw const IST1_STACK.0 as u64) + (IST_STACK_SIZE as u64);
        TSS.ist1 = ist1_top;

        for i in 0..GDT.len() {
            GDT[i] = 0;
        }
        GDT[6] = gdt_make_data64();
        GDT[7] = gdt_make_code64();
        let (tss_low, tss_high) =
            gdt_make_tss_descriptor((&raw const TSS) as u64, core::mem::size_of::<Tss>() as u32);
        GDT[8] = tss_low;
        GDT[9] = tss_high;

        let ptr = GdtPointer {
            limit: (core::mem::size_of::<[u64; 10]>() - 1) as u16,
            base: (&raw const GDT) as u64,
        };

        asm!("lgdt [{0}]", in(reg) &ptr, options(nostack, preserves_flags));

        // Reload data segments.
        asm!(
            "mov ds, {0:x}\n\
             mov es, {0:x}\n\
             mov ss, {0:x}",
            in(reg) GDT_SEL_DATA,
            options(nostack, preserves_flags)
        );

        // Load Task Register with our TSS selector.
        asm!("ltr {0:x}", in(reg) GDT_SEL_TSS, options(nostack, preserves_flags));

        // CS reload is not required because we keep the inherited selectors valid (CS=0x38).
    }
}

fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
}

fn lapic_write(offset: u32, value: u32) {
    unsafe {
        if LAPIC_MMIO == 0 {
            return;
        }
        let reg = (LAPIC_MMIO + offset as u64) as *mut u32;
        core::ptr::write_volatile(reg, value);
        // Read-after-write for posted writes
        let _ = core::ptr::read_volatile(reg);
    }
}

fn lapic_enable() {
    // Ensure the APIC is enabled in IA32_APIC_BASE.
    const IA32_APIC_BASE: u32 = 0x1B;
    let mut apic_base = rdmsr(IA32_APIC_BASE);
    apic_base |= 1 << 11; // APIC Global Enable
    wrmsr(IA32_APIC_BASE, apic_base);

    // Software-enable the local APIC.
    lapic_write(0x80, 0); // TPR = 0
    lapic_write(0xF0, (SPURIOUS_VECTOR as u32) | 0x100);
}

fn lapic_eoi() {
    lapic_write(0xB0, 0);
}

fn ioapic_read(reg: u8) -> u32 {
    unsafe {
        let sel = IOAPIC_MMIO as *mut u32;
        let win = (IOAPIC_MMIO + 0x10) as *mut u32;
        core::ptr::write_volatile(sel, reg as u32);
        core::ptr::read_volatile(win)
    }
}

fn ioapic_write(reg: u8, value: u32) {
    unsafe {
        let sel = IOAPIC_MMIO as *mut u32;
        let win = (IOAPIC_MMIO + 0x10) as *mut u32;
        core::ptr::write_volatile(sel, reg as u32);
        core::ptr::write_volatile(win, value);
    }
}

fn ioapic_set_redir(gsi: u32, vector: u8, dest_apic_id: u8, flags: u16) {
    // Flags are from MADT Interrupt Source Override:
    // bits 0-1 polarity, bits 2-3 trigger.
    // We'll treat "conforms" as active-high edge for IRQ0.
    let polarity = flags & 0b11;
    let trigger = (flags >> 2) & 0b11;
    let mut low: u32 = vector as u32;

    // polarity: 2 = active low
    if polarity == 2 {
        low |= 1 << 13;
    }
    // trigger: 2 = level
    if trigger == 2 {
        low |= 1 << 15;
    }

    // Unmasked (bit 16 = 0)
    let high: u32 = (dest_apic_id as u32) << 24;

    let index = gsi;
    let reg = 0x10 + (index * 2);
    ioapic_write(reg as u8, low);
    ioapic_write((reg + 1) as u8, high);
}

#[no_mangle]
extern "C" fn page_fault_handler(error_code: u64, rip: u64, cr2: u64) -> ! {
    serial_write_str("EXC PF err=0x");
    serial_write_hex_u64(error_code);
    serial_write_str(" rip=0x");
    serial_write_hex_u64(rip);
    serial_write_str(" cr2=0x");
    serial_write_hex_u64(cr2);
    serial_write_str("\n");
    halt_forever();
}

#[no_mangle]
extern "C" fn general_protection_handler(error_code: u64, rip: u64) -> ! {
    serial_write_str("EXC GP err=0x");
    serial_write_hex_u64(error_code);
    serial_write_str(" rip=0x");
    serial_write_hex_u64(rip);
    serial_write_str("\n");
    halt_forever();
}

#[no_mangle]
extern "C" fn double_fault_handler(error_code: u64, rip: u64) -> ! {
    serial_write_str("EXC DF err=0x");
    serial_write_hex_u64(error_code);
    serial_write_str(" rip=0x");
    serial_write_hex_u64(rip);
    serial_write_str("\n");
    halt_forever();
}

#[no_mangle]
extern "C" fn invalid_opcode_handler(rip: u64) -> ! {
    serial_write_str("EXC UD rip=0x");
    serial_write_hex_u64(rip);
    serial_write_str("\n");
    halt_forever();
}

fn pic_remap_and_unmask_irq0() {
    // Remap PIC to 0x20..0x2F and unmask IRQ0.
    unsafe {
        outb(0x20, 0x11);
        outb(0xA0, 0x11);
        outb(0x21, 0x20);
        outb(0xA1, 0x28);
        outb(0x21, 0x04);
        outb(0xA1, 0x02);
        outb(0x21, 0x01);
        outb(0xA1, 0x01);
        // Unmask IRQ0 only
        outb(0x21, 0xFE);
        outb(0xA1, 0xFF);
    }
}

fn pic_unmask_irq1() {
    unsafe {
        // Clear bit 1 on master PIC mask
        let mask = inb(0x21);
        outb(0x21, mask & !0x02);
    }
}

fn pic_mask_all() {
    unsafe {
        outb(0x21, 0xFF);
        outb(0xA1, 0xFF);
    }
}

fn pit_init_hz(hz: u32) {
    let hz = hz.max(1);
    let divisor: u16 = (1193182u32 / hz).min(0xFFFF) as u16;
    unsafe {
        // Channel 0, lobyte/hibyte, mode 2, binary
        outb(0x43, 0x34);
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, (divisor >> 8) as u8);
    }
}

#[no_mangle]
extern "C" fn timer_interrupt_handler() {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    IRQ_TIMER_COUNT.fetch_add(1, Ordering::Relaxed);
    // Run a small slice of System 1 each tick to make it "running" without a scheduler.
    system1_process_budget(8);
    lapic_eoi();
}

#[no_mangle]
extern "C" fn keyboard_interrupt_handler() {
    IRQ_KBD_COUNT.fetch_add(1, Ordering::Relaxed);
    // Read scancode from PS/2 data port.
    let sc = unsafe { inb(0x60) };
    LAST_SCANCODE.store(sc as u64, Ordering::Relaxed);

    // Track modifier state (set 1 scancodes).
    match sc {
        0x2A | 0x36 => {
            // LShift/RShift down
            SHIFT_DOWN.store(1, Ordering::Relaxed);
        }
        0xAA | 0xB6 => {
            // LShift/RShift up
            SHIFT_DOWN.store(0, Ordering::Relaxed);
        }
        0x3A => {
            // CapsLock toggle (make code only)
            let cur = CAPS_LOCK.load(Ordering::Relaxed);
            CAPS_LOCK.store(cur ^ 1, Ordering::Relaxed);
        }
        _ => {}
    }

    // Ignore break codes for character generation.
    if sc & 0x80 == 0 {
        let shift = SHIFT_DOWN.load(Ordering::Relaxed) != 0;
        let caps = CAPS_LOCK.load(Ordering::Relaxed) != 0;
        if let Some(ch) = scancode_set1_to_ascii(sc, shift, caps) {
            kbd_buf_push(ch);
            LAST_ASCII.store(ch as u64, Ordering::Relaxed);
        }
    }
    lapic_eoi();
}

fn kbd_buf_push(byte: u8) {
    let head = KBD_BUF_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) & (KBD_BUF_SIZE - 1);
    let tail = KBD_BUF_TAIL.load(Ordering::Acquire);
    if next == tail {
        return;
    }
    unsafe {
        KBD_BUF[head] = byte;
    }
    KBD_BUF_HEAD.store(next, Ordering::Release);
}

fn kbd_buf_pop() -> Option<u8> {
    let tail = KBD_BUF_TAIL.load(Ordering::Relaxed);
    let head = KBD_BUF_HEAD.load(Ordering::Acquire);
    if tail == head {
        return None;
    }
    let byte = unsafe { KBD_BUF[tail] };
    let next = (tail + 1) & (KBD_BUF_SIZE - 1);
    KBD_BUF_TAIL.store(next, Ordering::Release);
    Some(byte)
}

fn kbd_try_read_byte() -> Option<u8> {
    kbd_buf_pop()
}

pub fn kbd_read_byte() -> u8 {
    loop {
        if let Some(b) = kbd_buf_pop() {
            return b;
        }
        halt_spin();
    }
}

pub fn kbd_read_line(buf: &mut [u8]) -> usize {
    let mut len: usize = 0;
    render_input_line(buf, len);
    loop {
        let b = kbd_read_byte();
        match b {
            b'\n' => {
                serial_write_str("\n");
                render_input_line(buf, len);
                return len;
            }
            0x08 => {
                // Backspace
                if len > 0 {
                    len -= 1;
                    serial_write_byte(0x08);
                    serial_write_byte(b' ');
                    serial_write_byte(0x08);
                    render_input_line(buf, len);
                }
            }
            b'\t' => {
                // Convert tabs to spaces.
                for _ in 0..4 {
                    if len < buf.len() {
                        buf[len] = b' ';
                        len += 1;
                        serial_write_byte(b' ');
                    }
                }
                render_input_line(buf, len);
            }
            0x20..=0x7E => {
                if len < buf.len() {
                    buf[len] = b;
                    len += 1;
                    serial_write_byte(b);
                    render_input_line(buf, len);
                }
            }
            _ => {}
        }
    }
}

fn bytes_eq(buf: &[u8], s: &[u8]) -> bool {
    if buf.len() != s.len() {
        return false;
    }
    for i in 0..buf.len() {
        if buf[i] != s[i] {
            return false;
        }
    }
    true
}

fn shell_split_whitespace<'a>(line: &'a [u8], argv: &mut [&'a [u8]; 8]) -> usize {
    let mut argc = 0usize;
    let mut i = 0usize;
    while i < line.len() {
        while i < line.len() && line[i] == b' ' {
            i += 1;
        }
        if i >= line.len() {
            break;
        }
        let start = i;
        while i < line.len() && line[i] != b' ' {
            i += 1;
        }
        let end = i;
        if argc < argv.len() {
            argv[argc] = &line[start..end];
            argc += 1;
        } else {
            break;
        }
    }
    argc
}

fn shell_print_usables() {
    let (count, regions) = unsafe { (USABLE_REGION_COUNT, USABLE_REGIONS) };
    serial_write_str("mmap regions=0x");
    serial_write_hex_u64(count as u64);
    serial_write_str("\n");
    for i in 0..count {
        let r = regions[i];
        serial_write_str("  [");
        serial_write_hex_u64(i as u64);
        serial_write_str("] start=0x");
        serial_write_hex_u64(r.start);
        serial_write_str(" end=0x");
        serial_write_hex_u64(r.end);
        serial_write_str("\n");
    }
}

fn shell_print_mmap_raw() {
    let Some(bi) = bootinfo_ref() else {
        serial_write_str("mmapraw bootinfo=NULL\n");
        return;
    };
    if bi.magic != BOOTINFO_MAGIC {
        serial_write_str("mmapraw bootinfo=BAD_MAGIC\n");
        return;
    }
    if bi.memory_map_ptr == 0 || bi.memory_map_size == 0 {
        serial_write_str("mmapraw empty\n");
        return;
    }

    let desc_size = bi.memory_desc_size as usize;
    serial_write_str("mmapraw desc_size=0x");
    serial_write_hex_u64(desc_size as u64);
    serial_write_str(" version=0x");
    serial_write_hex_u64(bi.memory_desc_version as u64);
    serial_write_str("\n");

    if desc_size != core::mem::size_of::<BootMemoryDescriptor>() {
        serial_write_str("mmapraw unsupported_desc_size\n");
        return;
    }

    let desc_count = (bi.memory_map_size as usize) / desc_size;
    serial_write_str("mmapraw count=0x");
    serial_write_hex_u64(desc_count as u64);
    serial_write_str("\n");

    // BootInfo carries physical pointers; access them via the HHDM.
    let desc_phys = bi.memory_map_ptr;
    if desc_phys >= hhdm_phys_limit() {
        serial_write_str("mmapraw mmap_ptr_out_of_range\n");
        return;
    }
    let descs = unsafe {
        core::slice::from_raw_parts(phys_as_ptr::<BootMemoryDescriptor>(desc_phys), desc_count)
    };

    for i in 0..desc_count {
        let d = descs[i];
        serial_write_str("  [");
        serial_write_hex_u64(i as u64);
        serial_write_str("] ty=0x");
        serial_write_hex_u64(d.ty as u64);
        serial_write_str(" start=0x");
        serial_write_hex_u64(d.phys_start);
        serial_write_str(" pages=0x");
        serial_write_hex_u64(d.page_count);
        serial_write_str("\n");
    }
}

fn shell_print_irqs() {
    let t = IRQ_TIMER_COUNT.load(Ordering::Relaxed);
    let k = IRQ_KBD_COUNT.load(Ordering::Relaxed);
    let last = LAST_SCANCODE.load(Ordering::Relaxed);
    serial_write_str("irqs timer=0x");
    serial_write_hex_u64(t);
    serial_write_str(" kbd=0x");
    serial_write_hex_u64(k);
    serial_write_str(" last_sc=0x");
    serial_write_hex_u64(last);
    serial_write_str("\n");
}

fn bytes_starts_with(buf: &[u8], prefix: &[u8]) -> bool {
    if buf.len() < prefix.len() {
        return false;
    }
    for i in 0..prefix.len() {
        if buf[i] != prefix[i] {
            return false;
        }
    }
    true
}

//=============================================================================
// Cortex -> Kernel protocol (minimal line-based transport)
//=============================================================================

// Transport: ASCII line over serial (host->guest) OR via shell injection.
// Format:
//   CORTEX:<TYPE> <k>=<v> <k>=<v> ...\n
// Types:
//   CORTEX:GAZE x=<0..1000> y=<0..1000> conf=<0..1000> ts=<ms>
//   CORTEX:OBJ label=<ascii> conf=<0..1000> x=<0..1000> y=<0..1000> w=<0..1000> h=<0..1000> ts=<ms>
//   CORTEX:INTENT kind=<select|move|delete|create|break|idle> target=<ascii> src=<ascii> dst=<ascii>

static CORTEX_RX_COUNT: AtomicU64 = AtomicU64::new(0);
static CORTEX_LAST_TYPE: AtomicU64 = AtomicU64::new(0); // 1=gaze,2=obj,3=intent
static CORTEX_LAST_TS_MS: AtomicU64 = AtomicU64::new(0);

fn cortex_kv_find<'a>(tokens: &'a [&'a [u8]; 16], count: usize, key: &[u8]) -> Option<&'a [u8]> {
    for i in 0..count {
        let t = tokens[i];
        // key=value
        let mut j = 0usize;
        while j < t.len() && t[j] != b'=' {
            j += 1;
        }
        if j == 0 || j >= t.len() {
            continue;
        }
        if bytes_eq(&t[..j], key) {
            return Some(&t[(j + 1)..]);
        }
    }
    None
}

fn cortex_parse_kv_tokens<'a>(line: &'a [u8], out: &mut [&'a [u8]; 16]) -> usize {
    // Split by spaces into at most 16 tokens.
    let mut argc = 0usize;
    let mut i = 0usize;
    while i < line.len() {
        while i < line.len() && line[i] == b' ' {
            i += 1;
        }
        if i >= line.len() {
            break;
        }
        let start = i;
        while i < line.len() && line[i] != b' ' {
            i += 1;
        }
        let end = i;
        if argc < out.len() {
            out[argc] = &line[start..end];
            argc += 1;
        } else {
            break;
        }
    }
    argc
}

fn cortex_handle_line(line: &[u8]) {
    if !bytes_starts_with(line, b"CORTEX:") {
        return;
    }

    // Extract TYPE
    let mut i = 7usize; // after "CORTEX:"
    let type_start = i;
    while i < line.len() && line[i] != b' ' {
        i += 1;
    }
    let ty = &line[type_start..i];
    while i < line.len() && line[i] == b' ' {
        i += 1;
    }
    let rest = if i < line.len() { &line[i..] } else { b"" };

    let mut tokens: [&[u8]; 16] = [b""; 16];
    let tokc = cortex_parse_kv_tokens(rest, &mut tokens);

    CORTEX_RX_COUNT.fetch_add(1, Ordering::Relaxed);

    if bytes_eq(ty, b"GAZE") {
        CORTEX_LAST_TYPE.store(1, Ordering::Relaxed);
        let x = cortex_kv_find(&tokens, tokc, b"x").and_then(parse_u32_decimal).unwrap_or(0);
        let y = cortex_kv_find(&tokens, tokc, b"y").and_then(parse_u32_decimal).unwrap_or(0);
        let conf = cortex_kv_find(&tokens, tokc, b"conf").and_then(parse_u32_decimal).unwrap_or(0);
        let ts = cortex_kv_find(&tokens, tokc, b"ts").and_then(parse_u32_decimal).unwrap_or(0) as u64;
        CORTEX_LAST_TS_MS.store(ts, Ordering::Relaxed);

        serial_write_str("cortex gaze x=");
        serial_write_hex_u64(x as u64);
        serial_write_str(" y=");
        serial_write_hex_u64(y as u64);
        serial_write_str(" conf=");
        serial_write_hex_u64(conf as u64);
        serial_write_str(" ts=");
        serial_write_hex_u64(ts);
        serial_write_str("\n");
        return;
    }

    if bytes_eq(ty, b"OBJ") {
        CORTEX_LAST_TYPE.store(2, Ordering::Relaxed);
        let conf = cortex_kv_find(&tokens, tokc, b"conf").and_then(parse_u32_decimal).unwrap_or(0);
        let x = cortex_kv_find(&tokens, tokc, b"x").and_then(parse_u32_decimal).unwrap_or(0);
        let y = cortex_kv_find(&tokens, tokc, b"y").and_then(parse_u32_decimal).unwrap_or(0);
        let w = cortex_kv_find(&tokens, tokc, b"w").and_then(parse_u32_decimal).unwrap_or(0);
        let h = cortex_kv_find(&tokens, tokc, b"h").and_then(parse_u32_decimal).unwrap_or(0);
        let ts = cortex_kv_find(&tokens, tokc, b"ts").and_then(parse_u32_decimal).unwrap_or(0) as u64;
        CORTEX_LAST_TS_MS.store(ts, Ordering::Relaxed);

        serial_write_str("cortex obj label=");
        if let Some(label) = cortex_kv_find(&tokens, tokc, b"label") {
            for &b in label {
                if b >= 0x20 && b <= 0x7E {
                    serial_write_byte(b);
                }
            }
        }
        serial_write_str(" conf=");
        serial_write_hex_u64(conf as u64);
        serial_write_str(" bbox=");
        serial_write_hex_u64(x as u64);
        serial_write_byte(b',');
        serial_write_hex_u64(y as u64);
        serial_write_byte(b',');
        serial_write_hex_u64(w as u64);
        serial_write_byte(b',');
        serial_write_hex_u64(h as u64);
        serial_write_str(" ts=");
        serial_write_hex_u64(ts);
        serial_write_str("\n");
        return;
    }

    if bytes_eq(ty, b"INTENT") {
        CORTEX_LAST_TYPE.store(3, Ordering::Relaxed);
        let kind = cortex_kv_find(&tokens, tokc, b"kind").unwrap_or(b"?");
        serial_write_str("cortex intent kind=");
        for &b in kind {
            if b >= 0x20 && b <= 0x7E {
                serial_write_byte(b);
            }
        }
        serial_write_str(" target=");
        if let Some(t) = cortex_kv_find(&tokens, tokc, b"target") {
            for &b in t {
                if b >= 0x20 && b <= 0x7E {
                    serial_write_byte(b);
                }
            }
        }
        serial_write_str("\n");

        // Feed System 2 by converting into a short text prompt.
        let mut tmp = [0u8; 128];
        let mut n = 0usize;
        for &b in kind {
            if n >= tmp.len() {
                break;
            }
            if b >= 0x20 && b <= 0x7E {
                tmp[n] = b;
                n += 1;
            }
        }
        if n < tmp.len() {
            tmp[n] = b' ';
            n += 1;
        }
        if let Some(t) = cortex_kv_find(&tokens, tokc, b"target") {
            for &b in t {
                if n >= tmp.len() {
                    break;
                }
                if b >= 0x20 && b <= 0x7E {
                    tmp[n] = b;
                    n += 1;
                }
            }
        }
        let input = &tmp[..n];
        let mut rays = [LogicRay::empty(); 4];
        let count = system2_parse_to_rays(input, &mut rays);
        let mut pushed = 0u64;
        for ri in 0..count {
            let ok = rayq_push(rays[ri]);
            if ok {
                pushed += 1;
                SYSTEM1_ENQUEUED.fetch_add(1, Ordering::Relaxed);
            } else {
                SYSTEM1_DROPPED.fetch_add(1, Ordering::Relaxed);
            }
        }
        serial_write_str("cortex s2 rays=0x");
        serial_write_hex_u64(count as u64);
        serial_write_str(" pushed=0x");
        serial_write_hex_u64(pushed);
        serial_write_str("\n");
        return;
    }

    serial_write_str("cortex unknown type\n");
}

fn shell_execute(line: &[u8]) {
    if line.is_empty() {
        return;
    }

    const EMPTY: &[u8] = b"";
    let mut argv: [&[u8]; 8] = [EMPTY; 8];
    let argc = shell_split_whitespace(line, &mut argv);
    if argc == 0 {
        return;
    }

    let cmd = argv[0];

    if bytes_eq(cmd, b"help") {
        serial_write_str("Commands: help, mem, ticks, irq, mmap [raw], fault <pf|gp|ud>, echo <text>, rag <text>, cortex <CORTEX:...>, s1 <start|stop|stats>, s2 <text>\n");
        return;
    }
    if bytes_eq(cmd, b"mem") {
        let (used, total, pages) = memory_stats();
        serial_write_str("mem used=0x");
        serial_write_hex_u64(used as u64);
        serial_write_str(" total=0x");
        serial_write_hex_u64(total as u64);
        serial_write_str(" pages=0x");
        serial_write_hex_u64(pages as u64);
        serial_write_str("\n");
        return;
    }
    if bytes_eq(cmd, b"ticks") {
        let ticks = TIMER_TICKS.load(Ordering::Relaxed);
        serial_write_str("ticks=0x");
        serial_write_hex_u64(ticks);
        serial_write_str("\n");
        return;
    }
    if bytes_eq(cmd, b"irq") {
        shell_print_irqs();
        return;
    }
    if bytes_eq(cmd, b"mmap") {
        if argc >= 2 && bytes_eq(argv[1], b"raw") {
            shell_print_mmap_raw();
        } else {
            shell_print_usables();
        }
        return;
    }

    if bytes_eq(cmd, b"fault") {
        if argc < 2 {
            serial_write_str("usage: fault <pf|gp|ud>\n");
            return;
        }

        if bytes_eq(argv[1], b"pf") {
            serial_write_str("triggering PF...\n");
            unsafe {
                // Canonical address in higher-half that we do not map.
                let p = 0xffff_ffff_ffff_f000u64 as *const u64;
                core::ptr::read_volatile(p);
            }
            return;
        }

        if bytes_eq(argv[1], b"gp") {
            serial_write_str("triggering GP...\n");
            unsafe {
                // Non-canonical address will generate #GP in long mode.
                let p = 0x0000_8000_0000_0000u64 as *const u64;
                core::ptr::read_volatile(p);
            }
            return;
        }

        if bytes_eq(argv[1], b"ud") {
            serial_write_str("triggering UD...\n");
            unsafe {
                asm!("ud2", options(nomem, nostack));
            }
            return;
        }

        serial_write_str("unknown fault type\n");
        return;
    }
    if bytes_eq(cmd, b"echo") {
        serial_write_str("echo: ");
        for ai in 1..argc {
            if ai != 1 {
                serial_write_byte(b' ');
            }
            for &b in argv[ai] {
                if b >= 0x20 && b <= 0x7E {
                    serial_write_byte(b);
                }
            }
        }
        serial_write_str("\n");
        return;
    }

    if bytes_eq(cmd, b"s1") {
        if argc < 2 {
            serial_write_str("usage: s1 <start|stop|stats>\n");
            return;
        }
        if bytes_eq(argv[1], b"start") {
            SYSTEM1_RUNNING.store(true, Ordering::Relaxed);
            serial_write_str("s1 running=1\n");
            return;
        }
        if bytes_eq(argv[1], b"stop") {
            SYSTEM1_RUNNING.store(false, Ordering::Relaxed);
            serial_write_str("s1 running=0\n");
            return;
        }
        if bytes_eq(argv[1], b"stats") {
            let running = if SYSTEM1_RUNNING.load(Ordering::Relaxed) { 1u64 } else { 0u64 };
            let depth = rayq_depth() as u64;
            let enq = SYSTEM1_ENQUEUED.load(Ordering::Relaxed);
            let drop = SYSTEM1_DROPPED.load(Ordering::Relaxed);
            let done = SYSTEM1_PROCESSED.load(Ordering::Relaxed);
            let last = SYSTEM1_LAST_RAY_ID.load(Ordering::Relaxed);
            serial_write_str("s1 running=0x");
            serial_write_hex_u64(running);
            serial_write_str(" depth=0x");
            serial_write_hex_u64(depth);
            serial_write_str(" enq=0x");
            serial_write_hex_u64(enq);
            serial_write_str(" drop=0x");
            serial_write_hex_u64(drop);
            serial_write_str(" done=0x");
            serial_write_hex_u64(done);
            serial_write_str(" last=0x");
            serial_write_hex_u64(last);
            serial_write_str("\n");
            return;
        }
        serial_write_str("unknown s1 subcommand\n");
        return;
    }

    if bytes_eq(cmd, b"s2") {
        // Grab the original rest-of-line (preserve spaces) for deterministic parsing.
        let mut i = 0usize;
        while i < line.len() && line[i] != b' ' {
            i += 1;
        }
        while i < line.len() && line[i] == b' ' {
            i += 1;
        }
        if i >= line.len() {
            serial_write_str("usage: s2 <text>\n");
            return;
        }

        let input = &line[i..];

        // RAG retrieval hook: pull top matches and print them as context.
        // This is intentionally serial-only for now (no heap, deterministic).
        rag_print_topk(input, 2);
        let mut rays = [LogicRay::empty(); 4];
        let count = system2_parse_to_rays(input, &mut rays);

        let mut pushed = 0u64;
        for ri in 0..count {
            let ok = rayq_push(rays[ri]);
            if ok {
                pushed += 1;
                SYSTEM1_ENQUEUED.fetch_add(1, Ordering::Relaxed);
            } else {
                SYSTEM1_DROPPED.fetch_add(1, Ordering::Relaxed);
            }
        }

        serial_write_str("s2 rays=0x");
        serial_write_hex_u64(count as u64);
        serial_write_str(" pushed=0x");
        serial_write_hex_u64(pushed);
        serial_write_str(" op=0x");
        serial_write_hex_u64(rays[0].op as u64);
        serial_write_str(" prio=0x");
        serial_write_hex_u64(rays[0].priority as u64);
        serial_write_str("\n");
        return;
    }

    if bytes_eq(cmd, b"rag") {
        // Preserve spaces in the rest-of-line.
        let mut i = 0usize;
        while i < line.len() && line[i] != b' ' {
            i += 1;
        }
        while i < line.len() && line[i] == b' ' {
            i += 1;
        }
        if i >= line.len() {
            serial_write_str("usage: rag <text>\n");
            return;
        }
        let query = &line[i..];
        rag_print_topk(query, 3);
        return;
    }

    if bytes_eq(cmd, b"cortex") {
        // Pass through the rest-of-line as a raw CORTEX message.
        let mut i = 0usize;
        while i < line.len() && line[i] != b' ' {
            i += 1;
        }
        while i < line.len() && line[i] == b' ' {
            i += 1;
        }
        if i >= line.len() {
            serial_write_str("usage: cortex <CORTEX:...>\n");
            return;
        }
        cortex_handle_line(&line[i..]);
        return;
    }

    if bytes_eq(cmd, b"conductor") {
        if argc < 2 {
            serial_write_str("usage: conductor snapshot|submit <text>|enqueue <text>|start|stop\n");
            return;
        }

        if bytes_eq(argv[1], b"snapshot") {
            let running = if SYSTEM1_RUNNING.load(Ordering::Relaxed) { 1u64 } else { 0u64 };
            let conductor_running = if CONDUCTOR_RUNNING.load(Ordering::Relaxed) { 1u64 } else { 0u64 };
            let depth = rayq_depth() as u64;
            let tq_depth = taskq_depth() as u64;
            let c_sub = CONDUCTOR_SUBMITTED.load(Ordering::Relaxed);
            let c_drop = CONDUCTOR_DROPPED.load(Ordering::Relaxed);
            let c_last_tick = CONDUCTOR_LAST_TICK.load(Ordering::Relaxed);

            let s1_enq = SYSTEM1_ENQUEUED.load(Ordering::Relaxed);
            let s1_drop = SYSTEM1_DROPPED.load(Ordering::Relaxed);
            let s1_done = SYSTEM1_PROCESSED.load(Ordering::Relaxed);
            let s1_last_op = SYSTEM1_LAST_OP.load(Ordering::Relaxed);
            let s1_last_prio = SYSTEM1_LAST_PRIO.load(Ordering::Relaxed);
            let s1_last_arg = SYSTEM1_LAST_ARG.load(Ordering::Relaxed);

            let s2_last_hash = SYSTEM2_LAST_HASH.load(Ordering::Relaxed);
            let s2_last_op = SYSTEM2_LAST_OP.load(Ordering::Relaxed);
            let s2_last_prio = SYSTEM2_LAST_PRIO.load(Ordering::Relaxed);
            let s2_last_count = SYSTEM2_LAST_COUNT.load(Ordering::Relaxed);
            let s2_enq = SYSTEM2_ENQUEUED.load(Ordering::Relaxed);
            let s2_drop = SYSTEM2_DROPPED.load(Ordering::Relaxed);

            serial_write_str("conductor snapshot ");
            serial_write_str("s1_running=0x");
            serial_write_hex_u64(running);
            serial_write_str(" conductor_running=0x");
            serial_write_hex_u64(conductor_running);
            serial_write_str(" conductor_tq_depth=0x");
            serial_write_hex_u64(tq_depth);
            serial_write_str(" conductor_submitted=0x");
            serial_write_hex_u64(c_sub);
            serial_write_str(" conductor_dropped=0x");
            serial_write_hex_u64(c_drop);
            serial_write_str(" conductor_last_tick=0x");
            serial_write_hex_u64(c_last_tick);
            serial_write_str(" depth=0x");
            serial_write_hex_u64(depth);
            serial_write_str(" s1_enq=0x");
            serial_write_hex_u64(s1_enq);
            serial_write_str(" s1_drop=0x");
            serial_write_hex_u64(s1_drop);
            serial_write_str(" s1_done=0x");
            serial_write_hex_u64(s1_done);
            serial_write_str(" s1_last_op=0x");
            serial_write_hex_u64(s1_last_op);
            serial_write_str(" s1_last_prio=0x");
            serial_write_hex_u64(s1_last_prio);
            serial_write_str(" s1_last_arg=0x");
            serial_write_hex_u64(s1_last_arg);
            serial_write_str(" s2_last_hash=0x");
            serial_write_hex_u64(s2_last_hash);
            serial_write_str(" s2_last_op=0x");
            serial_write_hex_u64(s2_last_op);
            serial_write_str(" s2_last_prio=0x");
            serial_write_hex_u64(s2_last_prio);
            serial_write_str(" s2_last_count=0x");
            serial_write_hex_u64(s2_last_count);
            serial_write_str(" s2_enq=0x");
            serial_write_hex_u64(s2_enq);
            serial_write_str(" s2_drop=0x");
            serial_write_hex_u64(s2_drop);
            serial_write_str("\n");
            return;
        }

        if bytes_eq(argv[1], b"submit") {
            // Preserve rest-of-line after the "submit" token (including spaces).
            // Format: conductor submit <text>
            let mut i = 0usize;
            // Skip "conductor"
            while i < line.len() && line[i] != b' ' {
                i += 1;
            }
            while i < line.len() && line[i] == b' ' {
                i += 1;
            }
            // Skip "submit"
            while i < line.len() && line[i] != b' ' {
                i += 1;
            }
            while i < line.len() && line[i] == b' ' {
                i += 1;
            }

            if i >= line.len() {
                serial_write_str("usage: conductor submit <text>\n");
                return;
            }

            let input = &line[i..];
            let (count, pushed, op, prio, hash) = system2_submit_text(input);

            serial_write_str("conductor submit ");
            serial_write_str("count=0x");
            serial_write_hex_u64(count as u64);
            serial_write_str(" pushed=0x");
            serial_write_hex_u64(pushed);
            serial_write_str(" op=0x");
            serial_write_hex_u64(op as u64);
            serial_write_str(" prio=0x");
            serial_write_hex_u64(prio as u64);
            serial_write_str(" hash=0x");
            serial_write_hex_u64(hash);
            serial_write_str("\n");
            return;
        }

        if bytes_eq(argv[1], b"start") {
            CONDUCTOR_RUNNING.store(true, Ordering::Relaxed);
            serial_write_str("conductor running=1\n");
            return;
        }

        if bytes_eq(argv[1], b"stop") {
            CONDUCTOR_RUNNING.store(false, Ordering::Relaxed);
            serial_write_str("conductor running=0\n");
            return;
        }

        if bytes_eq(argv[1], b"enqueue") {
            // Preserve rest-of-line after the "enqueue" token.
            // Format: conductor enqueue <text>
            let mut i = 0usize;
            // Skip "conductor"
            while i < line.len() && line[i] != b' ' {
                i += 1;
            }
            while i < line.len() && line[i] == b' ' {
                i += 1;
            }
            // Skip "enqueue"
            while i < line.len() && line[i] != b' ' {
                i += 1;
            }
            while i < line.len() && line[i] == b' ' {
                i += 1;
            }

            if i >= line.len() {
                serial_write_str("usage: conductor enqueue <text>\n");
                return;
            }

            let input = &line[i..];
            let ok = conductor_enqueue(input);
            serial_write_str("conductor enqueue ok=0x");
            serial_write_hex_u64(if ok { 1 } else { 0 });
            serial_write_str(" tq_depth=0x");
            serial_write_hex_u64(taskq_depth() as u64);
            serial_write_str("\n");
            return;
        }

        serial_write_str("unknown conductor subcommand\n");
        return;
    }

    if bytes_eq(cmd, b"vol") {
        if argc < 2 {
            serial_write_str("usage: vol probe|stats|format|tail <n>\n");
            return;
        }

        if bytes_eq(argv[1], b"probe") {
            let ok = volume_init();
            serial_write_str("vol probe ok=0x");
            serial_write_hex_u64(if ok { 1 } else { 0 });
            serial_write_str(" cap=0x");
            serial_write_hex_u64(VOLUME_CAPACITY_SECTORS.load(Ordering::Relaxed));
            serial_write_str("\n");
            return;
        }

        if bytes_eq(argv[1], b"stats") {
            serial_write_str("vol stats ready=0x");
            serial_write_hex_u64(if VOLUME_READY.load(Ordering::Relaxed) { 1 } else { 0 });
            serial_write_str(" cap=0x");
            serial_write_hex_u64(VOLUME_CAPACITY_SECTORS.load(Ordering::Relaxed));
            serial_write_str(" write_idx=0x");
            serial_write_hex_u64(VOLUME_LOG_WRITE_IDX.load(Ordering::Relaxed));
            serial_write_str("\n");
            return;
        }

        if bytes_eq(argv[1], b"format") {
            let ok = volume_format();
            serial_write_str("vol format ok=0x");
            serial_write_hex_u64(if ok { 1 } else { 0 });
            serial_write_str("\n");
            return;
        }

        if bytes_eq(argv[1], b"tail") {
            if argc < 3 {
                serial_write_str("usage: vol tail <n>\n");
                return;
            }
            let Some(n) = parse_u64_dec(argv[2]) else {
                serial_write_str("usage: vol tail <n>\n");
                return;
            };
            let n = n.min(16) as u64;
            let write_seq = VOLUME_LOG_WRITE_IDX.load(Ordering::Relaxed);
            let cap = VOLUME_CAPACITY_SECTORS.load(Ordering::Relaxed);
            if cap <= VOLUME_LOG_BASE_LBA {
                return;
            }
            let log_cap = cap - VOLUME_LOG_BASE_LBA;
            if log_cap == 0 {
                return;
            }
            // If we've wrapped, we can't reconstruct overwritten history; just bound the scan.
            let available = if write_seq > log_cap { log_cap } else { write_seq };
            let start = if available > n { available - n } else { 0 };
            let mut sector = [0u8; VOLUME_SECTOR_SIZE];
            let mut seq = start;
            while seq < available {
                let lba = VOLUME_LOG_BASE_LBA + (seq % log_cap);
                if volume_read_sector(lba, &mut sector) {
                    let magic = (sector[0] as u32)
                        | ((sector[1] as u32) << 8)
                        | ((sector[2] as u32) << 16)
                        | ((sector[3] as u32) << 24);
                    if magic == RVOL_REC_MAGIC {
                        let kind = sector[4];
                        let len = (sector[6] as usize) | ((sector[7] as usize) << 8);
                        serial_write_str("vol rec idx=0x");
                        serial_write_hex_u64(seq);
                        serial_write_str(" kind=0x");
                        serial_write_hex_u64(kind as u64);
                        serial_write_str(" len=0x");
                        serial_write_hex_u64(len as u64);
                        serial_write_str(" text=");
                        let mut i = 0usize;
                        while i < len && (8 + i) < VOLUME_SECTOR_SIZE {
                            let b = sector[8 + i];
                            if b >= 0x20 && b <= 0x7E {
                                serial_write_byte(b);
                            }
                            i += 1;
                        }
                        serial_write_str("\n");
                    }
                }
                seq += 1;
            }
            return;
        }

        serial_write_str("unknown vol subcommand\n");
        return;
    }

    serial_write_str("Unknown command. Type 'help'.\n");
}

fn scancode_set1_to_ascii(sc: u8, shift: bool, caps: bool) -> Option<u8> {
    let letter_upper = shift ^ caps;
    let ch = match sc {
        // Numbers row
        0x02 => if shift { b'!' } else { b'1' },
        0x03 => if shift { b'@' } else { b'2' },
        0x04 => if shift { b'#' } else { b'3' },
        0x05 => if shift { b'$' } else { b'4' },
        0x06 => if shift { b'%' } else { b'5' },
        0x07 => if shift { b'^' } else { b'6' },
        0x08 => if shift { b'&' } else { b'7' },
        0x09 => if shift { b'*' } else { b'8' },
        0x0A => if shift { b'(' } else { b'9' },
        0x0B => if shift { b')' } else { b'0' },
        0x0C => if shift { b'_' } else { b'-' },
        0x0D => if shift { b'+' } else { b'=' },

        // Top row
        0x10 => if letter_upper { b'Q' } else { b'q' },
        0x11 => if letter_upper { b'W' } else { b'w' },
        0x12 => if letter_upper { b'E' } else { b'e' },
        0x13 => if letter_upper { b'R' } else { b'r' },
        0x14 => if letter_upper { b'T' } else { b't' },
        0x15 => if letter_upper { b'Y' } else { b'y' },
        0x16 => if letter_upper { b'U' } else { b'u' },
        0x17 => if letter_upper { b'I' } else { b'i' },
        0x18 => if letter_upper { b'O' } else { b'o' },
        0x19 => if letter_upper { b'P' } else { b'p' },
        0x1A => if shift { b'{' } else { b'[' },
        0x1B => if shift { b'}' } else { b']' },

        // Home row
        0x1E => if letter_upper { b'A' } else { b'a' },
        0x1F => if letter_upper { b'S' } else { b's' },
        0x20 => if letter_upper { b'D' } else { b'd' },
        0x21 => if letter_upper { b'F' } else { b'f' },
        0x22 => if letter_upper { b'G' } else { b'g' },
        0x23 => if letter_upper { b'H' } else { b'h' },
        0x24 => if letter_upper { b'J' } else { b'j' },
        0x25 => if letter_upper { b'K' } else { b'k' },
        0x26 => if letter_upper { b'L' } else { b'l' },
        0x27 => if shift { b':' } else { b';' },
        0x28 => if shift { b'"' } else { b'\'' },
        0x29 => if shift { b'~' } else { b'`' },

        // Bottom row
        0x2C => if letter_upper { b'Z' } else { b'z' },
        0x2D => if letter_upper { b'X' } else { b'x' },
        0x2E => if letter_upper { b'C' } else { b'c' },
        0x2F => if letter_upper { b'V' } else { b'v' },
        0x30 => if letter_upper { b'B' } else { b'b' },
        0x31 => if letter_upper { b'N' } else { b'n' },
        0x32 => if letter_upper { b'M' } else { b'm' },
        0x33 => if shift { b'<' } else { b',' },
        0x34 => if shift { b'>' } else { b'.' },
        0x35 => if shift { b'?' } else { b'/' },
        0x2B => if shift { b'|' } else { b'\\' },

        // Whitespace / editing
        0x1C => b'\n',
        0x0E => 0x08, // Backspace
        0x0F => b'\t',
        0x39 => b' ',

        _ => return None,
    };
    Some(ch)
}

#[repr(C, packed)]
struct RsdpV2 {
    signature: [u8; 8],
    checksum: u8,
    oemid: [u8; 6],
    revision: u8,
    rsdt_address: u32,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    _reserved: [u8; 3],
}

#[repr(C, packed)]
struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oemid: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

#[repr(C, packed)]
struct Madt {
    header: SdtHeader,
    lapic_addr: u32,
    flags: u32,
}

fn checksum_ok(ptr: *const u8, len: usize) -> bool {
    let mut sum: u8 = 0;
    for i in 0..len {
        unsafe { sum = sum.wrapping_add(core::ptr::read_volatile(ptr.add(i))) };
    }
    sum == 0
}

fn acpi_find_madt(rsdp_addr: u64) -> Option<(*const Madt, u64, u64, u32, u16, u32, u16)> {
    if rsdp_addr == 0 {
        return None;
    }

    if rsdp_addr >= hhdm_phys_limit() {
        return None;
    }

    let rsdp = unsafe { &*(phys_to_virt(rsdp_addr) as *const RsdpV2) };
    if &rsdp.signature != b"RSD PTR " {
        return None;
    }

    // Validate checksum(s) best-effort.
    let _ = checksum_ok(phys_as_ptr::<u8>(rsdp_addr), 20);
    if rsdp.revision >= 2 {
        let _ = checksum_ok(phys_as_ptr::<u8>(rsdp_addr), rsdp.length as usize);
    }

    let xsdt_addr = rsdp.xsdt_address;
    if xsdt_addr == 0 {
        return None;
    }

    if xsdt_addr >= hhdm_phys_limit() {
        return None;
    }

    let xsdt_hdr = unsafe { &*(phys_to_virt(xsdt_addr) as *const SdtHeader) };
    if &xsdt_hdr.signature != b"XSDT" {
        return None;
    }

    let xsdt_len = xsdt_hdr.length as usize;
    if xsdt_len < core::mem::size_of::<SdtHeader>() {
        return None;
    }

    let entry_count = (xsdt_len - core::mem::size_of::<SdtHeader>()) / 8;
    let entries_ptr = unsafe { phys_as_ptr::<u8>(xsdt_addr).add(core::mem::size_of::<SdtHeader>()) }
        as *const u64;

    for i in 0..entry_count {
        let table_addr = unsafe { core::ptr::read_unaligned(entries_ptr.add(i)) };
        if table_addr == 0 {
            continue;
        }
        if table_addr >= hhdm_phys_limit() {
            continue;
        }

        let table_virt = phys_to_virt(table_addr) as usize;
        let hdr = unsafe { &*(table_virt as *const SdtHeader) };
        if &hdr.signature == b"APIC" {
            let madt = unsafe { &*(table_virt as *const Madt) };
            // Defaults
            let mut ioapic_addr: u64 = 0;
            let mut ioapic_gsi_base: u32 = 0;
            let mut irq0_gsi: u32 = 0;
            let mut irq0_flags: u16 = 0;
            let mut irq1_gsi: u32 = 1;
            let mut irq1_flags: u16 = 0;

            let madt_len = madt.header.length as usize;
            let mut p = (table_virt + core::mem::size_of::<Madt>()) as *const u8;
            let end = (table_virt + madt_len) as *const u8;
            while (p as usize) + 2 <= (end as usize) {
                let ty = unsafe { core::ptr::read_volatile(p) };
                let len = unsafe { core::ptr::read_volatile(p.add(1)) } as usize;
                if len < 2 || (p as usize) + len > (end as usize) {
                    break;
                }
                match ty {
                    1 => {
                        // IOAPIC: type(1), len(1), id(1), reserved(1), addr(4), gsi_base(4)
                        if len >= 12 {
                            let addr = unsafe { core::ptr::read_unaligned(p.add(4) as *const u32) };
                            let gsi = unsafe { core::ptr::read_unaligned(p.add(8) as *const u32) };
                            ioapic_addr = addr as u64;
                            ioapic_gsi_base = gsi;
                        }
                    }
                    2 => {
                        // Interrupt Source Override: bus(1), source(1), gsi(4), flags(2)
                        if len >= 10 {
                            let source = unsafe { core::ptr::read_volatile(p.add(3)) };
                            let gsi = unsafe { core::ptr::read_unaligned(p.add(4) as *const u32) };
                            let flags = unsafe { core::ptr::read_unaligned(p.add(8) as *const u16) };
                            if source == 0 {
                                irq0_gsi = gsi;
                                irq0_flags = flags;
                            }
                            if source == 1 {
                                irq1_gsi = gsi;
                                irq1_flags = flags;
                            }
                        }
                    }
                    _ => {}
                }
                p = unsafe { p.add(len) };
            }

            return Some((
                madt as *const Madt,
                madt.lapic_addr as u64,
                ioapic_addr,
                if irq0_gsi != 0 { irq0_gsi } else { ioapic_gsi_base },
                irq0_flags,
                irq1_gsi,
                irq1_flags,
            ));
        }
    }

    None
}

#[repr(C)]
#[derive(Copy, Clone)]
struct BootMemoryDescriptor {
    ty: u32,
    _padding: u32,
    phys_start: u64,
    virt_start: u64,
    page_count: u64,
    att: u64,
}

const EFI_MEMORY_TYPE_CONVENTIONAL: u32 = 7;
// Exclude MMIO apertures from HHDM limit calculations; they can extend very high
// (e.g., 0x8000_0000_00) and cause us to map/allocate far more than needed.
const EFI_MEMORY_TYPE_MMIO: u32 = 11;
const EFI_MEMORY_TYPE_MMIO_PORT: u32 = 12;
// RAM-like / usable-for-direct-deref types we might want covered by HHDM.
// (We still clamp to a sane cap elsewhere.)
const EFI_MEMORY_TYPE_PERSISTENT_MEMORY: u32 = 14;

extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

#[derive(Copy, Clone)]
struct UsableRegion {
    start: u64,
    end: u64,
    next: u64,
}

impl UsableRegion {
    const fn empty() -> Self {
        Self {
            start: 0,
            end: 0,
            next: 0,
        }
    }
}

static mut USABLE_REGIONS: [UsableRegion; 64] = [UsableRegion::empty(); 64];
static mut USABLE_REGION_COUNT: usize = 0;

fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

fn ranges_overlap(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> bool {
    a_start < b_end && b_start < a_end
}

fn phys_alloc_init_from_bootinfo(bi: &BootInfo) {
    unsafe {
        USABLE_REGION_COUNT = 0;
    }

    if bi.memory_map_ptr == 0 || bi.memory_map_size == 0 {
        return;
    }

    let desc_size = bi.memory_desc_size as usize;
    if desc_size != core::mem::size_of::<BootMemoryDescriptor>() {
        return;
    }

    let desc_count = (bi.memory_map_size as usize) / desc_size;
    let descs = unsafe {
        core::slice::from_raw_parts(bi.memory_map_ptr as *const BootMemoryDescriptor, desc_count)
    };

    let kernel_start = unsafe { &__kernel_start as *const u8 as u64 };
    let kernel_end = unsafe { &__kernel_end as *const u8 as u64 };
    let fb_start = bi.fb_base;
    let fb_end = bi.fb_base
        + (bi.fb_height as u64)
            * (bi.fb_stride as u64)
            * 4;

    let min_phys: u64 = 0x10_0000; // 1MiB

    for d in descs {
        if d.ty != EFI_MEMORY_TYPE_CONVENTIONAL {
            continue;
        }

        let mut start = d.phys_start;
        let end = d.phys_start + d.page_count * 4096;
        if end <= min_phys {
            continue;
        }
        if start < min_phys {
            start = min_phys;
        }
        // Exclude kernel image and framebuffer memory.
        if ranges_overlap(start, end, kernel_start, kernel_end) {
            // Carve out kernel range from the region (simple split into up to 2 pieces).
            if start < kernel_start {
                add_usable_region(start, kernel_start);
            }
            if kernel_end < end {
                add_usable_region(kernel_end, end);
            }
            continue;
        }
        if ranges_overlap(start, end, fb_start, fb_end) {
            if start < fb_start {
                add_usable_region(start, fb_start);
            }
            if fb_end < end {
                add_usable_region(fb_end, end);
            }
            continue;
        }

        add_usable_region(start, end);
    }
}

fn bootinfo_max_phys_end(bi: &BootInfo) -> Option<u64> {
    if bi.memory_map_ptr == 0 || bi.memory_map_size == 0 {
        return None;
    }
    let desc_size = bi.memory_desc_size as usize;
    if desc_size != core::mem::size_of::<BootMemoryDescriptor>() {
        return None;
    }
    let desc_count = (bi.memory_map_size as usize) / desc_size;
    let descs = unsafe {
        core::slice::from_raw_parts(bi.memory_map_ptr as *const BootMemoryDescriptor, desc_count)
    };

    let mut max_end: u64 = 0;
    for d in descs {
        // Size HHDM only from RAM-like regions. In QEMU+OVMF there can be huge
        // RESERVED ranges (type 0) up near 1TiB+; mapping those is pointless and
        // can destabilize early paging transitions.
        let ram_like = (1..=10).contains(&d.ty) || d.ty == EFI_MEMORY_TYPE_PERSISTENT_MEMORY;
        if !ram_like {
            continue;
        }
        // Also ignore MMIO apertures.
        if d.ty == EFI_MEMORY_TYPE_MMIO || d.ty == EFI_MEMORY_TYPE_MMIO_PORT {
            continue;
        }
        let end = d.phys_start.saturating_add(d.page_count.saturating_mul(4096));
        if end > max_end {
            max_end = end;
        }
    }
    if max_end == 0 {
        None
    } else {
        Some(max_end)
    }
}

fn add_usable_region(start: u64, end: u64) {
    if end <= start {
        return;
    }

    // Align to 4KiB pages.
    let start = align_up(start, 4096);
    let end = end & !(4096 - 1);
    if end <= start {
        return;
    }

    unsafe {
        if USABLE_REGION_COUNT >= USABLE_REGIONS.len() {
            return;
        }

        USABLE_REGIONS[USABLE_REGION_COUNT] = UsableRegion {
            start,
            end,
            next: start,
        };
        USABLE_REGION_COUNT += 1;
    }
}

fn phys_alloc_bytes(size: usize, align: usize) -> Option<u64> {
    let align = align.max(4096) as u64;
    let size = align_up(size as u64, 4096);
    unsafe {
        for i in 0..USABLE_REGION_COUNT {
            let region = &mut USABLE_REGIONS[i];
            let addr = align_up(region.next, align);
            let end = addr.checked_add(size)?;
            if end <= region.end {
                region.next = end;
                return Some(addr);
            }
        }
    }
    None
}

fn phys_alloc_bytes_below(size: usize, align: usize, max_phys_exclusive: u64) -> Option<u64> {
    let align = align.max(4096) as u64;
    let size = align_up(size as u64, 4096);
    unsafe {
        for i in 0..USABLE_REGION_COUNT {
            let region = &mut USABLE_REGIONS[i];

            // Skip regions that start entirely above the limit.
            if region.start >= max_phys_exclusive {
                continue;
            }

            let region_end = if region.end > max_phys_exclusive {
                max_phys_exclusive
            } else {
                region.end
            };

            let addr = align_up(region.next, align);
            let end = addr.checked_add(size)?;
            if end <= region_end {
                region.next = end;
                return Some(addr);
            }
        }
    }
    None
}

fn phys_alloc_page() -> Option<u64> {
    phys_alloc_bytes(4096, 4096)
}

fn phys_alloc_page_below(max_phys_exclusive: u64) -> Option<u64> {
    phys_alloc_bytes_below(4096, 4096, max_phys_exclusive)
}

fn zero_page_identity(phys: u64) {
    unsafe {
        let p = phys as *mut u64;
        for i in 0..(4096 / 8) {
            core::ptr::write_volatile(p.add(i), 0);
        }
    }
}

fn zero_page_hhdm(phys: u64) {
    unsafe {
        let p = phys_as_mut_ptr::<u64>(phys);
        for i in 0..(4096 / 8) {
            core::ptr::write_volatile(p.add(i), 0);
        }
    }
}

fn init_paging() {
    // Build fresh page tables with:
    // - HHDM: map 0..HHDM_LIMIT at HHDM_OFFSET (2MiB pages)
    // - Low identity: map only what we still need to execute safely at low VA
    //   (kernel image + current stack page). This removes the broad identity map.

    const ONE_GIB: u64 = 0x4000_0000;
    const TWO_MIB: u64 = 0x20_0000;

    const PTE_P: u64 = 1 << 0;
    const PTE_W: u64 = 1 << 1;
    const PTE_PS: u64 = 1 << 7; // huge page (2MiB)

    let pml4 = match phys_alloc_page_below(DEFAULT_HHDM_PHYS_LIMIT) {
        Some(p) => p,
        None => return,
    };
    let pdpt_low = match phys_alloc_page_below(DEFAULT_HHDM_PHYS_LIMIT) {
        Some(p) => p,
        None => return,
    };
    let pdpt_hhdm = match phys_alloc_page_below(DEFAULT_HHDM_PHYS_LIMIT) {
        Some(p) => p,
        None => return,
    };

    zero_page_identity(pml4);
    zero_page_identity(pdpt_low);
    zero_page_identity(pdpt_hhdm);

    // Full HHDM mapping: PDPT has 512 entries => up to 512GiB coverage.
    let mut hhdm_gib_count = (hhdm_phys_limit() + ONE_GIB - 1) / ONE_GIB;
    if hhdm_gib_count == 0 {
        hhdm_gib_count = 4;
    }
    if hhdm_gib_count > 512 {
        hhdm_gib_count = 512;
    }

    // Tight low identity mapping (temporary; we'll jump to KERNEL_BASE and then
    // rebuild paging without identity).
    let mut low_pds = [0u64; 4];
    let kernel_start = unsafe { &__kernel_start as *const u8 as u64 };
    let kernel_end = unsafe { &__kernel_end as *const u8 as u64 };

    let cur_rsp: u64;
    unsafe {
        asm!("mov {0}, rsp", out(reg) cur_rsp, options(nomem, preserves_flags));
    }
    let stack_phys = cur_rsp;

    let mut map_identity_2m = |phys: u64| {
        let phys_aligned = align_down(phys, TWO_MIB);
        let pdpt_index = (phys_aligned / ONE_GIB) as usize;
        if pdpt_index >= 4 {
            return;
        }
        if low_pds[pdpt_index] == 0 {
            let pd = match phys_alloc_page_below(DEFAULT_HHDM_PHYS_LIMIT) {
                Some(p) => p,
                None => return,
            };
            zero_page_identity(pd);
            low_pds[pdpt_index] = pd;
        }
        let pd = low_pds[pdpt_index] as *mut u64;
        let pd_index = ((phys_aligned % ONE_GIB) / TWO_MIB) as usize;
        unsafe {
            *pd.add(pd_index) = (phys_aligned & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W | PTE_PS;
        }
    };

    // Identity map kernel image (2MiB granularity).
    let k_start = align_down(kernel_start, TWO_MIB);
    let k_end = align_up(kernel_end, TWO_MIB);
    let mut p = k_start;
    while p < k_end {
        map_identity_2m(p);
        p = p.wrapping_add(TWO_MIB);
    }
    // Identity map current stack page so we can safely return from init_paging.
    map_identity_2m(stack_phys);

    // Kernel higher-half mapping at KERNEL_BASE (2MiB pages).
    let kernel_phys_start_aligned = KERNEL_PHYS_START_ALIGNED.load(Ordering::Relaxed);
    let kernel_delta = KERNEL_VIRT_DELTA.load(Ordering::Relaxed);
    let virt_start = kernel_phys_start_aligned.wrapping_add(kernel_delta);

    let pdpt_kernel = match phys_alloc_page_below(DEFAULT_HHDM_PHYS_LIMIT) {
        Some(p) => p,
        None => return,
    };
    zero_page_identity(pdpt_kernel);

    // Map the kernel's higher-half region under the appropriate PML4 slot.
    unsafe {
        *(pml4 as *mut u64).add(pml4_index(virt_start)) =
            (pdpt_kernel & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
    }

    let mut kernel_pds = [0u64; 512];
    let mut phys = k_start;
    while phys < k_end {
        let virt = phys.wrapping_add(kernel_delta);
        let pdpt_i = pdpt_index(virt);
        let pd_i = pd_index(virt);

        if kernel_pds[pdpt_i] == 0 {
            let pd = match phys_alloc_page_below(DEFAULT_HHDM_PHYS_LIMIT) {
                Some(p) => p,
                None => return,
            };
            zero_page_identity(pd);
            kernel_pds[pdpt_i] = pd;
            unsafe {
                *(pdpt_kernel as *mut u64).add(pdpt_i) =
                    (pd & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
            }
        }

        let pd = kernel_pds[pdpt_i] as *mut u64;
        unsafe {
            *pd.add(pd_i) = (phys & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W | PTE_PS;
        }

        phys = phys.wrapping_add(TWO_MIB);
    }

    unsafe {
        // PML4[0] -> PDPT_LOW
        *(pml4 as *mut u64).add(0) = (pdpt_low & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
        // PML4[256] -> PDPT_HHDM
        *(pml4 as *mut u64).add(256) = (pdpt_hhdm & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;

        // PDPT_HHDM entries -> HHDM PDs
        for i in 0..(hhdm_gib_count as usize) {
            let pd = match phys_alloc_page_below(DEFAULT_HHDM_PHYS_LIMIT) {
                Some(p) => p,
                None => break,
            };
            zero_page_identity(pd);
            *(pdpt_hhdm as *mut u64).add(i) = (pd & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;

            let pd_ptr = pd as *mut u64;
            for j in 0..512u64 {
                let phys = (i as u64) * ONE_GIB + j * TWO_MIB;
                *pd_ptr.add(j as usize) = (phys & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W | PTE_PS;
            }
        }

        // PDPT_LOW entries -> selectively allocated PDs
        for i in 0..4 {
            if low_pds[i] != 0 {
                *(pdpt_low as *mut u64).add(i) = (low_pds[i] & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
            }
        }

        // Switch to our new page tables.
        asm!("mov cr3, {0}", in(reg) pml4, options(nostack, preserves_flags));
    }

    CURRENT_PML4_PHYS.store(pml4, Ordering::Relaxed);
}

fn init_paging_final_no_identity() {
    // Build page tables with:
    // - HHDM: map 0..HHDM_LIMIT at HHDM_OFFSET (2MiB pages)
    // - Kernel: map kernel physical image at KERNEL_BASE (2MiB pages)
    // No low identity mappings.

    const ONE_GIB: u64 = 0x4000_0000;
    const TWO_MIB: u64 = 0x20_0000;

    const PTE_P: u64 = 1 << 0;
    const PTE_W: u64 = 1 << 1;
    const PTE_PS: u64 = 1 << 7; // huge page (2MiB)

    let pml4 = match phys_alloc_page() {
        Some(p) => p,
        None => return,
    };
    let pdpt_hhdm = match phys_alloc_page() {
        Some(p) => p,
        None => return,
    };
    // Minimal low identity mapping used only to keep the kernel image reachable
    // via its physical addresses. Some early code paths still touch globals via
    // absolute (physical) addresses; without this, removing identity mapping
    // can immediately fault.
    let pdpt_low = match phys_alloc_page() {
        Some(p) => p,
        None => return,
    };
    let pdpt_kernel = match phys_alloc_page() {
        Some(p) => p,
        None => return,
    };

    zero_page_hhdm(pml4);
    zero_page_hhdm(pdpt_hhdm);
    zero_page_hhdm(pdpt_low);
    zero_page_hhdm(pdpt_kernel);

    let mut hhdm_gib_count = (hhdm_phys_limit() + ONE_GIB - 1) / ONE_GIB;
    if hhdm_gib_count == 0 {
        hhdm_gib_count = 4;
    }
    if hhdm_gib_count > 512 {
        hhdm_gib_count = 512;
    }


    unsafe {
        let pml4_ptr = phys_as_mut_ptr::<u64>(pml4);
        // Low identity slot (first 512GiB). We only populate the first PDPT entry.
        *pml4_ptr.add(0) = (pdpt_low & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
        // HHDM slot
        *pml4_ptr.add(256) = (pdpt_hhdm & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
        // Kernel slot
        *pml4_ptr.add(pml4_index(KERNEL_BASE)) =
            (pdpt_kernel & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
    }

    // Map just the kernel's physical image identity-mapped using 2MiB pages.
    // (We keep this narrow to avoid reintroducing a broad low-VA identity window.)
    {
        let kernel_start = KERNEL_PHYS_START_ALIGNED.load(Ordering::Relaxed);
        let kernel_end = KERNEL_PHYS_END_ALIGNED.load(Ordering::Relaxed);
        if kernel_start < 0x4000_0000 {
            let pd_low = match phys_alloc_page() {
                Some(p) => p,
                None => return,
            };
            zero_page_hhdm(pd_low);
            unsafe {
                *(phys_as_mut_ptr::<u64>(pdpt_low)).add(0) = (pd_low & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
            }

            let pd_ptr = phys_as_mut_ptr::<u64>(pd_low);
            let mut phys = kernel_start;
            while phys < kernel_end {
                let idx = pd_index(phys);
                unsafe {
                    *pd_ptr.add(idx) = (phys & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W | PTE_PS;
                }
                phys = phys.wrapping_add(TWO_MIB);
            }
        }
    }

    // Fill PDPT_HHDM
    for i in 0..(hhdm_gib_count as usize) {
        let pd = match phys_alloc_page() {
            Some(p) => p,
            None => break,
        };
        zero_page_hhdm(pd);
        unsafe {
            *(phys_as_mut_ptr::<u64>(pdpt_hhdm)).add(i) = (pd & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
        }

        let pd_ptr = phys_as_mut_ptr::<u64>(pd);
        for j in 0..512u64 {
            let phys = (i as u64) * ONE_GIB + j * TWO_MIB;
            unsafe {
                *pd_ptr.add(j as usize) = (phys & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W | PTE_PS;
            }
        }
    }


    // Map kernel at KERNEL_BASE
    let kernel_start = KERNEL_PHYS_START_ALIGNED.load(Ordering::Relaxed);
    let kernel_end = KERNEL_PHYS_END_ALIGNED.load(Ordering::Relaxed);
    let kernel_delta = KERNEL_VIRT_DELTA.load(Ordering::Relaxed);

    let mut kernel_pds = [0u64; 512];
    let mut phys = kernel_start;
    while phys < kernel_end {
        let virt = phys.wrapping_add(kernel_delta);
        let pdpt_i = pdpt_index(virt);
        let pd_i = pd_index(virt);

        if kernel_pds[pdpt_i] == 0 {
            let pd = match phys_alloc_page() {
                Some(p) => p,
                None => return,
            };
            zero_page_hhdm(pd);
            kernel_pds[pdpt_i] = pd;
            unsafe {
                *(phys_as_mut_ptr::<u64>(pdpt_kernel)).add(pdpt_i) =
                    (pd & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W;
            }
        }

        unsafe {
            let pd_ptr = phys_as_mut_ptr::<u64>(kernel_pds[pdpt_i]);
            *pd_ptr.add(pd_i) = (phys & 0x000f_ffff_ffff_f000) | PTE_P | PTE_W | PTE_PS;
        }

        phys = phys.wrapping_add(TWO_MIB);
    }

    unsafe {
        asm!("mov cr3, {0}", in(reg) pml4, options(nostack, preserves_flags));
    }
    CURRENT_PML4_PHYS.store(pml4, Ordering::Relaxed);
}

//=============================================================================
// Minimal page-table manager (4-level, 4KiB mapping)
//=============================================================================

const PTE_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
const PTE_NX: u64 = 1u64 << 63;

const PTE_P: u64 = 1 << 0;
const PTE_W: u64 = 1 << 1;
const PTE_U: u64 = 1 << 2;
const PTE_PWT: u64 = 1 << 3;
const PTE_PCD: u64 = 1 << 4;
const PTE_A: u64 = 1 << 5;
const PTE_D: u64 = 1 << 6;
const PTE_PS: u64 = 1 << 7;
const PTE_G: u64 = 1 << 8;

#[derive(Copy, Clone)]
enum TableLevel {
    Pml4,
    Pdpt,
    Pd,
}

#[inline(always)]
fn pte_addr(pte: u64) -> u64 {
    pte & PTE_ADDR_MASK
}

#[inline(always)]
fn read_table_entry(parent_table_phys: u64, index: usize) -> u64 {
    let parent = phys_as_mut_ptr::<u64>(parent_table_phys);
    unsafe { core::ptr::read_volatile(parent.add(index)) }
}

#[inline(always)]
fn write_table_entry(parent_table_phys: u64, index: usize, value: u64) {
    let parent = phys_as_mut_ptr::<u64>(parent_table_phys);
    unsafe { core::ptr::write_volatile(parent.add(index), value) }
}

fn split_2mib_pde_to_pt(pd_phys: u64, pde_index: usize, pde: u64, virt_any: u64) -> Option<u64> {
    // Convert a 2MiB PDE mapping into a PT (512 x 4KiB PTEs) and update the PDE.
    // Assumes HHDM is active for touching paging structures.
    let base_phys = pte_addr(pde);

    let pt_phys = phys_alloc_page()?;
    zero_page_hhdm(pt_phys);

    // Preserve a conservative subset of leaf flags.
    let mut leaf_flags = pde & (PTE_W | PTE_U | PTE_PWT | PTE_PCD | PTE_G | PTE_NX);
    // Clear bits that don't exist / differ for 4KiB PTEs.
    leaf_flags &= !PTE_PS;

    let pt = phys_as_mut_ptr::<u64>(pt_phys);
    for i in 0..512u64 {
        let phys = base_phys.wrapping_add(i * 4096);
        unsafe {
            core::ptr::write_volatile(
                pt.add(i as usize),
                (phys & PTE_ADDR_MASK) | leaf_flags | PTE_P,
            );
        }
    }

    // Non-leaf PDE flags: keep RW/US/PWT/PCD; clear PS and NX.
    let nonleaf_flags = pde & (PTE_W | PTE_U | PTE_PWT | PTE_PCD);
    write_table_entry(pd_phys, pde_index, (pt_phys & PTE_ADDR_MASK) | nonleaf_flags | PTE_P);

    // Flush any cached 2MiB translation.
    invlpg(virt_any & !0x1f_ffff);

    Some(pt_phys)
}

fn ensure_table_hhdm(parent_table_phys: u64, index: usize, level: TableLevel, virt_for_split: u64) -> Option<u64> {
    let entry = read_table_entry(parent_table_phys, index);

    if (entry & PTE_P) == 0 {
        let new_table = phys_alloc_page()?;
        zero_page_hhdm(new_table);
        write_table_entry(parent_table_phys, index, (new_table & PTE_ADDR_MASK) | PTE_P | PTE_W);
        return Some(new_table);
    }

    if (entry & PTE_PS) != 0 {
        // Only support splitting 2MiB PDEs on-demand for now.
        if let TableLevel::Pd = level {
            return split_2mib_pde_to_pt(parent_table_phys, index, entry, virt_for_split);
        }
        return None;
    }

    Some(pte_addr(entry))
}

fn walk_to_pt_create(virt: u64) -> Option<u64> {
    let pml4_phys = CURRENT_PML4_PHYS.load(Ordering::Relaxed);
    if pml4_phys == 0 {
        return None;
    }

    let pdpt_phys = ensure_table_hhdm(pml4_phys, pml4_index(virt), TableLevel::Pml4, virt)?;
    let pd_phys = ensure_table_hhdm(pdpt_phys, pdpt_index(virt), TableLevel::Pdpt, virt)?;
    let pt_phys = ensure_table_hhdm(pd_phys, pd_index(virt), TableLevel::Pd, virt)?;
    Some(pt_phys)
}

fn walk_to_pt_existing(virt: u64) -> Option<u64> {
    let pml4_phys = CURRENT_PML4_PHYS.load(Ordering::Relaxed);
    if pml4_phys == 0 {
        return None;
    }

    let pml4e = read_table_entry(pml4_phys, pml4_index(virt));
    if (pml4e & PTE_P) == 0 {
        return None;
    }
    let pdpt_phys = pte_addr(pml4e);

    let pdpte = read_table_entry(pdpt_phys, pdpt_index(virt));
    if (pdpte & PTE_P) == 0 || (pdpte & PTE_PS) != 0 {
        return None;
    }
    let pd_phys = pte_addr(pdpte);

    let pde = read_table_entry(pd_phys, pd_index(virt));
    if (pde & PTE_P) == 0 {
        return None;
    }
    if (pde & PTE_PS) != 0 {
        // Split on-demand to allow fine-grained unmapping.
        return split_2mib_pde_to_pt(pd_phys, pd_index(virt), pde, virt);
    }
    let pt_phys = pte_addr(pde);
    Some(pt_phys)
}

fn invlpg(virt: u64) {
    unsafe { asm!("invlpg [{0}]", in(reg) virt, options(nostack, preserves_flags)) };
}

fn map_page_4k(virt: u64, phys: u64, flags: u64) -> bool {
    let pt_phys = match walk_to_pt_create(virt) {
        Some(p) => p,
        None => return false,
    };

    let pt = phys_as_mut_ptr::<u64>(pt_phys);
    unsafe {
        core::ptr::write_volatile(
            pt.add(pt_index(virt)),
            (phys & PTE_ADDR_MASK) | flags | PTE_P,
        );
    }
    invlpg(virt);
    true
}

fn unmap_page_4k(virt: u64) -> bool {
    let pt_phys = match walk_to_pt_existing(virt) {
        Some(p) => p,
        None => return false,
    };

    let pt = phys_as_mut_ptr::<u64>(pt_phys);
    let pte = unsafe { core::ptr::read_volatile(pt.add(pt_index(virt))) };
    if (pte & PTE_P) == 0 {
        return false;
    }
    unsafe { core::ptr::write_volatile(pt.add(pt_index(virt)), 0) };
    invlpg(virt);
    true
}

//=============================================================================
// Minimal spinlock + bump allocator (kernel-local heap)
//=============================================================================

struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    fn lock(&self) -> SpinLockGuard<'_, T> {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }
}

struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> core::ops::Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> core::ops::DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

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
        let aligned_addr = (self.next + align - 1) & !(align - 1);
        let end_addr = aligned_addr.checked_add(size)?;
        if end_addr > self.heap_end {
            return None;
        }
        self.next = end_addr;
        ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
        Some(aligned_addr)
    }

    fn allocated_bytes(&self) -> usize {
        self.next.saturating_sub(self.heap_start)
    }
}

const HEAP_SIZE: usize = 256 * 1024;

#[repr(align(16))]
struct HeapBuf([u8; HEAP_SIZE]);

static mut HEAP: HeapBuf = HeapBuf([0; HEAP_SIZE]);

static HEAP_ALLOCATOR: SpinLock<BumpAllocator> = SpinLock::new(BumpAllocator::new());
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

#[no_mangle]
pub extern "C" fn _start(boot_info_phys: u64) -> ! {
    serial_init();
    // UEFI may leave interrupts enabled. Until we have an IDT installed, any IRQ
    // can cause a triple-fault reset. Keep interrupts off during early bring-up.
    cli();
    serial_write_str("RayOS kernel-bare: _start\n");

    cpu_enable_x87_sse();

    BOOT_INFO_PHYS.store(boot_info_phys, Ordering::Relaxed);

    // Compute the kernel's physical base (we start executing with identity mappings)
    // and precompute the virtual delta for the higher-half mapping.
    const TWO_MIB: u64 = 0x20_0000;
    let kernel_phys_start = unsafe { &__kernel_start as *const u8 as u64 };
    let kernel_phys_end = unsafe { &__kernel_end as *const u8 as u64 };
    let kernel_phys_start_aligned = align_down(kernel_phys_start, TWO_MIB);
    let kernel_phys_end_aligned = align_up(kernel_phys_end, TWO_MIB);
    KERNEL_PHYS_START_ALIGNED.store(kernel_phys_start_aligned, Ordering::Relaxed);
    KERNEL_PHYS_END_ALIGNED.store(kernel_phys_end_aligned, Ordering::Relaxed);
    KERNEL_VIRT_DELTA.store(KERNEL_BASE.wrapping_sub(kernel_phys_start_aligned), Ordering::Relaxed);

    let (fb_phys, fb_width, fb_height, fb_stride, rsdp_phys, model_ptr, model_size, boot_unix, boot_time_valid) = unsafe {
        if boot_info_phys == 0 {
            serial_write_str("  boot_info_phys=0\n");
            (0usize, 0usize, 0usize, 0usize, 0u64, 0u64, 0u64, 0u64, 0u32)
        } else {
            let bi = &*(boot_info_phys as *const BootInfo);
            if bi.magic != BOOTINFO_MAGIC {
                serial_write_str("  boot_info magic mismatch\n");
            }
            (
                bi.fb_base as usize,
                bi.fb_width as usize,
                bi.fb_height as usize,
                bi.fb_stride as usize,
                bi.rsdp_addr,
                bi.model_ptr,
                bi.model_size,
                bi.boot_unix_seconds,
                bi.boot_time_valid,
            )
        }
    };

    serial_write_str("  fb_base=0x");
    serial_write_hex_u64(fb_phys as u64);
    serial_write_str(" width=");
    serial_write_hex_u64(fb_width as u64);
    serial_write_str(" height=");
    serial_write_hex_u64(fb_height as u64);
    serial_write_str(" stride=");
    serial_write_hex_u64(fb_stride as u64);
    serial_write_str("\n");

    serial_write_str("  rsdp=0x");
    serial_write_hex_u64(rsdp_phys);
    serial_write_str("\n");

    unsafe {
        // Before we install our own paging, the bootloader's mappings are in effect.
        // Use the physical address for the initial proof pixel write.
        FB_BASE = fb_phys;
        FB_WIDTH = fb_width;
        FB_HEIGHT = fb_height;
        FB_STRIDE = fb_stride;
    }

    // Record optional model blob for local LLM.
    MODEL_PHYS.store(model_ptr, Ordering::Relaxed);
    MODEL_SIZE.store(model_size, Ordering::Relaxed);

    // Record boot wall-clock baseline, if provided.
    BOOT_UNIX_SECONDS_AT_BOOT.store(boot_unix, Ordering::Relaxed);
    BOOT_TIME_VALID.store(boot_time_valid as u64, Ordering::Relaxed);

    if boot_time_valid != 0 {
        serial_write_str("SYS: boot time valid unix=0x");
        serial_write_hex_u64(boot_unix);
        serial_write_str("\n");
    } else {
        serial_write_str("SYS: boot time not available\n");
    }

    if LOCAL_LLM_ENABLED {
        if model_ptr != 0 && model_size != 0 {
            serial_write_str("SYS: local LLM model loaded bytes=0x");
            serial_write_hex_u64(model_size);
            serial_write_str("\n");
        } else {
            serial_write_str("SYS: local LLM enabled but model.bin missing (EFI/RAYOS/model.bin)\n");
        }
    }

    // Early visible proof we got control.
    unsafe {
        let fb = FB_BASE as *mut u32;
        if !fb.is_null() {
            *fb = 0x00_ff_00;
            *fb.add(1) = 0x00_ff_00;
            *fb.add(FB_STRIDE) = 0x00_ff_00;
            *fb.add(FB_STRIDE + 1) = 0x00_ff_00;
        }
    }

    if boot_info_phys == 0 {
        // No BootInfo means no paging/alloc init; limp along on bootloader mappings.
        init_gdt();
        init_memory();
        kernel_main();
    }

    // Expand HHDM mapping coverage based on the UEFI memory map so we can safely
    // dereference physical pointers above 4GiB (ACPI tables, memory map, etc.).
    serial_write_str("  bootinfo: scanning max phys end...\n");
    unsafe {
        let bi = &*(boot_info_phys as *const BootInfo);
        if bi.magic == BOOTINFO_MAGIC {
            if let Some(max_end) = bootinfo_max_phys_end(bi) {
                set_hhdm_phys_limit(max_end);
            }
        }
    }

    serial_write_str("  bootinfo: hhdm_phys_limit=0x");
    serial_write_hex_u64(hhdm_phys_limit());
    serial_write_str("\n");

    serial_write_str("  phys_alloc: init from bootinfo...\n");

    unsafe { phys_alloc_init_from_bootinfo(&*(boot_info_phys as *const BootInfo)) };

    serial_write_str("  phys_alloc: init OK\n");
    serial_write_str("  paging: init...\n");
    init_paging();
    serial_write_str("  paging: init OK\n");

    // Relocate the stack into HHDM without returning to the old stack.
    // We must not switch stacks mid-Rust-frame and then keep using locals.
    serial_write_str("  stack: allocating page...\n");
    if let Some(stack_phys) = phys_alloc_page() {
        serial_write_str("  stack: phys=0x");
        serial_write_hex_u64(stack_phys);
        serial_write_str("\n");
        let top = phys_to_virt(stack_phys + 4096);
        let new_rsp = (top & !0xF).wrapping_sub(8);
        let delta = KERNEL_VIRT_DELTA.load(Ordering::Relaxed);
        let entry = (kernel_after_paging as usize as u64).wrapping_add(delta);
        unsafe {
            asm!(
                "mov rsp, {stack}",
                "xor rbp, rbp",
                "jmp {entry}",
                stack = in(reg) new_rsp,
                entry = in(reg) entry,
                in("rdi") rsdp_phys,
                options(noreturn)
            );
        }
    }

    // If stack allocation failed, continue on the current stack (still mapped).
    kernel_after_paging(rsdp_phys)
}

#[no_mangle]
extern "C" fn kernel_after_paging(rsdp_phys: u64) -> ! {
    serial_write_str("RayOS kernel-bare: kernel_after_paging\n");
    // Now executing from the higher-half kernel mapping; rebuild paging to remove
    // low identity mappings entirely.
    serial_write_str("  paging: final_no_identity...\n");
    init_paging_final_no_identity();
    serial_write_str("  paging: final_no_identity OK\n");

    // From here on, prefer HHDM virtual addresses for physical resources.
    unsafe {
        if FB_BASE != 0 {
            FB_BASE = phys_to_virt(FB_BASE as u64) as usize;
        }
    }

    serial_write_str("  fb: remapped\n");

    // Install a known-good GDT+TSS (for IST-based fault containment).
    serial_write_str("  gdt: init...\n");
    init_gdt();
    serial_write_str("  gdt: init OK\n");

    // ACPI + interrupt bring-up
    serial_write_str("  idt: install...\n");
    cli();
    unsafe {
        idt_set_gate(TIMER_VECTOR, isr_timer as *const () as u64);
        idt_set_gate(KEYBOARD_VECTOR, isr_keyboard as *const () as u64);

        idt_set_gate(UD_VECTOR, isr_invalid_opcode as *const () as u64);
        idt_set_gate(PF_VECTOR, isr_page_fault as *const () as u64);
        idt_set_gate(GP_VECTOR, isr_general_protection as *const () as u64);
        idt_set_gate_ist(DF_VECTOR, isr_double_fault as *const () as u64, DF_IST_INDEX);

        lidt();
    }
    serial_write_str("  idt: install OK\n");

    // Best-effort APIC discovery via ACPI MADT.
    serial_write_str("  acpi: probing MADT...\n");
    if rsdp_phys != 0 {
        if let Some((_madt, lapic_phys, ioapic_phys, irq0_gsi, irq0_flags, irq1_gsi, irq1_flags)) =
            acpi_find_madt(rsdp_phys)
        {
            unsafe {
                LAPIC_MMIO = lapic_phys + HHDM_OFFSET;
                IOAPIC_MMIO = ioapic_phys + HHDM_OFFSET;
                IRQ0_GSI = irq0_gsi;
                IRQ0_FLAGS = irq0_flags;
                IRQ1_GSI = irq1_gsi;
                IRQ1_FLAGS = irq1_flags;
            }

            serial_write_str("  MADT: LAPIC=0x");
            serial_write_hex_u64(lapic_phys);
            serial_write_str(" IOAPIC=0x");
            serial_write_hex_u64(ioapic_phys);
            serial_write_str(" IRQ0_GSI=0x");
            serial_write_hex_u64(irq0_gsi as u64);
            serial_write_str("\n");

            pic_mask_all();
            lapic_enable();
            if ioapic_phys != 0 {
                ioapic_set_redir(irq0_gsi, TIMER_VECTOR, 0, irq0_flags);
                ioapic_set_redir(irq1_gsi, KEYBOARD_VECTOR, 0, irq1_flags);
            } else {
                pic_remap_and_unmask_irq0();
                pic_unmask_irq1();
            }
        } else {
            // Fallback: PIC timer
            serial_write_str("  MADT not found; using PIC timer\n");
            pic_remap_and_unmask_irq0();
            pic_unmask_irq1();
        }
    } else {
        serial_write_str("  RSDP missing; using PIC timer\n");
        pic_remap_and_unmask_irq0();
        pic_unmask_irq1();
    }

    pit_init_hz(100);
    sti();

    init_memory();
    kernel_main()
}

fn kernel_main() -> ! {
    // Framebuffer is now initialized by _start from bootloader parameters

    // Clear screen to dark blue
    clear_screen(0x1a_1a_2e);

    // Draw kernel banner
    let panel_bg = 0x2a_2a_4e;
    draw_box(30, 30, 700, 450, panel_bg);
    draw_text_bg(50, 50, "RayOS Kernel v0.1 - LIVE!", 0xff_ff_ff, panel_bg);

    // System status
    draw_text_bg(50, 100, "Hardware Initialization:", 0xff_ff_88, panel_bg);
    draw_text_bg(70, 130, "[OK] IDT: Interrupt Descriptor Table", 0x88_ff_88, panel_bg);
    draw_text_bg(70, 160, "[OK] GDT: Global Descriptor Table", 0x88_ff_88, panel_bg);
    draw_text_bg(70, 190, "[OK] Memory Manager: Active", 0x88_ff_88, panel_bg);
    draw_text_bg(70, 220, "[OK] Framebuffer: Active", 0x88_ff_88, panel_bg);

    draw_text_bg(50, 270, "Subsystems:", 0xff_ff_88, panel_bg);
    clear_text_line(70, 300, 48, panel_bg);
    draw_text_bg(70, 300, "[ ] System 1: GPU Reflex Engine", 0xaa_aa_aa, panel_bg);
    draw_text_bg(70, 330, "[ ] System 2: LLM Cognitive Engine", 0xaa_aa_aa, panel_bg);
    draw_text_bg(70, 360, "[ ] Conductor: Task Orchestration", 0xaa_aa_aa, panel_bg);
    draw_text_bg(70, 390, "[ ] Volume: Persistent Storage", 0xaa_aa_aa, panel_bg);
    draw_text_bg(70, 420, "[ ] Intent: Natural Language Parser", 0xaa_aa_aa, panel_bg);

    // IDT/GDT are initialized during early boot in `_start`.
    draw_text_bg(70, 130, "[OK] IDT: Interrupt Descriptor Table", 0x00_ff_00, panel_bg);
    draw_text_bg(70, 160, "[OK] GDT: Global Descriptor Table", 0x00_ff_00, panel_bg);
    draw_text_bg(70, 190, "[OK] Memory Manager: Active", 0x00_ff_00, panel_bg);

    // Test the allocator
    let test_alloc = kalloc(4096, 4096);
    if test_alloc.is_some() {
        draw_text_bg(70, 220, "[OK] Zero-Copy Allocator: VERIFIED", 0x00_ff_00, panel_bg);
    } else {
        draw_text_bg(70, 220, "[!!] Zero-Copy Allocator: FAILED", 0xff_00_00, panel_bg);
    }

    // GPU INITIALIZATION - System 1
    // Temporarily disabled until we verify kernel stability
    clear_text_line(70, 300, 48, panel_bg);
    draw_text_bg(70, 300, "[..] System 1: GPU Init (stub)", 0xff_ff_00, panel_bg);

    // Re-enabled: a safe PCI probe that never touches a GPU driver.
    // This gives us a real signal in logs/UI without risking instability.
    let gpu_detected = pci_probe_display_controller_bus0().is_some();
    serial_write_str("[gpu] pci display controller: ");
    if gpu_detected {
        serial_write_str("present\n");
    } else {
        serial_write_str("absent\n");
    }

    clear_text_line(70, 300, 48, panel_bg);
    if gpu_detected {
        draw_text_bg(70, 300, "[OK] System 1: GPU Detected (reflex loop)", 0x00_ff_88, panel_bg);
    } else {
        draw_text_bg(70, 300, "[--] System 1: No PCI GPU (reflex loop)", 0xaa_aa_aa, panel_bg);
    }
    // System 2 starts as a deterministic parser stub integrated with System 1 via the ray queue.
    clear_text_line(70, 330, 48, panel_bg);
    draw_text_bg(70, 330, "[OK] System 2: Ready (parser stub)", 0xff_88_00, panel_bg);

    // Intent is provided by System 2 in the current kernel build (deterministic NL parser stub).
    clear_text_line(70, 420, 48, panel_bg);
    draw_text_bg(70, 420, "[OK] Intent: Natural Language Parser", 0x00_ff_00, panel_bg);

    SYSTEM1_RUNNING.store(true, Ordering::Relaxed);

    // Conductor starts inside the kernel (minimal orchestrator stub for now).
    CONDUCTOR_RUNNING.store(true, Ordering::Relaxed);
    // Seed a deterministic task so the orchestrator demonstrates activity after boot.
    // This feeds System 2, which enqueues rays for System 1 to process.
    let _ = conductor_enqueue(b"find now");
    clear_text_line(70, 360, 48, panel_bg);
    draw_text_bg(70, 360, "[OK] Conductor: Task Orchestration", 0x00_ff_00, panel_bg);

    // Host-side Ouroboros runs outside the guest; show a small badge when the host bridge is actually active.
    draw_box(520, 360, 200, 20, panel_bg);
    draw_text(520, 360, "Ouro(host):", 0xaa_aa_aa);
    if HOST_AI_ENABLED {
        if HOST_BRIDGE_CONNECTED.load(Ordering::Relaxed) {
            draw_text(570, 360, "[OK] host", 0x00_ff_00);
        } else {
            draw_text(570, 360, "[..] wait", 0xff_ff_00);
        }
    } else {
        if LOCAL_AI_ENABLED {
            draw_text(570, 360, "[--] local", 0xaa_aa_aa);
        } else {
            draw_text(570, 360, "[--] off", 0xaa_aa_aa);
        }
    }

    // Volume: probe + mount the persistent log.
    let vol_ok = volume_init();
    clear_text_line(70, 390, 48, panel_bg);
    if vol_ok {
        draw_text_bg(70, 390, "[OK] Volume: Persistent Storage", 0x00_ff_00, panel_bg);
    } else {
        draw_text_bg(70, 390, "[!!] Volume: Not Found", 0xff_00_00, panel_bg);
    }

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
    // Show keyboard state
    draw_text_bg(50, 520, "Keyboard: scancode=", 0xaa_aa_aa, 0x1a_1a_2e);
    draw_box(210, 520, 200, 20, 0x1a_1a_2e);

    // Show typed input
    draw_text_bg(50, 540, "Typed:", 0xaa_aa_aa, 0x1a_1a_2e);
    draw_box(110, 540, 620, 20, 0x1a_1a_2e);

    // Show response/status for the last submitted line.
    draw_text_bg(50, 560, "Response:", 0xaa_aa_aa, 0x1a_1a_2e);
    draw_box(140, 560, 590, 20, 0x1a_1a_2e);

    // Chat transcript area.
    draw_text_bg(50, 590, "Transcript:", 0xaa_aa_aa, 0x1a_1a_2e);
    draw_box(50, 610, 680, 180, 0x1a_1a_2e);

    // Bicameral interactive loop:
    // - Default: free-form text is fed to System 2, which enqueues rays.
    // - Prefix ':' to run debug shell commands (help/mem/irq/s1/s2/etc).
    serial_write_str("RayOS bicameral loop ready (':' for shell)\n");

    let mut line_buf = [0u8; 128];
    let mut len: usize = 0;
    render_input_line(&line_buf, len);

    // Serial AI response buffer (reads from COM1).
    let mut ai_buf = [0u8; 256];
    let mut ai_len: usize = 0;

    // Chat transcript + protocol state.
    let mut chat = ChatLog::new();
    let mut next_msg_id: u32 = 1;
    let mut pending_id: u32 = 0;
    let mut pending_thinking: bool = false;

    // Seed transcript with a short hint (and also print to serial for headless logs).
    if HOST_AI_ENABLED {
        let msg = b"host AI bridge enabled; type a request";
        chat.push_line(b"SYS: ", msg);
        serial_write_str("SYS: host AI bridge enabled; type a request\n");
    } else if LOCAL_AI_ENABLED {
        let msg = b"local AI enabled (in-guest); type a request";
        chat.push_line(b"SYS: ", msg);
        serial_write_str("SYS: local AI enabled (in-guest); type a request\n");
    } else {
        let msg = b"AI disabled; type a request anyway";
        chat.push_line(b"SYS: ", msg);
        serial_write_str("SYS: AI disabled; type a request anyway\n");
    }
    render_chat_log(&chat);

    let mut last_tick = TIMER_TICKS.load(Ordering::Relaxed);

    fn trim_ascii_spaces(mut s: &[u8]) -> &[u8] {
        while !s.is_empty() {
            let b = s[0];
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                s = &s[1..];
            } else {
                break;
            }
        }
        while !s.is_empty() {
            let b = s[s.len() - 1];
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                s = &s[..s.len() - 1];
            } else {
                break;
            }
        }
        s
    }

    fn eq_ignore_ascii_case(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut i = 0;
        while i < a.len() {
            let mut ca = a[i];
            let mut cb = b[i];
            if ca >= b'A' && ca <= b'Z' {
                ca = ca + 32;
            }
            if cb >= b'A' && cb <= b'Z' {
                cb = cb + 32;
            }
            if ca != cb {
                return false;
            }
            i += 1;
        }
        true
    }

    fn starts_with_ignore_ascii_case(s: &[u8], prefix: &[u8]) -> bool {
        if s.len() < prefix.len() {
            return false;
        }
        eq_ignore_ascii_case(&s[..prefix.len()], prefix)
    }

    fn is_show_linux_desktop(line: &[u8]) -> bool {
        let s = trim_ascii_spaces(line);

        // Fast path for exact phrases.
        if eq_ignore_ascii_case(s, b"show linux desktop")
            || eq_ignore_ascii_case(s, b"show desktop")
            || eq_ignore_ascii_case(s, b"linux desktop")
        {
            return true;
        }

        // Token-aware tolerant match.
        // This intentionally accepts "show linu desktop" as a fallback when the 'x'
        // key is currently producing a space.
        let mut toks: [&[u8]; 4] = [&[]; 4];
        let mut nt: usize = 0;
        let mut i: usize = 0;
        while i < s.len() {
            while i < s.len() {
                let b = s[i];
                if b == b' ' || b == b'\t' {
                    i += 1;
                } else {
                    break;
                }
            }
            if i >= s.len() {
                break;
            }
            let start = i;
            while i < s.len() {
                let b = s[i];
                if b == b' ' || b == b'\t' {
                    break;
                }
                i += 1;
            }
            if nt < toks.len() {
                toks[nt] = &s[start..i];
                nt += 1;
            } else {
                break;
            }
        }

        // Tolerant token matching.
        // We require the intent words to appear, but allow extra tokens.
        let mut seen_show = false;
        let mut seen_desktop = false;
        let mut seen_linux = false;
        let mut t = 0usize;
        while t < nt {
            let tok = toks[t];
            if eq_ignore_ascii_case(tok, b"show") {
                seen_show = true;
            } else if eq_ignore_ascii_case(tok, b"desktop") {
                seen_desktop = true;
            } else if eq_ignore_ascii_case(tok, b"linux") || eq_ignore_ascii_case(tok, b"linu") {
                seen_linux = true;
            }
            t += 1;
        }

        // Accept either:
        // - "show" + "linux/linu" + "desktop" in any order
        // - or just "linux/linu" + "desktop" (for shorter commands)
        (seen_linux && seen_desktop && seen_show) || (seen_linux && seen_desktop)
    }

    fn parse_linux_sendtext(line: &[u8]) -> Option<&[u8]> {
        let s = trim_ascii_spaces(line);

        // Supported forms:
        // - type <text>
        // - send <text>
        // - linux type <text>
        // - linux send <text>
        const P1: &[u8] = b"type ";
        const P2: &[u8] = b"send ";
        const P3: &[u8] = b"linux type ";
        const P4: &[u8] = b"linux send ";

        let rest = if starts_with_ignore_ascii_case(s, P3) {
            &s[P3.len()..]
        } else if starts_with_ignore_ascii_case(s, P4) {
            &s[P4.len()..]
        } else if starts_with_ignore_ascii_case(s, P1) {
            &s[P1.len()..]
        } else if starts_with_ignore_ascii_case(s, P2) {
            &s[P2.len()..]
        } else {
            return None;
        };

        let rest = trim_ascii_spaces(rest);
        if rest.is_empty() {
            return None;
        }
        Some(rest)
    }

    fn is_linux_shutdown(line: &[u8]) -> bool {
        let s = trim_ascii_spaces(line);

        // Fast path.
        if eq_ignore_ascii_case(s, b"shutdown linux")
            || eq_ignore_ascii_case(s, b"shutdown linux desktop")
            || eq_ignore_ascii_case(s, b"stop linux")
            || eq_ignore_ascii_case(s, b"stop linux desktop")
        {
            return true;
        }

        // Tolerant token match: require shutdown/stop + linux/linu.
        let mut toks: [&[u8]; 4] = [&[]; 4];
        let mut nt: usize = 0;
        let mut i: usize = 0;
        while i < s.len() {
            while i < s.len() {
                let b = s[i];
                if b == b' ' || b == b'\t' {
                    i += 1;
                } else {
                    break;
                }
            }
            if i >= s.len() {
                break;
            }
            let start = i;
            while i < s.len() {
                let b = s[i];
                if b == b' ' || b == b'\t' {
                    break;
                }
                i += 1;
            }
            if nt < toks.len() {
                toks[nt] = &s[start..i];
                nt += 1;
            } else {
                break;
            }
        }

        let mut seen_shutdown = false;
        let mut seen_linux = false;
        let mut t = 0usize;
        while t < nt {
            let tok = toks[t];
            if eq_ignore_ascii_case(tok, b"shutdown") || eq_ignore_ascii_case(tok, b"stop") {
                seen_shutdown = true;
            } else if eq_ignore_ascii_case(tok, b"linux") || eq_ignore_ascii_case(tok, b"linu") {
                seen_linux = true;
            }
            t += 1;
        }

        seen_shutdown && seen_linux
    }

    fn parse_linux_sendkey(line: &[u8]) -> Option<&[u8]> {
        let s = trim_ascii_spaces(line);

        // Supported forms:
        // - press <key>
        // - key <key>
        // - linux press <key>
        // - linux key <key>
        const P1: &[u8] = b"press ";
        const P2: &[u8] = b"key ";
        const P3: &[u8] = b"linux press ";
        const P4: &[u8] = b"linux key ";

        let rest = if starts_with_ignore_ascii_case(s, P3) {
            &s[P3.len()..]
        } else if starts_with_ignore_ascii_case(s, P4) {
            &s[P4.len()..]
        } else if starts_with_ignore_ascii_case(s, P1) {
            &s[P1.len()..]
        } else if starts_with_ignore_ascii_case(s, P2) {
            &s[P2.len()..]
        } else {
            return None;
        };

        let rest = trim_ascii_spaces(rest);
        if rest.is_empty() {
            return None;
        }
        Some(rest)
    }

    loop {
        // Drain available keyboard input without blocking.
        while let Some(b) = kbd_try_read_byte() {
            match b {
                b'\n' => {
                    serial_write_str("\n");

                    // Process entered line.
                    if len != 0 {
                        if line_buf[0] == b':' {
                            // Shell command (strip leading ':').
                            shell_execute(&line_buf[1..len]);

                            // Provide visible feedback in the framebuffer UI.
                            draw_box(140, 560, 590, 20, 0x1a_1a_2e);
                            draw_text(140, 560, "shell ok (see serial)", 0xff_ff_88);
                        } else {
                            // Host-integrated commands (non-shell): keep these simple and explicit.
                            // This is a stepping stone until the Linux desktop is actually embedded.
                            if is_show_linux_desktop(&line_buf[..len]) {
                                serial_write_str("RAYOS_HOST_EVENT:SHOW_LINUX_DESKTOP\n");
                                chat.push_line(b"SYS: ", b"requesting Linux desktop (host will launch)");
                                render_chat_log(&chat);
                                draw_box(140, 560, 590, 20, 0x1a_1a_2e);
                                draw_text(140, 560, "launching Linux desktop...", 0xff_ff_88);
                            } else if let Some(text) = parse_linux_sendtext(&line_buf[..len]) {
                                serial_write_str("RAYOS_HOST_EVENT:LINUX_SENDTEXT:");
                                for &b in text {
                                    serial_write_byte(b);
                                }
                                serial_write_str("\n");
                                chat.push_line(b"SYS: ", b"typing into Linux desktop (host inject)");
                                render_chat_log(&chat);
                                draw_box(140, 560, 590, 20, 0x1a_1a_2e);
                                draw_text(140, 560, "typing into Linux desktop...", 0xff_ff_88);
                            } else if is_linux_shutdown(&line_buf[..len]) {
                                serial_write_str("RAYOS_HOST_EVENT:LINUX_SHUTDOWN\n");
                                chat.push_line(b"SYS: ", b"requesting Linux shutdown (host will stop VM)");
                                render_chat_log(&chat);
                                draw_box(140, 560, 590, 20, 0x1a_1a_2e);
                                draw_text(140, 560, "shutting down Linux desktop...", 0xff_ff_88);
                            } else if let Some(keyspec) = parse_linux_sendkey(&line_buf[..len]) {
                                serial_write_str("RAYOS_HOST_EVENT:LINUX_SENDKEY:");
                                for &b in keyspec {
                                    serial_write_byte(b);
                                }
                                serial_write_str("\n");
                                chat.push_line(b"SYS: ", b"sending key to Linux desktop (host inject)");
                                render_chat_log(&chat);
                                draw_box(140, 560, 590, 20, 0x1a_1a_2e);
                                draw_text(140, 560, "sending key to Linux desktop...", 0xff_ff_88);
                            } else {
                            // Update transcript with the user's line.
                            chat.push_line(b"YOU: ", &line_buf[..len]);

                            // If host bridge mode is enabled, emit the request tagged with a message id.
                            // Otherwise, answer locally.
                            let msg_id = next_msg_id;
                            next_msg_id = next_msg_id.wrapping_add(1);

                            if HOST_AI_ENABLED {
                                serial_write_tagged_input(msg_id, &line_buf[..len]);
                                pending_id = msg_id;
                                pending_thinking = true;
                                chat.push_line(b"AI: ", b"(thinking...)");
                                render_chat_log(&chat);
                            }

                            if LOCAL_AI_ENABLED {
                                let mut reply_buf = [0u8; CHAT_MAX_COLS];
                                let reply_len = local_ai_reply(&line_buf[..len], &mut reply_buf);
                                let reply = &reply_buf[..reply_len];

                                // Also print to serial so headless logs can see local replies.
                                serial_write_str("AI_LOCAL:");
                                for &b in reply {
                                    serial_write_byte(b);
                                }
                                serial_write_str("\n");

                                // Render immediately.
                                chat.push_line(b"AI: ", reply);
                                render_chat_log(&chat);
                                render_ai_text_line(reply);
                            }

                            let (count, pushed, op, prio, hash) = system2_submit_text(&line_buf[..len]);
                            serial_write_str("s2 submit count=0x");
                            serial_write_hex_u64(count as u64);
                            serial_write_str(" pushed=0x");
                            serial_write_hex_u64(pushed);
                            serial_write_str(" op=0x");
                            serial_write_hex_u64(op as u64);
                            serial_write_str(" prio=0x");
                            serial_write_hex_u64(prio as u64);
                            serial_write_str(" hash=0x");
                            serial_write_hex_u64(hash);
                            serial_write_str("\n");

                            // Provide human-readable feedback in the framebuffer UI.
                            let _ = count; // keep serial print; avoid unused warnings in some configs
                            render_response_line(&line_buf[..len], pushed, op, prio, hash);
                            }
                        }
                    }

                    // Reset input.
                    len = 0;
                    render_input_line(&line_buf, len);
                }
                0x08 => {
                    // Backspace
                    if len > 0 {
                        len -= 1;
                        render_input_line(&line_buf, len);
                    }
                }
                _ => {
                    // Printable ASCII only.
                    if b >= 0x20 && b <= 0x7E {
                        if len < line_buf.len() {
                            line_buf[len] = b;
                            len += 1;
                            render_input_line(&line_buf, len);
                        }
                    }
                }
            }
        }

        // Update UI on tick changes (driven by PIT interrupt).
        let tick = TIMER_TICKS.load(Ordering::Relaxed);
        if tick != last_tick {
            last_tick = tick;

            // Run a small slice of Conductor orchestration in thread context.
            conductor_tick(tick);

            // Keyboard scancode (hex)
            let sc = LAST_SCANCODE.load(Ordering::Relaxed) as usize;
            draw_box(210, 520, 200, 20, 0x1a_1a_2e);
            draw_hex_number(210, 520, sc, 0x00_ff_ff);

            // Last decoded ASCII byte (hex) to help diagnose keymap issues.
            draw_text(350, 520, "ascii=", 0xaa_aa_aa);
            let last_ascii = LAST_ASCII.load(Ordering::Relaxed) as usize;
            draw_box(410, 520, 60, 20, 0x1a_1a_2e);
            draw_hex_number(410, 520, last_ascii & 0xff, 0x00_ff_ff);

            // System 1 quick stats on the right side of the System 1 line.
            draw_box(520, 300, 200, 20, panel_bg);
            draw_text(520, 300, "q:", 0xaa_aa_aa);
            draw_number(545, 300, rayq_depth(), 0x88_ff_88);
            draw_text(590, 300, "done:", 0xaa_aa_aa);
            draw_number(640, 300, SYSTEM1_PROCESSED.load(Ordering::Relaxed) as usize, 0x88_ff_88);

            // System 2 quick stats on the right side of the System 2 line.
            draw_box(520, 330, 200, 20, panel_bg);
            draw_text(520, 330, "op:", 0xaa_aa_aa);
            draw_number(555, 330, SYSTEM2_LAST_OP.load(Ordering::Relaxed) as usize, 0xff_aa_88);
            draw_text(590, 330, "prio:", 0xaa_aa_aa);
            draw_number(640, 330, SYSTEM2_LAST_PRIO.load(Ordering::Relaxed) as usize, 0xff_aa_88);

            // Host bridge / Ouroboros badge next to Conductor.
            draw_box(520, 360, 200, 20, panel_bg);
            draw_text(520, 360, "Ouro(host):", 0xaa_aa_aa);
            if HOST_AI_ENABLED {
                if HOST_BRIDGE_CONNECTED.load(Ordering::Relaxed) {
                    draw_text(570, 360, "[OK] host", 0x00_ff_00);
                } else {
                    draw_text(570, 360, "[..] wait", 0xff_ff_00);
                }
            } else {
                if LOCAL_AI_ENABLED {
                    draw_text(570, 360, "[--] local", 0xaa_aa_aa);
                } else {
                    draw_text(570, 360, "[--] off", 0xaa_aa_aa);
                }
            }

            // Light “activity indicator” box.
            let blink = (tick & 0x10) != 0;
            let color = if blink { 0x00_ff_00 } else { 0x00_44_00 };
            draw_box(490, 300, 16, 16, color);
            let color2 = if blink { 0xff_88_00 } else { 0x44_22_00 };
            draw_box(490, 330, 16, 16, color2);
        }

        // Drain any serial input (host->guest).
        // - AI:* lines update chat/response UI (host bridge).
        // - CORTEX:* lines are sensory events injected by the Cortex daemon.
        while let Some(b) = serial_try_read_byte() {
            if b == b'\r' {
                continue;
            }
            if b == b'\n' {
                if bytes_starts_with(&ai_buf[..ai_len], b"CORTEX:") {
                    cortex_handle_line(&ai_buf[..ai_len]);
                } else if ai_len >= 3 && ai_buf[0] == b'A' && ai_buf[1] == b'I' {
                    // Supported lines:
                    // - AI:<id>:<text>
                    // - AI_END:<id>
                    // Back-compat:
                    // - AI:<text>
                    if bytes_starts_with(&ai_buf[..ai_len], b"AI_END:") {
                        let id = parse_u32_decimal(&ai_buf[7..ai_len]).unwrap_or(0);
                        if id == pending_id {
                            pending_id = 0;
                            pending_thinking = false;
                        }
                    } else if ai_len >= 3 && ai_buf[0] == b'A' && ai_buf[1] == b'I' && ai_buf[2] == b':' {
                        // Try parse id prefix.
                        let mut i = 3usize;
                        while i < ai_len && ai_buf[i] != b':' {
                            i += 1;
                        }

                        if i < ai_len {
                            let id = parse_u32_decimal(&ai_buf[3..i]).unwrap_or(0);
                            let payload = &ai_buf[(i + 1)..ai_len];

                            // Seeing a reply implies the host bridge is alive.
                            HOST_BRIDGE_CONNECTED.store(true, Ordering::Relaxed);

                            // Only treat it as "in-flight" if ids match; otherwise still display.
                            if pending_thinking && id == pending_id {
                                chat.replace_last_line(b"AI: ", payload);
                                pending_thinking = false;
                            } else {
                                chat.push_line(b"AI: ", payload);
                            }
                            render_chat_log(&chat);
                            render_ai_text_line(payload);
                        } else {
                            // No second ':' found; treat as plain text.
                            let payload = &ai_buf[3..ai_len];

                            // Seeing a reply implies the host bridge is alive.
                            HOST_BRIDGE_CONNECTED.store(true, Ordering::Relaxed);
                            if pending_thinking {
                                chat.replace_last_line(b"AI: ", payload);
                                pending_thinking = false;
                            } else {
                                chat.push_line(b"AI: ", payload);
                            }
                            render_chat_log(&chat);
                            render_ai_text_line(payload);
                        }
                    }
                }
                ai_len = 0;
                continue;
            }
            if ai_len < ai_buf.len() {
                ai_buf[ai_len] = b;
                ai_len += 1;
            }
        }

        // Idle until the next interrupt (timer/keyboard).
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)); }
    }
}

fn render_input_line(buf: &[u8], len: usize) {
    // Render the current line buffer into the framebuffer 'Typed:' line.
    draw_box(110, 540, 620, 20, 0x1a_1a_2e);
    let mut x = 110;
    for i in 0..len {
        let b = buf[i];
        if b >= 0x20 && b <= 0x7E {
            draw_char_bg(x, 540, b as char, 0x00_ff_ff, 0x1a_1a_2e);
            x += FONT_WIDTH;
        }
        if x >= 110 + 620 {
            break;
        }
    }
}

fn s2_op_label(op: u8) -> &'static str {
    match op {
        0 => "chat",
        1 => "open",
        2 => "close",
        3 => "search",
        4 => "write",
        _ => "unknown",
    }
}

fn s2_prio_label(prio: u8) -> &'static str {
    match prio {
        0 => "low",
        1 => "normal",
        2 => "urgent",
        _ => "?",
    }
}

fn render_response_line(input: &[u8], pushed: u64, op: u8, prio: u8, hash: u64) {
    // Single-line, deterministic, heap-free.
    draw_box(140, 560, 590, 20, 0x1a_1a_2e);

    draw_text(140, 560, "heard:", 0xaa_aa_aa);
    let mut x = 200;
    for &b in input.iter().take(24) {
        if b >= 0x20 && b <= 0x7E {
            draw_char_bg(x, 560, b as char, 0xff_ff_ff, 0x1a_1a_2e);
            x += FONT_WIDTH;
        }
        if x >= 140 + 590 {
            break;
        }
    }

    draw_text(420, 560, "->", 0xaa_aa_aa);
    draw_text(440, 560, s2_op_label(op), 0xff_ff_88);
    draw_text(500, 560, "prio:", 0xaa_aa_aa);
    draw_text(540, 560, s2_prio_label(prio), 0xff_aa_88);
    draw_text(600, 560, "q:", 0xaa_aa_aa);
    draw_number(620, 560, pushed as usize, 0x00_ff_ff);
    draw_text(660, 560, "h:", 0xaa_aa_aa);
    draw_hex_number(680, 560, (hash & 0xffff) as usize, 0x88_ff_88);
}

fn render_ai_text_line(text: &[u8]) {
    // Render host-provided AI output into the existing Response line area.
    draw_box(140, 560, 590, 20, 0x1a_1a_2e);
    draw_text(140, 560, "AI:", 0x88_ff_88);

    let mut x = 170;
    for &b in text.iter().take(80) {
        if b >= 0x20 && b <= 0x7E {
            draw_char_bg(x, 560, b as char, 0xff_ff_ff, 0x1a_1a_2e);
            x += FONT_WIDTH;
        }
        if x >= 140 + 590 {
            break;
        }
    }
}

fn render_chat_log(chat: &ChatLog) {
    // Clear transcript box.
    draw_box(50, 610, 680, 180, 0x1a_1a_2e);

    let mut y = 610;
    for i in 0..CHAT_MAX_LINES {
        if let Some((line, len)) = chat.get_nth_oldest(i) {
            draw_chat_line(60, y, line, len);
            y += 18;
        }
    }
}

fn draw_chat_line(x0: usize, y: usize, line: &[u8], len: usize) {
    let mut x = x0;
    for &b in line.iter().take(len) {
        if b >= 0x20 && b <= 0x7E {
            draw_char_bg(x, y, b as char, 0xff_ff_ff, 0x1a_1a_2e);
            x += FONT_WIDTH;
        }
        if x >= 50 + 680 {
            break;
        }
    }
}

fn parse_u32_decimal(buf: &[u8]) -> Option<u32> {
    let mut v: u32 = 0;
    let mut any = false;
    for &b in buf {
        if b == b' ' {
            continue;
        }
        if b < b'0' || b > b'9' {
            break;
        }
        any = true;
        v = v.saturating_mul(10).saturating_add((b - b'0') as u32);
    }
    if any { Some(v) } else { None }
}

fn init_memory() {
    // Prefer a heap carved from real physical memory via the UEFI memory map.
    if let Some(heap_phys) = phys_alloc_bytes(HEAP_SIZE, 4096) {
        // Use HHDM to avoid assuming identity mapping.
        let heap_virt = phys_to_virt(heap_phys) as usize;
        HEAP_ALLOCATOR.lock().init(heap_virt, HEAP_SIZE);
        return;
    }

    // Fallback: static heap.
    let heap_ptr = unsafe { HEAP.0.as_ptr() as usize };
    HEAP_ALLOCATOR.lock().init(heap_ptr, HEAP_SIZE);
}

/// Public allocation function for kernel use
pub fn kalloc(size: usize, align: usize) -> Option<*mut u8> {
    HEAP_ALLOCATOR.lock().allocate(size, align).map(|addr| addr as *mut u8)
}

/// Get memory statistics
pub fn memory_stats() -> (usize, usize, usize) {
    let allocator = HEAP_ALLOCATOR.lock();
    let used = allocator.allocated_bytes();
    let total = HEAP_SIZE;
    let pages = (ALLOCATED_BYTES.load(Ordering::Relaxed) + 4095) / 4096;
    (used, total, pages)
}

#[inline(always)]
fn halt_spin() {
    unsafe {
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
    core::hint::spin_loop();
}

// Framebuffer operations
fn clear_screen(color: u32) {
    unsafe {
        let fb = FB_BASE as *mut u32;
        // Use stride for proper line-by-line clearing
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

fn draw_char_bg(x: usize, y: usize, ch: char, fg: u32, bg: u32) {
    let glyph = get_glyph(ch);
    for row in 0..FONT_HEIGHT {
        let byte = glyph[row];
        for col in 0..FONT_WIDTH {
            let color = if byte & (1 << (7 - col)) != 0 { fg } else { bg };
            draw_pixel(x + col, y + row, color);
        }
    }
}

fn draw_text(x: usize, y: usize, text: &str, color: u32) {
    for (i, ch) in text.chars().enumerate() {
        draw_char(x + (i * FONT_WIDTH), y, ch, color);
    }
}

fn draw_text_bg(x: usize, y: usize, text: &str, fg: u32, bg: u32) {
    for (i, ch) in text.chars().enumerate() {
        draw_char_bg(x + (i * FONT_WIDTH), y, ch, fg, bg);
    }
}

fn clear_text_line(x: usize, y: usize, max_chars: usize, bg: u32) {
    draw_box(x, y, max_chars * FONT_WIDTH, FONT_HEIGHT, bg);
}

fn draw_number(x: usize, y: usize, mut num: usize, color: u32) {
    let mut digits = [0u8; 20];
    let mut count = 0;

    if num == 0 {
        draw_char(x, y, '0', color);
        return;
    }

    // Manual division to avoid compiler-generated intrinsics that may fail
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

    // Use bit shifting for hex (no division needed)
    while num > 0 {
        digits[count] = (num & 0xF) as u8;
        num = num >> 4;
        count = count.wrapping_add(1);
    }

    // Pad to at least 4 hex digits
    while count < 4 {
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
        'X' => [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
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
        'x' => [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
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
        halt_spin();
    }
}
