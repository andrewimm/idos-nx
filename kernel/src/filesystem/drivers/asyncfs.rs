use alloc::string::ToString;
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
        
        let response: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));

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
        let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
        let shared_to_driver = shared_range.share_with_task(self.task);

        let response = self.async_op(
            AsyncIO::Read(
                handle.into(),
                shared_to_driver.get_range_start(),
                shared_to_driver.range_length,
            )
        );

        match response {
            Some(count) => Ok(count as usize),
            None => Err(()),
        }
        
    }

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<usize, ()> {
        let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
        let shared_to_driver = shared_range.share_with_task(self.task);

        let response = self.async_op(
            AsyncIO::Write(
                handle.into(),
                shared_to_driver.get_range_start(),
                shared_to_driver.range_length,
            )
        );

        match response {
            Some(count) => Ok(count as usize),
            None => Err(()),
        }
    }
    
    fn close(&self, handle: DriverHandle) -> Result<(),  ()> {
        let response = self.async_op(
            AsyncIO::Close(handle.into())
        );
        
        match response {
            Some(_) => Ok(()),
            None => Err(()),
        }
    }
}

// Below are the resources used by async fs implementations

#[repr(u32)]
pub enum AsyncCommand {
    Open = 1,
    OpenRaw,
    Read,
    Write,
    Close,
    // Every time a new command is added, modify the From<u32> impl below

    Invalid = 0xffffffff,
}

impl From<u32> for AsyncCommand {
    fn from(value: u32) -> Self {
        if value >= 1 && value <= 5 {
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
        AsyncIO::OpenRaw(id) => {
            let code = AsyncCommand::OpenRaw as u32;
            Message(code, id, 0, 0)
        },
        AsyncIO::Read(open_instance, buffer_start, buffer_len) => {
            let code = AsyncCommand::Read as u32;
            Message(code, open_instance, buffer_start, buffer_len)
        },
        AsyncIO::Write(open_instance, buffer_start, buffer_len) => {
            let code = AsyncCommand::Write as u32;
            Message(code, open_instance, buffer_start, buffer_len)
        },
        AsyncIO::Close(handle) => {
            let code = AsyncCommand::Close as u32;
            Message(code, handle, 0, 0)
        },
    }
}

pub trait AsyncDriver {
    fn handle_request(&mut self, message: Message) -> Option<Message> {
        match AsyncCommand::from(message.0) {
            AsyncCommand::Open => {
                let path_str_start = message.1 as *const u8;
                let path_str_len = message.2 as usize;
                let path_slice = unsafe {
                    core::slice::from_raw_parts(path_str_start, path_str_len)
                };
                let path = core::str::from_utf8(path_slice).ok()?;
                let handle = self.open(path);
                Some((handle, 0, 0))
            },
            AsyncCommand::OpenRaw => {
                let id_as_path = message.1.to_string();
                let handle = self.open(id_as_path.as_str());
                Some((handle, 0, 0))
            },
            AsyncCommand::Read => {
                let open_instance = message.1;
                let buffer_start = message.2 as *mut u8;
                let buffer_len = message.3 as usize;
                let buffer = unsafe {
                    core::slice::from_raw_parts_mut(buffer_start, buffer_len)
                };
                let written = self.read(open_instance, buffer);
                Some((written, 0, 0))
            },
            AsyncCommand::Write => {
                let open_instance = message.1;
                let buffer_start = message.2 as *mut u8;
                let buffer_len = message.3 as usize;
                let buffer = unsafe {
                    core::slice::from_raw_parts(buffer_start, buffer_len)
                };
                let written = self.write(open_instance, buffer);
                Some((written, 0,  0))
            },
            AsyncCommand::Close => {
                let handle = message.1 as u32;
                self.close(handle);
                Some((0,  0,  0))
            },
            _ => {
                crate::kprint!("Async driver: unknown request\n");
                None
            },
        }.map(|(a, b, c)| Message(ASYNC_RESPONSE_MAGIC, a, b, c))
    }

    fn open(&mut self, path: &str) -> u32;

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> u32;

    fn write(&mut self, instance: u32, buffer: &[u8]) -> u32;

    fn close(&mut self, handle: u32);
}

