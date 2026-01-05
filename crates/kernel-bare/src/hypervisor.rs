//! Hypervisor runtime skeleton (x86_64 VMX-first).
//!
//! This is intentionally a *skeleton*:
//! - Detects whether VMX is available.
//! - Enables VMX operation (when allowed by IA32_FEATURE_CONTROL).
//! - Allocates VMXON + VMCS regions and executes VMXON/VMCLEAR/VMPTRLD.
//! - Does not yet build EPT/NPT, a guest VM, or a VM-exit handler loop.

#![allow(dead_code)]

use core::arch::asm;
use core::cmp;
use core::convert::TryInto;
use core::ptr;
use core::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use crate::VirtqDesc;

// Test hook: when non-zero, cause inject_guest_interrupt to always fail (unit tests).
#[cfg(any(test, feature = "vmm_inject_force_all_fail"))]
pub(crate) static INJECT_FORCE_FAIL: AtomicU32 = AtomicU32::new(0);
#[cfg(not(any(test, feature = "vmm_inject_force_all_fail")))]
static INJECT_FORCE_FAIL: AtomicU32 = AtomicU32::new(0);

use crate::guest_driver_template::{
    GUEST_DRIVER_BINARY, GUEST_DRIVER_DESC_DATA_OFFSET, GUEST_DRIVER_DESC_DATA_PTR_OFFSET,
};

use crate::LAPIC_MMIO;

#[cfg(feature = "vmm_virtio_gpu")]
use crate::vmm::virtio_gpu::VirtioGpuDevice;

// MSRs
const IA32_FEATURE_CONTROL: u32 = 0x3A;
const IA32_VMX_BASIC: u32 = 0x480;
const IA32_VMX_CR0_FIXED0: u32 = 0x486;
const IA32_VMX_CR0_FIXED1: u32 = 0x487;
const IA32_VMX_CR4_FIXED0: u32 = 0x488;
const IA32_VMX_CR4_FIXED1: u32 = 0x489;

// IA32_FEATURE_CONTROL bits
const FC_LOCK: u64 = 1 << 0;
const FC_VMXON_OUTSIDE_SMX: u64 = 1 << 2;

// CR4.VMXE
const CR4_VMXE: u64 = 1 << 13;

// VMCS fields (encodings).
// We only use the VM-instruction error field initially; more will be added as
// we progress toward a real VM-exit loop.
const VMCS_VM_INSTRUCTION_ERROR: u64 = 0x4400;
const VMCS_EXIT_REASON: u64 = 0x4402;
const VMCS_EXIT_INTERRUPTION_INFO: u64 = 0x4404;
const VMCS_EXIT_INTERRUPTION_ERROR_CODE: u64 = 0x4406;
const VMCS_VMEXIT_INSTRUCTION_LEN: u64 = 0x440C;
const VMCS_EXIT_QUALIFICATION: u64 = 0x6400;
const GUEST_LINEAR_ADDRESS: u64 = 0x640A;
const GUEST_PHYSICAL_ADDRESS: u64 = 0x2400;
// VM-entry interruption info field (to request an injected interrupt on VM-entry).
const VMCS_ENTRY_INTERRUPTION_INFO: u64 = 0x4016;
// VM-entry exception error code field (used when injecting exceptions that push an error code).
const VMCS_ENTRY_EXCEPTION_ERROR_CODE: u64 = 0x4018;
// Default vector to use for virtio-mmio notifications (pick a free vector in the usable range).
const VIRTIO_MMIO_IRQ_VECTOR: u8 = 0x20; // IRQ vector 32 (first non-reserved)

// Bounded retry attempts for pending interrupt injection
const MAX_INT_INJECT_ATTEMPTS: u32 = 5;

// 64-bit control fields
const VMCS_LINK_POINTER: u64 = 0x2800;
const EPT_POINTER: u64 = 0x201A;
const IO_BITMAP_A: u64 = 0x2000;
const IO_BITMAP_B: u64 = 0x2002;
const MSR_BITMAPS: u64 = 0x2004;

// 32-bit control fields
const PIN_BASED_VM_EXEC_CONTROL: u64 = 0x4000;
const CPU_BASED_VM_EXEC_CONTROL: u64 = 0x4002;
const EXCEPTION_BITMAP: u64 = 0x4004;
const VM_EXIT_CONTROLS: u64 = 0x400C;
const VM_ENTRY_CONTROLS: u64 = 0x4012;
const SECONDARY_VM_EXEC_CONTROL: u64 = 0x401E;

// VMX-preemption timer (optional): if supported, can be used to force periodic VM-exits for
// debugging even when the guest is executing pure compute loops with no exits.
const PIN_CTL_VMX_PREEMPTION_TIMER: u32 = 1 << 6;
const VMX_PREEMPTION_TIMER_VALUE: u64 = 0x482E;

// CPU-based execution control bits
const CPU_CTL_HLT_EXITING: u32 = 1 << 7;
const CPU_CTL_CPUID_EXITING: u32 = 1 << 21;
const CPU_CTL_UNCOND_IO_EXITING: u32 = 1 << 24;
const CPU_CTL_USE_IO_BITMAPS: u32 = 1 << 25;
const CPU_CTL_USE_MSR_BITMAPS: u32 = 1 << 28;
const CPU_CTL_ACTIVATE_SECONDARY_CONTROLS: u32 = 1 << 31;

// Secondary processor-based execution controls
const CPU2_CTL_ENABLE_EPT: u32 = 1 << 1;
const CPU2_CTL_ENABLE_RDTSCP: u32 = 1 << 3;
const CPU2_CTL_UNRESTRICTED_GUEST: u32 = 1 << 7;
const CPU2_CTL_ENABLE_INVPCID: u32 = 1 << 12;

// Guest-state fields (subset)
const GUEST_CR0: u64 = 0x6800;
const GUEST_CR3: u64 = 0x6802;
const GUEST_CR4: u64 = 0x6804;

// Control register virtualization fields.
// See Intel SDM: VMCS encodings for CR0/CR4 guest/host mask and read shadow.
const CR0_GUEST_HOST_MASK: u64 = 0x6000;
const CR4_GUEST_HOST_MASK: u64 = 0x6002;
const CR0_READ_SHADOW: u64 = 0x6004;
const CR4_READ_SHADOW: u64 = 0x6006;

const GUEST_RSP: u64 = 0x681C;
const GUEST_RIP: u64 = 0x681E;
const GUEST_RFLAGS: u64 = 0x6820;

// Guest interruptibility/activity state (32-bit fields).
// Useful to understand whether the guest is halting with interrupts disabled.
const GUEST_INTERRUPTIBILITY_STATE: u64 = 0x4824;
const GUEST_ACTIVITY_STATE: u64 = 0x4826;

const GUEST_ES_SELECTOR: u64 = 0x0800;
const GUEST_CS_SELECTOR: u64 = 0x0802;
const GUEST_SS_SELECTOR: u64 = 0x0804;
const GUEST_DS_SELECTOR: u64 = 0x0806;
const GUEST_FS_SELECTOR: u64 = 0x0808;
const GUEST_GS_SELECTOR: u64 = 0x080A;
const GUEST_LDTR_SELECTOR: u64 = 0x080C;
const GUEST_TR_SELECTOR: u64 = 0x080E;

const GUEST_ES_LIMIT: u64 = 0x4800;
const GUEST_CS_LIMIT: u64 = 0x4802;
const GUEST_SS_LIMIT: u64 = 0x4804;
const GUEST_DS_LIMIT: u64 = 0x4806;
const GUEST_FS_LIMIT: u64 = 0x4808;
const GUEST_GS_LIMIT: u64 = 0x480A;
const GUEST_LDTR_LIMIT: u64 = 0x480C;
const GUEST_TR_LIMIT: u64 = 0x480E;
const GUEST_GDTR_LIMIT: u64 = 0x4810;
const GUEST_IDTR_LIMIT: u64 = 0x4812;

const GUEST_ES_AR_BYTES: u64 = 0x4814;
const GUEST_CS_AR_BYTES: u64 = 0x4816;
const GUEST_SS_AR_BYTES: u64 = 0x4818;
const GUEST_DS_AR_BYTES: u64 = 0x481A;
const GUEST_FS_AR_BYTES: u64 = 0x481C;
const GUEST_GS_AR_BYTES: u64 = 0x481E;
const GUEST_LDTR_AR_BYTES: u64 = 0x4820;
const GUEST_TR_AR_BYTES: u64 = 0x4822;

const GUEST_ES_BASE: u64 = 0x6806;
const GUEST_CS_BASE: u64 = 0x6808;
const GUEST_SS_BASE: u64 = 0x680A;
const GUEST_DS_BASE: u64 = 0x680C;
const GUEST_FS_BASE: u64 = 0x680E;
const GUEST_GS_BASE: u64 = 0x6810;
const GUEST_LDTR_BASE: u64 = 0x6812;
const GUEST_TR_BASE: u64 = 0x6814;
const GUEST_GDTR_BASE: u64 = 0x6816;
const GUEST_IDTR_BASE: u64 = 0x6818;

const GUEST_DR7: u64 = 0x681A;
const GUEST_GDT_STATIC_ENTRIES: usize = 3;
const GUEST_GDT_ENTRY_COUNT: usize = GUEST_GDT_STATIC_ENTRIES + 2; // TSS descriptor takes two slots
const GUEST_GDT_ENTRIES: [u64; GUEST_GDT_STATIC_ENTRIES] =
    [0, 0x00AF9B000000FFFF, 0x00CF93000000FFFF];
const GUEST_GDT_LIMIT_VALUE: u64 = (GUEST_GDT_ENTRY_COUNT * 8 - 1) as u64;
const GUEST_IDTR_LIMIT_VALUE: u64 = 0;
const GUEST_CODE_SELECTOR: u16 = 1 << 3;
const GUEST_DATA_SELECTOR: u16 = 2 << 3;
const GUEST_TSS_SELECTOR: u16 = (GUEST_GDT_STATIC_ENTRIES as u16) << 3;
const GUEST_SEGMENT_LIMIT_VALUE: u64 = 0x000F_FFFF;
const GUEST_CS_AR_VALUE: u64 = 0xA09B;
const GUEST_DS_AR_VALUE: u64 = 0xC093;

// Linux boot protocol requires __BOOT_CS=0x10 and __BOOT_DS=0x18.
// We provide those selectors in the guest GDT when staging a Linux guest.
const LINUX_BOOT_CS_SELECTOR: u16 = 0x10;
const LINUX_BOOT_DS_SELECTOR: u16 = 0x18;
const LINUX_TSS_SELECTOR: u16 = 0x20;
const LINUX_GDT_STATIC_ENTRIES: usize = 4;
const LINUX_GDT_ENTRY_COUNT: usize = LINUX_GDT_STATIC_ENTRIES + 2; // TSS descriptor takes two slots
const LINUX_GDT_ENTRIES_64: [u64; LINUX_GDT_STATIC_ENTRIES] =
    [0, 0, 0x00AF9B000000FFFF, 0x00CF93000000FFFF];
const LINUX_GDT_LIMIT_VALUE: u64 = (LINUX_GDT_ENTRY_COUNT * 8 - 1) as u64;
const LINUX_CS_AR_VALUE: u64 = 0xA09B;
static mut GUEST_TSS_PHYS_VALUE: u64 = 0;
static mut GUEST_TSS_LIMIT_VALUE: u64 = 0;
static mut GUEST_TSS_AR_VALUE: u64 = 0;

// Bring-up paging fixups sometimes need to synthesize new paging structures inside
// guest RAM. Reserve a small region at the top of RAM (and mark it reserved in the
// E820 map) so Linux won't reuse/overwrite these pages.
const PF_FIXUP_PT_RESERVE_BYTES: u64 = 16 * 1024 * 1024;
static mut PF_FIXUP_PT_ALLOC_NEXT_GPA: u64 = 0;
const GUEST_TSS_PAGE_INDEX: usize = GUEST_IDT_PAGE_INDEX + 1;
const TSS_IO_MAP_BASE: usize = 0x68;
const TSS_IO_BITMAP_BYTES: usize = 8192;
const TSS_IO_BITMAP_TAIL_BYTES: usize = 1;

// Guest/host EFER fields (needed for IA-32e guests)
const IA32_EFER: u32 = 0xC000_0080;
const GUEST_IA32_EFER: u64 = 0x2806;
const HOST_IA32_EFER: u64 = 0x2C02;

// PAT MSR/fields (only required when VM-entry/exit controls request load/save PAT).
const IA32_PAT: u32 = 0x277;
const GUEST_IA32_PAT: u64 = 0x2804;
const HOST_IA32_PAT: u64 = 0x2C00;

// VM-exit control bits (Intel SDM)
const EXIT_CTL_HOST_ADDR_SPACE_SIZE: u32 = 1 << 9;
const EXIT_CTL_SAVE_IA32_PAT: u32 = 1 << 18;
const EXIT_CTL_LOAD_IA32_PAT: u32 = 1 << 19;
const EXIT_CTL_SAVE_IA32_EFER: u32 = 1 << 20;
const EXIT_CTL_LOAD_IA32_EFER: u32 = 1 << 21;

// VM-entry control bits (Intel SDM)
const ENTRY_CTL_IA32E_MODE_GUEST: u32 = 1 << 9;
const ENTRY_CTL_LOAD_IA32_PAT: u32 = 1 << 14;
const ENTRY_CTL_LOAD_IA32_EFER: u32 = 1 << 15;

#[inline(always)]
unsafe fn alloc_zeroed_page() -> Option<u64> {
    let p = crate::phys_alloc_page()?;
    let v = crate::phys_to_virt(p) as *mut u8;
    core::ptr::write_bytes(v, 0, 4096);
    Some(p)
}
fn guest_ram_gpa(page: usize) -> u64 {
    (page as u64) * (PAGE_SIZE as u64)
}

unsafe fn allocate_guest_ram_page(page: usize) -> Option<u64> {
    if page >= GUEST_RAM_PAGES {
        return None;
    }
    let phys = alloc_zeroed_page()?;
    GUEST_RAM_PHYS[page] = phys;
    Some(phys)
}

fn guest_ram_phys(page: usize) -> u64 {
    unsafe { GUEST_RAM_PHYS[page] }
}

fn prepare_guest_memory() -> bool {
    unsafe {
        if GUEST_RAM_INITIALIZED {
            return true;
        }
    }

    init_hypervisor_mmio();

    #[cfg(feature = "vmm_linux_guest")]
    {
        let k = crate::linux_kernel_ptr_and_len().map(|(_, len)| len).unwrap_or(0);
        let i = crate::linux_initrd_ptr_and_len().map(|(_, len)| len).unwrap_or(0);
        let c = crate::linux_cmdline_ptr_and_len().map(|(_, len)| len).unwrap_or(0);
        crate::serial_write_str("RAYOS_VMM:LINUX_BLOBS:KERNEL_SIZE=");
        crate::serial_write_hex_u64(k as u64);
        crate::serial_write_str(" INITRD_SIZE=");
        crate::serial_write_hex_u64(i as u64);
        crate::serial_write_str(" CMDLINE_SIZE=");
        crate::serial_write_hex_u64(c as u64);
        crate::serial_write_str("\n");
    }

    for page in 0..GUEST_RAM_PAGES {
        unsafe {
            if GUEST_RAM_PHYS[page] != 0 {
                continue;
            }
        }
        if unsafe { allocate_guest_ram_page(page) }.is_none() {
            crate::serial_write_str("RAYOS_VMM:VMX:GUEST_RAM_ALLOC_FAIL\n");
            return false;
        }
    }

    unsafe {
        build_guest_page_tables();
        install_guest_descriptor_tables();

        #[cfg(feature = "vmm_linux_guest")]
        {
            if !try_install_linux_guest() {
                install_guest_code();
            }
        }

        #[cfg(not(feature = "vmm_linux_guest"))]
        {
            install_guest_code();
        }
        GUEST_RAM_INITIALIZED = true;
    }

    true
}

unsafe fn build_guest_page_tables() {
    const PRESENT_RW: u64 = 0x3;

    let page_limit = GUEST_MAPPED_PAGES;

    let pml4_phys = guest_ram_phys(0);
    let pdpt_phys = guest_ram_phys(1);
    let pd_phys = guest_ram_phys(2);

    let pml4 = crate::phys_to_virt(pml4_phys) as *mut u64;
    let pdpt = crate::phys_to_virt(pdpt_phys) as *mut u64;
    let pd = crate::phys_to_virt(pd_phys) as *mut u64;

    core::ptr::write_volatile(
        pml4.add(0),
        (guest_ram_gpa(1) & 0xFFFF_FFFF_FFFF_F000) | PRESENT_RW,
    );
    core::ptr::write_volatile(
        pdpt.add(0),
        (guest_ram_gpa(2) & 0xFFFF_FFFF_FFFF_F000) | PRESENT_RW,
    );

    for pd_idx in 0..GUEST_EPT_PD_COUNT {
        let pt_page = GUEST_PT_PAGE_START + pd_idx;
        let pt_phys = guest_ram_phys(pt_page);
        let pt = crate::phys_to_virt(pt_phys) as *mut u64;
        core::ptr::write_volatile(
            pd.add(pd_idx),
            (guest_ram_gpa(pt_page) & 0xFFFF_FFFF_FFFF_F000) | PRESENT_RW,
        );

        let base_index = pd_idx * 512;
        for pt_idx in 0..512 {
            let page_index = base_index + pt_idx;
            if page_index >= page_limit {
                break;
            }
            let page_gpa = guest_ram_gpa(page_index);
            core::ptr::write_volatile(
                pt.add(pt_idx),
                (page_gpa & 0xFFFF_FFFF_FFFF_F000) | PRESENT_RW,
            );
        }
    }

    // Paging sanity probe: log the translation chain for 0x0010_0200.
    // In PAE mode this should be:
    //   CR3 -> PDPT[0] -> PD[0] -> PT[0x100] (page 0x100000)
    #[cfg(feature = "vmm_linux_guest")]
    {
        let pdpt0 = core::ptr::read_volatile(pdpt.add(0));
        let pd0 = core::ptr::read_volatile(pd.add(0));
        let pt0_phys = guest_ram_phys(GUEST_PT_PAGE_START);
        let pt0 = crate::phys_to_virt(pt0_phys) as *const u64;
        let pte = core::ptr::read_volatile(pt0.add(0x100));
        crate::serial_write_str("RAYOS_VMM:PAGING:PDPT0=0x");
        crate::serial_write_hex_u64(pdpt0);
        crate::serial_write_str(" PD0=0x");
        crate::serial_write_hex_u64(pd0);
        crate::serial_write_str(" PTE[0x100]=0x");
        crate::serial_write_hex_u64(pte);
        crate::serial_write_str("\n");
    }
}

unsafe fn install_guest_code() {
    let code_phys = guest_ram_phys(GUEST_CODE_PAGE_INDEX);
    let code_ptr = crate::phys_to_virt(code_phys) as *mut u8;
    let code_gpa = guest_ram_gpa(GUEST_CODE_PAGE_INDEX);

    let driver_len = GUEST_DRIVER_BINARY.len();
    ptr::copy_nonoverlapping(GUEST_DRIVER_BINARY.as_ptr(), code_ptr, driver_len);
    let desc_data_gpa = code_gpa + GUEST_DRIVER_DESC_DATA_OFFSET as u64;
    let patch_ptr = code_ptr.add(GUEST_DRIVER_DESC_DATA_PTR_OFFSET) as *mut u64;
    crate::serial_write_str("RAYOS_VMM:INSTALL_GUEST_CODE:PATCHING\n");
    crate::serial_write_str("  code_gpa=0x");
    crate::serial_write_hex_u64(code_gpa);
    crate::serial_write_str("\n");
    crate::serial_write_str("  desc_data_gpa=0x");
    crate::serial_write_hex_u64(desc_data_gpa);
    crate::serial_write_str("\n");
    crate::serial_write_str("  ptr_offset=0x");
    crate::serial_write_hex_u64(GUEST_DRIVER_DESC_DATA_PTR_OFFSET as u64);
    crate::serial_write_str("\n");
    // Dump 16 bytes from the descriptor data area in the code image for verification.
    let desc_src = code_ptr.add(GUEST_DRIVER_DESC_DATA_OFFSET) as *const u8;
    crate::serial_write_str("  desc_src_bytes=");
    for i in 0..16 {
        let b = core::ptr::read_volatile(desc_src.add(i));
        crate::serial_write_hex_u8(b);
        crate::serial_write_str(" ");
    }
    crate::serial_write_str("\n");
    ptr::write_unaligned(patch_ptr, desc_data_gpa);
    crate::serial_write_str("  wrote patch ptr -> ");
    crate::serial_write_hex_u64(core::ptr::read_unaligned(patch_ptr));
    crate::serial_write_str("\n");

    // Smoke-mode helper: if descriptor data appears non-zero, copy it into the
    // virtqueue descriptors area and submit the avail ring so we can exercise
    // virtio queue handling without requiring the guest to execute its rep movs.
    #[cfg(feature = "vmm_hypervisor_smoke")]
    {
        // The built-in guest-driver template is blk/net oriented; when we're
        // exposing virtio-gpu on the MMIO device ID, skip this generic submit
        // (virtio-gpu has a dedicated selftest path).
        let active_dev = VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed);
        if active_dev == VIRTIO_GPU_DEVICE_ID {
            return;
        }

        let mut d = [0u8; 16];
        if read_guest_bytes(desc_data_gpa, &mut d) {
            let any_nonzero = d.iter().any(|&b| b != 0);
            if any_nonzero {
                crate::serial_write_str("RAYOS_VMM:INSTALL_GUEST_CODE:SMOKE_COPY\n");
                let ok = write_guest_bytes(VIRTIO_QUEUE_DESC_GPA, &d);
                if ok {
                    crate::serial_write_str("RAYOS_VMM:INSTALL_GUEST_CODE:SMOKE_COPY_OK\n");
                    // avail.ring[0]=0
                    let _ = write_u16(VIRTIO_QUEUE_DRIVER_GPA + VIRTQ_AVAIL_ENTRY_SIZE * 0, 0);
                    // avail.idx = 1
                    let _ = write_u16(VIRTIO_QUEUE_DRIVER_GPA + VIRTQ_AVAIL_INDEX_OFFSET, 1);
                    // Ensure used.idx = 0
                    let _ = write_u16(VIRTIO_QUEUE_USED_GPA + VIRTQ_USED_IDX_OFFSET, 0);
                    crate::serial_write_str("RAYOS_VMM:INSTALL_GUEST_CODE:SMOKE_SUBMIT\n");
                    log_virtq_descriptors(VIRTIO_QUEUE_DESC_GPA, VIRTIO_QUEUE_SIZE_VALUE as u32);
                    log_virtq_avail(VIRTIO_QUEUE_DRIVER_GPA, VIRTIO_QUEUE_SIZE_VALUE as u32);
                    process_virtq_queue(
                        VIRTIO_QUEUE_DESC_GPA,
                        VIRTIO_QUEUE_DRIVER_GPA,
                        VIRTIO_QUEUE_USED_GPA,
                        VIRTIO_QUEUE_SIZE_VALUE as u32,
                        VIRTIO_QUEUE_READY_VALUE as u32,
                        0,
                    );
                } else {
                    crate::serial_write_str("RAYOS_VMM:INSTALL_GUEST_CODE:SMOKE_COPY_FAIL\n");
                }
            }
        } else {
            crate::serial_write_str("RAYOS_VMM:INSTALL_GUEST_CODE:SMOKE_READ_FAIL\n");
        }
    }
}

#[cfg(feature = "vmm_linux_guest")]
unsafe fn try_install_linux_guest() -> bool {
    // Linux/x86 boot protocol: https://www.kernel.org/doc/html/latest/arch/x86/boot.html
    const SETUP_SECTS_OFFSET: usize = 0x01F1;
    const HDRS_MAGIC_OFFSET: usize = 0x0202;
    const VERSION_OFFSET: usize = 0x0206;
    const TYPE_OF_LOADER_OFFSET: usize = 0x0210;
    const LOADFLAGS_OFFSET: usize = 0x0211;
    const RAMDISK_IMAGE_OFFSET: usize = 0x0218;
    const RAMDISK_SIZE_OFFSET: usize = 0x021C;
    const CMDLINE_PTR_OFFSET: usize = 0x0228;
    const INITRD_ADDR_MAX_OFFSET: usize = 0x022C;
    const XLOADFLAGS_OFFSET: usize = 0x0236;

    // boot_params e820 map (struct boot_params in bootparam.h)
    // Common offsets in the 4096-byte boot_params "zero page".
    const E820_ENTRIES_OFFSET: usize = 0x01E8;
    const E820_TABLE_OFFSET: usize = 0x02D0;
    const E820_ENTRY_SIZE: usize = 20; // u64 addr + u64 size + u32 type (packed)
    const E820_TYPE_RAM: u32 = 1;
    const E820_TYPE_RESERVED: u32 = 2;

    const BOOT_PARAMS_GPA: u64 = 0x0009_0000;
    const CMDLINE_GPA: u64 = 0x0009_A000;
    const KERNEL_LOAD_GPA: u64 = 0x0010_0000;

    let (kptr, klen) = match crate::linux_kernel_ptr_and_len() {
        Some(v) => v,
        None => return false,
    };
    let kernel = core::slice::from_raw_parts(kptr, klen);
    if kernel.len() < 0x0300 {
        crate::serial_write_str("RAYOS_VMM:LINUX:KERNEL_TOO_SMALL\n");
        return false;
    }
    if &kernel[HDRS_MAGIC_OFFSET..HDRS_MAGIC_OFFSET + 4] != b"HdrS" {
        crate::serial_write_str("RAYOS_VMM:LINUX:BAD_HDRS_MAGIC\n");
        return false;
    }
    let version = u16::from_le_bytes([kernel[VERSION_OFFSET], kernel[VERSION_OFFSET + 1]]);
    let loadflags = kernel[LOADFLAGS_OFFSET];
    let is_bzimage = version >= 0x0200 && (loadflags & 0x01) != 0;
    if !is_bzimage {
        crate::serial_write_str("RAYOS_VMM:LINUX:NOT_BZIMAGE\n");
        return false;
    }

    // Optional sanity: if xloadflags exists, warn if kernel doesn't advertise a 64-bit entry.
    if kernel.len() >= XLOADFLAGS_OFFSET + 2 {
        let xloadflags = u16::from_le_bytes([kernel[XLOADFLAGS_OFFSET], kernel[XLOADFLAGS_OFFSET + 1]]);
        if (xloadflags & 0x0001) == 0 {
            crate::serial_write_str("RAYOS_VMM:LINUX:WARN_XLF_KERNEL_64_MISSING\n");
        }
    }

    let mut setup_sects = kernel[SETUP_SECTS_OFFSET];
    if setup_sects == 0 {
        setup_sects = 4;
    }
    let setup_bytes = ((setup_sects as usize) + 1) * 512;
    if setup_bytes >= kernel.len() {
        crate::serial_write_str("RAYOS_VMM:LINUX:SETUP_SIZE_INVALID\n");
        return false;
    }

    // Load protected-mode payload at 0x100000.
    let payload = &kernel[setup_bytes..];
    if !write_guest_bytes(KERNEL_LOAD_GPA, payload) {
        crate::serial_write_str("RAYOS_VMM:LINUX:KERNEL_COPY_FAIL\n");
        return false;
    }

    // Ensure boot_params is zero.
    if !fill_descriptor_payload(BOOT_PARAMS_GPA, 4096, 0) {
        crate::serial_write_str("RAYOS_VMM:LINUX:BOOT_PARAMS_CLEAR_FAIL\n");
        return false;
    }

    // Populate boot_params by copying the setup area (boot sector + setup header)
    // into the boot_params page at the same offsets.
    // boot_params is 4096 bytes.
    let copy_len = core::cmp::min(4096usize, setup_bytes);
    if !write_guest_bytes(BOOT_PARAMS_GPA, &kernel[..copy_len]) {
        crate::serial_write_str("RAYOS_VMM:LINUX:BOOT_PARAMS_SETUP_COPY_FAIL\n");
        return false;
    }

    // Provide a minimal E820 map: low RAM, reserved hole, then high RAM.
    // This is required for many kernels to size/locate usable memory correctly.
    let write_e820_entry = |idx: usize, addr: u64, size: u64, typ: u32| -> bool {
        let mut entry = [0u8; E820_ENTRY_SIZE];
        entry[0..8].copy_from_slice(&addr.to_le_bytes());
        entry[8..16].copy_from_slice(&size.to_le_bytes());
        entry[16..20].copy_from_slice(&typ.to_le_bytes());
        write_guest_bytes(
            BOOT_PARAMS_GPA + (E820_TABLE_OFFSET as u64) + (idx as u64) * (E820_ENTRY_SIZE as u64),
            &entry,
        )
    };

    // Low RAM: 0..0x9F000 (leave BIOS/EBDA area alone).
    if !write_e820_entry(0, 0x0000_0000, 0x0009_F000, E820_TYPE_RAM) {
        crate::serial_write_str("RAYOS_VMM:LINUX:E820_WRITE_FAIL\n");
        return false;
    }
    // High RAM: 1MB..guest RAM top (minus a small reserved region for VMM fixups).
    let high_base = 0x0010_0000u64;
    let ram_top = GUEST_RAM_SIZE_BYTES as u64;
    let reserve_base = ram_top.saturating_sub(PF_FIXUP_PT_RESERVE_BYTES);
    let high_top = reserve_base;
    let high_size = high_top.saturating_sub(high_base);
    if !write_e820_entry(1, high_base, high_size, E820_TYPE_RAM) {
        crate::serial_write_str("RAYOS_VMM:LINUX:E820_WRITE_FAIL\n");
        return false;
    }
    // Reserved top-of-RAM region for bring-up paging fixups.
    if PF_FIXUP_PT_RESERVE_BYTES != 0 {
        if !write_e820_entry(2, reserve_base, PF_FIXUP_PT_RESERVE_BYTES, E820_TYPE_RESERVED) {
            crate::serial_write_str("RAYOS_VMM:LINUX:E820_WRITE_FAIL\n");
            return false;
        }
    }
    // e820_entries
    if !write_guest_bytes(BOOT_PARAMS_GPA + (E820_ENTRIES_OFFSET as u64), &[3u8]) {
        crate::serial_write_str("RAYOS_VMM:LINUX:E820_COUNT_WRITE_FAIL\n");
        return false;
    }

    // Command line (NUL-terminated; classic placement below 0xA0000).
    let mut cmdline_buf = [0u8; 1024];
    let cmdline_len = match crate::linux_cmdline_ptr_and_len() {
        Some((ptr, len)) => {
            let src = core::slice::from_raw_parts(ptr, len);
            let mut n = 0usize;
            for &b in src.iter() {
                if n + 1 >= cmdline_buf.len() {
                    break;
                }
                cmdline_buf[n] = b;
                n += 1;
            }
            while n > 0
                && (cmdline_buf[n - 1] == b'\n'
                    || cmdline_buf[n - 1] == b'\r'
                    || cmdline_buf[n - 1] == 0)
            {
                n -= 1;
            }

            // Disable KASLR for deterministic early paging layout. Our bring-up path
            // currently relies on predictable high-half virtual bases (physmap/kernel).
            // Keep this minimal: just append " nokaslr" when there is room.
            const NOKASLR: &[u8] = b" nokaslr";
            if n + 1 + NOKASLR.len() < cmdline_buf.len() {
                for &b in NOKASLR {
                    cmdline_buf[n] = b;
                    n += 1;
                }
            }

            // Helps identify exactly which initcall we hang in.
            const INITCALL_DEBUG: &[u8] = b" initcall_debug";
            if n + 1 + INITCALL_DEBUG.len() < cmdline_buf.len() {
                for &b in INITCALL_DEBUG {
                    cmdline_buf[n] = b;
                    n += 1;
                }
            }

            cmdline_buf[n] = 0;
            n + 1
        }
        None => {
            let s = b"auto\0";
            cmdline_buf[..s.len()].copy_from_slice(s);
            s.len()
        }
    };

    crate::serial_write_str("RAYOS_VMM:LINUX:CMDLINE_GPA=0x");
    crate::serial_write_hex_u64(CMDLINE_GPA);
    crate::serial_write_str(" len=0x");
    crate::serial_write_hex_u64(cmdline_len as u64);
    crate::serial_write_str(" text='");
    for &b in cmdline_buf.iter().take(cmdline_len) {
        if b == 0 {
            break;
        }
        crate::serial_write_byte(b);
    }
    crate::serial_write_str("'\n");

    if !write_guest_bytes(CMDLINE_GPA, &cmdline_buf[..cmdline_len]) {
        crate::serial_write_str("RAYOS_VMM:LINUX:CMDLINE_COPY_FAIL\n");
        return false;
    }
    if !write_u32(BOOT_PARAMS_GPA + (CMDLINE_PTR_OFFSET as u64), CMDLINE_GPA as u32) {
        crate::serial_write_str("RAYOS_VMM:LINUX:CMDLINE_PTR_WRITE_FAIL\n");
        return false;
    }

    if let Some(ptr) = read_u32(BOOT_PARAMS_GPA + (CMDLINE_PTR_OFFSET as u64)) {
        crate::serial_write_str("RAYOS_VMM:LINUX:CMDLINE_PTR_RD=0x");
        crate::serial_write_hex_u64(ptr as u64);
        crate::serial_write_str("\n");
    }

    // type_of_loader = 0xFF (undefined).
    if !write_guest_bytes(BOOT_PARAMS_GPA + (TYPE_OF_LOADER_OFFSET as u64), &[0xFF]) {
        crate::serial_write_str("RAYOS_VMM:LINUX:TYPE_OF_LOADER_WRITE_FAIL\n");
        return false;
    }

    // Initrd placement (optional).
    if let Some((iptr, ilen)) = crate::linux_initrd_ptr_and_len() {
        let initrd = core::slice::from_raw_parts(iptr, ilen);

        let initrd_addr_max = if version >= 0x0203 {
            u32::from_le_bytes([
                kernel[INITRD_ADDR_MAX_OFFSET],
                kernel[INITRD_ADDR_MAX_OFFSET + 1],
                kernel[INITRD_ADDR_MAX_OFFSET + 2],
                kernel[INITRD_ADDR_MAX_OFFSET + 3],
            ])
        } else {
            0x37FF_FFFFu32
        };

        let guest_max = (GUEST_RAM_SIZE_BYTES as u64).saturating_sub(1);
        let max_end = core::cmp::min(initrd_addr_max as u64, guest_max);
        let start_unaligned = (max_end + 1).saturating_sub(initrd.len() as u64);
        let initrd_gpa = start_unaligned & !0xFFFu64;

        if initrd_gpa < KERNEL_LOAD_GPA + 0x2000 {
            crate::serial_write_str("RAYOS_VMM:LINUX:INITRD_PLACEMENT_TOO_LOW\n");
            return false;
        }
        if !write_guest_bytes(initrd_gpa, initrd) {
            crate::serial_write_str("RAYOS_VMM:LINUX:INITRD_COPY_FAIL\n");
            return false;
        }
        if !write_u32(BOOT_PARAMS_GPA + (RAMDISK_IMAGE_OFFSET as u64), initrd_gpa as u32) {
            crate::serial_write_str("RAYOS_VMM:LINUX:RAMDISK_IMAGE_WRITE_FAIL\n");
            return false;
        }
        if !write_u32(BOOT_PARAMS_GPA + (RAMDISK_SIZE_OFFSET as u64), initrd.len() as u32) {
            crate::serial_write_str("RAYOS_VMM:LINUX:RAMDISK_SIZE_WRITE_FAIL\n");
            return false;
        }
    } else {
        let _ = write_u32(BOOT_PARAMS_GPA + (RAMDISK_IMAGE_OFFSET as u64), 0);
        let _ = write_u32(BOOT_PARAMS_GPA + (RAMDISK_SIZE_OFFSET as u64), 0);
    }

    // 64-bit boot protocol entry: loaded kernel start + 0x200.
    // See kernel.org boot protocol section "64-bit Boot Protocol".
    let entry_rip = KERNEL_LOAD_GPA + 0x200;
    LINUX_GUEST_ENTRY_RIP = entry_rip;
    LINUX_GUEST_BOOT_PARAMS_GPA = BOOT_PARAMS_GPA;

    seed_linux_guest_msrs();

    crate::serial_write_str("RAYOS_VMM:LINUX:READY entry=0x");
    crate::serial_write_hex_u64(entry_rip);
    crate::serial_write_str(" bp=0x");
    crate::serial_write_hex_u64(BOOT_PARAMS_GPA);
    crate::serial_write_str("\n");

    true
}

