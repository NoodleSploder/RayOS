// ===== RayOS Device Pass-through & IOMMU Integration (Phase 9B Task 4) =====
// PCI device pass-through, VFIO, IOMMU management, interrupt remapping

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ===== Constants =====

const MAX_IOMMU_DOMAINS: usize = 64;
const MAX_DEVICES_PER_DOMAIN: usize = 16;
const MAX_PASSTHROUGH_DEVICES: usize = 32;
const MAX_INTERRUPTS: usize = 256;
const MAX_DMA_REGIONS: usize = 128;

// ===== PCI Address =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PciAddress {
    /// PCI segment (domain)
    pub segment: u16,
    /// Bus number
    pub bus: u8,
    /// Device number (5 bits)
    pub device: u8,
    /// Function number (3 bits)
    pub function: u8,
}

impl PciAddress {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        PciAddress {
            segment: 0,
            bus,
            device: device & 0x1F,
            function: function & 0x07,
        }
    }

    pub fn with_segment(segment: u16, bus: u8, device: u8, function: u8) -> Self {
        PciAddress {
            segment,
            bus,
            device: device & 0x1F,
            function: function & 0x07,
        }
    }

    /// BDF (Bus:Device.Function) as u16
    pub fn bdf(&self) -> u16 {
        ((self.bus as u16) << 8) | ((self.device as u16) << 3) | (self.function as u16)
    }

    /// Full address including segment
    pub fn full_address(&self) -> u32 {
        ((self.segment as u32) << 16) | (self.bdf() as u32)
    }
}

// ===== PCI Device Info =====

#[derive(Debug, Copy, Clone)]
pub struct PciDeviceInfo {
    /// PCI address
    pub address: PciAddress,
    /// Vendor ID
    pub vendor_id: u16,
    /// Device ID
    pub device_id: u16,
    /// Class code
    pub class_code: u32,
    /// Subvendor ID
    pub subsys_vendor_id: u16,
    /// Subdevice ID
    pub subsys_device_id: u16,
    /// Revision ID
    pub revision_id: u8,
    /// Number of BARs
    pub num_bars: u8,
    /// BAR addresses
    pub bars: [u64; 6],
    /// BAR sizes
    pub bar_sizes: [u64; 6],
    /// Is BAR 64-bit?
    pub bar_64bit: [bool; 6],
    /// Is BAR prefetchable?
    pub bar_prefetch: [bool; 6],
    /// MSI capability offset
    pub msi_cap_offset: u8,
    /// MSI-X capability offset
    pub msix_cap_offset: u8,
}

impl PciDeviceInfo {
    pub fn new(address: PciAddress, vendor_id: u16, device_id: u16) -> Self {
        PciDeviceInfo {
            address,
            vendor_id,
            device_id,
            class_code: 0,
            subsys_vendor_id: 0,
            subsys_device_id: 0,
            revision_id: 0,
            num_bars: 0,
            bars: [0; 6],
            bar_sizes: [0; 6],
            bar_64bit: [false; 6],
            bar_prefetch: [false; 6],
            msi_cap_offset: 0,
            msix_cap_offset: 0,
        }
    }

    /// Check if device is a GPU
    pub fn is_gpu(&self) -> bool {
        (self.class_code >> 16) == 0x03  // Display controller
    }

    /// Check if device is a NIC
    pub fn is_network(&self) -> bool {
        (self.class_code >> 16) == 0x02  // Network controller
    }

    /// Check if device is storage
    pub fn is_storage(&self) -> bool {
        let class = self.class_code >> 16;
        class == 0x01  // Mass storage controller
    }

    /// Check if device supports MSI
    pub fn has_msi(&self) -> bool {
        self.msi_cap_offset != 0
    }

    /// Check if device supports MSI-X
    pub fn has_msix(&self) -> bool {
        self.msix_cap_offset != 0
    }
}

// ===== IOMMU Types =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IommuType {
    /// Intel VT-d
    IntelVtd,
    /// AMD-Vi (IOMMU)
    AmdVi,
    /// ARM SMMU
    ArmSmmu,
    /// No hardware IOMMU
    Software,
}

// ===== IOMMU Domain =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DomainType {
    /// Host domain (identity mapping)
    Host,
    /// Guest VM domain
    Guest,
    /// User space driver domain
    Userspace,
    /// Isolated device domain
    Isolated,
}

