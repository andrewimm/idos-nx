use crate::task::id::TaskID;
use crate::io::async_io::{AsyncOp, OPERATION_FLAG_TASK, TASK_OP_WAIT, AsyncOpID, OpIdGenerator, AsyncOpQueue};
use super::IOProvider;

/// Inner contents of the handle generated when a child task is spawned. This
/// can be used to listen for status changes in the child task, such as when it
/// exits.
pub struct TaskIOProvider {
    child_id: TaskID,
    exit_code: Option<u32>,

    next_op_id: OpIdGenerator,
    pending_ops: AsyncOpQueue,
}

impl TaskIOProvider {
    pub fn for_task(id: TaskID) -> Self {
        Self {
            child_id: id,
            exit_code: None,
            next_op_id: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
        }
    }

    pub fn matches_task(&self, id: TaskID) -> bool {
        self.child_id == id
    }

    pub fn task_exited(&mut self, code: u32) {
        self.exit_code = Some(code);
        loop {
            match self.pending_ops.pop() {
                Some((_, op)) => op.complete(code),
                None => break,
            }
        }
    }
}

impl IOProvider for TaskIOProvider {
    fn add_op(&mut self, _index: u32, op: AsyncOp) -> Result<AsyncOpID, ()> {
        if op.op_code & OPERATION_FLAG_TASK == 0 {
            return Err(());
        }

        match op.op_code & 0xffff {
            TASK_OP_WAIT => {
                let id = self.next_op_id.next_id();
                if let Some(code) = self.exit_code {
                    // immediately complete op without queueing
                    op.complete(code);
                    return Ok(id);
                }

                self.pending_ops.push(id, op);
                Ok(id)
            },
            _ => Err(()), // unsupported op
        }
    }
}

