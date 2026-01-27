use core::sync::atomic::{AtomicU32, Ordering};

use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::{
    io::{
        async_io::{AsyncOpID, ASYNC_OP_CLOSE, ASYNC_OP_SHARE},
        filesystem::driver::AsyncIOCallback,
        handle::Handle,
    },
    task::{
        actions::send_message, id::TaskID, map::get_task, scheduling::reenqueue_task,
        switching::get_current_id,
    },
};

use super::comms::DriverIoAction;

use idos_api::io::error::{IoError, IoResult};

struct IncomingRequest {
    // information on the handle/op performing the async action:
    /// The ID of the driver task
    pub driver_id: TaskID,
    /// The ID of the originating task
    pub source_task: TaskID,
    /// The index of the async io handle pointing to this file
    pub source_io: u32,
    /// The individual async op
    pub source_op: AsyncOpID,

    // the actual action data:
    /// The action to encode and send to the driver
    pub action: DriverIoAction,
}

static PENDING_REQUESTS: Mutex<BTreeMap<u32, IncomingRequest>> = Mutex::new(BTreeMap::new());
static NEXT_REQUEST: AtomicU32 = AtomicU32::new(0);

pub fn send_async_request(driver_id: TaskID, io_callback: AsyncIOCallback, action: DriverIoAction) {
    let request = IncomingRequest {
        driver_id,
        source_task: io_callback.0,
        source_io: io_callback.1,
        source_op: io_callback.2,
        action,
    };
    let request_id = NEXT_REQUEST.fetch_add(1, Ordering::SeqCst);
    let message = request.action.encode_to_message(request_id);
    PENDING_REQUESTS.lock().insert(request_id, request);
    send_message(driver_id, message, 0xffffffff);
}

pub fn request_complete(request_id: u32, return_value: IoResult) {
    let current_id = get_current_id();
    let pending_request = PENDING_REQUESTS.lock().remove(&request_id);
    let Some(request) = pending_request else {
        return;
    };
    if request.driver_id != current_id {
        // TODO: shouldn't be a panic, should be an error
        panic!("Can't respond to a request for a different driver");
    }
    let Some(task_lock) = get_task(request.source_task) else {
        return;
    };

    // file mapping operations don't have per-op completions
    match request.action {
        DriverIoAction::CreateFileMapping { .. }
        | DriverIoAction::RemoveFileMapping { .. }
        | DriverIoAction::PageInFileMapping { .. } => {
            task_lock.write().resolve_file_mapping_request(return_value);
            reenqueue_task(request.source_task);
            return;
        }
        _ => (),
    }

    let io_entry = task_lock.read().async_io_complete(request.source_io);
    let Some(entry) = io_entry else {
        return;
    };
    let Some(op) = entry
        .inner()
        .async_complete(request.source_op, return_value.clone())
    else {
        return;
    };
    // If the operation was successful, and it was a close or share,
    // we need to remove the original handle. It would be incorrect
    // to prematurely remove it before the result is known.
    if let Ok(_) = return_value {
        op.maybe_close_handle(task_lock, request.source_io);
    }
}
