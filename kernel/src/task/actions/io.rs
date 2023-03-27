use alloc::string::ToString;
use crate::files::path::Path;
use crate::filesystem::drive::DriveID;
use crate::filesystem::{get_driver_by_id, get_drive_id_by_name};
use crate::task::files::{OpenFile, CurrentDrive};
use crate::task::switching::get_current_task;

use super::super::files::FileHandle;

#[derive(Debug)]
pub enum IOError {
    /// A File or Directory with the given path does not exist
    NotFound,
    /// The file handle used for IO is not currently open
    FileHandleInvalid,
    /// The file handle used for IO is not the correct type for that operation
    FileHandleWrongType,
    /// A read operation failed
    ReadFailed,
    /// A write operation failed
    WriteFailed,
}

pub fn set_active_drive(drive_name: &str) -> Result<DriveID, IOError> {
    let found_id = get_drive_id_by_name(drive_name);
    match found_id {
        Ok(id) => {
            let task_lock = get_current_task();
            let mut task = task_lock.write();
            task.current_drive = CurrentDrive {
                name: drive_name.to_string(),
                id,
            };
            Ok(id)
        },
        _ => Err(IOError::NotFound),
    }
}

/// Open a file at a specified path. If the provided string is not an absolute
/// path, it will be opened relative to the current task's working directory.
/// On success, a new File Handle will be opened and returned.
pub fn open_path<'path>(path_string: &'path str) -> Result<FileHandle, IOError> {
    let (drive_id, path) = if Path::is_absolute(path_string) {
        return Err(IOError::NotFound);
    } else {
        let (current_drive_id, mut working_dir) = {
            let task_lock = get_current_task();
            let task = task_lock.read();
            (task.current_drive.id, task.working_dir.clone())
        };
        working_dir.push(path_string);
        (current_drive_id, working_dir)
    };

    let driver_handle = get_driver_by_id(drive_id)
        .map_err(|_| IOError::NotFound)?
        .open(path.clone())
        .map_err(|_| IOError::NotFound)?;

    let open_handle_index = {
        let task_lock = get_current_task();
        let mut task = task_lock.write();
        task.open_files.insert(
            OpenFile {
                drive: drive_id,
                driver_handle,
                filename: path,
            }
        )
    };

    Ok(FileHandle::new(open_handle_index))
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
    let (drive_id, driver_handle) = {
        let task_lock = get_current_task();
        let task = task_lock.read();
        let entry = task.open_files.get(handle.into()).ok_or(IOError::FileHandleInvalid)?;
        (entry.drive, entry.driver_handle)
    };

    get_driver_by_id(drive_id)
        .map_err(|_| IOError::NotFound)?
        .read(driver_handle, buffer)
        .map_err(|_| IOError::ReadFailed)
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
