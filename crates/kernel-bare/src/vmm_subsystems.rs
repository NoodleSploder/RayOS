// ===== RayOS VMM & Subsystems Integration Module (Phase 9B Task 4) =====
// Kernel VMM exposure via syscalls, VM registry, Linux/Windows subsystem management
// Device pass-through, binary compatibility layers, resource isolation

use core::fmt::Write;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ===== VMM Integration Constants =====

const MAX_VMS: usize = 16;
const MAX_VM_NAME: usize = 64;
const MAX_DEVICES: usize = 32;
const MAX_MEMORY_REGIONS: usize = 16;
const MAX_VCPUS: usize = 32;
const MAX_SUBSYSTEMS: usize = 4;

// ===== VM State =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VmState {
    /// VM is not created
    NotCreated,
    /// VM is created but not started
    Created,
    /// VM is starting up
    Starting,
    /// VM is running
    Running,
    /// VM is paused
    Paused,
    /// VM is shutting down
    ShuttingDown,
    /// VM is stopped
    Stopped,
    /// VM crashed or encountered an error
    Error,
    /// VM is being migrated
    Migrating,
    /// VM is suspended to disk
    Suspended,
}

impl VmState {
    pub fn as_str(&self) -> &'static str {
        match self {
            VmState::NotCreated => "not-created",
            VmState::Created => "created",
            VmState::Starting => "starting",
            VmState::Running => "running",
            VmState::Paused => "paused",
            VmState::ShuttingDown => "shutting-down",
            VmState::Stopped => "stopped",
            VmState::Error => "error",
            VmState::Migrating => "migrating",
            VmState::Suspended => "suspended",
        }
    }

    pub fn can_start(&self) -> bool {
        matches!(self, VmState::Created | VmState::Stopped | VmState::Suspended)
    }

    pub fn can_stop(&self) -> bool {
        matches!(self, VmState::Running | VmState::Paused)
    }

    pub fn can_pause(&self) -> bool {
        *self == VmState::Running
    }

    pub fn can_resume(&self) -> bool {
        *self == VmState::Paused
    }
}

// ===== VM Type =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VmType {
    /// Full virtual machine with hardware emulation
    FullVm,
    /// Container-based isolation
    Container,
    /// Linux subsystem (WSL-like)
    LinuxSubsystem,
    /// Windows subsystem
    WindowsSubsystem,
    /// Lightweight microVM
    MicroVm,
    /// Process-level sandboxing
    Sandbox,
}

impl VmType {
    pub fn as_str(&self) -> &'static str {
        match self {
            VmType::FullVm => "full-vm",
            VmType::Container => "container",
            VmType::LinuxSubsystem => "linux-subsystem",
            VmType::WindowsSubsystem => "windows-subsystem",
            VmType::MicroVm => "microvm",
            VmType::Sandbox => "sandbox",
        }
    }
}

// ===== Memory Region =====

#[derive(Debug, Copy, Clone)]
pub struct MemoryRegion {
    /// Guest physical address
    pub guest_phys_addr: u64,
    /// Host virtual address (for mapping)
    pub host_virt_addr: u64,
    /// Size in bytes
    pub size: u64,
    /// Region flags (read, write, execute, etc.)
    pub flags: MemoryFlags,
    /// Is this region backed by a file?
    pub file_backed: bool,
    /// Slot index in KVM/hypervisor
    pub slot: u32,
}

#[derive(Debug, Copy, Clone)]
pub struct MemoryFlags {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub dirty_logging: bool,
    pub readonly: bool,
}

impl MemoryFlags {
    pub fn rwx() -> Self {
        MemoryFlags {
            read: true,
            write: true,
            execute: true,
            dirty_logging: false,
            readonly: false,
        }
    }

    pub fn ro() -> Self {
        MemoryFlags {
            read: true,
            write: false,
            execute: false,
            dirty_logging: false,
            readonly: true,
        }
    }
}

impl MemoryRegion {
    pub fn new(guest_addr: u64, size: u64, slot: u32) -> Self {
        MemoryRegion {
            guest_phys_addr: guest_addr,
            host_virt_addr: 0,
            size,
            flags: MemoryFlags::rwx(),
            file_backed: false,
            slot,
        }
    }
}

// ===== vCPU State =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VcpuState {
    /// vCPU is not created
    NotCreated,
    /// vCPU is created but not running
    Created,
    /// vCPU is running
    Running,
    /// vCPU is halted (waiting for interrupt)
    Halted,
    /// vCPU hit an exit
    Exited,
    /// vCPU encountered an error
    Error,
}

#[derive(Copy, Clone)]
pub struct Vcpu {
    pub id: u32,
    pub state: VcpuState,
    /// APIC ID
    pub apic_id: u32,
    /// Last exit reason
    pub exit_reason: u32,
    /// CPU time in nanoseconds
    pub cpu_time_ns: u64,
    /// Number of exits
    pub exit_count: u64,
}

