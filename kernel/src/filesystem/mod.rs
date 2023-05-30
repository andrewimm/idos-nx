pub mod arbiter;
pub mod drive;
pub mod drivers;
pub mod error;
pub mod kernel;

use alloc::boxed::Box;
use alloc::sync::Arc;
use drive::{DriveID, DriveMap, FileSystemDriver};
use drivers::initfs::InitFileSystem;
use error::FsError;

use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::id::TaskID;
use crate::devices::zero::ZeroDriver;

use self::drivers::devfs::DevFileSystem;

static DRIVE_MAP: DriveMap = DriveMap::new();

pub fn init_fs() {
    DRIVE_MAP.install("INIT", Box::new(InitFileSystem::new()));

    {
        let dev_fs = DevFileSystem::new();
        dev_fs.install_sync_driver("ZERO", Arc::new(Box::new(ZeroDriver::new())));

        DRIVE_MAP.install("DEV", Box::new(dev_fs));
    }

    let async_demo = create_kernel_task(drivers::demofs::demo_fs_task);
    DRIVE_MAP.install_async("DEMO", async_demo);

    create_kernel_task(arbiter::arbiter_task);
}

pub fn get_drive_id_by_name(name: &str) -> Result<DriveID, FsError> {
    DRIVE_MAP.get_id_by_name(name).ok_or_else(|| FsError::DriveNotFound)
}

pub fn get_driver_by_id(id: DriveID) -> Result<FileSystemDriver, ()> {
    DRIVE_MAP.get_driver(id).ok_or_else(|| ())
}

pub fn install_device_driver(name: &str, task: TaskID, sub_id: u32) -> Result<(), FsError> {
    let dev_fs_id = get_drive_id_by_name("DEV")?;
    let driver = get_driver_by_id(dev_fs_id).map_err(|_| FsError::DriveNotFound)?;

    let command = self::drivers::devfs::ConfigurationCommands::InstallDevice as u32;
    let name_start = name.as_ptr() as u32;
    let name_len = name.len() as u32;
    driver.configure(command, name_start, name_len, task.into(), sub_id)
        .map(|_| ())
        .map_err(|_| FsError::InstallFailed)
}

#[cfg(test)]
mod tests {

    #[test_case]
    fn sync_device_read() {
        let mut buffer: [u8; 5] = [b'A'; 5];
        let devzero = crate::task::actions::io::open_path("DEV:\\ZERO").unwrap();
        let read_len = crate::task::actions::io::read_file(devzero, &mut buffer).unwrap();
        assert_eq!(read_len, 5);
        assert_eq!(buffer[0], 0);
        assert_eq!(buffer[4], 0);
        crate::task::actions::io::close_file(devzero);
    }
}

