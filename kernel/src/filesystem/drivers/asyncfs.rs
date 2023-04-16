use alloc::sync::Arc;
use spin::Mutex;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::memory::shared::SharedMemoryRange;
use crate::task::id::TaskID;
use crate::task::messaging::Message;
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
    fn async_op(&self, request: AsyncIO) -> Option<u32> {
        
        let mut response: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));

        // send the request
        begin_io(self.task, request, response.clone());

        match Arc::try_unwrap(response) {
            Ok(inner) => *inner.lock(),
            Err(_) => None,
        }
    }
}

impl KernelFileSystem for AsyncFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, ()> {
        let path_str = path.as_str();
        let path_slice = path_str.as_bytes();

        let shared_range = SharedMemoryRange::for_slice::<u8>(path_slice);
        let shared_to_driver = shared_range.share_with_task(self.task);

        let response = self.async_op(
            AsyncIO::Open(
                shared_to_driver.get_range_start(),
                shared_to_driver.range_length,
            )
        );

        // TODO: clean up shared range

        match response {
            Some(handle) => Ok(DriverHandle(handle)),
            None => Err(()),
        }
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<usize, ()> {
        let response = self.async_op(
            AsyncIO::Read,
        );

        match response {
            Some(count) => Ok(count as usize),
            None => Err(()),
        }
        
    }
}

// Below are the resources used by async fs implementations

#[repr(u32)]
pub enum AsyncCommand {
    Open = 1,
    Read,
    Write,
    Close,

    Invalid = 0xffffffff,
}

impl From<u32> for AsyncCommand {
    fn from(value: u32) -> Self {
        if value >= 1 && value <= 4 {
            unsafe { core::mem::transmute(value) }
        } else {
            AsyncCommand::Invalid
        }
    }
}

pub static ASYNC_RESPONSE_MAGIC: u32 = 0x00524553; // "\0RES"

pub fn encode_request(request: AsyncIO) -> Message {
    match request {
        AsyncIO::Open(path_str_start, path_str_len) => {
            let code = AsyncCommand::Open as u32;
            Message(code, path_str_start, path_str_len, 0)
        },
        AsyncIO::Read => {
            let code = AsyncCommand::Read as u32;
            let buffer_start = 0;
            let buffer_len = 0;
            Message(code, buffer_start, buffer_len, 0)
        },
        _ => panic!("Unsupported async io type"),
    }
}

