use core::ops::Deref;

use crate::interrupts::pic::add_interrupt_listener;
use crate::io::async_io::{IOType, AsyncOp, OPERATION_FLAG_MESSAGE};
use crate::io::handle::{Handle, PendingHandleOp};
use crate::io::notify::NotifyQueue;
use crate::io::provider::file::FileIOProvider;
use crate::io::provider::irq::InterruptIOProvider;
use crate::io::provider::message::MessageIOProvider;
use crate::io::provider::task::TaskIOProvider;
use crate::pipes::driver::{create_pipe, get_pipe_drive_id};
use crate::task::id::TaskID;

use super::switching::get_current_task;
use super::yield_coop;

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
    let code = op.op_code;
    let (io_index, io_type) = {
        let mut task = task_lock.read();
        let io_index = task.open_handles.get(handle).ok_or(())?.clone();
        let io = task.async_io_table.get(io_index).ok_or(())?;
        (io_index, io.io_type.clone())
    };
    io_type.lock().add_op(io_index, op)?;
    if code & OPERATION_FLAG_MESSAGE != 0 {
        // if it's a messaging op, and it was successfully added, make sure
        // all message queue handles are refreshed
        task_lock.write().handle_incoming_messages();
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

pub fn create_file_handle() -> Handle {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    let io = IOType::File(FileIOProvider::new(task.id));
    let io_index = task.async_io_table.add_io(io);
    task.open_handles.insert(io_index)
}

pub fn create_pipe_handles() -> (Handle, Handle) {
    let (reader_instance, writer_instance) = create_pipe();
    let pipe_driver_id = get_pipe_drive_id();
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    let read_io = FileIOProvider::bound(task.id, pipe_driver_id, reader_instance);
    let read_io_index = task.async_io_table.add_io(IOType::File(read_io));
    let write_io = FileIOProvider::bound(task.id, pipe_driver_id, writer_instance);
    let write_io_index = task.async_io_table.add_io(IOType::File(write_io));
    
    (
        task.open_handles.insert(read_io_index),
        task.open_handles.insert(write_io_index),
    )
}

pub fn open_interrupt_handle(irq: u8) -> Handle {
    let task_lock = get_current_task();
    let (task_id, handle, io_index) = {
        let mut task = task_lock.write();
        let io = IOType::Interrupt(InterruptIOProvider::new(irq));
        let io_index = task.async_io_table.add_io(io);
        let handle = task.open_handles.insert(io_index);

        (task.id, handle, io_index)
    };
    add_interrupt_listener(irq, task_id, io_index);
    handle
}

pub fn create_notify_queue() -> Handle {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    let queue = NotifyQueue::new();
    task.notify_queues.insert(queue)
}

pub fn add_handle_to_notify_queue(queue: Handle, handle: Handle) {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    let io_index = match task.open_handles.get(handle) {
        Some(index) => *index,
        None => return,
    };

    match task.notify_queues.get_mut(queue) {
        Some(q) => q.add_listener(io_index),
        None => (),
    }
}

pub fn wait_on_notify(queue: Handle, timeout: Option<u32>) {
    let task_lock = get_current_task();
    task_lock.write().wait_on_notify_queue(queue, timeout);
    yield_coop();
}

pub fn open_file_op(handle: Handle, path: &str) -> PendingHandleOp {
    use crate::io::async_io::{OPERATION_FLAG_FILE, FILE_OP_OPEN};

    let path_ptr = path.as_ptr() as u32;
    let path_len = path.len() as u32;
    PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0)
}

pub fn read_file_op(handle: Handle, buffer: &mut [u8]) -> PendingHandleOp {
    use crate::io::async_io::{OPERATION_FLAG_FILE, FILE_OP_READ};

    let buffer_ptr = buffer.as_ptr() as u32;
    let buffer_len = buffer.len() as u32;
    PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_READ, buffer_ptr, buffer_len, 0)
}

pub fn write_file_op(handle: Handle, buffer: &[u8]) -> PendingHandleOp {
    use crate::io::async_io::{OPERATION_FLAG_FILE, FILE_OP_WRITE};

    let buffer_ptr = buffer.as_ptr() as u32;
    let buffer_len = buffer.len() as u32;
    PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_WRITE, buffer_ptr, buffer_len, 0)
}

#[cfg(test)]
mod tests {
    use crate::io::handle::PendingHandleOp;
    use crate::io::async_io::{OPERATION_FLAG_TASK, TASK_OP_WAIT, OPERATION_FLAG_MESSAGE, MESSAGE_OP_READ, OPERATION_FLAG_FILE, FILE_OP_OPEN, FILE_OP_READ};

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
        let op = PendingHandleOp::new(handle, OPERATION_FLAG_TASK | TASK_OP_WAIT, 0, 0, 0);
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

    #[test_case]
    fn open_file_sync() {
        {
            let handle = super::create_file_handle();
            let path: &str = "TEST:\\MYFILE.TXT";
            let path_ptr = path.as_ptr() as u32;
            let path_len = path.len() as u32;
            let op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
            let result = op.wait_for_completion();
            assert_eq!(result, 1);
        }

        {
            let handle = super::create_file_handle();
            let path = "TEST:\\NOTREAL.TXT";
            let path_ptr = path.as_ptr() as u32;
            let path_len = path.len() as u32;
            let op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
            let result = op.wait_for_completion();
            // Error: Not Found
            assert_eq!(result, 0x80000002);
        }
    }

