use crate::{serial_write_str, acpi::Mcfg};

#[derive(Debug)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
}

pub fn enumerate_pci(_mcfg: &Mcfg) {
    serial_write_str("pci::enumerate_pci called\n");
    // Dummy implementation
}

