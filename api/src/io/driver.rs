use core::str::Utf8Error;

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
    CreateMapping,
    RemoveMapping,
    PageInMapping,
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
            9 => DriverCommand::CreateMapping,
            10 => DriverCommand::RemoveMapping,
            11 => DriverCommand::PageInMapping,
            _ => DriverCommand::Invalid,
        }
    }
}

/// Newtype wrapper for a driver's unique internal identifier for an open file.
/// Opening a file returns one of these, and it is used to re-reference that
/// file up until closing.
#[repr(transparent)]
pub struct DriverFileReference(u32);

impl DriverFileReference {
    pub fn new(id: u32) -> Self {
        DriverFileReference(id)
    }
}

impl core::ops::Deref for DriverFileReference {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Newtype wrapper for a token returned by the kernel when mapping a file into
/// memory. When the kernel needs to establish a file mapping, it sends a request
/// to the driver, similar to opening a file. On success, the driver returns a
/// token. Future page faults use this token for requests to fill a frame.
/// Because the token handling is driver-specific, it allows the driver to
/// maintain a single token for each file, so that different tasks can map to
/// the same file without needing to re-open it each time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct DriverMappingToken(u32);

impl DriverMappingToken {
    pub fn new(id: u32) -> Self {
        DriverMappingToken(id)
    }
}

impl core::ops::Deref for DriverMappingToken {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn number_to_utf8_bytes(num: u32, digits: &mut [u8; 10]) -> Result<&str, Utf8Error> {
    let mut digit_index: usize = 10;
    let mut remaining = num;
    if remaining == 0 {
        digits[9] = b'0';
        digit_index = 9;
    }
    while remaining > 0 && digit_index > 0 {
        digit_index -= 1;
        digits[digit_index] = (remaining % 10) as u8 + b'0';
        remaining /= 10;
    }

