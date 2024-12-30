use super::super::arbiter::{begin_io, AsyncIO};
use super::super::kernel::KernelFileSystem;
use crate::files::cursor::SeekMethod;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::files::stat::FileStatus;
use crate::io::IOError;
use crate::memory::shared::SharedMemoryRange;
use crate::task::id::TaskID;
use crate::task::messaging::Message;
use alloc::string::ToString;
use alloc::sync::Arc;
use spin::Mutex;

/// The AsyncFileSystem is a stub indicating that all requests need to be sent
/// to an out-of-kernel driver, through the fs arbiter.
pub struct AsyncFileSystem {
    task: TaskID,
}

impl AsyncFileSystem {
    pub const fn new(task: TaskID) -> Self {
        Self { task }
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
    fn async_op(&self, request: AsyncIO) -> Option<Result<u32, u32>> {
        let response: Arc<Mutex<Option<Result<u32, u32>>>> = Arc::new(Mutex::new(None));

        // send the request
        begin_io(self.task, request, response.clone());

        match Arc::try_unwrap(response) {
            Ok(inner) => *inner.lock(),
            Err(_) => None,
        }
    }
}

impl KernelFileSystem for AsyncFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, IOError> {
        let path_str = path.as_str();
        let path_slice = path_str.as_bytes();

        let response = if path_slice.len() == 0 {
            // can't share memory for an empty slice, just special case it
            self.async_op(AsyncIO::Open(0, 0))
        } else {
            let shared_range = SharedMemoryRange::for_slice::<u8>(path_slice);
            let shared_to_driver = shared_range.share_with_task(self.task);

            self.async_op(AsyncIO::Open(
                shared_to_driver.get_range_start(),
                shared_to_driver.range_length,
            ))
        };

        unwrap_async_response(response).map(|handle| DriverHandle(handle))
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<u32, IOError> {
        let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
        let shared_to_driver = shared_range.share_with_task(self.task);

        let response = self.async_op(AsyncIO::Read(
            handle.into(),
            shared_to_driver.get_range_start(),
            shared_to_driver.range_length,
        ));

        unwrap_async_response(response)
    }

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<u32, IOError> {
        let shared_range = SharedMemoryRange::for_slice::<u8>(buffer);
        let shared_to_driver = shared_range.share_with_task(self.task);

        let response = self.async_op(AsyncIO::Write(
            handle.into(),
            shared_to_driver.get_range_start(),
            shared_to_driver.range_length,
        ));

        unwrap_async_response(response)
    }

    fn close(&self, handle: DriverHandle) -> Result<(), IOError> {
        let response = self.async_op(AsyncIO::Close(handle.into()));

        unwrap_async_response(response).map(|_| ())
    }

    fn seek(&self, handle: DriverHandle, offset: SeekMethod) -> Result<u32, IOError> {
        let (method, delta) = offset.encode();
        let response = self.async_op(AsyncIO::Seek(handle.into(), method, delta));

        unwrap_async_response(response)
    }

    fn stat(&self, handle: DriverHandle) -> Result<FileStatus, IOError> {
        let status = FileStatus::new();
        let shared_range = SharedMemoryRange::for_struct(&status);
        let shared_to_driver = shared_range.share_with_task(self.task);

        let response = self.async_op(AsyncIO::Stat(
            handle.into(),
            shared_to_driver.get_range_start(),
            shared_to_driver.range_length,
        ));

        match response {
            Some(Ok(_)) => Ok(status),
            Some(Err(err)) => Err(IOError::try_from(err).unwrap()),
            None => Err(IOError::FileSystemError),
        }
    }

