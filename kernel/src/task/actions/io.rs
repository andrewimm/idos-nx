use alloc::string::ToString;
use crate::files::cursor::SeekMethod;
use crate::files::error::IOError;
use crate::files::path::Path;
use crate::filesystem::drive::DriveID;
use crate::filesystem::{get_driver_by_id, get_drive_id_by_name};
use crate::pipes::{create_pipe, get_pipe_drive_id};
use crate::task::files::{OpenFile, CurrentDrive};
use crate::task::id::TaskID;
use crate::task::switching::{get_current_task, get_task};

use super::super::files::FileHandle;

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

/// Do the actual work of opening a file from a filesystem driver, but don't
/// attach it to the current task yet
pub fn prepare_open_file(path_string: &str) -> Result<OpenFile, IOError> {
    let (drive_id, path) = if Path::is_absolute(path_string) {
        let mut parts = path_string.split(':');
        let drive_name = parts.next().ok_or(IOError::NotFound)?;
        let path_portion = parts.next().ok_or(IOError::NotFound)?;
        let drive_id = get_drive_id_by_name(drive_name).map_err(|_| IOError::NotFound)?;
       
        (drive_id, Path::from_str(path_portion))
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

    Ok(
        OpenFile {
            drive: drive_id,
            driver_handle,
            filename: path,
        }
    )
}

/// Open a file at a specified path. If the provided string is not an absolute
/// path, it will be opened relative to the current task's working directory.
/// On success, a new File Handle will be opened and returned.
pub fn open_path<'path>(path_string: &'path str) -> Result<FileHandle, IOError> {
    let open_file = prepare_open_file(path_string)?;

    let open_handle_index = {
        let task_lock = get_current_task();
        let mut task = task_lock.write();
        task.open_files.insert(open_file)
    };

    Ok(FileHandle::new(open_handle_index))
}

pub fn open_pipe() -> Result<(FileHandle, FileHandle), IOError> {
    let (read_handle, write_handle) = create_pipe();
    let drive_id = get_pipe_drive_id();

    let (read_handle_index, write_handle_index) = {
        let task_lock = get_current_task();
        let mut task = task_lock.write();
        let read = task.open_files.insert(
            OpenFile {
                drive: drive_id,
                driver_handle: read_handle,
                filename: Path::from_str("READ PIPE"),
            }
        );
        let write = task.open_files.insert(
            OpenFile {
                drive: drive_id,
                driver_handle: write_handle,
                filename: Path::from_str("WRITE PIPE"),
            }
        );

        (read, write)
    };

    Ok((
        FileHandle::new(read_handle_index),
        FileHandle::new(write_handle_index),
    ))
}

/// Transfer an open file handle to another task
pub fn transfer_handle(handle: FileHandle, task: TaskID) -> Result<FileHandle, IOError> {
    let open_file = {
        let task_lock = get_current_task();
        let mut task = task_lock.write();
        task.open_files.remove(handle.into()).ok_or(IOError::FileHandleInvalid)?
    };

    let new_index = {
        let task_lock = get_task(task).ok_or(IOError::NotFound)?;
        let mut task = task_lock.write();
        task.open_files.insert(open_file)
    };

    Ok(FileHandle::new(new_index))
}

/// Open a directory at a specified path. Similar to opening a file,
/// non-absolute paths will be opened relative to the task's working directory.
/// On success, a new File Handle will be opened and returned.
pub fn open_directory<'path>(_path_string: &'path str) -> Result<FileHandle, IOError> {
    Err(IOError::NotFound)
}

/// Read bytes from an open file into a mutable byte buffer. On success, return
/// the number of bytes read.
pub fn read_file(handle: FileHandle, buffer: &mut [u8]) -> Result<u32, IOError> {
    let (drive_id, driver_handle) = {
        let task_lock = get_current_task();
        let task = task_lock.read();
        let entry = task.open_files.get(handle.into()).ok_or(IOError::FileHandleInvalid)?;
        (entry.drive, entry.driver_handle)
    };

    get_driver_by_id(drive_id)
        .map_err(|_| IOError::NotFound)?
        .read(driver_handle, buffer)
        .map_err(|_| IOError::OperationFailed)
}

/// Write bytes from a byte buffer to a file. On success, return the number of
/// bytes written.
pub fn write_file(handle: FileHandle, buffer: &[u8]) -> Result<u32, IOError> {
    //return Err(IOError::FileHandleInvalid);
    let (drive_id, driver_handle) = {
        let task_lock = get_current_task();
        let task = task_lock.read();
        let entry = task.open_files.get(handle.into()).ok_or(IOError::FileHandleInvalid)?;
        (entry.drive, entry.driver_handle)
    };

    get_driver_by_id(drive_id)
        .map_err(|_| IOError::NotFound)?
        .write(driver_handle, buffer)
        .map_err(|_| IOError::OperationFailed)
}

/// Close a currently opened file. Upon return, regardless of success or error,
/// the file handled used will no longer be valid.
pub fn close_file(handle: FileHandle) -> Result<(), IOError> {
    let (drive_id, driver_handle) = {
        let task_lock = get_current_task();
        let mut task = task_lock.write();
        let entry = task.open_files.remove(handle.into()).ok_or(IOError::FileHandleInvalid)?;
        (entry.drive, entry.driver_handle)
    };

    get_driver_by_id(drive_id)
        .map_err(|_| IOError::NotFound)?
        .close(driver_handle)
        .map_err(|_| IOError::OperationFailed)
}

pub fn seek_file(handle: FileHandle, method: SeekMethod) -> Result<u32, IOError> {
    let (drive_id, driver_handle) = {
        let task_lock = get_current_task();
        let task = task_lock.read();
        let entry = task.open_files.get(handle.into()).ok_or(IOError::FileHandleInvalid)?;
        (entry.drive, entry.driver_handle)
    };

    get_driver_by_id(drive_id)
        .map_err(|_| IOError::NotFound)?
        .seek(driver_handle, method)
        .map_err(|_| IOError::OperationFailed)
}
