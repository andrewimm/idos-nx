use crate::task::id::TaskID;
use crate::io::async_io::{AsyncOp, AsyncOpID, OpIdGenerator, AsyncOpQueue};
use super::IOProvider;

/// Inner contents of the handle generated when a child task is spawned. This
/// can be used to listen for status changes in the child task, such as when it
/// exits.
pub struct TaskIOProvider {
    child_id: TaskID,
    exit_code: Option<u32>,

    pending_ops: AsyncOpQueue,
}

impl TaskIOProvider {
    pub fn for_task(id: TaskID) -> Self {
        Self {
            child_id: id,
            exit_code: None,
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

    fn read(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<super::IOResult> {
        if let Some(code) = self.exit_code {
            return Some(Ok(code));
        }
        None
    }
}

