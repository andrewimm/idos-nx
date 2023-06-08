pub mod bpb;
pub mod dir;
pub mod disk;
pub mod driver;
pub mod fs;
pub mod table;

use crate::filesystem::install_async_fs;
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::{read_message_blocking, send_message};
use self::driver::FatDriver;
use super::asyncfs::AsyncDriver;

fn run_driver() -> ! {
    crate::kprint!("Mount FAT FS on ATA1\n");

    let mut driver_impl = FatDriver::new("DEV:\\ATA1");

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
    let task = create_kernel_task(run_driver);
    install_async_fs("A", task);
}

