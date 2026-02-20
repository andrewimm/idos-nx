/// Get the number of milliseconds since the kernel started, with sub-tick
/// precision derived from the PIT counter.
pub fn get_monotonic_ms() -> u64 {
    let (lo, hi) = super::syscall_2(0x40, 0, 0, 0);
    (hi as u64) << 32 | lo as u64
}
