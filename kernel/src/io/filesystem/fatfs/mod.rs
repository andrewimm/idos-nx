pub mod bpb;
pub mod dir;
pub mod disk;
pub mod driver;
pub mod fs;
pub mod table;

use self::driver::FatDriver;
use super::install_task_fs;
use crate::io::driver::async_driver::AsyncDriver;
use crate::io::handle::Handle;
use crate::task::actions::handle::{create_pipe_handles, open_message_queue, transfer_handle};
use crate::task::actions::io::{
    close_sync, driver_io_complete, read_struct_sync, read_sync, write_sync,
};
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::messaging::Message;

fn run_driver() -> ! {
    let args_reader = Handle::new(0);
    let response_writer = Handle::new(1);

    let mut name_length_buffer: [u8; 1] = [0; 1];
    let _ = read_sync(args_reader, &mut name_length_buffer, 0);
    let name_length = name_length_buffer[0] as usize;

    let mut dev_name_buffer: [u8; 5 + 8] = [0; 5 + 8];
    dev_name_buffer[0..5].copy_from_slice("DEV:\\".as_bytes());
    let dev_name_len =
        5 + read_sync(args_reader, &mut dev_name_buffer[5..(5 + name_length)], 0).unwrap() as usize;
    let _ = close_sync(args_reader);

    let dev_name = unsafe { core::str::from_utf8_unchecked(&dev_name_buffer[..dev_name_len]) };

    crate::kprint!("Mount FAT FS on {}\n", dev_name);

    let messages = open_message_queue();
    let mut incoming_message = Message::empty();

    let mut driver_impl = FatDriver::new(dev_name);

    let _ = write_sync(response_writer, &[1], 0);
    let _ = close_sync(response_writer);

    loop {
        if let Ok(_sender) = read_struct_sync(messages, &mut incoming_message) {
            let request_id = incoming_message.unique_id;
            match driver_impl.handle_request(incoming_message) {
                Some(response) => driver_io_complete(request_id, response),
                None => (),
            }
        }
    }
}

pub fn mount_fat_fs() {
    let pairs = [("A", "FD1"), ("C", "ATA1")];

    for pair in pairs {
        let (args_reader, args_writer) = create_pipe_handles();
        let (response_reader, response_writer) = create_pipe_handles();

        let task_id = create_kernel_task(run_driver, Some("FATFS"));
        transfer_handle(args_reader, task_id);
        transfer_handle(response_writer, task_id);

        let _ = write_sync(args_writer, &[pair.1.len() as u8], 0);
        let _ = write_sync(args_writer, pair.1.as_bytes(), 0);
        let _ = read_sync(response_reader, &mut [0u8], 0);

        install_task_fs(pair.0, task_id);
    }
}
