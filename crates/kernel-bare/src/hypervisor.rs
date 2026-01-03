//! Hypervisor runtime skeleton (x86_64 VMX-first).
//!
//! This is intentionally a *skeleton*:
//! - Detects whether VMX is available.
//! - Enables VMX operation (when allowed by IA32_FEATURE_CONTROL).
//! - Allocates VMXON + VMCS regions and executes VMXON/VMCLEAR/VMPTRLD.
//! - Does not yet build EPT/NPT, a guest VM, or a VM-exit handler loop.

#![allow(dead_code)]

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};

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
const VMCS_VMEXIT_INSTRUCTION_LEN: u64 = 0x440C;
const VMCS_EXIT_QUALIFICATION: u64 = 0x6400;

// 64-bit control fields
const VMCS_LINK_POINTER: u64 = 0x2800;

// 32-bit control fields
const PIN_BASED_VM_EXEC_CONTROL: u64 = 0x4000;
const CPU_BASED_VM_EXEC_CONTROL: u64 = 0x4002;
const VM_EXIT_CONTROLS: u64 = 0x400C;
const VM_ENTRY_CONTROLS: u64 = 0x4012;
const SECONDARY_VM_EXEC_CONTROL: u64 = 0x401E;

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

extern "C" fn vmx_exit_stub() -> ! {
    handle_vmexit_and_halt()
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
        7 => "INVALID_GUEST_STATE",
        10 => "CPUID",
        12 => "HLT",
        18 => "VMCALL",
        30 => "IO_INSTRUCTION",
        48 => "EPT_VIOLATION",
        _ => "(unknown)",
    }
}

fn handle_vmexit_and_halt() -> ! {
    crate::serial_write_str("RAYOS_VMM:VMX:VMEXIT\n");
    let count = VMEXIT_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    crate::serial_write_str("RAYOS_VMM:VMX:VMEXIT_COUNT=0x");
    crate::serial_write_hex_u64(count as u64);
    crate::serial_write_str("\n");

    unsafe {
        let (ok_r, reason) = vmread(VMCS_EXIT_REASON);
        let (ok_q, qual) = vmread(VMCS_EXIT_QUALIFICATION);
        let (ok_len, ilen) = vmread(VMCS_VMEXIT_INSTRUCTION_LEN);
        let (ok_grip, grip) = vmread(GUEST_RIP);

        if ok_r {
            let basic = (reason & 0xffff) as u32;
            let entry_fail = ((reason >> 31) & 1) as u32;
            crate::serial_write_str("RAYOS_VMM:VMX:EXIT_REASON=0x");
            crate::serial_write_hex_u64(reason);
            crate::serial_write_str("\n");
            crate::serial_write_str("RAYOS_VMM:VMX:EXIT_BASIC=0x");
            crate::serial_write_hex_u64(basic as u64);
            crate::serial_write_str(" name=");
            crate::serial_write_str(exit_reason_name(basic));
            crate::serial_write_str(" entry_fail=0x");
            crate::serial_write_hex_u64(entry_fail as u64);
            crate::serial_write_str("\n");

            // Minimal VMRESUME smoke: advance RIP over HLT and resume a few times.
            if basic == 12 && count < 4 {
                if ok_len && ok_grip {
                    let new_rip = grip.wrapping_add(ilen);
                    let _ = vmcs_write_or_log(GUEST_RIP, new_rip);
                }
                crate::serial_write_str("RAYOS_VMM:VMX:VMRESUME\n");
                let (ok_resume, rf) = vmresume();
                if !ok_resume {
                    crate::serial_write_str("RAYOS_VMM:VMX:VMRESUME_FAIL rflags=0x");
                    crate::serial_write_hex_u64(rf);
                    crate::serial_write_str("\n");
                    let (ok_err, err) = vmread(VMCS_VM_INSTRUCTION_ERROR);
                    if ok_err {
                        crate::serial_write_str("RAYOS_VMM:VMX:VM_INSTR_ERR=0x");
                        crate::serial_write_hex_u64(err);
                        crate::serial_write_str("\n");
                    }
                }
            }
        } else {
            crate::serial_write_str("RAYOS_VMM:VMX:VMREAD_EXIT_REASON_FAIL\n");
        }
        if ok_q {
            crate::serial_write_str("RAYOS_VMM:VMX:EXIT_QUAL=0x");
            crate::serial_write_hex_u64(qual);
            crate::serial_write_str("\n");
        }
        if ok_len {
            crate::serial_write_str("RAYOS_VMM:VMX:EXIT_ILEN=0x");
            crate::serial_write_hex_u64(ilen);
            crate::serial_write_str("\n");
        }
        if ok_grip {
            crate::serial_write_str("RAYOS_VMM:VMX:GUEST_RIP=0x");
            crate::serial_write_hex_u64(grip);
            crate::serial_write_str("\n");
        }

        vmxoff();
    }
    crate::serial_write_str("RAYOS_VMM:VMX:HALT\n");
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
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
    // - Request 64-bit host mode for VM-exit (exit ctl bit 9).
    // - Request IA-32e guest (entry ctl bit 9) + load IA32_EFER (entry ctl bit 15).
    // - Request save/load IA32_EFER on exit (exit ctl bits 20/21).
    let pin = adjust_vmx_controls(msr_pin, 0);
    let cpu = adjust_vmx_controls(msr_cpu, 1 << 7);
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
        let cpu2 = adjust_vmx_controls(IA32_VMX_PROCBASED_CTLS2, 0);
        crate::serial_write_str("RAYOS_VMM:VMX:CTL_CPU2=0x");
        crate::serial_write_hex_u64(cpu2 as u64);
        crate::serial_write_str("\n");
        let _ = vmcs_write_or_log(SECONDARY_VM_EXEC_CONTROL, cpu2 as u64);
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
    let rip = vmx_exit_stub as u64;
    let _ = vmcs_write_or_log(HOST_RSP, rsp);
    let _ = vmcs_write_or_log(HOST_RIP, rip);

    crate::serial_write_str("RAYOS_VMM:VMX:VMCS_SETUP_DONE\n");
}

