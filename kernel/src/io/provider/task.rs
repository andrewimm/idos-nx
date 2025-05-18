use core::sync::atomic::Ordering;

use idos_api::io::AsyncOp;
use spin::RwLock;

use super::{AsyncOpQueue, IOProvider, OpIdGenerator, UnmappedAsyncOp};
use crate::io::async_io::AsyncOpID;
use crate::io::handle::Handle;
use crate::task::id::TaskID;
use crate::task::switching::get_current_id;

/// Inner contents of the handle generated when a child task is spawned. This
/// can be used to listen for status changes in the child task, such as when it
/// exits.
pub struct TaskIOProvider {
    child_id: TaskID,
    exit_code: RwLock<Option<u32>>,

    active: RwLock<Option<(AsyncOpID, UnmappedAsyncOp)>>,
    id_gen: OpIdGenerator,
    pending_ops: AsyncOpQueue,
}

impl TaskIOProvider {
    pub fn for_task(id: TaskID) -> Self {
        Self {
            child_id: id,
            exit_code: RwLock::new(None),

            active: RwLock::new(None),
            id_gen: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
        }
    }

    pub fn matches_task(&self, id: TaskID) -> bool {
        self.child_id == id
    }

    pub fn task_exited(&self, host_task: TaskID, provider_index: u32, code: u32) {
        self.exit_code.write().replace(code);
        let id = match *self.active.read() {
            Some((id, _)) => id,
            None => return,
        };
        self.async_complete(host_task, provider_index, id, Ok(code));
    }
}

impl IOProvider for TaskIOProvider {
    fn enqueue_op(&self, provider_index: u32, op: &AsyncOp, wake_set: Option<Handle>) -> AsyncOpID {
        let id = self.id_gen.next_id();
        let unmapped =
            UnmappedAsyncOp::from_op(op, wake_set.map(|handle| (get_current_id(), handle)));
        if self.active.read().is_some() {
            self.pending_ops.push(id, unmapped);
            return id;
        }

        *self.active.write() = Some((id, unmapped));
        match self.run_active_op(provider_index) {
            Some(result) => {
                *self.active.write() = None;
                let return_value = self.transform_result(op.op_code, result);
                op.return_value.store(return_value, Ordering::SeqCst);
                op.signal.store(1, Ordering::SeqCst);
            }
            None => (),
        }
        id
    }

    fn get_active_op(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)> {
        self.active.read().clone()
    }

    fn take_active_op(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)> {
        self.active.write().take()
    }

    fn pop_queued_op(&self) {
        let next = self.pending_ops.pop();
        *self.active.write() = next;
    }

    fn read(
        &self,
        _provider_index: u32,
        _id: AsyncOpID,
        _op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        if let Some(code) = *self.exit_code.read() {
            return Some(Ok(code));
        }
        None
    }
}
