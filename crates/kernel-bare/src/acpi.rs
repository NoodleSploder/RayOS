use crate::serial_write_str;

// For now, MCFG is a dummy struct.
#[derive(Debug)]
pub struct Mcfg;

pub fn find_mcfg(_rsdp_addr: u64) -> Option<&'static Mcfg> {
    serial_write_str("acpi::find_mcfg called\n");
    // Dummy implementation
    None
}
