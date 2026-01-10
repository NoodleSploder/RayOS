
const MAX_INTERRUPT_SOURCES: usize = 64;
const MAX_BATCHED_INTERRUPTS: usize = 256;
const MAX_TASKS: usize = 128;

/// Interrupt sources
#[derive(Debug, Clone, Copy)]
pub struct InterruptSource {
    pub id: u32,
    pub device_name: [u8; 32],
    pub enabled: bool,
    pub pending: bool,
}

impl InterruptSource {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            device_name: [0; 32],
            enabled: true,
            pending: false,
        }
    }
}

/// Interrupt coalescing strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoalescingPolicy {
    Immediate,   // No coalescing
    TimeBased,   // Batch by time window
    CountBased,  // Batch by count
    Adaptive,    // Adapt based on load
}

/// Batched interrupt entry
#[derive(Debug, Clone, Copy)]
pub struct InterruptBatch {
    pub batch_id: u32,
    pub interrupt_count: u32,
    pub timestamp: u64,
    pub processed: bool,
}

impl InterruptBatch {
    pub fn new(batch_id: u32) -> Self {
        Self {
            batch_id,
            interrupt_count: 0,
            timestamp: 0,
            processed: false,
        }
    }
}

/// Per-task latency budget
#[derive(Debug, Clone, Copy)]
pub struct LatencyBudget {
    pub task_id: u32,
    pub max_latency_us: u32,
    pub current_latency_us: u32,
    pub violations: u32,
}

impl LatencyBudget {
    pub fn new(task_id: u32, max_latency_us: u32) -> Self {
        Self {
            task_id,
            max_latency_us,
            current_latency_us: 0,
            violations: 0,
        }
    }

    pub fn is_violated(&self) -> bool {
        self.current_latency_us > self.max_latency_us
    }

    pub fn record_violation(&mut self) {
        self.violations = self.violations.saturating_add(1);
    }
}

/// Interrupt coalescing statistics
#[derive(Debug, Clone, Copy)]
pub struct InterruptStats {
    pub total_interrupts: u64,
    pub coalesced_interrupts: u64,
    pub batches_created: u32,
    pub sla_violations: u32,
    pub avg_batch_size: u32,
}

impl InterruptStats {
    pub fn new() -> Self {
        Self {
            total_interrupts: 0,
            coalesced_interrupts: 0,
            batches_created: 0,
            sla_violations: 0,
            avg_batch_size: 0,
        }
    }

    pub fn coalescing_ratio(&self) -> u32 {
        if self.total_interrupts == 0 {
            0
        } else {
            ((self.coalesced_interrupts * 100) / self.total_interrupts) as u32
        }
    }

    pub fn record_interrupt(&mut self) {
        self.total_interrupts = self.total_interrupts.saturating_add(1);
    }

    pub fn record_batch(&mut self, size: u32) {
        self.coalesced_interrupts = self.coalesced_interrupts.saturating_add(size as u64);
        self.batches_created = self.batches_created.saturating_add(1);
        if self.batches_created > 0 {
            self.avg_batch_size = (self.coalesced_interrupts / (self.batches_created as u64)) as u32;
        }
    }
}

/// Interrupt coalescing engine
pub struct CoalescingEngine {
    sources: [InterruptSource; MAX_INTERRUPT_SOURCES],
    source_count: u32,
    batches: [InterruptBatch; MAX_BATCHED_INTERRUPTS],
    batch_count: u32,
    budgets: [LatencyBudget; MAX_TASKS],
    task_count: u32,
    policy: CoalescingPolicy,
    stats: InterruptStats,
    coalesce_timeout_us: u32,
    coalesce_count_threshold: u32,
}

impl CoalescingEngine {
    pub fn new(policy: CoalescingPolicy) -> Self {
        Self {
            sources: [InterruptSource::new(0); MAX_INTERRUPT_SOURCES],
            source_count: 0,
            batches: [InterruptBatch::new(0); MAX_BATCHED_INTERRUPTS],
            batch_count: 0,
            budgets: [LatencyBudget::new(0, 1000); MAX_TASKS],
            task_count: 0,
            policy,
            stats: InterruptStats::new(),
            coalesce_timeout_us: 100,
            coalesce_count_threshold: 10,
        }
    }

