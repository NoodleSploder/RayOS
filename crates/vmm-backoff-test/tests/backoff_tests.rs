use std::sync::atomic::Ordering;
use vmm_backoff_test::*;

#[test]
fn backoff_retry_failure_exercises_failed_max() {
    let s = BackoffState::new();

    // Set pending
    s.pending.store(1, Ordering::Relaxed);
    s.attempts.store(0, Ordering::Relaxed);
    s.last_tick.store(0, Ordering::Relaxed);

    // Reset metrics
    s.metrics.total_attempts.store(0, Ordering::Relaxed);
    s.metrics.succeeded.store(0, Ordering::Relaxed);
    s.metrics.failed_max.store(0, Ordering::Relaxed);

    // Force inject to always fail. Advance time according to backoff windows
    // so each retry becomes "due" deterministically.
    let mut iter = 0;
    while s.pending.load(Ordering::Relaxed) != 0 && iter < 200 {
        // Compute a now tick that is past the backoff window to force an attempt.
        let now = s.last_tick.load(Ordering::Relaxed)
            .saturating_add((1u64 << s.attempts.load(Ordering::Relaxed)) * 128)
            .saturating_add(1);
        s.try_retry_pending_if_due(now, || false);
        let total = s.metrics.total_attempts.load(Ordering::Relaxed);
        eprintln!("iter {} now={} total_attempts={} attempts={} pending={}", iter, now, total, s.attempts.load(Ordering::Relaxed), s.pending.load(Ordering::Relaxed));
        iter += 1;
    }

    let failed = s.metrics.failed_max.load(Ordering::Relaxed);
    assert!(failed >= 1, "expected at least one failed-max event");

    let total = s.metrics.total_attempts.load(Ordering::Relaxed);
    assert!(total >= MAX_INT_INJECT_ATTEMPTS, "expected attempts >= MAX");

    let succ = s.metrics.succeeded.load(Ordering::Relaxed);
    assert_eq!(succ, 0, "no successful injections expected");
}

#[test]
fn backoff_retry_succeeds_clears_pending() {
    let s = BackoffState::new();
    s.pending.store(1, Ordering::Relaxed);
    s.attempts.store(0, Ordering::Relaxed);
    s.last_tick.store(0, Ordering::Relaxed);

    // succeed on the 3rd effective attempt. Use a counter to control when the
    // injector returns true.
    let call_count = std::sync::atomic::AtomicU32::new(0);
    let mut iter = 0;
    while s.pending.load(Ordering::Relaxed) != 0 && iter < 200 {
        let now = s.last_tick.load(Ordering::Relaxed)
            .saturating_add((1u64 << s.attempts.load(Ordering::Relaxed)) * 128)
            .saturating_add(1);
        s.try_retry_pending_if_due(now, || {
            let c = call_count.fetch_add(1, Ordering::Relaxed);
            // succeed when we've been called 2 times already (0-based), i.e., on the 3rd call
            c >= 2
        });
        iter += 1;
    }

    let succ = s.metrics.succeeded.load(Ordering::Relaxed);
    assert_eq!(succ, 1, "expected one successful injection");
    assert_eq!(s.pending.load(Ordering::Relaxed), 0, "pending cleared on success");
}
