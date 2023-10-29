use alloc::boxed::Box;
use idos_api::io::error::IOError;

use crate::{task::id::TaskID, files::path::Path};

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

pub trait SyncDriver {
    #![allow(unused_variables)]

    fn open(&self, path: Path) -> IOResult;

    fn read(&self, instance: u32, buffer: &mut [u8]) -> IOResult;
}

pub type IOResult = Result<u32, IOError>;

