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
}

impl DriverIOAction {
    pub fn encode_to_message(&self, request_id: u32) -> Message {
        match self {
            Self::Open(path_ptr, path_len) => {
                let code = create_request_code(request_id, DriverCommand::Open);
                Message(code, *path_ptr, *path_len, 0)
            },
            Self::OpenRaw(id) => {
                let code = create_request_code(request_id, DriverCommand::OpenRaw);
                Message(code, *id, 0, 0)
            },
            Self::Close(id) => {
                let code = create_request_code(request_id, DriverCommand::Close);
                Message(code, *id, 0, 0)
            },
            Self::Read(open_instance, buffer_ptr, buffer_len) => {
                let code = create_request_code(request_id, DriverCommand::Read);
                Message(code, *open_instance, *buffer_ptr, *buffer_len)
            },
            Self::Write(open_instance, buffer_ptr, buffer_len) => {
                let code = create_request_code(request_id, DriverCommand::Write);
                Message(code, *open_instance, *buffer_ptr, *buffer_len)
            },
        }
    }
}

fn create_request_code(id: u32, command: DriverCommand) -> u32 {
    let high = (command as u32) << 24;
    high | id
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

pub fn decode_command_and_id(code: u32) -> (DriverCommand, u32) {
    let high = code >> 24;
    let id = code & 0x00ffffff;

    let command: DriverCommand = if high >= 1 && high <= 8 /* update this number when new commands are added */ {
        unsafe { core::mem::transmute(high) }
    } else {
        DriverCommand::Invalid
    };

    (command, id)

}

/// Messages from drivers to the Driver IO system will send this magic u32 in
/// their first message payload. That will tell the kernel that this is a valid
/// driver message and not just noise.
pub const DRIVER_RESPONSE_MAGIC: u32 = 0x00534552; // "RES\0"
