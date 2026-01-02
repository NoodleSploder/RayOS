use core::sync::atomic::{AtomicU32, Ordering};

use crate::guest_surface::{self, GuestSurface};

// Dev-only synthetic scanout producer.
//
// This exists purely to validate the "native presentation" blit path end-to-end
// in QEMU before the real virtio-gpu/VMM scanout feed is wired up.
//
// It intentionally assumes the current kernel memory mapping makes this buffer
// accessible when treated as a physical pointer (identity-mapped early bring-up).
// Do NOT rely on this for production; the real scanout path should publish a
// guest-owned physical backing buffer.

const W: usize = 320;
const H: usize = 200;

#[repr(align(16))]
struct AlignedBuf([u32; W * H]);

static mut BUF: AlignedBuf = AlignedBuf([0; W * H]);
static FRAME: AtomicU32 = AtomicU32::new(0);
static LOG_ONCE: AtomicU32 = AtomicU32::new(0);
static PUBLISHED: AtomicU32 = AtomicU32::new(0);

#[inline(always)]
pub fn tick_if_presented() {
    if LOG_ONCE.fetch_add(1, Ordering::Relaxed) == 0 {
        crate::serial_write_str("DEV_SCANOUT: enabled\n");
    }

    // If another producer is already publishing a surface, don't interfere.
    // (We treat "published" as "non-empty".)
    if guest_surface::surface_snapshot().is_none() {
        // Avoid creating references to `static mut` (Rust 2024 UB lint).
        let backing_phys = unsafe { core::ptr::addr_of!(BUF.0) as *const u32 as u64 };

        if PUBLISHED.fetch_add(1, Ordering::Relaxed) == 0 {
            crate::serial_write_str("DEV_SCANOUT: publish surface phys=0x");
            crate::serial_write_hex_u64(backing_phys);
            crate::serial_write_str(" size=");
            crate::serial_write_hex_u64(W as u64);
            crate::serial_write_str("x");
            crate::serial_write_hex_u64(H as u64);
            crate::serial_write_str("\n");
        }

        guest_surface::publish_surface(GuestSurface {
            width: W as u32,
            height: H as u32,
            stride_px: W as u32,
            bpp: 32,
            backing_phys,
        });
    }

    let f = FRAME.fetch_add(1, Ordering::Relaxed);

    // Simple moving gradient + bar; pixels use the same 0x00RRGGBB convention
    // as the rest of the framebuffer UI code.
    unsafe {
        // Avoid creating references to `static mut` (Rust 2024 UB lint).
        let buf = core::ptr::addr_of_mut!(BUF.0) as *mut u32;
        let bar_x = (f as usize) % W;
        let bar_y = ((f as usize) / 2) % H;

        let mut y = 0usize;
        while y < H {
            let gy = ((y * 255) / (H.max(1) - 1).max(1)) as u32;
            let mut x = 0usize;
            while x < W {
                let gx = ((x * 255) / (W.max(1) - 1).max(1)) as u32;
                let mut r = gx;
                let mut g = gy;
                let mut b = (f as u32) & 0xff;

                // Draw a bright moving crosshair.
                if x == bar_x || y == bar_y {
                    r = 0xff;
                    g = 0xff;
                    b = 0xff;
                }

                *buf.add(y * W + x) = (r << 16) | (g << 8) | b;
                x += 1;
            }
            y += 1;
        }
    }

    guest_surface::bump_frame_seq();
}
