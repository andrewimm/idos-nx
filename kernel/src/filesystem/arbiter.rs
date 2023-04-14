use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::{RwLock, Mutex, Once, MutexGuard};
use crate::task::actions::lifecycle::wait_for_io;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::id::TaskID;
use crate::task::messaging::Message;
use crate::task::switching::{get_current_id, get_task};

#[derive(Copy, Clone, Debug)]
pub enum AsyncIO {
    Open,
    Read,
    Write,
    Close,
}

pub type AsyncResponse = Arc<Mutex<Option<u32>>>;

/// Enqueue a new IO request. Assuming the target is a valid FS driver, the
/// current task will be IO-blocked until the request completes. The Arbiter
/// will consume the AsyncIO request and send an appropriate message to the FS
/// driver.
pub fn begin_io(driver_id: TaskID, io: AsyncIO, response: AsyncResponse) {
    let current_id = get_current_id();
    // Add the request to the queue
    get_arbiter_queue().push_back(
        IncomingRequest {
            driver_id,
            requestor_id: current_id,
            io,
            response,
        }
    );
    
    // Make sure the arbiter is awake
    let id = get_arbiter_task_id();
    send_message(id, Message::empty(), 0xffffffff);
    wait_for_io(None);
}

struct IncomingRequest {
    pub driver_id: TaskID,
    pub requestor_id: TaskID,
    pub io: AsyncIO,
    pub response: AsyncResponse,
}

static ARBITER_TASK_ID: RwLock<TaskID> = RwLock::new(TaskID::new(0));
static ARBITER_QUEUE: Once<Mutex<VecDeque<IncomingRequest>>> = Once::new();

pub fn get_arbiter_task_id() -> TaskID {
    *ARBITER_TASK_ID.read()
}

fn get_arbiter_queue() -> MutexGuard<'static, VecDeque<IncomingRequest>> {
    ARBITER_QUEUE.call_once(|| {
        Mutex::new(VecDeque::new())
    }).lock()
}

/// The core loop of the Arbiter task. The Arbiter exists as an independent
/// kernel-level task so that other 
pub fn arbiter_task() -> ! {
    let id = get_current_id();
    *ARBITER_TASK_ID.write() = id;

    loop {
        // reading this is only necessary when we start handling responses from
        // drivers
        let (_next_message, _) = read_message_blocking(None);

        crate::kprint!("= Arbiter woke up\n");

        {
            let mut queue = get_arbiter_queue();
            // Once the task is awake, read all the incoming requests.
            // The Arbiter will use the task ID to determine the destination of
            // this request. If no request is currently outstanding, 
            //
            // Notice that this logic is specific to file IO operations, but 
            // has no concept of higher level file systems. That allows the
            // same Arbiter task to be used for device drivers in the DEV: FS.
            loop {
                let head = queue.pop_front();
                match head {
                    Some(IncomingRequest { driver_id, requestor_id, io, response }) => {
                        crate::kprint!("  IO Req: {:?}, to {:?}\n", io, driver_id);

                        match io {
                            AsyncIO::Open => {
                                response.lock().replace(1);
                            },
                            AsyncIO::Read => {
                                response.lock().replace(3);
                            },
                            _ => (),
                        }

                        crate::kprint!("  IO complete, resume {:?}\n", requestor_id);
                        if let Some(task_lock) = get_task(requestor_id) {
                            let mut task = task_lock.write();
                            task.io_complete();
                        }
                    },
                    None => break,
                }
            }
        }
    }
}
