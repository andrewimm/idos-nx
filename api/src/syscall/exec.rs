pub fn terminate(code: u32) -> ! {
    super::syscall(0, code, 0, 0);
    unreachable!();
}
pub fn yield_coop() {
    super::syscall(6, 0, 0, 0);
}
