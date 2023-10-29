pub mod driver;
#[cfg(test)]
pub mod testing;

use core::sync::atomic::{AtomicU32, Ordering};

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use idos_api::io::error::IOError;
use spin::RwLock;

use crate::files::path::Path;
use crate::task::id::TaskID;

use self::driver::{DriverID, IOResult};
use self::driver::DriverType;
use self::driver::InstalledDriver;

use super::async_io::AsyncOp;

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

/// Run the open() operation on an installed driver
pub fn driver_open(id: DriverID, path: Path) -> Option<IOResult> {
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
        _ => panic!("Not implemented"),
    }
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
        _ => panic!("Not implemented"),
    }
}

