use core::sync::atomic::Ordering;

use super::{AsyncOpQueue, IOProvider, OpIdGenerator, UnmappedAsyncOp};
use crate::{
    files::path::Path,
    io::{
        async_io::{AsyncOpID, FILE_OP_IOCTL, FILE_OP_MKDIR, FILE_OP_RMDIR, FILE_OP_STAT, FILE_OP_UNLINK},
        filesystem::{
            driver::DriverID, driver_close, driver_ioctl, driver_mkdir, driver_open, driver_read,
            driver_rmdir, driver_share, driver_stat, driver_unlink, driver_write,
            get_driver_id_by_name,
        },
        handle::Handle,
        prepare_file_path,
    },
    task::{
        id::{AtomicTaskID, TaskID},
        map::get_task,
        switching::{get_current_id, get_current_task},
    },
};
use alloc::collections::BTreeMap;
use idos_api::io::{
    error::{IoError, IoResult},
    file::FileStatus,
    AsyncOp,
};
use spin::{Mutex, RwLock};

/// Inner contents of a handle that is bound to a file for reading/writing
pub struct FileIOProvider {
    /// ID of the task that created the provider
    source_id: AtomicTaskID,
    driver_id: Mutex<Option<DriverID>>,
    bound_instance: Mutex<Option<u32>>,

    id_gen: OpIdGenerator,
    pending_ops: RwLock<BTreeMap<AsyncOpID, UnmappedAsyncOp>>,
}