impl Vcpu {
    pub fn new(id: u32) -> Self {
        Vcpu {
            id,
            state: VcpuState::NotCreated,
            apic_id: id,
            exit_reason: 0,
            cpu_time_ns: 0,
            exit_count: 0,
        }
    }
}

// ===== Device Pass-through =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DeviceType {
    /// VirtIO block device
    VirtioBlock,
    /// VirtIO network device
    VirtioNet,
    /// VirtIO GPU device
    VirtioGpu,
    /// VirtIO console
    VirtioConsole,
    /// VirtIO input (keyboard/mouse)
    VirtioInput,
    /// VirtIO filesystem (virtiofs)
    VirtioFs,
    /// VirtIO vsock
    VirtioVsock,
    /// PCI passthrough device
    PciPassthrough,
    /// VFIO device
    VfioDevice,
    /// Emulated serial port
    Serial,
    /// Emulated RTC
    Rtc,
    /// Emulated i8042 (keyboard controller)
    I8042,
}

#[derive(Copy, Clone)]
pub struct VmDevice {
    pub device_type: DeviceType,
    pub device_id: u32,
    /// PCI bus/device/function for PCI devices
    pub pci_bdf: u32,
    /// MMIO base address
    pub mmio_base: u64,
    /// MMIO size
    pub mmio_size: u64,
    /// IRQ number
    pub irq: u32,
    /// Is device enabled
    pub enabled: bool,
    /// Device-specific data
    pub data: u64,
}

impl VmDevice {
    pub fn virtio_block(id: u32, mmio: u64) -> Self {
        VmDevice {
            device_type: DeviceType::VirtioBlock,
            device_id: id,
            pci_bdf: 0,
            mmio_base: mmio,
            mmio_size: 0x1000,
            irq: 32 + id,
            enabled: true,
            data: 0,
        }
    }

    pub fn virtio_net(id: u32, mmio: u64) -> Self {
        VmDevice {
            device_type: DeviceType::VirtioNet,
            device_id: id,
            pci_bdf: 0,
            mmio_base: mmio,
            mmio_size: 0x1000,
            irq: 32 + id,
            enabled: true,
            data: 0,
        }
    }

    pub fn virtio_gpu(id: u32, mmio: u64) -> Self {
        VmDevice {
            device_type: DeviceType::VirtioGpu,
            device_id: id,
            pci_bdf: 0,
            mmio_base: mmio,
            mmio_size: 0x1000,
            irq: 32 + id,
            enabled: true,
            data: 0,
        }
    }
}

// ===== VM Configuration =====

#[derive(Copy, Clone)]
pub struct VmConfig {
    /// Number of vCPUs
    pub vcpu_count: u32,
    /// Memory size in MiB
    pub memory_mib: u32,
    /// VM type
    pub vm_type: VmType,
    /// Enable KVM acceleration
    pub kvm_enabled: bool,
    /// Enable nested virtualization
    pub nested_virt: bool,
    /// Enable hugepages
    pub hugepages: bool,
    /// CPU topology (sockets)
    pub sockets: u32,
    /// CPU topology (cores per socket)
    pub cores_per_socket: u32,
    /// CPU topology (threads per core)
    pub threads_per_core: u32,
}

impl VmConfig {
    pub fn default_linux() -> Self {
        VmConfig {
            vcpu_count: 2,
            memory_mib: 2048,
            vm_type: VmType::LinuxSubsystem,
            kvm_enabled: true,
            nested_virt: false,
            hugepages: false,
            sockets: 1,
            cores_per_socket: 2,
            threads_per_core: 1,
        }
    }

    pub fn default_windows() -> Self {
        VmConfig {
            vcpu_count: 4,
            memory_mib: 4096,
            vm_type: VmType::WindowsSubsystem,
            kvm_enabled: true,
            nested_virt: false,
            hugepages: true,
            sockets: 1,
            cores_per_socket: 4,
            threads_per_core: 1,
        }
    }

    pub fn microvm(vcpus: u32, mem_mib: u32) -> Self {
        VmConfig {
            vcpu_count: vcpus,
            memory_mib: mem_mib,
            vm_type: VmType::MicroVm,
            kvm_enabled: true,
            nested_virt: false,
            hugepages: false,
            sockets: 1,
            cores_per_socket: vcpus,
            threads_per_core: 1,
        }
    }
}

// ===== VM Instance =====

#[derive(Copy, Clone)]
pub struct VmInstance {
    /// Unique VM ID
    pub vm_id: u32,
    /// VM name
    name: [u8; MAX_VM_NAME],
    name_len: usize,
    /// VM configuration
    pub config: VmConfig,
    /// Current state
    pub state: VmState,
    /// vCPUs
    vcpus: [Vcpu; MAX_VCPUS],
    vcpu_count: usize,
    /// Memory regions
    memory_regions: [MemoryRegion; MAX_MEMORY_REGIONS],
    region_count: usize,
    /// Attached devices
    devices: [VmDevice; MAX_DEVICES],
    device_count: usize,
    /// Creation timestamp
    pub created_at: u64,
    /// Start timestamp
    pub started_at: u64,
    /// Total runtime in seconds
    pub runtime_secs: u64,
    /// Exit code (if stopped)
    pub exit_code: i32,
}

