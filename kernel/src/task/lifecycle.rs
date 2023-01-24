use super::id::TaskID;
use super::yield_coop;

pub fn terminate_id(id: TaskID, exit_code: u32) {
    let parent_id = {
        let terminated_task = super::switching::get_task(id);
        match terminated_task {
            Some(task_lock) => {
                let mut task = task_lock.write();
                task.terminate();
                task.parent_id
            },
            None => return,
        }
    };

    let parent_task = super::switching::get_task(parent_id);
    if let Some(parent_lock) = parent_task {
        parent_lock.write().child_terminated(id, exit_code);
    }
}

pub fn terminate(exit_code: u32) {
    let cur_id = super::switching::get_current_id();
    terminate_id(cur_id, exit_code);
    yield_coop();
}

pub fn wait_for_child(id: TaskID, timeout: Option<u32>) -> u32 {
    let current_lock = super::switching::get_current_task();
    current_lock.write().wait_for_child(id, timeout);
    yield_coop();
    let code = current_lock.write().resume_from_wait();
    code
}

pub fn exception() {
    let cur_id = super::switching::get_current_id();
    crate::kprint!("EXCEPTION! {:?}\n", cur_id);
    // TODO: implement exception handling
    
    terminate_id(cur_id, 255);
    yield_coop();
}
