use core::ops::Deref;

use crate::io::async_io::{IOType, AsyncOp};
use crate::io::handle::Handle;
use crate::io::provider::task::TaskIOProvider;
use crate::task::id::TaskID;

use super::switching::get_current_task;

pub fn create_kernel_task(task_body: fn() -> !, name: Option<&str>) -> (Handle, TaskID) {
    let child = super::lifecycle::create_kernel_task(task_body, name);
    let task_lock = get_current_task();
    let mut task = task_lock.write();

    let io = IOType::ChildTask(TaskIOProvider::for_task(child));
    let io_index = task.async_io_table.add_io(io);
    let handle = task.open_handles.insert(io_index);

    (handle, child)
}

pub fn add_io_op(handle: Handle, op: AsyncOp) -> Result<(), ()> {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    let io_index = task.open_handles.get(handle).ok_or(())?.clone();
    task.async_io_table.add_op(io_index, op)
}


#[cfg(test)]
mod tests {
    use crate::io::handle::PendingHandleOp;
    use crate::io::async_io::{OPERATION_FLAG_TASK, TASK_OP_WAIT};

    #[test_case]
    fn wait_for_child() {
        fn child_task_body() -> ! {
            crate::task::actions::lifecycle::terminate(4);
        }

        let (handle, _) = super::create_kernel_task(child_task_body, Some("CHILD"));
        let op = PendingHandleOp::new(handle, OPERATION_FLAG_TASK | TASK_OP_WAIT, 0, 0, 0);
        let result = op.wait_for_completion();
        assert_eq!(result, 4);
    }

    #[test_case]
    fn child_early_exit() {
        fn child_task_body() -> ! {
            crate::task::actions::lifecycle::terminate(4);
        }
        let (handle, _) = super::create_kernel_task(child_task_body, Some("CHILD"));
        crate::task::actions::sleep(500);
        let op = PendingHandleOp::new(handle, 0x40000001, 0, 0, 0);
        let result = op.wait_for_completion();
        assert_eq!(result, 4);
    }

    #[test_case]
    fn child_multiple_ops() {
        fn child_task_body() -> ! {
            crate::task::actions::lifecycle::terminate(3);
        }

        let (handle, _) = super::create_kernel_task(child_task_body, Some("CHILD"));
        let op1 = PendingHandleOp::new(handle, OPERATION_FLAG_TASK | TASK_OP_WAIT, 0, 0, 0);
        let op2 = PendingHandleOp::new(handle, OPERATION_FLAG_TASK | TASK_OP_WAIT, 0, 0, 0);

        let mut result = op1.wait_for_completion();
        assert_eq!(result, 3);
        assert!(op2.is_complete());
        result = op2.wait_for_completion();
        assert_eq!(result, 3);
    }
}
