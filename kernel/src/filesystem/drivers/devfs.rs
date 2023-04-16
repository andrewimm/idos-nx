//! Device drivers can be exposed as files under the DEV: drive
//! This is an extension of DOS's globally available device names like COM,
//! LPI, etc. Now, every device can be given a filename without polluting the
//! global namespace.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::filesystem::kernel::KernelFileSystem;
use crate::task::id::TaskID;
use spin::RwLock;

pub struct DevFileSystem {
    map: RwLock<BTreeMap<String, TaskID>>,
}

impl DevFileSystem {
    pub const fn new() -> Self {
        Self {
            map: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn install_driver(&self, name: &str, driver_id: TaskID) {
        let key = name.to_string();
        let mut map = self.map.write();
        if map.contains_key(&key) {
            // should probably error out
            return;
        }
        map.insert(key, driver_id);
    }
    
    fn get_driver_id(&self, name: String) -> Option<TaskID> {
        self.map.read().get(&name).copied()
    }
}

impl KernelFileSystem for DevFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, ()> {
        crate::kprint!("  Open Device {}\n", path.as_str());
        let driver_id = self.get_driver_id(path.into())
            .ok_or(())?;
        Err(())
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

