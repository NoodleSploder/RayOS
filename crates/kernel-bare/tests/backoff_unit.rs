use std::sync::atomic::Ordering;

// This unit test exercises the backoff retry helper in a host test environment.
// It sets runtime force-fail flag, initializes the pending state, advances
// the TIMER_TICKS counter, calls the retry helper repeatedly, and asserts
// that metrics counters reflect observed behavior.

#[test]
fn backoff_retry_unit_test() {
    // Import the internal statics and helpers from the hypervisor module.
    use rayos_kernel_bare::hypervisor::*;
    use rayos_kernel_bare::TIMER_TICKS;

    // Ensure injector will fail.
    INJECT_FORCE_FAIL.store(1, Ordering::Relaxed);

    // Initialize state to pending with zero attempts.
    VIRTIO_MMIO_STATE.interrupt_pending.store(1, Ordering::Relaxed);
    VIRTIO_MMIO_STATE
        .interrupt_pending_attempts
        .store(0, Ordering::Relaxed);
    VIRTIO_MMIO_STATE
        .interrupt_pending_last_tick
        .store(0, Ordering::Relaxed);

    // Reset metrics.
    VIRTIO_MMIO_STATE
        .interrupt_backoff_total_attempts
        .store(0, Ordering::Relaxed);
    VIRTIO_MMIO_STATE
        .interrupt_backoff_succeeded
        .store(0, Ordering::Relaxed);
    VIRTIO_MMIO_STATE
        .interrupt_backoff_failed_max
        .store(0, Ordering::Relaxed);

    // Run retries until the helper gives up (MAX_INT_INJECT_ATTEMPTS attempts).
    for i in 0..(MAX_INT_INJECT_ATTEMPTS + 2) {
        // Advance timer sufficiently each iteration to allow a retry.
        TIMER_TICKS.store(i as u64 * 256, Ordering::Relaxed);
        try_retry_pending_if_due();
    }

    // After the loop, the failed_max counter should have been incremented by 1.
    let failed = VIRTIO_MMIO_STATE
        .interrupt_backoff_failed_max
        .load(Ordering::Relaxed);
    assert!(failed >= 1, "expected at least one failed-max event");

    let total = VIRTIO_MMIO_STATE
        .interrupt_backoff_total_attempts
        .load(Ordering::Relaxed);
    assert!(total >= MAX_INT_INJECT_ATTEMPTS, "expected attempts >= MAX");

    // Ensure no successful injections were recorded.
    let succ = VIRTIO_MMIO_STATE
        .interrupt_backoff_succeeded
        .load(Ordering::Relaxed);
    assert_eq!(succ, 0);

    // Clear force.
    INJECT_FORCE_FAIL.store(0, Ordering::Relaxed);
}