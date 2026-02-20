/// Get the number of milliseconds since the kernel started, with sub-tick
/// precision derived from the PIT counter.
pub fn get_monotonic_ms() -> u64 {
    let (lo, hi) = super::syscall_2(0x40, 0, 0, 0);
    (hi as u64) << 32 | lo as u64
}

/// Kernel sleep syscall. Yields the CPU for at least `ms` milliseconds.
/// Actual wake-up is rounded up to the next scheduler tick (~10ms granularity).
fn kernel_sleep(ms: u32) {
    super::syscall(0x02, ms, 0, 0);
}

/// The kernel scheduler tick period in milliseconds. Sleep durations are
/// rounded up to the next multiple of this value.
const TICK_MS: u64 = 10;

/// Sleep for the specified number of milliseconds with sub-tick precision.
/// Uses the kernel sleep syscall for the bulk of the wait (yielding the CPU),
/// then spin-waits on the monotonic clock for the final tick period.
pub fn sleep_ms(ms: u32) {
    if ms == 0 {
        return;
    }

    let start = get_monotonic_ms();
    let target = start + ms as u64;

    // For durations longer than one tick, kernel-sleep for the bulk and
    // yield the CPU to other tasks. We subtract one full tick period so
    // the kernel never overshoots our target.
    if ms as u64 > TICK_MS {
        kernel_sleep(ms - TICK_MS as u32);
    }

    // Spin for the remainder
    while get_monotonic_ms() < target {
        core::hint::spin_loop();
    }
}
