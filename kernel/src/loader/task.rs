//! The Loader Task is a resident that receives requests to attach executable
//! programs to new tasks.
//! Other tasks can send IPC messages in a specific format, telling the loader
//! which task to modify and which program to load. Most of the details of this
//! will be handled by the stdlib.

use spin::Once;

use crate::task::actions::handle::{
    add_handle_to_notify_queue, create_kernel_task, create_notify_queue, handle_op_read_struct,
    open_message_queue, wait_on_notify,
};
use crate::task::id::TaskID;
use crate::task::messaging::Message;

fn loader_resident() -> ! {
    let messages = open_message_queue();
    let mut incoming_message = Message::empty();
    let notify = create_notify_queue();
    add_handle_to_notify_queue(notify, messages);

    let mut message_read = handle_op_read_struct(messages, &mut incoming_message);

    crate::kprintln!("Loader task ready to receive");
    loop {
        if let Some(_sender) = message_read.get_result() {
            crate::kprintln!("LOADER REQUEST");

            message_read = handle_op_read_struct(messages, &mut incoming_message);
        } else {
            wait_on_notify(notify, None);
        }
    }
}

pub static LOADER_ID: Once<TaskID> = Once::new();

pub fn get_loader_id() -> TaskID {
    LOADER_ID
        .call_once(|| {
            let (_, task_id) = create_kernel_task(loader_resident, Some("LOADER"));
            // TODO: Register the task, or better yet execute it from within the registry

            task_id
        })
        .clone()
}
