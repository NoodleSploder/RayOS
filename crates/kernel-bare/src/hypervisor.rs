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

use crate::guest_driver_template::{GUEST_DRIVER_BINARY, GUEST_DRIVER_DESC_DATA_OFFSET, GUEST_DRIVER_DESC_DATA_PTR_OFFSET};

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
const VMCS_VMEXIT_INSTRUCTION_LEN: u64 = 0x440C;
const VMCS_EXIT_QUALIFICATION: u64 = 0x6400;
const GUEST_LINEAR_ADDRESS: u64 = 0x640A;
const GUEST_PHYSICAL_ADDRESS: u64 = 0x2400;

// 64-bit control fields
const VMCS_LINK_POINTER: u64 = 0x2800;
const EPT_POINTER: u64 = 0x201A;
const IO_BITMAP_A: u64 = 0x2000;
const IO_BITMAP_B: u64 = 0x2002;

// 32-bit control fields
const PIN_BASED_VM_EXEC_CONTROL: u64 = 0x4000;
const CPU_BASED_VM_EXEC_CONTROL: u64 = 0x4002;
const VM_EXIT_CONTROLS: u64 = 0x400C;
const VM_ENTRY_CONTROLS: u64 = 0x4012;
const SECONDARY_VM_EXEC_CONTROL: u64 = 0x401E;

// CPU-based execution control bits
const CPU_CTL_HLT_EXITING: u32 = 1 << 7;
const CPU_CTL_CPUID_EXITING: u32 = 1 << 21;
const CPU_CTL_UNCOND_IO_EXITING: u32 = 1 << 24;
const CPU_CTL_USE_IO_BITMAPS: u32 = 1 << 25;
const CPU_CTL_ACTIVATE_SECONDARY_CONTROLS: u32 = 1 << 31;

// Secondary processor-based execution controls
const CPU2_CTL_ENABLE_EPT: u32 = 1 << 1;

// Guest-state fields (subset)
const GUEST_CR0: u64 = 0x6800;
const GUEST_CR3: u64 = 0x6802;
const GUEST_CR4: u64 = 0x6804;

const GUEST_RSP: u64 = 0x681C;
const GUEST_RIP: u64 = 0x681E;
const GUEST_RFLAGS: u64 = 0x6820;

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
const GUEST_GDT_ENTRIES: [u64; GUEST_GDT_STATIC_ENTRIES] = [
    0,
    0x00AF9B000000FFFF,
    0x00CF93000000FFFF,
];
const GUEST_GDT_LIMIT_VALUE: u64 = (GUEST_GDT_ENTRY_COUNT * 8 - 1) as u64;
const GUEST_IDTR_LIMIT_VALUE: u64 = 0;
const GUEST_CODE_SELECTOR: u16 = 1 << 3;
const GUEST_DATA_SELECTOR: u16 = 2 << 3;
const GUEST_TSS_SELECTOR: u16 = (GUEST_GDT_STATIC_ENTRIES as u16) << 3;
const GUEST_SEGMENT_LIMIT_VALUE: u64 = 0x000F_FFFF;
const GUEST_CS_AR_VALUE: u64 = 0xAF9B;
const GUEST_DS_AR_VALUE: u64 = 0xCF93;
static mut GUEST_TSS_PHYS_VALUE: u64 = 0;
static mut GUEST_TSS_LIMIT_VALUE: u64 = 0;
static mut GUEST_TSS_AR_VALUE: u64 = 0;
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
        install_guest_code();
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

    core::ptr::write_volatile(pml4.add(0), (guest_ram_gpa(1) & 0xFFFF_FFFF_FFFF_F000) | PRESENT_RW);
    core::ptr::write_volatile(pdpt.add(0), (guest_ram_gpa(2) & 0xFFFF_FFFF_FFFF_F000) | PRESENT_RW);

    for pd_idx in 0..GUEST_EPT_PD_COUNT {
        let pt_page = GUEST_PT_PAGE_START + pd_idx;
        let pt_phys = guest_ram_phys(pt_page);
        let pt = crate::phys_to_virt(pt_phys) as *mut u64;
        core::ptr::write_volatile(pd.add(pd_idx), (guest_ram_gpa(pt_page) & 0xFFFF_FFFF_FFFF_F000) | PRESENT_RW);

        let base_index = pd_idx * 512;
        for pt_idx in 0..512 {
            let page_index = base_index + pt_idx;
            if page_index >= page_limit {
                break;
            }
            let page_gpa = guest_ram_gpa(page_index);
            core::ptr::write_volatile(pt.add(pt_idx), (page_gpa & 0xFFFF_FFFF_FFFF_F000) | PRESENT_RW);
        }
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
    ptr::write_unaligned(patch_ptr, desc_data_gpa);
}

