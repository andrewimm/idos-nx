use core::sync::atomic::{AtomicU32, Ordering};
use crate::io::IOError;

use super::SyncDriver;

/// DEV:\\ZERO is a synchronous, in-kernel device that simply reads out zeroes
pub struct ZeroDriver {
    next_handle: AtomicU32,
}

impl ZeroDriver {
    pub const fn new() -> Self {
        Self {
            next_handle: AtomicU32::new(1),
        }
    }
}

impl SyncDriver for ZeroDriver {
    fn open(&self) -> Result<u32, IOError> {
        let handle = self.next_handle.fetch_add(1, Ordering::SeqCst);
        Ok(handle)
    }

    fn close(&self, _index: u32) -> Result<(),  IOError> {
        Ok(())
    }

    fn read(&self, _index: u32, buffer: &mut [u8]) -> Result<u32, IOError> {
        for i in 0..buffer.len() {
            buffer[i] = 0;
        }
        Ok(buffer.len() as u32)
    }

    fn write(&self, _index: u32, buffer: &[u8]) -> Result<u32, IOError> {
        Ok(buffer.len() as u32)
    }
}