impl VmInstance {
    pub fn new(vm_id: u32, config: VmConfig) -> Self {
        VmInstance {
            vm_id,
            name: [0u8; MAX_VM_NAME],
            name_len: 0,
            config,
            state: VmState::NotCreated,
            vcpus: [Vcpu::new(0); MAX_VCPUS],
            vcpu_count: 0,
            memory_regions: [MemoryRegion::new(0, 0, 0); MAX_MEMORY_REGIONS],
            region_count: 0,
            devices: [VmDevice::virtio_block(0, 0); MAX_DEVICES],
            device_count: 0,
            created_at: 0,
            started_at: 0,
            runtime_secs: 0,
            exit_code: 0,
        }
    }

    pub fn set_name(&mut self, name: &str) {
        let len = core::cmp::min(name.len(), MAX_VM_NAME - 1);
        for i in 0..len {
            self.name[i] = name.as_bytes()[i];
        }
        self.name_len = len;
    }

    pub fn name(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.name[..self.name_len]) }
    }

    pub fn create(&mut self) -> Result<(), &'static str> {
        if self.state != VmState::NotCreated {
            return Err("VM already created");
        }

        // Initialize vCPUs
        for i in 0..self.config.vcpu_count as usize {
            if i >= MAX_VCPUS {
                break;
            }
            self.vcpus[i] = Vcpu::new(i as u32);
            self.vcpus[i].state = VcpuState::Created;
            self.vcpu_count = i + 1;
        }

        // Create default memory region
        let mem_size = (self.config.memory_mib as u64) * 1024 * 1024;
        self.memory_regions[0] = MemoryRegion::new(0, mem_size, 0);
        self.region_count = 1;

        self.state = VmState::Created;
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), &'static str> {
        if !self.state.can_start() {
            return Err("Cannot start VM in current state");
        }

        self.state = VmState::Starting;

        // Start all vCPUs
        for i in 0..self.vcpu_count {
            self.vcpus[i].state = VcpuState::Running;
        }

        self.state = VmState::Running;
        self.started_at = 0;  // Would be actual timestamp
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), &'static str> {
        if !self.state.can_stop() {
            return Err("Cannot stop VM in current state");
        }

        self.state = VmState::ShuttingDown;

        // Stop all vCPUs
        for i in 0..self.vcpu_count {
            self.vcpus[i].state = VcpuState::Halted;
        }

        self.state = VmState::Stopped;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), &'static str> {
        if !self.state.can_pause() {
            return Err("Cannot pause VM");
        }

        for i in 0..self.vcpu_count {
            self.vcpus[i].state = VcpuState::Halted;
        }

        self.state = VmState::Paused;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), &'static str> {
        if !self.state.can_resume() {
            return Err("Cannot resume VM");
        }

        for i in 0..self.vcpu_count {
            self.vcpus[i].state = VcpuState::Running;
        }

        self.state = VmState::Running;
        Ok(())
    }

    pub fn add_device(&mut self, device: VmDevice) -> Result<u32, &'static str> {
        if self.device_count >= MAX_DEVICES {
            return Err("Maximum devices reached");
        }

        let idx = self.device_count;
        self.devices[idx] = device;
        self.device_count += 1;
        Ok(idx as u32)
    }

    pub fn add_memory_region(&mut self, region: MemoryRegion) -> Result<u32, &'static str> {
        if self.region_count >= MAX_MEMORY_REGIONS {
            return Err("Maximum memory regions reached");
        }

        let idx = self.region_count;
        self.memory_regions[idx] = region;
        self.region_count += 1;
        Ok(idx as u32)
    }

    pub fn get_vcpu(&self, id: u32) -> Option<&Vcpu> {
        if (id as usize) < self.vcpu_count {
            Some(&self.vcpus[id as usize])
        } else {
            None
        }
    }

    pub fn total_memory(&self) -> u64 {
        let mut total = 0u64;
        for i in 0..self.region_count {
            total += self.memory_regions[i].size;
        }
        total
    }
}

// ===== VM Registry =====

pub struct VmRegistry {
    vms: [VmInstance; MAX_VMS],
    vm_count: usize,
    next_vm_id: AtomicU32,
}

impl VmRegistry {
    pub fn new() -> Self {
        VmRegistry {
            vms: [VmInstance::new(0, VmConfig::default_linux()); MAX_VMS],
            vm_count: 0,
            next_vm_id: AtomicU32::new(1),
        }
    }

    pub fn create_vm(&mut self, name: &str, config: VmConfig) -> Result<u32, &'static str> {
        if self.vm_count >= MAX_VMS {
            return Err("Maximum VMs reached");
        }

