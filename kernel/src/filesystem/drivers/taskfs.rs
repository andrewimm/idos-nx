use alloc::string::{String, ToString};
use spin::RwLock;

use crate::collections::SlotList;
use crate::files::{handle::DriverHandle, path::Path};
use crate::filesystem::kernel::KernelFileSystem;
use crate::io::IOError;
use crate::task::id::TaskID;
use crate::task::switching::{for_each_task_id, get_task};

pub struct TaskFileSystem {
    handles: RwLock<SlotList<OpenHandle>>,
}

impl TaskFileSystem {
    pub fn new() -> Self {
        Self {
            handles: RwLock::new(SlotList::new()),
        }
    }

    pub fn generate_root_listing() -> String {
        let mut content = String::new();
        for_each_task_id(|id| {
            content.push_str(&Into::<u32>::into(id).to_string());
            content.push('\0');
        });
        content
    }

    pub fn generate_content_for_task(id: TaskID) -> Option<String> {
        let task_lock = get_task(id)?;
        let task = task_lock.read();
        let mut content = String::new();
        content.push_str("ID: ");
        content.push_str(&id.to_string());
        content.push_str("\nName: ");
        content.push_str(&task.filename);
        content.push_str("\nState: ");
        content.push_str(&task.state.to_string());
        content.push_str("\nParent: ");
        content.push_str(&task.parent_id.to_string());
        content.push('\n');
        Some(content)
    }
}

pub struct OpenHandle {
    task: TaskID,
    cursor: usize,
    content: String,
}

impl KernelFileSystem for TaskFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, IOError> {
        let handle = if path.is_empty() {
            // list tasks
            let content = Self::generate_root_listing();
            OpenHandle {
                task: TaskID::new(0),
                cursor: 0,
                content,
            }
        } else {
            let id = TaskID::new(
                path.as_str()
                    .parse::<u32>()
                    .map_err(|_| IOError::NotFound)?,
            );
            let content = Self::generate_content_for_task(id).ok_or(IOError::NotFound)?;
            OpenHandle {
                task: id,
                cursor: 0,
                content,
            }
        };
        let index = self.handles.write().insert(handle);
        Ok(DriverHandle(index as u32))
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<u32, IOError> {
        let mut handles = self.handles.write();
        let handle = handles
            .get_mut(handle.into())
            .ok_or(IOError::FileHandleInvalid)?;
        let content_bytes = handle.content.as_bytes();
        let bytes_unread = content_bytes.len() - handle.cursor;
        let to_write = bytes_unread.min(buffer.len());
        let copy_start = handle.cursor;
        let copy_end = copy_start + to_write;
        buffer[..to_write].copy_from_slice(&content_bytes[copy_start..copy_end]);
        handle.cursor += to_write;
        Ok(to_write as u32)
    }

    fn write(&self, _handle: DriverHandle, _buffer: &[u8]) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn close(&self, handle: DriverHandle) -> Result<(), IOError> {
        if self.handles.write().remove(handle.into()).is_none() {
            Err(IOError::FileHandleInvalid)
        } else {
            Ok(())
        }
    }

    fn seek(
        &self,
        _handle: DriverHandle,
        _offset: crate::files::cursor::SeekMethod,
    ) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn stat(&self, handle: DriverHandle) -> Result<crate::files::stat::FileStatus, IOError> {
        let handles = self.handles.read();
        let handle = handles
            .get(handle.into())
            .ok_or(IOError::FileHandleInvalid)?;
        let task_lock = get_task(handle.task).ok_or(IOError::NotFound)?;
        let task = task_lock.read();

        let stat = crate::files::stat::FileStatus {
            byte_size: 0,
            file_type: 1,
            drive_id: 0,
            modification_time: task.created_at.as_u32(),
        };
        Ok(stat)
    }
}
