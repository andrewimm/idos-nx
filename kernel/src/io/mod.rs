extern crate idos_api;

pub mod async_io;
pub mod devices;
pub mod driver_io;
pub mod filesystem;
pub mod handle;
pub mod provider;

pub use idos_api::io::cursor::SeekMethod;
pub use idos_api::io::error::IOError;

use alloc::boxed::Box;
use crate::task::actions::lifecycle::create_kernel_task;

pub fn init_async_io_system() {

    #[cfg(test)]
    {
        let test_sync_fs = self::filesystem::testing::sync_fs::TestSyncFS::new();
        self::filesystem::install_sync_fs("TEST", Box::new(test_sync_fs));
    }

    let null_dev = self::devices::null::NullDev::new();
    self::filesystem::install_sync_dev("NULL", Box::new(null_dev));

    let zero_dev = self::devices::zero::ZeroDev::new();
    self::filesystem::install_sync_dev("ZERO", Box::new(zero_dev));

    create_kernel_task(self::driver_io::driver_io_task, Some("DRIVER IO"));
}
