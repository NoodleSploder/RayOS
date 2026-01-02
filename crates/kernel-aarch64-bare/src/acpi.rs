
use crate::uart_write_str;

#[repr(C, packed)]
pub struct Mcfg {
    pub base_addr: u64,
    pub segment: u16,
    pub bus_start: u8,
    pub bus_end: u8,
}

// ACPI SDT header
#[repr(C, packed)]
struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

// RSDP v2 struct (partial)
#[repr(C, packed)]
struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_addr: u32,
    length: u32,
    xsdt_addr: u64,
    // ...
}

pub fn find_mcfg(rsdp_addr: u64) -> Option<&'static Mcfg> {
    uart_write_str("acpi::find_mcfg called\n");
    let rsdp = unsafe { &*(rsdp_addr as *const Rsdp) };
    if &rsdp.signature != b"RSD PTR " {
        uart_write_str("acpi: bad RSDP signature\n");
        return None;
    }
    let xsdt_addr = rsdp.xsdt_addr;
    if xsdt_addr == 0 {
        uart_write_str("acpi: no XSDT\n");
        return None;
    }
    let xsdt = unsafe { &*(xsdt_addr as *const SdtHeader) };
    let entry_count = (xsdt.length as usize - core::mem::size_of::<SdtHeader>()) / 8;
    let entries = unsafe { (xsdt_addr as *const u8).add(core::mem::size_of::<SdtHeader>()) as *const u64 };
    for i in 0..entry_count {
        let table_addr = unsafe { *entries.add(i) };
        let hdr = unsafe { &*(table_addr as *const SdtHeader) };
        if &hdr.signature == b"MCFG" {
            uart_write_str("acpi: found MCFG\n");
            // MCFG table: header + reserved (8) + entries
            let mcfg_base = (table_addr + core::mem::size_of::<SdtHeader>() as u64 + 8) as *const Mcfg;
            let mcfg = unsafe { &*mcfg_base };
            return Some(mcfg);
        }
    }
    uart_write_str("acpi: MCFG not found\n");
    None
}
