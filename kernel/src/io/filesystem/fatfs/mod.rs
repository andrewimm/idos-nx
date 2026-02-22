pub mod bpb;
pub mod dir;
pub mod disk;
pub mod driver;
pub mod fs;
pub mod table;

use self::driver::FatDriver;
use super::install_task_fs;
use crate::io::handle::Handle;
use crate::log::TaggedLogger;
use crate::task::actions::handle::{create_pipe_handles, open_message_queue, transfer_handle};
use crate::task::actions::io::{
    close_sync, driver_io_complete, read_struct_sync, read_sync, write_sync,
};
use crate::task::actions::lifecycle::create_kernel_task;
use idos_api::io::driver::AsyncDriver;
use idos_api::ipc::Message;

const LOGGER: TaggedLogger = TaggedLogger::new("FATFS", 34);

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

    LOGGER.log(format_args!("Mount FAT FS on {}", dev_name));

    let messages = open_message_queue();
    let mut incoming_message = Message::empty();

    let mut driver_impl = FatDriver::new(dev_name);

    let _ = write_sync(response_writer, &[1], 0);
    let _ = close_sync(response_writer);

    loop {
        if let Ok(_sender) = read_struct_sync(messages, &mut incoming_message, 0) {
            let request_id = incoming_message.unique_id;
            match driver_impl.handle_request(incoming_message) {
                Some(response) => driver_io_complete(request_id, response),
                None => (),
            }
        }
    }
}

/// Try to mount a FAT filesystem using the userspace driver loaded by the
/// bootloader as a flat binary. Returns true on success, false if the binary
/// wasn't loaded or the task couldn't be started.
fn try_mount_userspace(drive_letter: &str, dev_name: &str) -> bool {
    use crate::exec::{exec_flat_binary, get_fatdrv_boot_info};
    use crate::task::actions::handle::create_task;

    let (phys_addr, file_size) = get_fatdrv_boot_info();
    if phys_addr == 0 || file_size == 0 {
        LOGGER.log(format_args!("No FATDRV binary loaded by bootloader"));
        return false;
    }

    let (args_reader, args_writer) = create_pipe_handles();
    let (response_reader, response_writer) = create_pipe_handles();

    let (_handle, task_id) = create_task();
    transfer_handle(args_reader, task_id);
    transfer_handle(response_writer, task_id);

    match exec_flat_binary(task_id, phys_addr, file_size) {
        Ok(_) => {}
        Err(e) => {
            LOGGER.log(format_args!("Failed to exec FATDRV flat binary: {:?}", e));
            return false;
        }
    }

    // Protocol: [u8 drive_letter_len][drive_letter][u8 dev_name_len][dev_name]
    let _ = write_sync(args_writer, &[drive_letter.len() as u8], 0);
    let _ = write_sync(args_writer, drive_letter.as_bytes(), 0);
    let _ = write_sync(args_writer, &[dev_name.len() as u8], 0);
    let _ = write_sync(args_writer, dev_name.as_bytes(), 0);

    // Wait for driver to signal ready
    let _ = read_sync(response_reader, &mut [0u8], 0);

    LOGGER.log(format_args!(
        "Successfully started userspace FAT driver for {}:\\",
        drive_letter
    ));
    true
}

/// Mount using the in-kernel driver (original approach)
fn mount_kernel_driver(drive_letter: &str, dev_name: &str) {
    let (args_reader, args_writer) = create_pipe_handles();
    let (response_reader, response_writer) = create_pipe_handles();

    let task_id = create_kernel_task(run_driver, Some("FATFS"));
    transfer_handle(args_reader, task_id);
    transfer_handle(response_writer, task_id);

    let _ = write_sync(args_writer, &[dev_name.len() as u8], 0);
    let _ = write_sync(args_writer, dev_name.as_bytes(), 0);
    let _ = read_sync(response_reader, &mut [0u8], 0);

    install_task_fs(drive_letter, task_id);
}

pub fn mount_fat_fs() {
    let pairs = [("A", "FD1"), ("C", "ATA1")];

    for pair in pairs.iter() {
        LOGGER.log(format_args!("Mounting {}:\\ on DEV:\\{}", pair.0, pair.1));

        // Try the userspace driver first (loaded by bootloader as a flat binary).
        // Falls back to the in-kernel driver if the binary wasn't loaded.
        if !try_mount_userspace(pair.0, pair.1) {
            LOGGER.log(format_args!(
                "Userspace driver unavailable, using kernel driver for {}:\\",
                pair.0
            ));
            mount_kernel_driver(pair.0, pair.1);
        }
    }
}