    core::str::from_utf8(&digits[digit_index..])
}

/// Trait implemented by all async drivers. It provides a helper method to
/// translate incoming messages from the DriverIO system into file IO method
/// calls.
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
                Some(self.open(path).map(|file_ref| *file_ref))
            }
            DriverCommand::OpenRaw => {
                // Convert to str without allocation:
                // 10 digits should be enough for any u32, and we can just skip
                // leading zeros
                let mut digits: [u8; 10] = [0; 10];
                let id_as_path = number_to_utf8_bytes(message.args[0], &mut digits).ok()?;
                Some(self.open(id_as_path).map(|file_ref| *file_ref))
            }
            DriverCommand::Close => {
                let file_ref = DriverFileReference(message.args[0]);
                Some(self.close(file_ref))
            }
            DriverCommand::Read => {
                let file_ref = DriverFileReference(message.args[0]);
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                let offset = message.args[3];
                let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
                let result = self.read(file_ref, buffer, offset);
                self.release_buffer(buffer_ptr, buffer_len);
                Some(result)
            }
            DriverCommand::Write => {
                let file_ref = DriverFileReference(message.args[0]);
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                let offset = message.args[3];
                let buffer = unsafe { core::slice::from_raw_parts(buffer_ptr, buffer_len) };
                let result = self.write(file_ref, buffer, offset);
                self.release_buffer(buffer_ptr, buffer_len);
                Some(result)
            }
            DriverCommand::Share => {
                let file_ref = DriverFileReference(message.args[0]);
                let transfer_to_id = message.args[1];
                let result = self.share(file_ref, transfer_to_id, message.args[2] != 0);
                Some(result)
            }
            DriverCommand::Stat => {
                let file_ref = DriverFileReference(message.args[0]);
                let struct_ptr = message.args[1] as *mut FileStatus;
                let struct_len = message.args[2] as usize;
                if struct_len != core::mem::size_of::<FileStatus>() {
                    // invalid size?
                    self.release_buffer(struct_ptr as *mut u8, struct_len);
                    return None;
                }
                let status_struct = unsafe { &mut *struct_ptr };

                let result = self.stat(file_ref, status_struct);
                self.release_buffer(struct_ptr as *mut u8, struct_len);
                Some(result)
            }
            DriverCommand::Ioctl => {
                let file_ref = DriverFileReference(message.args[0]);
                let ioctl = message.args[1];
                let arg = message.args[2];
                let arg_len = message.args[3] as usize;
                if arg_len != 0 {
                    // attempt to interpret arg as pointer to struct
                    let result = self.ioctl_struct(file_ref, ioctl, arg as *mut u8, arg_len);
                    self.release_buffer(arg as *mut u8, arg_len);
                    Some(result)
                } else {
                    // assume arg is just a u32 value
                    let result = self.ioctl(file_ref, ioctl, arg);
                    Some(result)
                }
            }
            DriverCommand::CreateMapping => {
                let path_ptr = message.args[0] as *mut u8;
                let path_len = message.args[1] as usize;
                let mut possible_sub_driver_path: [u8; 10] = [0; 10];
                let path = if path_len == 0 {
                    // length == 0 could mean empty string or sub-driver
                    // if the path ptr is 0xffff_ffff, treat it as empty string
                    if message.args[0] == 0xffff_ffff {
                        ""
                    } else {
                        // else, we stringify the u32 as the path (possible_sub_driver_path)
                        number_to_utf8_bytes(message.args[0], &mut possible_sub_driver_path).ok()?
                    }
                } else {
                    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
                    core::str::from_utf8(path_slice).ok()?
                };
                Some(self.create_mapping(path).map(|map_token| *map_token))
            }
            DriverCommand::RemoveMapping => {
                let map_token = DriverMappingToken(message.args[0]);
                Some(self.remove_mapping(map_token))
            }
            DriverCommand::PageInMapping => {
                let map_token = DriverMappingToken(message.args[0]);
                let offset = message.args[1];
                let frame_paddr = message.args[2];
                Some(self.page_in_mapping(map_token, offset, frame_paddr))
            }
            DriverCommand::Invalid => Some(Err(IoError::UnsupportedCommand)),
        }
    }

    /// Open a file by path. The path is an opaque string interpreted by the
    /// driver, and can be used to specify sub-resources within the driver. For
    /// example, a driver for a disk might interpret paths as file paths within
    /// the disk, while a driver for a USB controller might interpret paths as
    /// USB device identifiers.
    /// On success, the driver returns a DriverFileReference, which is used for
    /// all IO operations on that file until it is closed.
    /// On failure, the driver returns an IoError.
    fn open(&mut self, path: &str) -> IoResult<DriverFileReference> {
        Err(IoError::UnsupportedOperation)
    }

    /// Create a memory mapping for a file. When a task requests to map a file
    /// into memory, the kernel sends this request to the driver, which can
    /// allow or deny the mapping and perform any initialization.
    /// On success, the driver returns a DriverMapToken, which is used by the
    /// kernel to fill frames on page faults.
    /// On failure, the driver returns an IoError.
    fn create_mapping(&mut self, path: &str) -> IoResult<DriverMappingToken> {
        Err(IoError::UnsupportedOperation)
    }

    /// Close a file reference, indicating that it is no longer needed. The
    /// driver can use this as a hint to release resources associated with that
    /// file. After closing, the file reference is no longer valid, but nothing
    /// prevents the driver from reusing the same reference for a future open.
    /// On failure, the driver returns an IoError.
    fn close(&mut self, file_ref: DriverFileReference) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    /// Remove a memory mapping, indicating that it is no longer needed. The
    /// driver can use this as a hint to release resources associated with that
    /// mapping. After removing, the map token is no longer valid, but nothing
    /// prevents the driver from reusing the same token for a future mapping.
    /// On failure, the driver returns an IoError.
    fn remove_mapping(&mut self, map_token: DriverMappingToken) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    /// Read from a file reference into a buffer. The driver fills the buffer
    /// with data from the file, starting at the given offset.
    /// On success, the driver returns the number of bytes read, which may be
    /// less than the buffer size if the end of the file is reached.
    /// On failure, the driver returns an IoError.
    fn read(&mut self, file_ref: DriverFileReference, buffer: &mut [u8], offset: u32) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    /// Write to a file reference from a buffer. The driver writes data from the
    /// buffer to the file, starting at the given offset.
    /// On success, the driver returns the number of bytes written, which may be
    /// less than the buffer size if there is no more available space or the
    /// file is not writable.
    /// On failure, the driver returns an IoError.
    fn write(&mut self, file_ref: DriverFileReference, buffer: &[u8], offset: u32) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    /// Share a file reference with another task. This is used for inter-process
    /// communication, allowing a file reference to be transferred from one task
    /// to another. The driver can use this as a hint to allow multiple tasks to
    /// access the same file reference, or to deny sharing if it is not
    /// supported. If is_move is true, the file reference is transferred and
    /// should no longer be used by the sender after sharing. If is_move is
    /// false, the file reference is shared and can still be used by the sender
    /// after sharing.
    /// The default behavior is just to return success, because not all drivers
    /// need to do anything special to support sharing.
    fn share(
        &mut self,
        file_ref: DriverFileReference,
        transfer_to_id: u32,
        is_move: bool,
    ) -> IoResult {
        Ok(1)
    }

    /// Get the status of a file reference. The driver fills the provided
    /// `FileStatus` struct with information about the file, such as its size
    /// and timestamps.
    fn stat(&mut self, file_ref: DriverFileReference, status_struct: &mut FileStatus) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    /// Fill a page frame for a memory mapping. When a page fault occurs on a
    /// memory mapping, the kernel sends this request to the driver, with the
    /// map token provided when the mapping was created, the offset within the
    /// file, and a pointer to the page frame that needs to be filled. The
    /// driver is responsible for filling a 4096-byte page frame with the
    /// appropriate data for that offset within the mapping. The kernel does
    /// not map the page frame into the driver's address space, so the driver
    /// must perform any direct memory-mapping necessary to access the
    /// underlying frame.
    /// On success, the driver returns the number of bytes filled in the page
    /// frame. If the offset is beyond the end of the file or there is no more
    /// data to fill, the driver can return a value less than 4096, and the
    /// kernel will zero-fill the rest of the page.
    /// On failure, the driver returns an IoError, and the kernel will handle
    /// the page fault appropriately.
    fn page_in_mapping(
        &mut self,
        map_token: DriverMappingToken,
        offset_in_file: u32,
        frame_paddr: u32,
    ) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    /// Perform a driver-specific control operation on a file reference. The
    /// ioctl code and argument are interpreted by the driver, allowing for
    /// arbitrary operations that don't fit into the standard read/write/close
    /// model. This method handles IOCTLs where the argument is a simple u32
    /// value.
    fn ioctl(&mut self, file_ref: DriverFileReference, ioctl: u32, arg: u32) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }

    /// Perform a driver-specific control operation on a file reference, where
    /// the argument is a pointer to a struct. The driver can read and write to
    /// the struct as needed for the operation. This method allows for more
    /// complex IOCTL operations that require more data than can fit in a simple
    /// u32 argument. The driver is responsible for validating the struct size
    /// using the `arg_len` parameter, and should return an error if the size is
    /// incorrect.
    fn ioctl_struct(
        &mut self,
        file_ref: DriverFileReference,
        ioctl: u32,
        arg_ptr: *mut u8,
        arg_len: usize,
    ) -> IoResult {
        Err(IoError::UnsupportedOperation)
    }
}
