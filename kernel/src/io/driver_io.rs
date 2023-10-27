use alloc::collections::{VecDeque, BTreeMap};
use spin::{RwLock, Once, Mutex, MutexGuard};

use crate::task::{id::TaskID, switching::{get_current_id, get_task}, actions::{handle::open_message_queue, send_message}, messaging::Message};

use super::{async_io::{OPERATION_FLAG_MESSAGE, MESSAGE_OP_READ}, handle::PendingHandleOp};


struct IncomingRequest {
    pub driver_id: TaskID,
    pub source_task: TaskID,
    pub source_io: usize,
    pub source_op: usize,
}

pub const DRIVER_IO_RESPONSE_MAGIC: u32 = 0x00534552; // "RES\0"

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

pub fn driver_io_task() -> ! {
    let id = get_current_id();
    *DRIVER_IO_TASK_ID.write() = id;

    let message_handle = open_message_queue();
    let mut message = Message(0, 0, 0, 0);
    let message_ptr = &mut message as *mut Message as u32;

    let mut next_request_id: u32 = 1;

    loop {
        let op = PendingHandleOp::new(message_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, message_ptr, 0, 0);
        let sender = op.wait_for_completion();

        if message.0 == DRIVER_IO_RESPONSE_MAGIC {
            let response_to = message.1;
            let return_value = message.2;

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

                        // TODO: I don't like locking this when also holding the queue lock
                        PENDING_REQUESTS.lock().insert(request_id, request);

                        let message = encode_request(request_id);
                        send_message(driver_id, message, 0xffffffff);
                    },
                    None => break,
                }
            }
        }
    }
}

fn encode_request(id: u32) -> Message {
    Message(0, 0, 0, 0)
}
