use crate::io::handle::Handle;
use crate::io::AsyncOp;

use super::syscall;

pub fn append_io_op(handle: Handle, async_op: &AsyncOp, wait_set: Option<Handle>) -> u32 {
    syscall(
        0x10,
        handle.as_u32(),
        async_op as *const AsyncOp as u32,
        wait_set.map(|h| h.as_u32()).unwrap_or(0xffff_ffff),
    )
}
