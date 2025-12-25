RayOS Boot Information
======================

Architecture: x86_64 (Intel/AMD) UEFI
Bootloader: UEFI x86_64 PE/COFF
System: Bicameral Kernel (System 1 GPU + System 2 LLM)

Files:
- EFI/BOOT/BOOTX64.EFI: x86_64 UEFI bootloader entry point
- EFI/RAYOS/kernel.bin: RayOS kernel binary

Boot Steps:
1. Boot from UEFI firmware (x86_64 VM/Machine)
2. You should see the UEFI bootloader greeting
3. Kernel will be loaded and display status
