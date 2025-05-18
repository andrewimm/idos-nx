//! Provider for hardware interrupts that come from the PIC chip

use core::sync::atomic::Ordering;

use idos_api::io::AsyncOp;
use spin::RwLock;

use super::{AsyncOpQueue, IOProvider, OpIdGenerator, UnmappedAsyncOp};
use crate::interrupts::pic::{acknowledge_interrupt, is_interrupt_active};
use crate::io::async_io::AsyncOpID;
use crate::io::handle::Handle;

/// Inner contents of the handle used to read IPC messages.
pub struct InterruptIOProvider {
    irq: u8,

    active: RwLock<Option<(AsyncOpID, UnmappedAsyncOp)>>,
    id_gen: OpIdGenerator,
    pending_ops: AsyncOpQueue,
}

impl InterruptIOProvider {
    pub fn new(irq: u8) -> Self {
        Self {
            irq,

            active: RwLock::new(None),
            id_gen: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
        }
    }
}

impl IOProvider for InterruptIOProvider {
    fn enqueue_op(&self, provider_index: u32, op: &AsyncOp, wake_set: Option<Handle>) -> AsyncOpID {
        let id = self.id_gen.next_id();
        let unmapped = UnmappedAsyncOp::from_op(op, wake_set);
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

    /// `read`ing an irq listens for the interrupt
    fn read(
        &self,
        _provider_index: u32,
        _id: AsyncOpID,
        _op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        if is_interrupt_active(self.irq) {
            return Some(Ok(1));
        }
        None
    }

    /// `write` acknowledges the irq, allowing it to be triggered again
    fn write(
        &self,
        _provider_index: u32,
        _id: AsyncOpID,
        _op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        acknowledge_interrupt(self.irq);
        Some(Ok(1))
    }
}
