# RayOS Boot Troubleshooting Guide

## Problem: "No bootable option or device was found"

This error occurs when the UEFI firmware cannot find or recognize the bootloader on the media. This guide provides systematic solutions.

## Root Causes & Solutions

### Issue 1: Secure Boot is Enabled (Most Common)

**Symptoms:**

- "No bootable option found" on first boot attempt
- Firmware fails to load the EFI binary
- Works in VirtualBox but not on real hardware

**Solution:**

1. Restart your computer
2. Enter BIOS/UEFI:

   - **Dell**: Press F2 during startup
   - **HP**: Press F2 or Esc
   - **Lenovo**: Press F2 or Fn+F2
   - **Asus**: Press Del or F2
   - **Surface**: Hold Volume Up + Power
   - **Generic**: Try F1, F2, Del, or Esc

3. Find **Security** settings (might be under "System Security")
4. Look for **Secure Boot**
5. **DISABLE** Secure Boot
6. Look for **Fast Boot** - **DISABLE** it as well
7. Save and exit (usually Ctrl+S or F10)
8. Try booting again

### Issue 2: Boot Mode is Legacy/CSM, Not UEFI

**Symptoms:**

- BIOS recognizes USB/CD but won't boot
- System tries to boot but gives error
- Works with old BIOS-format ISOs but not UEFI

**Solution:**

1. Enter BIOS/UEFI
2. Find **Boot Mode** or **BIOS Mode** setting
3. Change from **Legacy** to **UEFI**
4. Save and exit
5. Try booting again

### Issue 3: USB Drive Not Written Correctly

**Symptoms:**

- USB appears in boot menu but "No bootable option"
- Worked on one computer but not another
- ISO worked in VM but not on USB

**Solution - Regenerate USB with Rufus:**

1. Download Rufus from https://rufus.ie/
2. Insert USB drive (will be erased!)
3. In Rufus:
   - **Device**: Select your USB
   - **Boot selection**: Click "SELECT" → choose `iso-output/rayos.iso`
   - **Partition scheme**: Change to **GPT**
   - **Target system**: Change to **UEFI (non-CSM)**
   - **Volume label**: RayOS
   - **File system**: FAT32
   - **Allocation unit size**: 4096 bytes
   - **Options**: Leave defaults
4. Click **START** (all data on USB will be erased!)
5. Wait for completion (2-5 minutes)
6. Safely eject USB
7. Insert into target computer and boot

### Issue 4: Wrong Boot Device Selected

**Symptoms:**

- Error occurs immediately when selecting boot device
- "No bootable option" but other USB drives work
- Boot menu shows the USB but won't boot

**Solution:**

1. Restart and enter BIOS boot menu (during startup, press):
   - **Dell**: F12
   - **HP**: F9
   - **Lenovo**: F12 or F11
   - **Asus**: Esc
   - **Surface**: Hold Volume Down + Power
2. Look for USB entry like:

   - "USB: RayOS"
   - "UEFI: SanDisk USB..."
   - "UEFI: Kingston USB..."

3. **Avoid** entries labeled:

   - Just "USB Device" (might be Legacy)
   - "Hard Drive" or "SATA"

4. Select the **UEFI** version of your USB
5. If no UEFI option appears, the USB may not be written correctly (go to Issue 3)

### Issue 5: BIOS/Firmware Doesn't Support UEFI

**Symptoms:**

- No "UEFI mode" option in BIOS
- No "Secure Boot" option (very old system)
- System is pre-2010 vintage

**Solution:**

- Your system may not support UEFI boot
- RayOS requires UEFI/EFI firmware
- You may need a newer motherboard or system
- Check your system manual for UEFI/EFI support

### Issue 6: ISO File is Corrupted

**Symptoms:**

- Multiple USB writes fail
- Works in some VMs but not others
- Intermittent boot failures

**Solution:**

```powershell
# Verify ISO integrity
$iso = "iso-output\rayos.iso"
$file = Get-Item $iso
$hash = Get-FileHash $iso -Algorithm SHA256
Write-Host "ISO File: $($file.FullName)"
Write-Host "Size: $($file.Length) bytes (should be 8257536)"
Write-Host "SHA256: $($hash.Hash)"

# Rebuild ISO if corrupted
cd "C:\Users\caden\Documents\Programming Scripts\Personal\Rust\ray-os"
PowerShell -ExecutionPolicy Bypass -File build-iso-final.ps1 -Clean
```

## Systematic Testing Approach

### Step 1: Verify ISO Works (Virtual Machine)

Use **VirtualBox** (fastest):

