pub mod arbiter;
pub mod drive;
pub mod drivers;
pub mod error;
pub mod kernel;

use alloc::boxed::Box;
use drive::{DriveID, DriveMap, FileSystemDriver};
use drivers::initfs::InitFileSystem;
use error::FsError;

use crate::task::actions::lifecycle::create_kernel_task;

use self::drivers::devfs::DevFileSystem;

static DRIVE_MAP: DriveMap = DriveMap::new();

pub fn init_fs() {
    DRIVE_MAP.install("INIT", Box::new(InitFileSystem::new()));

    DRIVE_MAP.install("DEV", Box::new(DevFileSystem::new()));

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

