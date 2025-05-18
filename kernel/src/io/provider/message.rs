use core::sync::atomic::Ordering;

use super::{AsyncOpQueue, IOProvider, OpIdGenerator, UnmappedAsyncOp};
use crate::{
    io::{async_io::AsyncOpID, handle::Handle},
    memory::{
        address::{PhysicalAddress, VirtualAddress},
        virt::scratch::UnmappedPage,
    },
    task::{
        id::TaskID,
        messaging::{Message, MessagePacket, MessageQueue},
        paging::get_current_physical_address,
        switching::{get_current_id, get_task},
    },
};
use idos_api::io::AsyncOp;
use spin::RwLock;

/// Inner contents of the handle used to read IPC messages.
pub struct MessageIOProvider {
    task_id: TaskID,
    active: RwLock<Option<(AsyncOpID, UnmappedAsyncOp)>>,
    id_gen: OpIdGenerator,
    pending_ops: AsyncOpQueue,
}

impl MessageIOProvider {
    pub fn for_task(task_id: TaskID) -> Self {
        Self {
            task_id,
            active: RwLock::new(None),
            id_gen: OpIdGenerator::new(),
            pending_ops: AsyncOpQueue::new(),
        }
    }

    pub fn pop_message(&self) -> Option<MessagePacket> {
        let current_ticks = 0;
        let task_lock = get_task(self.task_id)?;
        let (first_message, _has_more) = {
            let mut task_guard = task_lock.write();
            task_guard.message_queue.read(current_ticks)
        };
        first_message
    }

    pub fn check_messages(&self) {
        let message_paddr = {
            let active = self.active.read();
            match *active {
                Some((_, ref op)) => op.args[0],
                None => return,
            }
        };
        let packet = match self.pop_message() {
            Some(packet) => packet,
            None => return,
        };
        let (sender, message) = packet.open();
        Self::copy_message(message_paddr, message);

        let active_op = match self.take_active_op() {
            Some((_, op)) => op,
            None => return,
        };
        active_op.complete(sender.into());
        if let Some(ws_handle) = active_op.wake_set {
            //remove_address_from_wake_set(task_id, ws_handle, active_op.signal_address);
        }
    }

    pub fn copy_message(message_paddr: u32, message: Message) {
        let phys_frame_start = message_paddr & 0xfffff000;
        let unmapped_phys = PhysicalAddress::new(phys_frame_start);
        let unmapped_page = UnmappedPage::map(unmapped_phys);
        let message_offset = message_paddr & 0xfff;
        unsafe {
            let ptr = (unmapped_page.virtual_address() + message_offset).as_ptr_mut::<Message>();
            core::ptr::write_volatile(ptr, message);
        }
    }
}

impl IOProvider for MessageIOProvider {
    fn enqueue_op(&self, provider_index: u32, op: &AsyncOp, wake_set: Option<Handle>) -> AsyncOpID {
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
        let mut unmapped =
            UnmappedAsyncOp::from_op(op, wake_set.map(|handle| (get_current_id(), handle)));
        unmapped.args[0] = message_phys.as_u32();
        if self.active.read().is_some() {
            self.pending_ops.push(id, unmapped);
            return id;
        }

        *self.active.write() = Some((id, unmapped));
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
        provider_index: u32,
        id: AsyncOpID,
        op: UnmappedAsyncOp,
    ) -> Option<super::IOResult> {
        let packet = self.pop_message()?;
        let (sender, message) = packet.open();
        Self::copy_message(op.args[0], message);
        Some(Ok(sender.into()))
    }
}
