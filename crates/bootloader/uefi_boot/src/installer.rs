/// Installer invocation logic for the UEFI bootloader.
///
/// If the installer is requested (via a flag in the registry), this module
/// handles loading and executing the installer binary instead of the kernel.

/// Check if the installer should be invoked.
///
/// Reads /EFI/RAYOS/registry.json and looks for an "installer_mode" field.
/// If present and true, returns true; otherwise returns false.
///
/// NOTE: Simplified stub - always returns false for now.
/// Full implementation requires alloc support for String/Vec.
pub fn should_invoke_installer(
    _root: &mut uefi::proto::media::file::Directory,
) -> bool {
    // TODO: Implement registry.json parsing with alloc support
    // For now, bootloader only launches kernel
    false
}
