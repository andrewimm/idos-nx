use alloc::string::ToString;
use crate::task::messaging::Message;
use super::comms::{DriverCommand, DRIVER_RESPONSE_MAGIC, IOResult, decode_command_and_id};

/// Trait implemented by all async drivers. It provides a helper method to
/// translate incoming messages from the DriverIO system into file IO method
/// calls.
///
/// TODO: This should eventually get moved out into the SDK.
pub trait AsyncDriver {
    fn handle_request(&mut self, message: Message) -> Option<Message> {
        let (command, request_id) = decode_command_and_id(message.0);
        let driver_result: Option<IOResult> = match command {
            DriverCommand::Open => {
                let path_ptr = message.1 as *const u8;
                let path_len = message.2 as usize;
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
                let id_as_path = message.1.to_string();
                Some(self.open(id_as_path.as_str()))
            },
            DriverCommand::Read => {
                let instance = message.1;
                let buffer_ptr = message.2 as *mut u8;
                let buffer_len = message.3 as usize;
                let buffer = unsafe {
                    core::slice::from_raw_parts_mut(buffer_ptr, buffer_len)
                };
                Some(self.read(instance, buffer))
            },
            _ => {
                crate::kprintln!("Async driver: Unknown Request");
                None
            },
        };
        match driver_result {
            Some(Ok(result)) => {
                let code = result & 0x7fffffff;
                Some(Message(DRIVER_RESPONSE_MAGIC, request_id, code, 0))
            },
            Some(Err(err)) => {
                let code = Into::<u32>::into(err) | 0x80000000;
                Some(Message(DRIVER_RESPONSE_MAGIC, request_id, code, 0))
            },
            None => None,
        }
    }

    fn open(&mut self, path: &str) -> IOResult;

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> IOResult;
}
