use alloc::string::{String, ToString};
use spin::RwLock;

use crate::collections::SlotList;
use crate::files::path::Path;
use crate::files::stat::FileStatus;
use crate::io::driver::comms::IOResult;
use crate::io::driver::kernel_driver::KernelDriver;
use crate::io::filesystem::driver::AsyncIOCallback;
use crate::io::IOError;
use crate::task::id::TaskID;
use crate::task::switching::{for_each_task_id, get_task};

pub struct TaskFileSystem {
    open_files: RwLock<SlotList<OpenFile>>,
}

impl TaskFileSystem {
    pub fn new() -> Self {
        Self {
            open_files: RwLock::new(SlotList::new()),
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

    fn open_impl(&self, path: Path) -> IOResult {
        let open_file = if path.is_empty() {
            // list tasks
            let content = Self::generate_root_listing();
            OpenFile {
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
            OpenFile {
                task: id,
                cursor: 0,
                content,
            }
        };
        let index = self.open_files.write().insert(open_file);
        Ok(index as u32)
    }

    fn read_impl(&self, instance: u32, buffer: &mut [u8]) -> IOResult {
        let mut open_files = self.open_files.write();
        let open_file = open_files
            .get_mut(instance as usize)
            .ok_or(IOError::FileHandleInvalid)?;
        let content_bytes = open_file.content.as_bytes();
        let bytes_unread = content_bytes.len() - open_file.cursor;
        let to_write = bytes_unread.min(buffer.len());
        let copy_start = open_file.cursor;
        let copy_end = copy_start + to_write;
        buffer[..to_write].copy_from_slice(&content_bytes[copy_start..copy_end]);
        open_file.cursor += to_write;
        Ok(to_write as u32)
    }

    fn stat_impl(&self, instance: u32, file_status: &mut FileStatus) -> IOResult {
        let open_files = self.open_files.read();
        let open_file = open_files
            .get(instance as usize)
            .ok_or(IOError::FileHandleInvalid)?;
        let task_lock = get_task(open_file.task).ok_or(IOError::NotFound)?;
        let task = task_lock.read();
        file_status.byte_size = 0;
        file_status.file_type = 1;
        file_status.modification_time = task.created_at.as_u32();
        Ok(1)
    }
}

pub struct OpenFile {
    task: TaskID,
    cursor: usize,
    content: String,
}

impl KernelDriver for TaskFileSystem {
    fn open(&self, path: Option<Path>, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        match path {
            Some(p) => Some(self.open_impl(p)),
            None => Some(Err(IOError::NotFound)),
        }
    }

    fn read(
        &self,
        instance: u32,
        buffer: &mut [u8],
        _offset: u32,
        _io_callback: AsyncIOCallback,
    ) -> Option<IOResult> {
        Some(self.read_impl(instance, buffer))
    }

    fn stat(
        &self,
        instance: u32,
        file_status: &mut FileStatus,
        _io_callback: AsyncIOCallback,
    ) -> Option<IOResult> {
        Some(self.stat_impl(instance, file_status))
    }

    fn close(&self, instance: u32, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        if self.open_files.write().remove(instance as usize).is_none() {
            Some(Err(IOError::FileHandleInvalid))
        } else {
            Some(Ok(1))
        }
    }
}
