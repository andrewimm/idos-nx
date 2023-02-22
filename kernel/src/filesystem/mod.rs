pub mod drive;
pub mod driver;
pub mod drivers;
pub mod error;
pub mod kernel;

use alloc::boxed::Box;
use drive::{DriveID, DriveMap};
use driver::FileSystemDriver;
use drivers::initfs::InitFileSystem;
use error::FsError;

static DRIVE_MAP: DriveMap = DriveMap::new();

pub fn init_fs() {
    let fs = Box::new(InitFileSystem::new());
    DRIVE_MAP.install_sync("INIT", fs);
}

pub fn get_drive_id_by_name(name: &str) -> Result<DriveID, FsError> {
    return Err(FsError::DriveNotFound);
}

pub fn get_driver_by_id(id: DriveID) -> Result<FileSystemDriver, ()> {
    DRIVE_MAP.get_driver(id).ok_or_else(|| ())
}

