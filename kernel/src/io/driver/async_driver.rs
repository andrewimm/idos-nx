use alloc::string::ToString;
use idos_api::io::error::IOError;
use crate::{task::messaging::Message, files::stat::FileStatus};
use super::comms::{DriverCommand, DRIVER_RESPONSE_MAGIC, IOResult};

/// Trait implemented by all async drivers. It provides a helper method to
/// translate incoming messages from the DriverIO system into file IO method
/// calls.
///
/// TODO: This should eventually get moved out into the SDK.
pub trait AsyncDriver {
    fn handle_request(&mut self, message: Message) -> Option<Message> {
        let driver_result: Option<IOResult> = match DriverCommand::from_u32(message.message_type) {
            DriverCommand::Open => {
                let path_ptr = message.args[0] as *const u8;
                let path_len = message.args[1] as usize;
                let path = if path_len == 0 {
                    ""
                } else {
                    let path_slice = unsafe {
                        core::slice::from_raw_parts(path_ptr, path_len)
                    };
                    core::str::from_utf8(path_slice).ok()?
                };
                Some(self.open(path))
            },
            DriverCommand::OpenRaw => {
                let id_as_path = message.args[0].to_string();
                Some(self.open(id_as_path.as_str()))
            },
            DriverCommand::Close => {
                let instance = message.args[0];
                Some(self.close(instance))
            },
            DriverCommand::Read => {
                let instance = message.args[0];
                let _offset = message.args[1];
                let buffer_ptr = message.args[2] as *mut u8;
                let buffer_len = message.args[3] as usize;
                let buffer = unsafe {
                    core::slice::from_raw_parts_mut(buffer_ptr, buffer_len)
                };
                Some(self.read(instance, buffer))
            },
            DriverCommand::Stat => {
                let instance = message.args[0];
                let struct_ptr = message.args[1] as *mut FileStatus;
                let struct_len = message.args[2] as usize;
                if struct_len != core::mem::size_of::<FileStatus>() {
                    // invalid size?
                    return None;
                }
                let status_struct = unsafe { &mut *struct_ptr };

                Some(self.stat(instance, status_struct))
            },
            _ => {
                crate::kprintln!("Async driver: Unknown Request");
                None
            },
        };
        match driver_result {
            Some(Ok(result)) => {
                let code = result & 0x7fffffff;
                Some(
                    Message {
                        message_type: DRIVER_RESPONSE_MAGIC,
                        unique_id: message.unique_id,
                        args: [code, 0, 0, 0, 0, 0],
                    }
                )
            },
            Some(Err(err)) => {
                let code = Into::<u32>::into(err) | 0x80000000;
                Some(
                    Message {
                        message_type: DRIVER_RESPONSE_MAGIC,
                        unique_id: message.unique_id,
                        args: [code, 0, 0, 0, 0, 0],
                    }
                )
            },
            None => None,
        }
    }

    fn open(&mut self, path: &str) -> IOResult;

    fn close(&mut self, instance: u32) -> IOResult;

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> IOResult;

    fn stat(&mut self, instance: u32, status_struct: &mut FileStatus) -> IOResult {
        Err(IOError::UnsupportedOperation)
    }
}
