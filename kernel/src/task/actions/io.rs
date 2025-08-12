use core::sync::atomic::Ordering;

use idos_api::io::{error::IOError, AsyncOp};

use crate::{
    io::{
        async_io::{ASYNC_OP_CLOSE, ASYNC_OP_SHARE},
        handle::Handle,
        provider::IOResult,
    },
    memory::address::VirtualAddress,
    sync::futex::futex_wait,
    task::{id::TaskID, switching::get_current_task},
};

/// Enqueue an IO operation for the specified handle. Progress can be tracked
/// by the signal futex on the AsyncOp. If an optional Wake Set is provided,
/// the signal futex will be temporarily added to that Wake Set. When the IO
/// operation is completed, the address will be removed from the Wake Set.
pub fn send_io_op(handle: Handle, op: &AsyncOp, wake_set: Option<Handle>) -> Result<(), ()> {
    let task_lock = get_current_task();
    let code = op.op_code;
    let mut args = [op.args[0], op.args[1], op.args[2]];
    if code & 0xfff == ASYNC_OP_CLOSE {
        // Check how many copies of this handle are open
        // If it's only one, we need to issue the close op; if there are
        // multiple copies, just remove this particular reference.
        let mut task = task_lock.write();
        // TODO: use a real error type for this
        let io_index = task.open_handles.get(handle).ok_or(())?.clone();
        let ref_count = task
            .async_io_table
            .get_reference_count(io_index)
            .ok_or(())?;
        if ref_count > 1 {
            // if there are multiple open handles, just remove the reference
            // but don't actually close the provider
            task.async_io_table.remove_reference(io_index);
            task.open_handles.remove(handle);

            op.return_value.store(1, Ordering::SeqCst);
            op.signal.store(1, Ordering::SeqCst);
            return Ok(());
        }
        // if this is the only handle, it needs to be closed on driver success.
        // args[0] is the original user-facing handle, so that it can be
        // removed on success
        args[0] = *handle as u32;
    } else if code & 0xfff == ASYNC_OP_SHARE {
        // Sharing a handle will remove the Handle itself if successful.
        // The backing IO Provider may be transferred if it is the only
        // instance, or it will be cloned if there are multiple references.
        // op.args[0] is the new task ID to share with, and is the only arg
        // set by the user.
        // op.args[1] should determine if this is a move or a duplicate action.
        // If there are multiple references to the same IO instance, this must
        // be a duplicate (0). If this is the only reference, it can be a move (1).
        // op.args[2] should be set to the original user-facing handle, so that
        // it can be removed on success
        let mut task = task_lock.read();
        let io_index = task.open_handles.get(handle).ok_or(())?.clone();
        let ref_count = task
            .async_io_table
            .get_reference_count(io_index)
            .ok_or(())?;
        args[1] = if ref_count > 1 { 0 } else { 1 }; // non-zero means is_move
        args[2] = *handle as u32;
    }

    let (io_instance, io_type) = {
        let task_guard = task_lock.read();
        let io_instance = task_guard.open_handles.get(handle).ok_or(())?.clone();
        let io = task_guard.async_io_table.get(io_instance).ok_or(())?;
        (io_instance, io.io_type.clone())
    };

    io_type.op_request(io_instance, op, args, wake_set);

    Ok(())
}

pub fn driver_io_complete(request_id: u32, return_value: IOResult) {
    crate::io::driver::pending::request_complete(request_id, return_value);
}

pub fn open_sync(handle: Handle, path: &str) -> IOResult {
    use crate::io::async_io::ASYNC_OP_OPEN;

    let path_ptr = path.as_ptr() as u32;
    let path_len = path.len() as u32;
    io_sync(handle, ASYNC_OP_OPEN, path_ptr, path_len, 0)
}

pub fn read_sync(handle: Handle, buffer: &mut [u8], offset: u32) -> IOResult {
    use crate::io::async_io::ASYNC_OP_READ;

    let buffer_ptr = buffer.as_ptr() as u32;
    let buffer_len = buffer.len() as u32;
    io_sync(handle, ASYNC_OP_READ, buffer_ptr, buffer_len, offset)
}

pub fn read_struct_sync<T: Sized>(handle: Handle, struct_ref: &mut T, offset: u32) -> IOResult {
    use crate::io::async_io::ASYNC_OP_READ;

    let ptr = struct_ref as *mut T as u32;
    let len = core::mem::size_of::<T>() as u32;
    io_sync(handle, ASYNC_OP_READ, ptr, len, offset)
}

pub fn write_sync(handle: Handle, buffer: &[u8], offset: u32) -> IOResult {
    use crate::io::async_io::ASYNC_OP_WRITE;

    let buffer_ptr = buffer.as_ptr() as u32;
    let buffer_len = buffer.len() as u32;
    io_sync(handle, ASYNC_OP_WRITE, buffer_ptr, buffer_len, offset)
}

pub fn write_struct_sync<T: Sized>(handle: Handle, struct_ref: &T) -> IOResult {
    use crate::io::async_io::ASYNC_OP_WRITE;

    let ptr = struct_ref as *const T as u32;
    let len = core::mem::size_of::<T>() as u32;
    io_sync(handle, ASYNC_OP_WRITE, ptr, len, 0)
}

pub fn close_sync(handle: Handle) -> IOResult {
    io_sync(handle, ASYNC_OP_CLOSE, 0, 0, 0)
}

pub fn share_sync(handle: Handle, transfer_to: TaskID) -> IOResult {
    io_sync(handle, ASYNC_OP_SHARE, transfer_to.into(), 0, 0)
}

pub fn io_sync(handle: Handle, op_code: u32, arg0: u32, arg1: u32, arg2: u32) -> IOResult {
    let async_op = AsyncOp::new(op_code, arg0, arg1, arg2);
    send_io_op(handle, &async_op, None).unwrap();

    while async_op.signal.load(Ordering::SeqCst) == 0 {
        futex_wait(VirtualAddress::new(async_op.signal_address()), 0, None);
    }

    let return_value = async_op.return_value.load(Ordering::SeqCst);

    if return_value & 0x80000000 != 0 {
        let io_error = IOError::try_from(return_value & 0x7fffffff).unwrap_or(IOError::Unknown);
        Err(io_error)
    } else {
        Ok(return_value)
    }
}
