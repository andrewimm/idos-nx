use alloc::string::ToString;
use idos_api::io::driver::DriverCommand;
use idos_api::io::error::{IoError, IoResult};
use idos_api::io::file::FileStatus;
use idos_api::ipc::Message;

/// Trait implemented by all async drivers. It provides a helper method to
/// translate incoming messages from the DriverIO system into file IO method
/// calls.
///
/// TODO: This should eventually get moved out into the SDK.
#[allow(unused_variables)]
pub trait AsyncDriver {
    // Overridable helper method to release buffers after use.
    fn release_buffer(&mut self, buffer_ptr: *mut u8, buffer_len: usize) {
        use crate::memory::{address::VirtualAddress, shared::release_buffer};
        release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
    }

    fn handle_request(&mut self, message: Message) -> Option<IoResult> {
        match DriverCommand::from_u32(message.message_type) {
            DriverCommand::Open => {
                let path_ptr = message.args[0] as *mut u8;
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
                self.release_buffer(buffer_ptr, buffer_len);
                Some(result)
            }
            DriverCommand::Write => {
                let instance = message.args[0];
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                let offset = message.args[3];
                let buffer = unsafe { core::slice::from_raw_parts(buffer_ptr, buffer_len) };
                let result = self.write(instance, buffer, offset);
                self.release_buffer(buffer_ptr, buffer_len);
                Some(result)
            }
            DriverCommand::Share => {
                let instance = message.args[0];
                let transfer_to_id = message.args[1];
                let result = self.share(instance, transfer_to_id, message.args[2] != 0);
                Some(result)
            }
            DriverCommand::Stat => {
                let instance = message.args[0];
                let struct_ptr = message.args[1] as *mut FileStatus;
                let struct_len = message.args[2] as usize;
                if struct_len != core::mem::size_of::<FileStatus>() {
                    // invalid size?
                    self.release_buffer(struct_ptr as *mut u8, struct_len);
                    return None;
                }
                let status_struct = unsafe { &mut *struct_ptr };

                let result = self.stat(instance, status_struct);
                self.release_buffer(struct_ptr as *mut u8, struct_len);
                Some(result)
            }
            DriverCommand::Ioctl => {
                let instance = message.args[0];
                let ioctl = message.args[1];
                let arg = message.args[2];
                let arg_len = message.args[3] as usize;
                if arg_len != 0 {
                    // attempt to interpret arg as pointer to struct
                    let result = self.ioctl_struct(instance, ioctl, arg as *mut u8, arg_len);
                    self.release_buffer(arg as *mut u8, arg_len);
                    Some(result)
                } else {
                    // assume arg is just a u32 value
                    let result = self.ioctl(instance, ioctl, arg);
                    Some(result)
                }
            }
            DriverCommand::Invalid => {
                crate::kprintln!("Async driver: Unknown Request");
                None
            }
        }
    }

    fn open(&mut self, path: &str) -> IoResult;

    fn close(&mut self, instance: u32) -> IoResult;

    fn read(&mut self, instance: u32, buffer: &mut [u8], offset: u32) -> IoResult;

    fn write(&mut self, instance: u32, buffer: &[u8], offset: u32) -> IoResult;

    fn share(&mut self, instance: u32, transfer_to_id: u32, is_move: bool) -> IoResult {
        Ok(1)
    }

    fn stat(&mut self, instance: u32, status_struct: &mut FileStatus) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    fn ioctl(&mut self, instance: u32, ioctl: u32, arg: u32) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    fn ioctl_struct(
        &mut self,
        instance: u32,
        ioctl: u32,
        arg_ptr: *mut u8,
        arg_len: usize,
    ) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }
}
