use idos_api::io::error::IOError;

use crate::files::path::Path;

use super::driver::{SyncDriver, IOResult};

pub struct TestSyncFS {
}

impl TestSyncFS {
    pub fn new() -> Self {
        Self {
        }
    }
}

impl SyncDriver for TestSyncFS {
    fn open(&self, path: Path) -> IOResult {
        crate::kprintln!("TEST FS OPEN \"{}\"", path.as_str());
        if path.as_str() == "MYFILE.TXT" {
            Ok(1)
        } else {
            Err(IOError::NotFound)
        }
    }

    fn read(&self, instance: usize, buffer: &mut [u8]) -> IOResult {
        Err(IOError::UnsupportedOperation)
        
    }
}