        let vm_id = self.next_vm_id.fetch_add(1, Ordering::SeqCst);
        let idx = self.vm_count;

        self.vms[idx] = VmInstance::new(vm_id, config);
        self.vms[idx].set_name(name);
        self.vms[idx].create()?;
        self.vm_count += 1;

        Ok(vm_id)
    }

    pub fn get_vm(&self, vm_id: u32) -> Option<&VmInstance> {
        for i in 0..self.vm_count {
            if self.vms[i].vm_id == vm_id {
                return Some(&self.vms[i]);
            }
        }
        None
    }

    pub fn get_vm_mut(&mut self, vm_id: u32) -> Option<&mut VmInstance> {
        for i in 0..self.vm_count {
            if self.vms[i].vm_id == vm_id {
                return Some(&mut self.vms[i]);
            }
        }
        None
    }

    pub fn destroy_vm(&mut self, vm_id: u32) -> Result<(), &'static str> {
        let mut found_idx = None;
        for i in 0..self.vm_count {
            if self.vms[i].vm_id == vm_id {
                if self.vms[i].state == VmState::Running {
                    return Err("Cannot destroy running VM");
                }
                found_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = found_idx {
            // Shift remaining VMs
            for i in idx..self.vm_count - 1 {
                self.vms[i] = self.vms[i + 1];
            }
            self.vm_count -= 1;
            Ok(())
        } else {
            Err("VM not found")
        }
    }

    pub fn list_vms(&self) -> impl Iterator<Item = &VmInstance> {
        self.vms[..self.vm_count].iter()
    }

    pub fn vm_count(&self) -> usize {
        self.vm_count
    }

    pub fn running_count(&self) -> usize {
        self.vms[..self.vm_count]
            .iter()
            .filter(|vm| vm.state == VmState::Running)
            .count()
    }
}

// ===== Subsystem Type =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SubsystemType {
    /// Linux subsystem (like WSL)
    Linux,
    /// Windows subsystem (WINE-like or full VM)
    Windows,
    /// Android subsystem
    Android,
    /// FreeBSD subsystem
    FreeBsd,
}

impl SubsystemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubsystemType::Linux => "linux",
            SubsystemType::Windows => "windows",
            SubsystemType::Android => "android",
            SubsystemType::FreeBsd => "freebsd",
        }
    }
}

// ===== Subsystem State =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SubsystemState {
    /// Not installed
    NotInstalled,
    /// Installing
    Installing,
    /// Installed but not running
    Stopped,
    /// Starting up
    Starting,
    /// Running
    Running,
    /// Shutting down
    ShuttingDown,
    /// Error state
    Error,
    /// Updating
    Updating,
}

// ===== Binary Format =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BinaryFormat {
    /// Native RayOS binary
    Native,
    /// Linux ELF binary
    LinuxElf,
    /// Windows PE binary
    WindowsPe,
    /// macOS Mach-O binary
    MachO,
    /// WebAssembly
    Wasm,
    /// Script (shebang)
    Script,
}

impl BinaryFormat {
    pub fn detect(header: &[u8]) -> Option<Self> {
        if header.len() < 4 {
            return None;
        }

        // ELF magic
        if header[0] == 0x7F && header[1] == b'E' && header[2] == b'L' && header[3] == b'F' {
            return Some(BinaryFormat::LinuxElf);
        }

        // PE magic (MZ)
        if header[0] == b'M' && header[1] == b'Z' {
            return Some(BinaryFormat::WindowsPe);
        }

        // Mach-O magic
        if header.len() >= 4 {
            let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            if magic == 0xFEEDFACE || magic == 0xFEEDFACF ||
               magic == 0xCAFEBABE || magic == 0xBEBAFECA {
                return Some(BinaryFormat::MachO);
            }
        }

        // WebAssembly magic
        if header[0] == 0x00 && header[1] == b'a' && header[2] == b's' && header[3] == b'm' {
            return Some(BinaryFormat::Wasm);
        }

        // Shebang
        if header[0] == b'#' && header[1] == b'!' {
            return Some(BinaryFormat::Script);
        }

        None
    }
}

// ===== Subsystem Instance =====

#[derive(Copy, Clone)]
pub struct Subsystem {
    /// Subsystem type
    pub subsystem_type: SubsystemType,
    /// Current state
    pub state: SubsystemState,
    /// Associated VM ID (if running in VM)
    pub vm_id: Option<u32>,
    /// Root filesystem path
    rootfs_path: [u8; 128],
    rootfs_path_len: usize,
    /// Default user ID
    pub default_uid: u32,
    /// Memory limit in MiB
    pub memory_limit_mib: u32,
    /// CPU limit (percentage * 100)
    pub cpu_limit: u32,
    /// Network isolation enabled
    pub network_isolated: bool,
    /// Filesystem isolation enabled
    pub fs_isolated: bool,
    /// Number of running processes
    pub process_count: u32,
    /// Total CPU time used (seconds)
    pub cpu_time_secs: u64,
    /// Total memory used (bytes)
    pub memory_used: u64,
}

