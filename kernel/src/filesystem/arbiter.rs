use alloc::collections::VecDeque;
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

/// Enqueue a new IO request. Assuming the target is a valid FS driver, the
/// current task will be IO-blocked until the request completes. The Arbiter
/// will consume the AsyncIO request and send an appropriate message to the FS
/// driver.
pub fn begin_io(io: AsyncIO) {
    let current_id = get_current_id();
    // Add the request to the queue
    get_arbiter_queue().push_back((current_id, io));
    
    // Make sure the arbiter is awake
    let id = get_arbiter_task_id();
    send_message(id, Message::empty(), 0xffffffff);
    wait_for_io(None);
}

static ARBITER_TASK_ID: RwLock<TaskID> = RwLock::new(TaskID::new(0));
static ARBITER_QUEUE: Once<Mutex<VecDeque<(TaskID, AsyncIO)>>> = Once::new();

pub fn get_arbiter_task_id() -> TaskID {
    *ARBITER_TASK_ID.read()
}

pub fn get_arbiter_queue() -> MutexGuard<'static, VecDeque<(TaskID, AsyncIO)>> {
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

            loop {
                let head = queue.pop_front();
                match head {
                    Some((from, req)) => {
                        crate::kprint!("  IO Req: {:?}\n", req);

                        crate::kprint!("  IO complete, resume {:?}\n", from);
                        if let Some(task_lock) = get_task(from) {
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
