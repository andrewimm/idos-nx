//! Provider for hardware interrupts that come from the PIC chip

use crate::io::async_io::{OpIdGenerator, AsyncOpQueue, AsyncOp, AsyncOpID, INTERRUPT_OP_LISTEN, INTERRUPT_OP_ACK};
use crate::interrupts::pic::{is_interrupt_active, acknowledge_interrupt};
use super::IOProvider;

/// Inner contents of the handle used to read IPC messages.
pub struct InterruptIOProvider {
    irq: u8,
    next_id: OpIdGenerator,
    pending_ops: AsyncOpQueue,
}

impl InterruptIOProvider {
    pub fn new(irq: u8) -> Self {
        Self {
            irq,
            next_id: OpIdGenerator::new(),
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
    fn add_op(&mut self, index: u32, op: AsyncOp) -> Result<AsyncOpID, ()> {
        let id = self.next_id.next_id();
        match op.op_code & 0xffff {
            INTERRUPT_OP_LISTEN => {
                if is_interrupt_active(self.irq) {
                    op.complete(1);
                    return Ok(id);
                }
                self.pending_ops.push(id, op);
                Ok(id)
            },
            INTERRUPT_OP_ACK => {
                acknowledge_interrupt(self.irq);
                op.complete(1);
                Ok(id)
            },
            _ => Err(()),
        }
    }
}
