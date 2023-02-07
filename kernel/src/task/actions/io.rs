use crate::files::path::Path;
use crate::task::switching::get_current_task;

use super::super::files::FileHandle;

pub enum IOError {
    /// A File or Directory with the given path does not exist
    NotFound,
    /// The file handle used for IO is not currently open
    FileHandleInvalid,
    /// The file handle used for IO is not the correct type for that operation
    FileHandleWrongType,
}

/// Open a file at a specified path. If the provided string is not an absolute
/// path, it will be opened relative to the current task's working directory.
/// On success, a new File Handle will be opened and returned.
pub fn open_path<'path>(path_string: &'path str) -> Result<FileHandle, IOError> {
    let (drive, path) = if Path::is_absolute(path_string) {
        return Err(IOError::NotFound);
    } else {
        let mut working_dir = get_current_task().read().working_dir.clone();
        working_dir.push(path_string);
        (0, working_dir)
    };
    Err(IOError::NotFound)
}

/// Open a directory at a specified path. Similar to opening a file,
/// non-absolute paths will be opened relative to the task's working directory.
/// On success, a new File Handle will be opened and returned.
pub fn open_directory<'path>(path_string: &'path str) -> Result<FileHandle, IOError> {
    Err(IOError::NotFound)
}

/// Read bytes from an open file into a mutable byte buffer. On success, return
/// the number of bytes read.
pub fn read_file(handle: FileHandle, buffer: &mut [u8]) -> Result<usize, IOError> {
    Err(IOError::FileHandleInvalid)
}

/// Write bytes from a byte buffer to a file. On success, return the number of
/// bytes written.
pub fn write_file(handle: FileHandle, buffer: &[u8]) -> Result<usize, IOError> {
    Err(IOError::FileHandleInvalid)
}

/// Close a currently opened file. Upon return, regardless of success or error,
/// the file handled used will no longer be valid.
pub fn close_file(handle: FileHandle) -> Result<(), IOError> {
    Err(IOError::FileHandleInvalid)
}
