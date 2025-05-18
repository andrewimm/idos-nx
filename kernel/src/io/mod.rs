extern crate idos_api;

pub mod async_io;
pub mod devices;
pub mod driver;
pub mod filesystem;
pub mod handle;
pub mod provider;

pub use idos_api::io::error::IOError;

use alloc::boxed::Box;

pub fn init_async_io_system() {
    #[cfg(test)]
    {
        let test_sync_fs = self::filesystem::testing::sync_fs::TestSyncFS::new();
        self::filesystem::install_kernel_fs("TEST", Box::new(test_sync_fs));

        let async_fs_task = create_kernel_task(
            self::filesystem::testing::async_fs::driver_task,
            Some("TEST FS ASYNC"),
        );
        self::filesystem::install_task_fs("ATEST", async_fs_task);

        let async_dev_task = create_kernel_task(
            self::filesystem::testing::async_dev::driver_task,
            Some("TEST DEV ASYNC"),
        );
        self::filesystem::install_task_dev("ASYNCDEV", async_dev_task, 0);
    }

    let null_dev = self::devices::null::NullDev::new();
    self::filesystem::install_kernel_dev("NULL", Box::new(null_dev));

    let zero_dev = self::devices::zero::ZeroDev::new();
    self::filesystem::install_kernel_dev("ZERO", Box::new(zero_dev));

    let task_fs = self::filesystem::taskfs::TaskFileSystem::new();
    self::filesystem::install_kernel_fs("TASK", Box::new(task_fs));

    {
        let (driver, name) = crate::console::driver::create_new_console();
        self::filesystem::install_kernel_dev(name.as_str(), driver);
    }

    crate::pipes::driver::install();
}
