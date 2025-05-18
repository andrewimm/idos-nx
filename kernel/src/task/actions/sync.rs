use alloc::sync::Arc;

use crate::{io::handle::Handle, sync::wake_set::WakeSet, task::switching::get_current_task};

pub fn create_wake_set() -> Handle {
    let wake_set = WakeSet::new();
    let current_task_lock = get_current_task();
    let mut current_task_guard = current_task_lock.write();
    current_task_guard.wake_sets.insert(Arc::new(wake_set))
}

pub fn block_on_wake_set(set_id: Handle, timeout: Option<u32>) {
    let wake_set_found = {
        let task_lock = get_current_task();
        let task_guard = task_lock.read();
        match task_guard.wake_sets.get(set_id) {
            Some(set) => set.clone(),
            None => return,
        }
    };
    wake_set_found.wait(timeout);
}
