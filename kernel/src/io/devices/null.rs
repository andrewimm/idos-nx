use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::files::path::Path;
use crate::io::driver::comms::IOResult;
use crate::io::driver::kernel_driver::KernelDriver;
use crate::io::filesystem::driver::AsyncIOCallback;
use idos_api::io::error::IOError;
use spin::RwLock;

pub struct NullDev {
    next_instance: AtomicU32,
    open_files: RwLock<BTreeMap<u32, OpenFile>>,
}

struct OpenFile {
}

impl OpenFile {
    pub fn new() -> Self {
        Self {
        }
    }
}

impl NullDev {
    pub fn new() -> Self {
        Self {
            next_instance: AtomicU32::new(1),
            open_files: RwLock::new(BTreeMap::new()),
        }
    }

    fn open_impl(&self) -> IOResult {
        let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
        self.open_files.write().insert(instance, OpenFile::new());
        Ok(instance)
    }

    fn read_impl(&self, instance: u32, buffer: &mut [u8]) -> IOResult {
        let mut open_files = self.open_files.write();
        let _found = open_files.get_mut(&instance).ok_or(IOError::FileHandleInvalid)?;
        Ok(0)
    }

    fn close_impl(&self, instance: u32) -> IOResult {
        let mut open_files = self.open_files.write();
        open_files.remove(&instance).ok_or(IOError::FileHandleInvalid)?;
        Ok(0)
    }
}

impl KernelDriver for NullDev {
    fn open(&self, _path: Option<Path>, _: AsyncIOCallback) -> Option<IOResult> {
        Some(self.open_impl())
    }

    fn read(&self, instance: u32, buffer: &mut [u8], _: AsyncIOCallback) -> Option<IOResult> {
        Some(self.read_impl(instance, buffer))
    }

    fn close(&self, instance: u32, _: AsyncIOCallback) -> Option<IOResult> {
        Some(self.close_impl(instance))
    }
}
