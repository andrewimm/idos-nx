use core::sync::atomic::{AtomicU32, Ordering};

use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::{
    io::{async_io::AsyncOpID, filesystem::driver::AsyncIOCallback},
    task::{actions::send_message, id::TaskID, map::get_task, switching::get_current_id},
};

use super::comms::{DriverIOAction, IOResult};

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
    pub action: DriverIOAction,
}

static PENDING_REQUESTS: Mutex<BTreeMap<u32, IncomingRequest>> = Mutex::new(BTreeMap::new());
static NEXT_REQUEST: AtomicU32 = AtomicU32::new(0);

pub fn send_async_request(driver_id: TaskID, io_callback: AsyncIOCallback, action: DriverIOAction) {
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

pub fn request_complete(request_id: u32, return_value: IOResult) {
    let current_id = get_current_id();
    let pending_request = PENDING_REQUESTS.lock().remove(&request_id);
    if let Some(request) = pending_request {
        if request.driver_id != current_id {
            panic!("Can't respond to a request for a different driver");
        }

        if let Some(task_lock) = get_task(request.source_task) {
            let io_entry = task_lock.read().async_io_complete(request.source_io);
            if let Some(entry) = io_entry {
                entry.inner().async_complete(
                    request.source_task,
                    request.source_io,
                    request.source_op,
                    return_value,
                );
            }
        }
    }
}
