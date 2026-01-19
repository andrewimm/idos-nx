use crate::io::error::{IoError, IoResult};
use crate::io::file::FileStatus;
use crate::ipc::Message;

/// DriverCommand is an enum shared between the kernel and user-space drivers,
/// used to encode / decode messages sent to Async IO drivers.
#[repr(u32)]
pub enum DriverCommand {
    Open = 1,
    OpenRaw,
    Read,
    Write,
    Close,
    Stat,
    Share,
    Ioctl,
    // Every time a new command is added, modify the method below that decodes the command
    Invalid = 0xffffffff,
}

impl DriverCommand {
    pub fn from_u32(code: u32) -> DriverCommand {
        match code {
            1 => DriverCommand::Open,
            2 => DriverCommand::OpenRaw,
            3 => DriverCommand::Read,
            4 => DriverCommand::Write,
            5 => DriverCommand::Close,
            6 => DriverCommand::Stat,
            7 => DriverCommand::Share,
            8 => DriverCommand::Ioctl,
            _ => DriverCommand::Invalid,
        }
    }
}

/// Trait implemented by all async drivers. It provides a helper method to
/// translate incoming messages from the DriverIO system into file IO method
/// calls.
///
/// TODO: This should eventually get moved out into the SDK.
#[allow(unused_variables)]
pub trait AsyncDriver {
    // Overridable helper method to release buffers after use.
    fn release_buffer(&mut self, buffer_ptr: *mut u8, buffer_len: usize);

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
                // Convert to str without allocation:
                // 10 digits should be enough for any u32, and we can just skip
                // leading zeros
                let mut digits: [u8; 10] = [0; 10];
                let mut digit_index: usize = 10;
                let mut remaining = message.args[0];
                if remaining == 0 {
                    digits[9] = b'0';
                    digit_index = 9;
                }
                while remaining > 0 && digit_index > 0 {
                    digit_index -= 1;
                    digits[digit_index] = (remaining % 10) as u8 + b'0';
                    remaining /= 10;
                }

                let id_as_path = core::str::from_utf8(&digits[digit_index..]).ok()?;
                Some(self.open(id_as_path))
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
            DriverCommand::Invalid => Some(Err(IoError::UnsupportedCommand)),
        }
    }

    fn open(&mut self, path: &str) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    fn close(&mut self, instance: u32) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8], offset: u32) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    fn write(&mut self, instance: u32, buffer: &[u8], offset: u32) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    fn share(&mut self, instance: u32, transfer_to_id: u32, is_move: bool) -> IoResult {
        Err(IoError::UnsupportedOperation)
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