impl Subsystem {
    pub fn new(subsystem_type: SubsystemType) -> Self {
        Subsystem {
            subsystem_type,
            state: SubsystemState::NotInstalled,
            vm_id: None,
            rootfs_path: [0u8; 128],
            rootfs_path_len: 0,
            default_uid: 1000,
            memory_limit_mib: 2048,
            cpu_limit: 10000,  // 100%
            network_isolated: false,
            fs_isolated: true,
            process_count: 0,
            cpu_time_secs: 0,
            memory_used: 0,
        }
    }

    pub fn set_rootfs(&mut self, path: &str) {
        let len = core::cmp::min(path.len(), 127);
        for i in 0..len {
            self.rootfs_path[i] = path.as_bytes()[i];
        }
        self.rootfs_path_len = len;
    }

    pub fn rootfs(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.rootfs_path[..self.rootfs_path_len]) }
    }

    pub fn install(&mut self) -> Result<(), &'static str> {
        if self.state != SubsystemState::NotInstalled {
            return Err("Subsystem already installed");
        }

        self.state = SubsystemState::Installing;
        // Would download and extract rootfs
        self.state = SubsystemState::Stopped;
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), &'static str> {
        match self.state {
            SubsystemState::Stopped => {
                self.state = SubsystemState::Starting;
                // Would start init process
                self.state = SubsystemState::Running;
                Ok(())
            }
            SubsystemState::Running => Ok(()),
            _ => Err("Cannot start subsystem in current state"),
        }
    }

    pub fn stop(&mut self) -> Result<(), &'static str> {
        if self.state != SubsystemState::Running {
            return Err("Subsystem not running");
        }

        self.state = SubsystemState::ShuttingDown;
        // Would terminate all processes
        self.process_count = 0;
        self.state = SubsystemState::Stopped;
        Ok(())
    }

    pub fn uninstall(&mut self) -> Result<(), &'static str> {
        if self.state == SubsystemState::Running {
            return Err("Stop subsystem before uninstalling");
        }

        // Would delete rootfs
        self.state = SubsystemState::NotInstalled;
        self.rootfs_path_len = 0;
        Ok(())
    }
}

// ===== Subsystem Manager =====

pub struct SubsystemManager {
    subsystems: [Subsystem; MAX_SUBSYSTEMS],
    initialized: [bool; MAX_SUBSYSTEMS],
    vm_registry: VmRegistry,
}

impl SubsystemManager {
    pub fn new() -> Self {
        SubsystemManager {
            subsystems: [
                Subsystem::new(SubsystemType::Linux),
                Subsystem::new(SubsystemType::Windows),
                Subsystem::new(SubsystemType::Android),
                Subsystem::new(SubsystemType::FreeBsd),
            ],
            initialized: [false; MAX_SUBSYSTEMS],
            vm_registry: VmRegistry::new(),
        }
    }

    pub fn get_subsystem(&self, subsystem_type: SubsystemType) -> Option<&Subsystem> {
        let idx = subsystem_type as usize;
        if idx < MAX_SUBSYSTEMS && self.initialized[idx] {
            Some(&self.subsystems[idx])
        } else {
            None
        }
    }

    pub fn get_subsystem_mut(&mut self, subsystem_type: SubsystemType) -> &mut Subsystem {
        let idx = subsystem_type as usize;
        self.initialized[idx] = true;
        &mut self.subsystems[idx]
    }

    pub fn install_subsystem(&mut self, subsystem_type: SubsystemType, rootfs: &str) -> Result<(), &'static str> {
        let subsystem = self.get_subsystem_mut(subsystem_type);
        subsystem.set_rootfs(rootfs);
        subsystem.install()
    }

    pub fn start_subsystem(&mut self, subsystem_type: SubsystemType) -> Result<(), &'static str> {
        let idx = subsystem_type as usize;
        self.initialized[idx] = true;

        // For full isolation, create a VM
        if self.subsystems[idx].fs_isolated {
            let config = match subsystem_type {
                SubsystemType::Linux => VmConfig::default_linux(),
                SubsystemType::Windows => VmConfig::default_windows(),
                _ => VmConfig::microvm(2, 1024),
            };

            let vm_id = self.vm_registry.create_vm(
                subsystem_type.as_str(),
                config,
            )?;

            if let Some(vm) = self.vm_registry.get_vm_mut(vm_id) {
                vm.start()?;
            }

            self.subsystems[idx].vm_id = Some(vm_id);
        }

        self.subsystems[idx].start()
    }

    pub fn stop_subsystem(&mut self, subsystem_type: SubsystemType) -> Result<(), &'static str> {
        let idx = subsystem_type as usize;
        self.initialized[idx] = true;

        // Stop associated VM
        let vm_id_opt = self.subsystems[idx].vm_id;
        if let Some(vm_id) = vm_id_opt {
            if let Some(vm) = self.vm_registry.get_vm_mut(vm_id) {
                let _ = vm.stop();
            }
            self.subsystems[idx].vm_id = None;
        }

        self.subsystems[idx].stop()
    }

    pub fn execute_binary(&mut self, path: &str, header: &[u8]) -> Result<u32, &'static str> {
        let format = BinaryFormat::detect(header).ok_or("Unknown binary format")?;

        match format {
            BinaryFormat::Native => {
                // Execute directly
                Ok(0)
            }
            BinaryFormat::LinuxElf => {
                // Ensure Linux subsystem is running
                let linux = self.get_subsystem_mut(SubsystemType::Linux);
                if linux.state != SubsystemState::Running {
                    linux.start()?;
                }
                linux.process_count += 1;
                Ok(linux.process_count)
            }
            BinaryFormat::WindowsPe => {
                // Ensure Windows subsystem is running
                let windows = self.get_subsystem_mut(SubsystemType::Windows);
                if windows.state != SubsystemState::Running {
                    windows.start()?;
                }
                windows.process_count += 1;
                Ok(windows.process_count)
            }
            BinaryFormat::Wasm => {
                // Execute in WASM runtime
                Ok(0)
            }
            _ => Err("Unsupported binary format"),
        }
    }

    pub fn vm_registry(&self) -> &VmRegistry {
        &self.vm_registry
    }

    pub fn vm_registry_mut(&mut self) -> &mut VmRegistry {
        &mut self.vm_registry
    }
}

