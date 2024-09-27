use alloc::collections::{VecDeque, BTreeMap};
use idos_api::io::error::IOError;
use spin::{RwLock, Once, Mutex, MutexGuard};

use crate::{task::{id::TaskID, switching::{get_current_id, get_task}, actions::{handle::{open_message_queue, create_notify_queue, add_handle_to_notify_queue, wait_on_notify, handle_op_read_struct}, send_message}, messaging::Message}, memory::shared::SharedMemoryRange, io::{filesystem::driver::AsyncIOCallback, async_io::AsyncOpID}};

use crate::io::handle::PendingHandleOp;

use super::comms::{DriverIOAction, DRIVER_RESPONSE_MAGIC};

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
    /// A shared memory range, if needed to share a buffer with the driver
    pub shared_range: Option<SharedMemoryRange>,
}

static DRIVER_IO_TASK_ID: RwLock<TaskID> = RwLock::new(TaskID::new(0));

static INCOMING_QUEUE: Once<Mutex<VecDeque<IncomingRequest>>> = Once::new();

static PENDING_REQUESTS: Mutex<BTreeMap<u32, IncomingRequest>> = Mutex::new(BTreeMap::new());

pub fn get_driver_io_task_id() -> TaskID {
    *DRIVER_IO_TASK_ID.read()
}

fn get_incoming_queue() -> MutexGuard<'static, VecDeque<IncomingRequest>> {
    INCOMING_QUEUE.call_once(|| {
        Mutex::new(VecDeque::new())
    }).lock()
}

pub fn send_async_request(
    driver_id: TaskID,
    io_callback: AsyncIOCallback,
    action: DriverIOAction,
    shared_range: Option<SharedMemoryRange>,
) {
    let request = IncomingRequest {
        driver_id,
        source_task: io_callback.0,
        source_io: io_callback.1,
        source_op: io_callback.2,
        action,
        shared_range,
    };

    get_incoming_queue().push_back(request);

    // make sure the driverio task is awake
    let id = get_driver_io_task_id();
    send_message(id, Message::empty(), 0xffffffff);
}

pub fn driver_io_task() -> ! {
    let id = get_current_id();
    *DRIVER_IO_TASK_ID.write() = id;

    let notify = create_notify_queue();

    let message_handle = open_message_queue();
    add_handle_to_notify_queue(notify, message_handle);

    let mut message = Message::empty();

    let mut next_request_id: u32 = 1;

    loop {
        let op = handle_op_read_struct(message_handle, &mut message);
        while !op.is_complete() {
            wait_on_notify(notify, None);
        }
        let sender = op.wait_for_completion();

        if message.message_type == DRIVER_RESPONSE_MAGIC {
            let response_to = message.unique_id;
            let return_value = if message.args[0] & 0x80000000 == 0 {
                Ok(message.args[0] & 0x7fffffff)
            } else {
                let error = IOError::try_from(message.args[0] & 0x7fffffff).unwrap();
                Err(error)
            };

            match PENDING_REQUESTS.lock().remove(&response_to) {
                Some(request) => {
                    if Into::<u32>::into(request.driver_id) != sender {
                        // TODO: put it back in the map
                        panic!("Got a response from the wrong driver");
                    }

                    if let Some(task_lock) = get_task(request.source_task) {
                        let mut task = task_lock.write();
                        task.async_io_complete(request.source_io, request.source_op, return_value);
                    }
                },
                None => (),
            }
        }

        // Check for incoming requests
        {
            let mut incoming_queue = get_incoming_queue();
            // Once the task is awake, read all incoming requests.

            loop {
                let head = incoming_queue.pop_front();
                match head {
                    Some(request) => {
                        let driver_id = request.driver_id;
                        let request_id = next_request_id;
                        next_request_id += 1;

                        let message = request.action.encode_to_message(request_id);

                        // TODO: I don't like locking this when also holding the queue lock
                        PENDING_REQUESTS.lock().insert(request_id, request);
                        send_message(driver_id, message, 0xffffffff);
                    },
                    None => break,
                }
            }
        }
    }
}

