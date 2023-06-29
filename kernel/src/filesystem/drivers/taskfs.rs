use alloc::string::{String, ToString};
use spin::RwLock;

use crate::collections::SlotList;
use crate::filesystem::kernel::KernelFileSystem;
use crate::files::{path::Path, handle::DriverHandle, error::IOError};
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
        Some(String::from("A Task!"))
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
            let id = TaskID::new(path.as_str().parse::<u32>().map_err(|_| IOError::NotFound)?);
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
        let handle = handles.get_mut(handle.into()).ok_or(IOError::FileHandleInvalid)?;
        let content_bytes = handle.content.as_bytes();
        let bytes_unread = content_bytes.len() - handle.cursor;
        let to_write = bytes_unread.min(buffer.len());
        let copy_start = handle.cursor;
        let copy_end = copy_start + to_write;
        buffer[..to_write].copy_from_slice(&content_bytes[copy_start..copy_end]);
        handle.cursor += to_write;
        Ok(to_write as u32)
    }

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn close(&self, handle: DriverHandle) -> Result<(), IOError> {
        if self.handles.write().remove(handle.into()).is_none() {
            Err(IOError::FileHandleInvalid)
        } else {
            Ok(())
        }
    }

    fn seek(&self, handle: DriverHandle, offset: crate::files::cursor::SeekMethod) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }
}