// ===== VMM Syscall Interface =====

#[derive(Debug, Copy, Clone)]
pub enum VmmSyscall {
    /// Create a new VM
    CreateVm = 0x100,
    /// Destroy a VM
    DestroyVm = 0x101,
    /// Start a VM
    StartVm = 0x102,
    /// Stop a VM
    StopVm = 0x103,
    /// Pause a VM
    PauseVm = 0x104,
    /// Resume a VM
    ResumeVm = 0x105,
    /// Get VM info
    GetVmInfo = 0x106,
    /// Set VM memory
    SetVmMemory = 0x107,
    /// Add device to VM
    AddVmDevice = 0x108,
    /// Remove device from VM
    RemoveVmDevice = 0x109,
    /// Create vCPU
    CreateVcpu = 0x110,
    /// Run vCPU
    RunVcpu = 0x111,
    /// Get vCPU registers
    GetVcpuRegs = 0x112,
    /// Set vCPU registers
    SetVcpuRegs = 0x113,
    /// Install subsystem
    InstallSubsystem = 0x120,
    /// Start subsystem
    StartSubsystem = 0x121,
    /// Stop subsystem
    StopSubsystem = 0x122,
    /// Execute in subsystem
    ExecSubsystem = 0x123,
}

#[derive(Debug, Copy, Clone)]
pub struct VmmSyscallResult {
    pub success: bool,
    pub error_code: i32,
    pub value: u64,
}

impl VmmSyscallResult {
    pub fn ok(value: u64) -> Self {
        VmmSyscallResult {
            success: true,
            error_code: 0,
            value,
        }
    }

    pub fn err(code: i32) -> Self {
        VmmSyscallResult {
            success: false,
            error_code: code,
            value: 0,
        }
    }
}

// ===== VMM Syscall Handler =====

pub struct VmmSyscallHandler {
    subsystem_manager: SubsystemManager,
}

impl VmmSyscallHandler {
    pub fn new() -> Self {
        VmmSyscallHandler {
            subsystem_manager: SubsystemManager::new(),
        }
    }

