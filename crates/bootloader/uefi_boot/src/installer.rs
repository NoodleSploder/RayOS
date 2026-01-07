/// Installer invocation logic for the UEFI bootloader.
///
/// If the installer is requested (via a flag in the registry), this module
/// handles loading and executing the installer binary instead of the kernel.

use uefi::proto::media::file::{File, FileAttribute, FileMode, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{BootServices, MemoryType};

/// Check if the installer should be invoked.
///
/// Reads /EFI/RAYOS/registry.json and looks for an "installer_mode" field.
/// If present and true, returns true; otherwise returns false.
pub fn should_invoke_installer(
    root: &mut uefi::proto::media::file::Directory,
) -> bool {
    // Attempt to read registry.json
    match read_registry_json(root) {
        Ok(Some(registry)) => {
            // Simple check: look for "installer_mode":true in the JSON
            registry.contains("\"installer_mode\":true")
                || registry.contains("\"installer_mode\": true")
        }
        _ => false,
    }
}

/// Read the entire registry.json file from the ESP.
fn read_registry_json(
    root: &mut uefi::proto::media::file::Directory,
) -> Result<Option<String>, &'static str> {
    let registry_path = "EFI\\RAYOS\\registry.json";

    let file_handle = root
        .open(registry_path, FileMode::Read, FileAttribute::empty())
        .ok()
        .and_then(|f| f)
        .ok_or("registry.json not found")?;

    let file = file_handle
        .into_type()
        .map_err(|_| "Failed to determine file type")?
        .ok_or("No file")?;

    let mut file: RegularFile = match file {
        uefi::proto::media::file::FileType::Regular(f) => f,
        _ => return Err("registry.json is not a regular file"),
    };

    let _ = file.set_position(uefi::proto::media::file::RegularFile::END_OF_FILE);
    let size = file
        .get_position()
        .map_err(|_| "Failed to read registry size")?
        .ok_or("Failed to get registry size")? as usize;
    let _ = file.set_position(0);

    if size == 0 || size > 1024 * 1024 {
        // Registry too large or empty
        return Ok(None);
    }

    let mut buffer = vec![0u8; size];
    let bytes_read = file
        .read(&mut buffer)
        .map_err(|_| "Failed to read registry")?
        .ok_or("Failed to get bytes read")?;

    if bytes_read != size {
        return Ok(None);
    }

    let content = String::from_utf8(buffer)
        .map_err(|_| "Registry is not valid UTF-8")?;
    Ok(Some(content))
}

/// Load the installer binary from /EFI/RAYOS/installer.bin
/// Returns (entry_point, buffer_ptr, size_bytes) similar to kernel loading.
pub fn load_installer_binary(
    bt: &BootServices,
    root: &mut uefi::proto::media::file::Directory,
) -> Result<(*const u8, usize), &'static str> {
    let installer_path = "EFI\\RAYOS\\installer.bin";

    let file_handle = root
        .open(installer_path, FileMode::Read, FileAttribute::empty())
        .map_err(|_| "Failed to open installer.bin")?
        .ok_or("installer.bin not found")?;

    let file = file_handle
        .into_type()
        .map_err(|_| "Failed to determine installer file type")?
        .ok_or("No file")?;

    let mut file: RegularFile = match file {
        uefi::proto::media::file::FileType::Regular(f) => f,
        _ => return Err("installer.bin is not a regular file"),
    };

    // Determine file size
    let _ = file.set_position(RegularFile::END_OF_FILE);
    let file_size = file
        .get_position()
        .map_err(|_| "Failed to read installer size")?
        .ok_or("Failed to get installer size")? as usize;
    let _ = file.set_position(0);

    if file_size == 0 || file_size > 256 * 1024 * 1024 {
        return Err("Invalid installer size");
    }

    // Allocate memory for the installer binary
    let pages = (file_size + 4095) / 4096;
    let installer_addr = bt
        .allocate_pages(
            uefi::table::boot::AllocateType::MaxAddress(0xFFFF_F000),
            MemoryType::LOADER_DATA,
            pages,
        )
        .map_err(|_| "Failed to allocate memory for installer")?
        .ok_or("Allocation returned null")?;

    let installer_buffer = unsafe { core::slice::from_raw_parts_mut(installer_addr as *mut u8, file_size) };
    let bytes_read = file
        .read(installer_buffer)
        .map_err(|_| "Failed to read installer")?
        .ok_or("Failed to get bytes read")?;

    if bytes_read != file_size {
        return Err("Incomplete installer read");
    }

    Ok((installer_addr as *const u8, file_size))
}

/// Installer entry point signature.
/// The installer binary is expected to be a statically-linked ELF executable.
/// For now, we do not invoke it directly; this is a placeholder for future
/// integration where the bootloader could chainload the installer.
#[cfg(target_arch = "x86_64")]
pub type InstallerEntryPoint = extern "sysv64" fn();

#[cfg(not(target_arch = "x86_64"))]
pub type InstallerEntryPoint = extern "C" fn();
