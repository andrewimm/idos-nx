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
    /// Read(instance, buffer pointer, buffer length, starting offset)
    Read(u32, u32, u32, u32),
    /// Write(instance, buffer pointer, buffer length, starting offset)
    Write(u32, u32, u32, u32),
    /// Stat(instance, buffer pointer, buffer length)
    Stat(u32, u32, u32),
    /// Transfer(instance, dest task id, is move)
    Share(u32, u32, u32),
}

impl DriverIOAction {
    pub fn encode_to_message(&self, request_id: u32) -> Message {
        match self {
            Self::Open(path_ptr, path_len) => Message {
                message_type: DriverCommand::Open as u32,
                unique_id: request_id,
                args: [*path_ptr, *path_len, 0, 0, 0, 0],
            },
            Self::OpenRaw(id) => Message {
                message_type: DriverCommand::OpenRaw as u32,
                unique_id: request_id,
                args: [*id, 0, 0, 0, 0, 0],
            },
            Self::Close(id) => Message {
                message_type: DriverCommand::Close as u32,
                unique_id: request_id,
                args: [*id, 0, 0, 0, 0, 0],
            },
            Self::Read(open_instance, buffer_ptr, buffer_len, offset) => Message {
                message_type: DriverCommand::Read as u32,
                unique_id: request_id,
                args: [*open_instance, *buffer_ptr, *buffer_len, *offset, 0, 0],
            },
            Self::Write(open_instance, buffer_ptr, buffer_len, offset) => Message {
                message_type: DriverCommand::Write as u32,
                unique_id: request_id,
                args: [*open_instance, *buffer_ptr, *buffer_len, *offset, 0, 0],
            },
            Self::Stat(open_instance, buffer_ptr, buffer_len) => Message {
                message_type: DriverCommand::Stat as u32,
                unique_id: request_id,
                args: [*open_instance, *buffer_ptr, *buffer_len, 0, 0, 0],
            },
            Self::Share(open_instance, transfer_to, is_move) => Message {
                message_type: DriverCommand::Share as u32,
                unique_id: request_id,
                args: [*open_instance, *transfer_to, *is_move, 0, 0, 0],
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
    Stat,
    Share,
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
