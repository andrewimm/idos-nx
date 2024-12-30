//! Device drivers can be exposed as files under the DEV: drive
//! This is an extension of DOS's globally available device names like COM,
//! LPI, etc. Now, every device can be given a filename without polluting the
//! global namespace.

use crate::collections::SlotList;
use crate::devices::{DeviceDriver, SyncDriverType};
use crate::files::cursor::SeekMethod;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::filesystem::arbiter::{begin_io, AsyncIO};
use crate::filesystem::kernel::KernelFileSystem;
use crate::io::IOError;
use crate::memory::shared::SharedMemoryRange;
use crate::task::id::TaskID;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use spin::{Mutex, RwLock};

pub struct DevFileSystem {
    /// Stores the actual device driver enums
    installed_drivers: RwLock<SlotList<DeviceDriver>>,
    /// Maps a device name to the index in the installed_drivers list
    drivers_by_name: RwLock<BTreeMap<String, usize>>,
    /// Map an open file to the driver and file instance it references
    open_handles: RwLock<SlotList<OpenHandle>>,
}

enum OpenHandle {
    // handle for listing the contents of the DEV: drive
    RootDir(String, usize),
    // handle pointing to a device file
    // tuple of a driver index and unique file instance
    DeviceDriver(usize, u32),
}

impl DevFileSystem {
    pub const fn new() -> Self {
        Self {
            installed_drivers: RwLock::new(SlotList::new()),
            drivers_by_name: RwLock::new(BTreeMap::new()),
            open_handles: RwLock::new(SlotList::new()),
        }
    }

    fn install(&self, name: &str, driver: DeviceDriver) {
        let key = name.to_string();
        if self.drivers_by_name.read().contains_key(&key) {
            // should probably error out
            return;
        }
        let index = self.installed_drivers.write().insert(driver);
        self.drivers_by_name.write().insert(key, index);
    }

    pub fn install_sync_driver(&self, name: &str, driver: Arc<Box<SyncDriverType>>) {
        self.install(name, DeviceDriver::SyncDriver(driver));
    }

    pub fn install_async_driver(&self, name: &str, driver_id: TaskID, sub_id: u32) {
        self.install(name, DeviceDriver::AsyncDriver(driver_id, sub_id));
    }

    fn get_driver(&self, index: usize) -> Option<DeviceDriver> {
        self.installed_drivers.read().get(index).cloned()
    }

    fn get_driver_by_name(&self, name: String) -> Option<(usize, DeviceDriver)> {
        let index: usize = self.drivers_by_name.read().get(&name).copied()?;
        self.installed_drivers
            .read()
            .get(index)
            .cloned()
            .map(|driver| (index, driver))
    }

    fn async_op(&self, task: TaskID, request: AsyncIO) -> Option<Result<u32, u32>> {
        let response: Arc<Mutex<Option<Result<u32, u32>>>> = Arc::new(Mutex::new(None));

        // send the request
        begin_io(task, request, response.clone());

        match Arc::try_unwrap(response) {
            Ok(inner) => *inner.lock(),
            Err(_) => None,
        }
    }
}

impl KernelFileSystem for DevFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, IOError> {
        crate::kprint!("  Open Device {}\n", path.as_str());
        if path.is_empty() {
            let mut drives = String::new();
            for (name, _) in self.drivers_by_name.read().iter() {
                drives.push_str(name);
                drives.push('\0');
            }
            let handle = self
                .open_handles
                .write()
                .insert(OpenHandle::RootDir(drives, 0));
            return Ok(DriverHandle(handle as u32));
        }
        let (driver_index, driver) = self
            .get_driver_by_name(path.into())
            .ok_or(IOError::NotFound)?;
        match driver {
            DeviceDriver::SyncDriver(driver) => {
                let open_instance: u32 = driver.open()?;
                let handle = self
                    .open_handles
                    .write()
                    .insert(OpenHandle::DeviceDriver(driver_index, open_instance));
                Ok(DriverHandle(handle as u32))
            }
            DeviceDriver::AsyncDriver(id, sub_id) => {
                // send the command to the async driver
                let result = self
                    .async_op(id, AsyncIO::OpenRaw(sub_id))
                    .ok_or(IOError::FileSystemError)?;

                let open_instance = result.map_err(|err| IOError::try_from(err).unwrap())?;

                let handle = self
                    .open_handles
                    .write()
                    .insert(OpenHandle::DeviceDriver(driver_index, open_instance));
                Ok(DriverHandle(handle as u32))
            }
        }
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<u32, IOError> {
        let (driver_index, open_instance) = match self.open_handles.write().get_mut(handle.into()) {
            Some(OpenHandle::DeviceDriver(driver, instance)) => (*driver, *instance),
            Some(OpenHandle::RootDir(names, cursor)) => {
                let mut bytes_written = 0;
                let bytes_remaining = names.len() - *cursor;
                let bytes_to_write = bytes_remaining.min(buffer.len());
                while bytes_written < bytes_to_write {
                    buffer[bytes_written] = names.as_bytes()[*cursor + bytes_written];
                    bytes_written += 1;
                }
                *cursor += bytes_written;
                return Ok(bytes_written as u32);
            }
            None => return Err(IOError::FileHandleInvalid),
        };
        let driver = self.get_driver(driver_index).ok_or(IOError::NotFound)?;
        match driver {
            DeviceDriver::SyncDriver(driver) => driver.read(open_instance, buffer),
            DeviceDriver::AsyncDriver(id, _) => {
                let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
                let shared_to_driver = shared_range.share_with_task(id);
                let response = self.async_op(
                    id,
                    AsyncIO::Read(
                        open_instance,
                        shared_to_driver.get_range_start(),
                        shared_to_driver.range_length,
                    ),
                );

                match response {
                    Some(Ok(count)) => Ok(count),
                    Some(Err(err)) => Err(IOError::try_from(err).unwrap()),
                    None => Err(IOError::FileSystemError),
                }
            }
        }
    }

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<u32, IOError> {
        let (driver_index, open_instance) = match self.open_handles.read().get(handle.into()) {
            Some(OpenHandle::DeviceDriver(driver, instance)) => (*driver, *instance),
            Some(_) => return Err(IOError::FileHandleWrongType),
            _ => return Err(IOError::FileHandleInvalid),
        };
        let driver = self.get_driver(driver_index).ok_or(IOError::NotFound)?;
        match driver {
            DeviceDriver::SyncDriver(driver) => driver.write(open_instance, buffer),
            DeviceDriver::AsyncDriver(id, _) => {
                let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
                let shared_to_driver = shared_range.share_with_task(id);

                let response = self.async_op(
                    id,
                    AsyncIO::Write(
                        open_instance,
                        shared_to_driver.get_range_start(),
                        shared_to_driver.range_length,
                    ),
                );

                match response {
                    Some(Ok(count)) => Ok(count),
                    Some(Err(err)) => Err(IOError::try_from(err).unwrap()),
                    None => Err(IOError::FileSystemError),
                }
            }
        }
    }

