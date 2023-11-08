pub mod driver;
#[cfg(test)]
pub mod testing;

use core::sync::atomic::{AtomicU32, Ordering};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use idos_api::io::error::IOError;
use spin::RwLock;

use crate::files::path::Path;
use crate::memory::shared::SharedMemoryRange;
use crate::task::id::TaskID;

use self::driver::{DriverID, DriverType, InstalledDriver, AsyncIOCallback};

use super::async_io::{AsyncOp, AsyncOpID};
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

pub fn install_async_dev(name: &str, task: TaskID, sub_driver: u32) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(id, (name.to_string(), DriverType::AsyncDevice(task, sub_driver)));
    DriverID::new(id)
}

/// Run the open() operation on an installed driver
pub fn driver_open(driver_id: DriverID, path: Path, io_callback: AsyncIOCallback) -> Option<IOResult> {
    let drivers = INSTALLED_DRIVERS.read();
    let (_, driver) = match drivers.get(&driver_id) {
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
        DriverType::AsyncDevice(dev, sub) => {
            let action = DriverIOAction::OpenRaw(*sub);
            send_async_request(*dev, io_callback, action, None);
            return None;
        },
        _ => panic!("Not implemented"),
    }
}

fn async_open(task: TaskID, path: Path, io_callback: AsyncIOCallback) {
    // Unlike other ops that wait for a buffer to be filled, the path string
    // passed to the original call is never used again. This means the original
    // memory can easily be dropped by the time the driver tries to consume the
    // path. In order to ensure a version of the string is still available, we
    // create a copy that will be dropped when the op completes.
    let path_boxed = Into::<String>::into(path).into_boxed_str();
    let path_len = path_boxed.len();
    // doing this ensures the box is not dropped
    let path_ptr = Box::into_raw(path_boxed) as *const u8;
    let (shared_range, action) = if path_len == 0 {
        // can't share memory for an empty slice, just hardcode it
        (None, DriverIOAction::Open(0, 0))
    } else {
        // TODO: This is not ideal. We're sharing a page of kernel heap with
        // the driver. That's not safe. We should create a new shared memory
        // concept that allocates a new frame, copies a string to it, and maps
        // it to the shared task.
        let boxed_slice = unsafe {
            core::slice::from_raw_parts(path_ptr, path_len)
        };
        let shared_range = SharedMemoryRange::for_slice::<u8>(boxed_slice);
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
        io_callback,
        action,
        shared_range,
    );
}

pub fn driver_read(id: DriverID, instance: u32, buffer: &mut [u8], io_callback: AsyncIOCallback) -> Option<IOResult> {
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
        DriverType::AsyncFilesystem(fs_id) => {
            async_read(*fs_id, instance, buffer, io_callback);
            return None;
        },
        DriverType::SyncDevice(dev) => {
            return Some(dev.read(instance, buffer));
        },
        DriverType::AsyncDevice(dev, _) => {
            async_read(*dev, instance, buffer, io_callback);
            return None;
        },
        _ => panic!("Not implemented"),
    }
}

fn async_read(task: TaskID, instance: u32, buffer: &mut [u8], io_callback: AsyncIOCallback) {
    let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
    let shared_to_driver = shared_range.share_with_task(task);

    let action = DriverIOAction::Read(
        instance,
        shared_to_driver.get_range_start(),
        shared_to_driver.range_length,
    );

    send_async_request(
        task,
        io_callback,
        action,
        Some(shared_range),
    );
}