#[derive(Copy, Clone)]
pub struct DmaRegion {
    /// Guest/User virtual address
    pub iova: u64,
    /// Host physical address
    pub phys_addr: u64,
    /// Size in bytes
    pub size: u64,
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
}

impl DmaRegion {
    pub fn new(iova: u64, phys_addr: u64, size: u64, read: bool, write: bool) -> Self {
        DmaRegion {
            iova,
            phys_addr,
            size,
            read,
            write,
        }
    }
}

#[derive(Copy, Clone)]
pub struct IommuDomain {
    /// Domain ID
    pub domain_id: u32,
    /// Domain type
    pub domain_type: DomainType,
    /// Associated devices
    devices: [PciAddress; MAX_DEVICES_PER_DOMAIN],
    device_count: usize,
    /// DMA regions
    dma_regions: [DmaRegion; MAX_DMA_REGIONS],
    region_count: usize,
    /// Address space size (bits)
    pub address_bits: u8,
    /// Is domain active
    pub active: bool,
}

impl IommuDomain {
    pub fn new(domain_id: u32, domain_type: DomainType) -> Self {
        IommuDomain {
            domain_id,
            domain_type,
            devices: [PciAddress::new(0, 0, 0); MAX_DEVICES_PER_DOMAIN],
            device_count: 0,
            dma_regions: [DmaRegion::new(0, 0, 0, false, false); MAX_DMA_REGIONS],
            region_count: 0,
            address_bits: 48,
            active: false,
        }
    }

    pub fn attach_device(&mut self, device: PciAddress) -> Result<(), &'static str> {
        if self.device_count >= MAX_DEVICES_PER_DOMAIN {
            return Err("Domain full");
        }

        // Check if already attached
        for i in 0..self.device_count {
            if self.devices[i] == device {
                return Err("Device already attached");
            }
        }

        self.devices[self.device_count] = device;
        self.device_count += 1;
        Ok(())
    }

    pub fn detach_device(&mut self, device: PciAddress) -> Result<(), &'static str> {
        for i in 0..self.device_count {
            if self.devices[i] == device {
                // Shift remaining
                for j in i..self.device_count - 1 {
                    self.devices[j] = self.devices[j + 1];
                }
                self.device_count -= 1;
                return Ok(());
            }
        }
        Err("Device not found")
    }

    pub fn map_dma(&mut self, region: DmaRegion) -> Result<(), &'static str> {
        if self.region_count >= MAX_DMA_REGIONS {
            return Err("Maximum DMA regions reached");
        }

        // Check for overlap
        for i in 0..self.region_count {
            let existing = &self.dma_regions[i];
            let overlap = region.iova < existing.iova + existing.size &&
                         region.iova + region.size > existing.iova;
            if overlap {
                return Err("DMA region overlaps");
            }
        }

        self.dma_regions[self.region_count] = region;
        self.region_count += 1;
        Ok(())
    }

    pub fn unmap_dma(&mut self, iova: u64, size: u64) -> Result<(), &'static str> {
        for i in 0..self.region_count {
            let region = &self.dma_regions[i];
            if region.iova == iova && region.size == size {
                // Shift remaining
                for j in i..self.region_count - 1 {
                    self.dma_regions[j] = self.dma_regions[j + 1];
                }
                self.region_count -= 1;
                return Ok(());
            }
        }
        Err("DMA region not found")
    }

    pub fn translate_iova(&self, iova: u64) -> Option<u64> {
        for i in 0..self.region_count {
            let region = &self.dma_regions[i];
            if iova >= region.iova && iova < region.iova + region.size {
                return Some(region.phys_addr + (iova - region.iova));
            }
        }
        None
    }

    pub fn devices(&self) -> &[PciAddress] {
        &self.devices[..self.device_count]
    }

    pub fn regions(&self) -> &[DmaRegion] {
        &self.dma_regions[..self.region_count]
    }
}

// ===== IOMMU Controller =====

pub struct IommuController {
    /// IOMMU type
    pub iommu_type: IommuType,
    /// IOMMU domains
    domains: [IommuDomain; MAX_IOMMU_DOMAINS],
    domain_count: usize,
    /// Next domain ID
    next_domain_id: AtomicU32,
    /// Is IOMMU enabled
    pub enabled: bool,
    /// Supports interrupt remapping
    pub interrupt_remapping: bool,
    /// Supports x2APIC mode
    pub x2apic_mode: bool,
    /// Base address of IOMMU registers
    pub base_address: u64,
}

