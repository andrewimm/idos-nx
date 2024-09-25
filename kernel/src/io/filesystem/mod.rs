pub mod devfs;
pub mod driver;
pub mod fatfs;
pub mod taskfs;
#[cfg(test)]
pub mod testing;

use core::sync::atomic::{AtomicU32, Ordering};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use idos_api::io::error::IOError;
use spin::{Once, RwLock};

use crate::files::cursor::SeekMethod;
use crate::files::path::Path;
use crate::files::stat::FileStatus;
use crate::memory::shared::SharedMemoryRange;
use crate::task::id::TaskID;

use self::devfs::DevFileSystem;
use self::driver::{DriverID, DriverType, InstalledDriver, AsyncIOCallback};

use super::async_io::{AsyncOp, AsyncOpID};
use super::driver::comms::{IOResult, DriverIOAction};
use super::driver::io_task::send_async_request;

static INSTALLED_DRIVERS: RwLock<BTreeMap<u32, (String, DriverType)>> = RwLock::new(BTreeMap::new());
static NEXT_DRIVER_ID: AtomicU32 = AtomicU32::new(1);
static DEV_FS: Once<DriverType> = Once::new();

pub fn get_dev_fs() -> &'static DriverType {
    DEV_FS.call_once(|| {
        DriverType::KernelFilesystem(Box::new(DevFileSystem::new()))
    })
}

pub fn get_driver_id_by_name(name: &str) -> Option<DriverID> {
    let drivers = INSTALLED_DRIVERS.read();
    for (id, (drive_name, _)) in drivers.iter() {
        if drive_name.as_str() == name {
            return Some(DriverID::new(*id));
        }
    }
    None
}

pub fn get_all_drive_names() -> Vec<String> {
    let drivers = INSTALLED_DRIVERS.read();
    drivers.iter().filter_map(|(_, (name, driver))| {
        match driver {
            DriverType::KernelFilesystem(_) | DriverType::TaskFilesystem(_) => Some(name.clone()),
            _ => None,
        }
    }).collect()
}

pub fn get_all_dev_names() -> Vec<String> {
    let drivers = INSTALLED_DRIVERS.read();
    drivers.iter().filter_map(|(_, (name, driver))| {
        match driver {
            DriverType::KernelDevice(_) | DriverType::TaskDevice(_, _) => Some(name.clone()),
            _ => None,
        }
    }).collect()
}

pub fn install_kernel_fs(name: &str, driver: InstalledDriver) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(id, (name.to_string(), DriverType::KernelFilesystem(driver))); 
    DriverID::new(id)
}

pub fn install_kernel_dev(name: &str, driver: InstalledDriver) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(id, (name.to_string(), DriverType::KernelDevice(driver)));
    DriverID::new(id)
}

pub fn install_task_fs(name: &str, task: TaskID) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(id, (name.to_string(), DriverType::TaskFilesystem(task)));
    DriverID::new(id)
}

pub fn install_task_dev(name: &str, task: TaskID, sub_driver: u32) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(id, (name.to_string(), DriverType::TaskDevice(task, sub_driver)));
    DriverID::new(id)
}

pub fn with_driver<F>(driver_id: DriverID, f: F) -> Option<IOResult>
    where F: FnOnce(&DriverType) -> Option<IOResult> {

    if driver_id.is_dev() {
        return f(get_dev_fs());
    }

    let drivers = INSTALLED_DRIVERS.read();
    let (_, driver) = match drivers.get(&driver_id) {
        Some(d) => d,
        None => return Some(Err(IOError::NotFound)),
    };
    f(driver)
}

/// Run the open() operation on an installed driver
pub fn driver_open(driver_id: DriverID, path: Path, io_callback: AsyncIOCallback) -> Option<IOResult> {
    with_driver(driver_id, |driver| {
        match driver {
            DriverType::KernelFilesystem(fs) => {
                return fs.open(Some(path), io_callback);
            },
            DriverType::KernelDevice(dev) => {
                return dev.open(None, io_callback);
            },
            DriverType::TaskDevice(dev, sub) => {
                let action = DriverIOAction::OpenRaw(*sub);
                send_async_request(*dev, io_callback, action, None);
                return None;
            },
            DriverType::TaskFilesystem(task) => {
                // Unlike other ops that wait for a buffer to be filled, the
                // path string passed to the original call is never used again.
                // This means the original memory can easily be dropped by the
                // time the driver tries to consume the path. In order to
                // ensure a version of the string is still available, we create
                // a copy that will be dropped when the op completes.
                let path_boxed = Into::<String>::into(path).into_boxed_str();
                let path_len = path_boxed.len();
                // doing this ensures the box is not dropped
                let path_ptr = Box::into_raw(path_boxed) as *const u8;
                let (shared_range, action) = if path_len == 0 {
                    // can't share memory for an empty slice, just hardcode it
                    (None, DriverIOAction::Open(0, 0))
                } else {
                    // TODO: This is not ideal. We're sharing a page of kernel
                    // heap with the driver. That's not safe. We should create
                    // a new shared memory concept that allocates a new frame,
                    // copies a string to it, and maps it to the shared task.
                    let boxed_slice = unsafe {
                        core::slice::from_raw_parts(path_ptr, path_len)
                    };
                    let shared_range = SharedMemoryRange::for_slice::<u8>(boxed_slice);
                    let shared_to_driver = shared_range.share_with_task(*task);
                    (
                        Some(shared_range),
                        DriverIOAction::Open(
                            shared_to_driver.get_range_start(),
                            shared_to_driver.range_length,
                        ),
                    )
                };

                send_async_request(
                    *task,
                    io_callback,
                    action,
                    shared_range,
                );
                None
            },
        }
    })
}