    fn close(&self, handle: DriverHandle) -> Result<(), IOError> {
        let list_index: usize = handle.into();
        let open_handle = self
            .open_handles
            .write()
            .remove(list_index)
            .ok_or(IOError::FileHandleInvalid)?;
        let (driver_index, open_instance) = match open_handle {
            OpenHandle::DeviceDriver(index, instance) => (index, instance),
            _ => return Ok(()),
        };
        let driver = self
            .installed_drivers
            .read()
            .get(driver_index)
            .cloned()
            .ok_or(IOError::FileSystemError)?;
        match driver {
            DeviceDriver::SyncDriver(driver) => driver.close(open_instance),
            DeviceDriver::AsyncDriver(id, _) => {
                let response = self.async_op(id, AsyncIO::Close(open_instance));
                match response {
                    Some(Ok(_)) => Ok(()),
                    Some(Err(err)) => Err(IOError::try_from(err).unwrap()),
                    None => Err(IOError::FileSystemError),
                }
            }
        }
    }

    fn seek(&self, handle: DriverHandle, offset: SeekMethod) -> Result<u32, IOError> {
        let (driver_index, open_instance) = match self.open_handles.read().get(handle.into()) {
            Some(OpenHandle::DeviceDriver(driver, instance)) => (*driver, *instance),
            Some(_) => return Err(IOError::FileHandleWrongType),
            _ => return Err(IOError::FileHandleInvalid),
        };
        let driver = self.get_driver(driver_index).ok_or(IOError::NotFound)?;
        match driver {
            DeviceDriver::SyncDriver(driver) => driver.seek(open_instance, offset),
            DeviceDriver::AsyncDriver(id, _) => {
                let (method, delta) = offset.encode();
                let response = self.async_op(id, AsyncIO::Seek(open_instance, method, delta));

                match response {
                    Some(Ok(count)) => Ok(count),
                    Some(Err(err)) => Err(IOError::try_from(err).unwrap()),
                    None => Err(IOError::FileSystemError),
                }
            }
        }
    }

    fn dup(&self, handle: DriverHandle, dup_into: Option<u32>) -> Result<DriverHandle, IOError> {
        let (driver_index, open_instance) = match self.open_handles.read().get(handle.into()) {
            Some(OpenHandle::DeviceDriver(driver, instance)) => (*driver, *instance),
            Some(_) => return Err(IOError::FileHandleWrongType),
            _ => return Err(IOError::FileHandleInvalid),
        };
        let driver = self.get_driver(driver_index).ok_or(IOError::NotFound)?;
        match driver {
            DeviceDriver::SyncDriver(driver) => {
                let open_instance: u32 = driver.dup(open_instance, dup_into)?;
                let handle = self
                    .open_handles
                    .write()
                    .insert(OpenHandle::DeviceDriver(driver_index, open_instance));
                Ok(DriverHandle(handle as u32))
            }
            DeviceDriver::AsyncDriver(id, _) => {
                let dup_into_encoded = match dup_into {
                    Some(value) => value,
                    None => 0xffffffff,
                };
                let result = self
                    .async_op(id, AsyncIO::Dup(handle.into(), dup_into_encoded))
                    .ok_or(IOError::FileSystemError)?;

                let open_instance = result.map_err(|err| IOError::try_from(err).unwrap())?;

                let handle = self
                    .open_handles
                    .write()
                    .insert(OpenHandle::DeviceDriver(driver_index, open_instance));
                Ok(DriverHandle(handle as u32))
            }
        }
    }

    fn configure(
        &self,
        command: u32,
        arg0: u32,
        arg1: u32,
        arg2: u32,
        arg3: u32,
    ) -> Result<u32, IOError> {
        match command {
            1 => {
                // Install device driver with TaskID of `arg2`
                // arg3 will be used as the driver sub-id, for drivers that
                // service multiple device names with a single Task.
                // Assume arg0 and arg1 are the pointer and length of a string
                // containing the device name
                let name_slice =
                    unsafe { core::slice::from_raw_parts(arg0 as *const u8, arg1 as usize) };
                let name =
                    core::str::from_utf8(name_slice).map_err(|_| IOError::FileSystemError)?;
                self.install_async_driver(name, TaskID::new(arg2), arg3);
                Ok(0)
            }
            _ => Err(IOError::UnsupportedCommand),
        }
    }
}

#[repr(u32)]
pub enum ConfigurationCommands {
    InstallDevice = 1,
}