```powershell
# Create test VM (Windows PowerShell)
$VBoxPath = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"

# Check if VirtualBox installed
if (Test-Path $VBoxPath) {
    & $VBoxPath createvm --name RayOS-Test --ostype Linux26_64 --register
    & $VBoxPath modifyvm RayOS-Test --memory 2048 --firmware efi --cpus 2
    & $VBoxPath storagectl RayOS-Test --name IDE --add ide
    & $VBoxPath storageattach RayOS-Test --storagectl IDE --port 0 --device 0 --type dvddrive --medium "$(Get-Location)\iso-output\rayos.iso"
    & $VBoxPath startvm RayOS-Test --type gui
} else {
    Write-Host "VirtualBox not found. Install from https://www.virtualbox.org/"
}
```

**Expected outcome:**

- VirtualBox opens VM
- You see "RayOS UEFI Bootloader v0.1" message
- → ISO and bootloader are correct, problem is BIOS settings

### Step 2: Test on Real Hardware

Once VM works:

1. Follow "Issue 3" above to regenerate USB
2. Physically try on target computer
3. If still fails, check BIOS settings (Issues 1 & 2)

### Step 3: Verify Boot Settings Combination

Create a checklist for your specific system:

```
☐ Boot Mode: UEFI (not Legacy/CSM)
☐ Secure Boot: DISABLED
☐ Fast Boot: DISABLED
☐ Boot order: USB above Hard Drive
☐ USB: UEFI version selected (not Legacy)
☐ ISO: Written with Rufus (GPT + UEFI)
☐ USB: FAT32 file system
```

## Advanced Debugging

### Check ISO Structure

```powershell
cd "c:\Users\caden\Documents\Programming Scripts\Personal\Rust\ray-os"

# List ISO contents
wsl 'iso-content=$(mktemp -d) && mount -o loop iso-output/rayos.iso $iso-content && find $iso-content -type f && umount $iso-content'

# Or verify EFI binary exists in ISO
wsl tar -tzf iso-output/rayos.iso | grep -i boot
```

### Verify Bootloader Binary Format

```powershell
# Check bootloader is PE/COFF executable
wsl file bootloader/target/x86_64-unknown-uefi/release/uefi_boot.efi

# Output should contain: "PE32+ executable (DLL) (EFI application) x86-64"
```

### Check Boot Variables (Linux/WSL)

```bash
# If booted from UEFI on Linux:
efibootmgr
efibootmgr -v
# Shows boot order and UEFI variables
```

## Common Error Messages & Meanings

| Error                                    | Cause                     | Fix                              |
| ---------------------------------------- | ------------------------- | -------------------------------- |
| "No bootable option or device was found" | BIOS/Secure Boot issue    | Disable Secure Boot, enable UEFI |
| "Invalid partition table"                | USB write failed          | Regenerate with Rufus            |
| "Boot device not found"                  | Wrong boot device         | Select UEFI USB entry            |
| "File not found"                         | EFI binary missing        | Rebuild ISO                      |
| "Security policy violation"              | Secure Boot prevents boot | Disable Secure Boot              |
| "Exit Boot Services" repeatedly          | Bootloader not exiting    | Code issue, rebuild              |

## Quick Troubleshooting Checklist

- [ ] BIOS: Boot Mode = UEFI ✓
- [ ] BIOS: Secure Boot = DISABLED ✓
- [ ] BIOS: Fast Boot = DISABLED ✓
- [ ] USB: Regenerated with Rufus (GPT + UEFI) ✓
- [ ] USB: FAT32 filesystem ✓
- [ ] USB: Appears in UEFI boot menu ✓
- [ ] ISO: Tested in VirtualBox ✓
- [ ] BIOS: Boot order USB first ✓

## Still Not Working?

Try these diagnostic questions:

1. **Can you boot other UEFI USB drives?** (Windows installer, Linux distro)

   - Yes → RayOS bootloader issue
   - No → Your system has BIOS/hardware issue

2. **Does it work in VirtualBox?**

   - Yes → BIOS settings issue on target hardware
   - No → ISO/bootloader is corrupted

3. **Do you see firmware boot messages?**

   - Yes → System is trying to boot, failing later
   - No → BIOS won't recognize boot device

4. **What's your system manufacturer?**
   - Dell/HP/Lenovo → Look up their specific UEFI settings docs
   - Custom built → Check motherboard manual

## Contact/Research

- RayOS UEFI Crate: https://docs.rs/uefi/
- UEFI Specification: https://uefi.org/
- Rust OS Dev: https://os.phil-opp.com/
- Bootloader Resources: https://github.com/rust-osdev/bootloader

---

**Last Updated:** RayOS Phase 1 UEFI Bootloader
**Bootloader Version:** 0.1
**Expected Output:** "RayOS UEFI Bootloader v0.1"
