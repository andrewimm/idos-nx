pub mod zero;

use alloc::boxed::Box;
use alloc::sync::Arc;
use crate::files::cursor::SeekMethod;
use crate::io::IOError;
use crate::task::id::TaskID;

pub trait SyncDriver {
    fn open(&self) -> Result<u32, IOError>;
    fn read(&self, index: u32, buffer: &mut [u8]) -> Result<u32, IOError>;
    fn write(&self, index: u32, buffer: &[u8]) -> Result<u32, IOError>;
    fn close(&self, index: u32) -> Result<(), IOError>;
    fn seek(&self, index: u32, offset: SeekMethod) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }
    fn dup(&self, index: u32, dup_into: Option<u32>) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }
}

pub type SyncDriverType = dyn SyncDriver + Sync + Send;

#[derive(Clone)]
pub enum DeviceDriver {
    AsyncDriver(TaskID, u32),
    SyncDriver(Arc<Box<SyncDriverType>>),
}
