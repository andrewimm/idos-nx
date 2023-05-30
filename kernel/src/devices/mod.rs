pub mod zero;

use alloc::boxed::Box;
use alloc::sync::Arc;
use crate::task::id::TaskID;

pub trait SyncDriver {
    fn open(&self) -> Result<u32, ()>;
    fn read(&self, index: u32, buffer: &mut [u8]) -> Result<usize, ()>;
    fn write(&self, index: u32, buffer: &[u8]) -> Result<usize, ()>;
    fn close(&self, index: u32) -> Result<(), ()>;
}

pub type SyncDriverType = dyn SyncDriver + Sync + Send;

#[derive(Clone)]
pub enum DeviceDriver {
    AsyncDriver(TaskID, u32),
    SyncDriver(Arc<Box<SyncDriverType>>),
}