impl IommuController {
    pub fn new(iommu_type: IommuType) -> Self {
        IommuController {
            iommu_type,
            domains: [IommuDomain::new(0, DomainType::Host); MAX_IOMMU_DOMAINS],
            domain_count: 0,
            next_domain_id: AtomicU32::new(1),
            enabled: false,
            interrupt_remapping: false,
            x2apic_mode: false,
            base_address: 0,
        }
    }

    pub fn enable(&mut self) -> Result<(), &'static str> {
        if self.iommu_type == IommuType::Software {
            return Err("Software IOMMU cannot be enabled");
        }

        // Would write to IOMMU registers
        self.enabled = true;
        Ok(())
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn create_domain(&mut self, domain_type: DomainType) -> Result<u32, &'static str> {
        if self.domain_count >= MAX_IOMMU_DOMAINS {
            return Err("Maximum domains reached");
        }

        let domain_id = self.next_domain_id.fetch_add(1, Ordering::SeqCst);
        let idx = self.domain_count;

        self.domains[idx] = IommuDomain::new(domain_id, domain_type);
        self.domains[idx].active = true;
        self.domain_count += 1;

        Ok(domain_id)
    }

    pub fn destroy_domain(&mut self, domain_id: u32) -> Result<(), &'static str> {
        for i in 0..self.domain_count {
            if self.domains[i].domain_id == domain_id {
                if !self.domains[i].devices().is_empty() {
                    return Err("Domain has attached devices");
                }

                // Shift remaining
                for j in i..self.domain_count - 1 {
                    self.domains[j] = self.domains[j + 1];
                }
                self.domain_count -= 1;
                return Ok(());
            }
        }
        Err("Domain not found")
    }

    pub fn get_domain(&self, domain_id: u32) -> Option<&IommuDomain> {
        for i in 0..self.domain_count {
            if self.domains[i].domain_id == domain_id {
                return Some(&self.domains[i]);
            }
        }
        None
    }

    pub fn get_domain_mut(&mut self, domain_id: u32) -> Option<&mut IommuDomain> {
        for i in 0..self.domain_count {
            if self.domains[i].domain_id == domain_id {
                return Some(&mut self.domains[i]);
            }
        }
        None
    }

    /// Find domain containing a device
    pub fn find_device_domain(&self, device: PciAddress) -> Option<u32> {
        for i in 0..self.domain_count {
            for dev in self.domains[i].devices() {
                if *dev == device {
                    return Some(self.domains[i].domain_id);
                }
            }
        }
        None
    }
}

// ===== VFIO Device State =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VfioDeviceState {
    /// Device not bound
    Unbound,
    /// Device bound to VFIO driver
    Bound,
    /// Device opened by user
    Opened,
    /// Device configured for passthrough
    Configured,
    /// Device active (in use by VM)
    Active,
    /// Device error
    Error,
}

// ===== VFIO Device =====

#[derive(Copy, Clone)]
pub struct VfioDevice {
    /// PCI device info
    pub pci_info: PciDeviceInfo,
    /// VFIO state
    pub state: VfioDeviceState,
    /// IOMMU domain ID
    pub domain_id: Option<u32>,
    /// VM ID (if assigned to VM)
    pub vm_id: Option<u32>,
    /// Interrupt count
    pub irq_count: u32,
    /// Is device reset supported
    pub supports_reset: bool,
    /// Is FLR (Function Level Reset) supported
    pub supports_flr: bool,
    /// Reference count
    pub ref_count: u32,
}

impl VfioDevice {
    pub fn new(pci_info: PciDeviceInfo) -> Self {
        VfioDevice {
            pci_info,
            state: VfioDeviceState::Unbound,
            domain_id: None,
            vm_id: None,
            irq_count: 0,
            supports_reset: false,
            supports_flr: false,
            ref_count: 0,
        }
    }

    pub fn bind(&mut self) -> Result<(), &'static str> {
        if self.state != VfioDeviceState::Unbound {
            return Err("Device already bound");
        }

