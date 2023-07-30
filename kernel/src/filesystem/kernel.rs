use crate::files::cursor::SeekMethod;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::files::stat::FileStatus;
use crate::io::IOError;

pub trait KernelFileSystem {
    #![allow(unused_variables)]

    fn open(&self, path: Path) -> Result<DriverHandle, IOError>;

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<u32, IOError>;

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<u32, IOError>;

    fn close(&self, handle: DriverHandle) -> Result<(), IOError>;

    fn seek(&self, handle: DriverHandle, offset: SeekMethod) -> Result<u32, IOError>;

    fn stat(&self, handle: DriverHandle) -> Result<FileStatus, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn dup(&self, handle: DriverHandle, dup_into: Option<u32>) -> Result<DriverHandle, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn configure(&self, command: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }
}
