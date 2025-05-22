use core::sync::atomic::AtomicU32;

pub fn terminate(code: u32) -> ! {
    super::syscall(0, code, 0, 0);
    unreachable!();
}
pub fn yield_coop() {
    super::syscall(1, 0, 0, 0);
}

pub fn futex_wait_u32(atomic: &AtomicU32, value: u32, timeout_opt: Option<u32>) -> u32 {
    let timeout = timeout_opt.unwrap_or(0xffff_ffff);
    super::syscall(0x13, atomic.as_ptr() as u32, value, timeout)
}