    pub fn handle(&mut self, syscall: VmmSyscall, arg1: u64, arg2: u64, arg3: u64) -> VmmSyscallResult {
        match syscall {
            VmmSyscall::CreateVm => {
                let vcpus = arg1 as u32;
                let mem_mib = arg2 as u32;
                let config = VmConfig::microvm(vcpus, mem_mib);
                match self.subsystem_manager.vm_registry_mut().create_vm("vm", config) {
                    Ok(vm_id) => VmmSyscallResult::ok(vm_id as u64),
                    Err(_) => VmmSyscallResult::err(-1),
                }
            }
            VmmSyscall::DestroyVm => {
                let vm_id = arg1 as u32;
                match self.subsystem_manager.vm_registry_mut().destroy_vm(vm_id) {
                    Ok(()) => VmmSyscallResult::ok(0),
                    Err(_) => VmmSyscallResult::err(-1),
                }
            }
            VmmSyscall::StartVm => {
                let vm_id = arg1 as u32;
                if let Some(vm) = self.subsystem_manager.vm_registry_mut().get_vm_mut(vm_id) {
                    match vm.start() {
                        Ok(()) => VmmSyscallResult::ok(0),
                        Err(_) => VmmSyscallResult::err(-1),
                    }
                } else {
                    VmmSyscallResult::err(-2)
                }
            }
            VmmSyscall::StopVm => {
                let vm_id = arg1 as u32;
                if let Some(vm) = self.subsystem_manager.vm_registry_mut().get_vm_mut(vm_id) {
                    match vm.stop() {
                        Ok(()) => VmmSyscallResult::ok(0),
                        Err(_) => VmmSyscallResult::err(-1),
                    }
                } else {
                    VmmSyscallResult::err(-2)
                }
            }
            VmmSyscall::PauseVm => {
                let vm_id = arg1 as u32;
                if let Some(vm) = self.subsystem_manager.vm_registry_mut().get_vm_mut(vm_id) {
                    match vm.pause() {
                        Ok(()) => VmmSyscallResult::ok(0),
                        Err(_) => VmmSyscallResult::err(-1),
                    }
                } else {
                    VmmSyscallResult::err(-2)
                }
            }
            VmmSyscall::ResumeVm => {
                let vm_id = arg1 as u32;
                if let Some(vm) = self.subsystem_manager.vm_registry_mut().get_vm_mut(vm_id) {
                    match vm.resume() {
                        Ok(()) => VmmSyscallResult::ok(0),
                        Err(_) => VmmSyscallResult::err(-1),
                    }
                } else {
                    VmmSyscallResult::err(-2)
                }
            }
            VmmSyscall::GetVmInfo => {
                let vm_id = arg1 as u32;
                if let Some(vm) = self.subsystem_manager.vm_registry().get_vm(vm_id) {
                    VmmSyscallResult::ok(vm.state as u64)
                } else {
                    VmmSyscallResult::err(-2)
                }
            }
            VmmSyscall::InstallSubsystem => {
                let subsystem_type = match arg1 {
                    0 => SubsystemType::Linux,
                    1 => SubsystemType::Windows,
                    2 => SubsystemType::Android,
                    3 => SubsystemType::FreeBsd,
                    _ => return VmmSyscallResult::err(-3),
                };
                match self.subsystem_manager.install_subsystem(subsystem_type, "/subsystems") {
                    Ok(()) => VmmSyscallResult::ok(0),
                    Err(_) => VmmSyscallResult::err(-1),
                }
            }
            VmmSyscall::StartSubsystem => {
                let subsystem_type = match arg1 {
                    0 => SubsystemType::Linux,
                    1 => SubsystemType::Windows,
                    2 => SubsystemType::Android,
                    3 => SubsystemType::FreeBsd,
                    _ => return VmmSyscallResult::err(-3),
                };
                match self.subsystem_manager.start_subsystem(subsystem_type) {
                    Ok(()) => VmmSyscallResult::ok(0),
                    Err(_) => VmmSyscallResult::err(-1),
                }
            }
            VmmSyscall::StopSubsystem => {
                let subsystem_type = match arg1 {
                    0 => SubsystemType::Linux,
                    1 => SubsystemType::Windows,
                    2 => SubsystemType::Android,
                    3 => SubsystemType::FreeBsd,
                    _ => return VmmSyscallResult::err(-3),
                };
                match self.subsystem_manager.stop_subsystem(subsystem_type) {
                    Ok(()) => VmmSyscallResult::ok(0),
                    Err(_) => VmmSyscallResult::err(-1),
                }
            }
            _ => VmmSyscallResult::err(-4),  // Not implemented
        }
    }

    pub fn subsystem_manager(&self) -> &SubsystemManager {
        &self.subsystem_manager
    }

    pub fn subsystem_manager_mut(&mut self) -> &mut SubsystemManager {
        &mut self.subsystem_manager
    }
}

// ===== Resource Isolation =====

#[derive(Copy, Clone)]
pub struct ResourceLimits {
    /// CPU limit (percentage * 100, 10000 = 100%)
    pub cpu_limit: u32,
    /// Memory limit in bytes
    pub memory_limit: u64,
    /// Disk I/O limit (bytes per second)
    pub disk_io_limit: u64,
    /// Network bandwidth limit (bytes per second)
    pub network_limit: u64,
    /// Maximum number of processes
    pub max_processes: u32,
    /// Maximum number of open files
    pub max_files: u32,
    /// Maximum number of threads
    pub max_threads: u32,
}

impl ResourceLimits {
    pub fn default() -> Self {
        ResourceLimits {
            cpu_limit: 10000,  // 100%
            memory_limit: 4 * 1024 * 1024 * 1024,  // 4 GB
            disk_io_limit: 100 * 1024 * 1024,  // 100 MB/s
            network_limit: 100 * 1024 * 1024,  // 100 MB/s
            max_processes: 1000,
            max_files: 65536,
            max_threads: 10000,
        }
    }

    pub fn restricted() -> Self {
        ResourceLimits {
            cpu_limit: 5000,  // 50%
            memory_limit: 1024 * 1024 * 1024,  // 1 GB
            disk_io_limit: 10 * 1024 * 1024,  // 10 MB/s
            network_limit: 10 * 1024 * 1024,  // 10 MB/s
            max_processes: 100,
            max_files: 1024,
            max_threads: 500,
        }
    }
}

