extern crate idos_api;

use idos_api::ipc::Message;
use idos_api::io::cursor::SeekMethod;

pub trait AsyncDriver {
    fn handle_request(&mut self, message: Message) -> Option<Message> {
        match message.0 {
            1 => { // Open
                let path_str_start = message.1 as *const u8;
                let path_str_len = message.2 as usize;
                let path = if path_str_len == 0 {
                    ""
                } else {
                    let path_slice = unsafe {
                        core::slice::from_raw_parts(path_str_start, path_str_len)
                    };
                    core::str::from_utf8(path_slice).ok()?
                };
                match self.open(path) {
                    Ok(handle) => Some((handle, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            },
            2 => { // OpenRaw
                if message.1 > 9 {
                    // 
                }
                let id: [u8; 1] = [message.1 as u8 + 0x30];
                let id_as_path = unsafe { core::str::from_utf8_unchecked(&id) };
                match self.open(id_as_path) {
                    Ok(handle) => Some((handle, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            },
            3 => { // Read
                let open_instance = message.1;
                let buffer_start = message.2 as *mut u8;
                let buffer_len = message.3 as usize;
                let buffer = unsafe {
                    core::slice::from_raw_parts_mut(buffer_start, buffer_len)
                };
                match self.read(open_instance, buffer) {
                    Ok(written) => Some((written, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            },
            4 => { // Write
                let open_instance = message.1;
                let buffer_start = message.2 as *mut u8;
                let buffer_len = message.3 as usize;
                let buffer = unsafe {
                    core::slice::from_raw_parts(buffer_start, buffer_len)
                };
                match self.write(open_instance, buffer) {
                    Ok(written) => Some((written, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            },
            5 => { // Close
                let handle = message.1 as u32;
                match self.close(handle) {
                    Ok(_) => Some((0, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            },
            6 => {
                let open_instance = message.1;
                let method = message.2;
                let delta = message.3;
                let offset = SeekMethod::decode(method, delta).unwrap();
                match self.seek(open_instance, offset) {
                    Ok(new_position) => Some((new_position, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            },
            7 => {
                let open_instance = message.1;
                let buffer_start = message.2 as *mut FileStatus;
                // assuming the length is the size of a file status
                // not sure if that's a good idea or not
                let status = unsafe { &mut *buffer_start };
                match self.stat(open_instance, status) {
                    Ok(_) => Some((0, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            },
            8 => {
                let open_instance = message.1;
                let dup_into = if message.2 == 0xffffffff {
                    None
                } else {
                    Some(message.2)
                };
                match self.dup(open_instance, dup_into) {
                    Ok(new_handle) => Some((new_handle, 0, 0)),
                    Err(err) => Some((0, err as u32, 0)),
                }
            },
            _ => {
                None
            },
        }.map(|(a, b, c)| Message(0x00524553, a, b, c))
    }

    fn open(&mut self, path: &str) -> Result<u32, IOError>;

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> Result<u32, IOError>;

    fn write(&mut self, instance: u32, buffer: &[u8]) -> Result<u32, IOError>;

    fn close(&mut self, handle: u32) -> Result<(), IOError>;

    fn seek(&mut self, instance: u32, offset: SeekMethod) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn stat(&mut self, instance: u32, status: &mut FileStatus) -> Result<(), IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn dup(&mut self, instance: u32, dup_into: Option<u32>) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }
}

#[repr(u32)]
pub enum IOError {
    // No enum value should be backed by a value of 0

    /// An error occurred within the file system
    FileSystemError = 1,
    /// A File or Directory with the given path does not exist
    NotFound,
    /// The file handle used for IO is not currently open
    FileHandleInvalid,
    /// The file handle used for IO is not the correct type for that operation
    FileHandleWrongType,
    /// An IO operation failed
    OperationFailed,
    /// Attempted a FS method that isn't supported by the driver
    UnsupportedOperation,
    /// Sent a control command that was not recognized
    UnsupportedCommand,

    Unknown = 0xffffffff,
}

impl TryFrom<u32> for IOError {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::FileSystemError),
            2 => Ok(Self::NotFound),
            3 => Ok(Self::FileHandleInvalid),
            4 => Ok(Self::FileHandleWrongType),
            5 => Ok(Self::OperationFailed),
            6 => Ok(Self::UnsupportedOperation),
            7 => Ok(Self::UnsupportedCommand),
            _ => Ok(Self::Unknown),
        }
    }
}

pub struct FileStatus {
    /// Size of the file, in bytes
    pub byte_size: u32,
    /// Bitmap indicating the type of the file
    pub file_type: u32,
    /// ID of the drive this file is found on
    pub drive_id: u32,
    //pub modification_time: u32,
}

impl FileStatus {
    pub fn new() -> Self {
        Self {
            byte_size: 0,
            file_type: 0,
            drive_id: 0,
        }
    }
}
