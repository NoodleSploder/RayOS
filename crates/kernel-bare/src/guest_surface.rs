use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum PresentationState {
    Hidden = 0,
    Presented = 1,
}

static PRESENTATION_STATE: AtomicU8 = AtomicU8::new(PresentationState::Hidden as u8);

pub fn presentation_state() -> PresentationState {
    match PRESENTATION_STATE.load(Ordering::Relaxed) {
        1 => PresentationState::Presented,
        _ => PresentationState::Hidden,
    }
}

pub fn set_presentation_state(state: PresentationState) {
    PRESENTATION_STATE.store(state as u8, Ordering::Relaxed);
}

#[derive(Copy, Clone)]
pub struct GuestSurface {
    pub width: u32,
    pub height: u32,
    pub stride_px: u32,
    pub bpp: u32,
    pub backing_phys: u64,
}

impl GuestSurface {
    pub const fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            stride_px: 0,
            bpp: 0,
            backing_phys: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.width != 0 && self.height != 0 && self.stride_px != 0 && self.backing_phys != 0
    }
}

// Published guest scanout surface.
//
// This is intentionally lock-free: a future VMM/virtio-gpu model can publish new
// scanout metadata (and eventually frames) from an IRQ/context without taking a
// blocking lock.
//
// Seqlock-style protocol:
// - Writers bump SURFACE_SEQ to an odd value, write fields, then bump to even.
// - Readers sample SURFACE_SEQ, read fields, then re-sample and accept only if
//   the sequence is unchanged and even.
static SURFACE_SEQ: AtomicU64 = AtomicU64::new(0);
static SURFACE_W: AtomicU32 = AtomicU32::new(0);
static SURFACE_H: AtomicU32 = AtomicU32::new(0);
static SURFACE_STRIDE_PX: AtomicU32 = AtomicU32::new(0);
static SURFACE_BPP: AtomicU32 = AtomicU32::new(0);
static SURFACE_BACKING_PHYS: AtomicU64 = AtomicU64::new(0);

// Monotonic counter incremented when a new guest frame is ready.
static FRAME_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn publish_surface(surface: GuestSurface) {
    let seq0 = SURFACE_SEQ.load(Ordering::Relaxed);
    // Mark write in-progress (odd).
    SURFACE_SEQ.store(seq0.wrapping_add(1) | 1, Ordering::Release);

    SURFACE_W.store(surface.width, Ordering::Relaxed);
    SURFACE_H.store(surface.height, Ordering::Relaxed);
    SURFACE_STRIDE_PX.store(surface.stride_px, Ordering::Relaxed);
    SURFACE_BPP.store(surface.bpp, Ordering::Relaxed);
    SURFACE_BACKING_PHYS.store(surface.backing_phys, Ordering::Relaxed);

    // Mark write complete (even).
    SURFACE_SEQ.store(seq0.wrapping_add(2) & !1, Ordering::Release);
}

pub fn clear_surface() {
    publish_surface(GuestSurface::empty());
}

pub fn surface_snapshot() -> Option<GuestSurface> {
    // Small bounded retry loop; avoids livelock if a writer is active.
    for _ in 0..3 {
        let seq1 = SURFACE_SEQ.load(Ordering::Acquire);
        if (seq1 & 1) != 0 {
            continue;
        }

        let surface = GuestSurface {
            width: SURFACE_W.load(Ordering::Relaxed),
            height: SURFACE_H.load(Ordering::Relaxed),
            stride_px: SURFACE_STRIDE_PX.load(Ordering::Relaxed),
            bpp: SURFACE_BPP.load(Ordering::Relaxed),
            backing_phys: SURFACE_BACKING_PHYS.load(Ordering::Relaxed),
        };

        let seq2 = SURFACE_SEQ.load(Ordering::Acquire);
        if seq1 == seq2 {
            return if surface.is_valid() {
                Some(surface)
            } else {
                None
            };
        }
    }
    None
}

pub fn bump_frame_seq() {
    FRAME_SEQ.fetch_add(1, Ordering::Release);
}

pub fn frame_seq() -> u64 {
    FRAME_SEQ.load(Ordering::Acquire)
}