unsafe fn install_guest_descriptor_tables() {
    let gdt_phys = guest_ram_phys(GUEST_GDT_PAGE_INDEX);
    let gdt_dst = crate::phys_to_virt(gdt_phys) as *mut u64;
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
const EPT_MEMTYPE_WB: u64 = 6;

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

    core::ptr::write_volatile(pdpt_v.add(0), (pd & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS);
    core::ptr::write_volatile(pml4_v.add(0), (pdpt & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS);

    for pd_idx in 0..GUEST_EPT_PD_COUNT {
        let pt = crate::phys_to_virt(pt_phys[pd_idx]) as *mut u64;
        core::ptr::write_volatile(pd_v.add(pd_idx), (pt_phys[pd_idx] & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS);

        let base_index = pd_idx * 512;
        for pt_idx in 0..512 {
            let page_index = base_index + pt_idx;
            if page_index >= GUEST_RAM_PAGES {
                break;
            }
            let page_phys = guest_ram_phys(page_index);
            let entry = (page_phys & 0xFFFF_FFFF_FFFF_F000) | EPT_ENTRY_FLAGS | (EPT_MEMTYPE_WB << 3);
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

const PAGE_SIZE: usize = 4096;
const GUEST_RAM_SIZE_MB: usize = 16;
const GUEST_RAM_SIZE_BYTES: usize = GUEST_RAM_SIZE_MB * 1024 * 1024;
const GUEST_RAM_PAGES: usize = GUEST_RAM_SIZE_BYTES / PAGE_SIZE;
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
const VIRTIO_MMIO_DRIVER_FEATURES_OFFSET: u64 = 0x020;
const VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET: u64 = 0x060;
const VIRTIO_MMIO_INTERRUPT_ACK_OFFSET: u64 = 0x064;
const VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET: u64 = 0x050;
const VIRTIO_MMIO_STATUS_OFFSET: u64 = 0x070;
const VIRTIO_MMIO_QUEUE_DESC_OFFSET: u64 = 0x080;
const VIRTIO_MMIO_QUEUE_DRIVER_OFFSET: u64 = 0x088;
const VIRTIO_MMIO_QUEUE_USED_OFFSET: u64 = 0x090;
const VIRTIO_MMIO_QUEUE_SIZE_OFFSET: u64 = 0x098;
const VIRTIO_MMIO_QUEUE_READY_OFFSET: u64 = 0x09C;
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

static mut IO_BITMAP_A_PHYS: u64 = 0;
static mut IO_BITMAP_B_PHYS: u64 = 0;
static mut IO_BITMAPS_READY: bool = false;

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
    driver_features: AtomicU32,
    interrupt_status: AtomicU32,
    device_id: AtomicU32,
    queue_notify_count: AtomicU32,
    queue_desc_address: AtomicU64,
    queue_driver_address: AtomicU64,
    queue_used_address: AtomicU64,
    queue_avail_index: AtomicU16,
    queue_used_index: AtomicU16,
    queue_size: AtomicU32,
    queue_ready: AtomicU32,
}

impl VirtioMmioState {
    const fn new() -> Self {
        Self {
            status: AtomicU32::new(0),
            driver_features: AtomicU32::new(0),
            interrupt_status: AtomicU32::new(0),
            device_id: AtomicU32::new(VIRTIO_MMIO_DEVICE_ID_VALUE),
            queue_notify_count: AtomicU32::new(0),
            queue_desc_address: AtomicU64::new(0),
            queue_driver_address: AtomicU64::new(0),
            queue_used_address: AtomicU64::new(0),
            queue_avail_index: AtomicU16::new(0),
            queue_used_index: AtomicU16::new(0),
            queue_size: AtomicU32::new(0),
            queue_ready: AtomicU32::new(0),
        }
    }
}

static VIRTIO_MMIO_STATE: VirtioMmioState = VirtioMmioState::new();

struct MmioInstruction {
    kind: MmioAccessKind,
    size: usize,
    address: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

unsafe fn register_mmio_region(region: MmioRegion) -> bool {
    for slot in MMIO_REGIONS.iter_mut() {
        if slot.is_none() {
            *slot = Some(region);
            return true;
        }
    }
    false
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

fn init_hypervisor_mmio() {
    unsafe {
        if MMIO_REGIONS_INITIALIZED {
            return;
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
    }
}

fn mmio_counter_handler(_regs: &mut GuestRegs, access: &MmioAccess, value: Option<u64>) -> Option<u64> {
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

fn virtio_mmio_handler(_regs: &mut GuestRegs, access: &MmioAccess, value: Option<u64>) -> Option<u64> {
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
        (VIRTIO_MMIO_VERSION_OFFSET, MmioAccessKind::Read) => Some(VIRTIO_MMIO_VERSION_VALUE as u64),
        (VIRTIO_MMIO_DEVICE_ID_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.device_id.load(Ordering::Relaxed) as u64
        ),
        (VIRTIO_MMIO_VENDOR_ID_OFFSET, MmioAccessKind::Read) => Some(VIRTIO_MMIO_VENDOR_ID_VALUE as u64),
        (VIRTIO_MMIO_DEVICE_FEATURES_OFFSET, MmioAccessKind::Read) => {
            Some(VIRTIO_MMIO_FEATURES_VALUE as u64)
        }
        (VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.driver_features.load(Ordering::Relaxed) as u64,
        ),
        (VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.interrupt_status.load(Ordering::Relaxed) as u64,
        ),
        (VIRTIO_MMIO_STATUS_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.status.load(Ordering::Relaxed) as u64,
        ),
        (VIRTIO_MMIO_QUEUE_DESC_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.queue_desc_address.load(Ordering::Relaxed),
        ),
        (VIRTIO_MMIO_QUEUE_DRIVER_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.queue_driver_address.load(Ordering::Relaxed),
        ),
        (VIRTIO_MMIO_QUEUE_USED_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.queue_used_address.load(Ordering::Relaxed),
        ),
        (VIRTIO_MMIO_QUEUE_SIZE_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.queue_size.load(Ordering::Relaxed) as u64,
        ),
        (VIRTIO_MMIO_QUEUE_READY_OFFSET, MmioAccessKind::Read) => Some(
            VIRTIO_MMIO_STATE.queue_ready.load(Ordering::Relaxed) as u64,
        ),
        (VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .driver_features
                .store(write_value as u32, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:DRIVER_FEATURES=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");
            None
        }
        (VIRTIO_MMIO_STATUS_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .status
                .store(write_value as u32, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:STATUS=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");
            None
        }
        (VIRTIO_MMIO_QUEUE_DESC_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .queue_desc_address
                .store(write_value, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:QUEUE_DESC=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");
            None
        }
        (VIRTIO_MMIO_QUEUE_DRIVER_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .queue_driver_address
                .store(write_value, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:QUEUE_DRIVER=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");
            None
        }
        (VIRTIO_MMIO_QUEUE_USED_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .queue_used_address
                .store(write_value, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:QUEUE_USED=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");
            None
        }
        (VIRTIO_MMIO_QUEUE_SIZE_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .queue_size
                .store(write_value as u32, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:QUEUE_SIZE=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");
            None
        }
        (VIRTIO_MMIO_QUEUE_READY_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE
                .queue_ready
                .store(write_value as u32, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:QUEUE_READY=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str("\n");
            None
        }
        (VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET, MmioAccessKind::Write) => {
            VIRTIO_MMIO_STATE.queue_notify_count.fetch_add(1, Ordering::Relaxed);
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:QUEUE_NOTIFY=");
            crate::serial_write_hex_u64(write_value);
            crate::serial_write_str(" desc=");
            let queue_desc_addr = VIRTIO_MMIO_STATE.queue_desc_address.load(Ordering::Relaxed);
            let queue_driver_addr = VIRTIO_MMIO_STATE.queue_driver_address.load(Ordering::Relaxed);
            let queue_used_addr = VIRTIO_MMIO_STATE.queue_used_address.load(Ordering::Relaxed);
            let queue_size_value = VIRTIO_MMIO_STATE.queue_size.load(Ordering::Relaxed);
            let queue_ready_value = VIRTIO_MMIO_STATE.queue_ready.load(Ordering::Relaxed);
            crate::serial_write_hex_u64(queue_desc_addr);
            crate::serial_write_str(" driver=");
            crate::serial_write_hex_u64(queue_driver_addr);
            crate::serial_write_str(" used=");
            crate::serial_write_hex_u64(queue_used_addr);
            crate::serial_write_str(" size=");
            crate::serial_write_hex_u64(queue_size_value as u64);
            crate::serial_write_str(" ready=");
            crate::serial_write_hex_u64(queue_ready_value as u64);
            crate::serial_write_str("\n");
            log_virtq_descriptors(queue_desc_addr, queue_size_value);
            log_virtq_avail(queue_driver_addr, queue_size_value);
            log_virtq_used(queue_used_addr, queue_size_value);
            process_virtq_queue(
                queue_desc_addr,
                queue_driver_addr,
                queue_used_addr,
                queue_size_value,
                queue_ready_value,
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
            let high = core::ptr::read_unaligned((seg_desc_addr(selector, gdtr_base) + 8) as *const u32) as u64;
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
        10 => "CPUID",
        12 => "HLT",
        18 => "VMCALL",
        30 => "IO_INSTRUCTION",
        48 => "EPT_VIOLATION",
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

    // Intercept port 0xE9 (QEMU debugcon-style output).
    // I/O bitmap A covers ports 0..0x7FFF.
    let port: usize = 0xE9;
    let byte_index = port / 8;
    let bit = 1u8 << (port % 8);
    let cur = core::ptr::read_volatile(a_v.add(byte_index));
    core::ptr::write_volatile(a_v.add(byte_index), cur | bit);

    IO_BITMAP_A_PHYS = a;
    IO_BITMAP_B_PHYS = b;
    IO_BITMAPS_READY = true;
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

        let exit_basic = (reason & 0xffff) as u32;
        let entry_fail = ((reason >> 31) & 1) as u32;

        // Keep logs tight: only print full lines for the first few exits and for interesting reasons.
        let verbose = count <= 8 || exit_basic == 10 || exit_basic == 2 || exit_basic == 7;
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
        }

        match exit_basic {
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

                    if !direction_in && port == 0x00E9 && size == 1 {
                        let ch = (regs.rax & 0xFF) as u8;
                        crate::serial_write_str("RAYOS_GUEST_E9:");
                        crate::serial_write_byte(ch);
                        crate::serial_write_str("\n");
                    }

                    if verbose {
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
            10 => {
                // CPUID emulation: return host CPUID.
                let leaf = regs.rax as u32;
                let subleaf = regs.rcx as u32;
                let r = cpuid(leaf, subleaf);
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
        }
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
        for b in VIRTIO_BLK_DISK.iter_mut() {
            *b = VIRTIO_BLK_READ_PATTERN;
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

fn handle_virtio_blk_chain(header: VirtqDesc, data_descs: &[VirtqDesc], status_desc: Option<VirtqDesc>) {
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
                    if write_guest_bytes(desc.addr, &unsafe { VIRTIO_NET_LOOPBACK_PKT }[..to_write]) {
                        crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:NET_RX_INJECT len=");
                        crate::serial_write_hex_u64(to_write as u64);
                        crate::serial_write_str("\n");
                        unsafe {
                            VIRTIO_NET_LOOPBACK_PKT_LEN = 0; // Clear buffer after injection
                            VIRTIO_NET_RX_PACKETS = VIRTIO_NET_RX_PACKETS.wrapping_add(1);
                        }
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
        addr: u64::from_le_bytes(buf[0..8].try_into().ok()?),
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

fn log_descriptor_chain(base: u64, queue_size: u32, start_index: u32) -> Option<u32> {
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
        } else if status_desc.is_none()
            && (desc.flags & VIRTQ_DESC_F_WRITE) != 0
            && desc.len == 1
        {
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
                match device_id {
                    VIRTIO_MMIO_DEVICE_ID_VALUE => {
                        // Block device (0x0105)
                        handle_virtio_blk_chain(header, &data_descs[..data_desc_count], status_desc);
                    }
                    VIRTIO_NET_DEVICE_ID => {
                        // Network device (0x0101) - queue_id would need to be tracked
                        // For now, assume TX queue (0)
                        handle_virtio_net_chain(VIRTIO_NET_TX_QUEUE, &data_descs[..data_desc_count]);
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
                let imm = u32::from_le_bytes(bytes[1..5].try_into().ok()? ) as u64;
                if imm == gpa {
                    return Some(MmioInstruction { kind: MmioAccessKind::Read, size: 4, address: imm });
                }
            }
            0xA3 => {
                let imm = u32::from_le_bytes(bytes[1..5].try_into().ok()? ) as u64;
                if imm == gpa {
                    return Some(MmioInstruction { kind: MmioAccessKind::Write, size: 4, address: imm });
                }
            }
            _ => {}
        }
    }
    if ilen >= 10 && bytes[0] == 0x48 && bytes[1] == 0xA1 {
        let imm = u64::from_le_bytes(bytes[2..10].try_into().ok()?);
        if imm == gpa {
            return Some(MmioInstruction { kind: MmioAccessKind::Read, size: 8, address: imm });
        }
    }
    if ilen >= 10 && bytes[0] == 0x48 && bytes[1] == 0xA3 {
        let imm = u64::from_le_bytes(bytes[2..10].try_into().ok()?);
        if imm == gpa {
            return Some(MmioInstruction { kind: MmioAccessKind::Write, size: 8, address: imm });
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

    // Required bits are forced by adjust_vmx_controls.
    // - Request HLT exiting (CPU-based bit 7) so our trivial guest reliably VM-exits.
    // - Request CPUID exiting + I/O bitmaps so we can emulate guest-visible CPU/port I/O.
    // - Request 64-bit host mode for VM-exit (exit ctl bit 9).
    // - Request IA-32e guest (entry ctl bit 9) + load IA32_EFER (entry ctl bit 15).
    // - Request save/load IA32_EFER on exit (exit ctl bits 20/21).
    let pin = adjust_vmx_controls(msr_pin, 0);
    // Request secondary controls so we can enable EPT as the next foundation step.
    let cpu = adjust_vmx_controls(
        msr_cpu,
        CPU_CTL_HLT_EXITING
            | CPU_CTL_CPUID_EXITING
            | CPU_CTL_USE_IO_BITMAPS
            | CPU_CTL_ACTIVATE_SECONDARY_CONTROLS,
    );
    let exit_ctl = adjust_vmx_controls(
        msr_exit,
        EXIT_CTL_HOST_ADDR_SPACE_SIZE | EXIT_CTL_SAVE_IA32_EFER | EXIT_CTL_LOAD_IA32_EFER,
    );
    let entry_ctl = adjust_vmx_controls(
        msr_entry,
        ENTRY_CTL_IA32E_MODE_GUEST | ENTRY_CTL_LOAD_IA32_EFER,
    );

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

    if (cpu & CPU_CTL_USE_IO_BITMAPS) != 0 {
        if ensure_io_bitmaps() {
            let _ = vmcs_write_or_log(IO_BITMAP_A, unsafe { IO_BITMAP_A_PHYS });
            let _ = vmcs_write_or_log(IO_BITMAP_B, unsafe { IO_BITMAP_B_PHYS });
        } else {
            crate::serial_write_str("RAYOS_VMM:VMX:ALLOC_IO_BITMAPS_FAIL\n");
        }
    }

    // Some control bits imply additional VMCS fields must be valid.
    // Program IA32_PAT fields if requested by the adjusted controls.
    // (We mirror the host PAT into the guest for bring-up.)
    if (exit_ctl & (EXIT_CTL_SAVE_IA32_PAT | EXIT_CTL_LOAD_IA32_PAT)) != 0 || (entry_ctl & ENTRY_CTL_LOAD_IA32_PAT) != 0 {
        let pat = crate::rdmsr(IA32_PAT);
        let _ = vmcs_write_or_log(HOST_IA32_PAT, pat);
        let _ = vmcs_write_or_log(GUEST_IA32_PAT, pat);
    }

    // If the CPU-based controls enable/require secondary controls, program them too.
    // Bit 31 == "activate secondary controls".
    if (cpu & (1 << 31)) != 0 {
        let cpu2 = adjust_vmx_controls(IA32_VMX_PROCBASED_CTLS2, CPU2_CTL_ENABLE_EPT);
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

    // WARNING: This mirrors host paging/address space. This is bring-up only.
    let guest_cr0 = read_cr0();
    let guest_cr4 = read_cr4();
    let _ = vmcs_write_or_log(GUEST_CR0, guest_cr0);
    let guest_pml4_gpa = guest_ram_gpa(0);
    let _ = vmcs_write_or_log(GUEST_CR3, guest_pml4_gpa);
    let _ = vmcs_write_or_log(GUEST_CR4, guest_cr4);

    let _ = vmcs_write_or_log(GUEST_RFLAGS, 0x2);
    let guest_code_rip = guest_ram_gpa(GUEST_CODE_PAGE_INDEX);
    let _ = vmcs_write_or_log(GUEST_RIP, guest_code_rip);
    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_CODE_RIP=0x");
    crate::serial_write_hex_u64(guest_code_rip);
    crate::serial_write_str("\n");

    // Sanity: confirm the guest code bytes we installed are still present.
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
    let guest_stack_top = guest_ram_gpa(GUEST_STACK_START_INDEX + GUEST_STACK_PAGES);
    let _ = vmcs_write_or_log(GUEST_RSP, guest_stack_top - 0x10);
    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_STACK_TOP=0x");
    crate::serial_write_hex_u64(guest_stack_top);
    crate::serial_write_str("\n");

    // Guest EFER must be consistent with IA-32e entry controls.
    let efer = crate::rdmsr(IA32_EFER);
    let _ = vmcs_write_or_log(GUEST_IA32_EFER, efer);

    let code_selector = GUEST_CODE_SELECTOR as u64;
    let data_selector = GUEST_DATA_SELECTOR as u64;

    let _ = vmcs_write_or_log(GUEST_CS_SELECTOR, code_selector);
    let _ = vmcs_write_or_log(GUEST_SS_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_DS_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_ES_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_FS_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_GS_SELECTOR, data_selector);
    let _ = vmcs_write_or_log(GUEST_TR_SELECTOR, GUEST_TSS_SELECTOR as u64);
    let _ = vmcs_write_or_log(GUEST_LDTR_SELECTOR, 0);

    let _ = vmcs_write_or_log(GUEST_CS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_SS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_DS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_ES_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_FS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_GS_BASE, 0);
    let _ = vmcs_write_or_log(GUEST_TR_BASE, unsafe { GUEST_TSS_PHYS_VALUE });
    let _ = vmcs_write_or_log(GUEST_LDTR_BASE, 0);

    let _ = vmcs_write_or_log(GUEST_CS_LIMIT, GUEST_SEGMENT_LIMIT_VALUE);
    let _ = vmcs_write_or_log(GUEST_SS_LIMIT, GUEST_SEGMENT_LIMIT_VALUE);
    let _ = vmcs_write_or_log(GUEST_DS_LIMIT, GUEST_SEGMENT_LIMIT_VALUE);
    let _ = vmcs_write_or_log(GUEST_ES_LIMIT, GUEST_SEGMENT_LIMIT_VALUE);
    let _ = vmcs_write_or_log(GUEST_FS_LIMIT, GUEST_SEGMENT_LIMIT_VALUE);
    let _ = vmcs_write_or_log(GUEST_GS_LIMIT, GUEST_SEGMENT_LIMIT_VALUE);
    let _ = vmcs_write_or_log(GUEST_TR_LIMIT, unsafe { GUEST_TSS_LIMIT_VALUE });
    let _ = vmcs_write_or_log(GUEST_LDTR_LIMIT, 0);

    let _ = vmcs_write_or_log(GUEST_CS_AR_BYTES, GUEST_CS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_SS_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_DS_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_ES_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_FS_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_GS_AR_BYTES, GUEST_DS_AR_VALUE);
    let _ = vmcs_write_or_log(GUEST_TR_AR_BYTES, unsafe { GUEST_TSS_AR_VALUE });
    let _ = vmcs_write_or_log(GUEST_LDTR_AR_BYTES, 1u64 << 16);

    let guest_gdt_gpa = guest_ram_gpa(GUEST_GDT_PAGE_INDEX);
    let guest_idt_gpa = guest_ram_gpa(GUEST_IDT_PAGE_INDEX);
    let _ = vmcs_write_or_log(GUEST_GDTR_BASE, guest_gdt_gpa);
    let _ = vmcs_write_or_log(GUEST_IDTR_BASE, guest_idt_gpa);
    let _ = vmcs_write_or_log(GUEST_GDTR_LIMIT, GUEST_GDT_LIMIT_VALUE);
    let _ = vmcs_write_or_log(GUEST_IDTR_LIMIT, GUEST_IDTR_LIMIT_VALUE);

    let _ = vmcs_write_or_log(GUEST_DR7, 0x400);

    log_guest_vmcs_state();

    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_SETUP_DONE\n");
}

unsafe fn log_guest_vmcs_state() {
    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_SEGMENT_STATE\n");
    let fields: &[(u64, &str)] = &[
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

fn process_virtq_queue(
    desc_base: u64,
    driver_base: u64,
    used_base: u64,
    queue_size: u32,
    queue_ready: u32,
) {
    if queue_size == 0 || queue_ready == 0 {
        return;
    }
    if desc_base == 0 || driver_base == 0 || used_base == 0 {
        return;
    }
    let queue_size_u64 = queue_size as u64;
    let mut avail_processed = VIRTIO_MMIO_STATE.queue_avail_index.load(Ordering::Relaxed);
    let mut used_idx = VIRTIO_MMIO_STATE.queue_used_index.load(Ordering::Relaxed);
    let avail_idx = match read_u16(driver_base + VIRTQ_AVAIL_INDEX_OFFSET) {
        Some(v) => v,
        None => {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_MMIO:AVAIL_IDX_READ_FAIL\n");
            return;
        }
    };
    while avail_processed != avail_idx {
        let ring_pos = (avail_processed as u64) % queue_size_u64;
        let entry_offset = driver_base + VIRTQ_AVAIL_RING_OFFSET + ring_pos * VIRTQ_AVAIL_ENTRY_SIZE;
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
        let total_len = match log_descriptor_chain(desc_base, queue_size, desc_index as u32) {
            Some(len) => len,
            None => break,
        };
        let used_ring_pos = (used_idx as u64) % queue_size_u64;
        let used_entry_offset = used_base + VIRTQ_USED_RING_OFFSET + used_ring_pos * VIRTQ_USED_ENTRY_SIZE;
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
    VIRTIO_MMIO_STATE
        .queue_avail_index
        .store(avail_processed, Ordering::Relaxed);
    VIRTIO_MMIO_STATE
        .queue_used_index
        .store(used_idx, Ordering::Relaxed);
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

    unsafe {
        setup_vmcs_minimal_host_and_controls();
    }

    // By default, `vmm_hypervisor` only does bring-up and returns to RayOS.
    // The "enter guest" smoke test is gated behind a separate feature to avoid
    // surprising interactive boots.
    #[cfg(feature = "vmm_hypervisor_smoke")]
    {
        unsafe { setup_vmcs_minimal_guest_state() };

        // Attempt a VMLAUNCH into a trivial guest loop that executes HLT forever.
        // With HLT exiting enabled, this should produce a fast, deterministic VM-exit.
        crate::serial_write_str("RAYOS_VMM:VMX:VMLAUNCH_ATTEMPT\n");
        let (launch_ok, rflags) = unsafe { vmlaunch() };
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

    #[cfg(not(feature = "vmm_hypervisor_smoke"))]
    {
        crate::serial_write_str("RAYOS_VMM:VMX:SMOKE_DISABLED\n");
    }

    // Skeleton ends here for now.
    // We intentionally VMXOFF so the rest of the kernel continues normally.
    unsafe { vmxoff() };
    crate::serial_write_str("RAYOS_VMM:VMX:VMXOFF\n");

    true
}
