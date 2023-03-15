use spin::RwLock;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::id::TaskID;
use crate::task::messaging::Message;
use crate::task::switching::get_current_id;

#[derive(Debug)]
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
    // Add the request to the queue
    
    // Make sure the arbiter is awake
    let id = get_arbiter_task_id();
    send_message(id, Message::empty(), 0xffffffff);
}

static ARBITER_TASK_ID: RwLock<TaskID> = RwLock::new(TaskID::new(0));

pub fn get_arbiter_task_id() -> TaskID {
    *ARBITER_TASK_ID.read()
}

/// The core loop of the Arbiter task. The Arbiter exists as an independent
/// kernel-level task so that other 
pub fn arbiter_task() -> ! {
    let id = get_current_id();
    *ARBITER_TASK_ID.write() = id;

    loop {
        let next_message = read_message_blocking(None);
        
        crate::kprint!("= Arbiter woke up\n");
    }
}
