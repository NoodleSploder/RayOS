use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Same bound as kernel implementation
pub const MAX_INT_INJECT_ATTEMPTS: u32 = 5;

#[derive(Default)]
pub struct BackoffMetrics {
    pub total_attempts: AtomicU32,
    pub succeeded: AtomicU32,
    pub failed_max: AtomicU32,
}

pub struct BackoffState {
    pub pending: AtomicU32,
    pub attempts: AtomicU32,
    pub last_tick: AtomicU64,
    pub metrics: Arc<BackoffMetrics>,
}

impl BackoffState {
    pub fn new() -> Self {
        Self {
            pending: AtomicU32::new(0),
            attempts: AtomicU32::new(0),
            last_tick: AtomicU64::new(0),
            metrics: Arc::new(BackoffMetrics::default()),
        }
    }

    /// Attempt a retry if due. The `now_tick` is provided by the test harness
    /// and `inject` is a closure that returns whether injection succeeded.
    pub fn try_retry_pending_if_due<F>(&self, now_tick: u64, mut inject: F)
    where
        F: FnMut() -> bool,
    {
        if self.pending.load(Ordering::Relaxed) == 0 {
            return;
        }

        let attempts = self.attempts.load(Ordering::Relaxed);
        // Exponential backoff spacing heuristic: require at least (1 << attempts) * 128 ticks
        let min_wait = (1u64 << attempts) * 128;
        let last = self.last_tick.load(Ordering::Relaxed);
        if now_tick < last + min_wait && attempts > 0 {
            // Not due yet
            return;
        }

        // We're going to attempt
        self.metrics.total_attempts.fetch_add(1, Ordering::Relaxed);
        // remember we attempted at this tick
        self.last_tick.store(now_tick, Ordering::Relaxed);

        if inject() {
            // success
            self.metrics.succeeded.fetch_add(1, Ordering::Relaxed);
            self.pending.store(0, Ordering::Relaxed);
            self.attempts.store(0, Ordering::Relaxed);
        } else {
            // failure
            let new_attempts = self.attempts.fetch_add(1, Ordering::Relaxed) + 1;
            if new_attempts >= MAX_INT_INJECT_ATTEMPTS {
                self.metrics.failed_max.fetch_add(1, Ordering::Relaxed);
                self.pending.store(0, Ordering::Relaxed);
            }
        }
    }
}
