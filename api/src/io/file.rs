#[repr(u32)]
pub enum FileType {
    File = 1,
    Dir = 2,
}

#[repr(C)]
pub struct FileStatus {
    /// Size of the file, in bytes
    pub byte_size: u32,
    /// Bitmap indicating the type of the file
    pub file_type: u32,
    /// ID of the drive this file is found on
    pub drive_id: u32,
    /// System timestamp of last modification
    pub modification_time: u32,
}

impl FileStatus {
    pub fn new() -> Self {
        Self {
            byte_size: 0,
            file_type: 0,
            drive_id: 0,
            modification_time: 0,
        }
    }
}
