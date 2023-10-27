extern crate idos_api;

pub mod async_io;
pub mod driver_io;
pub mod filesystem;
pub mod handle;
pub mod provider;

pub use idos_api::io::cursor::SeekMethod;
pub use idos_api::io::error::IOError;

use crate::task::actions::lifecycle::create_kernel_task;

pub fn init_async_io_system() {
    create_kernel_task(self::driver_io::driver_io_task, Some("DRIVER IO"));
}