        // Would unbind from native driver and bind to vfio-pci
        self.state = VfioDeviceState::Bound;
        Ok(())
    }

    pub fn unbind(&mut self) -> Result<(), &'static str> {
        if self.state == VfioDeviceState::Active {
            return Err("Device is active");
        }
        if self.ref_count > 0 {
            return Err("Device has references");
        }

        self.state = VfioDeviceState::Unbound;
        self.domain_id = None;
        self.vm_id = None;
        Ok(())
    }

    pub fn open(&mut self) -> Result<(), &'static str> {
        if self.state != VfioDeviceState::Bound {
            return Err("Device not bound to VFIO");
        }

        self.ref_count += 1;
        self.state = VfioDeviceState::Opened;
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), &'static str> {
        if self.ref_count == 0 {
            return Err("Device not opened");
        }

        self.ref_count -= 1;
        if self.ref_count == 0 {
            self.state = VfioDeviceState::Bound;
        }
        Ok(())
    }

    pub fn configure_for_vm(&mut self, vm_id: u32, domain_id: u32) -> Result<(), &'static str> {
        if self.state != VfioDeviceState::Opened {
            return Err("Device not opened");
        }

        self.vm_id = Some(vm_id);
        self.domain_id = Some(domain_id);
        self.state = VfioDeviceState::Configured;
        Ok(())
    }

    pub fn activate(&mut self) -> Result<(), &'static str> {
        if self.state != VfioDeviceState::Configured {
            return Err("Device not configured");
        }

        self.state = VfioDeviceState::Active;
        Ok(())
    }

    pub fn deactivate(&mut self) -> Result<(), &'static str> {
        if self.state != VfioDeviceState::Active {
            return Err("Device not active");
        }

        self.state = VfioDeviceState::Configured;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), &'static str> {
        if !self.supports_reset && !self.supports_flr {
            return Err("Reset not supported");
        }

        // Would perform device reset
        Ok(())
    }
}

// ===== Interrupt Remapping =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InterruptType {
    /// Legacy INTx
    Intx,
    /// Message Signaled Interrupts
    Msi,
    /// MSI-X
    MsiX,
}

#[derive(Copy, Clone)]
pub struct InterruptMapping {
    /// Source device
    pub device: PciAddress,
    /// Interrupt type
    pub irq_type: InterruptType,
    /// Source interrupt number
    pub source_irq: u32,
    /// Target vector (in guest)
    pub target_vector: u32,
    /// Target APIC ID
    pub target_apic: u32,
    /// Is interrupt masked
    pub masked: bool,
}

impl InterruptMapping {
    pub fn new(device: PciAddress, irq_type: InterruptType, source: u32, target: u32) -> Self {
        InterruptMapping {
            device,
            irq_type,
            source_irq: source,
            target_vector: target,
            target_apic: 0,
            masked: false,
        }
    }
}

pub struct InterruptRemapper {
    /// Mappings
    mappings: [InterruptMapping; MAX_INTERRUPTS],
    mapping_count: usize,
    /// Is remapper enabled
    pub enabled: bool,
    /// Supports extended interrupt mode
    pub extended_mode: bool,
}

impl InterruptRemapper {
    pub fn new() -> Self {
        InterruptRemapper {
            mappings: [InterruptMapping::new(
                PciAddress::new(0, 0, 0),
                InterruptType::Intx,
                0,
                0,
            ); MAX_INTERRUPTS],
            mapping_count: 0,
            enabled: false,
            extended_mode: false,
        }
    }

    pub fn add_mapping(&mut self, mapping: InterruptMapping) -> Result<u32, &'static str> {
        if self.mapping_count >= MAX_INTERRUPTS {
            return Err("Maximum mappings reached");
        }

