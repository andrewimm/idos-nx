use alloc::{collections::VecDeque, string::String};
use idos_api::io::error::IOError;
use crate::{
    io::{
        async_io::{AsyncOp, OPERATION_FLAG_FILE, FILE_OP_OPEN, FILE_OP_READ},
        filesystem::{get_driver_id_by_name, driver::{DriverID, IOResult}, driver_open, driver_read},
    },
    files::path::Path,
    task::switching::{get_current_task, get_current_id},
};
use super::IOProvider;

/// Inner contents of a handle that is bound to a file for reading/writing
pub struct FileIOProvider {
    pending_ops: VecDeque<AsyncOp>,
    driver_id: Option<DriverID>,
    bound_instance: Option<u32>,
}

impl FileIOProvider {
    pub fn new() -> Self {
        Self {
            pending_ops: VecDeque::new(),
            driver_id: None,
            bound_instance: None,
        }
    }

    pub fn is_bound(&self) -> bool {
        self.bound_instance.is_some()
    }

    pub fn op_completed(&mut self, id: u32, result: IOResult) {
        // find the op
        // for now, just pull the first
        if self.pending_ops.is_empty() {
            return;
        }
        let op = self.pending_ops.pop_front().unwrap();
        if op.op_code & 0xffff == FILE_OP_OPEN {
            if let Ok(value) = result {
                self.bound_instance = Some(value);
                op.complete(1);
                return;
            }
        }
        op.complete_with_result(result);
    }
}

impl IOProvider for FileIOProvider {
    fn add_op(&mut self, index: u32, op: AsyncOp) -> Result<(), ()> {
        if op.op_code & OPERATION_FLAG_FILE == 0 {
            return Err(());
        }

        let op_code = op.op_code & 0xffff;

        if let Some(instance) = self.bound_instance {
            match op_code {
                FILE_OP_READ => {
                    let buffer_ptr = op.arg0 as *mut u8;
                    let buffer_len = op.arg1 as usize;
                    let buffer = unsafe {
                        core::slice::from_raw_parts_mut(buffer_ptr, buffer_len)
                    };

                    self.pending_ops.push_back(op);
                    let driver_id = self.driver_id.unwrap();
                    if let Some(result) = driver_read(driver_id, instance, buffer) {
                        self.op_completed(0, result);
                    }
                },
                FILE_OP_WRITE => panic!("NOT SUPPORTED"),
                FILE_OP_SEEK => panic!("NOT SUPPORTED"),
                FILE_OP_STAT => panic!("NOT SUPPORTED"),
                _ => return Err(()),
            }
            return Ok(());
        }

        if op_code == FILE_OP_OPEN {
            let path_ptr = op.arg0 as *const u8;
            let path_len = op.arg1 as usize;
            let path_str = unsafe {
                core::str::from_utf8(core::slice::from_raw_parts(path_ptr, path_len)).map_err(|_| ())?
            };
            crate::kprintln!("Open path \"{}\"", path_str);
            match prepare_file_path(path_str) {
                Ok((driver_id, path)) => {
                    self.pending_ops.push_back(op);
                    self.driver_id = Some(driver_id);
                    if let Some(result) = driver_open(driver_id, path) {
                        self.op_completed(0, result);
                    }
                },
                Err(_) => {
                    op.complete_with_result(Err(IOError::NotFound));
                },
            }
            return Ok(());
        }

        Err(())
    }
}

fn prepare_file_path(raw_path: &str) -> Result<(DriverID, Path), ()> {
    if Path::is_absolute(raw_path) {
        let (drive_name, path_portion) = Path::split_absolute_path(raw_path).ok_or(())?;
        let driver_id = get_driver_id_by_name(drive_name).ok_or(())?;

        Ok((driver_id, Path::from_str(path_portion)))
    } else {
        let (current_drive_id, mut working_dir) = {
            let task_lock = get_current_task();
            let task = task_lock.read();
            // TODO: task doesn't have a DriverID compatible current drive!
            (DriverID::new(0), task.working_dir.clone())
        };
        working_dir.push(raw_path);
        Ok((current_drive_id, working_dir))
    }
}