    fn dup(&self, handle: DriverHandle, dup_into: Option<u32>) -> Result<DriverHandle, IOError> {
        let dup_into_encoded = match dup_into {
            Some(value) => value,
            None => 0xffffffff,
        };
        let response = self.async_op(AsyncIO::Dup(handle.into(), dup_into_encoded));
        unwrap_async_response(response).map(|handle| DriverHandle(handle))
    }
}

fn unwrap_async_response(response: Option<Result<u32, u32>>) -> Result<u32, IOError> {
    match response {
        Some(Ok(res)) => Ok(res),
        Some(Err(err)) => Err(IOError::try_from(err).unwrap()),
        None => Err(IOError::FileSystemError),
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
    Seek,
    Stat,
    Dup,
    // Every time a new command is added, modify the From<u32> impl below
    Invalid = 0xffffffff,
}

impl From<u32> for AsyncCommand {
    fn from(value: u32) -> Self {
        if value >= 1 && value <= 8 {
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
            Message {
                message_type: code,
                unique_id: 0,
                args: [path_str_start, path_str_len, 0, 0, 0, 0],
            }
        }
        AsyncIO::OpenRaw(id) => {
            let code = AsyncCommand::OpenRaw as u32;
            Message {
                message_type: code,
                unique_id: 0,
                args: [id, 0, 0, 0, 0, 0],
            }
        }
        AsyncIO::Read(open_instance, buffer_start, buffer_len) => {
            let code = AsyncCommand::Read as u32;
            Message {
                message_type: code,
                unique_id: 0,
                args: [open_instance, buffer_start, buffer_len, 0, 0, 0],
            }
        }
        AsyncIO::Write(open_instance, buffer_start, buffer_len) => {
            let code = AsyncCommand::Write as u32;
            Message {
                message_type: code,
                unique_id: 0,
                args: [open_instance, buffer_start, buffer_len, 0, 0, 0],
            }
        }
        AsyncIO::Close(handle) => {
            let code = AsyncCommand::Close as u32;
            Message {
                message_type: code,
                unique_id: 0,
                args: [handle, 0, 0, 0, 0, 0],
            }
        }
        AsyncIO::Seek(open_instance, method, delta) => {
            let code = AsyncCommand::Seek as u32;
            Message {
                message_type: code,
                unique_id: 0,
                args: [open_instance, method, delta, 0, 0, 0],
            }
        }
        AsyncIO::Stat(open_instance, buffer_start, buffer_len) => {
            let code = AsyncCommand::Stat as u32;
            Message {
                message_type: code,
                unique_id: 0,
                args: [open_instance, buffer_start, buffer_len, 0, 0, 0],
            }
        }
        AsyncIO::Dup(open_instance, dup_into) => {
            let code = AsyncCommand::Dup as u32;
            Message {
                message_type: code,
                unique_id: 0,
                args: [open_instance, dup_into, 0, 0, 0, 0],
            }
        }
    }
}

pub trait AsyncDriver {
    fn handle_request(&mut self, message: Message) -> Option<Message> {
        match AsyncCommand::from(message.message_type) {
            AsyncCommand::Open => {
                let path_str_start = message.args[0] as *const u8;
                let path_str_len = message.args[1] as usize;
                let path = if path_str_len == 0 {
                    ""
                } else {
                    let path_slice =
                        unsafe { core::slice::from_raw_parts(path_str_start, path_str_len) };
                    core::str::from_utf8(path_slice).ok()?
                };
                match self.open(path) {
                    Ok(handle) => Some((handle, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            }
            AsyncCommand::OpenRaw => {
                let id_as_path = message.args[0].to_string();
                match self.open(id_as_path.as_str()) {
                    Ok(handle) => Some((handle, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            }
            AsyncCommand::Read => {
                let open_instance = message.args[0];
                let buffer_start = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_start, buffer_len) };
                match self.read(open_instance, buffer) {
                    Ok(written) => Some((written, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            }
            AsyncCommand::Write => {
                let open_instance = message.args[0];
                let buffer_start = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                let buffer = unsafe { core::slice::from_raw_parts(buffer_start, buffer_len) };
                match self.write(open_instance, buffer) {
                    Ok(written) => Some((written, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            }
            AsyncCommand::Close => {
                let handle = message.args[0] as u32;
                match self.close(handle) {
                    Ok(_) => Some((0, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            }
            AsyncCommand::Seek => {
                let open_instance = message.args[0];
                let method = message.args[1];
                let delta = message.args[2];
                let offset = SeekMethod::decode(method, delta).unwrap();
                match self.seek(open_instance, offset) {
                    Ok(new_position) => Some((new_position, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            }
            AsyncCommand::Stat => {
                let open_instance = message.args[0];
                let buffer_start = message.args[1] as *mut FileStatus;
                // assuming the length is the size of a file status
                // not sure if that's a good idea or not
                let status = unsafe { &mut *buffer_start };
                match self.stat(open_instance, status) {
                    Ok(_) => Some((0, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            }
            AsyncCommand::Dup => {
                let open_instance = message.args[0];
                let dup_into = if message.args[1] == 0xffffffff {
                    None
                } else {
                    Some(message.args[2])
                };
                match self.dup(open_instance, dup_into) {
                    Ok(new_handle) => Some((new_handle, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            }
            _ => {
                crate::kprint!("Async driver: unknown request\n");
                None
            }
        }
        .map(|(a, b, c)| Message {
            message_type: ASYNC_RESPONSE_MAGIC,
            unique_id: 0,
            args: [a, b, c, 0, 0, 0],
        })
    }

    fn open(&mut self, path: &str) -> Result<u32, IOError>;

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> Result<u32, IOError>;

    fn write(&mut self, instance: u32, buffer: &[u8]) -> Result<u32, IOError>;

    fn close(&mut self, handle: u32) -> Result<(), IOError>;

    #[allow(unused_variables)]
    fn seek(&mut self, instance: u32, offset: SeekMethod) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    #[allow(unused_variables)]
    fn stat(&mut self, instance: u32, status: &mut FileStatus) -> Result<(), IOError> {
        Err(IOError::UnsupportedOperation)
    }

    #[allow(unused_variables)]
    fn dup(&mut self, instance: u32, dup_into: Option<u32>) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }
}
