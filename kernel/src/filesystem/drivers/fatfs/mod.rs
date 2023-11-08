pub mod bpb;
pub mod dir;
pub mod disk;
pub mod driver;
pub mod fs;
pub mod table;

use crate::filesystem::install_async_fs;
use crate::task::actions::io::{open_pipe, transfer_handle, read_file, write_file, close_file};
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::files::FileHandle;
use self::driver::FatDriver;
use super::asyncfs::AsyncDriver;

fn run_driver() -> ! {
    let args_reader = FileHandle::new(0);
    let response_writer = FileHandle::new(1);
    
    let mut name_length_buffer: [u8; 1] = [0; 1];
    read_file(args_reader, &mut name_length_buffer).unwrap();
    let name_length = name_length_buffer[0] as usize;

    let mut dev_name_buffer: [u8; 5 + 8] = [0; 5 + 8];
    &mut dev_name_buffer[0..5].copy_from_slice("DEV:\\".as_bytes());
    let dev_name_len = 5 + read_file(args_reader, &mut dev_name_buffer[5..(5 + name_length)]).unwrap() as usize;
    
    let dev_name = unsafe {
        core::str::from_utf8_unchecked(&dev_name_buffer[..dev_name_len])
    };

    crate::kprint!("Mount FAT FS on {}\n", dev_name);

    let mut driver_impl = FatDriver::new(dev_name);

    write_file(response_writer, &[1]).unwrap();

    close_file(args_reader).unwrap();
    close_file(response_writer).unwrap();

    loop {
        let (message_read, _) = read_message_blocking(None);
        if let Some(packet) = message_read {
            let (sender, message) = packet.open();

            match driver_impl.handle_request(message) {
                Some(response) => send_message(sender, response, 0xffffffff),
                None => continue,
            }
        }
    }
}

pub fn mount_fat_fs() {
    let pairs = [
        //("A", "FD1"),
        ("C", "ATA1"),
    ];

    for pair in pairs {
        let (args_reader, args_writer) = open_pipe().unwrap();
        let (response_reader, response_writer) = open_pipe().unwrap();

        let task = create_kernel_task(run_driver, Some("FATFS"));
        transfer_handle(args_reader, task).unwrap();
        transfer_handle(response_writer, task).unwrap();

        write_file(args_writer, &[pair.1.len() as u8]).unwrap();
        write_file(args_writer, pair.1.as_bytes()).unwrap();
        read_file(response_reader, &mut [0u8]).unwrap();

        install_async_fs(pair.0, task);
    }
}

