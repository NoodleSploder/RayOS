#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start(
    fb_base: usize,
    fb_width: usize,
    fb_height: usize,
    fb_stride: usize,
) -> ! {
    // Simple red pixel test - write to a few pixels to verify framebuffer works
    let fb = fb_base as *mut u32;

    unsafe {
        // Write red pixels in a pattern to verify we're running
        for y in 100..120 {
            for x in 100..200 {
                let offset = y * fb_stride + x;
                if offset < fb_width * fb_height {
                    *fb.add(offset) = 0xff0000; // Red
                }
            }
        }

        // Write "KERNEL OK" pattern with different colors
        for y in 150..170 {
            for x in 150..300 {
                let offset = y * fb_stride + x;
                if offset < fb_width * fb_height {
                    *fb.add(offset) = 0x00ff00; // Green
                }
            }
        }
    }

    // Infinite loop to keep kernel running
    loop {
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}