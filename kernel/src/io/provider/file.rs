use core::sync::atomic::Ordering;

use super::{AsyncOpQueue, IOProvider, OpIdGenerator, UnmappedAsyncOp};
use crate::{
    files::{path::Path, stat::FileStatus},
    io::{
        async_io::{AsyncOpID, FILE_OP_STAT},
        filesystem::{
            driver::DriverID, driver_close, driver_open, driver_read, driver_stat, driver_write,
            get_driver_id_by_name,
        },
        handle::Handle,
    },
    task::{
        id::{AtomicTaskID, TaskID},
        switching::get_current_id,
    },
};
use idos_api::io::{error::IOError, AsyncOp};
use spin::Mutex;

/// Inner contents of a handle that is bound to a file for reading/writing
pub struct FileIOProvider {
    source_id: AtomicTaskID,
    driver_id: Mutex<Option<DriverID>>,
    bound_instance: Mutex<Option<u32>>,

    active: Mutex<Option<(AsyncOpID, UnmappedAsyncOp)>>,
    id_gen: OpIdGenerator,
    pending_ops: AsyncOpQueue,
}

impl FileIOProvider {
    pub fn new(source_id: TaskID) -> Self {
        Self {
            source_id: AtomicTaskID::new(source_id.into()),
            driver_id: Mutex::new(None),
            bound_instance: Mutex::new(None),

            active: Mutex::new(None),
            id_gen: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
        }
    }

    pub fn bound(source_id: TaskID, driver_id: DriverID, bound_instance: u32) -> Self {
        Self {
            source_id: AtomicTaskID::new(source_id.into()),
            driver_id: Mutex::new(Some(driver_id)),
            bound_instance: Mutex::new(Some(bound_instance)),

            active: Mutex::new(None),
            id_gen: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
        }
    }

    pub fn is_bound(&self) -> bool {
        self.bound_instance.lock().is_some()
    }

    pub fn set_task(&self, source_id: TaskID) {
        let _ = self.source_id.swap(source_id, Ordering::SeqCst);
    }

    // TODO: this isn't enough. Devices and other things need to handle
    // this properly at the driver level, so that we can ref-count things and
    // not free resources prematurely
    pub fn duplicate(&self, new_task: TaskID) -> Self {
        Self {
            source_id: AtomicTaskID::new(new_task.into()),
            driver_id: Mutex::new(self.driver_id.lock().clone()),
            bound_instance: Mutex::new(self.bound_instance.lock().clone()),
            active: Mutex::new(None),
            id_gen: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
        }
    }
}

impl IOProvider for FileIOProvider {
    fn enqueue_op(&self, provider_index: u32, op: &AsyncOp, wake_set: Option<Handle>) -> AsyncOpID {
        let id = self.id_gen.next_id();
        let unmapped =
            UnmappedAsyncOp::from_op(op, wake_set.map(|handle| (get_current_id(), handle)));
        if self.active.lock().is_some() {
            self.pending_ops.push(id, unmapped);
            return id;
        }

        *self.active.lock() = Some((id, unmapped));
        match self.run_active_op(provider_index) {
            Some(result) => {
                *self.active.lock() = None;
                let return_value = self.transform_result(op.op_code, result);
                op.return_value.store(return_value, Ordering::SeqCst);
                op.signal.store(1, Ordering::SeqCst);
            }
            None => (),
        }
        id
    }

    fn get_active_op(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)> {
        self.active.lock().clone()
    }

    fn take_active_op(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)> {
        self.active.lock().take()
    }

    fn pop_queued_op(&self) {
        let next = self.pending_ops.pop();
        *self.active.lock() = next;
    }

    fn bind_to(&self, instance: u32) {
        *self.bound_instance.lock() = Some(instance);
    }

