use crate::task::{
    actions::{handle::open_message_queue, io::read_struct_sync, send_message},
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

    let messages = open_message_queue();

    crate::kprint!("Cleanup Task ready\n");

    let mut incoming_message = Message::empty();
    let mut terminated: Vec<TaskID> = Vec::new();

    loop {
        let _ = read_struct_sync(messages, &mut incoming_message, 0);

        crate::task::map::for_each_task_mutfn(|t| {
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
    }
}

pub fn wake_cleanup_resident() {
    let id = *(CLEANUP_ID.read());
    send_message(id, Message::empty(), 0xffffffff);
}
