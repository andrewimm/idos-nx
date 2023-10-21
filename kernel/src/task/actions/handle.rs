//! Experimental actions for the new handle-based IO workflow

use crate::task::id::TaskID;

use crate::task::handle::{Handle, AsyncOp, MESSAGE_OP_READ, HandleType};
use crate::task::messaging::Message;
use crate::task::switching::get_current_task;

pub fn create_file_handle() -> Handle {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    task.handles.open_file()
}

pub fn create_task() -> (Handle, TaskID) {
    let child = super::lifecycle::create_task();
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    (task.handles.create_task(child), child)
}

pub fn create_kernel_task(task_body: fn() -> !, name: Option<&str>) -> (Handle, TaskID) {
    let child = super::lifecycle::create_kernel_task(task_body, name);
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    (task.handles.create_task(child), child)
}

pub fn open_socket() -> Handle {
    panic!("");
}

pub fn open_message_queue() -> Handle {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    task.handles.message_queue()
}

pub fn add_handle_op(handle: Handle, op: AsyncOp) {
    let task_lock = get_current_task();
    task_lock.write().add_handle_op(handle, op);

    /*
    let ops = match task_lock.write().handles.add_operation(handle, op) {
        Ok(len) => len,
        Err(_) => return,
    };
    if ops > 1 {
        return;
    }
    // run the operation
    let handle_type = match task_lock.read().handles.get_handle(handle) {
        Some(open_handle) => open_handle.handle_type.clone(),
        None => return,
    };
    match handle_type {
        HandleType::MessageQueue => run_message_op(op),
        _ => (),
    }
    */
}

/*
pub fn run_message_op(op: HandleOp) {
    let task_lock = get_current_task();
    let current_ticks = crate::time::system::get_system_ticks();

    match op.op_code & 0xffff {
        MESSAGE_OP_READ => {
            let msg_pointer = op.arg0 as *mut Message;
            let mut task = task_lock.write();
            if let (Some(packet), _) = task.message_queue.read(current_ticks) {
                let (sender, msg) = packet.open();
                unsafe { *msg_pointer = msg };
                op.complete(1);
            }
        },
        _ => return,
    }
}
*/

#[cfg(test)]
mod tests {
    use crate::io::handle::PendingHandleOp;

    #[test_case]
    fn wait_for_child() {
        fn child_task_body() -> ! {
            crate::task::actions::lifecycle::terminate(4);
        }

        let (handle, _) = super::create_kernel_task(child_task_body, Some("CHILD"));
        let op = PendingHandleOp::new(handle, 0x40000001, 0, 0, 0);
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

    //#[test_case]
    fn message_queue() {
        use crate::task::actions::send_message;
        use crate::task::actions::lifecycle::terminate;
        use crate::task::actions::messaging::Message;
        use crate::task::handle::{AsyncOp, OPERATION_FLAG_MESSAGE, MESSAGE_OP_READ};

        fn child_task_body() -> ! {
            let msg_handle = super::open_message_queue();
            let message = Message(0, 0, 0, 0);
            let sender: u32 = 0;

            let op = PendingHandleOp::new(msg_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, &message as *const Message as u32, &sender as *const u32 as u32, 0);
            op.wait_for_completion();

            assert_eq!(message.0, 1);
            assert_eq!(message.1, 2);
            assert_eq!(message.2, 3);
            assert_eq!(message.3, 4);
            terminate(1);
        }

        let (handle, child_id) = super::create_kernel_task(child_task_body, Some("CHILD"));
        super::super::send_message(child_id, Message(1, 2, 3, 4), 0xffffffff);

        let op = PendingHandleOp::new(handle, 0x40000001, 0, 0, 0);
        op.wait_for_completion();
    }

    //#[test_case]
    fn multiple_messages() {
        use crate::task::actions::send_message;
        use crate::task::actions::lifecycle::terminate;
        use crate::task::actions::messaging::Message;
        use crate::task::handle::{AsyncOp, OPERATION_FLAG_MESSAGE, MESSAGE_OP_READ};

        fn child_task_body() -> ! {
            let msg_handle = super::open_message_queue();
            let message = Message(0, 0, 0, 0);
            let sender: u32 = 0;
            crate::task::actions::sleep(100);
            let op_1 = PendingHandleOp::new(msg_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, &message as *const Message as u32, &sender as *const u32 as u32, 0);
            let op_2 = PendingHandleOp::new(msg_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, &message as *const Message as u32, &sender as *const u32 as u32, 0);
            op_1.wait_for_completion();
            op_2.wait_for_completion();

            assert_eq!(message, Message(5, 5, 5, 5));
            terminate(1);
        }

        let (handle, child_id) = super::create_kernel_task(child_task_body, Some("CHILD"));
        super::super::send_message(child_id, Message(1, 1, 1, 1), 0xffffffff);
        super::super::send_message(child_id, Message(5, 5, 5, 5), 0xffffffff);

        let op = PendingHandleOp::new(handle, 0x40000001, 0, 0, 0);
        op.wait_for_completion();
    }
}