unsafe fn install_guest_descriptor_tables() {
    let gdt_phys = guest_ram_phys(GUEST_GDT_PAGE_INDEX);
    let gdt_dst = crate::phys_to_virt(gdt_phys) as *mut u64;
    #[cfg(feature = "vmm_linux_guest")]
    let linux_guest = crate::linux_kernel_ptr_and_len().is_some();
    #[cfg(not(feature = "vmm_linux_guest"))]
    let linux_guest = false;

    if linux_guest {
        for (idx, entry) in LINUX_GDT_ENTRIES_64.iter().enumerate() {
            core::ptr::write_volatile(gdt_dst.add(idx), *entry);
        }

        let tss_phys = guest_ram_phys(GUEST_TSS_PAGE_INDEX);
        let tss_gpa = guest_ram_gpa(GUEST_TSS_PAGE_INDEX);
        let tss_virt = crate::phys_to_virt(tss_phys) as *mut u8;
        core::ptr::write_bytes(tss_virt, 0, PAGE_SIZE);

        // Point RSP0 at the top of the guest stack (minus a small red zone for pushes).
        let guest_stack_top = guest_ram_gpa(GUEST_STACK_START_INDEX + GUEST_STACK_PAGES);
        let rsp0_ptr = unsafe { tss_virt.add(4) } as *mut u64;
        core::ptr::write_volatile(rsp0_ptr, guest_stack_top - 0x10);

        // Configure an I/O bitmap so guest OUTs do not #GP when nested in VMX.
        let io_map_base_ptr = unsafe { tss_virt.add(0x66) } as *mut u16;
        core::ptr::write_volatile(io_map_base_ptr, TSS_IO_MAP_BASE as u16);
        let io_bitmap_tail =
            unsafe { tss_virt.add(TSS_IO_MAP_BASE + TSS_IO_BITMAP_BYTES) } as *mut u8;
        core::ptr::write_volatile(io_bitmap_tail, 0xFF);

        let tss_limit =
            (TSS_IO_MAP_BASE + TSS_IO_BITMAP_BYTES + TSS_IO_BITMAP_TAIL_BYTES - 1) as u32;
        let (tss_desc_lo, tss_desc_hi, tss_ar) = make_tss_descriptor(tss_gpa, tss_limit);
        core::ptr::write_volatile(gdt_dst.add(LINUX_GDT_STATIC_ENTRIES), tss_desc_lo);
        core::ptr::write_volatile(gdt_dst.add(LINUX_GDT_STATIC_ENTRIES + 1), tss_desc_hi);

        let gdt_used = LINUX_GDT_ENTRY_COUNT * 8;
        if gdt_used < PAGE_SIZE {
            let tail = (gdt_dst as *mut u8).add(gdt_used);
            core::ptr::write_bytes(tail, 0, PAGE_SIZE - gdt_used);
        }

        let idt_phys = guest_ram_phys(GUEST_IDT_PAGE_INDEX);
        let idt_dst = crate::phys_to_virt(idt_phys) as *mut u8;
        core::ptr::write_bytes(idt_dst, 0, PAGE_SIZE);

        GUEST_TSS_PHYS_VALUE = tss_gpa;
        GUEST_TSS_LIMIT_VALUE = tss_limit as u64;
        GUEST_TSS_AR_VALUE = tss_ar;
        return;
    }

    for (idx, entry) in GUEST_GDT_ENTRIES.iter().enumerate() {
        core::ptr::write_volatile(gdt_dst.add(idx), *entry);
    }

    let tss_phys = guest_ram_phys(GUEST_TSS_PAGE_INDEX);
    let tss_gpa = guest_ram_gpa(GUEST_TSS_PAGE_INDEX);
    let tss_virt = crate::phys_to_virt(tss_phys) as *mut u8;
    core::ptr::write_bytes(tss_virt, 0, PAGE_SIZE);

    // Point RSP0 at the top of the guest stack (minus a small red zone for pushes).
    let guest_stack_top = guest_ram_gpa(GUEST_STACK_START_INDEX + GUEST_STACK_PAGES);
    let rsp0_ptr = unsafe { tss_virt.add(4) } as *mut u64;
    core::ptr::write_volatile(rsp0_ptr, guest_stack_top - 0x10);

    // Configure an I/O bitmap so guest OUTs do not #GP when nested in VMX.
    let io_map_base_ptr = unsafe { tss_virt.add(0x66) } as *mut u16;
    core::ptr::write_volatile(io_map_base_ptr, TSS_IO_MAP_BASE as u16);
    let io_bitmap_tail = unsafe { tss_virt.add(TSS_IO_MAP_BASE + TSS_IO_BITMAP_BYTES) } as *mut u8;
    core::ptr::write_volatile(io_bitmap_tail, 0xFF);

    let tss_limit = (TSS_IO_MAP_BASE + TSS_IO_BITMAP_BYTES + TSS_IO_BITMAP_TAIL_BYTES - 1) as u32;
    let (tss_desc_lo, tss_desc_hi, tss_ar) = make_tss_descriptor(tss_gpa, tss_limit);
    core::ptr::write_volatile(gdt_dst.add(GUEST_GDT_STATIC_ENTRIES), tss_desc_lo);
    core::ptr::write_volatile(gdt_dst.add(GUEST_GDT_STATIC_ENTRIES + 1), tss_desc_hi);

    let gdt_used = GUEST_GDT_ENTRY_COUNT * 8;
    if gdt_used < PAGE_SIZE {
        let tail = (gdt_dst as *mut u8).add(gdt_used);
        core::ptr::write_bytes(tail, 0, PAGE_SIZE - gdt_used);
    }

    let idt_phys = guest_ram_phys(GUEST_IDT_PAGE_INDEX);
    let idt_dst = crate::phys_to_virt(idt_phys) as *mut u8;
    core::ptr::write_bytes(idt_dst, 0, PAGE_SIZE);

    GUEST_TSS_PHYS_VALUE = tss_gpa;
    GUEST_TSS_LIMIT_VALUE = tss_limit as u64;
    GUEST_TSS_AR_VALUE = tss_ar;

    let idt_phys = guest_ram_phys(GUEST_IDT_PAGE_INDEX);
    let idt_dst = crate::phys_to_virt(idt_phys) as *mut u8;
    core::ptr::write_bytes(idt_dst, 0, PAGE_SIZE);
}

fn make_tss_descriptor(base: u64, limit: u32) -> (u64, u64, u64) {
    let limit_low = (limit & 0xFFFF) as u64;
    let limit_high = ((limit >> 16) & 0xF) as u64;
    let base_low = base & 0xFFFF;
    let base_mid = (base >> 16) & 0xFF;
    let base_high = (base >> 24) & 0xFF;
    let access = 0x8Bu64; // present, type=0xB (busy 64-bit TSS)
    let flags = 0u64; // gran=0, db=0, l=0, avl=0
    let descriptor_low = limit_low
        | (base_low << 16)
        | (base_mid << 32)
        | (access << 40)
        | ((limit_high & 0xF) << 48)
        | (flags << 52)
        | (base_high << 56);
    let descriptor_high = (base >> 32) & 0xFFFF_FFFF;
    let ar_bytes = access | ((limit_high) << 8) | ((flags) << 12);
    (descriptor_low, descriptor_high, ar_bytes)
}

const EPT_ENTRY_FLAGS: u64 = 0b111;
const EPT_MEMTYPE_UC: u64 = 0;
const EPT_MEMTYPE_WB: u64 = 6;

#[repr(C, packed)]
struct InveptDescriptor {
    eptp: u64,
    _rsvd: u64,
}