        let idx = self.mapping_count;
        self.mappings[idx] = mapping;
        self.mapping_count += 1;
        Ok(idx as u32)
    }

    pub fn remove_mapping(&mut self, idx: u32) -> Result<(), &'static str> {
        let idx = idx as usize;
        if idx >= self.mapping_count {
            return Err("Invalid mapping index");
        }

        for i in idx..self.mapping_count - 1 {
            self.mappings[i] = self.mappings[i + 1];
        }
        self.mapping_count -= 1;
        Ok(())
    }

    pub fn find_mapping(&self, device: PciAddress, source_irq: u32) -> Option<&InterruptMapping> {
        for i in 0..self.mapping_count {
            let m = &self.mappings[i];
            if m.device == device && m.source_irq == source_irq {
                return Some(m);
            }
        }
        None
    }

    pub fn mask(&mut self, device: PciAddress, source_irq: u32) -> Result<(), &'static str> {
        for i in 0..self.mapping_count {
            let m = &mut self.mappings[i];
            if m.device == device && m.source_irq == source_irq {
                m.masked = true;
                return Ok(());
            }
        }
        Err("Mapping not found")
    }

    pub fn unmask(&mut self, device: PciAddress, source_irq: u32) -> Result<(), &'static str> {
        for i in 0..self.mapping_count {
            let m = &mut self.mappings[i];
            if m.device == device && m.source_irq == source_irq {
                m.masked = false;
                return Ok(());
            }
        }
        Err("Mapping not found")
    }
}

// ===== Passthrough Manager =====

pub struct PassthroughManager {
    /// IOMMU controller
    pub iommu: IommuController,
    /// Interrupt remapper
    pub interrupt_remapper: InterruptRemapper,
    /// Passthrough devices
    devices: [VfioDevice; MAX_PASSTHROUGH_DEVICES],
    device_count: usize,
}

impl PassthroughManager {
    pub fn new(iommu_type: IommuType) -> Self {
        PassthroughManager {
            iommu: IommuController::new(iommu_type),
            interrupt_remapper: InterruptRemapper::new(),
            devices: [VfioDevice::new(PciDeviceInfo::new(
                PciAddress::new(0, 0, 0),
                0,
                0,
            )); MAX_PASSTHROUGH_DEVICES],
            device_count: 0,
        }
    }

    pub fn add_device(&mut self, pci_info: PciDeviceInfo) -> Result<usize, &'static str> {
        if self.device_count >= MAX_PASSTHROUGH_DEVICES {
            return Err("Maximum devices reached");
        }

        let idx = self.device_count;
        self.devices[idx] = VfioDevice::new(pci_info);
        self.device_count += 1;
        Ok(idx)
    }

    pub fn get_device(&self, idx: usize) -> Option<&VfioDevice> {
        if idx < self.device_count {
            Some(&self.devices[idx])
        } else {
            None
        }
    }

    pub fn get_device_mut(&mut self, idx: usize) -> Option<&mut VfioDevice> {
        if idx < self.device_count {
            Some(&mut self.devices[idx])
        } else {
            None
        }
    }

    pub fn find_device(&self, address: PciAddress) -> Option<usize> {
        for i in 0..self.device_count {
            if self.devices[i].pci_info.address == address {
                return Some(i);
            }
        }
        None
    }

    /// Prepare device for VM passthrough
    pub fn prepare_for_vm(
        &mut self,
        device_idx: usize,
        vm_id: u32,
    ) -> Result<u32, &'static str> {
        if device_idx >= self.device_count {
            return Err("Invalid device index");
        }

        let device = &mut self.devices[device_idx];

        // Bind to VFIO
        device.bind()?;

        // Open device
        device.open()?;

        // Create IOMMU domain for VM
        let domain_id = self.iommu.create_domain(DomainType::Guest)?;

        // Attach device to domain
        if let Some(domain) = self.iommu.get_domain_mut(domain_id) {
            domain.attach_device(device.pci_info.address)?;
        }

        // Configure device
        device.configure_for_vm(vm_id, domain_id)?;

        // Set up interrupt remapping
        if device.pci_info.has_msix() {
            for irq in 0..device.irq_count {
                let mapping = InterruptMapping::new(
                    device.pci_info.address,
                    InterruptType::MsiX,
                    irq,
                    32 + irq,  // Guest vector
                );
                self.interrupt_remapper.add_mapping(mapping)?;
            }
        }

        Ok(domain_id)
    }

    /// Release device from VM
    pub fn release_from_vm(&mut self, device_idx: usize) -> Result<(), &'static str> {
        if device_idx >= self.device_count {
            return Err("Invalid device index");
        }

        let device = &mut self.devices[device_idx];

        // Deactivate
        if device.state == VfioDeviceState::Active {
            device.deactivate()?;
        }

        // Remove interrupt mappings
        // Would iterate and remove

        // Detach from IOMMU domain
        if let Some(domain_id) = device.domain_id {
            if let Some(domain) = self.iommu.get_domain_mut(domain_id) {
                domain.detach_device(device.pci_info.address)?;
            }

            // Destroy domain if empty
            if let Some(domain) = self.iommu.get_domain(domain_id) {
                if domain.devices().is_empty() {
                    self.iommu.destroy_domain(domain_id)?;
                }
            }
        }

        // Close and unbind
        device.close()?;
        device.unbind()?;

        Ok(())
    }

    /// Map guest memory for DMA
    pub fn map_guest_memory(
        &mut self,
        domain_id: u32,
        iova: u64,
        phys_addr: u64,
        size: u64,
        write: bool,
    ) -> Result<(), &'static str> {
        let domain = self.iommu.get_domain_mut(domain_id)
            .ok_or("Domain not found")?;

        let region = DmaRegion::new(iova, phys_addr, size, true, write);
        domain.map_dma(region)
    }

    /// Unmap guest memory
    pub fn unmap_guest_memory(
        &mut self,
        domain_id: u32,
        iova: u64,
        size: u64,
    ) -> Result<(), &'static str> {
        let domain = self.iommu.get_domain_mut(domain_id)
            .ok_or("Domain not found")?;

        domain.unmap_dma(iova, size)
    }

    pub fn device_count(&self) -> usize {
        self.device_count
    }
}

