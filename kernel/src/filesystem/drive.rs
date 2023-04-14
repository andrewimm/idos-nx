//! A Drive is the unique name by which each mounted filesystem is referenced.
//! It appears at the beginning of an absolute filepath, followed by a colon.
//! On DOS (and CP/M before it), 26 single-letter drive names are supported.
//! To expand upon this, the OS supports drive names with up to eight
//! case-insensitive letters. This allows longer, descriptive names for the
//! virtual filesystems like DEV:
//! Only the single-letter drives will be accessible to DOS programs, so
//! physical disks will be assigned to those unless otherwise specified.
//!
//! Drive letters are assigned using the same logic as MS-DOS:
//! 1) A: is assigned to the first floppy drive.
//! 2) B: is assigned to the second floppy drive. If none is present, it is
//!    mapped to a virtual drive that uses the same hardware as A:
//!    This allows copying from one floppy to another with only a single
//!    physical drive. It will read the source into memory, and then prompt the
//!    user to insert the second disk into the drive before continuing with the
//!    copy.
//! 3) Drive letters, starting with C:, are assigned to the primary partitions
//!    of all hard disks.
//! 4) For each hard disk, drive letters are assigned for all remaining
//!    partitions.
//! 5) After all hard disks and partitions have been assigned, letters are
//!    assigned to any drivers initialized at boot time.
//! 6) Dynamic volumes are assigned remaining letters if they are mounted after
//!    boot time.

use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::task::id::TaskID;
use spin::RwLock;
use super::drivers::asyncfs::AsyncFileSystem;
use super::driver::FileSystemDriver;
use super::kernel::KernelFileSystem;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct DriveID(pub usize);

pub struct DriveMap {
    next_id: AtomicUsize,
    map: RwLock<BTreeMap<DriveID, (String, FileSystemDriver)>>,
}

impl DriveMap {
    pub const fn new() -> Self {
        Self {
            next_id: AtomicUsize::new(0),
            map: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn install(&self, name: &str, driver: Box<dyn KernelFileSystem + Sync + Send>) -> DriveID {
        let id = DriveID(self.next_id.fetch_add(1, Ordering::SeqCst));
        self.map.write().insert(id, (name.to_string(), FileSystemDriver::new_sync(driver)));
        crate::kprint!("Installed FS \"{}:\" as {:?}\n", name, id);
        id
    }

    pub fn install_async(&self, name: &str, task_id: TaskID) -> DriveID {
        let async_fs = Box::new(AsyncFileSystem::new(task_id));
        self.install(name, async_fs)
    }

    pub fn get_id_by_name(&self, name: &str) -> Option<DriveID> {
        for (id, (drive_name, _)) in self.map.read().iter() {
            if drive_name.as_str() == name {
                return Some(*id);
            }
        }
        None
    }

    pub fn get_driver(&self, id: DriveID) -> Option<FileSystemDriver> {
        self.map.read().get(&id).map(|(_, fs)| {
            crate::kprint!("    ACCESS FS {:?}\n", id);
            fs.clone()
        })
    }
}

