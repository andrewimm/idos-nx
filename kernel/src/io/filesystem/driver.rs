use alloc::boxed::Box;

use crate::{task::id::TaskID, io::{driver::sync_driver::SyncDriver, async_io::AsyncOpID}};

#[derive(Copy, Clone)]
pub struct DriverID(u32);

impl DriverID {
    pub fn new(index: u32) -> Self {
        Self(index)
    }
}

impl core::ops::Deref for DriverID {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub type InstalledDriver = Box<dyn SyncDriver + Sync + Send>;

pub enum DriverType {
    SyncDevice(InstalledDriver),
    AsyncDevice(TaskID),
    SyncFilesystem(InstalledDriver),
    AsyncFilesystem(TaskID),
}

pub type AsyncIOCallback = (TaskID, u32, AsyncOpID);
