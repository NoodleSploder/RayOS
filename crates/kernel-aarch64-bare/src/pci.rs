use crate::{uart_write_hex_u64, uart_write_str, uart_write_u32_dec};
use crate::acpi::Mcfg;

const PCI_VENDOR_NONE: u16 = 0xFFFF;
const PCI_VENDOR_VIRTIO: u16 = 0x1AF4;

// Virtio device IDs are implementation-defined across QEMU versions, but
// virtio-gpu-pci is typically a display controller class (0x03).
const PCI_CLASS_DISPLAY: u8 = 0x03;

pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
}

// Read PCI config space (MMCONFIG)
fn pci_read8(base: u64, bus: u8, device: u8, function: u8, offset: u16) -> u8 {
    let addr = base
        + ((bus as u64) << 20)
        + ((device as u64) << 15)
        + ((function as u64) << 12)
        + (offset as u64);
    unsafe { core::ptr::read_volatile(addr as *const u8) }
}

fn pci_read16(base: u64, bus: u8, device: u8, function: u8, offset: u16) -> u16 {
    let addr = base
        + ((bus as u64) << 20)
        + ((device as u64) << 15)
        + ((function as u64) << 12)
        + (offset as u64);
    unsafe { core::ptr::read_volatile(addr as *const u16) }
}

fn pci_read32(base: u64, bus: u8, device: u8, function: u8, offset: u16) -> u32 {
    let addr = base
        + ((bus as u64) << 20)
        + ((device as u64) << 15)
        + ((function as u64) << 12)
        + (offset as u64);
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

fn pci_bar_base(base: u64, bus: u8, device: u8, function: u8, bar: u8) -> Option<u64> {
    if bar >= 6 {
        return None;
    }
    let off = 0x10u16 + (bar as u16) * 4;
    let low = pci_read32(base, bus, device, function, off);
    if (low & 0x1) != 0 {
        // I/O space BAR not expected for virtio-modern.
        return None;
    }
    let ty = (low >> 1) & 0x3;
    let low_addr = (low as u64) & !0xFu64;
    if ty == 0x2 {
        // 64-bit
        let high = pci_read32(base, bus, device, function, off + 4) as u64;
        Some((high << 32) | low_addr)
    } else {
        Some(low_addr)
    }
}

fn mmio_read8(addr: u64) -> u8 {
    unsafe { core::ptr::read_volatile(addr as *const u8) }
}

fn mmio_read16(addr: u64) -> u16 {
    unsafe { core::ptr::read_volatile(addr as *const u16) }
}

fn mmio_read32(addr: u64) -> u32 {
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

fn mmio_write8(addr: u64, val: u8) {
    unsafe { core::ptr::write_volatile(addr as *mut u8, val) }
}

fn mmio_write32(addr: u64, val: u32) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, val) }
}

