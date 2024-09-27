use idos_api::io::error::IOError;

use crate::task::messaging::Message;

pub type IOResult = Result<u32, IOError>;

#[derive(Copy, Clone, Debug)]
pub enum DriverIOAction {
    /// Open(ptr to path string, path string length)
    Open(u32, u32),
    /// Open a handle to the driver itself, with no path.
    /// The argument provides a way to embed a unique instance identifier
    /// without using a string path -- this is commonly used by async device
    /// drivers which run multiple instances from a single Task.
    OpenRaw(u32),
    /// Close(instance)
    Close(u32),
    /// Read(instance, buffer pointer, buffer length)
    Read(u32, u32, u32),
    /// Write(instance, buffer pointer, buffer length)
    Write(u32, u32, u32),
    /// Seek(instance, method, offset)
    Seek(u32, u32, u32),
    /// Stat(instance, buffer pointer, buffer length)
    Stat(u32, u32, u32),
}

impl DriverIOAction {
    pub fn encode_to_message(&self, request_id: u32) -> Message {
        match self {
            Self::Open(path_ptr, path_len) => {
                Message {
                    message_type: DriverCommand::Open as u32,
                    unique_id: request_id,
                    args: [*path_ptr, *path_len, 0, 0, 0, 0],
                }
            },
            Self::OpenRaw(id) => {
                Message {
                    message_type: DriverCommand::OpenRaw as u32,
                    unique_id: request_id,
                    args: [*id, 0, 0, 0, 0, 0],
                }
            },
            Self::Close(id) => {
                Message {
                    message_type: DriverCommand::Close as u32,
                    unique_id: request_id,
                    args: [*id, 0, 0, 0, 0, 0],
                }
            },
            Self::Read(open_instance, buffer_ptr, buffer_len) => {
                Message {
                    message_type: DriverCommand::Read as u32,
                    unique_id: request_id,
                    args: [*open_instance, 0, *buffer_ptr, *buffer_len, 0, 0],
                }
            },
            Self::Write(open_instance, buffer_ptr, buffer_len) => {
                Message {
                    message_type: DriverCommand::Write as u32,
                    unique_id: request_id,
                    args: [*open_instance, 0, *buffer_ptr, *buffer_len, 0, 0],
                }
            },
            Self::Seek(open_instance, method, offset) => {
                Message {
                    message_type: DriverCommand::Seek as u32,
                    unique_id: request_id,
                    args: [*open_instance, *method, *offset, 0, 0, 0],
                }
            },
            Self::Stat(open_instance, buffer_ptr, buffer_len) => {
                Message {
                    message_type: DriverCommand::Stat as u32,
                    unique_id: request_id,
                    args: [*open_instance, *buffer_ptr, *buffer_len, 0, 0, 0],
                }
            },
        }
    }
}

#[repr(u32)]
pub enum DriverCommand {
    Open = 1,
    OpenRaw,
    Read,
    Write,
    Close,
    Seek,
    Stat,
    // Every time a new command is added, modify the method below that decodes the command

    Invalid = 0xffffffff,
}

impl DriverCommand {
    pub fn from_u32(code: u32) -> DriverCommand {
        // UPDATE THIS NUMBER when new commands are added
        //                      V
        if code >= 1 && code <= 7 {
            unsafe { core::mem::transmute(code) }
        } else {
            DriverCommand::Invalid
        }
    }
}

/// Messages from drivers to the Driver IO system will send this magic u32 in
/// their first message payload. That will tell the kernel that this is a valid
/// driver message and not just noise.
pub const DRIVER_RESPONSE_MAGIC: u32 = 0x00534552; // "RES\0"
