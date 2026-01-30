use idos_api::io::driver::DriverMappingToken;
use idos_api::io::error::{IoError, IoResult};
use idos_api::io::file::FileStatus;

use crate::{files::path::Path, io::filesystem::driver::AsyncIOCallback, task::id::TaskID};

/// Kernel Driver methods execute immediately, but may not complete
/// synchronously. If the data is available by the time the method finishes, it
/// will return `Some(IoResult)`, and the kernel will immediately complete the
/// Op that started this request. If the data will be available later, the
/// method returns None, and uses the IOCallback info
pub trait KernelDriver {
    #![allow(unused_variables)]

    fn open(&self, path: Option<Path>, io_callback: AsyncIOCallback) -> Option<IoResult>;

    fn close(&self, instance: u32, io_callback: AsyncIOCallback) -> Option<IoResult>;

    fn read(
        &self,
        instance: u32,
        buffer: &mut [u8],
        offset: u32,
        io_callback: AsyncIOCallback,
    ) -> Option<IoResult>;

    fn write(
        &self,
        instance: u32,
        buffer: &[u8],
        offset: u32,
        io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        Some(Err(IoError::UnsupportedOperation))
    }

    fn stat(
        &self,
        instance: u32,
        file_status: &mut FileStatus,
        io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        Some(Err(IoError::UnsupportedOperation))
    }

    fn share(
        &self,
        instance: u32,
        target_task_id: TaskID,
        is_move: bool,
        io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        // default behavior is to return Ok, since it is assumed sharing is
        // safe unless drivers have special internal behavior
        Some(Ok(1))
    }

    fn ioctl(
        &self,
        instance: u32,
        ioctl: u32,
        arg: u32,
        arg_len: usize,
        io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        Some(Err(IoError::UnsupportedOperation))
    }

    fn create_mapping(&self, path: &str) -> Option<IoResult<DriverMappingToken>> {
        Some(Err(IoError::UnsupportedOperation))
    }

    fn remove_mapping(&self, map_token: DriverMappingToken) -> Option<IoResult> {
        Some(Err(IoError::UnsupportedOperation))
    }

    fn page_in_mapping(
        &self,
        map_token: DriverMappingToken,
        offset_in_file: u32,
        frame_paddr: u32,
    ) -> Option<IoResult> {
        Some(Err(IoError::UnsupportedOperation))
    }
}
