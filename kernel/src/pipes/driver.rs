use alloc::boxed::Box;
use idos_api::io::error::IOError;
use spin::Once;

use crate::io::filesystem::driver::DriverID;
use crate::io::driver::kernel_driver::KernelDriver;
use crate::files::path::Path;
use crate::io::filesystem::driver::AsyncIOCallback;
use crate::io::driver::comms::IOResult;

struct PipeFileSystem {
}

impl KernelDriver for PipeFileSystem {
    fn open(&self, _path: Option<Path>, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn read(&self, instance: u32, buffer: &mut [u8], io_callback: AsyncIOCallback) -> Option<IOResult> {
        None
    }
}

pub static PIPE_DRIVER_ID: Once<DriverID> = Once::new();

pub fn install() {
    PIPE_DRIVER_ID.call_once(|| {
        crate::io::filesystem::install_kernel_fs("", Box::new(PipeFileSystem {}))
    });
}

pub fn get_pipe_drive_id() -> DriverID {
    *PIPE_DRIVER_ID.get().expect("PIPE FS not initialized")
}

