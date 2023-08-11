//! Experimental actions for the new handle-based IO workflow

use super::super::handle::{Handle, HandleOp};
use super::super::switching::get_current_task;

pub fn create_file_handle() -> Handle {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    task.handles.open_file()
}

pub fn create_task() -> Handle {
    let child = super::lifecycle::create_task();
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    task.handles.create_task(child)
}

pub fn create_kernel_task(task_body: fn() -> !, name: Option<&str>) -> Handle {
    let child = super::lifecycle::create_kernel_task(task_body, name);
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    task.handles.create_task(child)
}

pub fn open_socket() -> Handle {
    panic!("");
}

pub fn add_handle_op(handle: Handle, op: HandleOp) {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    // TODO: use this result
    let _ = task.handles.add_operation(handle, op);
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn wait_for_child() {
        use core::sync::atomic::{AtomicU32, Ordering};

        fn child_task_body() -> ! {
            crate::task::actions::lifecycle::terminate(4);
        }

        let handle = super::create_kernel_task(child_task_body, Some("CHILD"));
        let sem: AtomicU32 = AtomicU32::new(0);

        let op = super::HandleOp::new(0x40000001, &sem as *const AtomicU32 as u32, 0, 0, 0);
        super::add_handle_op(handle, op);

        let result = loop {
            let res = sem.load(Ordering::SeqCst);
            if res != 0 {
                break res;
            }
            crate::task::actions::yield_coop();
        };
        assert_eq!(result, 4);
    }
}