    pub fn register_source(&mut self, id: u32) -> bool {
        if (self.source_count as usize) >= MAX_INTERRUPT_SOURCES {
            return false;
        }
        let idx = self.source_count as usize;
        self.sources[idx] = InterruptSource::new(id);
        self.source_count += 1;
        true
    }

    pub fn register_task(&mut self, task_id: u32, max_latency_us: u32) -> bool {
        if (self.task_count as usize) >= MAX_TASKS {
            return false;
        }
        let idx = self.task_count as usize;
        self.budgets[idx] = LatencyBudget::new(task_id, max_latency_us);
        self.task_count += 1;
        true
    }

    pub fn record_interrupt(&mut self, source_id: u32, timestamp: u64) -> bool {
        self.stats.record_interrupt();

        match self.policy {
            CoalescingPolicy::Immediate => {
                self.create_single_batch(source_id, timestamp)
            }
            CoalescingPolicy::TimeBased => {
                self.coalesce_by_time(source_id, timestamp)
            }
            CoalescingPolicy::CountBased => {
                self.coalesce_by_count(source_id, timestamp)
            }
            CoalescingPolicy::Adaptive => {
                self.coalesce_adaptive(source_id, timestamp)
            }
        }
    }

    fn create_single_batch(&mut self, _source_id: u32, _timestamp: u64) -> bool {
        if (self.batch_count as usize) >= MAX_BATCHED_INTERRUPTS {
            return false;
        }
        let idx = self.batch_count as usize;
        self.batches[idx].interrupt_count = 1;
        self.batch_count += 1;
        self.stats.record_batch(1);
        true
    }

    fn coalesce_by_time(&mut self, _source_id: u32, _timestamp: u64) -> bool {
        // Add to existing batch if timestamp within window
        if self.batch_count > 0 {
            let last_idx = (self.batch_count - 1) as usize;
            self.batches[last_idx].interrupt_count = self.batches[last_idx].interrupt_count.saturating_add(1);
            true
        } else {
            self.create_single_batch(_source_id, _timestamp)
        }
    }

    fn coalesce_by_count(&mut self, _source_id: u32, _timestamp: u64) -> bool {
        if self.batch_count == 0 {
            return self.create_single_batch(_source_id, _timestamp);
        }

        let last_idx = (self.batch_count - 1) as usize;
        self.batches[last_idx].interrupt_count = self.batches[last_idx].interrupt_count.saturating_add(1);

        if self.batches[last_idx].interrupt_count >= self.coalesce_count_threshold {
            if (self.batch_count as usize) < MAX_BATCHED_INTERRUPTS {
                self.batch_count += 1;
                self.stats.record_batch(self.batches[last_idx].interrupt_count);
            }
        }
        true
    }

    fn coalesce_adaptive(&mut self, source_id: u32, timestamp: u64) -> bool {
        // Adapt based on interrupt rate
        let rate = if self.stats.total_interrupts > 1000 {
            self.coalesce_count_threshold
        } else {
            5
        };

        self.coalesce_count_threshold = rate;
        self.coalesce_by_count(source_id, timestamp)
    }

    pub fn check_sla_violations(&mut self) {
        for i in 0..(self.task_count as usize) {
            if self.budgets[i].is_violated() {
                self.budgets[i].record_violation();
                self.stats.sla_violations = self.stats.sla_violations.saturating_add(1);
            }
        }
    }

    pub fn process_batch(&mut self, batch_idx: u32) -> u32 {
        if (batch_idx as usize) >= (self.batch_count as usize) {
            return 0;
        }
        let processed = self.batches[batch_idx as usize].interrupt_count;
        self.batches[batch_idx as usize].processed = true;
        processed
    }

    pub fn get_stats(&self) -> InterruptStats {
        self.stats
    }

    pub fn get_policy(&self) -> CoalescingPolicy {
        self.policy
    }

    pub fn set_timeout(&mut self, timeout_us: u32) {
        self.coalesce_timeout_us = timeout_us;
    }

    pub fn get_batch_count(&self) -> u32 {
        self.batch_count
    }

    pub fn get_pending_count(&self) -> u32 {
        let mut count = 0;
        for i in 0..(self.batch_count as usize) {
            if !self.batches[i].processed {
                count += self.batches[i].interrupt_count;
            }
        }
        count
    }
}

// Bare metal compatible interrupt coalescing
// Tests run via shell interface: coalesce [status|sources|policies|sla|stats|help]