// ===== GPU Passthrough =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

impl GpuVendor {
    pub fn from_vendor_id(vendor_id: u16) -> Self {
        match vendor_id {
            0x10DE => GpuVendor::Nvidia,
            0x1002 => GpuVendor::Amd,
            0x8086 => GpuVendor::Intel,
            _ => GpuVendor::Unknown,
        }
    }
}

#[derive(Copy, Clone)]
pub struct GpuPassthrough {
    /// VFIO device index
    pub device_idx: usize,
    /// GPU vendor
    pub vendor: GpuVendor,
    /// VBIOS loaded
    pub vbios_loaded: bool,
    /// ROM BAR enabled
    pub rom_bar_enabled: bool,
    /// Primary GPU
    pub is_primary: bool,
    /// UEFI GOP support
    pub uefi_gop: bool,
}

impl GpuPassthrough {
    pub fn new(device_idx: usize, vendor: GpuVendor) -> Self {
        GpuPassthrough {
            device_idx,
            vendor,
            vbios_loaded: false,
            rom_bar_enabled: false,
            is_primary: false,
            uefi_gop: false,
        }
    }

    pub fn load_vbios(&mut self, _vbios_data: &[u8]) -> Result<(), &'static str> {
        // Would load VBIOS for GPU
        self.vbios_loaded = true;
        Ok(())
    }

    pub fn enable_rom_bar(&mut self) -> Result<(), &'static str> {
        // Would enable ROM BAR
        self.rom_bar_enabled = true;
        Ok(())
    }

    pub fn set_primary(&mut self, primary: bool) {
        self.is_primary = primary;
    }
}

// ===== USB Passthrough =====

#[derive(Copy, Clone)]
pub struct UsbDevice {
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Bus number
    pub bus: u8,
    /// Device address
    pub address: u8,
    /// USB speed (1=Low, 2=Full, 3=High, 4=Super)
    pub speed: u8,
    /// Device class
    pub device_class: u8,
    /// Is device claimed for passthrough
    pub claimed: bool,
    /// Target VM ID
    pub vm_id: Option<u32>,
}

impl UsbDevice {
    pub fn new(vendor_id: u16, product_id: u16, bus: u8, address: u8) -> Self {
        UsbDevice {
            vendor_id,
            product_id,
            bus,
            address,
            speed: 3,  // High speed by default
            device_class: 0,
            claimed: false,
            vm_id: None,
        }
    }

    pub fn claim(&mut self, vm_id: u32) -> Result<(), &'static str> {
        if self.claimed {
            return Err("Device already claimed");
        }

        self.claimed = true;
        self.vm_id = Some(vm_id);
        Ok(())
    }

    pub fn release(&mut self) -> Result<(), &'static str> {
        if !self.claimed {
            return Err("Device not claimed");
        }

        self.claimed = false;
        self.vm_id = None;
        Ok(())
    }
}