fn probe_virtio_modern_and_handshake(base: u64, bus: u8, device: u8, function: u8) {
    // PCI capability list pointer.
    let mut cap_ptr = pci_read8(base, bus, device, function, 0x34) & 0xFC;
    if cap_ptr == 0 {
        return;
    }

    // Virtio vendor-specific capability (cap_id = 0x09).
    // struct virtio_pci_cap:
    //  u8 cap_vndr; u8 cap_next; u8 cap_len; u8 cfg_type;
    //  u8 bar; u8 padding[3]; u32 offset; u32 length;
    let mut common_cfg_addr: u64 = 0;
    let mut device_cfg_addr: u64 = 0;

    for _ in 0..64 {
        let cap_id = pci_read8(base, bus, device, function, cap_ptr as u16);
        let next = pci_read8(base, bus, device, function, (cap_ptr as u16) + 1) & 0xFC;
        if cap_id == 0x09 {
            let cfg_type = pci_read8(base, bus, device, function, (cap_ptr as u16) + 3);
            let bar = pci_read8(base, bus, device, function, (cap_ptr as u16) + 4);
            let off = pci_read32(base, bus, device, function, (cap_ptr as u16) + 8) as u64;
            // length at +12 (unused for now)
            if let Some(bar_base) = pci_bar_base(base, bus, device, function, bar) {
                let addr = bar_base.wrapping_add(off);
                match cfg_type {
                    1 => common_cfg_addr = addr,
                    4 => device_cfg_addr = addr,
                    _ => {}
                }
            }
        }

        if next == 0 || next == cap_ptr {
            break;
        }
        cap_ptr = next;
    }

    if common_cfg_addr == 0 {
        return;
    }

    uart_write_str("RAYOS_AARCH64_VIRTIO_GPU:FOUND\n");
    uart_write_str("virtio: common_cfg=0x");
    uart_write_hex_u64(common_cfg_addr);
    uart_write_str(" device_cfg=0x");
    uart_write_hex_u64(device_cfg_addr);
    uart_write_str("\n");

    // Minimal virtio-modern handshake through common config.
    // Offsets within virtio_pci_common_cfg
    let device_feature_select = common_cfg_addr + 0x00;
    let device_feature = common_cfg_addr + 0x04;
    let driver_feature_select = common_cfg_addr + 0x08;
    let driver_feature = common_cfg_addr + 0x0C;
    let num_queues = common_cfg_addr + 0x12;
    let device_status = common_cfg_addr + 0x14;

    // Read device features (two 32-bit words).
    mmio_write32(device_feature_select, 0);
    let feat0 = mmio_read32(device_feature);
    mmio_write32(device_feature_select, 1);
    let feat1 = mmio_read32(device_feature);

    uart_write_str("virtio: num_queues=");
    uart_write_u32_dec(mmio_read16(num_queues) as u32);
    uart_write_str(" features_hi=0x");
    uart_write_hex_u64(feat1 as u64);
    uart_write_str(" features_lo=0x");
    uart_write_hex_u64(feat0 as u64);
    uart_write_str("\n");

    // Reset + set ACKNOWLEDGE|DRIVER.
    mmio_write8(device_status, 0);
    mmio_write8(device_status, 0x01 | 0x02);

    // Negotiate no features (just for bring-up); set FEATURES_OK.
    mmio_write32(driver_feature_select, 0);
    mmio_write32(driver_feature, 0);
    mmio_write32(driver_feature_select, 1);
    mmio_write32(driver_feature, 0);

    let st = mmio_read8(device_status);
    mmio_write8(device_status, st | 0x08);
    let st2 = mmio_read8(device_status);
    if (st2 & 0x08) != 0 {
        uart_write_str("RAYOS_AARCH64_VIRTIO_GPU:FEATURES_OK\n");
    } else {
        uart_write_str("RAYOS_AARCH64_VIRTIO_GPU:FEATURES_FAIL\n");
    }
}

pub fn enumerate_pci(mcfg: &Mcfg) {
    let base = mcfg.base_addr;
    uart_write_str("pci::enumerate_pci: walking buses ");
    uart_write_u32_dec(mcfg.bus_start as u32);
    uart_write_str("..");
    uart_write_u32_dec(mcfg.bus_end as u32);
    uart_write_str("\n");

    for bus in mcfg.bus_start..=mcfg.bus_end {
        for device in 0u8..32 {
            for function in 0u8..8 {
                let vendor_id = pci_read16(base, bus, device, function, 0x00);
                if vendor_id == PCI_VENDOR_NONE {
                    continue;
                }
                let device_id = pci_read16(base, bus, device, function, 0x02);
                let class_rev = pci_read32(base, bus, device, function, 0x08);
                let class_code = ((class_rev >> 24) & 0xFF) as u8;

                uart_write_str("PCI dev: bus ");
                uart_write_u32_dec(bus as u32);
                uart_write_str(" dev ");
                uart_write_u32_dec(device as u32);
                uart_write_str(" fn ");
                uart_write_u32_dec(function as u32);
                uart_write_str(" vendor 0x");
                uart_write_hex_u64(vendor_id as u64);
                uart_write_str(" device 0x");
                uart_write_hex_u64(device_id as u64);
                uart_write_str(" class 0x");
                uart_write_hex_u64(class_code as u64);
                uart_write_str("\n");

                if vendor_id == PCI_VENDOR_VIRTIO && class_code == PCI_CLASS_DISPLAY {
                    probe_virtio_modern_and_handshake(base, bus, device, function);
                }
            }
        }
    }
}
