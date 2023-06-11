use alloc::string::String;
use crate::collections::SlotList;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::filesystem::drive::DriveID;

#[derive(Clone)]
pub struct CurrentDrive {
    pub name: String,
    pub id: DriveID,
}

impl CurrentDrive {
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            id: DriveID(0),
        }
    }
}

#[derive(Clone)]
pub struct OpenFile {
    pub drive: DriveID,
    pub driver_handle: DriverHandle,
    pub filename: Path,
}

/// An Open File Map maps numeric slots to files currently opened by this task.
pub type OpenFileMap = SlotList<OpenFile>;

/// A FileHandle is an identifier for a currently open file, and is an index
/// into the task's OpenFileMap
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct FileHandle(u32);

impl FileHandle {
    pub fn new(index: usize) -> Self {
        Self(index as u32)
    }
}

impl Into<usize> for FileHandle {
    fn into(self) -> usize {
        self.0 as usize
    }
}

