pub mod bpb;
pub mod dir;
pub mod disk;
pub mod table;

use crate::files::cursor::SeekMethod;
use crate::filesystem::install_async_fs;
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::{read_message_blocking, send_message};
use self::disk::DiskAccess;
use super::asyncfs::AsyncDriver;

pub struct FatFS {
    disk: DiskAccess,
}

impl FatFS {
    pub fn new(mount: &str) -> Self {
        let mut disk = DiskAccess::new(mount);

        //let mut volume_label: [u8; 11] = [0x20; 11];
        //disk.read_bytes_from_disk(0x2b, &mut volume_label);
        //let label_str = core::str::from_utf8(&volume_label).unwrap();
        //crate::kprint!("FAT VOLUME LABEL: \"{}\"\n", label_str);

        Self {
            disk,
        }
    }
}

impl AsyncDriver for FatFS {
    fn open(&mut self, path: &str) -> u32 {
        0
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> u32 {
        0
    }

    fn write(&mut self, instance: u32, buffer: &[u8]) -> u32 {
        0
    }

    fn close(&mut self, handle: u32) {
        
    }

    fn seek(&mut self, instance: u32, offset: SeekMethod) -> u32 {
        0
    }
}

fn run_driver() -> ! {
    crate::kprint!("Mount FAT FS on FD1\n");

    let mut driver_impl = FatFS::new("DEV:\\FD1");

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