impl FileIOProvider {
    pub fn new(source_id: TaskID) -> Self {
        Self {
            source_id: AtomicTaskID::new(source_id.into()),
            driver_id: Mutex::new(None),
            bound_instance: Mutex::new(None),

            id_gen: OpIdGenerator::new(),
            pending_ops: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn bound(source_id: TaskID, driver_id: DriverID, bound_instance: u32) -> Self {
        Self {
            source_id: AtomicTaskID::new(source_id.into()),
            driver_id: Mutex::new(Some(driver_id)),
            bound_instance: Mutex::new(Some(bound_instance)),

            id_gen: OpIdGenerator::new(),
            pending_ops: RwLock::new(BTreeMap::new()),
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
            id_gen: OpIdGenerator::new(),
            pending_ops: RwLock::new(BTreeMap::new()),
        }
    }
}

impl IOProvider for FileIOProvider {
    fn add_op(
        &self,
        provider_index: u32,
        op: &AsyncOp,
        args: [u32; 3],
        wake_set: Option<Handle>,
    ) -> AsyncOpID {
        let id = self.id_gen.next_id();
        let unmapped =
            UnmappedAsyncOp::from_op(op, args, wake_set.map(|handle| (get_current_id(), handle)));
        self.pending_ops.write().insert(id, unmapped);

        match self.run_op(provider_index, id) {
            Some(result) => {
                if let Some(completed_op) = self.remove_op(id) {
                    if let Ok(_) = result {
                        completed_op.maybe_close_handle(get_current_task(), provider_index);
                    }
                }
                let return_value = self.transform_result(op.op_code, result);
                op.return_value.store(return_value, Ordering::SeqCst);
                op.signal.store(1, Ordering::SeqCst);
            }
            None => (),
        }
        id
    }

    fn get_op(&self, id: AsyncOpID) -> Option<UnmappedAsyncOp> {
        self.pending_ops.read().get(&id).cloned()
    }

    fn remove_op(&self, id: AsyncOpID) -> Option<UnmappedAsyncOp> {
        self.pending_ops.write().remove(&id)
    }

    fn bind_to(&self, instance: u32) {
        *self.bound_instance.lock() = Some(instance);
    }

    fn open(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IoResult> {
        if self.bound_instance.lock().is_some() {
            return Some(Err(IoError::AlreadyOpen));
        }
        let path_ptr = op.args[0] as *const u8;
        let path_len = op.args[1] as usize;
        let path_str = unsafe {
            match core::str::from_utf8(core::slice::from_raw_parts(path_ptr, path_len)) {
                Ok(str) => str,
                Err(_) => return Some(Err(IoError::NotFound)),
            }
        };
        let flags = op.args[2];
        let (driver_id, path) = match prepare_file_path(path_str) {
            Ok(pair) => pair,
            Err(_) => return Some(Err(IoError::NotFound)),
        };
        *self.driver_id.lock() = Some(driver_id);
        driver_open(
            driver_id,
            path,
            flags,
            (self.source_id.load(Ordering::SeqCst), provider_index, id),
        )
    }

    fn read(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IoResult> {
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
        Some(Err(IoError::FileHandleInvalid))
    }

    fn write(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IoResult> {
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
        Some(Err(IoError::FileHandleInvalid))
    }

    fn close(&self, provider_index: u32, id: AsyncOpID, _op: UnmappedAsyncOp) -> Option<IoResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            let driver_id: DriverID = self.driver_id.lock().unwrap();
            return driver_close(
                driver_id,
                instance,
                (self.source_id.load(Ordering::SeqCst), provider_index, id),
            );
        }
        Some(Err(IoError::FileHandleInvalid))
    }

    /// Shares the provider with another task.
    /// A task may have multiple handles open to the same backing provider.
    /// When transferring one of those handles, the kernel determines whether
    /// the provider should be fully transferred or duplicated.
    /// This flag is passed onto the driver, which may need to handle resources
    /// differently based on how many Tasks are referencing the same provider.
    fn share(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IoResult> {
        if let Some(instance) = self.bound_instance.lock().clone() {
            let driver_id: DriverID = self.driver_id.lock().unwrap();
            let transfer_to = TaskID::new(op.args[0] as u32);
            let is_move = op.args[1] != 0;
            if get_task(transfer_to).is_none() {
                return Some(Err(IoError::InvalidArgument));
            }
            return driver_share(
                driver_id,
                instance,
                transfer_to,
                is_move,
                (self.source_id.load(Ordering::SeqCst), provider_index, id),
            );
        }
        Some(Err(IoError::FileHandleInvalid))
    }

    fn extended_op(
        &self,
        provider_index: u32,
        id: AsyncOpID,
        op: UnmappedAsyncOp,
    ) -> Option<IoResult> {
        let op_code = op.op_code & 0xffff;

        // Path-based operations that don't require a bound file instance
        match op_code {
            FILE_OP_MKDIR | FILE_OP_UNLINK | FILE_OP_RMDIR => {
                let path_ptr = op.args[0] as *const u8;
                let path_len = op.args[1] as usize;
                let path_str = unsafe {
                    match core::str::from_utf8(core::slice::from_raw_parts(path_ptr, path_len)) {
                        Ok(str) => str,
                        Err(_) => return Some(Err(IoError::NotFound)),
                    }
                };
                let (driver_id, path) = match prepare_file_path(path_str) {
                    Ok(pair) => pair,
                    Err(_) => return Some(Err(IoError::NotFound)),
                };
                let io_cb = (self.source_id.load(Ordering::SeqCst), provider_index, id);
                return match op_code {
                    FILE_OP_MKDIR => driver_mkdir(driver_id, path, io_cb),
                    FILE_OP_UNLINK => driver_unlink(driver_id, path, io_cb),
                    FILE_OP_RMDIR => driver_rmdir(driver_id, path, io_cb),
                    _ => unreachable!(),
                };
            }
            _ => {}
        }

        // Instance-bound operations
        if let Some(instance) = self.bound_instance.lock().clone() {
            match op_code {
                FILE_OP_STAT => {
                    let status_ptr = op.args[0] as *mut FileStatus;
                    let status_len = op.args[1] as usize;
                    if status_len < core::mem::size_of::<FileStatus>() {
                        return Some(Err(IoError::InvalidArgument));
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
                FILE_OP_IOCTL => {
                    let ioctl = op.args[0];
                    let arg = op.args[1];
                    let arg_len = op.args[2] as usize;
                    let driver_id: DriverID = self.driver_id.lock().unwrap();
                    return driver_ioctl(
                        driver_id,
                        instance,
                        ioctl,
                        arg,
                        arg_len,
                        (self.source_id.load(Ordering::SeqCst), provider_index, id),
                    );
                }
                _ => return Some(Err(IoError::UnsupportedOperation)),
            }
        }
        Some(Err(IoError::FileHandleInvalid))
    }
}
