# RayOS ISO Build Guide

This guide explains how to build a bootable UEFI ISO/USB for RayOS.

## Quick Start

### Windows (PowerShell)

```powershell
.\scripts\build-iso.ps1
```

### Linux/macOS/WSL (Bash)

```bash
chmod +x scripts/build-iso.sh
./scripts/build-iso.sh
```

## What the Build Script Does

1. **Builds the UEFI Bootloader** - Compiles the minimal UEFI application that initializes the system
2. **Builds the Kernel** - Compiles the RayOS kernel with GPU integration
3. **Creates EFI Directory Structure** - Sets up the proper boot partition layout
4. **Generates ISO Image** - Creates a hybrid ISO that boots on both UEFI and legacy systems

## Prerequisites

### Required

- **Rust** 1.70+ with nightly toolchain
- **x86_64-unknown-uefi target**: `rustup target add x86_64-unknown-uefi`

### For ISO Creation

- **xorriso** (GNU ISO 9660 Rock Ridge and Joliet filesystem editor)

#### Installation

**Windows:**

```powershell
# Using Chocolatey
choco install xorriso

# Or via WSL (Windows Subsystem for Linux)
wsl sudo apt-get install xorriso
```

**Linux (Ubuntu/Debian):**

```bash
sudo apt-get install xorriso
```

**Linux (Fedora/RHEL):**

```bash
sudo dnf install xorriso
```

**macOS:**

```bash
brew install xorriso
```

## Build Options

### PowerShell (Windows)

```powershell
# Standard build
.\scripts\build-iso.ps1

# Clean build (remove previous artifacts)
.\scripts\build-iso.ps1 -Clean

# Custom output directory
.\scripts\build-iso.ps1 -OutputDir "D:\boot-images"

# Combine options
.\scripts\build-iso.ps1 -Clean -OutputDir "D:\boot-images"
```

### Bash (Linux/macOS/WSL)

```bash
# Standard build
./scripts/build-iso.sh

# Clean build
./scripts/build-iso.sh --clean

# Custom output directory
./scripts/build-iso.sh --output "/tmp/rayos"

# Combine options
./scripts/build-iso.sh --clean --output "/tmp/rayos"
```

## ISO Output

The build script creates:

```
build/
├── rayos.iso                    # Bootable ISO image
└── iso-content/
    ├── EFI/
    │   ├── BOOT/
    │   │   └── BOOTX64.EFI      # UEFI bootloader
    │   └── RAYOS/
    │       └── kernel.bin       # RayOS kernel
    └── README.txt               # Boot information
```

**ISO Size:** ~5-10 MB (depending on kernel size)

## Writing to USB Drive

### Option 1: Rufus (Windows) - Easiest for Beginners

