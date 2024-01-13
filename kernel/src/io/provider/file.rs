use idos_api::io::error::IOError;
use spin::Mutex;
use crate::{
    io::{
        async_io::{AsyncOp, OPERATION_FLAG_FILE, FILE_OP_OPEN, FILE_OP_READ, AsyncOpQueue, OpIdGenerator, AsyncOpID},
        driver::comms::IOResult,
        filesystem::{get_driver_id_by_name, driver::DriverID, driver_open, driver_read, driver_write, driver_close},
    },
    files::path::Path,
    task::{switching::{get_current_task, get_current_id}, id::TaskID},
};
use super::IOProvider;

/// Inner contents of a handle that is bound to a file for reading/writing
pub struct FileIOProvider {
    pending_ops: AsyncOpQueue,
    source_id: TaskID,
    driver_id: Mutex<Option<DriverID>>,
    bound_instance: Mutex<Option<u32>>,
}

impl FileIOProvider {
    pub fn new(source_id: TaskID) -> Self {
        Self {
            pending_ops: AsyncOpQueue::new(),
            source_id,
            driver_id: Mutex::new(None),
            bound_instance: Mutex::new(None),
        }
    }

    pub fn bound(source_id: TaskID, driver_id: DriverID, bound_instance: u32) -> Self {
        Self {
            pending_ops: AsyncOpQueue::new(),
            source_id,
            driver_id: Mutex::new(Some(driver_id)),
            bound_instance: Mutex::new(Some(bound_instance)),
        }
    }

    pub fn is_bound(&self) -> bool {
        self.bound_instance.lock().is_some()
    }

    pub fn set_task(&mut self, source_id: TaskID) {
        self.source_id = source_id;
    }
}

impl IOProvider for FileIOProvider {
    fn enqueue_op(&self, op: AsyncOp) -> (AsyncOpID, bool) {
        let id = self.pending_ops.push(op);
        let should_run = self.pending_ops.len() < 2;
        (id, should_run)
    }

    fn peek_op(&self) -> Option<(AsyncOpID, AsyncOp)> {
        self.pending_ops.peek()
    }

    fn remove_op(&self, id: AsyncOpID) -> Option<AsyncOp> {
        self.pending_ops.remove(id)
    }

    fn bind_to(&self, instance: u32) {
        *self.bound_instance.lock() = Some(instance);
    }

    fn open(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<super::IOResult> {
        if self.bound_instance.lock().is_some() {
            return Some(Err(IOError::AlreadyOpen));
        }
        let path_ptr = op.arg0 as *const u8;
        let path_len = op.arg1 as usize;
        let try_path_str = unsafe {
            core::str::from_utf8(core::slice::from_raw_parts(path_ptr, path_len))
        };
        let path_str = match try_path_str {
            Ok(path) => path,
            Err(_) => return Some(Err(IOError::NotFound)),
        };
        let (driver_id, path) = match prepare_file_path(path_str) {
            Ok(pair) => pair,
            Err(_) => return Some(Err(IOError::NotFound)),
        };

        *self.driver_id.lock() = Some(driver_id);
        driver_open(driver_id, path, (self.source_id, provider_index, id))
    }

    fn read(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<super::IOResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            let buffer_ptr = op.arg0 as *mut u8;
            let buffer_len = op.arg1 as usize;
            let buffer = unsafe {
                core::slice::from_raw_parts_mut(buffer_ptr, buffer_len)
            };

            let driver_id: DriverID = self.driver_id.lock().unwrap();
            return driver_read(driver_id, instance, buffer, (self.source_id, provider_index, id));
        }
        Some(Err(IOError::FileHandleInvalid))
    }

    fn write(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<super::IOResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            let buffer_ptr = op.arg0 as *const u8;
            let buffer_len = op.arg1 as usize;
            let buffer = unsafe {
                core::slice::from_raw_parts(buffer_ptr, buffer_len)
            };
            let driver_id: DriverID = self.driver_id.lock().unwrap();
            return driver_write(driver_id, instance, buffer, (self.source_id, provider_index, id));
        }
        Some(Err(IOError::FileHandleInvalid))
    }

    fn close(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<super::IOResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            let driver_id: DriverID = self.driver_id.lock().unwrap();
            return driver_close(driver_id, instance, (self.source_id, provider_index, id));
        }
        Some(Err(IOError::FileHandleInvalid))
    }
}

fn prepare_file_path(raw_path: &str) -> Result<(DriverID, Path), ()> {
    if Path::is_absolute(raw_path) {
        let (drive_name, path_portion) = Path::split_absolute_path(raw_path).ok_or(())?;
        let driver_id = if drive_name == "DEV" {
            if path_portion.len() > 1 {
                get_driver_id_by_name(&path_portion[1..]).ok_or(())?
            } else {
                DriverID::new(0)
            }
        } else {
            get_driver_id_by_name(drive_name).ok_or(())?
        };

        Ok((driver_id, Path::from_str(path_portion)))
    } else {
        let (current_drive_id, mut working_dir): (DriverID, Path) = {
            let task_lock = get_current_task();
            let task = task_lock.read();
            (task.current_drive.driver_id, task.working_dir.clone())
        };
        working_dir.push(raw_path);
        Ok((current_drive_id, working_dir))
    }
}

