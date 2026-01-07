/// Installer invocation logic for the UEFI bootloader.
///
/// If the installer is requested (via a flag in the registry), this module
/// handles loading and executing the installer binary instead of the kernel.

use uefi::proto::media::file::{File, FileAttribute, FileMode, RegularFile};

/// Check if the installer should be invoked.
///
/// Reads /EFI/RAYOS/registry.json and looks for an "installer_mode" field.
/// If present and true, returns true; otherwise returns false.
///
/// Uses stack-allocated buffer to avoid alloc dependency.
/// Maximum registry size: 64 KB
pub fn should_invoke_installer(
    root: &mut uefi::proto::media::file::Directory,
) -> bool {
    // Try to read registry.json and check for installer_mode flag
    match read_registry_json_simple(root) {
        Ok(contains_installer_mode) => contains_installer_mode,
        Err(_) => {
            // If we can't read registry, default to kernel boot
            false
        }
    }
}

/// Read registry.json and check for "installer_mode":true
/// 
/// Does simple byte scanning without allocating Vec or String.
/// Returns true if "installer_mode":true is found in the JSON.
fn read_registry_json_simple(
    root: &mut uefi::proto::media::file::Directory,
) -> Result<bool, &'static str> {
    const MAX_REGISTRY_SIZE: usize = 64 * 1024; // 64 KB max
    let mut buffer: [u8; MAX_REGISTRY_SIZE] = [0; MAX_REGISTRY_SIZE];
    
    let registry_path = "EFI\\RAYOS\\registry.json";
    
    // Try to open the registry file
    let file_handle = match root.open(registry_path, FileMode::Read, FileAttribute::empty()) {
        Ok(v) => v.unwrap(),
        Err(_) => return Err("Failed to open registry.json"),
    };
    
    let file_type = file_handle
        .into_type()
        .map_err(|_| "Failed to determine file type")?
        .unwrap();
    
    let mut file: RegularFile = match file_type {
        uefi::proto::media::file::FileType::Regular(f) => f,
        _ => return Err("registry.json is not a regular file"),
    };
    
    // Read file into buffer
    let bytes_read = file
        .read(&mut buffer)
        .map_err(|_| "Failed to read registry")?
        .unwrap();
    
    if bytes_read == 0 || bytes_read > MAX_REGISTRY_SIZE {
        return Err("Invalid registry size");
    }
    
    // Search for "installer_mode":true in the buffer
    // We're looking for patterns like:
    //   "installer_mode": true
    //   "installer_mode":true
    //   "installer_mode" : true
    
    let json = &buffer[0..bytes_read];
    
    // Simple pattern matching for "installer_mode" followed by : and true
    for i in 0..json.len().saturating_sub(30) {
        // Look for "installer_mode" substring
        if json[i..].starts_with(b"\"installer_mode\"") {
            // Search for : and then true after this position
            let rest = &json[i + 16..]; // After "installer_mode"
            
            // Look for ':' followed by 'true'
            for j in 0..rest.len().saturating_sub(5) {
                if rest[j] == b':' {
                    // Skip whitespace and check for 'true'
                    let mut k = j + 1;
                    while k < rest.len() && (rest[k] == b' ' || rest[k] == b'\t' || rest[k] == b'\n' || rest[k] == b'\r') {
                        k += 1;
                    }
                    
                    if k + 4 <= rest.len() && rest[k..k+4] == *b"true" {
                        return Ok(true);
                    }
                    break;
                }
            }
        }
    }
    
    Ok(false)
}
