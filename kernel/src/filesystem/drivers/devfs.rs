//! Device drivers can be exposed as files under the DEV: drive
//! This is an extension of DOS's globally available device names like COM,
//! LPI, etc. Now, every device can be given a filename without polluting the
//! global namespace.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use crate::collections::SlotList;
use crate::devices::{DeviceDriver, SyncDriverType};
use crate::files::cursor::SeekMethod;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::filesystem::arbiter::{AsyncIO, begin_io};
use crate::filesystem::kernel::KernelFileSystem;
use crate::memory::shared::SharedMemoryRange;
use crate::task::id::TaskID;
use spin::{RwLock, Mutex};

pub struct DevFileSystem {
    /// Stores the actual device driver enums
    installed_drivers: RwLock<SlotList<DeviceDriver>>,
    /// Maps a device name to the index in the installed_drivers list
    drivers_by_name: RwLock<BTreeMap<String, usize>>,
    /// Map an open file to the driver and file instance it references
    open_handles: RwLock<SlotList<(usize, u32)>>,
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
    
    fn get_driver_by_name(&self, name: String) -> Option<(usize, DeviceDriver)> {
        let index: usize = self.drivers_by_name.read().get(&name).copied()?;
        self.installed_drivers.read().get(index).cloned()
            .map(|driver| (index, driver))
    }

    fn run_driver_operation<F, T>(&self, handle: DriverHandle, op: F) -> Result<T, ()>
        where F: FnOnce(DeviceDriver, u32) -> Result<T, ()> {
        let list_index: usize = handle.into();
        let (driver_index, open_instance) = self.open_handles
            .read()
            .get(list_index)
            .copied()
            .ok_or(())?;
        let driver = self.installed_drivers
            .read()
            .get(driver_index)
            .cloned()
            .ok_or(())?;
        op(driver, open_instance)
    }

    fn async_op(&self, task: TaskID, request: AsyncIO) -> Option<u32> {
        
        let response: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));

        // send the request
        begin_io(task, request, response.clone());

        match Arc::try_unwrap(response) {
            Ok(inner) => *inner.lock(),
            Err(_) => None,
        }
    }
}

impl KernelFileSystem for DevFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, ()> {
        crate::kprint!("  Open Device {}\n", path.as_str());
        let (driver_index, driver) = self.get_driver_by_name(path.into()).ok_or(())?;
        match driver {
            DeviceDriver::SyncDriver(driver) => {
                let open_instance: u32 = driver.open()?;
                let handle = self.open_handles.write().insert((driver_index, open_instance));
                Ok(DriverHandle(handle as u32))
            },
            DeviceDriver::AsyncDriver(id, sub_id) => {
                // send the command to the async driver
                let open_instance = self.async_op(
                    id,
                    AsyncIO::OpenRaw(sub_id),
                ).ok_or(())?;

                let handle = self.open_handles.write().insert((driver_index, open_instance));
                Ok(DriverHandle(handle as u32))
            },
        }
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<usize, ()> {
        self.run_driver_operation(
            handle,
            |driver, open_instance| {
                match driver {
                    DeviceDriver::SyncDriver(driver) => {
                        driver.read(open_instance, buffer)
                    },
                    DeviceDriver::AsyncDriver(id, _) => {
                        let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
                        let shared_to_driver = shared_range.share_with_task(id);

                        let response = self.async_op(
                            id,
                            AsyncIO::Read(
                                open_instance,
                                shared_to_driver.get_range_start(),
                                shared_to_driver.range_length,
                            )
                        );

                        match response {
                            Some(count) => Ok(count as usize),
                            None => Err(()),
                        }
                    },
                }
            },
        )
    }

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<usize, ()> {
        self.run_driver_operation(
            handle,
            |driver, open_instance| {
                match driver {
                    DeviceDriver::SyncDriver(driver) => {
                        driver.write(open_instance, buffer)
                    },
                    DeviceDriver::AsyncDriver(id, _) => {
                        let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
                        let shared_to_driver = shared_range.share_with_task(id);

                        let response = self.async_op(
                            id,
                            AsyncIO::Write(
                                open_instance,
                                shared_to_driver.get_range_start(),
                                shared_to_driver.range_length,
                            )
                        );

                        match response {
                            Some(count) => Ok(count as usize),
                            None => Err(()),
                        }
                    },
                }
            },
        )
    }

    fn close(&self, handle: DriverHandle) -> Result<(), ()> {
        let list_index: usize = handle.into();
        let (driver_index, open_instance) = self.open_handles
            .write()
            .remove(list_index)
            .ok_or(())?;
        let driver = self.installed_drivers
            .read()
            .get(driver_index)
            .cloned()
            .ok_or(())?;
        match driver {
            DeviceDriver::SyncDriver(driver) => {
                driver.close(open_instance)
            },
            DeviceDriver::AsyncDriver(id, _) => {
                let response = self.async_op(
                    id,
                    AsyncIO::Close(open_instance),
                );
                match response {
                    Some(_) => Ok(()),
                    None => Err(()),
                }
            },
        }
    }

    fn seek(&self, handle: DriverHandle, offset: SeekMethod) -> Result<usize, ()> {
        self.run_driver_operation(
            handle,
            |driver, open_instance| {
                match driver {
                    DeviceDriver::SyncDriver(driver) => {
                        driver.seek(open_instance, offset)
                    },
                    DeviceDriver::AsyncDriver(id, _) => {
                        let (method, delta) = offset.encode();
                        let response = self.async_op(
                            id,
                            AsyncIO::Seek(
                                open_instance,
                                method,
                                delta,
                            )
                        );

                        match response {
                            Some(count) => Ok(count as usize),
                            None => Err(()),
                        }
                    },
                }
            },
        )
    }

    fn configure(&self, command: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) -> Result<u32, ()> {
        match command {
            1 => {
                // Install device driver with TaskID of `arg2`
                // arg3 will be used as the driver sub-id, for drivers that
                // service multiple device names with a single Task.
                // Assume arg0 and arg1 are the pointer and length of a string
                // containing the device name
                let name_slice = unsafe {
                    core::slice::from_raw_parts(
                        arg0 as *const u8,
                        arg1 as usize,
                    )
                };
                let name = core::str::from_utf8(name_slice).map_err(|_| ())?;
                self.install_async_driver(name, TaskID::new(arg2), arg3);
                Ok(0)
            },
            _ => Err(()),
        }
    }
}

#[repr(u32)]
pub enum ConfigurationCommands {
    InstallDevice = 1,
}

