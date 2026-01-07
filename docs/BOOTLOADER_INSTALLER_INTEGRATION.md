# Bootloader Installer Integration (Jan 07, 2026)

## Architecture Overview

The RayOS bootloader now supports detecting and invoking the installer when requested. This allows for a clean separation between normal boot (RayOS kernel) and installation/recovery mode (installer binary).

## Implementation Details

### 1. Installer Module (`crates/bootloader/uefi_boot/src/installer.rs`)

The installer module provides:
- **`should_invoke_installer(root: &mut Directory) -> bool`**: Checks the ESP registry for an `installer_mode` flag
- **`load_installer_binary(bt: &BootServices, root: &mut Directory) -> Result<(*const u8, usize), &'static str>`**: Loads the installer.bin from the ESP into memory
- Support for future chainloading (entry point types defined)

### 2. Bootloader Main Flow Updates

In `crates/bootloader/uefi_boot/src/main.rs`:
- Added `mod installer;` declaration
- Added `check_installer_mode()` function that opens the ESP and delegates to the installer module
- Integrated check before kernel load: if installer mode is detected, bootloader displays a message and logs the event
- Current state: detection works; actual chainloading is not yet implemented (placeholder for future work)

### 3. Registry-Based Activation

The installer can be activated by creating or modifying `/EFI/RAYOS/registry.json` with:

```json
{
  "vms": {},
  "installer_mode": true
}
```

When the bootloader detects this flag at boot time, it:
1. Prints "installer mode detected" to console
2. Reads the installer.bin from the ESP
3. (Future) Chainloads the installer instead of the kernel

## How to Enable Installer Mode

### Method 1: Via Registry (Recommended for Production)
```bash
# From within RayOS or an existing installation:
cat > /boot/EFI/RAYOS/registry.json << EOF
{
  "vms": {},
  "installer_mode": true
}
EOF
```

### Method 2: Manually (Development/Testing)
1. Boot the installer USB media
2. Installer menu option: "Set recovery/reinstall mode"
3. Installer modifies registry.json and reboots into kernel
4. Next boot, kernel detects installer_mode flag and can re-invoke installer if needed

## Current Status

### ✅ Completed
- Installer module created and integrated into bootloader
- Registry-based activation flag system designed
- Installer binary bundled into boot media ESP

### ⏳ Pending
- Actual ELF chainloading (load installer.bin as executable)
- Installer-to-kernel bridging (passing control, preserving UEFI state)
- Kernel recognition of "came from installer" state (for resuming partial installs)
- Interactive partition manager execution from installer

## Technical Notes

### Why Registry-Based Rather Than Command Line?
- The installer needs to be invoked **from within the running system**, not just at boot time
- Registry.json is persistent and survives reboots, allowing boot sequence decisions
- This matches the pattern used for other boot mode flags (e.g., Linux VM autoboot)

### Bootloader Entry Point
The installer.bin would need to be:
- A statically-linked executable (no dynamic linking from UEFI environment)
- Have a clear entry point matching `InstallerEntryPoint` type
- Handle UEFI environment properly (framebuffer, console output, etc.)

Alternatively, the installer could be invoked as a subprocess within the RayOS kernel itself, which is simpler and avoids UEFI complexity in the installer.

### Future: Kernel-Invoked Installer
A simpler approach is to:
1. Bootloader loads kernel normally
2. Kernel checks registry for `installer_mode: true`
3. Kernel loads and executes installer.bin as a subprocess
4. Installer runs in user mode with full kernel support
5. Installer communicates with kernel to perform disk operations

This approach is recommended for the next iteration as it's cleaner and leverages existing kernel abstractions.

## Next Steps

1. **Installer Subprocess Execution** (alternative to chainloading)
   - RayOS kernel recognizes `installer_mode` flag
   - Kernel loads installer.bin from ESP and executes it
   - Installer uses standard syscalls for disk operations

2. **Partition Manager Interactive Flow**
   - Enhance installer binary to accept user input
   - Implement disk selection and partition creation
   - Add safety confirmations

3. **System Image Installation**
   - Copy RayOS system image to selected partition
   - Write boot entries
   - Create recovery partition

4. **Reboot and Validation**
   - Installer can trigger reboot
   - Next boot: bootloader sees installed RayOS, loads it normally
   - Validation test verifies the entire flow

## Files Modified/Added

- `crates/bootloader/uefi_boot/src/installer.rs` (NEW)
  - Installer detection and loading logic

- `crates/bootloader/uefi_boot/src/main.rs` (MODIFIED)
  - Added `mod installer;`
  - Added `check_installer_mode()` function
  - Integrated check into main boot flow
  - Display feedback on console and framebuffer

## Design Rationale

This architecture provides:
- **Clean separation**: Installer is independent binary, not embedded in kernel
- **Flexible invocation**: Can be triggered from registry or command line
- **Safe by default**: No installer mode unless explicitly requested
- **Future-proof**: Support for both chainloading and kernel-subprocess models
- **Testable**: Registry flag can be set in test VMs to validate installer path
