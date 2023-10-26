use core::ops::Deref;

use crate::io::async_io::{IOType, AsyncOp, OPERATION_FLAG_MESSAGE};
use crate::io::handle::Handle;
use crate::io::provider::message::MessageIOProvider;
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
    let code = op.op_code;
    let _ = task.async_io_table.add_op(io_index, op)?;
    if code & OPERATION_FLAG_MESSAGE != 0 {
        // if it's a messaging op, and it was successfully added, make sure
        // all message queue handles are refreshed
        task.handle_incoming_messages();
    }
    Ok(())
}

pub fn open_message_queue() -> Handle {
    let task_lock = get_current_task();
    let mut task = task_lock.write();

    let io = IOType::MessageQueue(MessageIOProvider::new());
    let io_index = task.async_io_table.add_io(io);
    task.open_handles.insert(io_index)
}


#[cfg(test)]
mod tests {
    use crate::io::handle::PendingHandleOp;
    use crate::io::async_io::{OPERATION_FLAG_TASK, TASK_OP_WAIT, OPERATION_FLAG_MESSAGE, MESSAGE_OP_READ};

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

    #[test_case]
    fn message_queue() {
        use crate::task::actions::send_message;
        use crate::task::actions::lifecycle::terminate;
        use crate::task::actions::messaging::Message;

        fn child_task_body() -> ! {
            let msg_handle = super::open_message_queue();
            let message = Message(0, 0, 0, 0);

            let op = PendingHandleOp::new(msg_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, &message as *const Message as u32, 0, 0);
            let _sender = op.wait_for_completion();

            assert_eq!(message.0, 1);
            assert_eq!(message.1, 2);
            assert_eq!(message.2, 3);
            assert_eq!(message.3, 4);
            terminate(1);
        }

        let (handle, child_id) = super::create_kernel_task(child_task_body, Some("CHILD"));
        send_message(child_id, Message(1, 2, 3, 4), 0xffffffff);

        let op = PendingHandleOp::new(handle, OPERATION_FLAG_TASK | TASK_OP_WAIT, 0, 0, 0);
        op.wait_for_completion();
    }

    #[test_case]
    fn multiple_messages() {
        use crate::task::actions::send_message;
        use crate::task::actions::lifecycle::terminate;
        use crate::task::actions::messaging::Message;
        use crate::io::async_io::{OPERATION_FLAG_MESSAGE, OPERATION_FLAG_TASK, MESSAGE_OP_READ, TASK_OP_WAIT};

        fn child_task_body() -> ! {
            let msg_handle = super::open_message_queue();
            let message = Message(0, 0, 0, 0);
            crate::task::actions::sleep(100);
            let op1 = PendingHandleOp::new(msg_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, &message as *const Message as u32, 0, 0);
            let op2 = PendingHandleOp::new(msg_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, &message as *const Message as u32, 0, 0);
            op1.wait_for_completion();
            op2.wait_for_completion();

            assert_eq!(message, Message(5, 5, 5, 5));
            terminate(1);
        }

        let (handle, child_id) = super::create_kernel_task(child_task_body, Some("CHILD"));
        send_message(child_id, Message(1, 1, 1, 1), 0xffffffff);
        send_message(child_id, Message(5, 5, 5, 5), 0xffffffff);

        let op = PendingHandleOp::new(handle, OPERATION_FLAG_TASK | TASK_OP_WAIT, 0, 0, 0);
        op.wait_for_completion();
    }
}
