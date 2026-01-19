//! Provider for hardware interrupts that come from the PIC chip

use core::sync::atomic::Ordering;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use idos_api::io::error::IoResult;
use idos_api::io::AsyncOp;
use spin::RwLock;

use super::{AsyncOpQueue, IOProvider, OpIdGenerator, UnmappedAsyncOp};
use crate::interrupts::pic::{acknowledge_interrupt, is_interrupt_active};
use crate::io::async_io::AsyncOpID;
use crate::io::handle::Handle;
use crate::task::id::TaskID;
use crate::task::switching::get_current_id;

/// Inner contents of the handle used to read IPC messages.
pub struct InterruptIOProvider {
    irq: u8,

    id_gen: OpIdGenerator,
    pending_ops: RwLock<BTreeMap<AsyncOpID, UnmappedAsyncOp>>,
}

impl InterruptIOProvider {
    pub fn new(irq: u8) -> Self {
        Self {
            irq,

            id_gen: OpIdGenerator::new(),
            pending_ops: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn interrupt_fired(&self) {
        let ids = self.pending_ops.read().keys().cloned().collect::<Vec<_>>();
        for id in ids {
            self.async_complete(id, Ok(1));
        }
    }
}

impl IOProvider for InterruptIOProvider {
    fn add_op(
        &self,
        provider_index: u32,
        op: &AsyncOp,
        args: [u32; 3],
        wake_set: Option<Handle>,
    ) -> AsyncOpID {
        let id = self.id_gen.next_id();
        let unmapped =
            UnmappedAsyncOp::from_op(op, args, wake_set.map(|handle| (get_current_id(), handle)));
        self.pending_ops.write().insert(id, unmapped);

        match self.run_op(provider_index, id) {
            Some(result) => {
                self.remove_op(id);
                let return_value = self.transform_result(op.op_code, result);
                op.return_value.store(return_value, Ordering::SeqCst);
                op.signal.store(1, Ordering::SeqCst);
            }
            None => (),
        }
        id
    }

    fn get_op(&self, id: AsyncOpID) -> Option<UnmappedAsyncOp> {
        self.pending_ops.read().get(&id).cloned()
    }

    fn remove_op(&self, id: AsyncOpID) -> Option<UnmappedAsyncOp> {
        self.pending_ops.write().remove(&id)
    }

    /// `read`ing an irq listens for the interrupt
    fn read(&self, _provider_index: u32, _id: AsyncOpID, _op: UnmappedAsyncOp) -> Option<IoResult> {
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
    ) -> Option<IoResult> {
        acknowledge_interrupt(self.irq);
        Some(Ok(1))
    }
}