#[inline(always)]
unsafe fn invept_all_contexts() -> bool {
    // Type 2: invalidate all EPT contexts.
    let desc = InveptDescriptor { eptp: 0, _rsvd: 0 };
    let inv_type: u64 = 2;
    let rflags: u64;
    asm!(
        "invept {0}, [{1}]\n\
         pushfq\n\
         pop {2}",
        in(reg) inv_type,
        in(reg) &desc,
        out(reg) rflags,
        options(nostack)
    );
    (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0
}

unsafe fn ept_map_4k_page(eptp: u64, gpa: u64, host_phys: u64, memtype: u64) -> bool {
    let pml4_phys = eptp & 0xFFFF_FFFF_FFFF_F000;
    if pml4_phys == 0 {
        return false;
    }

    let pml4_idx = ((gpa >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((gpa >> 30) & 0x1FF) as usize;
    let pd_idx = ((gpa >> 21) & 0x1FF) as usize;
    let pt_idx = ((gpa >> 12) & 0x1FF) as usize;

    let pml4_v = crate::phys_to_virt(pml4_phys) as *mut u64;
    let pml4e = core::ptr::read_volatile(pml4_v.add(pml4_idx));
    if (pml4e & EPT_ENTRY_FLAGS) == 0 {
        return false;
    }
    let pdpt_phys = pml4e & 0xFFFF_FFFF_FFFF_F000;
    if pdpt_phys == 0 {
        return false;
    }

    let pdpt_v = crate::phys_to_virt(pdpt_phys) as *mut u64;
    let mut pdpte = core::ptr::read_volatile(pdpt_v.add(pdpt_idx));
    if (pdpte & EPT_ENTRY_FLAGS) == 0 {
        let new_pd = match alloc_zeroed_page() {
            Some(p) => p,
            None => return false,
        };
        pdpte = (new_pd & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS;
        core::ptr::write_volatile(pdpt_v.add(pdpt_idx), pdpte);
    }
    let pd_phys = pdpte & 0xFFFF_FFFF_FFFF_F000;
    if pd_phys == 0 {
        return false;
    }

    let pd_v = crate::phys_to_virt(pd_phys) as *mut u64;
    let mut pde = core::ptr::read_volatile(pd_v.add(pd_idx));
    if (pde & EPT_ENTRY_FLAGS) == 0 {
        let new_pt = match alloc_zeroed_page() {
            Some(p) => p,
            None => return false,
        };
        pde = (new_pt & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS;
        core::ptr::write_volatile(pd_v.add(pd_idx), pde);
    }
    let pt_phys = pde & 0xFFFF_FFFF_FFFF_F000;
    if pt_phys == 0 {
        return false;
    }

    let pt_v = crate::phys_to_virt(pt_phys) as *mut u64;
    let entry = (host_phys & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS | ((memtype & 0x7) << 3);
    core::ptr::write_volatile(pt_v.add(pt_idx), entry);

    let _ = invept_all_contexts();
    true
}

unsafe fn build_guest_ram_ept() -> Option<u64> {
    let pml4 = alloc_zeroed_page()?;
    let pdpt = alloc_zeroed_page()?;
    let pd = alloc_zeroed_page()?;
    let mut pt_phys = [0u64; GUEST_EPT_PD_COUNT];
    for entry in pt_phys.iter_mut() {
        *entry = alloc_zeroed_page()?;
    }

    let pml4_v = crate::phys_to_virt(pml4) as *mut u64;
    let pdpt_v = crate::phys_to_virt(pdpt) as *mut u64;
    let pd_v = crate::phys_to_virt(pd) as *mut u64;

    core::ptr::write_volatile(
        pdpt_v.add(0),
        (pd & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS,
    );
    core::ptr::write_volatile(
        pml4_v.add(0),
        (pdpt & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS,
    );

    for pd_idx in 0..GUEST_EPT_PD_COUNT {
        let pt = crate::phys_to_virt(pt_phys[pd_idx]) as *mut u64;
        core::ptr::write_volatile(
            pd_v.add(pd_idx),
            (pt_phys[pd_idx] & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS,
        );

        let base_index = pd_idx * 512;
        for pt_idx in 0..512 {
            let page_index = base_index + pt_idx;
            if page_index >= GUEST_RAM_PAGES {
                break;
            }
            let page_phys = guest_ram_phys(page_index);
            let entry =
                (page_phys & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS | (EPT_MEMTYPE_WB << 3);
            core::ptr::write_volatile(pt.add(pt_idx), entry);
        }
    }

    Some((pml4 & 0xFFFF_FFFF_FFFF_F000) | (EPT_MEMTYPE_WB) | (3 << 3))
}

// Host-state fields (subset)
const HOST_CR0: u64 = 0x6C00;
const HOST_CR3: u64 = 0x6C02;
const HOST_CR4: u64 = 0x6C04;
const HOST_RSP: u64 = 0x6C14;
const HOST_RIP: u64 = 0x6C16;

const HOST_ES_SELECTOR: u64 = 0x0C00;
const HOST_CS_SELECTOR: u64 = 0x0C02;
const HOST_SS_SELECTOR: u64 = 0x0C04;
const HOST_DS_SELECTOR: u64 = 0x0C06;
const HOST_FS_SELECTOR: u64 = 0x0C08;
const HOST_GS_SELECTOR: u64 = 0x0C0A;
const HOST_TR_SELECTOR: u64 = 0x0C0C;

const HOST_FS_BASE: u64 = 0x6C06;
const HOST_GS_BASE: u64 = 0x6C08;
const HOST_TR_BASE: u64 = 0x6C0A;
const HOST_GDTR_BASE: u64 = 0x6C0C;
const HOST_IDTR_BASE: u64 = 0x6C0E;

// Sysenter MSRs/fields (optional but commonly required by VMX checks)
const IA32_SYSENTER_CS: u32 = 0x174;
const IA32_SYSENTER_ESP: u32 = 0x175;
const IA32_SYSENTER_EIP: u32 = 0x176;
const HOST_IA32_SYSENTER_CS: u64 = 0x4C00;
const HOST_IA32_SYSENTER_ESP: u64 = 0x6C10;
const HOST_IA32_SYSENTER_EIP: u64 = 0x6C12;

// FS/GS base MSRs
const IA32_FS_BASE: u32 = 0xC000_0100;
const IA32_GS_BASE: u32 = 0xC000_0101;
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

// Syscall MSRs (x86_64)
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_CSTAR: u32 = 0xC000_0083;
const IA32_FMASK: u32 = 0xC000_0084;

// APIC base MSR (commonly touched during early boot)
const IA32_APIC_BASE: u32 = 0x1B;

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct SegDesc {
    limit0: u16,
    base0: u16,
    base1: u8,
    access: u8,
    gran: u8,
    base2: u8,
}

// VMX capability MSRs
const IA32_VMX_PINBASED_CTLS: u32 = 0x481;
const IA32_VMX_PROCBASED_CTLS: u32 = 0x482;
const IA32_VMX_EXIT_CTLS: u32 = 0x483;
const IA32_VMX_ENTRY_CTLS: u32 = 0x484;
const IA32_VMX_PROCBASED_CTLS2: u32 = 0x48B;

const IA32_VMX_TRUE_PINBASED_CTLS: u32 = 0x48D;
const IA32_VMX_TRUE_PROCBASED_CTLS: u32 = 0x48E;
const IA32_VMX_TRUE_EXIT_CTLS: u32 = 0x48F;
const IA32_VMX_TRUE_ENTRY_CTLS: u32 = 0x490;

#[repr(C, packed)]
struct DtPtr {
    limit: u16,
    base: u64,
}

const VMX_STACK_SIZE: usize = 16 * 1024;
#[repr(align(16))]
struct VmxStack([u8; VMX_STACK_SIZE]);
static mut VMX_HOST_STACK: VmxStack = VmxStack([0; VMX_STACK_SIZE]);

static VMEXIT_COUNT: AtomicUsize = AtomicUsize::new(0);
static PFEXIT_COUNT: AtomicUsize = AtomicUsize::new(0);
static LOW_IDENTITY_PREFILL_DONE: AtomicUsize = AtomicUsize::new(0);
static PHYSMAP_PREFILL_DONE: AtomicUsize = AtomicUsize::new(0);
static PHYSMAP_PML4E_FRESH_LOGGED: AtomicUsize = AtomicUsize::new(0);
static GUEST_COM1_TX_BYTES: AtomicUsize = AtomicUsize::new(0);
static HLTEXIT_COUNT: AtomicUsize = AtomicUsize::new(0);
static PREEMPTEXIT_COUNT: AtomicUsize = AtomicUsize::new(0);

static mut GUEST_FAKE_LAPIC_PAGE_PHYS: u64 = 0;

const PAGE_SIZE: usize = 4096;
#[cfg(feature = "vmm_linux_guest")]
const GUEST_RAM_SIZE_MB: usize = 256;

#[cfg(not(feature = "vmm_linux_guest"))]
const GUEST_RAM_SIZE_MB: usize = 16;
const GUEST_RAM_SIZE_BYTES: usize = GUEST_RAM_SIZE_MB * 1024 * 1024;
const GUEST_RAM_PAGES: usize = GUEST_RAM_SIZE_BYTES / PAGE_SIZE;

// Linux x86_64 direct-map (physmap) base in the common 4-level paging layout.
// Bring-up aid: used only for heuristic page-table fixups.
const LINUX_X86_64_PHYSMAP_BASE: u64 = 0xFFFF_8880_0000_0000;

#[inline(always)]
fn linux_physmap_pa_for_va(va: u64) -> Option<u64> {
    if va < LINUX_X86_64_PHYSMAP_BASE {
        return None;
    }
    Some(va.wrapping_sub(LINUX_X86_64_PHYSMAP_BASE))
}
const MMIO_COUNTER_BASE: u64 = GUEST_RAM_SIZE_BYTES as u64;
const MMIO_COUNTER_SIZE: u64 = PAGE_SIZE as u64;
const MMIO_VIRTIO_BASE: u64 = MMIO_COUNTER_BASE + MMIO_COUNTER_SIZE;
const MMIO_VIRTIO_SIZE: u64 = PAGE_SIZE as u64;
const GUEST_MMIO_TOTAL_SIZE: u64 = MMIO_COUNTER_SIZE + MMIO_VIRTIO_SIZE;
const GUEST_MMIO_PAGE_COUNT: usize = (GUEST_MMIO_TOTAL_SIZE as usize + PAGE_SIZE - 1) / PAGE_SIZE;
const GUEST_MAPPED_PAGES: usize = GUEST_RAM_PAGES + GUEST_MMIO_PAGE_COUNT;
const VIRTIO_MMIO_MAGIC_VALUE_OFFSET: u64 = 0x000;
const VIRTIO_MMIO_VERSION_OFFSET: u64 = 0x004;
const VIRTIO_MMIO_DEVICE_ID_OFFSET: u64 = 0x008;
const VIRTIO_MMIO_VENDOR_ID_OFFSET: u64 = 0x00C;
const VIRTIO_MMIO_DEVICE_FEATURES_OFFSET: u64 = 0x010;
const VIRTIO_MMIO_DEVICE_FEATURES_SEL_OFFSET: u64 = 0x014;
const VIRTIO_MMIO_DRIVER_FEATURES_OFFSET: u64 = 0x020;
const VIRTIO_MMIO_DRIVER_FEATURES_SEL_OFFSET: u64 = 0x024;
const VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET: u64 = 0x060;
const VIRTIO_MMIO_INTERRUPT_ACK_OFFSET: u64 = 0x064;
const VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET: u64 = 0x050;
const VIRTIO_MMIO_QUEUE_SELECT_OFFSET: u64 = 0x030;
const VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET: u64 = 0x034;
const VIRTIO_MMIO_QUEUE_NUM_OFFSET: u64 = 0x038;
const VIRTIO_MMIO_QUEUE_READY_OFFSET: u64 = 0x044;
const VIRTIO_MMIO_STATUS_OFFSET: u64 = 0x070;
const VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET: u64 = 0x080;
const VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET: u64 = 0x084;
// Some guest test blobs use 64-bit writes to 0x080 for the whole QueueDesc.
const VIRTIO_MMIO_QUEUE_DESC_OFFSET: u64 = VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET;

const VIRTIO_MMIO_QUEUE_AVAIL_LOW_OFFSET: u64 = 0x090;
const VIRTIO_MMIO_QUEUE_AVAIL_HIGH_OFFSET: u64 = 0x094;
// Legacy alias used by older guest blobs in this repo.
const VIRTIO_MMIO_QUEUE_DRIVER_OFFSET: u64 = 0x088;

const VIRTIO_MMIO_QUEUE_USED_LOW_OFFSET: u64 = 0x0A0;
const VIRTIO_MMIO_QUEUE_USED_HIGH_OFFSET: u64 = 0x0A4;

// Legacy alias used by older guest blobs in this repo.
const VIRTIO_MMIO_QUEUE_SIZE_OFFSET: u64 = 0x098;
const VIRTIO_MMIO_CONFIG_SPACE_OFFSET: u64 = 0x100;
const VIRTIO_MMIO_FEATURES_VALUE: u32 = 1;
const VIRTIO_QUEUE_DESC_GPA: u64 = 0x0010_0000;
const VIRTIO_QUEUE_DRIVER_GPA: u64 = VIRTIO_QUEUE_DESC_GPA + 0x1000;
const VIRTIO_QUEUE_USED_GPA: u64 = VIRTIO_QUEUE_DRIVER_GPA + 0x1000;
const VIRTIO_QUEUE_SIZE_VALUE: u64 = 8;
const VIRTIO_QUEUE_READY_VALUE: u64 = 1;
const VIRTIO_BLK_REQ_GPA: u64 = 0x0010_4000;
const VIRTIO_BLK_DATA_GPA: u64 = VIRTIO_BLK_REQ_GPA + 0x1000;
const VIRTIO_BLK_STATUS_GPA: u64 = VIRTIO_BLK_DATA_GPA + 0x1000;
const VIRTIO_BLK_DATA_LEN: u32 = 512;
const VIRTIO_BLK_REQ_LEN: u32 = 16;
const VIRTIO_BLK_STATUS_LEN: u32 = 1;
const VIRTQ_DESC_SIZE: u64 = 16;
const MAX_VIRTQ_DESC_TO_LOG: u32 = 4;
const MAX_DESC_PAYLOAD_LOG_BYTES: usize = 16;
const DESC_RESPONSE_BYTE: u8 = 0x42;
const MAX_VIRTQ_DESC_CHAIN_ENTRIES: usize = 8;
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;
const MAX_VIRTIO_DATA_DESCS: usize = 4;
const VIRTIO_STATUS_OK: u8 = 0;
const VIRTIO_BLK_READ_PATTERN: u8 = 0xA5;
const VIRTIO_BLK_IDENTITY: &[u8; 16] = b"RAYOS VIRTIO BLK";

const VIRTIO_BLK_SECTOR_SIZE: usize = 512;
const VIRTIO_BLK_DISK_SECTORS: usize = 128;
const VIRTIO_BLK_DISK_BYTES: usize = VIRTIO_BLK_SECTOR_SIZE * VIRTIO_BLK_DISK_SECTORS;

static mut VIRTIO_BLK_DISK: [u8; VIRTIO_BLK_DISK_BYTES] = [0; VIRTIO_BLK_DISK_BYTES];
static mut VIRTIO_BLK_DISK_INITIALIZED: bool = false;

#[cfg(feature = "vmm_virtio_blk_image")]
static VIRTIO_BLK_DISK_IMAGE: &[u8] = include_bytes!("../assets/vmm_disk.img");
const VIRTQ_AVAIL_FLAGS_OFFSET: u64 = 0;
const VIRTQ_AVAIL_INDEX_OFFSET: u64 = 2;
const VIRTQ_AVAIL_RING_OFFSET: u64 = 4;
const VIRTQ_AVAIL_ENTRY_SIZE: u64 = 2;
const VIRTQ_USED_FLAGS_OFFSET: u64 = 0;
const VIRTQ_USED_IDX_OFFSET: u64 = 2;
const VIRTQ_USED_RING_OFFSET: u64 = 4;
const VIRTQ_USED_ENTRY_SIZE: u64 = 8;
const VIRTIO_MMIO_MAGIC_VALUE: u32 = 0x7472_6976;
const VIRTIO_MMIO_VERSION_VALUE: u32 = 2;
const VIRTIO_MMIO_DEVICE_ID_VALUE: u32 = 0x0105;
const VIRTIO_MMIO_VENDOR_ID_VALUE: u32 = 0x1AF4;
const VIRTIO_MMIO_INT_VRING: u32 = 1;

const VIRTIO_NET_DEVICE_ID: u32 = 0x0101;
// Standard virtio device ID for virtio-gpu.
const VIRTIO_GPU_DEVICE_ID: u32 = 16;
// Standard virtio device ID for virtio-input.
const VIRTIO_INPUT_DEVICE_ID: u32 = 18;
// Standard virtio device ID for virtio-console (P1)
const VIRTIO_CONSOLE_DEVICE_ID: u32 = 0x0107;

// Feature-gated device model instance for virtio-gpu.
//
// Safety: accessed only from the single-core hypervisor path today.
#[cfg(feature = "vmm_virtio_gpu")]
static mut VIRTIO_GPU_DEVICE: VirtioGpuDevice = VirtioGpuDevice::new();
const VIRTIO_NET_TX_QUEUE: u16 = 0;
const VIRTIO_NET_RX_QUEUE: u16 = 1;
const VIRTIO_NET_PKT_MAX: usize = 2048;
const VIRTIO_NET_MAC: &[u8; 6] = b"\x52\x55\x4F\x53\x00\x01"; // "RAYOS\0\1" (RAYOS in hex)

const VIRTIO_NET_RX_QUEUE_RING_OFFSET: u64 = 0x0010_8000;
const VIRTIO_NET_RX_QUEUE_USED_OFFSET: u64 = 0x0010_9000;

static mut VIRTIO_NET_TX_PACKETS: u32 = 0;
static mut VIRTIO_NET_RX_PACKETS: u32 = 0;
static mut VIRTIO_NET_LOOPBACK_ENABLED: bool = true;

// Loopback packet buffer: stores most recent TX packet for RX injection
static mut VIRTIO_NET_LOOPBACK_PKT: [u8; VIRTIO_NET_PKT_MAX] = [0u8; VIRTIO_NET_PKT_MAX];
static mut VIRTIO_NET_LOOPBACK_PKT_LEN: usize = 0;
const GUEST_EPT_PD_COUNT: usize = (GUEST_MAPPED_PAGES + 512 - 1) / 512;
const GUEST_PT_PAGE_START: usize = 3;
const GUEST_PAGE_TABLE_PAGES: usize = GUEST_PT_PAGE_START + GUEST_EPT_PD_COUNT;
const GUEST_DATA_PAGE_START: usize = GUEST_PAGE_TABLE_PAGES;
const GUEST_CODE_PAGE_INDEX: usize = GUEST_DATA_PAGE_START;
const GUEST_STACK_PAGES: usize = 4;
const GUEST_STACK_START_INDEX: usize = GUEST_CODE_PAGE_INDEX + 1;
const GUEST_STACK_END_INDEX: usize = GUEST_STACK_START_INDEX + GUEST_STACK_PAGES;
const GUEST_GDT_PAGE_INDEX: usize = GUEST_STACK_END_INDEX;
const GUEST_IDT_PAGE_INDEX: usize = GUEST_GDT_PAGE_INDEX + 1;

static mut GUEST_RAM_PHYS: [u64; GUEST_RAM_PAGES] = [0; GUEST_RAM_PAGES];
static mut GUEST_RAM_INITIALIZED: bool = false;

#[cfg(feature = "vmm_linux_guest")]
static mut LINUX_GUEST_ENTRY_RIP: u64 = 0;

#[cfg(feature = "vmm_linux_guest")]
static mut LINUX_GUEST_BOOT_PARAMS_GPA: u64 = 0;

static mut IO_BITMAP_A_PHYS: u64 = 0;
static mut IO_BITMAP_B_PHYS: u64 = 0;
static mut IO_BITMAPS_READY: bool = false;

static mut MSR_BITMAPS_PHYS: u64 = 0;
static mut MSR_BITMAPS_READY: bool = false;

const GUEST_MSR_STORE_CAPACITY: usize = 64;
static mut GUEST_MSR_KEYS: [u32; GUEST_MSR_STORE_CAPACITY] = [0; GUEST_MSR_STORE_CAPACITY];
static mut GUEST_MSR_VALUES: [u64; GUEST_MSR_STORE_CAPACITY] = [0; GUEST_MSR_STORE_CAPACITY];
static mut GUEST_MSR_COUNT: usize = 0;

#[inline(always)]
unsafe fn seed_linux_guest_msrs() {
    // IA32_APIC_BASE (0x1B): base 0xFEE0_0000, BSP=1, APIC global enable=1.
    // Many kernels expect something like 0xFEE00900 on reset.
    guest_msr_set(IA32_APIC_BASE, 0xFEE0_0000u64 | 0x900u64);
}

#[inline(always)]
unsafe fn guest_msr_get(msr: u32) -> Option<u64> {
    let count = GUEST_MSR_COUNT;
    let mut i = 0;
    while i < count {
        if GUEST_MSR_KEYS[i] == msr {
            return Some(GUEST_MSR_VALUES[i]);
        }
        i += 1;
    }
    None
}

#[inline(always)]
unsafe fn guest_msr_set(msr: u32, value: u64) {
    let count = GUEST_MSR_COUNT;
    let mut i = 0;
    while i < count {
        if GUEST_MSR_KEYS[i] == msr {
            GUEST_MSR_VALUES[i] = value;
            return;
        }
        i += 1;
    }
    if count < GUEST_MSR_STORE_CAPACITY {
        GUEST_MSR_KEYS[count] = msr;
        GUEST_MSR_VALUES[count] = value;
        GUEST_MSR_COUNT = count + 1;
    }
}

const COM1_BASE_PORT: u16 = 0x03F8;
const COM1_PORT_COUNT: u16 = 8;

struct Uart16550 {
    dll: u8,
    dlm: u8,
    ier: u8,
    fcr: u8,
    lcr: u8,
    mcr: u8,
    scr: u8,
}

static mut COM1_UART: Uart16550 = Uart16550 {
    // Default divisor 1 => 115200 baud (common default).
    dll: 1,
    dlm: 0,
    ier: 0,
    fcr: 0,
    lcr: 0,
    mcr: 0,
    scr: 0,
};

#[inline(always)]
fn com1_is_port(port: u16) -> bool {
    port >= COM1_BASE_PORT && port < COM1_BASE_PORT.wrapping_add(COM1_PORT_COUNT)
}

#[inline(always)]
unsafe fn com1_uart_in(offset: u16) -> u8 {
    // Minimal 16550 behavior: no RX, no interrupts.
    // Offsets: 0 RBR/THR/DLL, 1 IER/DLM, 2 IIR/FCR, 3 LCR, 4 MCR, 5 LSR, 6 MSR, 7 SCR
    let dlab = (COM1_UART.lcr & 0x80) != 0;
    match offset {
        0 => {
            if dlab {
                COM1_UART.dll
            } else {
                0 // RBR: no received data
            }
        }
        1 => {
            if dlab {
                COM1_UART.dlm
            } else {
                COM1_UART.ier
            }
        }
        2 => 0x01, // IIR: no interrupt pending
        3 => COM1_UART.lcr,
        4 => COM1_UART.mcr,
        5 => 0x60, // LSR: THR empty | TEMT
        6 => 0xB0, // MSR: CTS|DSR|DCD asserted (best-effort)
        7 => COM1_UART.scr,
        _ => 0,
    }
}

#[inline(always)]
unsafe fn com1_uart_out(offset: u16, value: u8) {
    let dlab = (COM1_UART.lcr & 0x80) != 0;
    match offset {
        0 => {
            if dlab {
                COM1_UART.dll = value;
            } else {
                // THR write: emit raw byte to host serial.
                let n = GUEST_COM1_TX_BYTES.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 {
                    crate::serial_write_str("RAYOS_VMM:COM1:TX_FIRST\n");
                }
                crate::serial_write_byte(value);
            }
        }
        1 => {
            if dlab {
                COM1_UART.dlm = value;
            } else {
                COM1_UART.ier = value;
            }
        }
        2 => {
            // FCR write (we ignore FIFO behavior, but store it).
            COM1_UART.fcr = value;
        }
        3 => COM1_UART.lcr = value,
        4 => COM1_UART.mcr = value,
        7 => COM1_UART.scr = value,
        _ => {}
    }
}

#[repr(C)]
struct GuestRegs {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rbp: u64,
    rsi: u64,
    rdi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

#[derive(Copy, Clone, PartialEq)]
enum MmioAccessKind {
    Read,
    Write,
}

#[derive(Copy, Clone)]
enum MmioRegister {
    Rax,
}

struct MmioAccess {
    offset: u64,
    size: usize,
    kind: MmioAccessKind,
    reg: MmioRegister,
}

#[derive(Copy, Clone)]
struct MmioInstruction {
    kind: MmioAccessKind,
    size: usize,
    address: u64,
}

type MmioHandler = fn(&mut GuestRegs, &MmioAccess, Option<u64>) -> Option<u64>;

#[derive(Copy, Clone)]
struct MmioRegion {
    base: u64,
    size: u64,
    handler: MmioHandler,
}

const MAX_MMIO_REGIONS: usize = 4;
static mut MMIO_REGIONS: [Option<MmioRegion>; MAX_MMIO_REGIONS] = [None; MAX_MMIO_REGIONS];
static mut MMIO_REGIONS_INITIALIZED: bool = false;
static MMIO_COUNTER: AtomicU64 = AtomicU64::new(0);

struct VirtioMmioState {
    status: AtomicU32,
    // Driver-selected feature bits (two 32-bit words selected by DRIVER_FEATURES_SEL).
    driver_features: [AtomicU32; 2],
    driver_features_sel: AtomicU32,
    // Device feature select (DEVICE_FEATURES_SEL). Device features themselves are computed
    // from the active device_id.
    device_features_sel: AtomicU32,
    interrupt_status: AtomicU32,
    // Pending retry flag: set to 1 when VM-entry injection fails and retries are needed.
    // Retry counter tracks how many retry attempts we've made.
    interrupt_pending: AtomicU32,
    interrupt_pending_attempts: AtomicU32,
    // Last retry tick (TIMER_TICKS) used for exponential backoff scheduling.
    interrupt_pending_last_tick: AtomicU64,
    // Metrics: counters for diagnostics and observability.
    interrupt_backoff_total_attempts: AtomicU32,
    interrupt_backoff_succeeded: AtomicU32,
    interrupt_backoff_failed_max: AtomicU32,
    device_id: AtomicU32,
    queue_notify_count: AtomicU32,
    queue_desc_address: [AtomicU64; 2],
    queue_driver_address: [AtomicU64; 2],
    queue_used_address: [AtomicU64; 2],
    queue_avail_index: [AtomicU16; 2],
    queue_used_index: [AtomicU16; 2],
    queue_size: [AtomicU32; 2],
    queue_ready: [AtomicU32; 2],
    queue_selected: AtomicU32,

    // virtio-input config selection state (select/subsel fields).
    // Linux-style virtio-input queries are modeled as reads from the MMIO config space
    // after the guest writes select/subsel.
    #[cfg(feature = "vmm_virtio_input")]
    input_cfg_select: AtomicU32,
    #[cfg(feature = "vmm_virtio_input")]
    input_cfg_subsel: AtomicU32,
}

impl VirtioMmioState {
    const fn new() -> Self {
        // Feature priority: if we're building the virtio-gpu device model, expose it.
        // Otherwise preserve existing net-test and default behaviors.
        #[cfg(feature = "vmm_virtio_gpu")]
        let device_id = VIRTIO_GPU_DEVICE_ID;
        #[cfg(all(not(feature = "vmm_virtio_gpu"), feature = "vmm_virtio_input"))]
        let device_id = VIRTIO_INPUT_DEVICE_ID;
        #[cfg(all(not(feature = "vmm_virtio_gpu"), feature = "vmm_virtio_console"))]
        let device_id = VIRTIO_CONSOLE_DEVICE_ID;
        #[cfg(all(
            not(feature = "vmm_virtio_gpu"),
            not(feature = "vmm_virtio_input"),
            not(feature = "vmm_virtio_console"),
            feature = "vmm_hypervisor_net_test"
        ))]
        let device_id = VIRTIO_NET_DEVICE_ID;
        #[cfg(all(
            not(feature = "vmm_virtio_gpu"),
            not(feature = "vmm_virtio_input"),
            not(feature = "vmm_virtio_console"),
            not(feature = "vmm_hypervisor_net_test")
        ))]
        let device_id = VIRTIO_MMIO_DEVICE_ID_VALUE;

        Self {
            status: AtomicU32::new(0),
            driver_features: [AtomicU32::new(0), AtomicU32::new(0)],
            driver_features_sel: AtomicU32::new(0),
            device_features_sel: AtomicU32::new(0),
            interrupt_status: AtomicU32::new(0),
            interrupt_pending: AtomicU32::new(0),
            interrupt_pending_attempts: AtomicU32::new(0),
            interrupt_pending_last_tick: AtomicU64::new(0),
            interrupt_backoff_total_attempts: AtomicU32::new(0),
            interrupt_backoff_succeeded: AtomicU32::new(0),
            interrupt_backoff_failed_max: AtomicU32::new(0),
            device_id: AtomicU32::new(device_id),
            queue_notify_count: AtomicU32::new(0),
            queue_desc_address: [AtomicU64::new(0), AtomicU64::new(0)],
            queue_driver_address: [AtomicU64::new(0), AtomicU64::new(0)],
            queue_used_address: [AtomicU64::new(0), AtomicU64::new(0)],
            queue_avail_index: [AtomicU16::new(0), AtomicU16::new(0)],
            queue_used_index: [AtomicU16::new(0), AtomicU16::new(0)],
            queue_size: [AtomicU32::new(0), AtomicU32::new(0)],
            queue_ready: [AtomicU32::new(0), AtomicU32::new(0)],
            queue_selected: AtomicU32::new(0),

            #[cfg(feature = "vmm_virtio_input")]
            input_cfg_select: AtomicU32::new(0),
            #[cfg(feature = "vmm_virtio_input")]
            input_cfg_subsel: AtomicU32::new(0),
        }
    }
}

static VIRTIO_MMIO_STATE: VirtioMmioState = VirtioMmioState::new();

fn virtio_device_features(device_id: u32) -> u64 {
    // Keep this tiny and deterministic: advertise VERSION_1 plus a legacy low-bit so
    // existing guest test blobs that only read the low word still see non-zero.
    const VIRTIO_F_VERSION_1: u64 = 1u64 << 32;
    let base = 1u64 | VIRTIO_F_VERSION_1;

    match device_id {
        VIRTIO_INPUT_DEVICE_ID => base,
        VIRTIO_NET_DEVICE_ID => base,
        VIRTIO_GPU_DEVICE_ID => base,
        VIRTIO_CONSOLE_DEVICE_ID => base,
        _ => base,
    }
}

// --- virtio-input (P3) ---
//
// Minimal async-ish virtio-input eventq model:
// - Guest posts writable buffers into queue 0 (eventq). We *stash* these buffers (do not
//   immediately complete them into the used ring).
// - RayOS enqueues input events (from the UI loop) into a small lock-free ring.
// - On VM-exits (and after queue notifications), we pump: write events into stashed
//   buffers, complete used-ring entries, and inject an interrupt.

#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_EVENTQ_INDEX: usize = 0;

#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_EVENT_RING_SIZE: usize = 64; // power-of-two

#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_FREE_BUFS_SIZE: usize = 64; // power-of-two

// --- virtio-input config space (minimal, Linux-ish) ---
//
// virtio-input exposes a device config structure starting at VIRTIO_MMIO_CONFIG_SPACE_OFFSET.
// The guest writes select/subsel (offset 0/1) and then reads `size` (offset 2) and `data`
// (offset 8+). We implement a small subset needed for basic pointer/buttons.

#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_CFG_ID_NAME: u8 = 1;
#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_CFG_ID_SERIAL: u8 = 2;
#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_CFG_ID_DEVIDS: u8 = 3;
#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_CFG_PROP_BITS: u8 = 0x10;
#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_CFG_EV_BITS: u8 = 0x11;
#[cfg(feature = "vmm_virtio_input")]
const VIRTIO_INPUT_CFG_ABS_INFO: u8 = 0x12;

#[cfg(feature = "vmm_virtio_input")]
const EV_SYN: u8 = 0x00;
#[cfg(feature = "vmm_virtio_input")]
const EV_KEY: u8 = 0x01;
#[cfg(feature = "vmm_virtio_input")]
const EV_ABS: u8 = 0x03;

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_cfg_name_bytes() -> &'static [u8] {
    b"rayos-virtio-input\0"
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_cfg_serial_bytes() -> &'static [u8] {
    b"rayos0\0"
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_cfg_devids_bytes() -> [u8; 8] {
    // struct virtio_input_devids { u16 bustype, vendor, product, version }
    // Use a stable but clearly-virtual identity.
    let bustype: u16 = 0x0006; // BUS_VIRTUAL
    let vendor: u16 = 0x1AF4; // virtio vendor
    let product: u16 = 0x0012; // virtio-input device id (informational)
    let version: u16 = 0x0001;
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&bustype.to_le_bytes());
    out[2..4].copy_from_slice(&vendor.to_le_bytes());
    out[4..6].copy_from_slice(&product.to_le_bytes());
    out[6..8].copy_from_slice(&version.to_le_bytes());
    out
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_cfg_absinfo_bytes(subsel: u8) -> Option<[u8; 20]> {
    // struct virtio_input_absinfo { i32 min, max, fuzz, flat, res }
    // Expose ABS_X/ABS_Y with a common 0..32767 range.
    const ABS_X: u8 = 0x00;
    const ABS_Y: u8 = 0x01;
    if subsel != ABS_X && subsel != ABS_Y {
        return None;
    }
    let min: i32 = 0;
    let max: i32 = 32767;
    let fuzz: i32 = 0;
    let flat: i32 = 0;
    let res: i32 = 0;
    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&min.to_le_bytes());
    out[4..8].copy_from_slice(&max.to_le_bytes());
    out[8..12].copy_from_slice(&fuzz.to_le_bytes());
    out[12..16].copy_from_slice(&flat.to_le_bytes());
    out[16..20].copy_from_slice(&res.to_le_bytes());
    Some(out)
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_cfg_bitmap_size_and_byte(ev_type: u8, idx: u64) -> (u8, u8) {
    // Return (size_in_bytes, byte_at_idx). The returned `size` is the guest-visible size.
    // Keep sizes small and deterministic.
    match ev_type {
        EV_SYN => {
            // SYN_REPORT (0)
            let b = if idx == 0 { 0x01 } else { 0x00 };
            (1, b)
        }
        EV_ABS => {
            // ABS_X (0), ABS_Y (1)
            let b = if idx == 0 { 0x03 } else { 0x00 };
            (1, b)
        }
        EV_KEY => {
            // Advertise a small but useful set of keycodes that RayOS can generate today.
            // Includes: ESC, 0-9, tab, backspace, enter, space, A-Z (subset), plus BTN_LEFT/BTN_RIGHT.
            // Keep size large enough for BTN_* (0x110+): bit index 0x110 => byte 34 bit 0.
            let size: u8 = 35;
            let b = match idx {
                // Key range (low codes):
                // - ESC (1) + KEY_1..KEY_6 (2..7)
                0 => 0xFE,
                // - KEY_7..KEY_0 (8..11) + BACKSPACE (14) + TAB (15)
                1 => 0xCF,
                // - QWERTY row: Q..I (16..23)
                2 => 0xFF,
                // - O,P (24..25) + ENTER (28) + A,S (30..31)
                3 => 0xD3,
                // - D..L (32..38)
                4 => 0x7F,
                // - Z,X,C,V (44..47)
                5 => 0xF0,
                // - B,N,M (48..50)
                6 => 0x07,
                // - SPACE (57)
                7 => 0x02,
                // Button range:
                // - BTN_LEFT (0x110), BTN_RIGHT (0x111)
                34 => 0x03,
                _ => 0x00,
            };
            (size, b)
        }
        _ => (0, 0),
    }
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_cfg_read_byte(cfg_offset: u64) -> u8 {
    // Layout:
    //  0: select (rw)
    //  1: subsel (rw)
    //  2: size (ro)
    //  3..7: reserved
    //  8..: data
    let select = VIRTIO_MMIO_STATE.input_cfg_select.load(Ordering::Relaxed) as u8;
    let subsel = VIRTIO_MMIO_STATE.input_cfg_subsel.load(Ordering::Relaxed) as u8;

    // Compute current payload size + data byte for the requested offset.
    let mut size: u8 = 0;
    let mut data_byte: u8 = 0;
    if cfg_offset >= 8 {
        let data_idx = cfg_offset - 8;
        match select {
            VIRTIO_INPUT_CFG_ID_NAME => {
                let s = virtio_input_cfg_name_bytes();
                size = core::cmp::min(s.len(), 128) as u8;
                if (data_idx as usize) < s.len() {
                    data_byte = s[data_idx as usize];
                }
            }
            VIRTIO_INPUT_CFG_ID_SERIAL => {
                let s = virtio_input_cfg_serial_bytes();
                size = core::cmp::min(s.len(), 128) as u8;
                if (data_idx as usize) < s.len() {
                    data_byte = s[data_idx as usize];
                }
            }
            VIRTIO_INPUT_CFG_ID_DEVIDS => {
                let ids = virtio_input_cfg_devids_bytes();
                size = 8;
                if (data_idx as usize) < ids.len() {
                    data_byte = ids[data_idx as usize];
                }
            }
            VIRTIO_INPUT_CFG_PROP_BITS => {
                // No special properties.
                size = 0;
            }
            VIRTIO_INPUT_CFG_EV_BITS => {
                let (s, b) = virtio_input_cfg_bitmap_size_and_byte(subsel, data_idx);
                size = s;
                data_byte = b;
            }
            VIRTIO_INPUT_CFG_ABS_INFO => {
                if let Some(abs) = virtio_input_cfg_absinfo_bytes(subsel) {
                    size = 20;
                    if (data_idx as usize) < abs.len() {
                        data_byte = abs[data_idx as usize];
                    }
                } else {
                    size = 0;
                }
            }
            _ => {
                size = 0;
            }
        }
    } else {
        // cfg_offset < 8 handled below.
        match select {
            VIRTIO_INPUT_CFG_ID_NAME => size = core::cmp::min(virtio_input_cfg_name_bytes().len(), 128) as u8,
            VIRTIO_INPUT_CFG_ID_SERIAL => size = core::cmp::min(virtio_input_cfg_serial_bytes().len(), 128) as u8,
            VIRTIO_INPUT_CFG_ID_DEVIDS => size = 8,
            VIRTIO_INPUT_CFG_PROP_BITS => size = 0,
            VIRTIO_INPUT_CFG_EV_BITS => size = virtio_input_cfg_bitmap_size_and_byte(subsel, 0).0,
            VIRTIO_INPUT_CFG_ABS_INFO => size = if virtio_input_cfg_absinfo_bytes(subsel).is_some() { 20 } else { 0 },
            _ => size = 0,
        }
    }

    match cfg_offset {
        0 => select,
        1 => subsel,
        2 => size,
        3..=7 => 0,
        _ => {
            // When out-of-range, return 0.
            data_byte
        }
    }
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_cfg_write_byte(cfg_offset: u64, byte: u8) {
    match cfg_offset {
        0 => {
            VIRTIO_MMIO_STATE
                .input_cfg_select
                .store(byte as u32, Ordering::Relaxed);
        }
        1 => {
            VIRTIO_MMIO_STATE
                .input_cfg_subsel
                .store(byte as u32, Ordering::Relaxed);
        }
        _ => {}
    }
}

#[cfg(feature = "vmm_virtio_input")]
#[derive(Copy, Clone, Default)]
struct VirtioInputBuf {
    desc_id: u32,
    addr: u64,
    len: u32,
}

#[cfg(feature = "vmm_virtio_input")]
static VIRTIO_INPUT_EVENT_HEAD: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "vmm_virtio_input")]
static VIRTIO_INPUT_EVENT_TAIL: AtomicUsize = AtomicUsize::new(0);

// Packed linux-style input_event (type:u16, code:u16, value:i32) into a u64.
#[cfg(feature = "vmm_virtio_input")]
static VIRTIO_INPUT_EVENTS: [AtomicU64; VIRTIO_INPUT_EVENT_RING_SIZE] = {
    const Z: AtomicU64 = AtomicU64::new(0);
    [Z; VIRTIO_INPUT_EVENT_RING_SIZE]
};

#[cfg(feature = "vmm_virtio_input")]
static mut VIRTIO_INPUT_FREE_BUFS: [VirtioInputBuf; VIRTIO_INPUT_FREE_BUFS_SIZE] =
    [VirtioInputBuf {
        desc_id: 0,
        addr: 0,
        len: 0,
    }; VIRTIO_INPUT_FREE_BUFS_SIZE];
#[cfg(feature = "vmm_virtio_input")]
static mut VIRTIO_INPUT_FREE_HEAD: usize = 0;
#[cfg(feature = "vmm_virtio_input")]
static mut VIRTIO_INPUT_FREE_TAIL: usize = 0;

#[cfg(feature = "vmm_virtio_input")]
#[inline(always)]
fn virtio_input_pack(ty: u16, code: u16, value: i32) -> u64 {
    (ty as u64) | ((code as u64) << 16) | (((value as u32) as u64) << 32)
}

#[cfg(feature = "vmm_virtio_input")]
#[inline(always)]
fn virtio_input_unpack_bytes(packed: u64) -> [u8; 8] {
    let ty = (packed & 0xffff) as u16;
    let code = ((packed >> 16) & 0xffff) as u16;
    let value = (packed >> 32) as u32;
    let mut ev = [0u8; 8];
    ev[0..2].copy_from_slice(&ty.to_le_bytes());
    ev[2..4].copy_from_slice(&code.to_le_bytes());
    ev[4..8].copy_from_slice(&value.to_le_bytes());
    ev
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_eventq_is_empty() -> bool {
    let head = VIRTIO_INPUT_EVENT_HEAD.load(Ordering::Acquire);
    let tail = VIRTIO_INPUT_EVENT_TAIL.load(Ordering::Acquire);
    head == tail
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_eventq_push(packed: u64) -> bool {
    let tail = VIRTIO_INPUT_EVENT_TAIL.load(Ordering::Relaxed);
    let next = (tail + 1) & (VIRTIO_INPUT_EVENT_RING_SIZE - 1);
    let head = VIRTIO_INPUT_EVENT_HEAD.load(Ordering::Acquire);
    if next == head {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:EVENTQ_FULL\n");
        return false;
    }
    VIRTIO_INPUT_EVENTS[tail].store(packed, Ordering::Relaxed);
    VIRTIO_INPUT_EVENT_TAIL.store(next, Ordering::Release);
    true
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_eventq_pop() -> Option<u64> {
    let head = VIRTIO_INPUT_EVENT_HEAD.load(Ordering::Relaxed);
    let tail = VIRTIO_INPUT_EVENT_TAIL.load(Ordering::Acquire);
    if head == tail {
        return None;
    }
    let packed = VIRTIO_INPUT_EVENTS[head].load(Ordering::Relaxed);
    let next = (head + 1) & (VIRTIO_INPUT_EVENT_RING_SIZE - 1);
    VIRTIO_INPUT_EVENT_HEAD.store(next, Ordering::Release);
    Some(packed)
}

#[cfg(feature = "vmm_virtio_input")]
unsafe fn virtio_input_freebuf_push(buf: VirtioInputBuf) -> bool {
    let next = (VIRTIO_INPUT_FREE_TAIL + 1) & (VIRTIO_INPUT_FREE_BUFS_SIZE - 1);
    if next == VIRTIO_INPUT_FREE_HEAD {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:FREEBUF_FULL\n");
        return false;
    }
    VIRTIO_INPUT_FREE_BUFS[VIRTIO_INPUT_FREE_TAIL] = buf;
    VIRTIO_INPUT_FREE_TAIL = next;
    true
}

#[cfg(feature = "vmm_virtio_input")]
unsafe fn virtio_input_freebuf_pop() -> Option<VirtioInputBuf> {
    if VIRTIO_INPUT_FREE_HEAD == VIRTIO_INPUT_FREE_TAIL {
        return None;
    }
    let buf = VIRTIO_INPUT_FREE_BUFS[VIRTIO_INPUT_FREE_HEAD];
    VIRTIO_INPUT_FREE_HEAD = (VIRTIO_INPUT_FREE_HEAD + 1) & (VIRTIO_INPUT_FREE_BUFS_SIZE - 1);
    Some(buf)
}

#[cfg(feature = "vmm_virtio_input")]
fn virtio_input_extract_writable_buf(
    desc_base: u64,
    queue_size: u32,
    start_index: u32,
) -> Option<VirtioInputBuf> {
    if queue_size == 0 {
        return None;
    }
    let mut idx = start_index;
    for _ in 0..MAX_VIRTQ_DESC_CHAIN_ENTRIES {
        if idx >= queue_size {
            return None;
        }
        let desc = read_virtq_descriptor(desc_base, idx)?;
        if (desc.flags & VIRTQ_DESC_F_WRITE) != 0 {
            return Some(VirtioInputBuf {
                desc_id: start_index,
                addr: desc.addr,
                len: desc.len,
            });
        }
        if (desc.flags & VIRTQ_DESC_F_NEXT) == 0 {
            break;
        }
        idx = desc.next as u32;
    }
    None
}

#[cfg(feature = "vmm_virtio_input")]
unsafe fn virtio_input_complete_used(
    used_base: u64,
    queue_size: u32,
    qi: usize,
    desc_id: u32,
    used_len: u32,
) -> bool {
    if queue_size == 0 {
        return false;
    }
    let queue_size_u64 = queue_size as u64;
    let mut used_idx: u16 = VIRTIO_MMIO_STATE.queue_used_index[qi].load(Ordering::Relaxed);
    let used_ring_pos = (used_idx as u64) % queue_size_u64;
    let used_entry_offset =
        used_base + VIRTQ_USED_RING_OFFSET + used_ring_pos * VIRTQ_USED_ENTRY_SIZE;
    if !write_u32(used_entry_offset, desc_id) {
        return false;
    }
    if !write_u32(used_entry_offset + 4, used_len) {
        return false;
    }

    used_idx = used_idx.wrapping_add(1);
    if !write_u16(used_base + VIRTQ_USED_IDX_OFFSET, used_idx) {
        return false;
    }
    VIRTIO_MMIO_STATE.queue_used_index[qi].store(used_idx, Ordering::Relaxed);

    let old_int = VIRTIO_MMIO_STATE
        .interrupt_status
        .fetch_or(VIRTIO_MMIO_INT_VRING, Ordering::Relaxed);
    if (old_int & VIRTIO_MMIO_INT_VRING) == 0 {
        if !inject_guest_interrupt(VIRTIO_MMIO_IRQ_VECTOR) {
            VIRTIO_MMIO_STATE.interrupt_pending.store(1, Ordering::Relaxed);
            VIRTIO_MMIO_STATE
                .interrupt_pending_attempts
                .store(0, Ordering::Relaxed);
            VIRTIO_MMIO_STATE
                .interrupt_pending_last_tick
                .store(crate::TIMER_TICKS.load(Ordering::Relaxed), Ordering::Relaxed);
        }
    }

    true
}

#[cfg(feature = "vmm_virtio_input")]
unsafe fn virtio_input_pump_queue0() {
    if VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed) != VIRTIO_INPUT_DEVICE_ID {
        return;
    }

    let qi = VIRTIO_INPUT_EVENTQ_INDEX;
    if VIRTIO_MMIO_STATE.queue_ready[qi].load(Ordering::Relaxed) == 0 {
        return;
    }

    let used_base = VIRTIO_MMIO_STATE.queue_used_address[qi].load(Ordering::Relaxed);
    let queue_size = VIRTIO_MMIO_STATE.queue_size[qi].load(Ordering::Relaxed);
    if used_base == 0 || queue_size == 0 {
        return;
    }

    while let Some(packed) = virtio_input_eventq_pop() {
        let Some(buf) = virtio_input_freebuf_pop() else {
            // Put the event back by rewinding head one slot would be messy; just drop with a marker.
            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:NO_FREEBUF\n");
            break;
        };

        if buf.len < 8 {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:FREEBUF_TOO_SMALL\n");
            continue;
        }

        let bytes = virtio_input_unpack_bytes(packed);
        if !write_guest_bytes(buf.addr, &bytes) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:EVENT_WRITE_FAIL\n");
            continue;
        }

        if virtio_input_complete_used(used_base, queue_size, qi, buf.desc_id, 8) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:EVENT_WRITTEN\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:USED_COMPLETE_FAIL\n");
        }
    }
}

#[cfg(feature = "vmm_virtio_input")]
pub fn virtio_input_enqueue_mouse_abs(x: u32, y: u32) -> bool {
    // Linux-style event stream: ABS_X, ABS_Y, SYN_REPORT.
    const EV_ABS: u16 = 0x03;
    const ABS_X: u16 = 0x00;
    const ABS_Y: u16 = 0x01;
    const EV_SYN: u16 = 0x00;
    const SYN_REPORT: u16 = 0x00;

    let ok1 = virtio_input_eventq_push(virtio_input_pack(EV_ABS, ABS_X, x as i32));
    let ok2 = virtio_input_eventq_push(virtio_input_pack(EV_ABS, ABS_Y, y as i32));
    let ok3 = virtio_input_eventq_push(virtio_input_pack(EV_SYN, SYN_REPORT, 0));
    if ok1 && ok2 && ok3 {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENQ_MOUSE_ABS\n");
        true
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENQ_MOUSE_ABS_FAIL\n");
        false
    }
}

#[cfg(feature = "vmm_virtio_input")]
pub fn virtio_input_enqueue_click_left() -> bool {
    const EV_KEY: u16 = 0x01;
    const BTN_LEFT: u16 = 0x110;
    const EV_SYN: u16 = 0x00;
    const SYN_REPORT: u16 = 0x00;

    let ok1 = virtio_input_eventq_push(virtio_input_pack(EV_KEY, BTN_LEFT, 1));
    let ok2 = virtio_input_eventq_push(virtio_input_pack(EV_SYN, SYN_REPORT, 0));
    let ok3 = virtio_input_eventq_push(virtio_input_pack(EV_KEY, BTN_LEFT, 0));
    let ok4 = virtio_input_eventq_push(virtio_input_pack(EV_SYN, SYN_REPORT, 0));
    if ok1 && ok2 && ok3 && ok4 {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENQ_CLICK_LEFT\n");
        true
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENQ_CLICK_LEFT_FAIL\n");
        false
    }
}

#[cfg(feature = "vmm_virtio_input")]
pub fn virtio_input_enqueue_click_right() -> bool {
    const EV_KEY: u16 = 0x01;
    const BTN_RIGHT: u16 = 0x111;
    const EV_SYN: u16 = 0x00;
    const SYN_REPORT: u16 = 0x00;

    let ok1 = virtio_input_eventq_push(virtio_input_pack(EV_KEY, BTN_RIGHT, 1));
    let ok2 = virtio_input_eventq_push(virtio_input_pack(EV_SYN, SYN_REPORT, 0));
    let ok3 = virtio_input_eventq_push(virtio_input_pack(EV_KEY, BTN_RIGHT, 0));
    let ok4 = virtio_input_eventq_push(virtio_input_pack(EV_SYN, SYN_REPORT, 0));
    if ok1 && ok2 && ok3 && ok4 {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENQ_CLICK_RIGHT\n");
        true
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENQ_CLICK_RIGHT_FAIL\n");
        false
    }
}

#[cfg(feature = "vmm_virtio_input")]
pub fn virtio_input_enqueue_key_press_release(evdev_code: u16) -> bool {
    const EV_KEY: u16 = 0x01;
    const EV_SYN: u16 = 0x00;
    const SYN_REPORT: u16 = 0x00;

    let ok1 = virtio_input_eventq_push(virtio_input_pack(EV_KEY, evdev_code, 1));
    let ok2 = virtio_input_eventq_push(virtio_input_pack(EV_SYN, SYN_REPORT, 0));
    let ok3 = virtio_input_eventq_push(virtio_input_pack(EV_KEY, evdev_code, 0));
    let ok4 = virtio_input_eventq_push(virtio_input_pack(EV_SYN, SYN_REPORT, 0));
    if ok1 && ok2 && ok3 && ok4 {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENQ_KEY\n");
        true
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENQ_KEY_FAIL\n");
        false
    }
}

fn find_mmio_region(gpa: u64) -> Option<MmioRegion> {
    unsafe {
        for slot in MMIO_REGIONS.iter() {
            if let Some(region) = slot {
                if gpa >= region.base && gpa < region.base + region.size {
                    return Some(*region);
                }
            }
        }
    }
    None
}

fn register_mmio_region(region: MmioRegion) -> bool {
    unsafe {
        for slot in MMIO_REGIONS.iter_mut() {
            if slot.is_none() {
                *slot = Some(region);
                return true;
            }
        }
    }
    false
}

fn init_hypervisor_mmio() {
    unsafe {
        if MMIO_REGIONS_INITIALIZED {
            return;
        }

        #[cfg(feature = "vmm_virtio_console")]
        {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:COMPILED\n");
        }

        let region = MmioRegion {
            base: MMIO_COUNTER_BASE,
            size: MMIO_COUNTER_SIZE,
            handler: mmio_counter_handler,
        };
        if !register_mmio_region(region) {
            crate::serial_write_str("RAYOS_VMM:VMX:MMIO_REGISTRATION_FAIL\n");
        }
        let virtio_region = MmioRegion {
            base: MMIO_VIRTIO_BASE,
            size: MMIO_VIRTIO_SIZE,
            handler: virtio_mmio_handler,
        };
        if !register_mmio_region(virtio_region) {
            crate::serial_write_str("RAYOS_VMM:VMX:MMIO_VIRTIO_FAIL\n");
        }
        MMIO_REGIONS_INITIALIZED = true;

        // Emit a deterministic marker when virtio-console is selected so smoke tests
        // can detect dispatch readiness without needing guest traffic.
        let dev = VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed);
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:DEVICE_ID=");
        crate::serial_write_hex_u64(dev as u64);
        crate::serial_write_str("\n");
        if dev == VIRTIO_CONSOLE_DEVICE_ID {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:ENABLED\n");
        } else if dev == VIRTIO_INPUT_DEVICE_ID {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:ENABLED\n");
        }
    }
}

fn mmio_counter_handler(
    _regs: &mut GuestRegs,
    access: &MmioAccess,
    value: Option<u64>,
) -> Option<u64> {
    match access.kind {
        MmioAccessKind::Read => {
            let v = MMIO_COUNTER.load(Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:MMIO_COUNTER_READ=");
            crate::serial_write_hex_u64(v);
            crate::serial_write_str("\n");
            Some(v)
        }
        MmioAccessKind::Write => {
            if let Some(v) = value {
                MMIO_COUNTER.store(v, Ordering::Relaxed);
                crate::serial_write_str("RAYOS_VMM:MMIO_COUNTER_WRITE=");
                crate::serial_write_hex_u64(v);
                crate::serial_write_str("\n");
            }
            None
        }
    }
}

fn virtio_mmio_handler(
    _regs: &mut GuestRegs,
    access: &MmioAccess,
    value: Option<u64>,
) -> Option<u64> {
    let mask = match access.size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        8 => u64::MAX,
        _ => return None,
    };
    let write_value = value.unwrap_or(0) & mask;

    match (access.offset, access.kind) {
        (VIRTIO_MMIO_MAGIC_VALUE_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_MAGIC_VALUE as u64)
        }
        (VIRTIO_MMIO_VERSION_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_VERSION_VALUE as u64)
        }
        (VIRTIO_MMIO_DEVICE_ID_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_VENDOR_ID_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_VENDOR_ID_VALUE as u64)
        }
        (VIRTIO_MMIO_DEVICE_FEATURES_OFFSET, MmioAccessKind::Read) => {
            let device_id = VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed);
            let sel = VIRTIO_MMIO_STATE.device_features_sel.load(Ordering::Relaxed) & 1;
            let feats = virtio_device_features(device_id);
            let word = if sel == 0 { feats as u32 } else { (feats >> 32) as u32 };
            Some(word as u64)
        }
        (VIRTIO_MMIO_DEVICE_FEATURES_SEL_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_STATE.device_features_sel.load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_DEVICE_FEATURES_SEL_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .device_features_sel
                .store(write_value as u32, Ordering::Relaxed);
            None
        }
        (VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, MmioAccessKind::Read) => {
            let sel = (VIRTIO_MMIO_STATE.driver_features_sel.load(Ordering::Relaxed) & 1) as usize;
            Some(VIRTIO_MMIO_STATE.driver_features[sel].load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_DRIVER_FEATURES_SEL_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_STATE.driver_features_sel.load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_DRIVER_FEATURES_SEL_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .driver_features_sel
                .store(write_value as u32, Ordering::Relaxed);
            None
        }
        (VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_STATE.interrupt_status.load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_STATUS_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_STATE.status.load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_QUEUE_SELECT_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET, MmioAccessKind::Read) => {
            // This VMM only supports a small queue size today.
            let _qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some(VIRTIO_QUEUE_SIZE_VALUE)
        }
        (VIRTIO_MMIO_QUEUE_NUM_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some(VIRTIO_MMIO_STATE.queue_size[qi].load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_QUEUE_READY_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some(VIRTIO_MMIO_STATE.queue_ready[qi].load(Ordering::Relaxed) as u64)
        }

        // Modern split address registers (high parts).
        (VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some((VIRTIO_MMIO_STATE.queue_desc_address[qi].load(Ordering::Relaxed) >> 32) as u64)
        }
        (VIRTIO_MMIO_QUEUE_AVAIL_HIGH_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some((VIRTIO_MMIO_STATE.queue_driver_address[qi].load(Ordering::Relaxed) >> 32) as u64)
        }
        (VIRTIO_MMIO_QUEUE_USED_LOW_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some((VIRTIO_MMIO_STATE.queue_used_address[qi].load(Ordering::Relaxed) & 0xFFFF_FFFF) as u64)
        }
        (VIRTIO_MMIO_QUEUE_USED_HIGH_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some((VIRTIO_MMIO_STATE.queue_used_address[qi].load(Ordering::Relaxed) >> 32) as u64)
        }
        (VIRTIO_MMIO_QUEUE_DESC_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            let v = VIRTIO_MMIO_STATE.queue_desc_address[qi].load(Ordering::Relaxed);
            if access.size == 4 {
                Some((v & 0xFFFF_FFFF) as u64)
            } else {
                Some(v)
            }
        }
        (VIRTIO_MMIO_QUEUE_DRIVER_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some(VIRTIO_MMIO_STATE.queue_driver_address[qi].load(Ordering::Relaxed))
        }
        (VIRTIO_MMIO_QUEUE_AVAIL_LOW_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            let v = VIRTIO_MMIO_STATE.queue_driver_address[qi].load(Ordering::Relaxed);
            if access.size == 8 {
                Some(v)
            } else {
                Some((v & 0xFFFF_FFFF) as u64)
            }
        }
        (VIRTIO_MMIO_QUEUE_SIZE_OFFSET, MmioAccessKind::Read) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            Some(VIRTIO_MMIO_STATE.queue_size[qi].load(Ordering::Relaxed) as u64)
        }
        (VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, MmioAccessKind::Write) => {
            let sel = (VIRTIO_MMIO_STATE.driver_features_sel.load(Ordering::Relaxed) & 1) as usize;
            VIRTIO_MMIO_STATE.driver_features[sel].store(write_value as u32, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:DRIVER_FEATURES=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");
            None
        }
        (VIRTIO_MMIO_STATUS_OFFSET, MmioAccessKind::Write) => {
            let old = VIRTIO_MMIO_STATE.status.load(Ordering::Relaxed);
            VIRTIO_MMIO_STATE
                .status
                .store(write_value as u32, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:STATUS=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");

            #[cfg(feature = "vmm_virtio_input")]
            {
                // Emit a deterministic marker when the guest sets DRIVER_OK for virtio-input.
                // This is a helpful milestone even when VMX-gated smokes are used.
                const VIRTIO_STATUS_DRIVER_OK: u32 = 0x04;
                if VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed) == VIRTIO_INPUT_DEVICE_ID {
                    let newv = write_value as u32;
                    if (old & VIRTIO_STATUS_DRIVER_OK) == 0 && (newv & VIRTIO_STATUS_DRIVER_OK) != 0 {
                        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:DRIVER_OK\n");
                    }
                }
            }
            None
        }

        (VIRTIO_MMIO_QUEUE_SELECT_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .queue_selected
                .store((write_value as u32) & 1, Ordering::Relaxed);
            None
        }

        (VIRTIO_MMIO_QUEUE_NUM_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            VIRTIO_MMIO_STATE.queue_size[qi].store(write_value as u32, Ordering::Relaxed);
            None
        }

        (VIRTIO_MMIO_QUEUE_READY_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            VIRTIO_MMIO_STATE.queue_ready[qi].store((write_value as u32) & 1, Ordering::Relaxed);
            None
        }

        (VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            let old = VIRTIO_MMIO_STATE.queue_desc_address[qi].load(Ordering::Relaxed);
            let newv = (old & 0x0000_0000_FFFF_FFFF) | ((write_value as u32 as u64) << 32);
            VIRTIO_MMIO_STATE.queue_desc_address[qi].store(newv, Ordering::Relaxed);
            None
        }
        // Spec QueueAvailHigh.
        (VIRTIO_MMIO_QUEUE_AVAIL_HIGH_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            let old = VIRTIO_MMIO_STATE.queue_driver_address[qi].load(Ordering::Relaxed);
            let newv = (old & 0x0000_0000_FFFF_FFFF) | ((write_value as u32 as u64) << 32);
            VIRTIO_MMIO_STATE.queue_driver_address[qi].store(newv, Ordering::Relaxed);
            None
        }

        (VIRTIO_MMIO_QUEUE_USED_LOW_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            let old = VIRTIO_MMIO_STATE.queue_used_address[qi].load(Ordering::Relaxed);
            let newv = (old & 0xFFFF_FFFF_0000_0000) | (write_value as u32 as u64);
            VIRTIO_MMIO_STATE.queue_used_address[qi].store(newv, Ordering::Relaxed);
            None
        }
        (VIRTIO_MMIO_QUEUE_USED_HIGH_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            let old = VIRTIO_MMIO_STATE.queue_used_address[qi].load(Ordering::Relaxed);
            let newv = (old & 0x0000_0000_FFFF_FFFF) | ((write_value as u32 as u64) << 32);
            VIRTIO_MMIO_STATE.queue_used_address[qi].store(newv, Ordering::Relaxed);
            None
        }
        (VIRTIO_MMIO_QUEUE_DESC_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            // Allow 64-bit whole writes at the base offset.
            if access.size == 8 {
                VIRTIO_MMIO_STATE
                    .queue_desc_address[qi]
                    .store(write_value as u64, Ordering::Relaxed);
            } else {
                let old = VIRTIO_MMIO_STATE.queue_desc_address[qi].load(Ordering::Relaxed);
                let newv = (old & 0xFFFF_FFFF_0000_0000) | (write_value as u32 as u64);
                VIRTIO_MMIO_STATE
                    .queue_desc_address[qi]
                    .store(newv, Ordering::Relaxed);
            }
            None
        }
        (VIRTIO_MMIO_QUEUE_DRIVER_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            VIRTIO_MMIO_STATE.queue_driver_address[qi].store(write_value as u64, Ordering::Relaxed);
            None
        }
        (VIRTIO_MMIO_QUEUE_AVAIL_LOW_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            if access.size == 8 {
                VIRTIO_MMIO_STATE
                    .queue_driver_address[qi]
                    .store(write_value as u64, Ordering::Relaxed);
            } else {
                let old = VIRTIO_MMIO_STATE.queue_driver_address[qi].load(Ordering::Relaxed);
                let newv = (old & 0xFFFF_FFFF_0000_0000) | (write_value as u32 as u64);
                VIRTIO_MMIO_STATE
                    .queue_driver_address[qi]
                    .store(newv, Ordering::Relaxed);
            }
            None
        }
        (VIRTIO_MMIO_QUEUE_SIZE_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            VIRTIO_MMIO_STATE.queue_size[qi].store(write_value as u32, Ordering::Relaxed);
            None
        }
        (VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET, MmioAccessKind::Write) => {
            let qi = VIRTIO_MMIO_STATE.queue_selected.load(Ordering::Relaxed) as usize;
            let queue_desc_addr = VIRTIO_MMIO_STATE.queue_desc_address[qi].load(Ordering::Relaxed);
            let queue_driver_addr =
                VIRTIO_MMIO_STATE.queue_driver_address[qi].load(Ordering::Relaxed);
            let queue_used_addr = VIRTIO_MMIO_STATE.queue_used_address[qi].load(Ordering::Relaxed);
            let queue_size_value = VIRTIO_MMIO_STATE.queue_size[qi].load(Ordering::Relaxed);
            let queue_ready_value = VIRTIO_MMIO_STATE.queue_ready[qi].load(Ordering::Relaxed);
            // (debug dumps removed)
            log_virtq_descriptors(queue_desc_addr, queue_size_value);
            log_virtq_avail(queue_driver_addr, queue_size_value);
            log_virtq_used(queue_used_addr, queue_size_value);
            process_virtq_queue(
                queue_desc_addr,
                queue_driver_addr,
                queue_used_addr,
                queue_size_value,
                queue_ready_value,
                write_value,
            );
            None
        }
        (VIRTIO_MMIO_INTERRUPT_ACK_OFFSET, MmioAccessKind::Write) => {
            // Virtio-MMIO spec: write 1s to clear corresponding bits.
            let old = VIRTIO_MMIO_STATE.interrupt_status.load(Ordering::Relaxed);
            let new = old & !(write_value as u32);
            VIRTIO_MMIO_STATE
                .interrupt_status
                .store(new, Ordering::Relaxed);
            if write_value != 0 || old != new {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_ACK=");
                crate::serial_write_hex_u64(write_value);
                crate::serial_write_str(" old=");
                crate::serial_write_hex_u64(old as u64);
                crate::serial_write_str(" new=");
                crate::serial_write_hex_u64(new as u64);
                crate::serial_write_str("\n");
            }
            None
        }
        _ => {
            // Handle config space (MAC address for virtio-net, etc.)
            if access.offset >= VIRTIO_MMIO_CONFIG_SPACE_OFFSET {
                let device_id = VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed);
                match device_id {
                    VIRTIO_NET_DEVICE_ID => {
                        let cfg_offset = access.offset - VIRTIO_MMIO_CONFIG_SPACE_OFFSET;
                        // MAC address at offset 0-5 in config space
                        if cfg_offset < 6 && access.kind == MmioAccessKind::Read {
                            Some(VIRTIO_NET_MAC[cfg_offset as usize] as u64)
                        } else {
                            None
                        }
                    }
                    #[cfg(feature = "vmm_virtio_input")]
                    VIRTIO_INPUT_DEVICE_ID => {
                        let cfg_offset = access.offset - VIRTIO_MMIO_CONFIG_SPACE_OFFSET;
                        match access.kind {
                            MmioAccessKind::Read => {
                                // Assemble little-endian value from byte reads.
                                let mut out: u64 = 0;
                                for i in 0..access.size {
                                    let b = virtio_input_cfg_read_byte(cfg_offset + i as u64) as u64;
                                    out |= b << (8 * i);
                                }
                                Some(out)
                            }
                            MmioAccessKind::Write => {
                                // Split writes into bytes; only select/subsel (offset 0/1) are modeled.
                                for i in 0..access.size {
                                    let b = ((write_value >> (8 * i)) & 0xFF) as u8;
                                    virtio_input_cfg_write_byte(cfg_offset + i as u64, b);
                                }
                                None
                            }
                        }
                    }
                    _ => None,
                }
            } else {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:UNKNOWN_ACCESS\n");
                None
            }
        }
    }
}

#[unsafe(naked)]
extern "C" fn vmx_exit_stub() -> ! {
    // Save guest GPRs to the host stack, call into Rust for exit handling,
    // then restore GPRs and resume guest execution.
    core::arch::naked_asm!(
        "push r15\n\
         push r14\n\
         push r13\n\
         push r12\n\
         push r11\n\
         push r10\n\
         push r9\n\
         push r8\n\
         push rdi\n\
         push rsi\n\
         push rbp\n\
         push rdx\n\
         push rcx\n\
         push rbx\n\
         push rax\n\
         mov rdi, rsp\n\
         sub rsp, 8\n\
         call {handler}\n\
         add rsp, 8\n\
         test al, al\n\
         jnz 2f\n\
         pop rax\n\
         pop rbx\n\
         pop rcx\n\
         pop rdx\n\
         pop rbp\n\
         pop rsi\n\
         pop rdi\n\
         pop r8\n\
         pop r9\n\
         pop r10\n\
         pop r11\n\
         pop r12\n\
         pop r13\n\
         pop r14\n\
         pop r15\n\
         vmresume\n\
         pushfq\n\
         pop rdi\n\
         sub rsp, 8\n\
         call {resume_fail}\n\
         ud2\n\
         2:\n\
         pop rax\n\
         pop rbx\n\
         pop rcx\n\
         pop rdx\n\
         pop rbp\n\
         pop rsi\n\
         pop rdi\n\
         pop r8\n\
         pop r9\n\
         pop r10\n\
         pop r11\n\
         pop r12\n\
         pop r13\n\
         pop r14\n\
         pop r15\n\
         jmp {halt}\n",
        handler = sym vmx_exit_handler,
        resume_fail = sym vmx_vmresume_failed,
        halt = sym vmx_host_halt,
    );
}

#[no_mangle]
extern "C" fn vmx_vmresume_failed(rflags: u64) -> ! {
    crate::serial_write_str("RAYOS_VMM:VMX:VMRESUME_FAIL rflags=0x");
    crate::serial_write_hex_u64(rflags);
    crate::serial_write_str("\n");
    unsafe {
        let (ok_err, err) = vmread(VMCS_VM_INSTRUCTION_ERROR);
        if ok_err {
            crate::serial_write_str("RAYOS_VMM:VMX:VM_INSTR_ERR=0x");
            crate::serial_write_hex_u64(err);
            crate::serial_write_str("\n");
        }
        vmxoff();
    }
    crate::serial_write_str("RAYOS_VMM:VMX:HALT\n");
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

#[no_mangle]
extern "C" fn vmx_host_halt() -> ! {
    unsafe {
        vmxoff();
    }
    crate::serial_write_str("RAYOS_VMM:VMX:HALT\n");
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

extern "C" fn vmx_guest_hlt_loop() -> ! {
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Cpuid {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

#[inline(always)]
fn cpuid(leaf: u32, subleaf: u32) -> Cpuid {
    // Use the compiler intrinsic instead of inline asm.
    // LLVM reserves RBX for its own purposes in some configurations.
    unsafe {
        let r = core::arch::x86_64::__cpuid_count(leaf, subleaf);
        Cpuid {
            eax: r.eax,
            ebx: r.ebx,
            ecx: r.ecx,
            edx: r.edx,
        }
    }
}

#[inline(always)]
fn linux_guest_active() -> bool {
    #[cfg(feature = "vmm_linux_guest")]
    unsafe {
        LINUX_GUEST_ENTRY_RIP != 0 && LINUX_GUEST_BOOT_PARAMS_GPA != 0
    }

    #[cfg(not(feature = "vmm_linux_guest"))]
    {
        false
    }
}

#[inline(always)]
fn read_cr2() -> u64 {
    let v: u64;
    unsafe { asm!("mov {}, cr2", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline(always)]
fn read_cr0() -> u64 {
    let v: u64;
    unsafe { asm!("mov {0}, cr0", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline(always)]
fn write_cr0(v: u64) {
    unsafe { asm!("mov cr0, {0}", in(reg) v, options(nomem, nostack, preserves_flags)) };
}

#[inline(always)]
fn read_cr4() -> u64 {
    let v: u64;
    unsafe { asm!("mov {0}, cr4", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline(always)]
fn write_cr4(v: u64) {
    unsafe { asm!("mov cr4, {0}", in(reg) v, options(nomem, nostack, preserves_flags)) };
}

#[repr(C, packed)]
struct InvvpidDescriptor {
    vpid: u16,
    _rsvd1: u16,
    _rsvd2: u32,
    linear_address: u64,
}

#[inline(always)]
unsafe fn invvpid_all_contexts() -> bool {
    // Type 2: invalidate all contexts.
    let desc = InvvpidDescriptor {
        vpid: 0,
        _rsvd1: 0,
        _rsvd2: 0,
        linear_address: 0,
    };
    let inv_type: u64 = 2;
    let rflags: u64;
    asm!(
        "invvpid {0}, [{1}]\n\
         pushfq\n\
         pop {2}",
        in(reg) inv_type,
        in(reg) &desc,
        out(reg) rflags,
        options(nostack)
    );
    (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0
}

#[inline(always)]
unsafe fn vmxon(phys: u64) -> bool {
    let mut region = phys;
    let rflags: u64;
    asm!(
        "vmxon [{0}]\n\
         pushfq\n\
         pop {1}",
        in(reg) &mut region,
        out(reg) rflags,
        options(nostack)
    );
    // If CF=1 or ZF=1 => fail.
    (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0
}

#[inline(always)]
unsafe fn vmxoff() {
    asm!("vmxoff", options(nostack));
}

#[inline(always)]
unsafe fn vmclear(phys: u64) -> bool {
    let mut region = phys;
    let rflags: u64;
    asm!(
        "vmclear [{0}]\n\
         pushfq\n\
         pop {1}",
        in(reg) &mut region,
        out(reg) rflags,
        options(nostack)
    );
    (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0
}

#[inline(always)]
unsafe fn vmptrld(phys: u64) -> bool {
    let mut region = phys;
    let rflags: u64;
    asm!(
        "vmptrld [{0}]\n\
         pushfq\n\
         pop {1}",
        in(reg) &mut region,
        out(reg) rflags,
        options(nostack)
    );
    (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0
}

#[inline(always)]
unsafe fn vmlaunch() -> (bool, u64) {
    let rflags: u64;
    asm!(
        "vmlaunch\n\
         pushfq\n\
         pop {0}",
        out(reg) rflags,
        options(nostack)
    );
    let ok = (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0;
    (ok, rflags)
}

#[cfg(feature = "vmm_linux_guest")]
unsafe fn vmlaunch_with_linux_boot_params(bp_gpa: u64) -> (bool, u64) {
    let mut rflags: u64;
    let mut success: u8;
    core::arch::asm!(
        "xor rbx, rbx",
        "xor rbp, rbp",
        "xor rdi, rdi",
        "mov rsi, {bp}",
        // Linux boot protocol: EAX must contain the magic 'HdrS' (0x53726448)
        // and RSI must point at boot_params.
        "mov eax, 0x53726448",
        "xor rcx, rcx",
        "xor rdx, rdx",
        "vmlaunch",
        "pushfq",
        "pop {rflags}",
        "setna {success}",
        bp = in(reg) bp_gpa,
        rflags = out(reg) rflags,
        success = out(reg_byte) success,
        options(preserves_flags),
    );
    (success == 0, rflags)
}

#[inline(always)]
unsafe fn vmresume() -> (bool, u64) {
    let rflags: u64;
    asm!(
        "vmresume\n\
         pushfq\n\
         pop {0}",
        out(reg) rflags,
        options(nostack)
    );
    let ok = (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0;
    (ok, rflags)
}

#[inline(always)]
unsafe fn vmread(field: u64) -> (bool, u64) {
    let mut value: u64 = 0;
    let rflags: u64;
    asm!(
        "vmread {0}, {1}\n\
         pushfq\n\
         pop {2}",
        out(reg) value,
        in(reg) field,
        out(reg) rflags,
        options(nostack)
    );
    let ok = (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0;
    (ok, value)
}

#[inline(always)]
unsafe fn vmwrite(field: u64, value: u64) -> bool {
    let rflags: u64;
    asm!(
        "vmwrite {1}, {0}\n\
         pushfq\n\
         pop {2}",
        in(reg) value,
        in(reg) field,
        out(reg) rflags,
        options(nostack)
    );
    (rflags & (1 << 0)) == 0 && (rflags & (1 << 6)) == 0
}

fn vmx_has_true_controls() -> bool {
    // IA32_VMX_BASIC bit 55 indicates the availability of true control MSRs.
    let basic = crate::rdmsr(IA32_VMX_BASIC);
    ((basic >> 55) & 1) != 0
}

fn adjust_vmx_controls(msr: u32, desired: u32) -> u32 {
    let caps = crate::rdmsr(msr);
    let allowed0 = caps as u32;
    let allowed1 = (caps >> 32) as u32;
    // Intel SDM recommended adjustment:
    // - Bits set in allowed0 must be 1.
    // - Bits clear in allowed1 must be 0.
    (desired | allowed0) & allowed1
}

fn read_seg_selector_cs() -> u16 {
    let v: u16;
    unsafe { asm!("mov {0:x}, cs", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

fn read_seg_selector_ss() -> u16 {
    let v: u16;
    unsafe { asm!("mov {0:x}, ss", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

fn read_seg_selector_ds() -> u16 {
    let v: u16;
    unsafe { asm!("mov {0:x}, ds", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

fn read_seg_selector_es() -> u16 {
    let v: u16;
    unsafe { asm!("mov {0:x}, es", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

fn read_seg_selector_fs() -> u16 {
    let v: u16;
    unsafe { asm!("mov {0:x}, fs", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

fn read_seg_selector_gs() -> u16 {
    let v: u16;
    unsafe { asm!("mov {0:x}, gs", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

fn read_seg_selector_tr() -> u16 {
    let v: u16;
    unsafe { asm!("str {0:x}", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

fn read_gdtr() -> DtPtr {
    let mut dt = DtPtr { limit: 0, base: 0 };
    unsafe { asm!("sgdt [{0}]", in(reg) &mut dt, options(nostack, preserves_flags)) };
    dt
}

fn read_idtr() -> DtPtr {
    let mut dt = DtPtr { limit: 0, base: 0 };
    unsafe { asm!("sidt [{0}]", in(reg) &mut dt, options(nostack, preserves_flags)) };
    dt
}

fn seg_desc_addr(selector: u16, gdtr_base: u64) -> u64 {
    let index = (selector as u64) & !0x7;
    gdtr_base.wrapping_add(index)
}

fn seg_desc_read(selector: u16, gdtr_base: u64) -> SegDesc {
    unsafe {
        let p = seg_desc_addr(selector, gdtr_base) as *const SegDesc;
        core::ptr::read_unaligned(p)
    }
}

fn seg_desc_base_from_gdt(selector: u16, gdtr_base: u64) -> u64 {
    if selector == 0 {
        return 0;
    }
    let d = seg_desc_read(selector, gdtr_base);
    let base_low = (d.base0 as u32) | ((d.base1 as u32) << 16) | ((d.base2 as u32) << 24);
    let is_system = (d.access & 0x10) == 0;
    if is_system {
        unsafe {
            let high =
                core::ptr::read_unaligned((seg_desc_addr(selector, gdtr_base) + 8) as *const u32)
                    as u64;
            (high << 32) | (base_low as u64)
        }
    } else {
        base_low as u64
    }
}

fn seg_desc_limit_from_gdt(selector: u16, gdtr_base: u64) -> u32 {
    if selector == 0 {
        return 0;
    }
    let d = seg_desc_read(selector, gdtr_base);
    let mut limit = (d.limit0 as u32) | (((d.gran as u32) & 0x0F) << 16);
    let g = (d.gran & 0x80) != 0;
    if g {
        limit = (limit << 12) | 0xFFF;
    }
    limit
}

fn seg_desc_ar_from_gdt(selector: u16, gdtr_base: u64) -> u32 {
    if selector == 0 {
        return 1u32 << 16;
    }
    let d = seg_desc_read(selector, gdtr_base);
    let access = d.access as u32;
    let flags = ((d.gran as u32) & 0xF0) << 8;
    access | flags
}

fn exit_reason_name(basic: u32) -> &'static str {
    match basic {
        0 => "EXCEPTION_OR_NMI",
        2 => "TRIPLE_FAULT",
        7 => "INVALID_GUEST_STATE",
        33 => "VM_ENTRY_FAILURE_INVALID_GUEST_STATE",
        10 => "CPUID",
        12 => "HLT",
        18 => "VMCALL",
        28 => "CR_ACCESS",
        30 => "IO_INSTRUCTION",
        31 => "RDMSR",
        32 => "WRMSR",
        48 => "EPT_VIOLATION",
        52 => "VMX_PREEMPTION_TIMER",
        _ => "(unknown)",
    }
}

unsafe fn ensure_io_bitmaps() -> bool {
    if IO_BITMAPS_READY {
        return true;
    }

    let a = match alloc_zeroed_page() {
        Some(p) => p,
        None => return false,
    };
    let b = match alloc_zeroed_page() {
        Some(p) => p,
        None => return false,
    };

    // Intel VMX I/O bitmaps: a set bit requests a VM-exit.
    // Default: allow all ports (all bits clear), then trap selected ports.
    let a_v = crate::phys_to_virt(a) as *mut u8;
    let b_v = crate::phys_to_virt(b) as *mut u8;
    core::ptr::write_bytes(a_v, 0x00, PAGE_SIZE);
    core::ptr::write_bytes(b_v, 0x00, PAGE_SIZE);

    // I/O bitmap A covers ports 0..0x7FFF.
    let mut set_trap_a = |port: u16| {
        let port = port as usize;
        let byte_index = port / 8;
        let bit = 1u8 << (port % 8);
        let cur = core::ptr::read_volatile(a_v.add(byte_index));
        core::ptr::write_volatile(a_v.add(byte_index), cur | bit);
    };

    // Intercept port 0xE9 (QEMU debugcon-style output).
    set_trap_a(0x00E9);

    // Intercept COM1 UART ports (Linux console=ttyS0 uses these).
    for p in COM1_BASE_PORT..COM1_BASE_PORT.wrapping_add(COM1_PORT_COUNT) {
        set_trap_a(p);
    }

    IO_BITMAP_A_PHYS = a;
    IO_BITMAP_B_PHYS = b;
    IO_BITMAPS_READY = true;
    true
}

unsafe fn ensure_msr_bitmaps() -> bool {
    if MSR_BITMAPS_READY {
        return true;
    }

    let phys = match alloc_zeroed_page() {
        Some(p) => p,
        None => return false,
    };
    let v = crate::phys_to_virt(phys) as *mut u8;
    core::ptr::write_bytes(v, 0x00, PAGE_SIZE);

    // Default: allow RDMSR (read bitmap all zeros). Trapping every RDMSR is extremely slow for
    // Linux and can prevent reaching user space within the harness timeout.

    // MSR bitmap layout (Intel SDM):
    // 0x000..0x3FF  read bitmap for MSRs 0x0000_0000..0x0000_1FFF
    // 0x400..0x7FF  read bitmap for MSRs 0xC000_0000..0xC000_1FFF
    // 0x800..0xBFF  write bitmap for MSRs 0x0000_0000..0x0000_1FFF
    // 0xC00..0xFFF  write bitmap for MSRs 0xC000_0000..0xC000_1FFF
    let set_bit = |base: usize, msr_index: usize| {
        let byte_index = base + (msr_index / 8);
        let bit = 1u8 << (msr_index % 8);
        unsafe {
            let cur = core::ptr::read_volatile(v.add(byte_index));
            core::ptr::write_volatile(v.add(byte_index), cur | bit);
        }
    };

    let intercept_read = |msr: u32| {
        if msr <= 0x1FFF {
            set_bit(0x000, msr as usize);
        } else if (0xC000_0000..=0xC000_1FFF).contains(&msr) {
            set_bit(0x400, (msr - 0xC000_0000) as usize);
        }
    };
    let intercept_write = |msr: u32| {
        if msr <= 0x1FFF {
            set_bit(0x800, msr as usize);
        } else if (0xC000_0000..=0xC000_1FFF).contains(&msr) {
            set_bit(0xC00, (msr - 0xC000_0000) as usize);
        }
    };

    // Safety first: intercept all WRMSR so the guest cannot mutate host MSRs.
    core::ptr::write_bytes(v.add(0x800), 0xFF, 0x400);
    core::ptr::write_bytes(v.add(0xC00), 0xFF, 0x400);

    // Intercept reads for MSRs we virtualize in software/VMCS.
    intercept_read(IA32_EFER);
    intercept_read(IA32_PAT);
    intercept_read(IA32_FS_BASE);
    intercept_read(IA32_GS_BASE);
    intercept_read(IA32_KERNEL_GS_BASE);

    intercept_read(IA32_SYSENTER_CS);
    intercept_read(IA32_SYSENTER_ESP);
    intercept_read(IA32_SYSENTER_EIP);

    intercept_read(IA32_STAR);
    intercept_read(IA32_LSTAR);
    intercept_read(IA32_CSTAR);
    intercept_read(IA32_FMASK);

    intercept_read(IA32_APIC_BASE);

    // Also intercept writes for these MSRs explicitly (even though we already trap all writes),
    // so this list stays in sync if we ever relax the global write policy.
    intercept_write(IA32_EFER);
    intercept_write(IA32_PAT);
    intercept_write(IA32_FS_BASE);
    intercept_write(IA32_GS_BASE);
    intercept_write(IA32_KERNEL_GS_BASE);
    intercept_write(IA32_SYSENTER_CS);
    intercept_write(IA32_SYSENTER_ESP);
    intercept_write(IA32_SYSENTER_EIP);
    intercept_write(IA32_STAR);
    intercept_write(IA32_LSTAR);
    intercept_write(IA32_CSTAR);
    intercept_write(IA32_FMASK);
    intercept_write(IA32_APIC_BASE);

    MSR_BITMAPS_PHYS = phys;
    MSR_BITMAPS_READY = true;
    true
}

#[no_mangle]
extern "C" fn vmx_exit_handler(regs: &mut GuestRegs) -> u8 {
    // return 0 => resume, 1 => halt
    let count = VMEXIT_COUNT.fetch_add(1, Ordering::Relaxed) + 1;

    unsafe {
        let (ok_r, reason) = vmread(VMCS_EXIT_REASON);
        let (ok_q, qual) = vmread(VMCS_EXIT_QUALIFICATION);
        let (ok_len, ilen) = vmread(VMCS_VMEXIT_INSTRUCTION_LEN);
        let (ok_grip, grip) = vmread(GUEST_RIP);
        let (ok_intr, intr_info) = vmread(VMCS_EXIT_INTERRUPTION_INFO);

        if !ok_r {
            crate::serial_write_str("RAYOS_VMM:VMX:VMREAD_EXIT_REASON_FAIL\n");
            return 1;
        }

        // Smoke harness expects this marker string.
        if count == 1 {
            crate::serial_write_str("RAYOS_VMM:VMX:VMEXIT\n");
        }

        #[cfg(feature = "vmm_virtio_input")]
        {
            // Opportunistically pump input events whenever we regain control.
            // This lets the device complete stashed buffers even if the guest
            // doesn't re-notify the queue.
            virtio_input_pump_queue0();
        }

        let exit_basic = (reason & 0xffff) as u32;
        let entry_fail = ((reason >> 31) & 1) as u32;

        // Page-fault exits can be extremely hot during Linux bring-up. Avoid drowning the
        // guest in serial I/O: keep detailed logging for only the first few #PFs.
        let is_pf_exit = exit_basic == 0 && ok_intr && ((intr_info & 0xFF) as u8) == 0x0E;
        let pf_count = if is_pf_exit {
            PFEXIT_COUNT.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            0
        };

        // Keep logs tight: only print full lines for the first few exits and for interesting reasons.
        let verbose = count <= 8
            || (exit_basic == 0 && (!is_pf_exit || pf_count <= 32))
            || exit_basic == 2
            || exit_basic == 7
            || exit_basic == 28
            || exit_basic == 48;
        if verbose {
            crate::serial_write_str("RAYOS_VMM:VMX:EXIT_REASON=0x");
            crate::serial_write_hex_u64(reason);
            crate::serial_write_str("\n");
            if ok_intr {
                crate::serial_write_str("RAYOS_VMM:VMX:EXIT_INTR_INFO=0x");
                crate::serial_write_hex_u64(intr_info);
                crate::serial_write_str("\n");
            }
            crate::serial_write_str("RAYOS_VMM:VMX:EXIT_BASIC=0x");
            crate::serial_write_hex_u64(exit_basic as u64);
            crate::serial_write_str(" name=");
            crate::serial_write_str(exit_reason_name(exit_basic));
            crate::serial_write_str(" entry_fail=0x");
            crate::serial_write_hex_u64(entry_fail as u64);
            crate::serial_write_str("\n");

            if entry_fail != 0 {
                let (ok_err, err) = vmread(VMCS_VM_INSTRUCTION_ERROR);
                if ok_err {
                    crate::serial_write_str("RAYOS_VMM:VMX:VM_INSTR_ERR=0x");
                    crate::serial_write_hex_u64(err);
                    crate::serial_write_str("\n");
                }
            }

            if exit_basic == 0 && ok_intr {
                let vector = (intr_info & 0xFF) as u8;
                let has_error = ((intr_info >> 11) & 1) != 0;
                crate::serial_write_str("RAYOS_VMM:VMX:EXC_VECTOR=0x");
                crate::serial_write_hex_u64(vector as u64);
                crate::serial_write_str("\n");
                if ok_q {
                    crate::serial_write_str("RAYOS_VMM:VMX:EXC_QUAL=0x");
                    crate::serial_write_hex_u64(qual);
                    crate::serial_write_str("\n");
                }
                if has_error {
                    let (ok_ec, ec) = vmread(VMCS_EXIT_INTERRUPTION_ERROR_CODE);
                    if ok_ec {
                        crate::serial_write_str("RAYOS_VMM:VMX:EXC_ERROR_CODE=0x");
                        crate::serial_write_hex_u64(ec);
                        crate::serial_write_str("\n");
                    }
                }
                let (ok_gla, gla) = vmread(GUEST_LINEAR_ADDRESS);
                if ok_gla {
                    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_LINEAR=0x");
                    crate::serial_write_hex_u64(gla);
                    crate::serial_write_str("\n");
                }
                if ok_grip {
                    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_RIP_AT_EXIT=0x");
                    crate::serial_write_hex_u64(grip);
                    crate::serial_write_str("\n");

                    let mut insn = [0u8; 16];
                    if read_guest_bytes(grip, &mut insn) {
                        crate::serial_write_str("RAYOS_VMM:VMX:GUEST_INSN_BYTES=");
                        for b in insn.iter() {
                            const HEX: &[u8; 16] = b"0123456789ABCDEF";
                            crate::serial_write_byte(HEX[(b >> 4) as usize]);
                            crate::serial_write_byte(HEX[(b & 0xF) as usize]);
                        }
                        crate::serial_write_str("\n");
                    }
                }

                // For early-boot failures, a page-walk dump is often the fastest way to
                // distinguish "paging format mismatch" from "mapping missing".
                if vector == 0x0E {
                    // Further throttle expensive #PF details after early bring-up.
                    if pf_count > 32 {
                        crate::serial_write_str("RAYOS_VMM:VMX:PF_THROTTLED count=0x");
                        crate::serial_write_hex_u64(pf_count as u64);
                        crate::serial_write_str("\n");
                        // Skip the rest of the detailed #PF diagnostics.
                        // (Fixups below still run; this only reduces serial overhead.)
                        //
                        // NOTE: This block is inside the verbose exception print path.
                        // Keeping it here ensures we still get *some* signal without flooding.
                        //
                        // Continue to the main exit handling.
                        //
                    } else {
                    let (ok_cr3, cr3) = vmread(GUEST_CR3);
                    let (ok_cr4, cr4) = vmread(GUEST_CR4);
                    let (ok_efer, efer) = vmread(GUEST_IA32_EFER);

                    // For trapped #PF, VM-exit qualification is the authoritative faulting VA.
                    // (Host CR2 is not meaningful here; nested bring-up environments may also
                    // report a bogus GUEST_LINEAR_ADDRESS.)
                    let cr2 = read_cr2();
                    crate::serial_write_str("RAYOS_VMM:VMX:PF_GUEST_CR2=0x");
                    crate::serial_write_hex_u64(cr2);
                    crate::serial_write_str("\n");

                    crate::serial_write_str("RAYOS_VMM:VMX:PF_REG_R11=0x");
                    crate::serial_write_hex_u64(regs.r11);
                    crate::serial_write_str(" R8=0x");
                    crate::serial_write_hex_u64(regs.r8);
                    crate::serial_write_str("\n");
                    let (ok_rsp, rsp) = vmread(GUEST_RSP);
                    if ok_rsp {
                        crate::serial_write_str("RAYOS_VMM:VMX:PF_GUEST_RSP=0x");
                        crate::serial_write_hex_u64(rsp);
                        crate::serial_write_str("\n");
                    }

                    if ok_cr3 {
                        crate::serial_write_str("RAYOS_VMM:VMX:PF_GUEST_CR3=0x");
                        crate::serial_write_hex_u64(cr3);
                        crate::serial_write_str("\n");
                    }
                    if ok_cr4 {
                        crate::serial_write_str("RAYOS_VMM:VMX:PF_GUEST_CR4=0x");
                        crate::serial_write_hex_u64(cr4);
                        crate::serial_write_str("\n");
                    }
                    if ok_efer {
                        crate::serial_write_str("RAYOS_VMM:VMX:PF_GUEST_EFER=0x");
                        crate::serial_write_hex_u64(efer);
                        crate::serial_write_str("\n");
                    }

                    // Primary: VM-exit qualification.
                    let mut fault_va = if ok_q && qual != 0 { qual } else { 0 };
                    // Fallbacks: GUEST_LINEAR and instruction heuristics (best effort only).
                    if fault_va == 0 {
                        fault_va = if cr2 != 0 {
                            cr2
                        } else if ok_gla && gla != 0 {
                            gla
                        } else {
                            0
                        };
                    }
                    if fault_va == 0 {
                        let mut insn = [0u8; 3];
                        if read_guest_bytes(grip, &mut insn) {
                            if insn == [0x45, 0x88, 0x03] {
                                fault_va = regs.r11;
                            }
                        }
                    }
                    if fault_va == 0 {
                        fault_va = grip;
                    }
                    crate::serial_write_str("RAYOS_VMM:VMX:PF_VA=0x");
                    crate::serial_write_hex_u64(fault_va);
                    crate::serial_write_str("\n");

                    if ok_cr3 && ok_cr4 {
                        let is_long_mode = ok_efer && ((efer >> 10) & 1) != 0;
                        if is_long_mode {
                            dump_guest_longmode_walk(cr3, fault_va);
                        } else {
                            dump_guest_pae_walk(cr3, fault_va);
                        }
                    }
                    }
                }
            }
        }

        let action = match exit_basic {
            52 => {
                // VMX-preemption timer expired: periodic forced exit for debugging.
                let n = PREEMPTEXIT_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
                if n <= 8 || n == 1024 || n == 16_384 {
                    crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_TICK=0x");
                    crate::serial_write_hex_u64(n as u64);
                    if ok_grip {
                        crate::serial_write_str(" rip=0x");
                        crate::serial_write_hex_u64(grip);
                    }
                    let (ok_rsp, rsp) = vmread(GUEST_RSP);
                    if ok_rsp {
                        crate::serial_write_str(" rsp=0x");
                        crate::serial_write_hex_u64(rsp);
                    }
                    let (ok_rflags, rflags) = vmread(GUEST_RFLAGS);
                    if ok_rflags {
                        crate::serial_write_str(" rflags=0x");
                        crate::serial_write_hex_u64(rflags);
                    }
                    crate::serial_write_str("\n");
                }

                // On first tick, try to decode the instruction at RIP (best-effort) by
                // translating RIP VA via the guest's current CR3.
                if n == 1 {
                    let (ok_cr3, cr3) = vmread(GUEST_CR3);
                    if ok_cr3 {
                        crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_CR3=0x");
                        crate::serial_write_hex_u64(cr3);
                        crate::serial_write_str("\n");

                        let (ok_cr4, cr4) = vmread(GUEST_CR4);
                        if ok_cr4 {
                            crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_CR4=0x");
                            crate::serial_write_hex_u64(cr4);
                            crate::serial_write_str("\n");
                        }

                        // Interrupt state can explain “stuck” cases.
                        let (ok_int, int_state) = vmread(GUEST_INTERRUPTIBILITY_STATE);
                        let (ok_act, act_state) = vmread(GUEST_ACTIVITY_STATE);
                        if ok_int || ok_act {
                            crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_INT_STATE=");
                            if ok_int {
                                crate::serial_write_str("0x");
                                crate::serial_write_hex_u64(int_state);
                            } else {
                                crate::serial_write_str("(na)");
                            }
                            crate::serial_write_str(" act_state=");
                            if ok_act {
                                crate::serial_write_str("0x");
                                crate::serial_write_hex_u64(act_state);
                            } else {
                                crate::serial_write_str("(na)");
                            }
                            crate::serial_write_str("\n");
                        }

                        let cr4_for_walk = if ok_cr4 { cr4 } else { 0 };

                        if let Some(pa) = guest_longmode_translate_pa_with_cr4(cr3, cr4_for_walk, grip) {
                            crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_RIP_PA=0x");
                            crate::serial_write_hex_u64(pa);
                            crate::serial_write_str("\n");

                            let mut insn = [0u8; 16];
                            if read_guest_bytes(pa, &mut insn) {
                                crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_RIP_BYTES=");
                                for b in insn.iter() {
                                    const HEX: &[u8; 16] = b"0123456789ABCDEF";
                                    crate::serial_write_byte(HEX[(b >> 4) as usize]);
                                    crate::serial_write_byte(HEX[(b & 0xF) as usize]);
                                }
                                crate::serial_write_str("\n");
                            }
                        } else {
                            crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_RIP_TRANSLATE_FAIL\n");
                        }

                        crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_RBX=0x");
                        crate::serial_write_hex_u64(regs.rbx);
                        crate::serial_write_str("\n");
                        if let Some(lock_word) = guest_longmode_read_u64_with_cr4(cr3, cr4_for_walk, regs.rbx) {
                            crate::serial_write_str("RAYOS_VMM:VMX:PREEMPT_RBX_QWORD=0x");
                            crate::serial_write_hex_u64(lock_word);
                            crate::serial_write_str("\n");
                        }

                        let (ok_rsp2, rsp2) = vmread(GUEST_RSP);
                        // Dump a small slice of the current stack (return addresses etc).
                        for i in 0..16u64 {
                            if !ok_rsp2 {
                                break;
                            }
                            let va = rsp2.wrapping_add(i * 8);
                            if let Some(v) = guest_longmode_read_u64_with_cr4(cr3, cr4_for_walk, va) {
                                crate::serial_write_str("RAYOS_VMM:VMX:STACK[");
                                crate::serial_write_hex_u64(i);
                                crate::serial_write_str("]=0x");
                                crate::serial_write_hex_u64(v);
                                crate::serial_write_str("\n");
                            } else {
                                // Stop early if translation fails; dump a walk for the first slot.
                                if i == 0 {
                                    dump_guest_longmode_walk_with_cr4(cr3, cr4_for_walk, rsp2);
                                }
                                break;
                            }
                        }
                    }
                }

                // Re-arm timer for the next period.
                let _ = vmwrite(VMX_PREEMPTION_TIMER_VALUE, 0x0100_0000);
                0
            }
            0 => {
                // Exception or NMI.
                //
                // For the Linux guest, reflect trapped exceptions back into the guest after
                // logging (instead of halting the VM). This is critical for diagnosing
                // early-boot hangs where Linux may be oopsing/panicking.
                if linux_guest_active() && ok_intr {
                    let intr_type = ((intr_info >> 8) & 0x7) as u8;
                    let has_error = ((intr_info >> 11) & 1) != 0;
                    let is_valid = ((intr_info >> 31) & 1) != 0;
                    let vector = (intr_info & 0xFF) as u8;

                    // intr_type values per SDM: 2=NMI, 3=hardware exception.
                    if is_valid && (intr_type == 2 || intr_type == 3) {
                        // Special-case #PF: trapping it is useful for getting a one-time
                        // diagnostic dump, but reflecting it back into the guest via
                        // VM-entry injection can be subtly wrong (e.g. CR2 semantics).
                        // Instead, after logging the first trapped #PF, disable #PF
                        // trapping and resume without injection so the guest takes the
                        // fault architecturally on re-execution.
                        if vector == 0x0E {
                            let (ok_bm, bm) = vmread(EXCEPTION_BITMAP);
                            if ok_bm {
                                let _ = vmwrite(EXCEPTION_BITMAP, bm & !(1u64 << 14));
                            }
                            return 0;
                        }

                        let _ = vmwrite(VMCS_ENTRY_INTERRUPTION_INFO, intr_info);
                        if has_error {
                            let (ok_ec, ec) = vmread(VMCS_EXIT_INTERRUPTION_ERROR_CODE);
                            if ok_ec {
                                let _ = vmwrite(VMCS_ENTRY_EXCEPTION_ERROR_CODE, ec);
                            }
                        }
                        // Do not advance RIP: faults are delivered at the faulting instruction.
                        return 0;
                    }
                    return 1;
                }

                // Non-Linux guests: if we trapped a #PF, try a best-effort page-table fixup.
                if ok_intr {
                    let vector = (intr_info & 0xFF) as u8;
                    if vector == 0x0E {
                        let (ok_cr3, cr3) = vmread(GUEST_CR3);
                        // Best-effort fault VA inference.
                        // For #PF exits, Intel reports the faulting linear address in VM-exit
                        // qualification. This has proven more reliable than CR2/GUEST_LINEAR
                        // in our nested/bring-up environment.
                        let mut fault_va = 0u64;
                        if ok_q && qual != 0 {
                            fault_va = qual;
                        }
                        let cr2 = read_cr2();
                        if fault_va == 0 && cr2 != 0 {
                            fault_va = cr2;
                        }

                        if fault_va == 0 {
                            let mut insn = [0u8; 8];
                            if ok_grip && read_guest_bytes(grip, &mut insn) {
                                if insn[0..3] == [0x45, 0x88, 0x03] {
                                    // mov byte ptr [r11], r8b
                                    fault_va = regs.r11;
                                } else if insn[0..5] == [0x66, 0x45, 0x89, 0x04, 0x4B] {
                                    // mov word ptr [r11 + rcx*2], r8w  (REX.B selects r11 as base; SIB scale=2)
                                    fault_va = regs.r11.wrapping_add(regs.rcx.wrapping_mul(2));
                                }
                            }
                            if fault_va == 0 {
                                let (ok_gla, gla) = vmread(GUEST_LINEAR_ADDRESS);
                                if ok_gla && gla != 0 {
                                    fault_va = gla;
                                }
                            }
                        }

                        if ok_cr3 {
                            let mut candidates = [fault_va, regs.r11];
                            // De-dup (common case: fault_va already came from r11).
                            if candidates[0] == candidates[1] {
                                candidates[1] = 0;
                            }

                            for cand in candidates {
                                if cand == 0 {
                                    continue;
                                }
                                let mut fixed = false;

                                // Low identity mapping.
                                if cand < (GUEST_RAM_SIZE_BYTES as u64) {
                                    fixed |= try_fixup_longmode_map_2mb(cr3, cand, cand);
                                }

                                // Linux x86_64 physmap direct mapping (common base).
                                if let Some(pa) = linux_physmap_pa_for_va(cand) {
                                    if pa < (GUEST_RAM_SIZE_BYTES as u64) {
                                        fixed |= try_fixup_longmode_map_2mb(cr3, cand, pa);
                                    }
                                }

                                // If the access is near the end of a 2MB region, it may be an
                                // indexed/word store that crosses into the next PDE.
                                let off_in_2mb = cand & 0x1F_FFFF;
                                if off_in_2mb >= 0x1FF000 {
                                    let next_2mb = (cand & !0x1F_FFFF) + 0x20_0000;
                                    // Identity next.
                                    if next_2mb < (GUEST_RAM_SIZE_BYTES as u64) {
                                        fixed |= try_fixup_longmode_map_2mb(cr3, next_2mb, next_2mb);
                                    }
                                    // Physmap next.
                                    if let Some(pa) = linux_physmap_pa_for_va(next_2mb) {
                                        if pa < (GUEST_RAM_SIZE_BYTES as u64) {
                                            fixed |= try_fixup_longmode_map_2mb(cr3, next_2mb, pa);
                                        }
                                    }
                                }

                                if fixed {
                                    let _ = unsafe { invvpid_all_contexts() };
                                    // Retry the faulting instruction.
                                    return 0;
                                }
                            }
                        }
                    }
                }
                1
            }
            28 => {
                // Control-register access (MOV to/from CR0/CR3/CR4).
                // We use CR4 guest/host mask + read shadow to let the guest believe CR4.VMXE=0
                // while keeping the actual guest CR4 compliant with VMX fixed-bit requirements.
                if !ok_q {
                    return 1;
                }

                let cr_num = (qual & 0xF) as u8;
                let access_type = ((qual >> 4) & 0x3) as u8;
                let gpr = ((qual >> 8) & 0xF) as u8;

                let get_gpr = |regs: &mut GuestRegs, idx: u8| -> Option<u64> {
                    match idx {
                        0 => Some(regs.rax),
                        1 => Some(regs.rcx),
                        2 => Some(regs.rdx),
                        3 => Some(regs.rbx),
                        4 => {
                            let (ok, v) = unsafe { vmread(GUEST_RSP) };
                            ok.then_some(v)
                        }
                        5 => Some(regs.rbp),
                        6 => Some(regs.rsi),
                        7 => Some(regs.rdi),
                        8 => Some(regs.r8),
                        9 => Some(regs.r9),
                        10 => Some(regs.r10),
                        11 => Some(regs.r11),
                        12 => Some(regs.r12),
                        13 => Some(regs.r13),
                        14 => Some(regs.r14),
                        15 => Some(regs.r15),
                        _ => None,
                    }
                };
                let set_gpr = |regs: &mut GuestRegs, idx: u8, value: u64| -> bool {
                    match idx {
                        0 => {
                            regs.rax = value;
                            true
                        }
                        1 => {
                            regs.rcx = value;
                            true
                        }
                        2 => {
                            regs.rdx = value;
                            true
                        }
                        3 => {
                            regs.rbx = value;
                            true
                        }
                        4 => unsafe { vmwrite(GUEST_RSP, value) },
                        5 => {
                            regs.rbp = value;
                            true
                        }
                        6 => {
                            regs.rsi = value;
                            true
                        }
                        7 => {
                            regs.rdi = value;
                            true
                        }
                        8 => {
                            regs.r8 = value;
                            true
                        }
                        9 => {
                            regs.r9 = value;
                            true
                        }
                        10 => {
                            regs.r10 = value;
                            true
                        }
                        11 => {
                            regs.r11 = value;
                            true
                        }
                        12 => {
                            regs.r12 = value;
                            true
                        }
                        13 => {
                            regs.r13 = value;
                            true
                        }
                        14 => {
                            regs.r14 = value;
                            true
                        }
                        15 => {
                            regs.r15 = value;
                            true
                        }
                        _ => false,
                    }
                };

                // access_type: 0=mov to CR, 1=mov from CR, 2=clts, 3=lmsw
                if access_type == 0 {
                    let Some(val) = get_gpr(regs, gpr) else {
                        return 1;
                    };
                    if cr_num == 4 {
                        // Virtualize away VMXE for the guest.
                        const CR4_VMXE: u64 = 1u64 << 13;
                        let masked = val & !CR4_VMXE;
                        // Guest-visible value in read shadow.
                        let _ = unsafe { vmwrite(CR4_READ_SHADOW, masked) };

                        // Enforced actual CR4 keeps VMXE set.
                        let actual = masked | CR4_VMXE;
                        let _ = unsafe { vmwrite(GUEST_CR4, actual) };
                    } else {
                        // For now, just let other CR writes go through.
                        let field = match cr_num {
                            0 => GUEST_CR0,
                            3 => GUEST_CR3,
                            4 => GUEST_CR4,
                            _ => return 1,
                        };
                        let _ = unsafe { vmwrite(field, val) };
                    }
                } else if access_type == 1 {
                    // Read CR into GPR.
                    let value = match cr_num {
                        4 => {
                            let (ok, v) = unsafe { vmread(CR4_READ_SHADOW) };
                            if ok { v } else { return 1 }
                        }
                        0 => {
                            let (ok, v) = unsafe { vmread(CR0_READ_SHADOW) };
                            if ok { v } else { return 1 }
                        }
                        3 => {
                            let (ok, v) = unsafe { vmread(GUEST_CR3) };
                            if ok { v } else { return 1 }
                        }
                        _ => return 1,
                    };
                    if !set_gpr(regs, gpr, value) {
                        return 1;
                    }
                } else {
                    // clts/lmsw not handled.
                    return 1;
                }

                if ok_len && ok_grip {
                    let _ = vmwrite(GUEST_RIP, grip.wrapping_add(ilen));
                }
                0
            }
            30 => {
                if ok_q {
                    // Intel SDM: I/O instruction exit qualification contains the port in bits 31:16.
                    let port = ((qual >> 16) & 0xffff) as u16;
                    let direction_in = ((qual >> 3) & 1) != 0;
                    let size_code = (qual & 0x7) as u8;
                    let size = match size_code {
                        0 => 1,
                        1 => 2,
                        2 => 4,
                        _ => 0,
                    };

                    if size == 1 {
                        if !direction_in && port == 0x00E9 {
                            let ch = (regs.rax & 0xFF) as u8;
                            crate::serial_write_str("RAYOS_GUEST_E9:");
                            crate::serial_write_byte(ch);
                            crate::serial_write_str("\n");
                        }

                        if com1_is_port(port) {
                            let offset = port.wrapping_sub(COM1_BASE_PORT);
                            if direction_in {
                                let v = com1_uart_in(offset);
                                regs.rax = (regs.rax & !0xFF) | (v as u64);
                            } else {
                                let v = (regs.rax & 0xFF) as u8;
                                com1_uart_out(offset, v);
                            }
                        }
                    }

                    if verbose && !com1_is_port(port) {
                        crate::serial_write_str("RAYOS_VMM:VMX:IO_EXIT_PORT=0x");
                        crate::serial_write_hex_u64(port as u64);
                        crate::serial_write_str(" dir=");
                        crate::serial_write_str(if direction_in { "IN" } else { "OUT" });
                        crate::serial_write_str(" size=");
                        crate::serial_write_hex_u64(size as u64);
                        crate::serial_write_str("\n");
                    }
                }

                if ok_len && ok_grip {
                    let _ = vmwrite(GUEST_RIP, grip.wrapping_add(ilen));
                }
                0
            }
            31 => {
                // RDMSR emulation.
                // We don't currently use MSR-load/store lists, so for the handful of MSRs
                // that Linux cares about early, mirror values in VMCS guest fields.
                let msr = regs.rcx as u32;
                let value = if linux_guest_active()
                    && matches!(
                        msr,
                        IA32_FS_BASE
                            | IA32_GS_BASE
                            | IA32_KERNEL_GS_BASE
                            | IA32_SYSENTER_CS
                            | IA32_SYSENTER_ESP
                            | IA32_SYSENTER_EIP
                            | IA32_STAR
                            | IA32_LSTAR
                            | IA32_CSTAR
                            | IA32_FMASK
                    )
                {
                    // Linux depends on these MSRs for non-trapping instructions like SWAPGS
                    // and SYSCALL/SYSRET. If we only virtualize them in software, the guest
                    // will read stale values from the real MSRs and can fault (e.g. #GP on iretq).
                    crate::rdmsr(msr)
                } else {
                    match msr {
                    IA32_EFER => {
                        let (ok, v) = unsafe { vmread(GUEST_IA32_EFER) };
                        if ok { v } else { 0 }
                    }
                    IA32_FS_BASE => {
                        let (ok, v) = unsafe { vmread(GUEST_FS_BASE) };
                        if ok { v } else { 0 }
                    }
                    IA32_GS_BASE => {
                        let (ok, v) = unsafe { vmread(GUEST_GS_BASE) };
                        if ok { v } else { 0 }
                    }
                    IA32_PAT => {
                        let (ok, v) = unsafe { vmread(GUEST_IA32_PAT) };
                        if ok { v } else { 0 }
                    }
                    IA32_KERNEL_GS_BASE
                    | IA32_SYSENTER_CS
                    | IA32_SYSENTER_ESP
                    | IA32_SYSENTER_EIP
                    | IA32_STAR
                    | IA32_LSTAR
                    | IA32_CSTAR
                    | IA32_FMASK
                    | IA32_APIC_BASE => unsafe { guest_msr_get(msr).unwrap_or(0) },
                    _ => 0,
                    }
                };

                regs.rax = (regs.rax & !0xFFFF_FFFF) | (value as u32 as u64);
                regs.rdx = (regs.rdx & !0xFFFF_FFFF) | ((value >> 32) as u32 as u64);

                if ok_len && ok_grip {
                    let _ = vmwrite(GUEST_RIP, grip.wrapping_add(ilen));
                }
                0
            }
            32 => {
                // WRMSR emulation.
                let msr = regs.rcx as u32;
                let value = ((regs.rdx as u64) << 32) | (regs.rax as u32 as u64);

                // Linux relies on certain MSRs being truly written because other instructions
                // consult the *hardware* MSRs directly (e.g. SWAPGS, SYSCALL/SYSRET). For those
                // MSRs, write-through to hardware for the Linux guest.
                if linux_guest_active()
                    && matches!(
                        msr,
                        IA32_FS_BASE
                            | IA32_GS_BASE
                            | IA32_KERNEL_GS_BASE
                            | IA32_SYSENTER_CS
                            | IA32_SYSENTER_ESP
                            | IA32_SYSENTER_EIP
                            | IA32_STAR
                            | IA32_LSTAR
                            | IA32_CSTAR
                            | IA32_FMASK
                    )
                {
                    // Keep VMCS base fields in sync when applicable.
                    match msr {
                        IA32_FS_BASE => {
                            let _ = unsafe { vmwrite(GUEST_FS_BASE, value) };
                        }
                        IA32_GS_BASE => {
                            let _ = unsafe { vmwrite(GUEST_GS_BASE, value) };
                        }
                        _ => {}
                    }

                    crate::wrmsr(msr, value);

                    if ok_len && ok_grip {
                        let _ = vmwrite(GUEST_RIP, grip.wrapping_add(ilen));
                    }
                    return 0;
                }

                match msr {
                    IA32_EFER => {
                        let _ = unsafe { vmwrite(GUEST_IA32_EFER, value) };
                    }
                    IA32_FS_BASE => {
                        let _ = unsafe { vmwrite(GUEST_FS_BASE, value) };
                    }
                    IA32_GS_BASE => {
                        let _ = unsafe { vmwrite(GUEST_GS_BASE, value) };
                    }
                    IA32_PAT => {
                        let _ = unsafe { vmwrite(GUEST_IA32_PAT, value) };
                    }
                    IA32_KERNEL_GS_BASE
                    | IA32_SYSENTER_CS
                    | IA32_SYSENTER_ESP
                    | IA32_SYSENTER_EIP
                    | IA32_STAR
                    | IA32_LSTAR
                    | IA32_CSTAR
                    | IA32_FMASK
                    | IA32_APIC_BASE => unsafe { guest_msr_set(msr, value) },
                    _ => {
                        // Keep trapping WRMSR for safety (MSR bitmap traps all writes).
                        // For now, ignore unknown writes.
                    }
                }

                if ok_len && ok_grip {
                    let _ = vmwrite(GUEST_RIP, grip.wrapping_add(ilen));
                }
                0
            }
            10 => {
                // CPUID emulation: return host CPUID.
                let leaf = regs.rax as u32;
                let subleaf = regs.rcx as u32;
                let mut r = cpuid(leaf, subleaf);

                if linux_guest_active() {
                    let (ok_cpu2, cpu2) = vmread(SECONDARY_VM_EXEC_CONTROL);
                    let cpu2 = if ok_cpu2 { cpu2 as u32 } else { 0 };

                    // Avoid advertising a hypervisor interface. Linux will otherwise select
                    // pvclock/kvm-clock paths that expect a consistent KVM CPUID+MSR surface.
                    // We currently do not fully implement that paravirt clock ABI.
                    if leaf == 0x4000_0000 {
                        // Report no hypervisor leaves.
                        r.eax = 0;
                        r.ebx = 0;
                        r.ecx = 0;
                        r.edx = 0;
                    } else if (0x4000_0000..=0x4000_00FF).contains(&leaf) {
                        r.eax = 0;
                        r.ebx = 0;
                        r.ecx = 0;
                        r.edx = 0;
                    } else if leaf == 0 {
                        // Cap max basic leaf to keep topology/feature probing simple.
                        // This avoids Linux relying on host-specific topology leaves.
                        r.eax = r.eax.min(7);
                    } else if leaf == 1 {
                        // Clear CPUID.1:ECX[31] "hypervisor present".
                        r.ecx &= !(1u32 << 31);

                        // Bring-up simplification: avoid XSAVE/OSXSAVE/AVX so Linux won't execute
                        // XSETBV (which would otherwise VM-exit and require full XCR0 emulation).
                        r.ecx &= !(1u32 << 26); // XSAVE
                        r.ecx &= !(1u32 << 27); // OSXSAVE
                        r.ecx &= !(1u32 << 28); // AVX

                        // Present a simple single-CPU topology.
                        // CPUID.1:EBX[23:16] = logical processors per package.
                        r.ebx = (r.ebx & !(0xffu32 << 16)) | (1u32 << 16);
                        // CPUID.1:EBX[31:24] = initial APIC ID.
                        r.ebx &= !(0xffu32 << 24);

                        // Clear HTT flag (CPUID.1:EDX[28]) to discourage SMP assumptions.
                        r.edx &= !(1u32 << 28);
                    } else if leaf == 7 && subleaf == 0 {
                        // INVPCID is only legal in VMX non-root if enabled via secondary controls.
                        if (cpu2 & CPU2_CTL_ENABLE_INVPCID) == 0 {
                            // CPUID.(EAX=7,ECX=0):EBX[10] == INVPCID
                            r.ebx &= !(1u32 << 10);
                        }

                        // Avoid advertising 5-level paging support (LA57). If Linux enables
                        // CR4.LA57 it changes paging structure depth and complicates bring-up.
                        // CPUID.(EAX=7,ECX=0):ECX[16] == LA57
                        r.ecx &= !(1u32 << 16);
                    } else if leaf == 0xB || leaf == 0x1F {
                        // Do not expose extended topology enumeration.
                        r.eax = 0;
                        r.ebx = 0;
                        r.ecx = 0;
                        r.edx = 0;
                    } else if leaf == 0x8000_0001 {
                        // RDTSCP is only legal in VMX non-root if enabled via secondary controls.
                        if (cpu2 & CPU2_CTL_ENABLE_RDTSCP) == 0 {
                            // CPUID.(EAX=0x80000001):EDX[27] == RDTSCP
                            r.edx &= !(1u32 << 27);
                        }
                    }
                }

                regs.rax = r.eax as u64;
                regs.rbx = r.ebx as u64;
                regs.rcx = r.ecx as u64;
                regs.rdx = r.edx as u64;

                if ok_len && ok_grip {
                    let _ = vmwrite(GUEST_RIP, grip.wrapping_add(ilen));
                }
                0
            }
            12 => {
                // HLT: advance and let the guest continue (our test guest jumps back after HLT).
                let n = HLTEXIT_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 {
                    crate::serial_write_str("RAYOS_VMM:VMX:HLT_SEEN\n");

                    let (ok_rip, rip) = vmread(GUEST_RIP);
                    let (ok_rflags, rflags) = vmread(GUEST_RFLAGS);
                    let (ok_cr3, cr3) = vmread(GUEST_CR3);
                    let (ok_int, int_state) = vmread(GUEST_INTERRUPTIBILITY_STATE);
                    let (ok_act, act_state) = vmread(GUEST_ACTIVITY_STATE);
                    if ok_rip && ok_rflags && ok_cr3 {
                        crate::serial_write_str("RAYOS_VMM:VMX:HLT_STATE rip=0x");
                        crate::serial_write_hex_u64(rip);
                        crate::serial_write_str(" rflags=0x");
                        crate::serial_write_hex_u64(rflags);
                        crate::serial_write_str(" cr3=0x");
                        crate::serial_write_hex_u64(cr3);
                        if ok_int {
                            crate::serial_write_str(" int_state=0x");
                            crate::serial_write_hex_u64(int_state);
                        }
                        if ok_act {
                            crate::serial_write_str(" act_state=0x");
                            crate::serial_write_hex_u64(act_state);
                        }
                        crate::serial_write_str("\n");
                    }
                } else if n == 100_000 || n == 1_000_000 {
                    crate::serial_write_str("RAYOS_VMM:VMX:HLT_COUNT=0x");
                    crate::serial_write_hex_u64(n as u64);
                    crate::serial_write_str("\n");
                }
                if ok_len && ok_grip {
                    let _ = vmwrite(GUEST_RIP, grip.wrapping_add(ilen));
                }
                0
            }
            48 => handle_ept_violation(regs, qual, grip, ilen, ok_len, ok_grip, verbose),
            2 | 7 => {
                // Triple fault or invalid state: stop.
                1
            }
            _ => {
                // Unknown exit: stop for now.
                1
            }
        };

        // If we're going to halt the VMM, always print at least minimal exit context,
        // even when verbose logging is suppressed.
        if action != 0 && !verbose {
            crate::serial_write_str("RAYOS_VMM:VMX:HALT_EXIT_REASON=0x");
            crate::serial_write_hex_u64(reason);
            crate::serial_write_str(" exit_basic=0x");
            crate::serial_write_hex_u64(exit_basic as u64);
            crate::serial_write_str(" name=");
            crate::serial_write_str(exit_reason_name(exit_basic));
            if ok_intr {
                crate::serial_write_str(" intr_info=0x");
                crate::serial_write_hex_u64(intr_info);
            }
            crate::serial_write_str("\n");
        }

        action
    }
}

fn ept_access_description(read: bool, write: bool, exec: bool) -> &'static str {
    match (read, write, exec) {
        (false, false, true) => "EXEC",
        (false, true, false) => "WRITE",
        (true, false, false) => "READ",
        (true, true, false) => "READ+WRITE",
        (true, false, true) => "READ+EXEC",
        (false, true, true) => "WRITE+EXEC",
        (true, true, true) => "READ+WRITE+EXEC",
        _ => "UNKNOWN",
    }
}

fn handle_ept_violation(
    regs: &mut GuestRegs,
    qual: u64,
    grip: u64,
    ilen: u64,
    ok_len: bool,
    ok_grip: bool,
    verbose: bool,
) -> u8 {
    let read = (qual & 1) != 0;
    let write = (qual & 2) != 0;
    let exec = (qual & 4) != 0;
    let gla_valid = ((qual >> 3) & 1) != 0;

    let (ok_gpa, gpa) = unsafe { vmread(GUEST_PHYSICAL_ADDRESS) };
    let (ok_gla, gla) = if gla_valid {
        unsafe { vmread(GUEST_LINEAR_ADDRESS) }
    } else {
        (false, 0)
    };

    if ok_gpa {
        // Special-case the guest local APIC MMIO page (0xFEE0_0000). With EPT enabled we do not
        // identity-map MMIO ranges, so Linux will otherwise hit an EPT violation very early.
        // For the Linux guest we need LAPIC register semantics (timer/IPIs/EOI), so prefer mapping
        // the real LAPIC MMIO page through EPT as UC.
        if linux_guest_active() {
            let lapic_page_gpa = gpa & !0xFFF;
            if lapic_page_gpa == 0xFEE0_0000 {
                unsafe {
                    // First try identity-mapping the LAPIC page so the guest can program the APIC.
                    let (ok_eptp, eptp) = vmread(EPT_POINTER);
                    if ok_eptp && ept_map_4k_page(eptp, 0xFEE0_0000, 0xFEE0_0000, EPT_MEMTYPE_UC) {
                        return 0;
                    }

                    // Fallback: map the 4KB LAPIC page to a host-allocated backing page (UC) so
                    // reads/writes succeed even if LAPIC passthrough is not possible.
                    if GUEST_FAKE_LAPIC_PAGE_PHYS == 0 {
                        if let Some(p) = alloc_zeroed_page() {
                            GUEST_FAKE_LAPIC_PAGE_PHYS = p;
                            let v = crate::phys_to_virt(p) as *mut u32;
                            // LAPIC ID (offset 0x20): APIC ID is bits 24..31.
                            core::ptr::write_volatile(v.add(0x20 / 4), 0u32);
                            // LAPIC Version (offset 0x30): pick a plausible version.
                            core::ptr::write_volatile(v.add(0x30 / 4), 0x14u32);
                        }
                    }

                    if GUEST_FAKE_LAPIC_PAGE_PHYS != 0 {
                        let (ok_eptp, eptp) = vmread(EPT_POINTER);
                        if ok_eptp
                            && ept_map_4k_page(
                                eptp,
                                0xFEE0_0000,
                                GUEST_FAKE_LAPIC_PAGE_PHYS,
                                EPT_MEMTYPE_UC,
                            )
                        {
                            return 0;
                        }
                    }
                }
            }
        }

        // For nested bring-up under QEMU, Linux will probe PCI devices whose BARs live in the
        // conventional MMIO window (e.g. 0x8000_0000+). Our default EPT only maps guest RAM and
        // RayOS synthetic MMIO, so allow identity-mapping this window on-demand.
        if linux_guest_active() {
            let page_gpa = gpa & !0xFFF;
            if (0x8000_0000..0x9000_0000).contains(&page_gpa) {
                unsafe {
                    let (ok_eptp, eptp) = vmread(EPT_POINTER);
                    if ok_eptp && ept_map_4k_page(eptp, page_gpa, page_gpa, EPT_MEMTYPE_UC) {
                        return 0;
                    }
                }
            }
        }

        if let Some(region) = find_mmio_region(gpa) {
            if emulate_mmio_access(&region, regs, gpa, qual, grip, ilen, ok_len, ok_grip) {
                return 0;
            }
        }
    }

    crate::serial_write_str("RAYOS_VMM:VMX:EPT_VIOLATION=0x");
    crate::serial_write_hex_u64(qual);
    crate::serial_write_str(" access=");
    crate::serial_write_str(ept_access_description(read, write, exec));
    crate::serial_write_str("\n");

    if ok_gpa {
        crate::serial_write_str("RAYOS_VMM:VMX:EPT_GPA=0x");
        crate::serial_write_hex_u64(gpa);
        crate::serial_write_str("\n");
    }
    if gla_valid {
        if ok_gla {
            crate::serial_write_str("RAYOS_VMM:VMX:EPT_GLA=0x");
            crate::serial_write_hex_u64(gla);
            crate::serial_write_str("\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VMX:EPT_GLA=INVALID\n");
        }
    }

    if verbose {
        crate::serial_write_str("RAYOS_VMM:VMX:EPT_ILEN=0x");
        crate::serial_write_hex_u64(ilen);
        crate::serial_write_str("\n");
        if ok_len && ok_grip {
            crate::serial_write_str("RAYOS_VMM:VMX:EPT_RIP=0x");
            crate::serial_write_hex_u64(grip);
            crate::serial_write_str("\n");
        }
    }

    crate::serial_write_str("RAYOS_VMM:VMX:EPT_VIOLATION_HALT\n");
    1
}

fn read_guest_bytes(gpa: u64, buf: &mut [u8]) -> bool {
    if buf.is_empty() {
        return true;
    }
    let mut logged = 0;
    with_guest_memory_range(gpa, buf.len(), |ptr, chunk| {
        for i in 0..chunk {
            buf[logged + i] = unsafe { core::ptr::read_volatile(ptr.add(i)) };
        }
        logged += chunk;
        true
    })
}

fn read_u16(gpa: u64) -> Option<u16> {
    let mut buf = [0u8; 2];
    if read_guest_bytes(gpa, &mut buf) {
        Some(u16::from_le_bytes(buf))
    } else {
        None
    }
}

fn read_u32(gpa: u64) -> Option<u32> {
    let mut buf = [0u8; 4];
    if read_guest_bytes(gpa, &mut buf) {
        Some(u32::from_le_bytes(buf))
    } else {
        None
    }
}

fn read_u64(gpa: u64) -> Option<u64> {
    let mut buf = [0u8; 8];
    if read_guest_bytes(gpa, &mut buf) {
        Some(u64::from_le_bytes(buf))
    } else {
        None
    }
}

fn dump_guest_pae_walk(cr3: u64, va: u64) {
    // PAE paging (32-bit with CR4.PAE=1, EFER.LMA=0):
    //  CR3 points to a 32-byte PDPT containing 4 x 64-bit PDPTEs.
    //  va[31:30] selects PDPT entry.
    //  va[29:21] selects PDE.
    //  va[20:12] selects PTE.
    let pdpt_base = cr3 & 0xFFFF_FFFF_FFFF_F000;
    let pdpt_idx = ((va >> 30) & 0x3) as u64;
    let pd_idx = ((va >> 21) & 0x1FF) as u64;
    let pt_idx = ((va >> 12) & 0x1FF) as u64;

    crate::serial_write_str("RAYOS_VMM:VMX:PAE_PDPT_BASE=0x");
    crate::serial_write_hex_u64(pdpt_base);
    crate::serial_write_str(" idx=0x");
    crate::serial_write_hex_u64(pdpt_idx);
    crate::serial_write_str("\n");

    let pdpte_gpa = pdpt_base.wrapping_add(pdpt_idx * 8);
    let Some(pdpte) = read_u64(pdpte_gpa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:PAE_PDPTE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:PAE_PDPTE=0x");
    crate::serial_write_hex_u64(pdpte);
    crate::serial_write_str("\n");

    if (pdpte & 0x1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:PAE_PDPTE_NOT_PRESENT\n");
        return;
    }

    let pd_base = pdpte & 0xFFFF_FFFF_FFFF_F000;
    let pde_gpa = pd_base.wrapping_add(pd_idx * 8);
    let Some(pde) = read_u64(pde_gpa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:PAE_PDE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:PAE_PD_BASE=0x");
    crate::serial_write_hex_u64(pd_base);
    crate::serial_write_str(" idx=0x");
    crate::serial_write_hex_u64(pd_idx);
    crate::serial_write_str(" PDE=0x");
    crate::serial_write_hex_u64(pde);
    crate::serial_write_str("\n");

    if (pde & 0x1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:PAE_PDE_NOT_PRESENT\n");
        return;
    }

    let ps = (pde >> 7) & 1;
    if ps != 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:PAE_PDE_PS_2MB\n");
        return;
    }

    let pt_base = pde & 0xFFFF_FFFF_FFFF_F000;
    let pte_gpa = pt_base.wrapping_add(pt_idx * 8);
    let Some(pte) = read_u64(pte_gpa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:PAE_PTE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:PAE_PT_BASE=0x");
    crate::serial_write_hex_u64(pt_base);
    crate::serial_write_str(" idx=0x");
    crate::serial_write_hex_u64(pt_idx);
    crate::serial_write_str(" PTE=0x");
    crate::serial_write_hex_u64(pte);
    crate::serial_write_str("\n");

    if (pte & 0x1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:PAE_PTE_NOT_PRESENT\n");
        return;
    }

    let pa = (pte & 0xFFFF_FFFF_FFFF_F000) | (va & 0xFFF);
    crate::serial_write_str("RAYOS_VMM:VMX:PAE_TRANSL_PA=0x");
    crate::serial_write_hex_u64(pa);
    crate::serial_write_str("\n");
}

fn dump_guest_longmode_walk(cr3: u64, va: u64) {
    // 4-level paging (IA-32e):
    //  CR3 -> PML4 -> PDPT -> PD -> PT.
    let pml4_base = cr3 & 0xFFFF_FFFF_FFFF_F000;
    let pml4_idx = ((va >> 39) & 0x1FF) as u64;
    let pdpt_idx = ((va >> 30) & 0x1FF) as u64;
    let pd_idx = ((va >> 21) & 0x1FF) as u64;
    let pt_idx = ((va >> 12) & 0x1FF) as u64;

    crate::serial_write_str("RAYOS_VMM:VMX:LM_PML4_BASE=0x");
    crate::serial_write_hex_u64(pml4_base);
    crate::serial_write_str(" idx=0x");
    crate::serial_write_hex_u64(pml4_idx);
    crate::serial_write_str("\n");

    let pml4e_gpa = pml4_base.wrapping_add(pml4_idx * 8);
    let Some(pml4e) = read_u64(pml4e_gpa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PML4E_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:LM_PML4E=0x");
    crate::serial_write_hex_u64(pml4e);
    crate::serial_write_str("\n");
    if (pml4e & 0x1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PML4E_NOT_PRESENT\n");
        return;
    }

    let pdpt_base = pml4e & 0xFFFF_FFFF_FFFF_F000;
    let pdpte_gpa = pdpt_base.wrapping_add(pdpt_idx * 8);
    let Some(pdpte) = read_u64(pdpte_gpa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDPTE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:LM_PDPT_BASE=0x");
    crate::serial_write_hex_u64(pdpt_base);
    crate::serial_write_str(" idx=0x");
    crate::serial_write_hex_u64(pdpt_idx);
    crate::serial_write_str(" PDPTE=0x");
    crate::serial_write_hex_u64(pdpte);
    crate::serial_write_str("\n");
    if (pdpte & 0x1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDPTE_NOT_PRESENT\n");
        return;
    }
    let ps1g = (pdpte >> 7) & 1;
    if ps1g != 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDPTE_PS_1GB\n");
        return;
    }

    let pd_base = pdpte & 0xFFFF_FFFF_FFFF_F000;
    let pde_gpa = pd_base.wrapping_add(pd_idx * 8);
    let Some(pde) = read_u64(pde_gpa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:LM_PD_BASE=0x");
    crate::serial_write_hex_u64(pd_base);
    crate::serial_write_str(" idx=0x");
    crate::serial_write_hex_u64(pd_idx);
    crate::serial_write_str(" PDE=0x");
    crate::serial_write_hex_u64(pde);
    crate::serial_write_str("\n");
    if (pde & 0x1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDE_NOT_PRESENT\n");
        return;
    }
    let ps2m = (pde >> 7) & 1;
    if ps2m != 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDE_PS_2MB\n");
        return;
    }

    let pt_base = pde & 0xFFFF_FFFF_FFFF_F000;
    let pte_gpa = pt_base.wrapping_add(pt_idx * 8);
    let Some(pte) = read_u64(pte_gpa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PTE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:LM_PT_BASE=0x");
    crate::serial_write_hex_u64(pt_base);
    crate::serial_write_str(" idx=0x");
    crate::serial_write_hex_u64(pt_idx);
    crate::serial_write_str(" PTE=0x");
    crate::serial_write_hex_u64(pte);
    crate::serial_write_str("\n");
    if (pte & 0x1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PTE_NOT_PRESENT\n");
        return;
    }

    let pa = (pte & 0xFFFF_FFFF_FFFF_F000) | (va & 0xFFF);
    crate::serial_write_str("RAYOS_VMM:VMX:LM_TRANSL_PA=0x");
    crate::serial_write_hex_u64(pa);
    crate::serial_write_str("\n");
}

fn guest_longmode_translate_pa_with_cr4(cr3: u64, cr4: u64, va: u64) -> Option<u64> {
    // IA-32e paging:
    // - 4-level: CR3 -> PML4 -> PDPT -> PD -> PT
    // - 5-level (LA57): CR3 -> PML5 -> PML4 -> PDPT -> PD -> PT
    // Mask to the address portion (bits 51:12). Do not propagate NX/attr bits.
    let base = cr3 & 0x000F_FFFF_FFFF_F000;
    if base == 0 {
        return None;
    }

    let la57 = (cr4 & (1u64 << 12)) != 0;
    let (pml5_idx, pml4_idx) = if la57 {
        (((va >> 48) & 0x1FF) as u64, ((va >> 39) & 0x1FF) as u64)
    } else {
        (0, ((va >> 39) & 0x1FF) as u64)
    };
    let pdpt_idx = ((va >> 30) & 0x1FF) as u64;
    let pd_idx = ((va >> 21) & 0x1FF) as u64;
    let pt_idx = ((va >> 12) & 0x1FF) as u64;

    let pml4_base = if la57 {
        let pml5e_gpa = base.wrapping_add(pml5_idx * 8);
        let pml5e = read_u64(pml5e_gpa)?;
        if (pml5e & 0x1) == 0 {
            return None;
        }
        pml5e & 0x000F_FFFF_FFFF_F000
    } else {
        base
    };

    let pml4e_gpa = pml4_base.wrapping_add(pml4_idx * 8);
    let pml4e = read_u64(pml4e_gpa)?;
    if (pml4e & 0x1) == 0 {
        return None;
    }

    let pdpt_base = pml4e & 0x000F_FFFF_FFFF_F000;
    let pdpte_gpa = pdpt_base.wrapping_add(pdpt_idx * 8);
    let pdpte = read_u64(pdpte_gpa)?;
    if (pdpte & 0x1) == 0 {
        return None;
    }
    if ((pdpte >> 7) & 1) != 0 {
        // 1GB page.
        let pa_base = pdpte & 0x000F_FFFF_C000_0000;
        return Some(pa_base | (va & 0x3FFF_FFFF));
    }

    let pd_base = pdpte & 0x000F_FFFF_FFFF_F000;
    let pde_gpa = pd_base.wrapping_add(pd_idx * 8);
    let pde = read_u64(pde_gpa)?;
    if (pde & 0x1) == 0 {
        return None;
    }
    if ((pde >> 7) & 1) != 0 {
        // 2MB page.
        let pa_base = pde & 0x000F_FFFF_FFE0_0000;
        return Some(pa_base | (va & 0x1F_FFFF));
    }

    let pt_base = pde & 0x000F_FFFF_FFFF_F000;
    let pte_gpa = pt_base.wrapping_add(pt_idx * 8);
    let pte = read_u64(pte_gpa)?;
    if (pte & 0x1) == 0 {
        return None;
    }

    let pa = (pte & 0x000F_FFFF_FFFF_F000) | (va & 0xFFF);
    Some(pa)
}

fn guest_longmode_translate_pa(cr3: u64, va: u64) -> Option<u64> {
    // Backwards-compatible wrapper used in bring-up paths where CR4 isn't available.
    guest_longmode_translate_pa_with_cr4(cr3, 0, va)
}

fn guest_longmode_read_u64_with_cr4(cr3: u64, cr4: u64, va: u64) -> Option<u64> {
    let pa = guest_longmode_translate_pa_with_cr4(cr3, cr4, va)?;
    read_u64(pa)
}

fn guest_longmode_read_u64(cr3: u64, va: u64) -> Option<u64> {
    let pa = guest_longmode_translate_pa(cr3, va)?;
    read_u64(pa)
}

fn dump_guest_longmode_walk_with_cr4(cr3: u64, cr4: u64, va: u64) {
    crate::serial_write_str("RAYOS_VMM:VMX:LM_WALK_BEGIN va=0x");
    crate::serial_write_hex_u64(va);
    crate::serial_write_str(" cr3=0x");
    crate::serial_write_hex_u64(cr3);
    crate::serial_write_str(" cr4=0x");
    crate::serial_write_hex_u64(cr4);
    crate::serial_write_str("\n");

    let base = cr3 & 0x000F_FFFF_FFFF_F000;
    let la57 = (cr4 & (1u64 << 12)) != 0;
    crate::serial_write_str("RAYOS_VMM:VMX:LM_WALK_BASE=0x");
    crate::serial_write_hex_u64(base);
    crate::serial_write_str(" la57=");
    crate::serial_write_hex_u64(if la57 { 1 } else { 0 });
    crate::serial_write_str("\n");
    if base == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_WALK_CR3_ZERO\n");
        return;
    }

    let pml5_idx = ((va >> 48) & 0x1FF) as u64;
    let pml4_idx = ((va >> 39) & 0x1FF) as u64;
    let pdpt_idx = ((va >> 30) & 0x1FF) as u64;
    let pd_idx = ((va >> 21) & 0x1FF) as u64;
    let pt_idx = ((va >> 12) & 0x1FF) as u64;

    let pml4_base = if la57 {
        let pml5e_pa = base.wrapping_add(pml5_idx * 8);
        let Some(pml5e) = read_u64(pml5e_pa) else {
            crate::serial_write_str("RAYOS_VMM:VMX:LM_PML5E_READ_FAIL\n");
            return;
        };
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PML5E idx=0x");
        crate::serial_write_hex_u64(pml5_idx);
        crate::serial_write_str(" val=0x");
        crate::serial_write_hex_u64(pml5e);
        crate::serial_write_str("\n");
        if (pml5e & 1) == 0 {
            crate::serial_write_str("RAYOS_VMM:VMX:LM_PML5E_NOT_PRESENT\n");
            return;
        }
        pml5e & 0x000F_FFFF_FFFF_F000
    } else {
        base
    };

    let pml4e_pa = pml4_base.wrapping_add(pml4_idx * 8);
    let Some(pml4e) = read_u64(pml4e_pa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PML4E_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:LM_PML4E idx=0x");
    crate::serial_write_hex_u64(pml4_idx);
    crate::serial_write_str(" val=0x");
    crate::serial_write_hex_u64(pml4e);
    crate::serial_write_str("\n");
    if (pml4e & 1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PML4E_NOT_PRESENT\n");
        return;
    }

    let pdpt_base = pml4e & 0x000F_FFFF_FFFF_F000;
    let pdpte_pa = pdpt_base.wrapping_add(pdpt_idx * 8);
    let Some(pdpte) = read_u64(pdpte_pa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDPTE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:LM_PDPTE idx=0x");
    crate::serial_write_hex_u64(pdpt_idx);
    crate::serial_write_str(" val=0x");
    crate::serial_write_hex_u64(pdpte);
    crate::serial_write_str("\n");
    if (pdpte & 1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDPTE_NOT_PRESENT\n");
        return;
    }

    if ((pdpte >> 7) & 1) != 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDPTE_PS_1GB\n");
        return;
    }

    let pd_base = pdpte & 0x000F_FFFF_FFFF_F000;
    let pde_pa = pd_base.wrapping_add(pd_idx * 8);
    let Some(pde) = read_u64(pde_pa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:LM_PDE idx=0x");
    crate::serial_write_hex_u64(pd_idx);
    crate::serial_write_str(" val=0x");
    crate::serial_write_hex_u64(pde);
    crate::serial_write_str("\n");
    if (pde & 1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDE_NOT_PRESENT\n");
        return;
    }

    if ((pde >> 7) & 1) != 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PDE_PS_2MB\n");
        return;
    }

    let pt_base = pde & 0x000F_FFFF_FFFF_F000;
    let pte_pa = pt_base.wrapping_add(pt_idx * 8);
    let Some(pte) = read_u64(pte_pa) else {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PTE_READ_FAIL\n");
        return;
    };
    crate::serial_write_str("RAYOS_VMM:VMX:LM_PTE idx=0x");
    crate::serial_write_hex_u64(pt_idx);
    crate::serial_write_str(" val=0x");
    crate::serial_write_hex_u64(pte);
    crate::serial_write_str("\n");
    if (pte & 1) == 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:LM_PTE_NOT_PRESENT\n");
        return;
    }

    let pa = (pte & 0x000F_FFFF_FFFF_F000) | (va & 0xFFF);
    crate::serial_write_str("RAYOS_VMM:VMX:LM_WALK_PA=0x");
    crate::serial_write_hex_u64(pa);
    crate::serial_write_str("\n");
}

fn try_fixup_longmode_map_2mb(cr3: u64, va: u64, pa: u64) -> bool {
    // Install a 2MB mapping for `va` into the guest's current paging structures if the
    // paging structures exist but the PDE is missing.
    // Bring-up aid only.
    let pml4_base = cr3 & 0xFFFF_FFFF_FFFF_F000;
    if pml4_base == 0 {
        return false;
    }
    let pml4_idx = ((va >> 39) & 0x1FF) as u64;
    let pml4e_pa = pml4_base + pml4_idx * 8;
    static mut LOW_IDENTITY_PDPT_BASE: u64 = 0;

    let mut pml4e = match read_u64(pml4e_pa) {
        Some(v) => v,
        None => return false,
    };

    // If the current PML4 does not map the needed region at all, try to graft back the
    // previously-observed low identity PDPT.
    if (pml4e & 1) == 0 {
        unsafe {
            let ram_top = GUEST_RAM_SIZE_BYTES as u64;

            // For physmap faults (and sometimes for low identity after Linux switches CR3),
            // build fresh paging structures in a reserved guest RAM region so we don't rely
            // on older page tables that Linux may have repurposed.
            let want_fresh_slot = va >= LINUX_X86_64_PHYSMAP_BASE || (pml4_idx == 0 && va < ram_top);
            if want_fresh_slot {
                if let Some(new_pdpt_gpa) = pf_fixup_alloc_zeroed_guest_page() {
                    let new_pml4e = (new_pdpt_gpa & 0xFFFF_FFFF_FFFF_F000) | 0x63;
                    if write_guest_bytes(pml4e_pa, &new_pml4e.to_le_bytes()) {
                        pml4e = new_pml4e;

                        if va >= LINUX_X86_64_PHYSMAP_BASE
                            && PHYSMAP_PML4E_FRESH_LOGGED
                                .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
                                .is_ok()
                        {
                            crate::serial_write_str("RAYOS_VMM:VMX:PF_FIXUP:PHYSMAP_FRESH_PML4E_OK va=0x");
                            crate::serial_write_hex_u64(va);
                            crate::serial_write_str(" cr3=0x");
                            crate::serial_write_hex_u64(cr3);
                            crate::serial_write_str(" pml4e_pa=0x");
                            crate::serial_write_hex_u64(pml4e_pa);
                            crate::serial_write_str(" new_pdpt=0x");
                            crate::serial_write_hex_u64(new_pdpt_gpa);
                            crate::serial_write_str(" new_pml4e=0x");
                            crate::serial_write_hex_u64(new_pml4e);
                            crate::serial_write_str("\n");
                        }

                        // Eagerly map all guest RAM using 2MB pages to avoid a #PF storm.
                        // This is especially important for the Linux physmap direct map.
                        let needed_pdpt = ((ram_top + ((1u64 << 30) - 1)) >> 30) as u64; // ceil / 1GB
                        let max_pdpt = core::cmp::min(512u64, needed_pdpt);
                        for slot in 0..max_pdpt {
                            if let Some(new_pd_gpa) = pf_fixup_alloc_zeroed_guest_page() {
                                // Pre-fill this PD with 2MB mappings for its 1GB chunk.
                                let chunk_base = slot << 30;
                                if chunk_base < ram_top {
                                    let chunk_size = core::cmp::min(1u64 << 30, ram_top - chunk_base);
                                    let pde_count = ((chunk_size + 0x1F_FFFF) >> 21) as u64; // ceil / 2MB
                                    let max_pde = core::cmp::min(512u64, pde_count);
                                    for i in 0..max_pde {
                                        let pa_base_2mb = chunk_base + (i << 21);
                                        let pde = (pa_base_2mb & 0xFFFF_FFFF_FFE0_0000) | 0x1E3;
                                        let _ = write_guest_bytes(new_pd_gpa + i * 8, &pde.to_le_bytes());
                                    }
                                }

                                let new_pdpte = (new_pd_gpa & 0xFFFF_FFFF_FFFF_F000) | 0x63;
                                let _ = write_guest_bytes(new_pdpt_gpa + slot * 8, &new_pdpte.to_le_bytes());
                            } else {
                                break;
                            }
                        }
                    }
                }
            }

            // If we still don't have a PML4E, fall back to grafting back a previously-observed
            // identity PDPT (best-effort).
            if (pml4e & 1) == 0 {
                // For low addresses we only graft slot 0; for physmap addresses, graft the
                // corresponding slot as well so VA->PA behaves like a direct map.
                if va >= LINUX_X86_64_PHYSMAP_BASE && LOW_IDENTITY_PDPT_BASE == 0 {
                    crate::serial_write_str("RAYOS_VMM:VMX:PF_FIXUP:PHYSMAP_PML4E_MISSING_LOW_PDPT_BASE_0 va=0x");
                    crate::serial_write_hex_u64(va);
                    crate::serial_write_str(" cr3=0x");
                    crate::serial_write_hex_u64(cr3);
                    crate::serial_write_str(" pml4_idx=0x");
                    crate::serial_write_hex_u64(pml4_idx);
                    crate::serial_write_str("\n");
                }

                if (pml4_idx == 0 || va >= LINUX_X86_64_PHYSMAP_BASE) && LOW_IDENTITY_PDPT_BASE != 0 {
                    if va >= LINUX_X86_64_PHYSMAP_BASE {
                        crate::serial_write_str("RAYOS_VMM:VMX:PF_FIXUP:GRAFT_PHYSMAP_SLOT va=0x");
                        crate::serial_write_hex_u64(va);
                        crate::serial_write_str(" low_pdpt=0x");
                        crate::serial_write_hex_u64(LOW_IDENTITY_PDPT_BASE);
                        crate::serial_write_str("\n");
                    }
                    // present|rw|user + no other special bits
                    let new_pml4e = (LOW_IDENTITY_PDPT_BASE & 0xFFFF_FFFF_FFFF_F000) | 0x63;
                    if write_guest_bytes(pml4e_pa, &new_pml4e.to_le_bytes()) {
                        pml4e = new_pml4e;
                        if va >= LINUX_X86_64_PHYSMAP_BASE {
                            crate::serial_write_str(
                                "RAYOS_VMM:VMX:PF_FIXUP:GRAFT_PHYSMAP_SLOT_OK new_pml4e=0x",
                            );
                            crate::serial_write_hex_u64(new_pml4e);
                            crate::serial_write_str("\n");
                        }
                    } else if va >= LINUX_X86_64_PHYSMAP_BASE {
                        crate::serial_write_str(
                            "RAYOS_VMM:VMX:PF_FIXUP:GRAFT_PHYSMAP_SLOT_WRITE_FAIL pml4e_pa=0x",
                        );
                        crate::serial_write_hex_u64(pml4e_pa);
                        crate::serial_write_str("\n");
                    }
                }
            }
        }
    }

    if (pml4e & 1) == 0 {
        return false;
    }

    // Remember a working low PDPT base for future CR3 switches.
    unsafe {
        if pml4_idx == 0 {
            LOW_IDENTITY_PDPT_BASE = pml4e & 0xFFFF_FFFF_FFFF_F000;
        }
    }

    let pdpt_base = pml4e & 0xFFFF_FFFF_FFFF_F000;
    let pdpt_idx = ((va >> 30) & 0x1FF) as u64;
    let pdpte_pa = pdpt_base + pdpt_idx * 8;
    let mut pdpte = match read_u64(pdpte_pa) {
        Some(v) => v,
        None => return false,
    };
    if (pdpte & 1) == 0 {
        if va < LINUX_X86_64_PHYSMAP_BASE {
            return false;
        }
        unsafe {
            if let Some(new_pd_gpa) = pf_fixup_alloc_zeroed_guest_page() {
                // Pre-fill this PD with 2MB direct-map PDEs for the portion of guest RAM
                // covered by this PDPT entry. This dramatically reduces physmap page-fault
                // storms during Linux bring-up.
                let chunk_base = pdpt_idx << 30; // 1GB per PDPT slot
                let ram_top = GUEST_RAM_SIZE_BYTES as u64;
                if chunk_base < ram_top {
                    let chunk_size = core::cmp::min(1u64 << 30, ram_top - chunk_base);
                    let pde_count = ((chunk_size + 0x1F_FFFF) >> 21) as u64; // ceil / 2MB
                    let max_pde = core::cmp::min(512u64, pde_count);
                    for i in 0..max_pde {
                        let pa_base_2mb = chunk_base + (i << 21);
                        let pde = (pa_base_2mb & 0xFFFF_FFFF_FFE0_0000) | 0x1E3;
                        let _ = write_guest_bytes(new_pd_gpa + i * 8, &pde.to_le_bytes());
                    }
                }

                let new_pdpte = (new_pd_gpa & 0xFFFF_FFFF_FFFF_F000) | 0x63;
                if write_guest_bytes(pdpte_pa, &new_pdpte.to_le_bytes()) {
                    pdpte = new_pdpte;
                }
            }
        }
    }
    if (pdpte & 1) == 0 {
        return false;
    }

    let pd_base = pdpte & 0xFFFF_FFFF_FFFF_F000;
    let pd_idx = ((va >> 21) & 0x1FF) as u64;
    let pde_pa = pd_base + pd_idx * 8;
    let Some(pde) = read_u64(pde_pa) else {
        return false;
    };

    // If we have an existing PD but it's empty, pre-fill a full set of 2MB mappings for
    // all guest RAM. This avoids spending dozens/hundreds of VM exits on sequential faults.
    if (pde & 1) == 0 {
        let ram_top = GUEST_RAM_SIZE_BYTES as u64;
        let is_physmap = va >= LINUX_X86_64_PHYSMAP_BASE;

        // Low identity: only consider PDPT slot 0.
        if !is_physmap && pdpt_idx == 0 && LOW_IDENTITY_PREFILL_DONE.load(Ordering::Relaxed) == 0 {
            let chunk_base = 0u64;
            let chunk_size = core::cmp::min(1u64 << 30, ram_top.saturating_sub(chunk_base));
            let pde_count = ((chunk_size + 0x1F_FFFF) >> 21) as u64; // ceil / 2MB
            let max_pde = core::cmp::min(512u64, pde_count);
            for i in 0..max_pde {
                let slot_pa = pd_base + i * 8;
                if let Some(existing) = read_u64(slot_pa) {
                    if existing & 1 != 0 {
                        continue;
                    }
                }
                let pa_base_2mb = chunk_base + (i << 21);
                let new_pde = (pa_base_2mb & 0xFFFF_FFFF_FFE0_0000) | 0x1E3;
                let _ = write_guest_bytes(slot_pa, &new_pde.to_le_bytes());
            }
            LOW_IDENTITY_PREFILL_DONE.store(1, Ordering::Relaxed);
            return true;
        }

        // Physmap: similarly prefill PDPT slot 0 once.
        if is_physmap && pdpt_idx == 0 && PHYSMAP_PREFILL_DONE.load(Ordering::Relaxed) == 0 {
            let chunk_base = 0u64;
            let chunk_size = core::cmp::min(1u64 << 30, ram_top.saturating_sub(chunk_base));
            let pde_count = ((chunk_size + 0x1F_FFFF) >> 21) as u64; // ceil / 2MB
            let max_pde = core::cmp::min(512u64, pde_count);
            for i in 0..max_pde {
                let slot_pa = pd_base + i * 8;
                if let Some(existing) = read_u64(slot_pa) {
                    if existing & 1 != 0 {
                        continue;
                    }
                }
                let pa_base_2mb = chunk_base + (i << 21);
                let new_pde = (pa_base_2mb & 0xFFFF_FFFF_FFE0_0000) | 0x1E3;
                let _ = write_guest_bytes(slot_pa, &new_pde.to_le_bytes());
            }
            PHYSMAP_PREFILL_DONE.store(1, Ordering::Relaxed);
            return true;
        }
    }

    if (pde & 1) != 0 {
        // Already present. If it's a 4KB page table (PS=0), it may still fault due to missing PTEs.
        // For physmap bring-up, prefer forcing a 2MB mapping to keep Linux moving forward.
        let ps2m = (pde >> 7) & 1;
        if ps2m != 0 {
            // Mapping is already a 2MB page. For bring-up purposes, treat this as a successful
            // fixup so the caller retries the faulting instruction (with a TLB flush).
            return true;
        }
        if va < LINUX_X86_64_PHYSMAP_BASE {
            return false;
        }
        // Fall through and overwrite with a 2MB PDE.
    }

    // Mirror typical Linux early-boot 2MB PDE flags (present|rw|user|pwt|pcd|accessed|dirty|ps).
    let flags: u64 = 0x1E3;
    let pa_base_2mb = pa & 0xFFFF_FFFF_FFE0_0000;
    let new_pde = pa_base_2mb | flags;
    let bytes = new_pde.to_le_bytes();
    write_guest_bytes(pde_pa, &bytes)
}

fn write_guest_bytes(gpa: u64, data: &[u8]) -> bool {
    if data.is_empty() {
        return true;
    }
    let mut written = 0;
    with_guest_memory_range(gpa, data.len(), |ptr, chunk| {
        for i in 0..chunk {
            unsafe { core::ptr::write_volatile(ptr.add(i), data[written + i]) };
        }
        written += chunk;
        true
    })
}

fn guest_gpa_to_phys(gpa: u64) -> Option<u64> {
    if gpa >= GUEST_RAM_SIZE_BYTES as u64 {
        return None;
    }
    let page = (gpa / PAGE_SIZE as u64) as usize;
    if page >= GUEST_RAM_PAGES {
        return None;
    }
    let base_phys = guest_ram_phys(page);
    if base_phys == 0 {
        return None;
    }
    let offset = gpa % PAGE_SIZE as u64;
    Some(base_phys + offset)
}

fn guest_gpa_to_host_ptr(gpa: u64) -> Option<*mut u8> {
    guest_gpa_to_phys(gpa).map(|phys| crate::phys_to_virt(phys) as *mut u8)
}

fn guest_host_chunk(gpa: u64) -> Option<(*mut u8, usize)> {
    let ptr = guest_gpa_to_host_ptr(gpa)?;
    let offset = (gpa % PAGE_SIZE as u64) as usize;
    let avail = PAGE_SIZE - offset;
    Some((ptr, avail))
}

/// Iterate contiguous guest-memory chunks for a GPA+length range.
fn with_guest_memory_range<F>(mut gpa: u64, mut len: usize, mut op: F) -> bool
where
    F: FnMut(*mut u8, usize) -> bool,
{
    while len > 0 {
        let (ptr, chunk_len) = match guest_host_chunk(gpa) {
            Some(v) => v,
            None => return false,
        };
        let chunk = cmp::min(len, chunk_len);
        if !op(ptr, chunk) {
            return false;
        }
        gpa += chunk as u64;
        len -= chunk;
    }
    true
}

fn write_u16(gpa: u64, value: u16) -> bool {
    write_guest_bytes(gpa, &value.to_le_bytes())
}

fn write_u32(gpa: u64, value: u32) -> bool {
    write_guest_bytes(gpa, &value.to_le_bytes())
}

fn fill_descriptor_payload(addr: u64, len: u32, pattern: u8) -> bool {
    if len == 0 {
        return true;
    }
    let mut remaining = len as usize;
    let mut cur = addr;
    let chunk_buf = [pattern; PAGE_SIZE];
    while remaining > 0 {
        let chunk = cmp::min(remaining, PAGE_SIZE);
        if !write_guest_bytes(cur, &chunk_buf[..chunk]) {
            return false;
        }
        remaining -= chunk;
        cur += chunk as u64;
    }
    true
}

unsafe fn pf_fixup_alloc_zeroed_guest_page() -> Option<u64> {
    let ram_top = GUEST_RAM_SIZE_BYTES as u64;
    let base = ram_top.saturating_sub(PF_FIXUP_PT_RESERVE_BYTES);
    let end = base.saturating_add(PF_FIXUP_PT_RESERVE_BYTES);

    if PF_FIXUP_PT_RESERVE_BYTES < 4096 {
        return None;
    }

    if PF_FIXUP_PT_ALLOC_NEXT_GPA == 0 {
        PF_FIXUP_PT_ALLOC_NEXT_GPA = base;
    }
    let gpa = PF_FIXUP_PT_ALLOC_NEXT_GPA;
    if gpa < base || gpa.saturating_add(4096) > end {
        return None;
    }
    PF_FIXUP_PT_ALLOC_NEXT_GPA = gpa + 4096;
    if !fill_descriptor_payload(gpa, 4096, 0) {
        return None;
    }
    Some(gpa)
}

fn respond_with_status(addr: u64, len: u32, status: u8) -> bool {
    if len == 0 {
        return true;
    }
    if !fill_descriptor_payload(addr, len, 0) {
        return false;
    }
    if !write_guest_bytes(addr + (len as u64) - 1, &[status]) {
        return false;
    }
    let mut read_back = [0u8; 1];
    if read_guest_bytes(addr + (len as u64) - 1, &mut read_back) {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:STATUS_VERIFY=");
        crate::serial_write_hex_u64(read_back[0] as u64);
        crate::serial_write_str("\n");
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:STATUS_VERIFY_FAIL\n");
    }
    true
}

#[derive(Copy, Clone)]
struct VirtioBlkRequest {
    req_type: u32,
    sector: u64,
}

fn read_virtio_blk_request(addr: u64) -> Option<VirtioBlkRequest> {
    let mut buf = [0u8; 16];
    if !read_guest_bytes(addr, &mut buf) {
        return None;
    }
    Some(VirtioBlkRequest {
        req_type: u32::from_le_bytes(buf[0..4].try_into().ok()?),
        sector: u64::from_le_bytes(buf[8..16].try_into().ok()?),
    })
}

fn virtio_blk_request_name(op: u32) -> &'static str {
    match op {
        0 => "VIRTIO_BLK_T_IN",
        1 => "VIRTIO_BLK_T_OUT",
        4 => "VIRTIO_BLK_T_FLUSH",
        8 => "VIRTIO_BLK_T_GET_ID",
        11 => "VIRTIO_BLK_T_BARRIER",
        _ => "UNKNOWN",
    }
}

fn validate_read_descriptor_pattern(desc: &VirtqDesc) {
    let to_check = cmp::min(desc.len as usize, 8);
    if to_check == 0 {
        return;
    }
    let mut buf = [0u8; 8];
    if read_guest_bytes(desc.addr, &mut buf[..to_check]) {
        let ok = buf[..to_check]
            .iter()
            .all(|&b| b == VIRTIO_BLK_READ_PATTERN);
        crate::serial_write_str(if ok {
            "RAYOS_VMM:VIRTIO_MMIO:READ_PATTERN_OK\n"
        } else {
            "RAYOS_VMM:VIRTIO_MMIO:READ_PATTERN_MISMATCH\n"
        });
    }
}

fn virtio_blk_disk_init_once() {
    unsafe {
        if VIRTIO_BLK_DISK_INITIALIZED {
            return;
        }

        #[cfg(feature = "vmm_virtio_blk_image")]
        {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_BLK:DISK_INIT_IMAGE\n");
            let n = core::cmp::min(VIRTIO_BLK_DISK_IMAGE.len(), VIRTIO_BLK_DISK.len());
            VIRTIO_BLK_DISK[..n].copy_from_slice(&VIRTIO_BLK_DISK_IMAGE[..n]);
            for b in VIRTIO_BLK_DISK[n..].iter_mut() {
                *b = 0;
            }
        }

        #[cfg(not(feature = "vmm_virtio_blk_image"))]
        {
            for b in VIRTIO_BLK_DISK.iter_mut() {
                *b = VIRTIO_BLK_READ_PATTERN;
            }
        }
        VIRTIO_BLK_DISK_INITIALIZED = true;
    }
}

fn virtio_blk_disk_range_for_sector(sector: u64) -> Option<usize> {
    let start = (sector as usize).checked_mul(VIRTIO_BLK_SECTOR_SIZE)?;
    if start >= VIRTIO_BLK_DISK_BYTES {
        return None;
    }
    Some(start)
}

fn virtio_blk_read_into_descriptors(sector: u64, descs: &[VirtqDesc]) {
    virtio_blk_disk_init_once();
    let mut disk_off = match virtio_blk_disk_range_for_sector(sector) {
        Some(v) => v,
        None => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_READ_OOB\n");
            return;
        }
    };

    let mut filled_any = false;
    let mut first_byte_logged = false;
    for desc in descs {
        if (desc.flags & VIRTQ_DESC_F_WRITE) == 0 {
            continue;
        }
        filled_any = true;
        let mut remaining = desc.len as usize;
        let mut gpa = desc.addr;
        while remaining > 0 {
            if disk_off >= VIRTIO_BLK_DISK_BYTES {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_READ_OOB\n");
                return;
            }
            let available = VIRTIO_BLK_DISK_BYTES - disk_off;
            let chunk = cmp::min(remaining, cmp::min(available, 256));
            let slice = unsafe { &VIRTIO_BLK_DISK[disk_off..disk_off + chunk] };
            if !write_guest_bytes(gpa, slice) {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_READ_WRITE_FAIL\n");
                return;
            }
            if !first_byte_logged && !slice.is_empty() {
                first_byte_logged = true;
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_READ_FIRST_BYTE=");
                crate::serial_write_hex_u64(slice[0] as u64);
                crate::serial_write_str("\n");
            }
            disk_off += chunk;
            gpa += chunk as u64;
            remaining -= chunk;
        }
    }

    if !filled_any {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:READ_NO_DATA\n");
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_READ_OK\n");
    }
}

fn virtio_blk_write_from_descriptors(sector: u64, descs: &[VirtqDesc]) {
    virtio_blk_disk_init_once();
    let mut disk_off = match virtio_blk_disk_range_for_sector(sector) {
        Some(v) => v,
        None => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_WRITE_OOB\n");
            return;
        }
    };

    let mut wrote_any = false;
    let mut first_byte_logged = false;
    for desc in descs {
        if (desc.flags & VIRTQ_DESC_F_WRITE) != 0 {
            continue;
        }
        wrote_any = true;
        let mut remaining = desc.len as usize;
        let mut gpa = desc.addr;
        let mut tmp = [0u8; 256];
        while remaining > 0 {
            if disk_off >= VIRTIO_BLK_DISK_BYTES {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_WRITE_OOB\n");
                return;
            }
            let available = VIRTIO_BLK_DISK_BYTES - disk_off;
            let chunk = cmp::min(remaining, cmp::min(available, tmp.len()));
            if !read_guest_bytes(gpa, &mut tmp[..chunk]) {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_WRITE_READ_FAIL\n");
                return;
            }
            unsafe {
                VIRTIO_BLK_DISK[disk_off..disk_off + chunk].copy_from_slice(&tmp[..chunk]);
            }
            if !first_byte_logged && chunk > 0 {
                first_byte_logged = true;
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_WRITE_FIRST_BYTE=");
                crate::serial_write_hex_u64(tmp[0] as u64);
                crate::serial_write_str("\n");
            }
            disk_off += chunk;
            gpa += chunk as u64;
            remaining -= chunk;
        }
    }

    if !wrote_any {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:WRITE_NO_DATA\n");
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BLK_WRITE_OK\n");
    }
}

fn fill_read_descriptors(descs: &[VirtqDesc]) {
    // Legacy helper retained for debugging/compat, but normal reads now come from the disk backing.
    let mut filled_any = false;
    for desc in descs {
        if (desc.flags & VIRTQ_DESC_F_WRITE) == 0 {
            continue;
        }
        filled_any = true;
        if fill_descriptor_payload(desc.addr, desc.len, VIRTIO_BLK_READ_PATTERN) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:READ_DESC_FILL_OK\n");
            validate_read_descriptor_pattern(desc);
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:READ_DESC_FILL_FAIL\n");
        }
    }
    if !filled_any {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:READ_NO_DATA\n");
    }
}

fn log_write_descriptors(descs: &[VirtqDesc]) {
    let mut saw_data = false;
    let mut saw_non_zero = false;
    for (idx, desc) in descs.iter().enumerate() {
        if (desc.flags & VIRTQ_DESC_F_WRITE) != 0 {
            continue;
        }
        saw_data = true;
        let to_read = cmp::min(desc.len as usize, MAX_DESC_PAYLOAD_LOG_BYTES);
        if to_read == 0 {
            continue;
        }
        let mut buf = [0u8; MAX_DESC_PAYLOAD_LOG_BYTES];
        if read_guest_bytes(desc.addr, &mut buf[..to_read]) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:WRITE_DESC#");
            crate::serial_write_hex_u64(idx as u64);
            crate::serial_write_str("=");
            for j in 0..to_read {
                crate::serial_write_hex_u64(buf[j] as u64);
                if j + 1 < to_read {
                    crate::serial_write_str(",");
                }
            }
            crate::serial_write_str("\n");
            if buf[..to_read].iter().any(|&b| b != 0) {
                saw_non_zero = true;
            }
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:WRITE_DATA_READ_FAIL\n");
        }
    }
    if !saw_data {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:WRITE_NO_DATA\n");
    } else if saw_non_zero {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:WRITE_DATA_NON_ZERO\n");
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:WRITE_DATA_ZERO\n");
    }
}

fn write_identity_descriptors(descs: &[VirtqDesc]) -> bool {
    let mut wrote = false;
    for desc in descs {
        if (desc.flags & VIRTQ_DESC_F_WRITE) == 0 {
            continue;
        }
        wrote = true;
        let to_write = cmp::min(desc.len as usize, VIRTIO_BLK_IDENTITY.len());
        if to_write == 0 {
            continue;
        }
        if write_guest_bytes(desc.addr, &VIRTIO_BLK_IDENTITY[..to_write]) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:GET_ID_DESC_FILLED\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:GET_ID_DESC_FAIL\n");
        }
    }
    wrote
}

fn handle_virtio_blk_chain(
    header: VirtqDesc,
    data_descs: &[VirtqDesc],
    status_desc: Option<VirtqDesc>,
) {
    if let Some(req) = read_virtio_blk_request(header.addr) {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:REQ_TYPE=");
        crate::serial_write_hex_u64(req.req_type as u64);
        crate::serial_write_str(" name=");
        crate::serial_write_str(virtio_blk_request_name(req.req_type));
        crate::serial_write_str(" sector=");
        crate::serial_write_hex_u64(req.sector);
        crate::serial_write_str("\n");

        match req.req_type {
            0 => {
                virtio_blk_read_into_descriptors(req.sector, data_descs);
            }
            1 => {
                log_write_descriptors(data_descs);
                virtio_blk_write_from_descriptors(req.sector, data_descs);
            }
            4 => {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:FLUSH\n");
            }
            8 => {
                if write_identity_descriptors(data_descs) {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:GET_ID_COMPLETE\n");
                } else {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:GET_ID_NO_DATA\n");
                }
            }
            11 => {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BARRIER\n");
            }
            _ => {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:UNKNOWN_REQ\n");
            }
        }
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:REQ_PARSE_FAIL\n");
    }

    if let Some(status) = status_desc {
        if respond_with_status(status.addr, status.len, VIRTIO_STATUS_OK) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:STATUS_WRITE_OK\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:STATUS_WRITE_FAIL\n");
        }
    }
}

fn handle_virtio_net_chain(queue_id: u16, data_descs: &[VirtqDesc]) {
    // Optionally inject a test packet on startup in network test mode
    #[cfg(feature = "vmm_hypervisor_net_test")]
    {
        static mut TEST_PACKET_INJECTED: bool = false;
        if !unsafe { TEST_PACKET_INJECTED } && queue_id == VIRTIO_NET_RX_QUEUE {
            unsafe {
                TEST_PACKET_INJECTED = true;
            }
            // Create a minimal test Ethernet frame: dest MAC, src MAC, ethertype, payload
            let mut test_pkt = [0u8; 64];
            // Destination MAC: AA:BB:CC:DD:EE:FF
            test_pkt[0] = 0xAA;
            test_pkt[1] = 0xBB;
            test_pkt[2] = 0xCC;
            test_pkt[3] = 0xDD;
            test_pkt[4] = 0xEE;
            test_pkt[5] = 0xFF;
            // Source MAC: 52:55:4F:53:00:01 (RAYOS)
            test_pkt[6] = 0x52;
            test_pkt[7] = 0x55;
            test_pkt[8] = 0x4F;
            test_pkt[9] = 0x53;
            test_pkt[10] = 0x00;
            test_pkt[11] = 0x01;
            // EtherType: 0x0800 (IPv4)
            test_pkt[12] = 0x08;
            test_pkt[13] = 0x00;
            // Payload: pattern
            for i in 14..64 {
                test_pkt[i] = 0x42; // 'B'
            }
            unsafe {
                VIRTIO_NET_LOOPBACK_PKT[..64].copy_from_slice(&test_pkt[..]);
                VIRTIO_NET_LOOPBACK_PKT_LEN = 64;
            }
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_TEST_PKT_INJECTED len=64\n");
        }
    }

    match queue_id {
        VIRTIO_NET_TX_QUEUE => {
            // TX queue: guest sends packets
            let mut pkt_len = 0;
            let mut ethertype = 0u16;
            let mut pkt_buf = [0u8; VIRTIO_NET_PKT_MAX];
            let mut pkt_off = 0usize;

            for (idx, desc) in data_descs.iter().enumerate() {
                if (desc.flags & VIRTQ_DESC_F_WRITE) == 0 {
                    pkt_len += desc.len;
                    // Gather packet bytes for loopback
                    let to_copy = cmp::min(desc.len as usize, VIRTIO_NET_PKT_MAX - pkt_off);
                    if read_guest_bytes(desc.addr, &mut pkt_buf[pkt_off..pkt_off + to_copy]) {
                        pkt_off += to_copy;
                    } else {
                        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_TX_READ_FAIL addr=");
                        crate::serial_write_hex_u64(desc.addr);
                        crate::serial_write_str(" len=");
                        crate::serial_write_hex_u64(desc.len as u64);
                        crate::serial_write_str(" idx=");
                        crate::serial_write_hex_u64(idx as u64);
                        crate::serial_write_str("\n");
                    }
                    // Read Ethernet type from first payload segment
                    if idx == 0 && desc.len > 14 {
                        let mut eth_frame = [0u8; 14];
                        if read_guest_bytes(desc.addr, &mut eth_frame) {
                            ethertype = u16::from_be_bytes([eth_frame[12], eth_frame[13]]);
                        }
                    }
                }
            }
            unsafe {
                VIRTIO_NET_TX_PACKETS = VIRTIO_NET_TX_PACKETS.wrapping_add(1);
            }
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_TX len=");
            crate::serial_write_hex_u64(pkt_len as u64);
            crate::serial_write_str(" type=");
            crate::serial_write_hex_u64(ethertype as u64);
            crate::serial_write_str(" count=");
            crate::serial_write_hex_u64(unsafe { VIRTIO_NET_TX_PACKETS as u64 });
            crate::serial_write_str("\n");

            // Loopback: echo TX to RX (swap MAC addresses)
            if unsafe { VIRTIO_NET_LOOPBACK_ENABLED } && pkt_off >= 12 {
                let mut loopback_pkt = pkt_buf;
                // Swap src/dst MAC addresses (first 12 bytes)
                for i in 0..6 {
                    loopback_pkt.swap(i, i + 6);
                }
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_LOOPBACK len=");
                crate::serial_write_hex_u64(pkt_len as u64);
                crate::serial_write_str("\n");
                // Store packet for RX injection when descriptors are available
                unsafe {
                    VIRTIO_NET_LOOPBACK_PKT = loopback_pkt;
                    VIRTIO_NET_LOOPBACK_PKT_LEN = pkt_off;
                }
            }
        }
        VIRTIO_NET_RX_QUEUE => {
            // RX queue: guest receives packets
            // Check if we have a looped-back packet to inject
            let pkt_len_to_inject = unsafe { VIRTIO_NET_LOOPBACK_PKT_LEN };
            if pkt_len_to_inject > 0 && data_descs.len() > 0 {
                // Take the first RX descriptor and inject packet data
                let desc = &data_descs[0];
                if (desc.flags & VIRTQ_DESC_F_WRITE) != 0 {
                    // This is a writable descriptor (good for RX)
                    let to_write = cmp::min(pkt_len_to_inject, desc.len as usize);
                    if write_guest_bytes(desc.addr, &unsafe { VIRTIO_NET_LOOPBACK_PKT }[..to_write])
                    {
                        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_RX_INJECT len=");
                        crate::serial_write_hex_u64(to_write as u64);
                        crate::serial_write_str("\n");
                        unsafe {
                            VIRTIO_NET_LOOPBACK_PKT_LEN = 0; // Clear buffer after injection
                            VIRTIO_NET_RX_PACKETS = VIRTIO_NET_RX_PACKETS.wrapping_add(1);
                        }
                    } else {
                        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_RX_INJECT_FAIL addr=");
                        crate::serial_write_hex_u64(desc.addr);
                        crate::serial_write_str(" len=");
                        crate::serial_write_hex_u64(to_write as u64);
                        crate::serial_write_str("\n");
                    }
                }
            }
            unsafe {
                if VIRTIO_NET_LOOPBACK_PKT_LEN == 0 {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_RX ready count=");
                    crate::serial_write_hex_u64(VIRTIO_NET_RX_PACKETS as u64);
                    crate::serial_write_str("\n");
                }
            }
        }
        _ => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_UNKNOWN_QUEUE\n");
        }
    }
}

fn read_virtq_descriptor(base: u64, index: u32) -> Option<VirtqDesc> {
    let offset = base.wrapping_add((index as u64) * VIRTQ_DESC_SIZE);
    let mut buf = [0u8; VIRTQ_DESC_SIZE as usize];
    if !read_guest_bytes(offset, &mut buf) {
        return None;
    }
    Some(VirtqDesc {
        // Mask address to 48 bits as a heuristic for guest-provided canonical addresses
        addr: u64::from_le_bytes(buf[0..8].try_into().ok()?) & 0x0000_FFFF_FFFF_FFFF,
        len: u32::from_le_bytes(buf[8..12].try_into().ok()?),
        flags: u16::from_le_bytes(buf[12..14].try_into().ok()?),
        next: u16::from_le_bytes(buf[14..16].try_into().ok()?),
    })
}

fn log_virtq_descriptors(base: u64, queue_size: u32) {
    if queue_size == 0 {
        return;
    }
    let count = if queue_size > MAX_VIRTQ_DESC_TO_LOG {
        MAX_VIRTQ_DESC_TO_LOG
    } else {
        queue_size
    };
    for idx in 0..count {
        if let Some(desc) = read_virtq_descriptor(base, idx) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:DESC#");
            crate::serial_write_hex_u64(idx as u64);
            crate::serial_write_str(" addr=");
            crate::serial_write_hex_u64(desc.addr);
            crate::serial_write_str(" len=");
            crate::serial_write_hex_u64(desc.len as u64);
            crate::serial_write_str(" flags=");
            crate::serial_write_hex_u64(desc.flags as u64);
            crate::serial_write_str(" next=");
            crate::serial_write_hex_u64(desc.next as u64);
            crate::serial_write_str("\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:DESC_INVALID\n");
            break;
        }
    }
}

// debug memory dump removed

#[allow(unreachable_code)]
fn inject_guest_interrupt(vector: u8) -> bool {
    // Allow unit-tests / runtime force to make injection fail.
    if INJECT_FORCE_FAIL.load(Ordering::Relaxed) != 0 {
        crate::serial_write_str("RAYOS_VMM:VMX:FORCED_VMWRITE_FAIL\n");
        return false;
    }

    // Format VM-entry interruption info: valid bit (31) + vector in low bits.
    let _val = (1u64 << 31) | (vector as u64 & 0xFF);

    // Allow forcing a VMWRITE failure for testing fallback paths with a feature flag.
    #[cfg(feature = "vmm_inject_force_fail")]
    {
        crate::serial_write_str("RAYOS_VMM:VMX:FORCED_VMWRITE_FAIL\n");
    }

    #[cfg(not(feature = "vmm_inject_force_fail"))]
    {
        if unsafe { vmwrite(VMCS_ENTRY_INTERRUPTION_INFO, _val) } {
            return true;
        }
    }

    // VM-entry injection failed; try APIC (LAPIC MMIO) as a fallback delivery path.
    crate::serial_write_str("RAYOS_VMM:VMX:VMWRITE_INJECT_FAIL, trying LAPIC fallback\n");

    // In test mode, if LAPIC_MMIO is not yet discovered, set a reasonable default
    // so the fallback path can be exercised in CI/smoke runs.
    #[cfg(feature = "vmm_inject_force_fail")]
    unsafe {
        if LAPIC_MMIO == 0 {
            // HHDM_OFFSET + 0xFEE0_0000 (typical local APIC physical base)
            const FALLBACK_LAPIC_MMIO: u64 = 0xffff_8000_0000_0000u64 + 0xFEE0_0000u64;
            LAPIC_MMIO = FALLBACK_LAPIC_MMIO;
            crate::serial_write_str("RAYOS_VMM:VMX:FORCE_SET_LAPIC_MMIO\n");
        }
    }

    unsafe {
        if LAPIC_MMIO != 0 {
            // In test mode we may *either* simulate a successful LAPIC injection (default),
            // or, when the MSI-forcing feature is enabled too, skip LAPIC simulation so
            // we can exercise the MSI fallback path.
            #[cfg(all(feature = "vmm_inject_force_fail", not(feature = "vmm_inject_force_msi_fail")))]
            {
                crate::serial_write_str("RAYOS_VMM:VMX:INJECT_VIA_LAPIC_SIM\n");
                return true;
            }

            #[cfg(all(feature = "vmm_inject_force_fail", feature = "vmm_inject_force_msi_fail"))]
            {
                crate::serial_write_str("RAYOS_VMM:VMX:SKIP_LAPIC_SIM_TO_EXERCISE_MSI\n");
                // Intentionally fall through without touching LAPIC MMIO so MSI fallback
                // can be exercised in test mode.
            }

            // When not forcing an MSI test, perform the real LAPIC MMIO IPI write.
            #[cfg(not(feature = "vmm_inject_force_msi_fail"))]
            {
                let low = (vector as u32) | (1u32 << 18);
                let reg_high = (LAPIC_MMIO + 0x310) as *mut u32;
                let reg_low = (LAPIC_MMIO + 0x300) as *mut u32;
                core::ptr::write_volatile(reg_high, 0);
                core::ptr::write_volatile(reg_low, low);
                // Read-after-write for posted writes
                let _ = core::ptr::read_volatile(reg_low);
                crate::serial_write_str("RAYOS_VMM:VMX:INJECT_VIA_LAPIC\n");
                return true;
            }
        }
    }

    // Try an MSI-style delivery: write to the canonical MSI target (local APIC address).
    crate::serial_write_str("RAYOS_VMM:VMX:VMWRITE_INJECT_FAIL, trying MSI fallback\n");

    // Allow forced test-mode MSI injection to be simulated (avoid PF in CI when APIC isn't mapped).
    #[cfg(feature = "vmm_inject_force_msi_fail")]
    {
        crate::serial_write_str("RAYOS_VMM:VMX:FORCED_MSI_INJECT\n");
        crate::serial_write_str("RAYOS_VMM:VMX:INJECT_VIA_MSI_SIM\n");
        return true;
    }

    // If test-mode wants to force *all* injection failure, return false here so
    // the backoff logic can be exercised deterministically.
    #[cfg(feature = "vmm_inject_force_all_fail")]
    {
        crate::serial_write_str("RAYOS_VMM:VMX:FORCED_ALL_INJECT_FAIL\n");
        return false;
    }

    // Attempt to perform an MSI by writing to the canonical APIC memory region at
    // phys 0xFEE0_0000 (HHDM_OFFSET + 0xFEE0_0000 -> virtual). This approximates the
    // effect of a device issuing a message-signaled interrupt.
    unsafe {
        let msi_virt = crate::HHDM_OFFSET + 0xFEE0_0000u64;
        let msi_ptr = msi_virt as *mut u32;
        core::ptr::write_volatile(msi_ptr, vector as u32);
        // Read-after-write for posted writes
        let _ = core::ptr::read_volatile(msi_ptr);
        crate::serial_write_str("RAYOS_VMM:VMX:INJECT_VIA_MSI\n");
        return true;
    }

    crate::serial_write_str("RAYOS_VMM:VMX:INJECT_FAIL_NO_FALLBACK\n");
    false
}

fn log_descriptor_chain(
    base: u64,
    queue_size: u32,
    start_index: u32,
    queue_index: u64,
) -> Option<u32> {
    if queue_size == 0 {
        return None;
    }
    let mut idx = start_index;
    let mut total_len = 0u32;
    let mut header_desc: Option<VirtqDesc> = None;
    let mut status_desc: Option<VirtqDesc> = None;
    let mut data_descs = [VirtqDesc::default(); MAX_VIRTIO_DATA_DESCS];
    let mut data_desc_count = 0;
    for _ in 0..MAX_VIRTQ_DESC_CHAIN_ENTRIES {
        if idx >= queue_size {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:DESC_INDEX_OOB\n");
            return None;
        }
        let desc = match read_virtq_descriptor(base, idx) {
            Some(d) => d,
            None => {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:CHAIN_DESC_INVALID\n");
                return None;
            }
        };
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:CHAIN_DESC#");
        crate::serial_write_hex_u64(idx as u64);
        crate::serial_write_str(" addr=");
        crate::serial_write_hex_u64(desc.addr);
        crate::serial_write_str(" len=");
        crate::serial_write_hex_u64(desc.len as u64);
        crate::serial_write_str(" flags=");
        crate::serial_write_hex_u64(desc.flags as u64);
        crate::serial_write_str(" next=");
        crate::serial_write_hex_u64(desc.next as u64);
        crate::serial_write_str("\n");
        log_descriptor_payload(desc.addr, desc.len);
        if header_desc.is_none() {
            header_desc = Some(desc);
        } else if status_desc.is_none() && (desc.flags & VIRTQ_DESC_F_WRITE) != 0 && desc.len == 1 {
            status_desc = Some(desc);
        } else if data_desc_count < MAX_VIRTIO_DATA_DESCS {
            data_descs[data_desc_count] = desc;
            data_desc_count += 1;
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:DESC_TOO_MANY\n");
        }
        total_len = total_len.wrapping_add(desc.len);
        if desc.flags & VIRTQ_DESC_F_NEXT == 0 {
            if let Some(header) = header_desc {
                // Dispatch to handler based on active device ID
                let device_id = VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed);
                // dispatching device (silent)
                match device_id {
                    VIRTIO_MMIO_DEVICE_ID_VALUE => {
                        // Block device (0x0105)
                        // If the chain only contains a single non-writable header descriptor
                        // that actually holds payload (guest may not split header/data),
                        // treat it as a data descriptor so handlers see the bytes.
                        if data_desc_count == 0 {
                            data_descs[0] = header;
                            data_desc_count = 1;
                        }
                        handle_virtio_blk_chain(
                            header,
                            &data_descs[..data_desc_count],
                            status_desc,
                        );
                    }
                    VIRTIO_NET_DEVICE_ID => {
                        // Network device (0x0101) - dispatch based on queue index
                        // If the chain only contains a single non-writable descriptor
                        // that actually holds packet payload, treat it as data.
                        if data_desc_count == 0 {
                            data_descs[0] = header;
                            data_desc_count = 1;
                        }
                        let qid = if queue_index == 1 {
                            VIRTIO_NET_RX_QUEUE
                        } else {
                            VIRTIO_NET_TX_QUEUE
                        };
                        handle_virtio_net_chain(qid, &data_descs[..data_desc_count]);
                    }
                    VIRTIO_GPU_DEVICE_ID => {
                        #[cfg(feature = "vmm_virtio_gpu")]
                        {
                            let used_len = unsafe {
                                handle_virtio_gpu_chain(header, &data_descs[..data_desc_count])
                            };
                            return Some(used_len);
                        }

                        #[cfg(not(feature = "vmm_virtio_gpu"))]
                        {
                            crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:FEATURE_DISABLED\n");
                        }
                    }

                    VIRTIO_INPUT_DEVICE_ID => {
                        #[cfg(feature = "vmm_virtio_input")]
                        {
                            let used_len = unsafe {
                                handle_virtio_input_chain(
                                    header,
                                    &data_descs[..data_desc_count],
                                )
                            };
                            return Some(used_len);
                        }

                        #[cfg(not(feature = "vmm_virtio_input"))]
                        {
                            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:FEATURE_DISABLED\n");
                        }
                    }

                    #[cfg(feature = "vmm_virtio_console")]
                    VIRTIO_CONSOLE_DEVICE_ID => {
                        // VIRTIO console uses separate queues: data queue (0) and control queue (1).
                        // If the chain only contains a single non-writable descriptor that actually
                        // holds payload, treat it as a data descriptor so handlers see the bytes.
                        if data_desc_count == 0 {
                            data_descs[0] = header;
                            data_desc_count = 1;
                        }
                        if queue_index == 0 {
                            let used_len = unsafe { handle_virtio_console_dataq(header, &data_descs[..data_desc_count]) };
                            return Some(used_len);
                        } else {
                            let used_len = unsafe { handle_virtio_console_ctrlq(header, &data_descs[..data_desc_count]) };
                            return Some(used_len);
                        }
                    }
                    _ => {
                        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:UNKNOWN_DEVICE\n");
                    }
                }
            }
            return Some(total_len);
        }
        idx = desc.next as u32;
    }
    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:CHAIN_TOO_LONG\n");
    None
}

#[cfg(feature = "vmm_virtio_input")]
unsafe fn handle_virtio_input_chain(header: VirtqDesc, descs: &[VirtqDesc]) -> u32 {
    // Minimal virtio-input eventq support:
    // - find the first device-writeable descriptor
    // - write a single input event (SYN_REPORT)
    // This is enough to prove the virtqueue transport can deliver an input event.

    const EV_SYN: u16 = 0;
    const SYN_REPORT: u16 = 0;

    let mut target: Option<VirtqDesc> = None;
    if (header.flags & VIRTQ_DESC_F_WRITE) != 0 {
        target = Some(header);
    } else {
        for d in descs {
            if (d.flags & VIRTQ_DESC_F_WRITE) != 0 {
                target = Some(*d);
                break;
            }
        }
    }

    let Some(out_desc) = target else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:NO_WRITABLE_DESC\n");
        return 0;
    };

    if out_desc.len < 8 {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:DESC_TOO_SMALL\n");
        return 0;
    }

    let mut ev = [0u8; 8];
    ev[0..2].copy_from_slice(&EV_SYN.to_le_bytes());
    ev[2..4].copy_from_slice(&SYN_REPORT.to_le_bytes());
    ev[4..8].copy_from_slice(&0u32.to_le_bytes());

    if write_guest_bytes(out_desc.addr, &ev) {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:EVENT_WRITTEN\n");
        8
    } else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:EVENT_WRITE_FAIL\n");
        0
    }
}

#[cfg(feature = "vmm_virtio_gpu")]
unsafe fn handle_virtio_gpu_chain(header: VirtqDesc, descs: &[VirtqDesc]) -> u32 {
    // virtio-gpu controlq: request buffer(s) are device-readable, response buffer is device-writable.
    // We accept the common 2-descriptor layout: OUT req + IN resp.
    let req = header;
    let mut resp: Option<VirtqDesc> = None;

    // Some guests may place the response buffer in the following descriptors.
    for d in descs {
        if (d.flags & VIRTQ_DESC_F_WRITE) != 0 {
            resp = Some(*d);
            break;
        }

        // If the header is writable (unusual), treat it as the response.
        if (req.flags & VIRTQ_DESC_F_WRITE) != 0 {
            resp = Some(req);
            break;
        }
    }

    let Some(resp_desc) = resp else {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:NO_RESP_DESC\n");
        return 0;
    };

    // Run the controlq handler and log result; the device model will publish
    // scanout/meta updates and call GuestScanoutPublisher.frame_ready() which
    // emits the first-frame marker on transition.
    let written = VIRTIO_GPU_DEVICE.handle_controlq_gpa(
        req.addr,
        req.len as usize,
        resp_desc.addr,
        resp_desc.len as usize,
        guest_gpa_to_phys,
    );
    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:CONTROLQ_DONE\n");
    written as u32
}

#[cfg(feature = "vmm_virtio_console")]
unsafe fn handle_virtio_console_chain(_header: VirtqDesc, descs: &[VirtqDesc]) -> u32 {
    // Minimal implementation: for each non-writeable descriptor (guest->device),
    // copy bytes from guest physical memory and emit them on the host serial.
    if descs.len() == 0 {
        return 0;
    }

    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:CHAIN_HANDLED\n");

    for d in descs.iter() {
        // If VIRTQ_DESC_F_WRITE is set, the descriptor is device-writeable (host->guest),
        // skip for now as we don't produce responses yet.
        if (d.flags & VIRTQ_DESC_F_WRITE) != 0 {
            continue;
        }
        let phys = d.addr;
        let len = d.len as usize;
        if len == 0 {
            continue;
        }
        let src = crate::phys_to_virt(phys) as *const u8;
        let slice = core::slice::from_raw_parts(src, len);
        // Prefix so CI can detect console output in serial log.
        crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:RECV:");
        crate::serial_write_bytes(slice);
        crate::serial_write_str("\n");
    }

        // No response bytes written.
    0
}

// Handle virtio-console data queue (guest->device messages and optional device->guest responses)
#[cfg(feature = "vmm_virtio_console")]
unsafe fn handle_virtio_console_dataq(_header: VirtqDesc, descs: &[VirtqDesc]) -> u32 {
    if descs.len() == 0 {
        return 0;
    }
    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:DATAQ_HANDLED\n");
    let mut total_consumed: u32 = 0;
    for d in descs.iter() {
        // Device-readable (guest->device)
        if (d.flags & VIRTQ_DESC_F_WRITE) == 0 {
            let len = d.len as usize;
            if len == 0 {
                continue;
            }
            let mut buf = [0u8; 256];
            let to_read = if len > buf.len() { buf.len() } else { len };
            if read_guest_bytes(d.addr, &mut buf[..to_read]) {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:RECV:");
                crate::serial_write_bytes(&buf[..to_read]);
                crate::serial_write_str("\n");
                total_consumed = total_consumed.wrapping_add(to_read as u32);
            } else {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:RECV_FAIL\n");
            }
        } else {
            // Device-writeable: provide a simple status response (fill zeros and set status 0)
            if d.len > 0 {
                if respond_with_status(d.addr, d.len, 0) {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:RESP_WRITTEN\n");
                } else {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:RESP_WRITE_FAIL\n");
                }
                total_consumed = total_consumed.wrapping_add(d.len);
            }
        }
    }
    total_consumed
}

// Handle virtio-console control queue (control messages + optional responses).
#[cfg(feature = "vmm_virtio_console")]
unsafe fn handle_virtio_console_ctrlq(_header: VirtqDesc, descs: &[VirtqDesc]) -> u32 {
    use crate::virtio_console_proto::VirtioConsoleCtrlHdr;
    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:CTRLQ_HANDLED\n");
    // Expect the header (first readable descriptor) to contain a VirtioConsoleCtrlHdr
    let mut consumed = 0u32;
    let mut resp_written = false;
    for d in descs.iter() {
        if (d.flags & VIRTQ_DESC_F_WRITE) == 0 {
            // Read control header if present
            if d.len as usize >= core::mem::size_of::<VirtioConsoleCtrlHdr>() {
                let mut buf = [0u8; core::mem::size_of::<VirtioConsoleCtrlHdr>()];
                if read_guest_bytes(d.addr, &mut buf) {
                    // parse fields
                    let type_ = u32::from_le_bytes(buf[0..4].try_into().unwrap());
                    let flags = u32::from_le_bytes(buf[4..8].try_into().unwrap());
                    let id = u32::from_le_bytes(buf[8..12].try_into().unwrap());
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:CTRL:type=");
                    crate::serial_write_hex_u64(type_ as u64);
                    crate::serial_write_str(" flags=");
                    crate::serial_write_hex_u64(flags as u64);
                    crate::serial_write_str(" id=");
                    crate::serial_write_hex_u64(id as u64);
                    crate::serial_write_str("\n");
                } else {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:CTRL_READ_FAIL\n");
                }
            }
            consumed = consumed.wrapping_add(d.len);
        } else {
            // Write a small response: acknowledge with a zero status in the last byte
            if d.len > 0 {
                if respond_with_status(d.addr, d.len, 0) {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:CTRL_RESP_WRITTEN\n");
                    resp_written = true;
                } else {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:CTRL_RESP_FAIL\n");
                }
                consumed = consumed.wrapping_add(d.len);
            }
        }
    }

    // If we wrote a response, report it as consumed; otherwise report the bytes read.
    if resp_written { consumed } else { consumed }
}

#[cfg(all(feature = "vmm_virtio_console", feature = "vmm_virtio_console_selftest", feature = "vmm_hypervisor_smoke"))]
unsafe fn run_virtio_console_selftest() {
    // Allocate a page for a small message, write ascii text, craft a single
    // readable descriptor and invoke the console *data* handler directly.
    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:SELFTEST:ALLOC_PAGE\n");
    let msg_phys = match crate::phys_alloc_page() {
        Some(p) => p,
        None => {
            crate::serial_write_str("RAYOS_VIRTIO_CONSOLE:SELFTEST:ALLOC_FAIL\n");
            return;
        }
    };

    // Prepare message bytes
    let msg = b"virtio-console selftest\n";
    let dst = crate::phys_to_virt(msg_phys) as *mut u8;
    core::ptr::copy_nonoverlapping(msg.as_ptr(), dst, msg.len());

    // Craft a virtq desc pointing to the message buffer (non-writeable by device)
    let desc = VirtqDesc { addr: msg_phys, len: msg.len() as u32, flags: 0, next: 0 };
    crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:SELFTEST:INVOKE\n");
    let _ = handle_virtio_console_dataq(VirtqDesc { addr: 0, len: 0, flags: 0, next: 0 }, &[desc]);
}


// Self-test / smoke routine to exercise the virtio-gpu model without a real
// guest driver. Allocates a few guest-physical pages and performs GET_DISPLAY_INFO
// -> RESOURCE_CREATE_2D -> RESOURCE_ATTACH_BACKING -> SET_SCANOUT -> RESOURCE_FLUSH
// which should trigger scanout publish and a frame-ready marker.
#[cfg(all(feature = "vmm_virtio_gpu", feature = "vmm_hypervisor_smoke"))]
unsafe fn run_virtio_gpu_selftest() {
    use crate::virtio_gpu_proto as proto;
    use core::mem;

    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:alloc_pages\n");
    let req_phys = match crate::phys_alloc_page() { Some(p) => p, None => { crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:ALLOC_REQ_FAIL\n"); return; } };
    let resp_phys = match crate::phys_alloc_page() { Some(p) => p, None => { crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:ALLOC_RESP_FAIL\n"); return; } };
    let backing_phys = match crate::phys_alloc_page() { Some(p) => p, None => { crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:ALLOC_BACKING_FAIL\n"); return; } };

    // Zero backing page and write a recognizable pattern so developers can inspect.
    core::ptr::write_bytes(crate::phys_to_virt(backing_phys) as *mut u8, 0xAA, 4096);

    // 1) GET_DISPLAY_INFO
    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:GET_DISPLAY_INFO\n");
    let hdr = proto::VirtioGpuCtrlHdr { type_: proto::VIRTIO_GPU_CMD_GET_DISPLAY_INFO, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 };
    core::ptr::write_unaligned(crate::phys_to_virt(req_phys) as *mut proto::VirtioGpuCtrlHdr, hdr);
    let _ = VIRTIO_GPU_DEVICE.handle_controlq(req_phys, mem::size_of::<proto::VirtioGpuCtrlHdr>(), resp_phys, mem::size_of::<proto::VirtioGpuRespDisplayInfo>());
    let resp: proto::VirtioGpuRespDisplayInfo = core::ptr::read_unaligned(crate::phys_to_virt(resp_phys) as *const proto::VirtioGpuRespDisplayInfo);
    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:GET_DISPLAY_INFO_RESP=");
    crate::serial_write_hex_u64(resp.hdr.type_ as u64);
    crate::serial_write_str("\n");

    // 2) RESOURCE_CREATE_2D (resource id = 1)
    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:CREATE_2D\n");
    let create = proto::VirtioGpuResourceCreate2d { hdr: proto::VirtioGpuCtrlHdr { type_: proto::VIRTIO_GPU_CMD_RESOURCE_CREATE_2D, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 }, resource_id: 1, format: proto::VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM, width: 64, height: 64 };
    core::ptr::write_unaligned(crate::phys_to_virt(req_phys) as *mut proto::VirtioGpuResourceCreate2d, create);
    let _ = VIRTIO_GPU_DEVICE.handle_controlq(req_phys, mem::size_of::<proto::VirtioGpuResourceCreate2d>(), resp_phys, mem::size_of::<proto::VirtioGpuCtrlHdr>());

    // 3) RESOURCE_ATTACH_BACKING (hdr + one entry)
    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:ATTACH_BACKING\n");
    let attach_hdr = proto::VirtioGpuResourceAttachBackingHdr { hdr: proto::VirtioGpuCtrlHdr { type_: proto::VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 }, resource_id: 1, nr_entries: 1 };
    core::ptr::write_unaligned(crate::phys_to_virt(req_phys) as *mut proto::VirtioGpuResourceAttachBackingHdr, attach_hdr);
    let entry_ptr = (crate::phys_to_virt(req_phys) as u64 + mem::size_of::<proto::VirtioGpuResourceAttachBackingHdr>() as u64) as *mut proto::VirtioGpuMemEntry;
    core::ptr::write_unaligned(entry_ptr, proto::VirtioGpuMemEntry { addr: backing_phys, length: 4096u32, padding: 0 });
    let _ = VIRTIO_GPU_DEVICE.handle_controlq(req_phys, mem::size_of::<proto::VirtioGpuResourceAttachBackingHdr>() + mem::size_of::<proto::VirtioGpuMemEntry>(), resp_phys, mem::size_of::<proto::VirtioGpuCtrlHdr>());

    // 4) SET_SCANOUT (scanout -> resource 1)
    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:SET_SCANOUT\n");
    let setscan = proto::VirtioGpuSetScanout { hdr: proto::VirtioGpuCtrlHdr { type_: proto::VIRTIO_GPU_CMD_SET_SCANOUT, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 }, rect: proto::VirtioGpuRect { x: 0, y: 0, width: 64, height: 64 }, scanout_id: 0, resource_id: 1 };
    core::ptr::write_unaligned(crate::phys_to_virt(req_phys) as *mut proto::VirtioGpuSetScanout, setscan);
    let _ = VIRTIO_GPU_DEVICE.handle_controlq(req_phys, mem::size_of::<proto::VirtioGpuSetScanout>(), resp_phys, mem::size_of::<proto::VirtioGpuCtrlHdr>());

    // 5) RESOURCE_FLUSH (should trigger frame_ready)
    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:RESOURCE_FLUSH\n");
    let flush = proto::VirtioGpuResourceFlush { hdr: proto::VirtioGpuCtrlHdr { type_: proto::VIRTIO_GPU_CMD_RESOURCE_FLUSH, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 }, rect: proto::VirtioGpuRect { x: 0, y: 0, width: 64, height: 64 }, resource_id: 1, padding: 0 };
    core::ptr::write_unaligned(crate::phys_to_virt(req_phys) as *mut proto::VirtioGpuResourceFlush, flush);
    let _ = VIRTIO_GPU_DEVICE.handle_controlq(req_phys, mem::size_of::<proto::VirtioGpuResourceFlush>(), resp_phys, mem::size_of::<proto::VirtioGpuCtrlHdr>());

    crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST:END\n");
}

// Backoff selftest helper: run when built with `vmm_inject_backoff_selftest`.
#[cfg(feature = "vmm_inject_backoff_selftest")]
fn run_inject_backoff_selftest() {
    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_BEGIN\n");

    // Ensure the test is deterministic by forcing all injection paths to fail.
    #[cfg(not(feature = "vmm_inject_force_all_fail"))]
    {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_SKIP_NO_FORCE_FLAG\n");
        return;
    }

    const MAX_INT_INJECT_ATTEMPTS_LOCAL: u32 = 5;
    VIRTIO_MMIO_STATE.interrupt_pending.store(1, Ordering::Relaxed);
    VIRTIO_MMIO_STATE
        .interrupt_pending_attempts
        .store(0, Ordering::Relaxed);
    let start = crate::TIMER_TICKS.load(Ordering::Relaxed);
    VIRTIO_MMIO_STATE
        .interrupt_pending_last_tick
        .store(start, Ordering::Relaxed);

    for _ in 0..(MAX_INT_INJECT_ATTEMPTS_LOCAL + 2) {
        // Advance time beyond the backoff window to force a retry path each loop.
        let now = crate::TIMER_TICKS.load(Ordering::Relaxed).wrapping_add(256);
        crate::TIMER_TICKS.store(now, Ordering::Relaxed);
        try_retry_pending_if_due();
    }

    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_END\n");
}

fn log_descriptor_payload(addr: u64, len: u32) {
    if len == 0 {
        return;
    }
    let to_read = cmp::min(len as usize, MAX_DESC_PAYLOAD_LOG_BYTES);
    let mut buf = [0u8; MAX_DESC_PAYLOAD_LOG_BYTES];
    if !read_guest_bytes(addr, &mut buf[..to_read]) {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:PAYLOAD_READ_FAIL\n");
        return;
    }
    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:DESC_PAYLOAD=");
    for i in 0..to_read {
        crate::serial_write_hex_u64(buf[i] as u64);
        if i + 1 < to_read {
            crate::serial_write_str(",");
        }
    }
    crate::serial_write_str("\n");
}

fn log_virtq_avail(base: u64, queue_size: u32) {
    if queue_size == 0 || base == 0 {
        return;
    }
    let flags = match read_u16(base) {
        Some(v) => v,
        None => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_INVALID\n");
            return;
        }
    };
    let idx = match read_u16(base + 2) {
        Some(v) => v,
        None => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_INVALID\n");
            return;
        }
    };
    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_FLAGS=");
    crate::serial_write_hex_u64(flags as u64);
    crate::serial_write_str(" idx=");
    crate::serial_write_hex_u64(idx as u64);
    crate::serial_write_str("\n");

    let count = queue_size.min(MAX_VIRTQ_DESC_TO_LOG);
    for entry in 0..count {
        let offset = base + 4 + (entry as u64) * 2;
        let desc_index = match read_u16(offset) {
            Some(v) => v,
            None => {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_ENTRY_INVALID\n");
                break;
            }
        };
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_ENTRY#");
        crate::serial_write_hex_u64(entry as u64);
        crate::serial_write_str(" desc=");
        crate::serial_write_hex_u64(desc_index as u64);
        crate::serial_write_str("\n");
    }
}

fn log_virtq_used(base: u64, queue_size: u32) {
    if queue_size == 0 || base == 0 {
        return;
    }
    let flags = match read_u16(base) {
        Some(v) => v,
        None => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_INVALID\n");
            return;
        }
    };
    let idx = match read_u16(base + 2) {
        Some(v) => v,
        None => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_INVALID\n");
            return;
        }
    };
    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_FLAGS=");
    crate::serial_write_hex_u64(flags as u64);
    crate::serial_write_str(" idx=");
    crate::serial_write_hex_u64(idx as u64);
    crate::serial_write_str("\n");

    let count = queue_size.min(MAX_VIRTQ_DESC_TO_LOG);
    for entry in 0..count {
        let offset = base + 4 + (entry as u64) * 8;
        let mut buf = [0u8; 8];
        if !read_guest_bytes(offset, &mut buf) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_ENTRY_INVALID\n");
            break;
        }
        let mut id_buf = [0u8; 4];
        let mut len_buf = [0u8; 4];
        id_buf.copy_from_slice(&buf[0..4]);
        len_buf.copy_from_slice(&buf[4..8]);
        let id = u32::from_le_bytes(id_buf);
        let len = u32::from_le_bytes(len_buf);
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_ENTRY#");
        crate::serial_write_hex_u64(entry as u64);
        crate::serial_write_str(" id=");
        crate::serial_write_hex_u64(id as u64);
        crate::serial_write_str(" len=");
        crate::serial_write_hex_u64(len as u64);
        crate::serial_write_str("\n");
    }
}

fn decode_mmio_instruction(bytes: &[u8], ilen: usize, gpa: u64) -> Option<MmioInstruction> {
    if ilen >= 5 {
        match bytes[0] {
            0xA1 => {
                let imm = u32::from_le_bytes(bytes[1..5].try_into().ok()?) as u64;
                if imm == gpa {
                    return Some(MmioInstruction {
                        kind: MmioAccessKind::Read,
                        size: 4,
                        address: imm,
                    });
                }
            }
            0xA3 => {
                let imm = u32::from_le_bytes(bytes[1..5].try_into().ok()?) as u64;
                if imm == gpa {
                    return Some(MmioInstruction {
                        kind: MmioAccessKind::Write,
                        size: 4,
                        address: imm,
                    });
                }
            }
            _ => {}
        }
    }
    if ilen >= 10 && bytes[0] == 0x48 && bytes[1] == 0xA1 {
        let imm = u64::from_le_bytes(bytes[2..10].try_into().ok()?);
        if imm == gpa {
            return Some(MmioInstruction {
                kind: MmioAccessKind::Read,
                size: 8,
                address: imm,
            });
        }
    }
    if ilen >= 10 && bytes[0] == 0x48 && bytes[1] == 0xA3 {
        let imm = u64::from_le_bytes(bytes[2..10].try_into().ok()?);
        if imm == gpa {
            return Some(MmioInstruction {
                kind: MmioAccessKind::Write,
                size: 8,
                address: imm,
            });
        }
    }
    None
}

fn emulate_mmio_access(
    region: &MmioRegion,
    regs: &mut GuestRegs,
    gpa: u64,
    qual: u64,
    grip: u64,
    ilen: u64,
    ok_len: bool,
    ok_grip: bool,
) -> bool {
    let read = (qual & 1) != 0;
    let write = (qual & 2) != 0;
    if !read && !write {
        return false;
    }
    let mut instr = [0u8; 16];
    if !read_guest_bytes(grip, &mut instr) {
        return false;
    }
    let instr_len = ilen as usize;
    if instr_len == 0 || instr_len > instr.len() {
        return false;
    }
    let decoded = match decode_mmio_instruction(&instr[..instr_len], instr_len, gpa) {
        Some(d) => d,
        None => return false,
    };
    if decoded.address != gpa {
        return false;
    }
    if (decoded.kind == MmioAccessKind::Read && !read)
        || (decoded.kind == MmioAccessKind::Write && !write)
    {
        return false;
    }
    let offset = gpa - region.base;
    if offset >= region.size {
        return false;
    }
    let size = decoded.size;
    let mask = match size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        8 => u64::MAX,
        _ => return false,
    };
    let access = MmioAccess {
        offset,
        size,
        kind: decoded.kind,
        reg: MmioRegister::Rax,
    };
    let write_value = if access.kind == MmioAccessKind::Write {
        Some(regs.rax & mask)
    } else {
        None
    };
    let result = (region.handler)(regs, &access, write_value);
    if access.kind == MmioAccessKind::Read {
        let out = result.unwrap_or(0) & mask;
        regs.rax = out;
    }
    if ok_len && ok_grip {
        let _ = unsafe { vmwrite(GUEST_RIP, grip.wrapping_add(ilen)) };
    }
    crate::serial_write_str("RAYOS_VMM:VMX:MMIO_HANDLED\n");
    true
}

unsafe fn vmcs_write_or_log(field: u64, value: u64) -> bool {
    if vmwrite(field, value) {
        true
    } else {
        crate::serial_write_str("RAYOS_VMM:VMX:VMWRITE_FAIL field=0x");
        crate::serial_write_hex_u64(field);
        crate::serial_write_str(" value=0x");
        crate::serial_write_hex_u64(value);
        crate::serial_write_str("\n");
        false
    }
}

unsafe fn setup_vmcs_minimal_host_and_controls() {
    crate::serial_write_str("RAYOS_VMM:VMX:VMCS_SETUP_BEGIN\n");

    // Link pointer must be -1 for VMCSs not using shadowing.
    let _ = vmcs_write_or_log(VMCS_LINK_POINTER, u64::MAX);

    // Controls: pick MSRs based on "true controls" availability.
    let (msr_pin, msr_cpu, msr_exit, msr_entry) = if vmx_has_true_controls() {
        (
            IA32_VMX_TRUE_PINBASED_CTLS,
            IA32_VMX_TRUE_PROCBASED_CTLS,
            IA32_VMX_TRUE_EXIT_CTLS,
            IA32_VMX_TRUE_ENTRY_CTLS,
        )
    } else {
        (
            IA32_VMX_PINBASED_CTLS,
            IA32_VMX_PROCBASED_CTLS,
            IA32_VMX_EXIT_CTLS,
            IA32_VMX_ENTRY_CTLS,
        )
    };

    let linux_guest = {
        #[cfg(feature = "vmm_linux_guest")]
        {
            unsafe { LINUX_GUEST_ENTRY_RIP != 0 && LINUX_GUEST_BOOT_PARAMS_GPA != 0 }
        }
        #[cfg(not(feature = "vmm_linux_guest"))]
        {
            false
        }
    };

    // Required bits are forced by adjust_vmx_controls.
    // - Request HLT exiting (CPU-based bit 7) so our trivial guest reliably VM-exits.
    // - Request CPUID exiting + I/O bitmaps so we can emulate guest-visible CPU/port I/O.
    // - Request MSR bitmaps so we can avoid trapping most RDMSR while still trapping all WRMSR.
    // - Request 64-bit host mode for VM-exit (exit ctl bit 9).
    // - By default, request an IA-32e guest (entry ctl bit 9). When booting a Linux bzImage
    //   directly via the boot protocol, enter at the 32-bit protected-mode entrypoint instead.
    // - Request save/load IA32_EFER on exit (exit ctl bits 20/21).
    // For debugging hangs in the Linux guest, request the VMX-preemption timer (if supported)
    // so we can force periodic VM-exits even when the guest is executing pure compute loops.
    let desired_pin = if linux_guest { PIN_CTL_VMX_PREEMPTION_TIMER } else { 0 };
    let pin = adjust_vmx_controls(msr_pin, desired_pin);
    let exit_ctl = adjust_vmx_controls(
        msr_exit,
        EXIT_CTL_HOST_ADDR_SPACE_SIZE | EXIT_CTL_SAVE_IA32_EFER | EXIT_CTL_LOAD_IA32_EFER,
    );
    // Request secondary controls so we can enable EPT as the next foundation step.
    // For the Linux guest, keep CPUID exiting enabled so we can present a stable,
    // minimal CPUID surface (avoiding host-specific feature leaks like LA57).
    let desired_cpu = if linux_guest {
        // Debugging aid: exit on HLT so we can observe whether Linux is stuck in idle/wfi.
        // (The HLT exit handler is throttled to avoid log spam.)
        CPU_CTL_HLT_EXITING
            | CPU_CTL_CPUID_EXITING
            | CPU_CTL_USE_IO_BITMAPS
            | CPU_CTL_USE_MSR_BITMAPS
            | CPU_CTL_ACTIVATE_SECONDARY_CONTROLS
    } else {
        CPU_CTL_HLT_EXITING
            | CPU_CTL_CPUID_EXITING
            | CPU_CTL_USE_IO_BITMAPS
            | CPU_CTL_USE_MSR_BITMAPS
            | CPU_CTL_ACTIVATE_SECONDARY_CONTROLS
    };
    let cpu = adjust_vmx_controls(msr_cpu, desired_cpu);
    let desired_entry = if linux_guest {
        // Enter the Linux bzImage using the 64-bit boot protocol entry (loaded kernel + 0x200).
        // This requires an IA-32e guest and a consistent EFER/CR0/CR4/CR3 setup.
        ENTRY_CTL_IA32E_MODE_GUEST | ENTRY_CTL_LOAD_IA32_EFER
    } else {
        ENTRY_CTL_IA32E_MODE_GUEST | ENTRY_CTL_LOAD_IA32_EFER
    };
    let entry_ctl = adjust_vmx_controls(msr_entry, desired_entry);

    crate::serial_write_str("RAYOS_VMM:VMX:CTL_PIN=0x");
    crate::serial_write_hex_u64(pin as u64);
    crate::serial_write_str("\n");
    crate::serial_write_str("RAYOS_VMM:VMX:CTL_CPU=0x");
    crate::serial_write_hex_u64(cpu as u64);
    crate::serial_write_str("\n");
    crate::serial_write_str("RAYOS_VMM:VMX:CTL_EXIT=0x");
    crate::serial_write_hex_u64(exit_ctl as u64);
    crate::serial_write_str("\n");
    crate::serial_write_str("RAYOS_VMM:VMX:CTL_ENTRY=0x");
    crate::serial_write_hex_u64(entry_ctl as u64);
    crate::serial_write_str("\n");

    let _ = vmcs_write_or_log(PIN_BASED_VM_EXEC_CONTROL, pin as u64);
    let _ = vmcs_write_or_log(CPU_BASED_VM_EXEC_CONTROL, cpu as u64);
    let _ = vmcs_write_or_log(VM_EXIT_CONTROLS, exit_ctl as u64);
    let _ = vmcs_write_or_log(VM_ENTRY_CONTROLS, entry_ctl as u64);

    // Read back key controls to catch any unexpected adjustments.
    let (ok_cpu_ctl, cpu_ctl_rb) = vmread(CPU_BASED_VM_EXEC_CONTROL);
    if ok_cpu_ctl {
        crate::serial_write_str("RAYOS_VMM:VMX:VMCS_CPU_CTL=0x");
        crate::serial_write_hex_u64(cpu_ctl_rb);
        crate::serial_write_str("\n");
    }

    // Exception bitmap: a set bit requests a VM-exit on the corresponding exception vector.
    //
    // For the Linux guest, we generally want the guest to handle its own exceptions.
    // However, trapping *rare* fatal exceptions (#UD/#GP) is extremely useful for diagnosing
    // early-boot hangs: we can log the faulting RIP/insn bytes, then reflect the exception
    // back into the guest so it can still oops/panic normally.
    let exception_bitmap = if linux_guest {
        // #UD (6), #GP (13), #PF (14)
        (1u32 << 6) | (1u32 << 13) | (1u32 << 14)
    } else {
        // #GP (13), #PF (14)
        (1u32 << 13) | (1u32 << 14)
    };
    let _ = vmcs_write_or_log(EXCEPTION_BITMAP, exception_bitmap as u64);

    // Arm VMX-preemption timer if enabled.
    if (pin as u32 & PIN_CTL_VMX_PREEMPTION_TIMER) != 0 {
        let _ = vmcs_write_or_log(VMX_PREEMPTION_TIMER_VALUE, 0x0100_0000);
    }

    if (cpu & CPU_CTL_USE_IO_BITMAPS) != 0 {
        if ensure_io_bitmaps() {
            let _ = vmcs_write_or_log(IO_BITMAP_A, unsafe { IO_BITMAP_A_PHYS });
            let _ = vmcs_write_or_log(IO_BITMAP_B, unsafe { IO_BITMAP_B_PHYS });
        } else {
            crate::serial_write_str("RAYOS_VMM:VMX:ALLOC_IO_BITMAPS_FAIL\n");
        }
    }

    if (cpu & CPU_CTL_USE_MSR_BITMAPS) != 0 {
        if ensure_msr_bitmaps() {
            let _ = vmcs_write_or_log(MSR_BITMAPS, unsafe { MSR_BITMAPS_PHYS });
        } else {
            crate::serial_write_str("RAYOS_VMM:VMX:ALLOC_MSR_BITMAPS_FAIL\n");
        }
    }

    // Some control bits imply additional VMCS fields must be valid.
    // Program IA32_PAT fields if requested by the adjusted controls.
    // (We mirror the host PAT into the guest for bring-up.)
    if (exit_ctl & (EXIT_CTL_SAVE_IA32_PAT | EXIT_CTL_LOAD_IA32_PAT)) != 0
        || (entry_ctl & ENTRY_CTL_LOAD_IA32_PAT) != 0
    {
        let pat = crate::rdmsr(IA32_PAT);
        let _ = vmcs_write_or_log(HOST_IA32_PAT, pat);
        let _ = vmcs_write_or_log(GUEST_IA32_PAT, pat);
    }

    // If the CPU-based controls enable/require secondary controls, program them too.
    // Bit 31 == "activate secondary controls".
    if (cpu & (1 << 31)) != 0 {
        let linux_guest = {
            #[cfg(feature = "vmm_linux_guest")]
            {
                unsafe { LINUX_GUEST_ENTRY_RIP != 0 && LINUX_GUEST_BOOT_PARAMS_GPA != 0 }
            }
            #[cfg(not(feature = "vmm_linux_guest"))]
            {
                false
            }
        };
        let desired_cpu2 = if linux_guest {
            CPU2_CTL_ENABLE_EPT
                | CPU2_CTL_UNRESTRICTED_GUEST
                | CPU2_CTL_ENABLE_INVPCID
                | CPU2_CTL_ENABLE_RDTSCP
        } else {
            CPU2_CTL_ENABLE_EPT
        };
        let cpu2 = adjust_vmx_controls(IA32_VMX_PROCBASED_CTLS2, desired_cpu2);
        crate::serial_write_str("RAYOS_VMM:VMX:CTL_CPU2=0x");
        crate::serial_write_hex_u64(cpu2 as u64);
        crate::serial_write_str("\n");
        let _ = vmcs_write_or_log(SECONDARY_VM_EXEC_CONTROL, cpu2 as u64);

        if (cpu2 & CPU2_CTL_ENABLE_EPT) != 0 {
            if let Some(eptp) = build_guest_ram_ept() {
                crate::serial_write_str("RAYOS_VMM:VMX:EPTP=0x");
                crate::serial_write_hex_u64(eptp);
                crate::serial_write_str("\n");
                let _ = vmcs_write_or_log(EPT_POINTER, eptp);
            } else {
                crate::serial_write_str("RAYOS_VMM:VMX:ALLOC_EPT_FAIL\n");
            }
        }
    }

    // Host state fields.
    let host_cr0 = read_cr0();
    let host_cr4 = read_cr4();
    // We don't have a direct CR3 read helper in this module; use asm.
    let host_cr3: u64;
    asm!("mov {0}, cr3", out(reg) host_cr3, options(nomem, nostack, preserves_flags));

    let _ = vmcs_write_or_log(HOST_CR0, host_cr0);
    let _ = vmcs_write_or_log(HOST_CR3, host_cr3);
    let _ = vmcs_write_or_log(HOST_CR4, host_cr4);

    let cs = read_seg_selector_cs();
    let ss = read_seg_selector_ss();
    let ds = read_seg_selector_ds();
    let es = read_seg_selector_es();
    let fs = read_seg_selector_fs();
    let gs = read_seg_selector_gs();
    let tr = read_seg_selector_tr();

    let _ = vmcs_write_or_log(HOST_CS_SELECTOR, cs as u64);
    let _ = vmcs_write_or_log(HOST_SS_SELECTOR, ss as u64);
    let _ = vmcs_write_or_log(HOST_DS_SELECTOR, ds as u64);
    let _ = vmcs_write_or_log(HOST_ES_SELECTOR, es as u64);
    let _ = vmcs_write_or_log(HOST_FS_SELECTOR, fs as u64);
    let _ = vmcs_write_or_log(HOST_GS_SELECTOR, gs as u64);
    let _ = vmcs_write_or_log(HOST_TR_SELECTOR, tr as u64);

    // Bases.
    let gdtr = read_gdtr();
    let idtr = read_idtr();
    let fs_base = crate::rdmsr(IA32_FS_BASE);
    let gs_base = crate::rdmsr(IA32_GS_BASE);

    // Decode TR base from the active GDT.
    let tr_base = seg_desc_base_from_gdt(tr, gdtr.base);

    let _ = vmcs_write_or_log(HOST_GDTR_BASE, gdtr.base);
    let _ = vmcs_write_or_log(HOST_IDTR_BASE, idtr.base);
    let _ = vmcs_write_or_log(HOST_FS_BASE, fs_base);
    let _ = vmcs_write_or_log(HOST_GS_BASE, gs_base);
    let _ = vmcs_write_or_log(HOST_TR_BASE, tr_base);

    // Sysenter.
    let sysenter_cs = crate::rdmsr(IA32_SYSENTER_CS);
    let sysenter_esp = crate::rdmsr(IA32_SYSENTER_ESP);
    let sysenter_eip = crate::rdmsr(IA32_SYSENTER_EIP);
    let _ = vmcs_write_or_log(HOST_IA32_SYSENTER_CS, sysenter_cs);
    let _ = vmcs_write_or_log(HOST_IA32_SYSENTER_ESP, sysenter_esp);
    let _ = vmcs_write_or_log(HOST_IA32_SYSENTER_EIP, sysenter_eip);

    // Host EFER.
    let host_efer = crate::rdmsr(IA32_EFER);
    let _ = vmcs_write_or_log(HOST_IA32_EFER, host_efer);

    // Host RIP/RSP: point at a known stub.
    let rsp = (&raw const VMX_HOST_STACK.0 as u64) + (VMX_STACK_SIZE as u64);
    let rip = vmx_exit_stub as *const () as u64;
    let _ = vmcs_write_or_log(HOST_RSP, rsp);
    let _ = vmcs_write_or_log(HOST_RIP, rip);

    crate::serial_write_str("RAYOS_VMM:VMX:VMCS_SETUP_DONE\n");
}

unsafe fn setup_vmcs_minimal_guest_state() {
    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_SETUP_BEGIN\n");

    let linux_guest = {
        #[cfg(feature = "vmm_linux_guest")]
        {
            unsafe { LINUX_GUEST_ENTRY_RIP != 0 && LINUX_GUEST_BOOT_PARAMS_GPA != 0 }
        }
        #[cfg(not(feature = "vmm_linux_guest"))]
        {
            false
        }
    };

    // WARNING: The smoke guest mirrors host paging/address space.
    // For Linux bzImage boot protocol (64-bit), enter at load+0x200 with paging enabled.
    let guest_pml4_gpa = guest_ram_gpa(0);
    if linux_guest {
        let cr0_fixed0 = crate::rdmsr(IA32_VMX_CR0_FIXED0);
        let cr0_fixed1 = crate::rdmsr(IA32_VMX_CR0_FIXED1);
        let cr4_fixed0 = crate::rdmsr(IA32_VMX_CR4_FIXED0);
        let cr4_fixed1 = crate::rdmsr(IA32_VMX_CR4_FIXED1);

        // 64-bit boot protocol requires paging enabled with IA-32e active.
        // Provide an identity-mapped 4-level page table rooted at PML4.
        let desired_cr0 = (1u64 << 0)
            | (1u64 << 1)
            | (1u64 << 4)
            | (1u64 << 5)
            | (1u64 << 31); // PE|MP|ET|NE|PG
        let desired_cr4 = (1u64 << 5); // PAE

        let guest_cr0 = (desired_cr0 | cr0_fixed0) & cr0_fixed1;
        let guest_cr4 = (desired_cr4 | cr4_fixed0) & cr4_fixed1;

        let _ = vmcs_write_or_log(GUEST_CR0, guest_cr0);
        let _ = vmcs_write_or_log(GUEST_CR3, guest_pml4_gpa);

        // Many CPUs require CR4.VMXE=1 while running in VMX non-root, via IA32_VMX_CR4_FIXED0.
        // Linux does not expect VMXE to be set, and will attempt to write CR4 without it.
        // Virtualize it away using mask+shadow, while keeping the actual guest CR4 VMXE-compliant.
        const CR4_VMXE: u64 = 1u64 << 13;
        let actual_cr4 = guest_cr4 | CR4_VMXE;
        let _ = vmcs_write_or_log(GUEST_CR4, actual_cr4);
        let _ = vmcs_write_or_log(CR4_GUEST_HOST_MASK, CR4_VMXE);
        let _ = vmcs_write_or_log(CR4_READ_SHADOW, guest_cr4 & !CR4_VMXE);
        let _ = vmcs_write_or_log(CR0_GUEST_HOST_MASK, 0);
        let _ = vmcs_write_or_log(CR0_READ_SHADOW, guest_cr0);
    } else {
        let guest_cr0 = read_cr0();
        let guest_cr4 = read_cr4();
        let _ = vmcs_write_or_log(GUEST_CR0, guest_cr0);
        let _ = vmcs_write_or_log(GUEST_CR3, guest_pml4_gpa);
        let _ = vmcs_write_or_log(GUEST_CR4, guest_cr4);

        let _ = vmcs_write_or_log(CR0_GUEST_HOST_MASK, 0);
        let _ = vmcs_write_or_log(CR4_GUEST_HOST_MASK, 0);
        let _ = vmcs_write_or_log(CR0_READ_SHADOW, guest_cr0);
        let _ = vmcs_write_or_log(CR4_READ_SHADOW, guest_cr4);
    }

    let _ = vmcs_write_or_log(GUEST_RFLAGS, 0x2);
    let guest_code_rip = guest_ram_gpa(GUEST_CODE_PAGE_INDEX);
    let guest_rip = {
        #[cfg(feature = "vmm_linux_guest")]
        {
            if LINUX_GUEST_ENTRY_RIP != 0 {
                LINUX_GUEST_ENTRY_RIP
            } else {
                guest_code_rip
            }
        }
        #[cfg(not(feature = "vmm_linux_guest"))]
        {
            guest_code_rip
        }
    };
    let _ = vmcs_write_or_log(GUEST_RIP, guest_rip);
    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_RIP=0x");
    crate::serial_write_hex_u64(guest_rip);
    crate::serial_write_str("\n");

    // Sanity: confirm the guest code bytes we installed are still present (guest-driver mode only).
    #[cfg(not(feature = "vmm_linux_guest"))]
    {
        // For `out imm8, al` the port immediate is the following byte.
        let code_phys = guest_ram_phys(GUEST_CODE_PAGE_INDEX);
        let code_ptr = crate::phys_to_virt(code_phys) as *const u8;
        let out0_imm = core::ptr::read_volatile(code_ptr.add(3)) as u64;
        let out1_imm = core::ptr::read_volatile(code_ptr.add(7)) as u64;
        crate::serial_write_str("RAYOS_VMM:VMX:GUEST_CODE_OUT_IMM0=0x");
        crate::serial_write_hex_u64(out0_imm);
        crate::serial_write_str("\n");
        crate::serial_write_str("RAYOS_VMM:VMX:GUEST_CODE_OUT_IMM1=0x");
        crate::serial_write_hex_u64(out1_imm);
        crate::serial_write_str("\n");
    }
    let guest_stack_top = guest_ram_gpa(GUEST_STACK_START_INDEX + GUEST_STACK_PAGES);
    let _ = vmcs_write_or_log(GUEST_RSP, guest_stack_top - 0x10);
    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_STACK_TOP=0x");
    crate::serial_write_hex_u64(guest_stack_top);
    crate::serial_write_str("\n");

    // Guest EFER must be consistent with VM-entry controls.
    let efer = if linux_guest {
        // LME=1 and LMA=1 for IA-32e guest with paging enabled.
        (1u64 << 8) | (1u64 << 10)
    } else {
        crate::rdmsr(IA32_EFER)
    };
    let _ = vmcs_write_or_log(GUEST_IA32_EFER, efer);

    let (code_selector, data_selector) = if linux_guest {
        (LINUX_BOOT_CS_SELECTOR as u64, LINUX_BOOT_DS_SELECTOR as u64)
    } else {
        (GUEST_CODE_SELECTOR as u64, GUEST_DATA_SELECTOR as u64)
    };

    let _ = vmcs_write_or_log(GUEST_CS_SELECTOR, code_selector);
    let _ = vmcs_write_or_log(GUEST_SS_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_DS_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_ES_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_FS_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_GS_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(
        GUEST_TR_SELECTOR,
        if linux_guest {
            LINUX_TSS_SELECTOR as u64
        } else {
            GUEST_TSS_SELECTOR as u64
        },
    );
    let _ = vmcs_write_or_log(GUEST_LDTR_SELECTOR, 0);

    let _ = vmcs_write_or_log(GUEST_CS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_SS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_DS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_ES_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_FS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_GS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_TR_BASE, unsafe { GUEST_TSS_PHYS_VALUE });
    let _ = vmcs_write_or_log(GUEST_LDTR_BASE, 0);

    let seg_limit = if linux_guest { 0xFFFF_FFFF } else { GUEST_SEGMENT_LIMIT_VALUE };
    let _ = vmcs_write_or_log(GUEST_CS_LIMIT, seg_limit);
    let _ = vmcs_write_or_log(GUEST_SS_LIMIT, seg_limit);
    let _ = vmcs_write_or_log(GUEST_DS_LIMIT, seg_limit);
    let _ = vmcs_write_or_log(GUEST_ES_LIMIT, seg_limit);
    let _ = vmcs_write_or_log(GUEST_FS_LIMIT, seg_limit);
    let _ = vmcs_write_or_log(GUEST_GS_LIMIT, seg_limit);
    let _ = vmcs_write_or_log(
        GUEST_TR_LIMIT,
        unsafe { GUEST_TSS_LIMIT_VALUE },
    );
    let _ = vmcs_write_or_log(GUEST_LDTR_LIMIT, 0);

    let cs_ar = if linux_guest { LINUX_CS_AR_VALUE } else { GUEST_CS_AR_VALUE };
    let _ = vmcs_write_or_log(GUEST_CS_AR_BYTES, cs_ar);
    let _ = vmcs_write_or_log(GUEST_SS_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_DS_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_ES_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_FS_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_GS_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(
        GUEST_TR_AR_BYTES,
        unsafe { GUEST_TSS_AR_VALUE },
    );
    let _ = vmcs_write_or_log(GUEST_LDTR_AR_BYTES, 1u64 << 16);

    let guest_gdt_gpa = guest_ram_gpa(GUEST_GDT_PAGE_INDEX);
    let guest_idt_gpa = guest_ram_gpa(GUEST_IDT_PAGE_INDEX);
    let _ = vmcs_write_or_log(GUEST_GDTR_BASE, guest_gdt_gpa);
    let _ = vmcs_write_or_log(GUEST_IDTR_BASE, if linux_guest { 0 } else { guest_idt_gpa });
    let _ = vmcs_write_or_log(
        GUEST_GDTR_LIMIT,
        if linux_guest {
            LINUX_GDT_LIMIT_VALUE
        } else {
            GUEST_GDT_LIMIT_VALUE
        },
    );
    let _ = vmcs_write_or_log(GUEST_IDTR_LIMIT, if linux_guest { 0 } else { GUEST_IDTR_LIMIT_VALUE });

    let _ = vmcs_write_or_log(GUEST_DR7, 0x400);

    log_guest_vmcs_state();

    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_SETUP_DONE\n");
}

unsafe fn log_guest_vmcs_state() {
    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_SEGMENT_STATE\n");
    let fields: &[(u64, &str)] = &[
        (VM_ENTRY_CONTROLS, "VM_ENTRY_CONTROLS"),
        (GUEST_CR0, "GUEST_CR0"),
        (GUEST_CR3, "GUEST_CR3"),
        (GUEST_CR4, "GUEST_CR4"),
        (GUEST_IA32_EFER, "GUEST_IA32_EFER"),
        (GUEST_CS_SELECTOR, "GUEST_CS_SELECTOR"),
        (GUEST_SS_SELECTOR, "GUEST_SS_SELECTOR"),
        (GUEST_DS_SELECTOR, "GUEST_DS_SELECTOR"),
        (GUEST_TR_SELECTOR, "GUEST_TR_SELECTOR"),
        (GUEST_CS_BASE, "GUEST_CS_BASE"),
        (GUEST_SS_BASE, "GUEST_SS_BASE"),
        (GUEST_DS_BASE, "GUEST_DS_BASE"),
        (GUEST_TR_BASE, "GUEST_TR_BASE"),
        (GUEST_CS_LIMIT, "GUEST_CS_LIMIT"),
        (GUEST_DS_LIMIT, "GUEST_DS_LIMIT"),
        (GUEST_TR_LIMIT, "GUEST_TR_LIMIT"),
        (GUEST_CS_AR_BYTES, "GUEST_CS_AR_BYTES"),
        (GUEST_DS_AR_BYTES, "GUEST_DS_AR_BYTES"),
        (GUEST_TR_AR_BYTES, "GUEST_TR_AR_BYTES"),
        (GUEST_GDTR_LIMIT, "GUEST_GDTR_LIMIT"),
        (GUEST_IDTR_LIMIT, "GUEST_IDTR_LIMIT"),
    ];
    for (field, label) in fields {
        let (ok, value) = vmread(*field);
        if ok {
            crate::serial_write_str("RAYOS_VMM:VMX:");
            crate::serial_write_str(label);
            crate::serial_write_str("=0x");
            crate::serial_write_hex_u64(value);
            crate::serial_write_str("\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VMX:VMREAD_FAIL field=");
            crate::serial_write_hex_u64(*field);
            crate::serial_write_str("\n");
        }
    }
}

fn try_retry_pending_if_due() {
    if VIRTIO_MMIO_STATE.interrupt_pending.load(Ordering::Relaxed) == 0 {
        return;
    }

    let attempts = VIRTIO_MMIO_STATE
        .interrupt_pending_attempts
        .load(Ordering::Relaxed);
    let last_tick = VIRTIO_MMIO_STATE
        .interrupt_pending_last_tick
        .load(Ordering::Relaxed);
    // Exponential backoff base (in timer ticks). This is intentionally small
    // for unit tests; real deployments can increase the cap if necessary.
    let base_backoff: u64 = 1;
    let max_backoff: u64 = 128;
    // Compute backoff: min(2^attempts * base, max_backoff)
    let mut backoff = base_backoff.wrapping_shl(attempts.min(63));
    if backoff == 0 {
        backoff = max_backoff;
    }
    if backoff > max_backoff {
        backoff = max_backoff;
    }
    let now = crate::TIMER_TICKS.load(Ordering::Relaxed);
    // If we've already exceeded attempts cap, give up.
    if attempts >= MAX_INT_INJECT_ATTEMPTS {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_FAILED_MAX\n");
        VIRTIO_MMIO_STATE.interrupt_pending.store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_attempts
            .store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_last_tick
            .store(0, Ordering::Relaxed);
        return;
    }

    if now.wrapping_sub(last_tick) < backoff {
        // Not yet time to retry; report wait interval for diagnostic purposes.
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_WAITING\n");
        return;
    }

    // Time to retry.
    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_ATTEMPT\n");
    if inject_guest_interrupt(VIRTIO_MMIO_IRQ_VECTOR) {
        VIRTIO_MMIO_STATE.interrupt_pending.store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_attempts
            .store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_last_tick
            .store(0, Ordering::Relaxed);
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_RETRY_OK\n");
    } else {
        let next = attempts.wrapping_add(1);
        VIRTIO_MMIO_STATE
            .interrupt_pending_attempts
            .store(next, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_last_tick
            .store(now, Ordering::Relaxed);
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_RETRY_FAIL\n");
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_ATTEMPT=");
        crate::serial_write_hex_u64(next as u64);
        crate::serial_write_str("\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::Ordering;

    #[test]
    fn backoff_retry_unit_test() {
        // Force injects to fail using the in-crate test hook.
        INJECT_FORCE_FAIL.store(1, Ordering::Relaxed);

        // Initialize pending state.
        VIRTIO_MMIO_STATE.interrupt_pending.store(1, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_attempts
            .store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_last_tick
            .store(0, Ordering::Relaxed);

        // Reset metrics.
        VIRTIO_MMIO_STATE
            .interrupt_backoff_total_attempts
            .store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_backoff_succeeded
            .store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_backoff_failed_max
            .store(0, Ordering::Relaxed);

        // Run retries until the helper gives up.
        for i in 0..(MAX_INT_INJECT_ATTEMPTS + 2) {
            // Advance timer sufficiently each iteration to allow a retry.
            crate::TIMER_TICKS.store(i as u64 * 256, Ordering::Relaxed);
            try_retry_pending_if_due();
        }

        let failed = VIRTIO_MMIO_STATE
            .interrupt_backoff_failed_max
            .load(Ordering::Relaxed);
        assert!(failed >= 1, "expected at least one failed-max event");

        let total = VIRTIO_MMIO_STATE
            .interrupt_backoff_total_attempts
            .load(Ordering::Relaxed);
        assert!(total >= MAX_INT_INJECT_ATTEMPTS, "expected attempts >= MAX");

        // Ensure no successful injections were recorded.
        let succ = VIRTIO_MMIO_STATE
            .interrupt_backoff_succeeded
            .load(Ordering::Relaxed);
        assert_eq!(succ, 0);

        INJECT_FORCE_FAIL.store(0, Ordering::Relaxed);
    }
}

fn process_virtq_queue(
    desc_base: u64,
    driver_base: u64,
    used_base: u64,
    queue_size: u32,
    queue_ready: u32,
    queue_index: u64,
) {
    if queue_size == 0 || queue_ready == 0 {
        return;
    }
    if desc_base == 0 || driver_base == 0 || used_base == 0 {
        return;
    }

    // Early backoff retry attempt: try a pending interrupt retry if due before
    // processing the queue to avoid starving retries when the guest isn't making
    // queue changes.
    try_retry_pending_if_due();
    let queue_size_u64 = queue_size as u64;
    // Index into per-queue arrays (cap to available queues)
    let qi = if queue_index as usize >= 2 {
        0
    } else {
        queue_index as usize
    };
    // Attempt retry of pending interrupt injections if needed. Uses a bounded retry
    // counter to avoid tight infinite retries and to provide more deterministic logs.
    const MAX_INT_INJECT_ATTEMPTS: u32 = 5;

fn try_retry_pending_if_due() {
    if VIRTIO_MMIO_STATE.interrupt_pending.load(Ordering::Relaxed) == 0 {
        return;
    }

    let attempts = VIRTIO_MMIO_STATE
        .interrupt_pending_attempts
        .load(Ordering::Relaxed);
    let last_tick = VIRTIO_MMIO_STATE
        .interrupt_pending_last_tick
        .load(Ordering::Relaxed);
    // Exponential backoff base (in timer ticks). This is intentionally small
    // for unit tests; real deployments can increase the cap if necessary.
    let base_backoff: u64 = 1;
    let max_backoff: u64 = 128;
    // Compute backoff: min(2^attempts * base, max_backoff)
    let mut backoff = base_backoff.wrapping_shl(attempts.min(63));
    if backoff == 0 {
        backoff = max_backoff;
    }
    if backoff > max_backoff {
        backoff = max_backoff;
    }
    let now = crate::TIMER_TICKS.load(Ordering::Relaxed);
    // If we've already exceeded attempts cap, give up.
    if attempts >= MAX_INT_INJECT_ATTEMPTS {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_FAILED_MAX\n");
        VIRTIO_MMIO_STATE.interrupt_pending.store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_attempts
            .store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_last_tick
            .store(0, Ordering::Relaxed);
        return;
    }

    if now.wrapping_sub(last_tick) < backoff {
        // Not yet time to retry; report wait interval for diagnostic purposes.
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_WAITING\n");
        return;
    }

    // Time to retry.
    crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_ATTEMPT\n");
    if inject_guest_interrupt(VIRTIO_MMIO_IRQ_VECTOR) {
        VIRTIO_MMIO_STATE.interrupt_pending.store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_attempts
            .store(0, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_last_tick
            .store(0, Ordering::Relaxed);
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_RETRY_OK\n");
    } else {
        let next = attempts.wrapping_add(1);
        VIRTIO_MMIO_STATE
            .interrupt_pending_attempts
            .store(next, Ordering::Relaxed);
        VIRTIO_MMIO_STATE
            .interrupt_pending_last_tick
            .store(now, Ordering::Relaxed);
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_RETRY_FAIL\n");
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_ATTEMPT=");
        crate::serial_write_hex_u64(next as u64);
        crate::serial_write_str("\n");
    }
}



    let mut avail_processed: u16 = VIRTIO_MMIO_STATE.queue_avail_index[qi].load(Ordering::Relaxed);
    let mut used_idx: u16 = VIRTIO_MMIO_STATE.queue_used_index[qi].load(Ordering::Relaxed);
    let avail_idx = match read_u16(driver_base + VIRTQ_AVAIL_INDEX_OFFSET) {
        Some(v) => v,
        None => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_IDX_READ_FAIL\n");
            return;
        }
    };
    while avail_processed != avail_idx {
        let ring_pos = (avail_processed as u64) % queue_size_u64;
        let entry_offset =
            driver_base + VIRTQ_AVAIL_RING_OFFSET + ring_pos * VIRTQ_AVAIL_ENTRY_SIZE;
        let desc_index = match read_u16(entry_offset) {
            Some(v) => v,
            None => {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_ENTRY_READ_FAIL\n");
                break;
            }
        };
        if desc_index as u32 >= queue_size {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_DESC_OOB\n");
            break;
        }

        // virtio-input eventq: take and stash writable buffers for later completion.
        #[cfg(feature = "vmm_virtio_input")]
        {
            let device_id = VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed);
            if device_id == VIRTIO_INPUT_DEVICE_ID && qi == VIRTIO_INPUT_EVENTQ_INDEX {
                if let Some(buf) =
                    virtio_input_extract_writable_buf(desc_base, queue_size, desc_index as u32)
                {
                    // If there are no queued events yet and no other free buffers, complete
                    // exactly one keepalive SYN_REPORT to keep headless smoke deterministic.
                    let free_empty = unsafe { VIRTIO_INPUT_FREE_HEAD == VIRTIO_INPUT_FREE_TAIL };
                    if virtio_input_eventq_is_empty() && free_empty {
                        // EV_SYN / SYN_REPORT / 0
                        let keepalive = virtio_input_pack(0, 0, 0);
                        if buf.len >= 8 {
                            let bytes = virtio_input_unpack_bytes(keepalive);
                            if write_guest_bytes(buf.addr, &bytes)
                                && unsafe {
                                    virtio_input_complete_used(
                                        used_base,
                                        queue_size,
                                        qi,
                                        buf.desc_id,
                                        8,
                                    )
                                }
                            {
                                crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:KEEPALIVE_WRITTEN\n");
                                avail_processed = avail_processed.wrapping_add(1);
                                continue;
                            }
                        }
                    }

                    unsafe {
                        if virtio_input_freebuf_push(buf) {
                            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:BUF_STASHED\n");
                        } else {
                            crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:BUF_STASH_FAIL\n");
                        }
                    }
                } else {
                    crate::serial_write_str("RAYOS_VMM:VIRTIO_INPUT:NO_WRITABLE_DESC\n");
                }

                // Always advance the device-side avail cursor for taken buffers.
                avail_processed = avail_processed.wrapping_add(1);
                continue;
            }
        }

        let total_len =
            match log_descriptor_chain(desc_base, queue_size, desc_index as u32, queue_index) {
                Some(len) => len,
                None => break,
            };
        let used_ring_pos = (used_idx as u64) % queue_size_u64;
        let used_entry_offset =
            used_base + VIRTQ_USED_RING_OFFSET + used_ring_pos * VIRTQ_USED_ENTRY_SIZE;
        if !write_u32(used_entry_offset, desc_index as u32) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_ENTRY_WRITE_FAIL\n");
            break;
        }
        if !write_u32(used_entry_offset + 4, total_len) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_LEN_WRITE_FAIL\n");
            break;
        }
        let mut verify_buf = [0u8; 8];
        if read_guest_bytes(used_entry_offset, &mut verify_buf) {
            let verified_id = u32::from_le_bytes(verify_buf[0..4].try_into().unwrap());
            let verified_len = u32::from_le_bytes(verify_buf[4..8].try_into().unwrap());
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_ENTRY_READBACK=id=");
            crate::serial_write_hex_u64(verified_id as u64);
            crate::serial_write_str(" len=");
            crate::serial_write_hex_u64(verified_len as u64);
            crate::serial_write_str("\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_ENTRY_READBACK_FAIL\n");
        }
        used_idx = used_idx.wrapping_add(1);
        if !write_u16(used_base + VIRTQ_USED_IDX_OFFSET, used_idx) {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_IDX_WRITE_FAIL\n");
            break;
        }

        // Publish a device interrupt status bit for VRING updates.
        let old_int = VIRTIO_MMIO_STATE
            .interrupt_status
            .fetch_or(VIRTIO_MMIO_INT_VRING, Ordering::Relaxed);
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_STATUS_SET old=");
        crate::serial_write_hex_u64(old_int as u64);
        crate::serial_write_str(" new=");
        crate::serial_write_hex_u64((old_int | VIRTIO_MMIO_INT_VRING) as u64);
        crate::serial_write_str("\n");

        // If this VRING interrupt bit was newly set, inject a VM-entry interrupt
        // so the guest will observe the IRQ (minimal injection path for P0).
        if (old_int & VIRTIO_MMIO_INT_VRING) == 0 {
            if inject_guest_interrupt(VIRTIO_MMIO_IRQ_VECTOR) {
                crate::serial_write_str("RAYOS_VMM:VMX:INJECT_IRQ_VEC=0x");
                crate::serial_write_hex_u64(VIRTIO_MMIO_IRQ_VECTOR as u64);
                crate::serial_write_str("\n");
            } else {
                crate::serial_write_str("RAYOS_VMM:VMX:INJECT_IRQ_FAIL\n");
                // Mark pending so retries can be attempted later when VM environment
                // might be ready (e.g., after APIC is initialized by guest).
                VIRTIO_MMIO_STATE.interrupt_pending.store(1, Ordering::Relaxed);
                VIRTIO_MMIO_STATE
                    .interrupt_pending_attempts
                    .store(0, Ordering::Relaxed);
                VIRTIO_MMIO_STATE
                    .interrupt_pending_last_tick
                    .store(crate::TIMER_TICKS.load(Ordering::Relaxed), Ordering::Relaxed);
                crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:INT_INJECT_PENDING\n");
            }
        }
        let mut idx_buf = [0u8; 2];
        if read_guest_bytes(used_base + VIRTQ_USED_IDX_OFFSET, &mut idx_buf) {
            let verified_idx = u16::from_le_bytes(idx_buf);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_IDX_READBACK=");
            crate::serial_write_hex_u64(verified_idx as u64);
            crate::serial_write_str("\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_IDX_READBACK_FAIL\n");
        }
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:USED_WRITE id=");
        crate::serial_write_hex_u64(desc_index as u64);
        crate::serial_write_str(" len=");
        crate::serial_write_hex_u64(total_len as u64);
        crate::serial_write_str(" idx=");
        crate::serial_write_hex_u64(used_idx as u64);
        crate::serial_write_str("\n");
        avail_processed = avail_processed.wrapping_add(1);
    }
    VIRTIO_MMIO_STATE.queue_avail_index[qi].store(avail_processed, Ordering::Relaxed);
    VIRTIO_MMIO_STATE.queue_used_index[qi].store(used_idx, Ordering::Relaxed);

    #[cfg(feature = "vmm_virtio_input")]
    unsafe {
        // If this device is virtio-input, try to pump queued events into stashed buffers.
        virtio_input_pump_queue0();
    }
}

#[inline(always)]
fn vmx_revision_id() -> u32 {
    (crate::rdmsr(IA32_VMX_BASIC) & 0x7fff_ffff) as u32
}

fn cpu_supports_vmx() -> bool {
    // CPUID(1): ECX[5] == VMX
    let r = cpuid(1, 0);
    (r.ecx & (1 << 5)) != 0
}

fn try_enable_vmx_feature_control() -> bool {
    let fc = crate::rdmsr(IA32_FEATURE_CONTROL);
    if (fc & FC_LOCK) != 0 {
        // If locked, it must already allow VMXON outside SMX.
        return (fc & FC_VMXON_OUTSIDE_SMX) != 0;
    }

    // Not locked: enable VMXON outside SMX and lock.
    // NOTE: On some real systems firmware locks this MSR; QEMU typically allows it.
    let new_fc = fc | FC_VMXON_OUTSIDE_SMX | FC_LOCK;
    crate::wrmsr(IA32_FEATURE_CONTROL, new_fc);
    true
}

fn apply_vmx_fixed_bits() {
    // Intel requires CR0/CR4 fixed bits to satisfy the VMX constraints.
    let cr0_fixed0 = crate::rdmsr(IA32_VMX_CR0_FIXED0);
    let cr0_fixed1 = crate::rdmsr(IA32_VMX_CR0_FIXED1);
    let cr4_fixed0 = crate::rdmsr(IA32_VMX_CR4_FIXED0);
    let cr4_fixed1 = crate::rdmsr(IA32_VMX_CR4_FIXED1);

    let mut cr0 = read_cr0();
    cr0 |= cr0_fixed0;
    cr0 &= cr0_fixed1;
    write_cr0(cr0);

    let mut cr4 = read_cr4();
    cr4 |= cr4_fixed0;
    cr4 &= cr4_fixed1;
    write_cr4(cr4);
}

/// Attempt to initialize VMX and prepare a VMCS.
///
/// This is safe to call multiple times; it will do a best-effort init and
/// return false on failure (without panicking).
pub fn try_init_vmx_skeleton() -> bool {
    crate::serial_write_str("RAYOS_VMM:VMX:INIT_BEGIN\n");

    if !cpu_supports_vmx() {
        crate::serial_write_str("RAYOS_VMM:VMX:UNSUPPORTED\n");
        #[cfg(feature = "vmm_hypervisor_smoke")]
        {
            crate::serial_write_str("RAYOS_VMM:VMX:SMOKE_FALLBACK\n");
            // Prepare guest memory and install guest code so smoke-mode tests can be exercised
            // even when VMX isn't available in the current environment.
            if !prepare_guest_memory() {
                crate::serial_write_str("RAYOS_VMM:VMX:SMOKE_PREP_FAIL\n");
                return false;
            }
        }
        return false;
    }
    crate::serial_write_str("RAYOS_VMM:VMX:SUPPORTED\n");

    if !try_enable_vmx_feature_control() {
        crate::serial_write_str("RAYOS_VMM:VMX:FEATURE_CONTROL_DENIED\n");
        return false;
    }
    crate::serial_write_str("RAYOS_VMM:VMX:FEATURE_CONTROL_OK\n");

    // Enable CR4.VMXE.
    let cr4 = read_cr4();
    write_cr4(cr4 | CR4_VMXE);

    // Apply the CR0/CR4 fixed-bit constraints.
    apply_vmx_fixed_bits();

    if !prepare_guest_memory() {
        crate::serial_write_str("RAYOS_VMM:VMX:GUEST_MEMORY_FAIL\n");
        return false;
    }

    let revision = vmx_revision_id();
    crate::serial_write_str("RAYOS_VMM:VMX:REV=0x");
    crate::serial_write_hex_u64(revision as u64);
    crate::serial_write_str("\n");

    // Allocate VMXON + VMCS regions.
    // We rely on the kernel's physical allocator and HHDM/identity mapping.
    let vmxon_phys = match crate::phys_alloc_page() {
        Some(p) => p,
        None => {
            crate::serial_write_str("RAYOS_VMM:VMX:ALLOC_VMXON_FAIL\n");
            return false;
        }
    };
    let vmcs_phys = match crate::phys_alloc_page() {
        Some(p) => p,
        None => {
            crate::serial_write_str("RAYOS_VMM:VMX:ALLOC_VMCS_FAIL\n");
            return false;
        }
    };

    // Write revision ID at start of each region.
    unsafe {
        let vmxon_ptr = crate::phys_to_virt(vmxon_phys) as *mut u32;
        core::ptr::write_volatile(vmxon_ptr, revision);

        let vmcs_ptr = crate::phys_to_virt(vmcs_phys) as *mut u32;
        core::ptr::write_volatile(vmcs_ptr, revision);
    }

    // Enter VMX operation.
    let ok_vmxon = unsafe { vmxon(vmxon_phys) };
    if !ok_vmxon {
        crate::serial_write_str("RAYOS_VMM:VMX:VMXON_FAIL\n");
        return false;
    }
    crate::serial_write_str("RAYOS_VMM:VMX:VMXON_OK\n");

    // Prepare VMCS (clear then load current VMCS).
    let ok_clear = unsafe { vmclear(vmcs_phys) };
    if !ok_clear {
        crate::serial_write_str("RAYOS_VMM:VMX:VMCLEAR_FAIL\n");
        unsafe { vmxoff() };
        return false;
    }

    let ok_load = unsafe { vmptrld(vmcs_phys) };
    if !ok_load {
        crate::serial_write_str("RAYOS_VMM:VMX:VMPTRLD_FAIL\n");
        unsafe { vmxoff() };
        return false;
    }

    crate::serial_write_str("RAYOS_VMM:VMX:VMCS_READY\n");

    // Run quick virtio-gpu selftest early after VMX is ready so logs appear
    // deterministically even if later VM entry paths encounter faults.
    #[cfg(all(feature = "vmm_virtio_gpu", feature = "vmm_hypervisor_smoke"))]
    {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST_BEGIN_EARLY\n");
        unsafe { run_virtio_gpu_selftest(); }
        crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST_DONE_EARLY\n");
    }

    #[cfg(feature = "vmm_inject_backoff_selftest")]
    {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_RUN\n");
        run_inject_backoff_selftest();
        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:BACKOFF_SELFTEST_RUN_DONE\n");
    }

    // Optional: run an early virtio-console selftest (writes a sample string into
    // guest-phys memory and invokes the console handler so we have deterministic
    // console dispatch coverage in smoke runs).
    #[cfg(all(feature = "vmm_virtio_console", feature = "vmm_virtio_console_selftest", feature = "vmm_hypervisor_smoke"))]
    {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:SELFTEST_BEGIN\n");
        unsafe { run_virtio_console_selftest(); }
        crate::serial_write_str("RAYOS_VMM:VIRTIO_CONSOLE:SELFTEST_END\n");
    }

    unsafe {
        setup_vmcs_minimal_host_and_controls();
    }

    // By default, `vmm_hypervisor` only does bring-up and returns to RayOS.
    // The "enter guest" smoke test is gated behind a separate feature to avoid
    // surprising interactive boots.
    #[cfg(any(feature = "vmm_hypervisor_smoke", feature = "vmm_hypervisor_net_test"))]
    {
        unsafe { setup_vmcs_minimal_guest_state() };

        // Attempt a VMLAUNCH into a trivial guest loop that executes HLT forever.
        // With HLT exiting enabled, this should produce a fast, deterministic VM-exit.
        crate::serial_write_str("RAYOS_VMM:VMX:VMLAUNCH_ATTEMPT\n");
        let (launch_ok, rflags) = unsafe {
            #[cfg(feature = "vmm_linux_guest")]
            {
                if LINUX_GUEST_ENTRY_RIP != 0 && LINUX_GUEST_BOOT_PARAMS_GPA != 0 {
                    vmlaunch_with_linux_boot_params(LINUX_GUEST_BOOT_PARAMS_GPA)
                } else {
                    vmlaunch()
                }
            }
            #[cfg(not(feature = "vmm_linux_guest"))]
            {
                vmlaunch()
            }
        };
        if launch_ok {
            // If we ever get here, it means we entered a guest and returned via some path.
            crate::serial_write_str("RAYOS_VMM:VMX:VMLAUNCH_OK_UNEXPECTED\n");
        } else {
            crate::serial_write_str("RAYOS_VMM:VMX:VMLAUNCH_FAIL rflags=0x");
            crate::serial_write_hex_u64(rflags);
            crate::serial_write_str("\n");

            let (ok_err, err) = unsafe { vmread(VMCS_VM_INSTRUCTION_ERROR) };
            if ok_err {
                crate::serial_write_str("RAYOS_VMM:VMX:VM_INSTR_ERR=0x");
                crate::serial_write_hex_u64(err);
                crate::serial_write_str("\n");
            } else {
                crate::serial_write_str("RAYOS_VMM:VMX:VMREAD_INSTR_ERR_FAIL\n");
            }
        }
    }

    #[cfg(not(any(feature = "vmm_hypervisor_smoke", feature = "vmm_hypervisor_net_test")))]
    {
        crate::serial_write_str("RAYOS_VMM:VMX:SMOKE_DISABLED\n");
    }

    // If virtio-gpu is present in this build, run a deterministic self-test that
    // exercises the controlq handler and validates scanout publish / first frame
    // markers. This helps catch regressions as we wire the device to a real
    // Guest virtqueue transport.
    #[cfg(all(feature = "vmm_virtio_gpu", feature = "vmm_hypervisor_smoke"))]
    {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST_BEGIN\n");
        unsafe { run_virtio_gpu_selftest(); }
        crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SELFTEST_DONE\n");
    }

    // Skeleton ends here for now.
    // We intentionally VMXOFF so the rest of the kernel continues normally.
    unsafe { vmxoff() };
    crate::serial_write_str("RAYOS_VMM:VMX:VMXOFF\n");

    true
}
