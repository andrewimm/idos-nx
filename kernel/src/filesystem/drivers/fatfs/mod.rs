pub mod bpb;
pub mod dir;
pub mod disk;
pub mod driver;
pub mod fs;
pub mod table;

use crate::filesystem::install_async_fs;
use crate::task::actions::io::{open_pipe, transfer_handle, read_file, write_file};
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::files::FileHandle;
use self::driver::FatDriver;
use super::asyncfs::AsyncDriver;

fn run_driver() -> ! {
    crate::kprint!("Mount FAT FS on FD1\n");

    let mut driver_impl = FatDriver::new("DEV:\\FD1");

    write_file(FileHandle::new(0), &[1]).unwrap();

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
    let (read_handle, write_handle) = open_pipe().unwrap();
    let task = create_kernel_task(run_driver);
    transfer_handle(write_handle, task).unwrap();
    read_file(read_handle, &mut [0u8]).unwrap();
    install_async_fs("A", task);
}

