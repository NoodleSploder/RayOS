// Minimal skeleton for a virtio-console device model.
// This will be extended in P1 to implement queue parsing and character I/O.

pub struct VirtioConsoleDevice {
    // placeholder state
    pub opened: bool,
}

impl VirtioConsoleDevice {
    pub const fn new() -> Self {
        Self { opened: false }
    }

    // Handle a control queue request (placeholder). Returns bytes written to resp.
    pub fn handle_controlq(&mut self, _req_addr: u64, _req_len: usize, _resp_addr: u64, _resp_len: usize) -> usize {
        // No-op for now
        0
    }
}