// ===== Tests =====

pub fn test_pci_address() -> bool {
    let addr = PciAddress::new(5, 10, 2);
    if addr.bus != 5 || addr.device != 10 || addr.function != 2 {
        return false;
    }

    let bdf = addr.bdf();
    if bdf != ((5 << 8) | (10 << 3) | 2) {
        return false;
    }

    // Test device/function masking
    let addr2 = PciAddress::new(0, 0xFF, 0xFF);
    if addr2.device != 0x1F || addr2.function != 0x07 {
        return false;
    }

    true
}

pub fn test_iommu_domain() -> bool {
    let mut domain = IommuDomain::new(1, DomainType::Guest);

    // Attach device
    let device = PciAddress::new(3, 0, 0);
    if domain.attach_device(device).is_err() {
        return false;
    }

    // Map DMA region
    let region = DmaRegion::new(0x1000, 0x100000, 0x10000, true, true);
    if domain.map_dma(region).is_err() {
        return false;
    }

    // Translate
    if domain.translate_iova(0x1000) != Some(0x100000) {
        return false;
    }
    if domain.translate_iova(0x5000) != Some(0x104000) {
        return false;
    }
    if domain.translate_iova(0x20000).is_some() {
        return false;
    }

    // Detach device
    if domain.detach_device(device).is_err() {
        return false;
    }

    true
}

pub fn test_vfio_device_lifecycle() -> bool {
    let pci_info = PciDeviceInfo::new(PciAddress::new(3, 0, 0), 0x10DE, 0x1234);
    let mut device = VfioDevice::new(pci_info);

    // Initial state
    if device.state != VfioDeviceState::Unbound {
        return false;
    }

    // Bind
    if device.bind().is_err() {
        return false;
    }
    if device.state != VfioDeviceState::Bound {
        return false;
    }

    // Open
    if device.open().is_err() {
        return false;
    }
    if device.state != VfioDeviceState::Opened {
        return false;
    }

    // Configure
    if device.configure_for_vm(1, 1).is_err() {
        return false;
    }
    if device.state != VfioDeviceState::Configured {
        return false;
    }

    // Activate
    if device.activate().is_err() {
        return false;
    }
    if device.state != VfioDeviceState::Active {
        return false;
    }

    // Deactivate
    if device.deactivate().is_err() {
        return false;
    }

    // Close
    if device.close().is_err() {
        return false;
    }

    // Unbind
    if device.unbind().is_err() {
        return false;
    }
    if device.state != VfioDeviceState::Unbound {
        return false;
    }

    true
}

pub fn test_passthrough_manager() -> bool {
    let mut manager = PassthroughManager::new(IommuType::IntelVtd);

    // Add GPU device
    let gpu_info = PciDeviceInfo::new(PciAddress::new(3, 0, 0), 0x10DE, 0x2684);
    let idx = match manager.add_device(gpu_info) {
        Ok(i) => i,
        Err(_) => return false,
    };

    // Prepare for VM
    let domain_id = match manager.prepare_for_vm(idx, 1) {
        Ok(d) => d,
        Err(_) => return false,
    };

    // Map guest memory
    if manager.map_guest_memory(domain_id, 0, 0x100000000, 0x80000000, true).is_err() {
        return false;
    }

    // Verify device state
    if let Some(device) = manager.get_device(idx) {
        if device.state != VfioDeviceState::Configured {
            return false;
        }
    } else {
        return false;
    }

    // Release from VM
    if manager.release_from_vm(idx).is_err() {
        return false;
    }

    true
}

pub fn test_interrupt_remapping() -> bool {
    let mut remapper = InterruptRemapper::new();

    let device = PciAddress::new(3, 0, 0);
    let mapping = InterruptMapping::new(device, InterruptType::MsiX, 0, 32);

    // Add mapping
    if remapper.add_mapping(mapping).is_err() {
        return false;
    }

    // Find mapping
    if remapper.find_mapping(device, 0).is_none() {
        return false;
    }

    // Mask
    if remapper.mask(device, 0).is_err() {
        return false;
    }
    if let Some(m) = remapper.find_mapping(device, 0) {
        if !m.masked {
            return false;
        }
    }

    // Unmask
    if remapper.unmask(device, 0).is_err() {
        return false;
    }

    true
}