pub fn driver_close(id: DriverID, instance: u32, io_callback: AsyncIOCallback) -> Option<IOResult> {
    with_driver(id, |driver| {
        match driver {
            DriverType::KernelFilesystem(d)
            | DriverType::KernelDevice(d) => d.close(instance, io_callback),

            DriverType::TaskFilesystem(task_id)
            | DriverType::TaskDevice(task_id, _) => {
                let action = DriverIOAction::Close(instance);
                send_async_request(
                    *task_id,
                    io_callback,
                    action,
                    None,
                );
                None
            },
        }
    })
}

pub fn driver_read(id: DriverID, instance: u32, buffer: &mut [u8], io_callback: AsyncIOCallback) -> Option<IOResult> {
    with_driver(id, |driver| {
        match driver {
            DriverType::KernelFilesystem(d)
            | DriverType::KernelDevice(d) => d.read(instance, buffer, io_callback),

            DriverType::TaskFilesystem(task_id)
            | DriverType::TaskDevice(task_id, _) => {
                let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
                let shared_to_driver = shared_range.share_with_task(*task_id);

                let action = DriverIOAction::Read(
                    instance,
                    shared_to_driver.get_range_start(),
                    shared_to_driver.range_length,
                );

                send_async_request(
                    *task_id,
                    io_callback,
                    action,
                    Some(shared_to_driver),
                );
                None
            },
        }
    })
}

pub fn driver_write(id: DriverID, instance: u32, buffer: &[u8], io_callback: AsyncIOCallback) -> Option<IOResult> {
    with_driver(id, |driver| {
        match driver {
            DriverType::KernelFilesystem(d)
            | DriverType::KernelDevice(d) => d.write(instance, buffer, io_callback),

            DriverType::TaskFilesystem(task_id)
            | DriverType::TaskDevice(task_id, _) => {
                let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
                let shared_to_driver = shared_range.share_with_task(*task_id);

                let action = DriverIOAction::Write(
                    instance,
                    shared_to_driver.get_range_start(),
                    shared_to_driver.range_length,
                );

                send_async_request(
                    *task_id,
                    io_callback,
                    action,
                    Some(shared_to_driver),
                );
                None
            },
        }
    })
}

pub fn driver_seek(id: DriverID, instance: u32, method: u32, offset: u32, io_callback: AsyncIOCallback) -> Option<IOResult> {
    let seek_method = match SeekMethod::decode(method, offset) {
        Some(m) => m,
        None => return Some(Err(IOError::InvalidArgument)),
    };
    with_driver(id, |driver| {
        match driver {
            DriverType::KernelFilesystem(d)
            | DriverType::KernelDevice(d) => d.seek(instance, seek_method, io_callback),

            DriverType::TaskFilesystem(task_id)
            | DriverType::TaskDevice(task_id, _) => {
                let action = DriverIOAction::Seek(
                    instance,
                    method,
                    offset,
                );

                send_async_request(
                    *task_id,
                    io_callback,
                    action,
                    None,
                );
                None
            },
        }
    })
}

pub fn driver_stat(id: DriverID, instance: u32, file_status: &mut FileStatus, io_callback: AsyncIOCallback) -> Option<IOResult> {
    with_driver(id, |driver| {
        match driver {
            DriverType::KernelFilesystem(d)
            | DriverType::KernelDevice(d) => d.stat(instance, file_status, io_callback),

            DriverType::TaskFilesystem(task_id)
            | DriverType::TaskDevice(task_id, _) => {
                let shared_range = SharedMemoryRange::for_struct::<FileStatus>(file_status);
                let shared_to_driver = shared_range.share_with_task(*task_id);

                let action = DriverIOAction::Stat(
                    instance,
                    shared_to_driver.get_range_start(),
                    shared_to_driver.range_length,
                ); 

                send_async_request(
                    *task_id,
                    io_callback,
                    action,
                    Some(shared_to_driver),
                );
                None
            },
        }
    })
}
