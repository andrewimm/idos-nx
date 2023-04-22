#[derive(Debug)]
pub enum FsError {
    /// Indicates that a drive does not exist, or was uninstalled
    DriveNotFound,
    /// An attempt to install a FS or device driver failed
    InstallFailed,
}
