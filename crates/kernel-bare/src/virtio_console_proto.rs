// Minimal protocol types for the virtio-console (placeholder for P1).

#[repr(C)]
#[derive(Copy, Clone)]
pub struct VirtioConsoleCtrlHdr {
    pub type_: u32,
    pub flags: u32,
    pub id: u32,
    pub reserved: u32,
}