#[derive(Copy, Clone)]
pub struct ResourceUsage {
    /// Current CPU usage (percentage * 100)
    pub cpu_usage: u32,
    /// Current memory usage in bytes
    pub memory_usage: u64,
    /// Disk I/O (bytes per second)
    pub disk_io: u64,
    /// Network I/O (bytes per second)
    pub network_io: u64,
    /// Current process count
    pub process_count: u32,
    /// Current open file count
    pub file_count: u32,
    /// Current thread count
    pub thread_count: u32,
}

impl ResourceUsage {
    pub fn new() -> Self {
        ResourceUsage {
            cpu_usage: 0,
            memory_usage: 0,
            disk_io: 0,
            network_io: 0,
            process_count: 0,
            file_count: 0,
            thread_count: 0,
        }
    }

    pub fn exceeds_limits(&self, limits: &ResourceLimits) -> bool {
        self.cpu_usage > limits.cpu_limit ||
        self.memory_usage > limits.memory_limit ||
        self.disk_io > limits.disk_io_limit ||
        self.network_io > limits.network_limit ||
        self.process_count > limits.max_processes ||
        self.file_count > limits.max_files ||
        self.thread_count > limits.max_threads
    }
}

// ===== Global VMM Handler =====

static mut VMM_HANDLER: Option<VmmSyscallHandler> = None;

pub fn vmm_handler() -> &'static mut VmmSyscallHandler {
    unsafe {
        if VMM_HANDLER.is_none() {
            VMM_HANDLER = Some(VmmSyscallHandler::new());
        }
        VMM_HANDLER.as_mut().unwrap()
    }
}

// ===== Tests =====

pub fn test_vm_lifecycle() -> bool {
    let mut registry = VmRegistry::new();

    // Create VM
    let vm_id = match registry.create_vm("test-vm", VmConfig::microvm(2, 1024)) {
        Ok(id) => id,
        Err(_) => return false,
    };

    // Get VM
    let vm = match registry.get_vm(vm_id) {
        Some(v) => v,
        None => return false,
    };

    if vm.state != VmState::Created {
        return false;
    }

    // Start VM
    if let Some(vm) = registry.get_vm_mut(vm_id) {
        if vm.start().is_err() {
            return false;
        }
    }

    // Verify running
    if let Some(vm) = registry.get_vm(vm_id) {
        if vm.state != VmState::Running {
            return false;
        }
    }

    // Stop VM
    if let Some(vm) = registry.get_vm_mut(vm_id) {
        if vm.stop().is_err() {
            return false;
        }
    }

    // Destroy VM
    if registry.destroy_vm(vm_id).is_err() {
        return false;
    }

    if registry.vm_count() != 0 {
        return false;
    }

    true
}

pub fn test_subsystem_lifecycle() -> bool {
    let mut manager = SubsystemManager::new();

    // Install Linux subsystem
    if manager.install_subsystem(SubsystemType::Linux, "/var/subsystems/linux").is_err() {
        return false;
    }

    // Start subsystem
    if manager.start_subsystem(SubsystemType::Linux).is_err() {
        return false;
    }

    // Verify running
    if let Some(subsystem) = manager.get_subsystem(SubsystemType::Linux) {
        if subsystem.state != SubsystemState::Running {
            return false;
        }
    } else {
        return false;
    }

    // Stop subsystem
    if manager.stop_subsystem(SubsystemType::Linux).is_err() {
        return false;
    }

    true
}

pub fn test_binary_format_detection() -> bool {
    // ELF binary
    let elf_header = [0x7F, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00];
    if BinaryFormat::detect(&elf_header) != Some(BinaryFormat::LinuxElf) {
        return false;
    }

    // PE binary
    let pe_header = [b'M', b'Z', 0x90, 0x00];
    if BinaryFormat::detect(&pe_header) != Some(BinaryFormat::WindowsPe) {
        return false;
    }

    // WASM binary
    let wasm_header = [0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00];
    if BinaryFormat::detect(&wasm_header) != Some(BinaryFormat::Wasm) {
        return false;
    }

    // Shebang
    let script_header = [b'#', b'!', b'/', b'b', b'i', b'n'];
    if BinaryFormat::detect(&script_header) != Some(BinaryFormat::Script) {
        return false;
    }

    true
}

pub fn test_resource_limits() -> bool {
    let limits = ResourceLimits::restricted();
    let mut usage = ResourceUsage::new();

    // Under limits
    usage.cpu_usage = 2500;  // 25%
    usage.memory_usage = 512 * 1024 * 1024;  // 512 MB
    if usage.exceeds_limits(&limits) {
        return false;
    }

    // Exceed CPU
    usage.cpu_usage = 6000;  // 60%
    if !usage.exceeds_limits(&limits) {
        return false;
    }

    // Exceed memory
    usage.cpu_usage = 2500;
    usage.memory_usage = 2 * 1024 * 1024 * 1024;  // 2 GB
    if !usage.exceeds_limits(&limits) {
        return false;
    }

    true
}
