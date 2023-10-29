use core::sync::atomic::{AtomicU32, Ordering};
use alloc::collections::BTreeMap;
use idos_api::io::error::IOError;
use spin::RwLock;

use crate::files::path::Path;

use super::driver::{SyncDriver, IOResult};

pub struct TestSyncFS {
    next_instance: AtomicU32,
    open_files: RwLock<BTreeMap<u32, OpenFile>>,
}

struct OpenFile {
    written: usize,
}

impl OpenFile {
    pub fn new() -> Self {
        Self {
            written: 0,
        }
    }
}

impl TestSyncFS {
    pub fn new() -> Self {
        Self {
            next_instance: AtomicU32::new(1),
            open_files: RwLock::new(BTreeMap::new()),
        }
    }
}

impl SyncDriver for TestSyncFS {
    fn open(&self, path: Path) -> IOResult {
        crate::kprintln!("TEST FS OPEN \"{}\"", path.as_str());
        if path.as_str() == "MYFILE.TXT" {
            let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
            self.open_files.write().insert(instance, OpenFile::new());
            Ok(instance)
        } else {
            Err(IOError::NotFound)
        }
    }

    fn read(&self, instance: u32, buffer: &mut [u8]) -> IOResult {
        let mut open_files = self.open_files.write();
        let found = open_files.get_mut(&instance).ok_or(IOError::FileHandleInvalid)?;
        for i in 0..buffer.len() {
            let value = ((found.written + i) % 26) + 0x41;
            buffer[i] = value as u8;
        }
        found.written += buffer.len();
        Ok(buffer.len() as u32)
    }
}