    fn open(
        &self,
        provider_index: u32,
        id: AsyncOpID,
        op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        if self.bound_instance.lock().is_some() {
            return Some(Err(IOError::AlreadyOpen));
        }
        let path_ptr = op.args[0] as *const u8;
        let path_len = op.args[1] as usize;
        let path_str = unsafe {
            match core::str::from_utf8(core::slice::from_raw_parts(path_ptr, path_len)) {
                Ok(str) => str,
                Err(_) => return Some(Err(IOError::NotFound)),
            }
        };
        let (driver_id, path) = match prepare_file_path(path_str) {
            Ok(pair) => pair,
            Err(_) => return Some(Err(IOError::NotFound)),
        };
        *self.driver_id.lock() = Some(driver_id);
        driver_open(
            driver_id,
            path,
            (self.source_id.load(Ordering::SeqCst), provider_index, id),
        )
    }

    fn read(
        &self,
        provider_index: u32,
        id: AsyncOpID,
        op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            let buffer_ptr = op.args[0] as *mut u8;
            let buffer_len = op.args[1] as usize;
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
            let read_offset = op.args[2];
            let driver_id: DriverID = self.driver_id.lock().unwrap();
            return driver_read(
                driver_id,
                instance,
                buffer,
                read_offset,
                (self.source_id.load(Ordering::SeqCst), provider_index, id),
            );
        }
        Some(Err(IOError::FileHandleInvalid))
    }

    fn write(
        &self,
        provider_index: u32,
        id: AsyncOpID,
        op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            let buffer_ptr = op.args[0] as *const u8;
            let buffer_len = op.args[1] as usize;
            let buffer = unsafe { core::slice::from_raw_parts(buffer_ptr, buffer_len) };
            let write_offset = op.args[2];
            let driver_id: DriverID = self.driver_id.lock().unwrap();
            return driver_write(
                driver_id,
                instance,
                buffer,
                write_offset,
                (self.source_id.load(Ordering::SeqCst), provider_index, id),
            );
        }
        Some(Err(IOError::FileHandleInvalid))
    }

    fn close(
        &self,
        provider_index: u32,
        id: AsyncOpID,
        _op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            let driver_id: DriverID = self.driver_id.lock().unwrap();
            return driver_close(
                driver_id,
                instance,
                (self.source_id.load(Ordering::SeqCst), provider_index, id),
            );
        }
        Some(Err(IOError::FileHandleInvalid))
    }

    fn extended_op(
        &self,
        provider_index: u32,
        id: AsyncOpID,
        op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            match op.op_code & 0xffff {
                FILE_OP_STAT => {
                    let status_ptr = op.args[0] as *mut FileStatus;
                    let status_len = op.args[1] as usize;
                    if status_len < core::mem::size_of::<FileStatus>() {
                        return Some(Err(IOError::InvalidArgument));
                    }
                    let driver_id: DriverID = self.driver_id.lock().unwrap();
                    let file_status: &mut FileStatus = unsafe { &mut *status_ptr };
                    return driver_stat(
                        driver_id,
                        instance,
                        file_status,
                        (self.source_id.load(Ordering::SeqCst), provider_index, id),
                    );
                }
                _ => return Some(Err(IOError::UnsupportedOperation)),
            }
        }
        Some(Err(IOError::FileHandleInvalid))
    }
}

fn prepare_file_path(raw_path: &str) -> Result<(DriverID, Path), IOError> {
    if Path::is_absolute(raw_path) {
        let (drive_name, path_portion) =
            Path::split_absolute_path(raw_path).ok_or(IOError::NotFound)?;
        let driver_id = if drive_name == "DEV" {
            if path_portion.len() > 1 {
                get_driver_id_by_name(&path_portion[1..]).ok_or(IOError::NotFound)?
            } else {
                DriverID::new(0)
            }
        } else {
            get_driver_id_by_name(drive_name).ok_or(IOError::NotFound)?
        };

        Ok((driver_id, Path::from_str(path_portion)))
    } else {
        Err(IOError::NotFound)
    }
}
