pub mod arbiter;
pub mod drive;
pub mod drivers;
pub mod error;
pub mod kernel;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use drive::{DriveID, DriveMap, FileSystemDriver};
use error::FsError;

use crate::devices::zero::ZeroDriver;
use crate::hardware::ps2::keyboard::KeyboardDriver;
use crate::task::actions::lifecycle::create_kernel_task;

use self::drivers::devfs::DevFileSystem;

static DRIVE_MAP: DriveMap = DriveMap::new();

pub fn init_fs() {
    {
        //let dev_fs = DevFileSystem::new();
        //dev_fs.install_sync_driver("KBD", Arc::new(Box::new(KeyboardDriver::new())));
        //DRIVE_MAP.install("DEV", Box::new(dev_fs));
    }

    create_kernel_task(arbiter::arbiter_task, Some("ARBITER"));
}

pub fn get_drive_id_by_name(name: &str) -> Result<DriveID, FsError> {
    DRIVE_MAP
        .get_id_by_name(name)
        .ok_or_else(|| FsError::DriveNotFound)
}

pub fn get_driver_by_id(id: DriveID) -> Result<FileSystemDriver, ()> {
    DRIVE_MAP.get_driver(id).ok_or_else(|| ())
}

pub fn get_drive_names() -> Vec<String> {
    DRIVE_MAP.get_all_names()
}
