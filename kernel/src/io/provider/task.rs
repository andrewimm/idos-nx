use alloc::collections::VecDeque;
use crate::{task::id::TaskID, io::async_io::{AsyncOp, OPERATION_FLAG_TASK, TASK_OP_WAIT}};
use super::IOProvider;

/// Inner contents of the handle generated when a child task is spawned. This
/// can be used to listen for status changes in the child task, such as when it
/// exits.
pub struct TaskIOProvider {
    child_id: TaskID,
    exit_code: Option<u32>,

    pending_ops: VecDeque<AsyncOp>
}

impl TaskIOProvider {
    pub fn for_task(id: TaskID) -> Self {
        Self {
            child_id: id,
            exit_code: None,
            pending_ops: VecDeque::new(),
        }
    }

    pub fn matches_task(&self, id: TaskID) -> bool {
        self.child_id == id
    }

    pub fn task_exited(&mut self, code: u32) {
        self.exit_code = Some(code);
        for op in self.pending_ops.iter() {
            op.complete(code);
        }

        self.pending_ops.clear();
    }
}

impl IOProvider for TaskIOProvider {
    fn add_op(&mut self, op: AsyncOp) -> Result<(), ()> {
        if op.op_code & OPERATION_FLAG_TASK == 0 {
            return Err(());
        }

        match op.op_code & 0xffff {
            TASK_OP_WAIT => {
                if let Some(code) = self.exit_code {
                    // immediately complete op without queueing
                    op.complete(code);
                    return Ok(());
                }

                self.pending_ops.push_back(op);
                Ok(())
            },
            _ => Err(()), // unsupported op
        }
    }
}

