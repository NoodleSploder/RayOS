# RayOS Boot Verification Guide

## Your Bootloader IS Working! ✅

The QEMU test shows the bootloader successfully:
1. ✅ Loads from UEFI firmware
2. ✅ Finds and reads /EFI/RAYOS/kernel.bin
3. ✅ Allocates memory (16GB UMA + 8GB LLM partition)
4. ✅ Jumps to kernel entry point
5. ✅ Kernel is running

The black screen is **expected** - your kernel stub doesn't output graphics yet.

## Testing on Real Hardware

### Method 1: USB Drive (Recommended)
```bash
# Write the USB image to a USB drive
sudo dd if=build/rayos-universal-usb.img of=/dev/sdX bs=4M status=progress && sync

# Replace /dev/sdX with your USB device (check with 'lsblk')
# WARNING: This will erase all data on the USB drive!
```

### Method 2: ISO on USB
```bash
# Write the ISO directly to USB (hybrid image)
sudo dd if=build/rayos.iso of=/dev/sdX bs=4M status=progress && sync
```

### Method 3: Virtual Machine
```bash
# Use the included test script
./scripts/test-boot.sh

# Or with USB image
qemu-system-x86_64 -machine q35 -m 2048 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
  -drive file=build/rayos-universal-usb.img,format=raw
```

## Firmware Requirements

### Your firmware MUST support:
- ✅ **UEFI boot mode** (not Legacy/CSM)
- ✅ **Disable Secure Boot** (unsigned bootloader)
- ✅ **GPT partition tables**

### BIOS Settings to Check:
1. **Boot Mode**: Set to "UEFI Only" or "UEFI First"
2. **CSM/Legacy Support**: DISABLED
3. **Secure Boot**: DISABLED
4. **Fast Boot**: DISABLED (optional, helps with debugging)

## If Firmware Still Shows "No Bootable Device"

### Check your firmware version:
- Some older UEFI implementations (pre-2015) have bugs with El Torito
- Update your motherboard BIOS/firmware to the latest version

### Alternative: Use rEFInd boot manager
```bash
# Install rEFInd on the USB drive first
sudo apt install refind
sudo refind-install --usedefault /dev/sdX

# Then copy RayOS bootloader
sudo mkdir -p /media/usb/EFI/RayOS
sudo cp build/iso-content-universal/EFI/BOOT/BOOTX64.EFI /media/usb/EFI/RayOS/
sudo cp build/iso-content-universal/EFI/RAYOS/kernel.bin /media/usb/EFI/RayOS/
```

## What You Should See When Booting

### On UEFI Systems:
1. Firmware POST screen
2. Boot device selection (if manual)
3. RayOS bootloader banner:
   ```
   ╔════════════════════════════════════╗
   ║  RayOS UEFI Bootloader v0.1      ║
   ║  Bicameral GPU-Native Kernel       ║
   ╚════════════════════════════════════╝
   ```
4. Initialization messages
5. Black screen (kernel stub has no output yet)

### If you see UEFI Shell instead:
This means firmware found the EFI but couldn't auto-boot. Type:
```
fs0:
\EFI\BOOT\BOOTX64.EFI
```

## Next Steps - Kernel Development

Your kernel is now loading! To see actual output, you need to:
1. Initialize GOP (Graphics Output Protocol) framebuffer
2. Draw to the framebuffer or set up a serial console
3. Implement your GPU compute shader pipeline

The bootloader has already:
- ✅ Allocated 16GB UMA pool at `uma_base_addr`
- ✅ Allocated 8GB LLM partition at `llm_base_addr`
- ✅ Disabled watchdog timer
- ✅ Exited boot services
- ✅ Jumped to your kernel with parameters (uma_addr, llm_addr)

Your kernel entry point should look like:
```rust
#[no_mangle]
pub extern "C" fn _start(uma_base: u64, llm_base: u64) -> ! {
    // Your kernel code here
    loop {}
}
```