unsafe fn setup_vmcs_minimal_guest_state() {
    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_SETUP_BEGIN\n");

    // WARNING: This mirrors host paging/address space. This is bring-up only.
    let guest_cr0 = read_cr0();
    let guest_cr4 = read_cr4();
    let guest_cr3: u64;
    asm!("mov {0}, cr3", out(reg) guest_cr3, options(nomem, nostack, preserves_flags));
    let _ = vmcs_write_or_log(GUEST_CR0, guest_cr0);
    let _ = vmcs_write_or_log(GUEST_CR3, guest_cr3);
    let _ = vmcs_write_or_log(GUEST_CR4, guest_cr4);

    let _ = vmcs_write_or_log(GUEST_RFLAGS, 0x2);
    let _ = vmcs_write_or_log(GUEST_RIP, vmx_guest_hlt_loop as u64);
    let rsp = (&raw const VMX_HOST_STACK.0 as u64) + (VMX_STACK_SIZE as u64) - 0x100;
    let _ = vmcs_write_or_log(GUEST_RSP, rsp);

    // Guest EFER must be consistent with IA-32e entry controls.
    let efer = crate::rdmsr(IA32_EFER);
    let _ = vmcs_write_or_log(GUEST_IA32_EFER, efer);

    let gdtr = read_gdtr();
    let idtr = read_idtr();

    let cs = read_seg_selector_cs();
    let ss = read_seg_selector_ss();
    let ds = read_seg_selector_ds();
    let es = read_seg_selector_es();
    let fs = read_seg_selector_fs();
    let gs = read_seg_selector_gs();
    let tr = read_seg_selector_tr();
    let ldtr: u16 = 0;

    let _ = vmcs_write_or_log(GUEST_CS_SELECTOR, cs as u64);
    let _ = vmcs_write_or_log(GUEST_SS_SELECTOR, ss as u64);
    let _ = vmcs_write_or_log(GUEST_DS_SELECTOR, ds as u64);
    let _ = vmcs_write_or_log(GUEST_ES_SELECTOR, es as u64);
    let _ = vmcs_write_or_log(GUEST_FS_SELECTOR, fs as u64);
    let _ = vmcs_write_or_log(GUEST_GS_SELECTOR, gs as u64);
    let _ = vmcs_write_or_log(GUEST_TR_SELECTOR, tr as u64);
    let _ = vmcs_write_or_log(GUEST_LDTR_SELECTOR, ldtr as u64);

    let _ = vmcs_write_or_log(GUEST_CS_BASE, seg_desc_base_from_gdt(cs, gdtr.base));
    let _ = vmcs_write_or_log(GUEST_SS_BASE, seg_desc_base_from_gdt(ss, gdtr.base));
    let _ = vmcs_write_or_log(GUEST_DS_BASE, seg_desc_base_from_gdt(ds, gdtr.base));
    let _ = vmcs_write_or_log(GUEST_ES_BASE, seg_desc_base_from_gdt(es, gdtr.base));

    // FS/GS base from MSRs for long mode.
    let fs_base = crate::rdmsr(IA32_FS_BASE);
    let gs_base = crate::rdmsr(IA32_GS_BASE);
    let _ = vmcs_write_or_log(GUEST_FS_BASE, fs_base);
    let _ = vmcs_write_or_log(GUEST_GS_BASE, gs_base);

    let _ = vmcs_write_or_log(GUEST_TR_BASE, seg_desc_base_from_gdt(tr, gdtr.base));
    let _ = vmcs_write_or_log(GUEST_LDTR_BASE, 0);

    let _ = vmcs_write_or_log(GUEST_CS_LIMIT, seg_desc_limit_from_gdt(cs, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_SS_LIMIT, seg_desc_limit_from_gdt(ss, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_DS_LIMIT, seg_desc_limit_from_gdt(ds, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_ES_LIMIT, seg_desc_limit_from_gdt(es, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_FS_LIMIT, seg_desc_limit_from_gdt(fs, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_GS_LIMIT, seg_desc_limit_from_gdt(gs, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_TR_LIMIT, seg_desc_limit_from_gdt(tr, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_LDTR_LIMIT, 0);

    let _ = vmcs_write_or_log(GUEST_CS_AR_BYTES, seg_desc_ar_from_gdt(cs, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_SS_AR_BYTES, seg_desc_ar_from_gdt(ss, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_DS_AR_BYTES, seg_desc_ar_from_gdt(ds, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_ES_AR_BYTES, seg_desc_ar_from_gdt(es, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_FS_AR_BYTES, seg_desc_ar_from_gdt(fs, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_GS_AR_BYTES, seg_desc_ar_from_gdt(gs, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_TR_AR_BYTES, seg_desc_ar_from_gdt(tr, gdtr.base) as u64);
    let _ = vmcs_write_or_log(GUEST_LDTR_AR_BYTES, 1u64 << 16);

    let _ = vmcs_write_or_log(GUEST_GDTR_BASE, gdtr.base);
    let _ = vmcs_write_or_log(GUEST_IDTR_BASE, idtr.base);
    let _ = vmcs_write_or_log(GUEST_GDTR_LIMIT, gdtr.limit as u64);
    let _ = vmcs_write_or_log(GUEST_IDTR_LIMIT, idtr.limit as u64);

    let _ = vmcs_write_or_log(GUEST_DR7, 0x400);

    crate::serial_write_str("RAYOS_VMM:VMX:GUEST_SETUP_DONE\n");
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
        setup_vmcs_minimal_guest_state();
    }

    // Attempt a VMLAUNCH into a trivial guest loop that executes HLT forever.
    // With HLT exiting enabled, this should produce a fast, deterministic VM-exit.
    crate::serial_write_str("RAYOS_VMM:VMX:VMLAUNCH_ATTEMPT\n");
    let (launch_ok, rflags) = unsafe { vmlaunch() };
    if launch_ok {
        // Unexpected at this stage. If we ever get here, it means we entered a
        // guest and returned via some path. Record and continue to VMXOFF.
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

    // Skeleton ends here for now.
    // We intentionally VMXOFF so the rest of the kernel continues normally.
    unsafe { vmxoff() };
    crate::serial_write_str("RAYOS_VMM:VMX:VMXOFF\n");

    true
}
