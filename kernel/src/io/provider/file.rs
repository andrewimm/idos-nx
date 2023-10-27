use alloc::{collections::VecDeque, string::String};
use crate::{io::{async_io::{AsyncOp, OPERATION_FLAG_FILE, FILE_OP_OPEN, FILE_OP_READ}, filesystem::{get_driver_id_by_name, driver::DriverID, send_driver_io_request}}, files::path::Path, task::switching::{get_current_task, get_current_id}};
use super::IOProvider;

/// Inner contents of a handle that is bound to a file for reading/writing
pub struct FileIOProvider {
    pending_ops: VecDeque<AsyncOp>
}

impl FileIOProvider {
    pub fn new() -> Self {
        Self {
            pending_ops: VecDeque::new(),
        }
    }

    pub fn is_bound(&self) -> bool {
        false
    }

    pub fn op_completed(&mut self, id: u32, result: u32) {
        // find the op
        // for now, just pull the first
        if self.pending_ops.is_empty() {
            return;
        }
        let op = self.pending_ops.pop_front().unwrap();
        op.complete(result);
    }
}

impl IOProvider for FileIOProvider {
    fn add_op(&mut self, index: u32, op: AsyncOp) -> Result<(), ()> {
        if op.op_code & OPERATION_FLAG_FILE == 0 {
            return Err(());
        }

        let op_code = op.op_code & 0xffff;

        if self.is_bound() {
            match op_code {
                FILE_OP_READ => panic!("NOT SUPPORTED"),
                FILE_OP_WRITE => panic!("NOT SUPPORTED"),
                FILE_OP_SEEK => panic!("NOT SUPPORTED"),
                FILE_OP_STAT => panic!("NOT SUPPORTED"),
                _ => return Err(()),
            }
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
                    self.pending_ops.push_back(op.clone());
                    send_driver_io_request(get_current_id(), driver_id, op);
                },
                Err(_) => {
                    op.complete(0xffffffff);
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
        let driver_id = get_driver_id_by_name(drive_name).map_err(|_| ())?;

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