    #[test_case]
    fn open_file_async() {
        {
            let handle = super::create_file_handle();
            let path: &str = "ATEST:\\MYFILE.TXT";
            let path_ptr = path.as_ptr() as u32;
            let path_len = path.len() as u32;
            let op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
            let result = op.wait_for_completion();
            assert_eq!(result, 1);
        }

        {
            let handle = super::create_file_handle();
            let path = "ATEST:\\NOTREAL.TXT";
            let path_ptr = path.as_ptr() as u32;
            let path_len = path.len() as u32;
            let op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
            let result = op.wait_for_completion();
            // Error: Not Found
            assert_eq!(result, 0x80000002);
        }
    }

    #[test_case]
    fn read_file_sync() {
        let handle = super::create_file_handle();
        let path = "TEST:\\MYFILE.TXT";
        let path_ptr = path.as_ptr() as u32;
        let path_len = path.len() as u32;
        let mut op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
        let mut result = op.wait_for_completion();
        assert_eq!(result, 1);

        let mut buffer: [u8; 5] = [0; 5];
        op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_READ, buffer.as_ptr() as u32, buffer.len() as u32, 0);
        result = op.wait_for_completion();
        assert_eq!(result, 5);
        assert_eq!(buffer, [b'A', b'B', b'C', b'D', b'E']);
    }

    #[test_case]
    fn read_file_async() {
        let handle = super::create_file_handle();
        let path = "ATEST:\\MYFILE.TXT";
        let path_ptr = path.as_ptr() as u32;
        let path_len = path.len() as u32;
        let mut op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
        let mut result = op.wait_for_completion();
        assert_eq!(result, 1);

        let mut buffer: [u8; 5] = [0; 5];
        op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_READ, buffer.as_ptr() as u32, buffer.len() as u32, 0);
        result = op.wait_for_completion();
        assert_eq!(result, 5);
        assert_eq!(buffer, [b'A', b'B', b'C', b'D', b'E']);

        op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_READ, buffer.as_ptr() as u32, 3, 0);
        result = op.wait_for_completion();
        assert_eq!(result, 3);
        assert_eq!(buffer[..3], [b'F', b'G', b'H']);
    }

    #[test_case]
    fn open_device_sync() {
        {
            let handle = super::create_file_handle();
            let path: &str = "DEV:\\ZERO";
            let path_ptr = path.as_ptr() as u32;
            let path_len = path.len() as u32;
            let op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
            let result = op.wait_for_completion();
            assert_eq!(result, 1);
        }

        {
            let handle = super::create_file_handle();
            let path = "DEV:\\FAKE";
            let path_ptr = path.as_ptr() as u32;
            let path_len = path.len() as u32;
            let op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
            let result = op.wait_for_completion();
            // Error: Not Found
            assert_eq!(result, 0x80000002);
        }
    }

    #[test_case]
    fn read_device_sync() {
        let handle = super::create_file_handle();
        let path = "DEV:\\ZERO";
        let path_ptr = path.as_ptr() as u32;
        let path_len = path.len() as u32;
        let mut op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
        let mut result = op.wait_for_completion();
        assert_eq!(result, 1);

        let mut buffer: [u8; 3] = [0xAA; 3];
        op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_READ, buffer.as_ptr() as u32, buffer.len() as u32, 0);
        result = op.wait_for_completion();
        assert_eq!(result, 3);
        assert_eq!(buffer, [0, 0, 0]);
    }

    #[test_case]
    fn read_device_async() {
        let handle = super::create_file_handle();
        let path = "DEV:\\ASYNCDEV";
        let path_ptr = path.as_ptr() as u32;
        let path_len = path.len() as u32;
        let mut op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
        let mut result = op.wait_for_completion();
        assert_eq!(result, 1);

        let mut buffer: [u8; 4] = [0xBB; 4];
        op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_READ, buffer.as_ptr() as u32, 2, 0);
        result = op.wait_for_completion();
        assert_eq!(result, 2);
        assert_eq!(buffer, [b't', b'e', 0xbb, 0xbb]);

        op = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_READ, buffer.as_ptr() as u32, 4, 0);
        result = op.wait_for_completion();
        assert_eq!(result, 4);
        assert_eq!(buffer, [b's', b't', b't', b'e']);
    }

    #[test_case]
    fn queueing_ops() {
        let handle = super::create_file_handle();
        let path = "ATEST:\\MYFILE.TXT";
        let path_ptr = path.as_ptr() as u32;
        let path_len = path.len() as u32;
        let mut buffer: [u8; 4] = [0; 4];
        let op1 = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
        let op2 = PendingHandleOp::new(handle, OPERATION_FLAG_FILE | FILE_OP_READ, buffer.as_ptr() as u32, buffer.len() as u32, 0);

        let result = op2.wait_for_completion();
        assert_eq!(result, 4);
        assert_eq!(buffer, [b'A', b'B', b'C', b'D']);
    }

    #[test_case]
    fn notify_queue() {
        let queue = super::create_notify_queue();
        let file = super::create_file_handle();
        super::add_handle_to_notify_queue(queue, file);
        let path = "ATEST:\\MYFILE.TXT";
        let path_ptr = path.as_ptr() as u32;
        let path_len = path.len() as u32;
        let op = PendingHandleOp::new(file, OPERATION_FLAG_FILE | FILE_OP_OPEN, path_ptr, path_len, 0);
        super::wait_on_notify(queue, None);
        assert!(op.is_complete());
        assert_eq!(op.get_result(), Some(1));
    }
}
