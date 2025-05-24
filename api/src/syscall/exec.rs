use core::sync::atomic::AtomicU32;

use crate::io::handle::Handle;

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

pub fn create_task() -> (Handle, u32) {
    let (handle, task_id) = super::syscall_2(0x20, 0, 0, 0);
    (Handle::new(handle), task_id)
}

pub fn load_executable(task_id: u32, path: &str) {
    let path_ptr = path.as_ptr() as u32;
    let path_len = path.len() as u32;
    super::syscall(0x06, task_id, path_ptr, path_len);
}
