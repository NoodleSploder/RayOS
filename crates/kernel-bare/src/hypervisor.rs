//! Hypervisor runtime skeleton (x86_64 VMX-first).
//!
//! This is intentionally a *skeleton*:
//! - Detects whether VMX is available.
//! - Enables VMX operation (when allowed by IA32_FEATURE_CONTROL).
//! - Allocates VMXON + VMCS regions and executes VMXON/VMCLEAR/VMPTRLD.
//! - Does not yet build EPT/NPT, a guest VM, or a VM-exit handler loop.

#![allow(dead_code)]

use core::arch::asm;

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
    let mut eax = leaf;
    let mut ebx: u32;
    let mut ecx = subleaf;
    let mut edx: u32;
    unsafe {
        asm!(
            "cpuid",
            inout("eax") eax,
            out("ebx") ebx,
            inout("ecx") ecx,
            out("edx") edx,
            options(nomem, nostack, preserves_flags)
        );
    }
    Cpuid { eax, ebx, ecx, edx }
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

    // Skeleton ends here for now.
    // We intentionally VMXOFF so the rest of the kernel continues normally.
    unsafe { vmxoff() };
    crate::serial_write_str("RAYOS_VMM:VMX:VMXOFF\n");

    true
}
