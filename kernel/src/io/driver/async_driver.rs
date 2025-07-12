use super::comms::{DriverCommand, IOResult};
use crate::{
    files::stat::FileStatus,
    memory::{address::VirtualAddress, shared::release_buffer},
    task::messaging::Message,
};
use alloc::string::ToString;
use idos_api::io::error::IOError;

/// Trait implemented by all async drivers. It provides a helper method to
/// translate incoming messages from the DriverIO system into file IO method
/// calls.
///
/// TODO: This should eventually get moved out into the SDK.
#[allow(unused_variables)]
pub trait AsyncDriver {
    fn handle_request(&mut self, message: Message) -> Option<IOResult> {
        // TODO: We should probably add some queueing so that multiple ops to the
        // same handle don't cause any conflict
        match DriverCommand::from_u32(message.message_type) {
            DriverCommand::Open => {
                let path_ptr = message.args[0] as *const u8;
                let path_len = message.args[1] as usize;
                let path = if path_len == 0 {
                    ""
                } else {
                    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
                    core::str::from_utf8(path_slice).ok()?
                };
                Some(self.open(path))
            }
            DriverCommand::OpenRaw => {
                let id_as_path = message.args[0].to_string();
                Some(self.open(id_as_path.as_str()))
            }
            DriverCommand::Close => {
                let instance = message.args[0];
                Some(self.close(instance))
            }
            DriverCommand::Read => {
                let instance = message.args[0];
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                let offset = message.args[3];
                let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
                let result = self.read(instance, buffer, offset);
                release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
                Some(result)
            }
            DriverCommand::Stat => {
                let instance = message.args[0];
                let struct_ptr = message.args[1] as *mut FileStatus;
                let struct_len = message.args[2] as usize;
                if struct_len != core::mem::size_of::<FileStatus>() {
                    // invalid size?
                    release_buffer(VirtualAddress::new(struct_ptr as u32), struct_len);
                    return None;
                }
                let status_struct = unsafe { &mut *struct_ptr };

                let result = self.stat(instance, status_struct);
                release_buffer(VirtualAddress::new(struct_ptr as u32), struct_len);
                Some(result)
            }
            _ => {
                crate::kprintln!("Async driver: Unknown Request");
                None
            }
        }
    }

    fn open(&mut self, path: &str) -> IOResult;

    fn close(&mut self, instance: u32) -> IOResult;

    fn read(&mut self, instance: u32, buffer: &mut [u8], offset: u32) -> IOResult;

    fn stat(&mut self, instance: u32, status_struct: &mut FileStatus) -> IOResult {
        Err(IOError::UnsupportedOperation)
    }
}
