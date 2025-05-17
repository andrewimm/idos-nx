use super::{AsyncOpQueue, IOProvider, OpIdGenerator, UnmappedAsyncOp};
use crate::{
    io::async_io::AsyncOpID,
    memory::{
        address::{PhysicalAddress, VirtualAddress},
        virt::scratch::UnmappedPage,
    },
    task::{
        messaging::{Message, MessageQueue},
        paging::get_current_physical_address,
    },
};
use idos_api::io::AsyncOp;
use spin::RwLock;

/// Inner contents of the handle used to read IPC messages.
pub struct MessageIOProvider {
    active: RwLock<Option<(AsyncOpID, UnmappedAsyncOp)>>,
    id_gen: OpIdGenerator,
    pending_ops: AsyncOpQueue,
}

impl MessageIOProvider {
    pub fn new() -> Self {
        Self {
            active: RwLock::new(None),
            id_gen: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
        }
    }

    /// This is an ugly hack because we can't access the message queue from
    /// within the provider -- they both require access to the Task. But from
    /// within the Task, we can receive a Message AND access all providers. So
    /// for this case only, we break out of the normal enqueue_op flow.
    pub fn check_message_queue(&self, current_ticks: u32, messages: &mut MessageQueue) {
        if self.active.read().is_none() {
            return;
        }
        loop {
            if self.active.read().is_none() {
                return;
            }
            let (first_message, has_more) = messages.read(current_ticks);
            let packet = match first_message {
                Some(packet) => packet,
                None => return,
            };
            let (sender, message) = packet.open();
            let (_, op) = self.active.read().clone().unwrap();
            let message_paddr = op.args[0];
            let phys_frame_start = message_paddr & 0xfffff000;
            let unmapped_phys = PhysicalAddress::new(phys_frame_start);
            let unmapped_page = UnmappedPage::map(unmapped_phys);
            let message_offset = message_paddr & 0xfff;
            unsafe {
                let ptr =
                    (unmapped_page.virtual_address() + message_offset).as_ptr_mut::<Message>();
                core::ptr::write_volatile(ptr, message);
            }
            self.pop_queued_op();
            op.complete(sender.into());

            if !has_more {
                return;
            }
        }
    }
}

impl IOProvider for MessageIOProvider {
    fn enqueue_op(&self, provider_index: u32, op: &AsyncOp) -> AsyncOpID {
        // convert the virtual address of the message pointer to a physical
        // address
        // TODO: if the message spans two physical pages, we're gonna have a problem!
        let message_size = core::mem::size_of::<Message>() as u32;
        if (op.args[0] & 0xfffff000) != ((op.args[0] + message_size) & 0xfffff000) {
            panic!("Messages can't bridge multiple pages (yet)");
        }
        let message_virt = VirtualAddress::new(op.args[0]);
        let message_phys = get_current_physical_address(message_virt)
            .expect("Tried to reference unmapped address");

        let id = self.id_gen.next_id();
        let mut unmapped = UnmappedAsyncOp::from_op(op);
        unmapped.args[0] = message_phys.as_u32();
        if self.active.read().is_some() {
            self.pending_ops.push(id, unmapped);
            return id;
        }

        *self.active.write() = Some((id, unmapped));
        /*
        match self.run_active_op(provider_index) {
            Some(result) => {
                *self.active.write() = None;
                let return_value = match result {
                    Ok(inner) => inner & 0x7fffffff,
                    Err(inner) => Into::<u32>::into(inner) | 0x80000000,
                };
                op.return_value.store(return_value, Ordering::SeqCst);
                op.signal.store(1, Ordering::SeqCst);
            }
            None => (),
        }
        */

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
        None
    }
}
