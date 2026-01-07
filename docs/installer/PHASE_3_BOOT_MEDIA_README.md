# RayOS Phase 3: Boot Testing Guide

**Quick Start for Boot Testing**

---

## 🚀 Available Boot Media

Two pre-built boot ISO images are ready for testing:

### 1. Kernel Mode (`rayos-kernel-test.iso` - 4.0 MB)
- **Location:** `build/rayos-kernel-test.iso`
- **Boot Path:** Direct kernel execution
- **Registry:** `installer_mode=false`
- **Use Case:** Validate kernel boot on UEFI systems

### 2. Installer Mode (`rayos-installer-test.iso` - 9.3 MB)
- **Location:** `build/rayos-installer-test.iso`
- **Boot Path:** Installer chainloading
- **Registry:** `installer_mode=true`
- **Use Case:** Validate installer execution via bootloader

---

## 🖥️ Testing Methods

### Method 1: QEMU Testing (Linux Host)

**Automated script available:**
```bash
bash scripts/test-qemu-kernel-boot.sh
```

**Manual QEMU testing:**
```bash
# Kernel mode boot
qemu-system-x86_64 \
  -bios /usr/share/qemu/OVMF.fd \
  -cdrom build/rayos-kernel-test.iso \
  -serial file:qemu-boot.log \
  -m 512 -enable-kvm

# Installer mode boot
qemu-system-x86_64 \
  -bios /usr/share/qemu/OVMF.fd \
  -cdrom build/rayos-installer-test.iso \
  -serial file:qemu-boot.log \
  -m 512 -enable-kvm
```

**Capture output:**
- Serial messages written to `qemu-boot.log`
- Monitor for boot progress messages
- Expected: Bootloader → Registry → Mode selection → Execution

---

### Method 2: Hardware Testing (Real UEFI Systems)

**Preparation:**
1. Write ISO to USB:
   ```bash
   sudo dd if=build/rayos-kernel-test.iso of=/dev/sdX bs=4M status=progress
   sudo sync
   ```
   *Replace `/dev/sdX` with your USB device*

2. Connect serial console (if available):
   - 115200 baud, 8N1
   - Records boot messages in real-time

3. Boot from USB (Set UEFI boot order)

**Expected Boot Sequence:**
```
[Bootloader] UEFI entry point reached
[Bootloader] Loading registry.json
[Bootloader] Boot mode: KERNEL (installer_mode=false)
[Bootloader] Loading kernel at 0x...
[Kernel] Kernel entry point
[Kernel] Memory initialization...
```

---

## 📝 What Each Boot Path Does

### Kernel Mode (`installer_mode=false`)
1. Bootloader loads registry.json
2. Detects: installer_mode=false
3. Skips installer binary
4. Directly loads kernel.bin
5. Executes kernel at entry point
6. Kernel initialization begins

### Installer Mode (`installer_mode=true`)
1. Bootloader loads registry.json
2. Detects: installer_mode=true
3. Loads installer.bin to 0x0000_4000_0000
4. Executes installer
5. Installer discovers kernel.bin
6. Installer loads kernel into target addresses
7. Installer transfers control to kernel

---

## 🔍 Verification Checklist

### Boot Media Content Validation
```bash
# Check kernel mode ISO structure
cd build
ls -lh kernel-mode/EFI/RAYOS/
ls -lh kernel-mode/EFI/Boot/bootx64.efi

# Check installer mode ISO structure
ls -lh installer-mode/EFI/RAYOS/
ls -lh installer-mode/EFI/Boot/bootx64.efi

# Verify registry files
echo "Kernel mode registry:"
cat kernel-mode/EFI/RAYOS/registry.json

echo "Installer mode registry:"
cat installer-mode/EFI/RAYOS/registry.json
```

### QEMU Boot Test
```bash
# Run automated test
bash scripts/test-qemu-kernel-boot.sh

# Check output
cat qemu-boot.log
```

### Hardware Boot Test
1. Connect serial monitor to system
2. Boot from USB
3. Observe initialization messages
4. Record any error codes or hangs
5. Document boot sequence

---

## 📊 Expected Output

### Kernel Mode Boot (QEMU Serial Output)
```
[00.000] UEFI Bootloader v0.1
[00.050] Loading registry from /EFI/RAYOS/registry.json
[00.100] Boot mode: KERNEL (installer_mode=false)
[00.150] Loading kernel binary from /EFI/RAYOS/kernel.bin
[00.200] Kernel size: 3.6 MB
[00.250] Allocation address: 0x...
[00.300] Jumping to kernel entry point
[00.350] [Kernel] Initialization starting...
```

### Installer Mode Boot (QEMU Serial Output)
```
[00.000] UEFI Bootloader v0.1
[00.050] Loading registry from /EFI/RAYOS/registry.json
[00.100] Boot mode: INSTALLER (installer_mode=true)
[00.150] Loading installer from /EFI/RAYOS/installer.bin
[00.200] Installer size: 5.3 MB
[00.250] Allocation address: 0x4000_0000
[00.300] Jumping to installer entry point
[00.350] [Installer] Starting installation...
[00.400] [Installer] Locating kernel...
[00.450] [Installer] Loading kernel...
[00.500] [Installer] Booting kernel...
```

---

## 🛠️ Troubleshooting

### Boot Media Not Recognized
- Verify ISO file size: kernel=4.0MB, installer=9.3MB
- Check USB write with: `sudo fdisk -l /dev/sdX`
- Try writing with: `dd if=ISO of=/dev/sdX bs=4M`

### No Serial Output
- Check baud rate: 115200
- Verify serial device: `/dev/ttyS0` or `/dev/ttyUSB0`
- QEMU flag: `-serial file:output.log`

### Boot Hangs After Bootloader
- Check registry.json syntax
- Verify binary files exist on ISO
- Check memory allocation addresses
- Review bootloader source for error handling

### QEMU Firmware Not Found
- Install: `sudo apt install qemu-system-x86 ovmf`
- Check: `ls /usr/share/qemu/OVMF.fd`
- Alt paths: `/usr/share/OVMF/OVMF_CODE.fd`

---

## 📚 Additional Resources

- **PHASE_3_BOOT_TESTING_GUIDE.md** - Complete testing procedures
- **BOOTLOADER_CHAINLOADING.md** - Technical implementation details
- **scripts/test-qemu-kernel-boot.sh** - Automated QEMU testing
- **scripts/create-custom-boot-media.sh** - Generate custom boot media variants

---

## 🎯 Success Criteria

Each boot path should:
1. ✓ Load and execute bootloader (57 KB EFI binary)
2. ✓ Read registry.json successfully
3. ✓ Detect correct boot mode (installer/kernel)
4. ✓ Load appropriate binary (kernel or installer)
5. ✓ Execute target binary at correct address
6. ✓ Produce initialization messages on serial console

---

## 🔗 Boot Flow Diagram

```
[UEFI Firmware]
    ↓
[Load bootx64.efi from EFI/Boot/]
    ↓
[Bootloader: read registry.json]
    ↓
    ├─→ installer_mode=true  →  Load /EFI/RAYOS/installer.bin
    │                              ↓
    │                          [Installer Mode Boot]
    │
    └─→ installer_mode=false →  Load /EFI/RAYOS/kernel.bin
                                  ↓
                              [Kernel Mode Boot]
```

---

**Status:** Phase 3 Boot Media Ready for Testing

Use these ISO images to validate that the bootloader chainloading system works correctly on your UEFI systems.
