// Phase 11 Task 1: Virtio Device Handler Integration
// Wires policy enforcement checks into actual device operations

use core::fmt;

/// Device operation types for tracking and auditing
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceOperationType {
    GpuMemoryAlloc,
    GpuMemoryFree,
    GpuRenderSubmit,
    GpuReadback,
    NetworkTxPacket,
    NetworkRxPacket,
    NetworkConfigSet,
    DiskRead,
    DiskWrite,
    DiskFormat,
    InputEventInject,
    InputPoll,
    ConsoleWrite,
    ConsoleRead,
    ConsoleResize,
}

impl fmt::Display for DeviceOperationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceOperationType::GpuMemoryAlloc => write!(f, "GPU_MEM_ALLOC"),
            DeviceOperationType::GpuMemoryFree => write!(f, "GPU_MEM_FREE"),
            DeviceOperationType::GpuRenderSubmit => write!(f, "GPU_RENDER_SUBMIT"),
            DeviceOperationType::GpuReadback => write!(f, "GPU_READBACK"),
            DeviceOperationType::NetworkTxPacket => write!(f, "NET_TX"),
            DeviceOperationType::NetworkRxPacket => write!(f, "NET_RX"),
            DeviceOperationType::NetworkConfigSet => write!(f, "NET_CONFIG"),
            DeviceOperationType::DiskRead => write!(f, "DISK_READ"),
            DeviceOperationType::DiskWrite => write!(f, "DISK_WRITE"),
            DeviceOperationType::DiskFormat => write!(f, "DISK_FORMAT"),
            DeviceOperationType::InputEventInject => write!(f, "INPUT_INJECT"),
            DeviceOperationType::InputPoll => write!(f, "INPUT_POLL"),
            DeviceOperationType::ConsoleWrite => write!(f, "CONSOLE_WRITE"),
            DeviceOperationType::ConsoleRead => write!(f, "CONSOLE_READ"),
            DeviceOperationType::ConsoleResize => write!(f, "CONSOLE_RESIZE"),
        }
    }
}

/// Device operation result with latency tracking
#[derive(Debug, Clone, Copy)]
pub struct DeviceOperation {
    pub op_type: DeviceOperationType,
    pub vm_id: u32,
    pub timestamp_us: u64,
    pub duration_us: u64,
    pub allowed: bool,
    pub denied_reason: Option<&'static str>,
}

/// GPU device handler with capability checks and memory quotas
pub struct VirtioGpuHandler {
    vm_id: u32,
    total_memory_bytes: u32,
    allocated_memory_bytes: u32,
    max_memory_bytes: u32,
    render_queue_depth: u32,
    operations: [DeviceOperation; 256],
    op_index: usize,
}

impl VirtioGpuHandler {
    pub fn new(vm_id: u32, max_memory_mb: u32) -> Self {
        VirtioGpuHandler {
            vm_id,
            total_memory_bytes: 0,
            allocated_memory_bytes: 0,
            max_memory_bytes: max_memory_mb * 1024 * 1024,
            render_queue_depth: 0,
            operations: [
                DeviceOperation {
                    op_type: DeviceOperationType::GpuMemoryAlloc,
                    vm_id: 0,
                    timestamp_us: 0,
                    duration_us: 0,
                    allowed: false,
                    denied_reason: None,
                };
                256
            ],
            op_index: 0,
        }
    }

