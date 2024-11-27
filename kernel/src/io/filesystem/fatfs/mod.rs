pub mod bpb;
pub mod dir;
pub mod disk;
pub mod driver;
pub mod fs;
pub mod table;

use idos_api::io::error::IOError;

use self::driver::FatDriver;
use super::install_task_fs;
use crate::io::driver::async_driver::AsyncDriver;
use crate::io::driver::comms::{DriverCommand, IOResult, DRIVER_RESPONSE_MAGIC};
use crate::io::handle::Handle;
use crate::task::actions::handle::{
    add_handle_to_notify_queue, create_notify_queue, create_pipe_handles, handle_op_close,
    handle_op_read, handle_op_read_struct, handle_op_write, open_message_queue, transfer_handle,
    wait_on_notify,
};
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::send_message;
use crate::task::id::TaskID;
use crate::task::messaging::Message;

fn run_driver() -> ! {
    let args_reader = Handle::new(0);
    let response_writer = Handle::new(1);

    let mut name_length_buffer: [u8; 1] = [0; 1];
    handle_op_read(args_reader, &mut name_length_buffer, 0).wait_for_completion();
    let name_length = name_length_buffer[0] as usize;

    let mut dev_name_buffer: [u8; 5 + 8] = [0; 5 + 8];
    &mut dev_name_buffer[0..5].copy_from_slice("DEV:\\".as_bytes());
    let dev_name_len =
        5 + handle_op_read(args_reader, &mut dev_name_buffer[5..(5 + name_length)], 0)
            .wait_for_completion() as usize;
    handle_op_close(args_reader).wait_for_completion();

    let dev_name = unsafe { core::str::from_utf8_unchecked(&dev_name_buffer[..dev_name_len]) };

    crate::kprint!("Mount FAT FS on {}\n", dev_name);

    let messages = open_message_queue();
    let mut incoming_message = Message::empty();
    let notify = create_notify_queue();
    add_handle_to_notify_queue(notify, messages);

    let mut message_read = handle_op_read_struct(messages, &mut incoming_message);

    let mut driver_impl = FatDriver::new(dev_name);

    handle_op_write(response_writer, &[1]).wait_for_completion();
    handle_op_close(response_writer).wait_for_completion();

    loop {
        if let Some(sender) = message_read.get_result() {
            match driver_impl.handle_request(incoming_message) {
                Some(response) => send_message(TaskID::new(sender), response, 0xffffffff),
                None => (),
            }

            message_read = handle_op_read_struct(messages, &mut incoming_message);
        } else {
            wait_on_notify(notify, None);
        }
    }
}

fn send_response(task: TaskID, request_id: u32, result: IOResult) {
    let message = match result {
        Ok(result) => {
            let code = result & 0x7fffffff;
            Message {
                message_type: DRIVER_RESPONSE_MAGIC,
                unique_id: request_id,
                args: [code, 0, 0, 0, 0, 0],
            }
        }
        Err(err) => {
            let code = Into::<u32>::into(err) | 0x80000000;
            Message {
                message_type: DRIVER_RESPONSE_MAGIC,
                unique_id: request_id,
                args: [code, 0, 0, 0, 0, 0],
            }
        }
    };
    send_message(task, message, 0xffffffff)
}

pub fn mount_fat_fs() {
    let pairs = [("A", "FD1"), ("C", "ATA1")];

    for pair in pairs {
        let (args_reader, args_writer) = create_pipe_handles();
        let (response_reader, response_writer) = create_pipe_handles();

        let task_id = create_kernel_task(run_driver, Some("FATFS"));
        transfer_handle(args_reader, task_id);
        transfer_handle(response_writer, task_id);

        handle_op_write(args_writer, &[pair.1.len() as u8]).wait_for_completion();
        handle_op_write(args_writer, pair.1.as_bytes()).wait_for_completion();
        handle_op_read(response_reader, &mut [0u8], 0).wait_for_completion();

        install_task_fs(pair.0, task_id);
    }
}
