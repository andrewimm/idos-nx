use alloc::string::String;
use crate::collections::SlotList;
use crate::files::handle::DriverHandle;

pub struct OpenFile {
    pub drive: u32,
    pub driver_handle: DriverHandle,
    pub filename: String,
}

/// An Open File Map maps numeric slots to files currently opened by this task.
pub type OpenFileMap = SlotList<OpenFile>;

/// A FileHandle is an identifier for a currently open file, and is an index
/// into the task's OpenFileMap
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