1. Download [Rufus](https://rufus.ie/)
2. Select `rayos.iso` as the image
3. Select your USB device
4. Click **Start**
5. Choose **Write in ISO Image mode**

### Option 2: Balena Etcher (Cross-platform) - Recommended

1. Download [Balena Etcher](https://www.balena.io/etcher/)
2. Click **Flash from file** and select `rayos.iso`
3. Click **Select target** and choose your USB drive
4. Click **Flash**

### Option 3: Command Line

**Linux/macOS:**

```bash
# Find your USB device
lsblk  # or diskutil list (macOS)

# Write ISO (be careful - this erases the drive!)
sudo dd if=rayos.iso of=/dev/sdX bs=4M && sudo sync
```

**Windows (PowerShell):**

```powershell
# Using diskpart
diskpart
# In diskpart:
# list disk
# select disk X  (where X is your USB number)
# clean
# create partition primary
# format fs=fat32
# exit

# Then copy the ISO contents manually or use:
# Physical Disk Imager tool
```

### Option 4: Mount Directly (for testing in VM)

**Windows (PowerShell):**

```powershell
Mount-DiskImage -ImagePath "C:\path\to\rayos.iso"
```

**Linux:**

```bash
sudo mount -o loop rayos.iso /mnt/rayos
```

**macOS:**

```bash
hdiutil attach rayos.iso
```

## BIOS/UEFI Configuration

To boot from your USB drive:

1. **Restart your computer** and enter BIOS/UEFI setup:

   - Dell: F2 or Del
   - HP/Compaq: F10 or Esc
   - Lenovo: F1 or Del
   - ASUS: Del or F2
   - MSI: Del

2. **Enable UEFI boot mode** (disable "Legacy" or "CSM" mode)

3. **Disable Secure Boot** (if enabled) - RayOS kernel is not signed

4. **Change boot order** to prioritize your USB drive

5. **Save and Exit**

6. **Reboot** and you should see the UEFI bootloader output

## Expected Boot Behavior

When booting RayOS, you should see:

```
Hello from RayOS UEFI bootloader!
```

This indicates the bootloader has successfully initialized the system and is preparing to transition to the kernel.

## Troubleshooting

### "ISO creation failed: xorriso not found"

- Install xorriso using the commands above
- Or run script from WSL/Linux environment

### "Bootloader build failed"

- Ensure nightly Rust is installed: `rustup default nightly`
- Verify the x86_64-unknown-uefi target is installed: `rustup target add x86_64-unknown-uefi`
- Run: `cargo clean` in the bootloader directory and retry

### "Kernel executable not found"

- This is non-fatal - the ISO will still boot with the bootloader
- Check kernel compilation output for specific errors

### USB not recognized as bootable

- Ensure you wrote the ISO in **ISO Image mode** (not as a data drive)
- Use Balena Etcher (least prone to user error)
- Verify UEFI is enabled in BIOS and Secure Boot is disabled

### "Mount-DiskImage: The path ... is not a valid Windows Path"

```powershell
# Use absolute path:
Mount-DiskImage -ImagePath "C:\Users\YourName\Documents\rayos.iso"
```

### Black screen after boot

- Check that UEFI mode is enabled (not Legacy BIOS)
- Verify the USB/ISO was written correctly
- Try a different USB port (especially on laptops - avoid front panel USB)

## Advanced: Manual ISO Creation

If the scripts don't work, you can create the ISO manually:

**Windows (PowerShell):**

```powershell
# Build components
cd bootloader
cargo build --release --target x86_64-unknown-uefi
cd ..\kernel
cargo build --release

# Create structure
mkdir iso-content\EFI\BOOT
mkdir iso-content\EFI\RAYOS
copy bootloader\target\x86_64-unknown-uefi\release\uefi_boot.efi iso-content\EFI\BOOT\BOOTX64.EFI
copy kernel\target\release\kernel iso-content\EFI\RAYOS\kernel.bin

# Create ISO using xorriso
xorriso -as mkisofs -R -J -b EFI/BOOT/BOOTX64.EFI -eltorito-alt-boot -e EFI/BOOT/BOOTX64.EFI -no-emul-boot -isohybrid-gpt-basdat -o rayos.iso iso-content\
```

**Linux/macOS:**

```bash
# Build components
cd bootloader
cargo build --release --target x86_64-unknown-uefi
cd ../kernel
cargo build --release

# Create structure
mkdir -p iso-content/EFI/BOOT
mkdir -p iso-content/EFI/RAYOS
cp bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi iso-content/EFI/BOOT/BOOTX64.EFI
cp kernel/target/release/kernel iso-content/EFI/RAYOS/kernel.bin

# Create ISO
xorriso -as mkisofs -R -J -b EFI/BOOT/BOOTX64.EFI -eltorito-alt-boot -e EFI/BOOT/BOOTX64.EFI -no-emul-boot -isohybrid-gpt-basdat -o rayos.iso iso-content/
```

## Next Steps

Once successfully booted:

1. **Monitor boot logs** - Check system initialization and GPU detection
2. **Test GPU integration** - Verify the megakernel GPU compute shader is running
3. **System profiling** - Monitor System 1 (reflex engine) and System 2 (cognitive engine) operation
4. **Iterate** - Rebuild and re-test as needed

## References

- [UEFI Specification](https://uefi.org/specifications)
- [xorriso Documentation](https://www.gnu.org/software/xorriso/xorriso_eng.html)
- [Rust UEFI crate](https://github.com/rust-osdev/uefi-rs)
- [RayOS Architecture](kernel/README.md)
