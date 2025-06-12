use crate::io::handle::Handle;
use crate::io::AsyncOp;

use super::syscall;

pub fn create_message_queue_handle() -> Handle {
    Handle::new(syscall(0x21, 0, 0, 0))
}

pub fn create_file_handle() -> Handle {
    Handle::new(syscall(0x23, 0, 0, 0))
}

pub fn futex_wake(address: u32, count: u32) {
    syscall(0x14, address, count, 0);
}

pub fn create_wake_set() -> Handle {
    Handle::new(syscall(0x15, 0, 0, 0))
}

pub fn block_on_wake_set(handle: Handle, timeout: Option<u32>) -> u32 {
    let timeout_value = timeout.unwrap_or(0xffff_ffff);
    syscall(0x16, handle.as_u32(), timeout_value, 0)
}

pub fn append_io_op(handle: Handle, async_op: &AsyncOp, wait_set: Option<Handle>) -> u32 {
    syscall(
        0x10,
        handle.as_u32(),
        async_op as *const AsyncOp as u32,
        wait_set.map(|h| h.as_u32()).unwrap_or(0xffff_ffff),
    )
}
