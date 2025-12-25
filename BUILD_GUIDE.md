# RayOS ISO Build & Boot Guide

## Current Status

✅ **ISO Successfully Created**: `iso-output/rayos.iso` (7.88 MB)
✅ **Bootloader**: `bootloader/uefi_boot.efi` (2,560 bytes)
✅ **Kernel**: `kernel/target/release/rayos-kernel.exe` (7.8 MB)
✅ **ISO Format**: ISO 9660 with UEFI hybrid partition (isohybrid-gpt-basdat)

## Quick Build

### PowerShell (Recommended)

```powershell
# Build bootloader
cd bootloader\uefi_boot
cargo build --release --target x86_64-unknown-uefi
cd ..\..

# Build kernel
cd kernel
cargo build --release
cd ..

# Create ISO structure
$OutputDir = "iso-output"
$IsoContentDir = Join-Path $OutputDir "iso-content"
$BootDir = Join-Path $IsoContentDir "EFI\BOOT"
$RayOSDir = Join-Path $IsoContentDir "EFI\RAYOS"

New-Item -ItemType Directory -Path $BootDir -Force | Out-Null
New-Item -ItemType Directory -Path $RayOSDir -Force | Out-Null

Copy-Item "bootloader\target\x86_64-unknown-uefi\release\uefi_boot.efi" (Join-Path $BootDir "BOOTX64.EFI") -Force
Copy-Item "kernel\target\release\rayos-kernel.exe" (Join-Path $RayOSDir "kernel.bin") -Force

# Create ISO using WSL xorriso
$IsoPathWSL = "/mnt/c/Users/caden/Documents/Programming Scripts/Personal/Rust/ray-os/iso-output/rayos.iso"
$IsoContentPathWSL = "/mnt/c/Users/caden/Documents/Programming Scripts/Personal/Rust/ray-os/iso-output/iso-content"
wsl xorriso -as mkisofs -R -J -V "RayOS" -isohybrid-gpt-basdat -o "$IsoPathWSL" "$IsoContentPathWSL"
```

## Testing the ISO

### Option 1: Virtual Machine (Fast, Recommended)

#### VirtualBox

1. Open VirtualBox → Create New VM
2. Name: "RayOS"
3. Memory: 2048 MB (or more)
4. Virtual Hard Disk: Create (or skip if CD-only)
5. Settings → Storage → Controller IDE:
   - Click the CD icon
   - Select `iso-output/rayos.iso`
6. System → Motherboard → Enable EFI/UEFI
7. Start the VM

#### Hyper-V (Windows)

```powershell
# Create new VM
New-VM -Name RayOS -MemoryStartupBytes 2GB -Generation 2

# Add ISO as DVD drive
Add-VMDvdDrive -VMName RayOS -Path "C:\Users\caden\Documents\Programming Scripts\Personal\Rust\ray-os\iso-output\rayos.iso"

# Start VM
Start-VM -Name RayOS
```

### Option 2: Physical USB Drive (Real Hardware)

#### Using Rufus (Windows) - Best for UEFI

1. Download [Rufus](https://rufus.ie/)
2. Insert USB drive (8GB+)
3. In Rufus:
   - Device: Select your USB drive
   - Boot selection: Select `iso-output/rayos.iso`
   - Partition scheme: **GPT**
   - Target system: **UEFI (non-CSM)**
   - File system: **FAT32**
   - Cluster size: **4096 bytes**
   - Click "START"
4. Warning: This will erase the USB drive!

#### Using Balena Etcher (Windows/Mac/Linux) - Simple

1. Download [Balena Etcher](https://www.balena.io/etcher/)
2. Click "Flash from file"
3. Select `iso-output/rayos.iso`
4. Select USB drive
5. Click "Flash"

#### Using dd (Linux/Mac/WSL)

```bash
# Identify USB drive (e.g., /dev/sdb, /dev/sdX)
lsblk

# Write ISO to USB (REPLACE sdX with your drive!)
sudo dd if=iso-output/rayos.iso of=/dev/sdX bs=4M status=progress
sudo sync
```

### Option 3: Mount ISO Directly (Testing)

```powershell
# Mount ISO
Mount-DiskImage -ImagePath "$(Get-Location)\iso-output\rayos.iso"

# Access files via File Explorer
# Eject when done:
Dismount-DiskImage -ImagePath "$(Get-Location)\iso-output\rayos.iso"
```

## Boot Configuration

### BIOS/UEFI Settings

1. Restart computer
2. Enter BIOS/UEFI:

   - Dell: F2 or F12
   - HP: F2 or Esc
   - Lenovo: F2 or Enter
   - Asus: Del or F2
   - Surface: Volume Up + Power

3. Look for:
   - **Boot Mode**: Change to UEFI (not Legacy/CSM)
   - **Secure Boot**: Try **Disabled** (if bootloader not recognized)
   - **Fast Boot**: Try **Disabled** (for consistency)
   - **Boot Order**: Move USB/CD to top

### Expected Behavior

When the ISO boots, you should see:

- UEFI firmware initializes
- "Hello from RayOS UEFI bootloader!" message
- (The kernel would take over from here - currently Phase 1 skeleton)

## Troubleshooting

### "No bootable option or device was found"

- **Cause 1**: Secure Boot enabled (try disabling)
- **Cause 2**: BIOS not in UEFI mode (enable UEFI)
- **Cause 3**: USB drive not written correctly (try Rufus again with GPT setting)
- **Cause 4**: Bootloader binary not valid (rebuild everything)

**Solution**:

```powershell
# Full clean rebuild
cd bootloader
cargo clean
cd ..\kernel
cargo clean
cd ..

# Then run the build steps above
```

### ISO Won't Mount on Windows

```powershell
# Try mounting with administrator privileges
$iso = "$(Get-Location)\iso-output\rayos.iso"
Mount-DiskImage -ImagePath $iso -StorageType ISO
```

### USB Drive Doesn't Appear in Boot Menu

- Try different USB port
- Format USB drive completely before flashing
- Use Rufus with "Bad blocks check" disabled

## File Structure

```
rayos.iso
├── EFI/
│   ├── BOOT/
│   │   └── BOOTX64.EFI          ← UEFI bootloader
│   └── RAYOS/
│       └── kernel.bin            ← Kernel binary
└── README.txt                     ← Boot info
```

## Dependencies Installed

- Rust nightly (x86_64-pc-windows-msvc)
- Target: x86_64-unknown-uefi
- xorriso 1.5.6 (via WSL)

## Notes

- Phase 1 skeleton: Bootloader boots, kernel is loaded but minimal
- Secure Boot may prevent bootloader from running (disable in BIOS)
- The 7.88 MB ISO is small enough for USB 3.0 or CD-R
- UEFI firmware recognizes .efi files at `EFI/BOOT/BOOTX64.EFI`

## Quick References

- UEFI Specification: https://uefi.org/
- Rust UEFI Crate: https://docs.rs/uefi/
- Bootloader Development: https://github.com/rust-osdev/bootloader
