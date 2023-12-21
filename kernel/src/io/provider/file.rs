use idos_api::io::error::IOError;
use crate::{
    io::{
        async_io::{AsyncOp, OPERATION_FLAG_FILE, FILE_OP_OPEN, FILE_OP_READ, AsyncOpQueue, OpIdGenerator, AsyncOpID},
        driver::comms::IOResult,
        filesystem::{get_driver_id_by_name, driver::DriverID, driver_open, driver_read, driver_write},
    },
    files::path::Path,
    task::{switching::{get_current_task, get_current_id}, id::TaskID},
};
use super::IOProvider;

/// Inner contents of a handle that is bound to a file for reading/writing
pub struct FileIOProvider {
    next_op_id: OpIdGenerator,
    pending_ops: AsyncOpQueue,
    driver_id: Option<DriverID>,
    source_id: TaskID,
    bound_instance: Option<u32>,
}

impl FileIOProvider {
    pub fn new(source_id: TaskID) -> Self {
        Self {
            next_op_id: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
            driver_id: None,
            source_id,
            bound_instance: None,
        }
    }

    pub fn bound(source_id: TaskID, driver_id: DriverID, bound_instance: u32) -> Self {
        Self {
            next_op_id: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
            driver_id: Some(driver_id),
            source_id,
            bound_instance: Some(bound_instance),
        }
    }

    pub fn is_bound(&self) -> bool {
        self.bound_instance.is_some()
    }

    pub fn set_task(&mut self, source_id: TaskID) {
        self.source_id = source_id;
    }

    pub fn close(&mut self) {
        // TODO: implement this?
        crate::kprintln!("CLOSE FILE");
    }

    pub fn op_completed(&mut self, index: u32, id: AsyncOpID, result: IOResult) {
        let op = match self.pending_ops.remove(id) {
            Some(op) => op,
            None => return,
        };
        if op.op_code & 0xffff == FILE_OP_OPEN {
            if let Ok(value) = result {
                self.bound_instance = Some(value);
                op.complete(1);
                if !self.pending_ops.is_empty() {
                    return self.run_first_op(index);
                }
                return;
            }
        }
        op.complete_with_result(result);
        if !self.pending_ops.is_empty() {
            return self.run_first_op(index);
        }
    }

    pub fn run_first_op(&mut self, index: u32) {
        let (id, code, arg0, arg1, arg2) = match self.pending_ops.peek() {
            Some((id, op)) => (*id, op.op_code, op.arg0, op.arg1, op.arg2),
            None => return,
        };
        let op_code = code & 0xffff;

        let completion: Option<IOResult> = if let Some(instance) = self.bound_instance {
            match op_code {
                FILE_OP_READ => {
                    let buffer_ptr = arg0 as *mut u8;
                    let buffer_len = arg1 as usize;
                    let buffer = unsafe {
                        core::slice::from_raw_parts_mut(buffer_ptr, buffer_len)
                    };

                    let driver_id = self.driver_id.unwrap();
                    if let Some(result) = driver_read(driver_id, instance, buffer, (self.source_id, index, id)) {
                        Some(result)
                    } else {
                        None
                    }
                },
                FILE_OP_WRITE => {
                    let buffer_ptr = arg0 as *const u8;
                    let buffer_len = arg1 as usize;
                    let buffer = unsafe {
                        core::slice::from_raw_parts(buffer_ptr, buffer_len)
                    };
                    let driver_id = self.driver_id.unwrap();
                    if let Some(result) = driver_write(driver_id, instance, buffer, (self.source_id, index, id)) {
                        Some(result)
                    } else {
                        None
                    }
                },
                FILE_OP_SEEK => panic!("NOT SUPPORTED"),
                FILE_OP_STAT => panic!("NOT SUPPORTED"),
                _ => {
                    Some(Err(IOError::OperationFailed))
                },
            }
        } else if op_code == FILE_OP_OPEN {
            let path_ptr = arg0 as *const u8;
            let path_len = arg1 as usize;
            let try_path_str = unsafe {
                core::str::from_utf8(core::slice::from_raw_parts(path_ptr, path_len))
            };
            match try_path_str {
                Ok(path_str) => {
                    crate::kprintln!("Open path \"{}\"", path_str);
                    match prepare_file_path(path_str) {
                        Ok((driver_id, path)) => {
                            self.driver_id = Some(driver_id);
                            if let Some(result) = driver_open(driver_id, path, (self.source_id, index, id)) {
                                Some(result)
                            } else {
                                None
                            }
                        },
                        Err(_) => {
                            Some(Err(IOError::NotFound))
                        },
                    }
                },
                Err(_) => {
                    Some(Err(IOError::NotFound))
                },
            }
        } else {
            Some(Err(IOError::OperationFailed))
        };

        match completion {
            Some(result) => {
                return self.op_completed(index, id, result);
            },
            None => (),
        }
    }
}

impl IOProvider for FileIOProvider {
    fn add_op(&mut self, index: u32, op: AsyncOp) -> Result<AsyncOpID, ()> {
        if op.op_code & OPERATION_FLAG_FILE == 0 {
            return Err(());
        }

        let id = self.next_op_id.next_id();
        self.pending_ops.push(id, op);

        if self.pending_ops.len() == 1 {
            self.run_first_op(index);
        }

        Ok(id)
    }
}

fn prepare_file_path(raw_path: &str) -> Result<(DriverID, Path), ()> {
    if Path::is_absolute(raw_path) {
        let (drive_name, path_portion) = Path::split_absolute_path(raw_path).ok_or(())?;
        let driver_id = if drive_name == "DEV" {
            get_driver_id_by_name(&path_portion[1..]).ok_or(())?
        } else {
            get_driver_id_by_name(drive_name).ok_or(())?
        };

        Ok((driver_id, Path::from_str(path_portion)))
    } else {
        let (current_drive_id, mut working_dir): (DriverID, Path) = {
            let task_lock = get_current_task();
            let task = task_lock.read();
            // TODO: task doesn't have a DriverID compatible current drive!
            panic!("Task doesn't have a current drive for the handle API!");
            //(task.current_drive, task.working_dir.clone())
        };
        working_dir.push(raw_path);
        Ok((current_drive_id, working_dir))
    }
}

