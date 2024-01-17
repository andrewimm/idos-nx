//! Provider for hardware interrupts that come from the PIC chip

use crate::io::async_io::{OpIdGenerator, AsyncOpQueue, AsyncOp, AsyncOpID};
use crate::interrupts::pic::{is_interrupt_active, acknowledge_interrupt};
use super::IOProvider;

/// Inner contents of the handle used to read IPC messages.
pub struct InterruptIOProvider {
    irq: u8,
    pending_ops: AsyncOpQueue,
}

impl InterruptIOProvider {
    pub fn new(irq: u8) -> Self {
        Self {
            irq,
            pending_ops: AsyncOpQueue::new(),
        }
    }

    pub fn interrupt_notify(&mut self) -> bool {
        let mut op_completed = false;
        loop {
            if let Some((_id, op)) = self.pending_ops.pop() {
                op.complete(1);
                op_completed = true;
            } else {
                break;
            }
        }
        op_completed
    }
}

impl IOProvider for InterruptIOProvider {
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

    /// `read`ing an irq listens for the interrupt
    fn read(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<super::IOResult> {
        if is_interrupt_active(self.irq) {
            return Some(Ok(1));
        }
        None
    }

    /// `write` acknowledges the irq, allowing it to be triggered again
    fn write(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<super::IOResult> {
        acknowledge_interrupt(self.irq);
        Some(Ok(1))
    }
}
