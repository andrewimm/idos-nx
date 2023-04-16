//! Device drivers can be exposed as files under the DEV: drive
//! This is an extension of DOS's globally available device names like COM,
//! LPI, etc. Now, every device can be given a filename without polluting the
//! global namespace.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use crate::devices::{DeviceDriver, SyncDriverType};
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::filesystem::kernel::KernelFileSystem;
use crate::task::id::TaskID;
use spin::RwLock;

pub struct DevFileSystem {
    map: RwLock<BTreeMap<String, DeviceDriver>>,
}

impl DevFileSystem {
    pub const fn new() -> Self {
        Self {
            map: RwLock::new(BTreeMap::new()),
        }
    }

    fn install(&self, name: &str, driver: DeviceDriver) {
        let key = name.to_string();
        let mut map = self.map.write();
        if map.contains_key(&key) {
            // should probably error out
            return;
        }
        map.insert(key, driver);
    }

    pub fn install_sync_driver(&self, name: &str, driver: Arc<Box<SyncDriverType>>) {
        self.install(name, DeviceDriver::SyncDriver(driver));
    }

    pub fn install_async_driver(&self, name: &str, driver_id: TaskID) {
        self.install(name, DeviceDriver::AsyncDriver(driver_id));
    }
    
    fn get_driver(&self, name: String) -> Option<DeviceDriver> {
        self.map.read().get(&name).cloned()
    }
}

impl KernelFileSystem for DevFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, ()> {
        crate::kprint!("  Open Device {}\n", path.as_str());
        match self.get_driver(path.into()).ok_or(())? {
            DeviceDriver::SyncDriver(driver) => {
                // TODO: store the id from the open() call, map it by a DriverHandl
                driver.open().map(|_| DriverHandle(1))
            },
            DeviceDriver::AsyncDriver(id) => {
                // send the command to the async driver
                Err(())
            },
        }
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<usize, ()> {
        Err(())
    }

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<usize, ()> {
        Err(())
    }

    fn close(&self, handle: DriverHandle) -> Result<(), ()> {
        Err(())
    }
}

