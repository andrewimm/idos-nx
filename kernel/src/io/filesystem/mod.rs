pub mod driver;
#[cfg(test)]
pub mod testing;

use core::sync::atomic::{AtomicU32, Ordering};

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use idos_api::io::error::IOError;
use spin::RwLock;

use crate::files::path::Path;
use crate::memory::shared::SharedMemoryRange;
use crate::task::id::TaskID;

use self::driver::DriverID;
use self::driver::DriverType;
use self::driver::InstalledDriver;

use super::async_io::AsyncOp;
use super::driver::comms::{IOResult, DriverIOAction};
use super::driver::io_task::send_async_request;

static INSTALLED_DRIVERS: RwLock<BTreeMap<u32, (String, DriverType)>> = RwLock::new(BTreeMap::new());
static NEXT_DRIVER_ID: AtomicU32 = AtomicU32::new(1);

pub fn get_driver_id_by_name(name: &str) -> Option<DriverID> {
    let drivers = INSTALLED_DRIVERS.read();
    for (id, (drive_name, _)) in drivers.iter() {
        if drive_name.as_str() == name {
            return Some(DriverID::new(*id));
        }
    }
    None
}

pub fn install_sync_fs(name: &str, driver: InstalledDriver) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(id, (name.to_string(), DriverType::SyncFilesystem(driver))); 
    DriverID::new(id)
}

pub fn install_sync_dev(name: &str, driver: InstalledDriver) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(id, (name.to_string(), DriverType::SyncDevice(driver)));
    DriverID::new(id)
}

pub fn install_async_fs(name: &str, task: TaskID) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(id, (name.to_string(), DriverType::AsyncFilesystem(task)));
    DriverID::new(id)
}

/// Run the open() operation on an installed driver
pub fn driver_open(id: DriverID, path: Path, io_callback: (u32, u32)) -> Option<IOResult> {
    let drivers = INSTALLED_DRIVERS.read();
    let (_, driver) = match drivers.get(&id) {
        Some(d) => d,
        None => {
            return Some(Err(IOError::NotFound));
        },
    };
    match driver {
        DriverType::SyncFilesystem(fs) => {
            return Some(fs.open(path));
        },
        DriverType::AsyncFilesystem(fs) => {
            async_open(*fs, path, io_callback);
            return None;
        },
        DriverType::SyncDevice(dev) => {
            return Some(dev.open(Path::from_str("")));
        },
        _ => panic!("Not implemented"),
    }
}

fn async_open(task: TaskID, path: Path, io_callback: (u32, u32)) {
    // clone the string so that when the method returns, the original path can
    // be dropped or moved
    let path_copy = Into::<String>::into(path).clone();
    let path_str = path_copy.as_str();
    let path_slice = path_str.as_bytes();

    let (shared_range, action) = if path_slice.len() == 0 {
        // can't share memory for an empty slice, just hardcode it
        (None, DriverIOAction::Open(0, 0))
    } else {
        let shared_range = SharedMemoryRange::for_slice::<u8>(path_slice);
        let shared_to_driver = shared_range.share_with_task(task);
        (
            Some(shared_range),
            DriverIOAction::Open(
                shared_to_driver.get_range_start(),
                shared_to_driver.range_length,
            ),
        )
    };

    send_async_request(
        task,
        io_callback.0,
        io_callback.1,
        action,
        shared_range,
    );
}

pub fn driver_read(id: DriverID, instance: u32, buffer: &mut [u8]) -> Option<IOResult> {
    let drivers = INSTALLED_DRIVERS.read();
    let (_, driver) = match drivers.get(&id) {
        Some(d) => d,
        None => {
            return Some(Err(IOError::NotFound));
        },
    };
    match driver {
        DriverType::SyncFilesystem(fs) => {
            return Some(fs.read(instance, buffer));
        },
        DriverType::SyncDevice(dev) => {
            return Some(dev.read(instance, buffer));
        },
        _ => panic!("Not implemented"),
    }
}

