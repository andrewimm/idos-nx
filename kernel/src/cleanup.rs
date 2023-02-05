use alloc::vec::Vec;
use crate::task::id::TaskID;
use spin::RwLock;

static CLEANUP_ID: RwLock<TaskID> = RwLock::new(TaskID::new(0));

/// When awakened by an incoming message, crawl over the set of all tasks and
/// release any resources held by terminated tasks before deleting their state
/// entirely.
pub fn cleanup_task() -> ! {
    let own_id = crate::task::switching::get_current_id();

    *(CLEANUP_ID.write()) = own_id;

    crate::kprint!("Cleanup Task ready\n");

    let mut terminated: Vec<TaskID> = Vec::new();

    loop {
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

        crate::task::actions::read_message_blocking(None);
    }
}

pub fn get_cleanup_task_id() -> TaskID {
    *(CLEANUP_ID.read())
}