    /// Allocate GPU memory with quota enforcement
    pub fn allocate_memory(&mut self, size_bytes: u32, has_gpu_cap: bool) -> bool {
        let denied_reason = if !has_gpu_cap {
            Some("GPU_CAPABILITY_DENIED")
        } else if self.allocated_memory_bytes + size_bytes > self.max_memory_bytes {
            Some("GPU_MEMORY_QUOTA_EXCEEDED")
        } else {
            None
        };

        let allowed = denied_reason.is_none();

        if allowed {
            self.allocated_memory_bytes += size_bytes;
            self.total_memory_bytes += size_bytes;
        }

        // Log operation
        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::GpuMemoryAlloc,
            vm_id: self.vm_id,
            timestamp_us: 0, // Would be actual timestamp
            duration_us: 10,
            allowed,
            denied_reason,
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    /// Free GPU memory
    pub fn free_memory(&mut self, size_bytes: u32) {
        if self.allocated_memory_bytes >= size_bytes {
            self.allocated_memory_bytes -= size_bytes;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::GpuMemoryFree,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: 5,
            allowed: true,
            denied_reason: None,
        };
        self.op_index = (self.op_index + 1) % 256;
    }

    /// Submit render command buffer
    pub fn submit_render(&mut self, has_gpu_cap: bool) -> bool {
        let allowed = has_gpu_cap && self.render_queue_depth < 32;

        if allowed {
            self.render_queue_depth += 1;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::GpuRenderSubmit,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: if allowed { 100 } else { 0 },
            allowed,
            denied_reason: if has_gpu_cap { None } else { Some("GPU_CAPABILITY_DENIED") },
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    pub fn get_memory_usage_percent(&self) -> u32 {
        if self.max_memory_bytes == 0 {
            return 0;
        }
        ((self.allocated_memory_bytes as u64 * 100) / self.max_memory_bytes as u64) as u32
    }

    pub fn get_operation_count(&self) -> u32 {
        self.op_index as u32
    }
}

/// Network device handler with firewall and bandwidth enforcement
pub struct VirtioNetHandler {
    vm_id: u32,
    tx_packets: u64,
    rx_packets: u64,
    tx_bytes: u64,
    rx_bytes: u64,
    max_bandwidth_kbps: u32,
    dropped_packets: u32,
    operations: [DeviceOperation; 256],
    op_index: usize,
}

impl VirtioNetHandler {
    pub fn new(vm_id: u32, max_bandwidth_kbps: u32) -> Self {
        VirtioNetHandler {
            vm_id,
            tx_packets: 0,
            rx_packets: 0,
            tx_bytes: 0,
            rx_bytes: 0,
            max_bandwidth_kbps,
            dropped_packets: 0,
            operations: [
                DeviceOperation {
                    op_type: DeviceOperationType::NetworkTxPacket,
                    vm_id: 0,
                    timestamp_us: 0,
                    duration_us: 0,
                    allowed: false,
                    denied_reason: None,
                };
                256
            ],
            op_index: 0,
        }
    }

    /// Transmit packet with firewall and rate limiting
    pub fn transmit_packet(&mut self, size_bytes: u32, has_net_cap: bool, firewall_allows: bool) -> bool {
        let denied_reason = if !has_net_cap {
            Some("NETWORK_CAPABILITY_DENIED")
        } else if !firewall_allows {
            Some("FIREWALL_RULE_DROP")
        } else if self.max_bandwidth_kbps > 0 && self.tx_bytes > (self.max_bandwidth_kbps as u64 * 1024) {
            Some("BANDWIDTH_QUOTA_EXCEEDED")
        } else {
            None
        };

        let allowed = denied_reason.is_none();

        if allowed {
            self.tx_packets += 1;
            self.tx_bytes += size_bytes as u64;
        } else {
            self.dropped_packets += 1;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::NetworkTxPacket,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: if allowed { 50 } else { 10 },
            allowed,
            denied_reason,
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    /// Receive packet
    pub fn receive_packet(&mut self, size_bytes: u32, has_net_cap: bool) -> bool {
        let allowed = has_net_cap;

        if allowed {
            self.rx_packets += 1;
            self.rx_bytes += size_bytes as u64;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::NetworkRxPacket,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: if allowed { 40 } else { 0 },
            allowed,
            denied_reason: if has_net_cap { None } else { Some("NETWORK_CAPABILITY_DENIED") },
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    pub fn get_statistics(&self) -> (u64, u64, u64, u64, u32) {
        (self.tx_packets, self.rx_packets, self.tx_bytes, self.rx_bytes, self.dropped_packets)
    }
}

/// Block device handler with quota enforcement
pub struct VirtioBlkHandler {
    vm_id: u32,
    read_operations: u64,
    write_operations: u64,
    total_read_bytes: u64,
    total_write_bytes: u64,
    max_disk_quota_mb: u32,
    io_errors: u32,
    operations: [DeviceOperation; 256],
    op_index: usize,
}

impl VirtioBlkHandler {
    pub fn new(vm_id: u32, max_disk_quota_mb: u32) -> Self {
        VirtioBlkHandler {
            vm_id,
            read_operations: 0,
            write_operations: 0,
            total_read_bytes: 0,
            total_write_bytes: 0,
            max_disk_quota_mb,
            io_errors: 0,
            operations: [
                DeviceOperation {
                    op_type: DeviceOperationType::DiskRead,
                    vm_id: 0,
                    timestamp_us: 0,
                    duration_us: 0,
                    allowed: false,
                    denied_reason: None,
                };
                256
            ],
            op_index: 0,
        }
    }

    /// Read from disk with capability check
    pub fn read_blocks(&mut self, size_bytes: u32, has_disk_read_cap: bool) -> bool {
        let allowed = has_disk_read_cap;

        if allowed {
            self.read_operations += 1;
            self.total_read_bytes += size_bytes as u64;
        } else {
            self.io_errors += 1;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::DiskRead,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: if allowed { 2000 } else { 0 },
            allowed,
            denied_reason: if has_disk_read_cap { None } else { Some("DISK_READ_CAPABILITY_DENIED") },
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    /// Write to disk with capability and quota checks
    pub fn write_blocks(&mut self, size_bytes: u32, has_disk_write_cap: bool) -> bool {
        let quota_bytes = (self.max_disk_quota_mb as u64) * 1024 * 1024;
        
        let denied_reason = if !has_disk_write_cap {
            Some("DISK_WRITE_CAPABILITY_DENIED")
        } else if self.total_write_bytes + (size_bytes as u64) > quota_bytes {
            Some("DISK_QUOTA_EXCEEDED")
        } else {
            None
        };

        let allowed = denied_reason.is_none();

        if allowed {
            self.write_operations += 1;
            self.total_write_bytes += size_bytes as u64;
        } else {
            self.io_errors += 1;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::DiskWrite,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: if allowed { 3000 } else { 0 },
            allowed,
            denied_reason,
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    pub fn get_io_statistics(&self) -> (u64, u64, u64, u64, u32) {
        (self.read_operations, self.write_operations, self.total_read_bytes, self.total_write_bytes, self.io_errors)
    }
}

/// Input device handler for keyboard/mouse with capability checks
pub struct VirtioInputHandler {
    vm_id: u32,
    key_events: u32,
    mouse_events: u32,
    touch_events: u32,
    input_queue_depth: u32,
    dropped_events: u32,
    operations: [DeviceOperation; 256],
    op_index: usize,
}

impl VirtioInputHandler {
    pub fn new(vm_id: u32) -> Self {
        VirtioInputHandler {
            vm_id,
            key_events: 0,
            mouse_events: 0,
            touch_events: 0,
            input_queue_depth: 0,
            dropped_events: 0,
            operations: [
                DeviceOperation {
                    op_type: DeviceOperationType::InputEventInject,
                    vm_id: 0,
                    timestamp_us: 0,
                    duration_us: 0,
                    allowed: false,
                    denied_reason: None,
                };
                256
            ],
            op_index: 0,
        }
    }

    /// Inject keyboard event
    pub fn inject_key_event(&mut self, has_input_cap: bool) -> bool {
        let allowed = has_input_cap && self.input_queue_depth < 64;

        if allowed {
            self.key_events += 1;
            self.input_queue_depth += 1;
        } else {
            self.dropped_events += 1;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::InputEventInject,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: if allowed { 20 } else { 0 },
            allowed,
            denied_reason: if has_input_cap { None } else { Some("INPUT_CAPABILITY_DENIED") },
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    /// Inject mouse event
    pub fn inject_mouse_event(&mut self, has_input_cap: bool) -> bool {
        let allowed = has_input_cap && self.input_queue_depth < 64;

        if allowed {
            self.mouse_events += 1;
            self.input_queue_depth += 1;
        } else {
            self.dropped_events += 1;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::InputEventInject,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: if allowed { 15 } else { 0 },
            allowed,
            denied_reason: if has_input_cap { None } else { Some("INPUT_CAPABILITY_DENIED") },
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    pub fn dequeue_event(&mut self) {
        if self.input_queue_depth > 0 {
            self.input_queue_depth -= 1;
        }
    }

    pub fn get_event_statistics(&self) -> (u32, u32, u32, u32) {
        (self.key_events, self.mouse_events, self.touch_events, self.dropped_events)
    }
}

/// Console device handler with audit logging
pub struct VirtioConsoleHandler {
    vm_id: u32,
    bytes_written: u64,
    bytes_read: u64,
    write_operations: u32,
    read_operations: u32,
    operations: [DeviceOperation; 256],
    op_index: usize,
}

impl VirtioConsoleHandler {
    pub fn new(vm_id: u32) -> Self {
        VirtioConsoleHandler {
            vm_id,
            bytes_written: 0,
            bytes_read: 0,
            write_operations: 0,
            read_operations: 0,
            operations: [
                DeviceOperation {
                    op_type: DeviceOperationType::ConsoleWrite,
                    vm_id: 0,
                    timestamp_us: 0,
                    duration_us: 0,
                    allowed: false,
                    denied_reason: None,
                };
                256
            ],
            op_index: 0,
        }
    }

    /// Write to console (always allowed, audited)
    pub fn write_bytes(&mut self, size_bytes: u32) -> bool {
        self.bytes_written += size_bytes as u64;
        self.write_operations += 1;

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::ConsoleWrite,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: 30,
            allowed: true,
            denied_reason: None,
        };
        self.op_index = (self.op_index + 1) % 256;

        true
    }

    /// Read from console
    pub fn read_bytes(&mut self, size_bytes: u32, has_console_read_cap: bool) -> bool {
        let allowed = has_console_read_cap;

        if allowed {
            self.bytes_read += size_bytes as u64;
            self.read_operations += 1;
        }

        self.operations[self.op_index] = DeviceOperation {
            op_type: DeviceOperationType::ConsoleRead,
            vm_id: self.vm_id,
            timestamp_us: 0,
            duration_us: if allowed { 25 } else { 0 },
            allowed,
            denied_reason: if has_console_read_cap { None } else { Some("CONSOLE_READ_CAPABILITY_DENIED") },
        };
        self.op_index = (self.op_index + 1) % 256;

        allowed
    }

    pub fn get_io_statistics(&self) -> (u64, u64, u32, u32) {
        (self.bytes_written, self.bytes_read, self.write_operations, self.read_operations)
    }
}

/// Device handler manager for all virtio devices
pub struct DeviceHandlerManager {
    gpu_handlers: [Option<VirtioGpuHandler>; 8],
    net_handlers: [Option<VirtioNetHandler>; 8],
    blk_handlers: [Option<VirtioBlkHandler>; 8],
    input_handlers: [Option<VirtioInputHandler>; 8],
    console_handlers: [Option<VirtioConsoleHandler>; 8],
}

impl DeviceHandlerManager {
    pub fn new() -> Self {
        DeviceHandlerManager {
            gpu_handlers: Default::default(),
            net_handlers: Default::default(),
            blk_handlers: Default::default(),
            input_handlers: Default::default(),
            console_handlers: Default::default(),
        }
    }

    pub fn register_vm_devices(&mut self, vm_id: u32, vm_slot: usize) {
        if vm_slot < 8 {
            self.gpu_handlers[vm_slot] = Some(VirtioGpuHandler::new(vm_id, 128)); // 128 MB default
            self.net_handlers[vm_slot] = Some(VirtioNetHandler::new(vm_id, 100_000)); // 100 Mbps default
            self.blk_handlers[vm_slot] = Some(VirtioBlkHandler::new(vm_id, 10_240)); // 10 GB default
            self.input_handlers[vm_slot] = Some(VirtioInputHandler::new(vm_id));
            self.console_handlers[vm_slot] = Some(VirtioConsoleHandler::new(vm_id));
        }
    }

    pub fn get_total_operation_count(&self) -> u32 {
        let mut total = 0;
        for handler in &self.gpu_handlers {
            if let Some(h) = handler {
                total += h.get_operation_count();
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_handler_memory_quota() {
        let mut handler = VirtioGpuHandler::new(1000, 256);
        
        // Allocate memory with capability
        assert!(handler.allocate_memory(100 * 1024 * 1024, true));
        assert_eq!(handler.get_memory_usage_percent(), 39); // ~100MB / 256MB
        
        // Exceed quota
        assert!(!handler.allocate_memory(200 * 1024 * 1024, true));
        
        // Deny without capability
        assert!(!handler.allocate_memory(10 * 1024 * 1024, false));
    }

    #[test]
    fn test_network_handler_bandwidth() {
        let mut handler = VirtioNetHandler::new(1000, 100_000);
        
        // Allow TX with capability and firewall
        assert!(handler.transmit_packet(1500, true, true));
        
        // Deny without capability
        assert!(!handler.transmit_packet(1500, false, true));
        
        // Deny by firewall
        assert!(!handler.transmit_packet(1500, true, false));
        
        let (tx, rx, _, _, _) = handler.get_statistics();
        assert_eq!(tx, 1);
        assert_eq!(rx, 0);
    }

    #[test]
    fn test_disk_handler_quota() {
        let mut handler = VirtioBlkHandler::new(2000, 1024); // 1 GB quota
        
        // Allow read with capability
        assert!(handler.read_blocks(4096, true));
        
        // Allow write with capability
        assert!(handler.write_blocks(4096, true));
        
        // Deny without write capability
        assert!(!handler.write_blocks(4096, false));
        
        let (reads, writes, _, _, _) = handler.get_io_statistics();
        assert_eq!(reads, 1);
        assert_eq!(writes, 1);
    }

    #[test]
    fn test_input_handler_queue() {
        let mut handler = VirtioInputHandler::new(1000);
        
        // Allow key event with capability
        assert!(handler.inject_key_event(true));
        
        // Deny without capability
        assert!(!handler.inject_key_event(false));
        
        let (keys, _, _, dropped) = handler.get_event_statistics();
        assert_eq!(keys, 1);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn test_console_handler() {
        let mut handler = VirtioConsoleHandler::new(1000);
        
        // Write always allowed
        assert!(handler.write_bytes(100));
        
        // Read requires capability
        assert!(handler.read_bytes(50, true));
        assert!(!handler.read_bytes(50, false));
        
        let (written, read, _, _) = handler.get_io_statistics();
        assert_eq!(written, 100);
        assert_eq!(read, 50);
    }
}
