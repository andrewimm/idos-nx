use idos_api::io::error::IOError;

use crate::{
    files::{path::Path, stat::FileStatus},
    io::filesystem::driver::AsyncIOCallback,
};

use super::comms::IOResult;

/// Kernel Driver methods execute immediately, but may not complete
/// synchronously. If the data is available by the time the method finishes, it
/// will return `Some(IOResult)`, and the kernel will immediately complete the
/// Op that started this request. If the data will be available later, the
/// method returns None, and uses the IOCallback info
pub trait KernelDriver {
    #![allow(unused_variables)]

    fn open(&self, path: Option<Path>, io_callback: AsyncIOCallback) -> Option<IOResult>;

    fn close(&self, instance: u32, io_callback: AsyncIOCallback) -> Option<IOResult>;

    fn read(
        &self,
        instance: u32,
        buffer: &mut [u8],
        offset: u32,
        io_callback: AsyncIOCallback,
    ) -> Option<IOResult>;

    fn write(
        &self,
        instance: u32,
        buffer: &[u8],
        offset: u32,
        io_callback: AsyncIOCallback,
    ) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn stat(
        &self,
        instance: u32,
        file_status: &mut FileStatus,
        io_callback: AsyncIOCallback,
    ) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }
}
