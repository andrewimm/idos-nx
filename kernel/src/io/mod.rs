extern crate idos_api;

pub mod async_io;
pub mod devices;
pub mod driver;
pub mod filesystem;
pub mod handle;
pub mod provider;

use alloc::boxed::Box;
use idos_api::io::error::IoError;

use crate::files::path::Path;

use self::filesystem::{driver::DriverID, get_driver_id_by_name};

pub fn init_async_io_system() {
    #[cfg(test)]
    {
        use crate::task::actions::handle::create_kernel_task;

        let test_sync_fs = self::filesystem::testing::sync_fs::TestSyncFS::new();
        self::filesystem::install_kernel_fs("TEST", Box::new(test_sync_fs));

        let (_, async_fs_task) = create_kernel_task(
            self::filesystem::testing::async_fs::driver_task,
            Some("TEST FS ASYNC"),
        );
        self::filesystem::install_task_fs("ATEST", async_fs_task);

        let (_, async_dev_task) = create_kernel_task(
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

    let sys_fs = self::filesystem::sysfs::SysFS::new();
    self::filesystem::install_kernel_fs("SYS", Box::new(sys_fs));

    crate::pipes::driver::install();
}

/// Split an absolute file string into a driver ID and path
pub fn prepare_file_path(raw_path: &str) -> Result<(DriverID, Path), IoError> {
    if Path::is_absolute(raw_path) {
        let (drive_name, path_portion) =
            Path::split_absolute_path(raw_path).ok_or(IoError::NotFound)?;
        let driver_id = if drive_name == "DEV" {
            if path_portion.len() > 1 {
                get_driver_id_by_name(&path_portion[1..]).ok_or(IoError::NotFound)?
            } else {
                DriverID::new(0)
            }
        } else {
            get_driver_id_by_name(drive_name).ok_or(IoError::NotFound)?
        };

        Ok((driver_id, Path::from_str(path_portion)))
    } else {
        Err(IoError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::prepare_file_path;

    #[test_case]
    fn test_prepare_empty_path() {
        use super::prepare_file_path;
        let result = prepare_file_path("");
        assert!(result.is_err());
    }

    #[test_case]
    fn test_prepare_relative_path() {
        use super::prepare_file_path;
        let result = prepare_file_path("relative\\path.txt");
        assert!(result.is_err());
    }

    #[test_case]
    fn test_prepare_drive_name_only() {
        use super::prepare_file_path;
        let result = prepare_file_path("DEV:\\");
        assert!(result.is_ok());
        let (driver_id, path) = result.unwrap();
        assert_eq!(*driver_id, 0);
        assert_eq!(path.as_str(), "");
    }

    #[test_case]
    fn test_prepare_valid_path() {
        let result = prepare_file_path("TEST:\\config\\settings.cfg");
        assert!(result.is_ok());
        let (driver_id, path) = result.unwrap();
        // in test mode, TEST: fs should be driver 2
        assert_eq!(*driver_id, 2);
        assert_eq!(path.as_str(), "config\\settings.cfg");
    }
}
