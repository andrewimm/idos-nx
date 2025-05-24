use core::sync::atomic::Ordering;

use super::error::{IOError, IOResult};
use super::handle::Handle;
use super::AsyncOp;

use crate::syscall::exec::futex_wait_u32;
use crate::syscall::io::append_io_op;

pub fn io_sync(handle: Handle, op_code: u32, arg0: u32, arg1: u32, arg2: u32) -> IOResult {
    let async_op = AsyncOp::new(op_code, arg0, arg1, arg2);
    append_io_op(handle, &async_op, None);

    while async_op.signal.load(Ordering::SeqCst) == 0 {
        futex_wait_u32(&async_op.signal, 0, None);
    }

    let return_value = async_op.return_value.load(Ordering::SeqCst);

    if return_value & 0x80000000 != 0 {
        let io_error = IOError::try_from(return_value & 0x7fffffff).unwrap_or(IOError::Unknown);
        Err(io_error)
    } else {
        Ok(return_value)
    }
}

pub fn open_sync(handle: Handle, path: &str) -> IOResult {
    use crate::io::ASYNC_OP_OPEN;

    let path_ptr = path.as_ptr() as u32;
    let path_len = path.len() as u32;
    io_sync(handle, ASYNC_OP_OPEN, path_ptr, path_len, 0)
}

pub fn read_sync(handle: Handle, buffer: &mut [u8], offset: u32) -> IOResult {
    use crate::io::ASYNC_OP_READ;

    let buffer_ptr = buffer.as_ptr() as u32;
    let buffer_len = buffer.len() as u32;
    io_sync(handle, ASYNC_OP_READ, buffer_ptr, buffer_len, offset)
}

pub fn write_sync(handle: Handle, buffer: &[u8], offset: u32) -> IOResult {
    use crate::io::ASYNC_OP_WRITE;

    let buffer_ptr = buffer.as_ptr() as u32;
    let buffer_len = buffer.len() as u32;
    io_sync(handle, ASYNC_OP_WRITE, buffer_ptr, buffer_len, offset)
}

pub fn close_sync(handle: Handle) -> IOResult {
    use crate::io::ASYNC_OP_CLOSE;

    io_sync(handle, ASYNC_OP_CLOSE, 0, 0, 0)
}
