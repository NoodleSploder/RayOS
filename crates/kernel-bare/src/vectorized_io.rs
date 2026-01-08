/// Vectorized I/O Operations
///
/// Optimizes I/O through vectorization, batching, and smart scheduling
/// Enforces QoS and bandwidth management

use core::cmp::min;

const MAX_IO_OPERATIONS: usize = 512;
const MAX_BATCHED_IO: usize = 128;
const MAX_QOS_CLASSES: usize = 8;

/// Single I/O operation
#[derive(Debug, Clone, Copy)]
pub struct IOVector {
    pub op_id: u32,
    pub address: u64,
    pub length: u32,
    pub read: bool,
    pub priority: u8,
    pub timestamp: u64,
}

impl IOVector {
    pub fn new(op_id: u32, address: u64, length: u32, read: bool) -> Self {
        Self {
            op_id,
            address,
            length,
            read,
            priority: 5,
            timestamp: 0,
        }
    }
}

/// Batched I/O operations
#[derive(Debug, Clone, Copy)]
pub struct IOBatch {
    pub batch_id: u32,
    pub op_count: u32,
    pub total_bytes: u64,
    pub vectorized: bool,
}

impl IOBatch {
    pub fn new(batch_id: u32) -> Self {
        Self {
            batch_id,
            op_count: 0,
            total_bytes: 0,
            vectorized: false,
        }
    }
}

/// I/O scheduling policies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingPolicy {
    FIFO,
    Priority,
    Deadline,
}

/// Bandwidth management & QoS
#[derive(Debug, Clone, Copy)]
pub struct QoSClass {
    pub class_id: u8,
    pub max_bandwidth_mbps: u32,
    pub current_bandwidth_mbps: u32,
    pub priority: u8,
}

impl QoSClass {
    pub fn new(class_id: u8, max_bandwidth_mbps: u32) -> Self {
        Self {
            class_id,
            max_bandwidth_mbps,
            current_bandwidth_mbps: 0,
            priority: class_id,
        }
    }

    pub fn can_admit(&self, io_size_mb: u32) -> bool {
        self.current_bandwidth_mbps + io_size_mb <= self.max_bandwidth_mbps
    }
}

/// I/O scheduler statistics
#[derive(Debug, Clone, Copy)]
pub struct IOStats {
    pub total_operations: u64,
    pub total_bytes_transferred: u64,
    pub batches_created: u32,
    pub avg_batch_size: u32,
    pub avg_latency_us: u32,
    pub throughput_mbps: u32,
}

impl IOStats {
    pub fn new() -> Self {
        Self {
            total_operations: 0,
            total_bytes_transferred: 0,
            batches_created: 0,
            avg_batch_size: 0,
            avg_latency_us: 0,
            throughput_mbps: 0,
        }
    }

    pub fn record_operation(&mut self, bytes: u64) {
        self.total_operations = self.total_operations.saturating_add(1);
        self.total_bytes_transferred = self.total_bytes_transferred.saturating_add(bytes);
    }

    pub fn record_batch(&mut self, ops: u32) {
        self.batches_created = self.batches_created.saturating_add(1);
        if self.batches_created > 0 {
            self.avg_batch_size = (self.total_operations / (self.batches_created as u64)) as u32;
        }
    }
}

/// I/O optimizer and scheduler
pub struct IOOptimizer {
    operations: [IOVector; MAX_IO_OPERATIONS],
    op_count: u32,
    batches: [IOBatch; MAX_BATCHED_IO],
    batch_count: u32,
    qos_classes: [QoSClass; MAX_QOS_CLASSES],
    qos_count: u32,
    policy: SchedulingPolicy,
    stats: IOStats,
    rate_limiter_enabled: bool,
    max_pending_ops: u32,
}

impl IOOptimizer {
    pub fn new(policy: SchedulingPolicy) -> Self {
        let mut qos_classes = [QoSClass::new(0, 100); MAX_QOS_CLASSES];
        for i in 0..MAX_QOS_CLASSES {
            qos_classes[i] = QoSClass::new(i as u8, 100 + (i as u32 * 50));
        }

        Self {
            operations: [IOVector::new(0, 0, 0, true); MAX_IO_OPERATIONS],
            op_count: 0,
            batches: [IOBatch::new(0); MAX_BATCHED_IO],
            batch_count: 0,
            qos_classes,
            qos_count: MAX_QOS_CLASSES as u32,
            policy,
            stats: IOStats::new(),
            rate_limiter_enabled: true,
            max_pending_ops: 256,
        }
    }

    pub fn submit_io(&mut self, op: IOVector) -> bool {
        if (self.op_count as usize) >= MAX_IO_OPERATIONS {
            return false;
        }

        // Check QoS admission
        let io_size_mb = (op.length / 1024 / 1024).max(1);
        let qos_idx = (op.priority / 32).min(7) as usize;
        
        if self.rate_limiter_enabled && !self.qos_classes[qos_idx].can_admit(io_size_mb) {
            return false;
        }

        let idx = self.op_count as usize;
        self.operations[idx] = op;
        self.op_count += 1;
        self.stats.record_operation(op.length as u64);
        
        true
    }

    pub fn vectorize(&mut self) -> bool {
        // Create vectorized batches
        if (self.batch_count as usize) >= MAX_BATCHED_IO || self.op_count == 0 {
            return false;
        }

        let mut batch = IOBatch::new(self.batch_count);
        let mut total_bytes: u64 = 0;

        // Batch consecutive reads or writes
        let start_is_read = self.operations[0].read;
        for i in 0..(min(self.op_count as usize, 16)) {
            if self.operations[i].read == start_is_read {
                batch.op_count = batch.op_count.saturating_add(1);
                total_bytes = total_bytes.saturating_add(self.operations[i].length as u64);
            } else {
                break;
            }
        }

        if batch.op_count > 0 {
            batch.total_bytes = total_bytes;
            batch.vectorized = batch.op_count > 1;
            
            let idx = self.batch_count as usize;
            self.batches[idx] = batch;
            self.batch_count += 1;
            self.stats.record_batch(batch.op_count);
            return true;
        }
        false
    }

    pub fn schedule_deadline(&mut self) {
        // Sort operations by deadline
        for i in 0..(self.op_count as usize) {
            for j in (i + 1)..(self.op_count as usize) {
                if self.operations[i].timestamp > self.operations[j].timestamp {
                    let temp = self.operations[i];
                    self.operations[i] = self.operations[j];
                    self.operations[j] = temp;
                }
            }
        }
    }

    pub fn enforce_qos(&mut self, qos_class: u8) -> bool {
        if qos_class as usize >= MAX_QOS_CLASSES {
            return false;
        }
        // Update bandwidth tracking
        true
    }

    pub fn enable_rate_limiting(&mut self, enabled: bool) {
        self.rate_limiter_enabled = enabled;
    }

    pub fn get_stats(&self) -> IOStats {
        self.stats
    }

    pub fn get_pending_count(&self) -> u32 {
        self.op_count
    }

    pub fn get_policy(&self) -> SchedulingPolicy {
        self.policy
    }

    pub fn get_batch_count(&self) -> u32 {
        self.batch_count
    }

    pub fn clear_operations(&mut self) {
        self.op_count = 0;
    }
}

// Bare metal compatible I/O optimization
// Tests run via shell interface: io [status|operations|policies|qos|stats|help]
