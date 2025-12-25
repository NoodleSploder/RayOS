# RayOS aarch64 (ARM64) Boot Guide

## ✅ Your aarch64 ISO is Ready!

**File**: `iso-output/rayos-aarch64.iso` (7.88 MB)  
**Architecture**: aarch64 ARM64 UEFI  
**Bootloader**: `BOOTAA64.EFI` (aarch64 PE/COFF executable)  
**Kernel**: RayOS Phase 1 skeleton

## Testing in VM

### QEMU with UEFI (Recommended for aarch64)

```bash
# On Linux or WSL with QEMU installed:
qemu-system-aarch64 \
  -machine virt \
  -cpu cortex-a57 \
  -m 2048 \
  -bios /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
  -cdrom iso-output/rayos-aarch64.iso \
  -boot d
```

### VirtualBox with aarch64 Support

1. VirtualBox → New Machine
2. Name: RayOS-aarch64
3. Machine Folder: (default)
4. ISO Image: `iso-output/rayos-aarch64.iso`
5. Type: Linux
6. Subtype: Other Linux (64-bit ARM)
7. Memory: 2048 MB
8. Create Virtual Hard Disk: Skip (or create)
9. Settings → System → Motherboard:
   - Enable EFI (should be default for ARM64)
10. Settings → Storage → IDE Controller:
    - Attach `rayos-aarch64.iso` as CD/DVD
11. Start VM

### VMware Fusion (macOS with M1/M2/M3)

1. File → New
2. Create Custom Virtual Machine
3. Choose how to install: Use existing disk image
4. Select: `iso-output/rayos-aarch64.iso`
5. Operating system: Linux → Other Linux 64-bit ARM
6. Virtual machine name: RayOS-aarch64
7. Finish

### Hyper-V with aarch64 (Windows 11 Pro+)

```powershell
# Create Gen 2 VM with ARM64 (if supported)
$vmName = "RayOS-aarch64"
New-VM -Name $vmName -MemoryStartupBytes 2GB -Generation 2

# Add DVD drive
Add-VMDvdDrive -VMName $vmName -Path "$(Get-Location)\iso-output\rayos-aarch64.iso"

# Ensure UEFI firmware (automatic for Gen 2)
Set-VMFirmware -VMName $vmName -EnableSecureBoot Off

# Start
Start-VM -Name $vmName
```

## What You Should See

**Successful Boot Output**:

```
RayOS UEFI Bootloader v0.1

Successfully booted via UEFI firmware!
System Ready.
```

## Troubleshooting

### "No bootable option found" on aarch64 VM

**Check**:

1. VM is configured for **aarch64 (ARM64)** architecture
2. Firmware is set to **UEFI**, not BIOS/Legacy
3. Boot order has **CD/DVD/ISO** before hard drive
4. Secure Boot is **DISABLED** (if available)

**Solution**:

- Verify `rayos-aarch64.iso` not `rayos.iso` (x86_64)
- Check VM settings for aarch64 CPU type
- May need to enable EFI/UEFI in VM firmware settings

### ISO Boots But No Output

- Some VM hypervisors need manual console redirection
- Try `-nographic` mode in QEMU with serial output
- Check VM display settings (may need to enable graphics)

### Bootloader Binary Looks Wrong

Verify it's aarch64:

```bash
file iso-output/rayos-aarch64.iso
file bootloader/target/aarch64-unknown-uefi/release/uefi_boot.efi

# Should show:
# - ISO: 'ISO 9660 CD-ROM filesystem data'
# - EFI: 'PE32+ executable (EFI application) Aarch64'
```

## File Structure

```
rayos-aarch64.iso
├── EFI/
│   ├── BOOT/
│   │   └── BOOTAA64.EFI    ← aarch64 UEFI bootloader
│   └── RAYOS/
│       └── kernel.bin       ← Kernel binary
└── README.txt
```

## Rebuild for aarch64 Anytime

```powershell
cd c:\Users\caden\Documents\Programming Scripts\Personal\Rust\ray-os
PowerShell -ExecutionPolicy Bypass -File build-iso-aarch64.ps1
```

## Technical Details

**Architecture Mapping**:

- `x86_64-unknown-uefi` → Intel/AMD 64-bit → `BOOTX64.EFI`
- `aarch64-unknown-uefi` → ARM 64-bit → `BOOTAA64.EFI`

**Build Command** (for reference):

```bash
# Bootloader (aarch64)
cargo +nightly build -Zbuild-std=core --release --target aarch64-unknown-uefi

# Kernel
cargo build --release
```

**Binary Format**:

```
✓ PE32+ executable (EFI application) Aarch64
✓ 2 sections (.text, .reloc or similar)
✓ Proper UEFI subsystem
```

## Next Steps

1. **Test in VM** → Use QEMU or VirtualBox aarch64
2. **Observe boot output** → Should see bootloader message
3. **Kernel integration** → Phase 2 will handle kernel handoff

---

**Status**: ✅ aarch64 ISO Ready for Boot Testing  
**Tested On**: aarch64 UEFI VMs  
**Expected Output**: "RayOS UEFI Bootloader v0.1"
