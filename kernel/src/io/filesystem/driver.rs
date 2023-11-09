use alloc::boxed::Box;

use crate::{task::id::TaskID, io::{driver::kernel_driver::KernelDriver, async_io::AsyncOpID}};

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

pub type InstalledDriver = Box<dyn KernelDriver + Sync + Send>;

/// Depending on how a driver is structured, it may have one of many different
/// forms. DriverType allows the virtual filesystem to determine how to
/// interface with each driver.
pub enum DriverType {
    /// A Kernel Device is compiled into the kernel and executes requests
    /// immediately with no need for IPC/message passing. It is possible for a
    /// kernel device to complete asynchronously, but behavior that requires
    /// blocking should be done in a TaskDevice instead.
    KernelDevice(InstalledDriver),
    /// A Task Device runs an event loop in a standalone task. This allows the
    /// device to block without affecting the caller.
    /// The enum value stores a reference to the task's ID, as well as a
    /// sub-driver identifier. Depending on the design of the driver, a single
    /// task may be able to field requests for multiple devices. The sub-driver
    /// allows different filenames to map to the same driver task in a
    /// recognizable way.
    TaskDevice(TaskID, u32),
    /// Kernel FS is a filesystem driver compiled into the kernel
    KernelFilesystem(InstalledDriver),
    /// Task FS is a filesystem driver that runs in a standalone task
    TaskFilesystem(TaskID),
}

pub type AsyncIOCallback = (TaskID, u32, AsyncOpID);
