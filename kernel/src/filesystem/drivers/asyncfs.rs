use alloc::boxed::Box;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::task::id::TaskID;
use super::super::arbiter::{begin_io, AsyncIO};
use super::super::kernel::KernelFileSystem;

/// The AsyncFileSystem is a stub indicating that all requests need to be sent
/// to an out-of-kernel driver, through the fs arbiter.
pub struct AsyncFileSystem {
    task: TaskID,
}

impl AsyncFileSystem {
    pub const fn new(task: TaskID) -> Self {
        Self {
            task,
        }
    }

    /// Perform an async IO operation
    /// The request body originates from one of the core File System calls.
    /// Once constructed, it is passed here, where it is placed on the Arbiter
    /// request queue. The Arbiter task is also woken up, in case it was
    /// dormant. The currently running task is immediately marked as blocked on
    /// an IO operation.
    /// The Arbiter will take repsonsibility for queuing up the request, and
    /// eventually passing it to the driver task. On completion, the Arbiter
    /// will wake the current task.
    fn async_op(&self, request: AsyncIO) -> Box<Option<u32>> {
        
        let mut response: Box<Option<u32>> = Box::new(None);

        // send the request
        begin_io(request);

        response.replace(1);

        response
    }
}

impl KernelFileSystem for AsyncFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, ()> {
        let path_str = path.as_str();
        let path_ptr = path_str.as_ptr();
        let path_size = path_str.len();

        let response = self.async_op(
            AsyncIO::Open,
        );

        match *response {
            Some(handle) => Ok(DriverHandle(handle)),
            None => Err(()),
        }
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<usize, ()> {
        let response = self.async_op(
            AsyncIO::Read,
        );

        match *response {
            Some(count) => Ok(count as usize),
            None => Err(()),
        }
        
    }
}
