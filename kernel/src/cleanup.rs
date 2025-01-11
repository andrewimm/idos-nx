use crate::task::{
    actions::{
        handle::{
            add_handle_to_notify_queue, create_notify_queue, handle_op_read_struct,
            open_message_queue, wait_on_notify,
        },
        send_message,
    },
    id::TaskID,
    messaging::Message,
};
use alloc::vec::Vec;
use spin::RwLock;

static CLEANUP_ID: RwLock<TaskID> = RwLock::new(TaskID::new(0));

/// The cleanup task is a resident that is created by the kernel and runs in the
/// background. It is responsible for cleaning up the resources of terminated
/// tasks. This is because a task cannot clean itself it -- cleanup must be done
/// in a different address space. The cleanup resident is a convenient place to
/// do this.
/// It blocks on reading a message queue; whenever a task is terminated, the
/// kernel wakes the cleanup task by sending it a message. The cleanup task
/// loops over all tasks, looking for terminated tasks and reclaiming their
/// resources.
pub fn cleanup_resident() -> ! {
    let own_id = crate::task::switching::get_current_id();

    *(CLEANUP_ID.write()) = own_id;

    let notify = create_notify_queue();
    let messages = open_message_queue();
    add_handle_to_notify_queue(notify, messages);

    crate::kprint!("Cleanup Task ready\n");

    let mut incoming_message = Message::empty();
    let mut terminated: Vec<TaskID> = Vec::new();

    loop {
        let _ = handle_op_read_struct(messages, &mut incoming_message).wait_for_completion();

        crate::task::switching::for_each_task_mut(|t| {
            let task = t.read();
            if task.is_terminated() {
                terminated.push(task.id);
            }
        });

        while !terminated.is_empty() {
            if let Some(id) = terminated.pop() {
                crate::task::switching::clean_up_task(id);
            }
        }

        wait_on_notify(notify, None);
    }
}

pub fn wake_cleanup_resident() {
    let id = *(CLEANUP_ID.read());
    send_message(id, Message::empty(), 0xffffffff);
}
