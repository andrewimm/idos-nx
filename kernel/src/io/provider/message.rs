use crate::{task::{id::TaskID, messaging::{MessageQueue, Message}}, io::async_io::{AsyncOp, OpIdGenerator, AsyncOpQueue, AsyncOpID}, memory::{address::{VirtualAddress, PhysicalAddress}, virt::scratch::UnmappedPage}};
use super::IOProvider;

/// Inner contents of the handle used to read IPC messages.
pub struct MessageIOProvider {
    pending_ops: AsyncOpQueue,
}

impl MessageIOProvider {
    pub fn new() -> Self {
        Self {
            pending_ops: AsyncOpQueue::new(),
        }
    }

    pub fn check_message_queue(&mut self, current_ticks: u32, messages: &mut MessageQueue) {
        while !self.pending_ops.is_empty() {
            let (first_message, has_more) = messages.read(current_ticks);
            match first_message {
                Some(packet) => {
                    let (sender, message) = packet.open();
                    let (op_id, op) = self.pending_ops.pop().unwrap();
                    // arg0 is the address of the Message
                    // return value is the ID of the sender
                    let phys_frame_start = op.arg0 & 0xfffff000;
                    let unmapped_phys = PhysicalAddress::new(phys_frame_start);
                    let unmapped_for_dir = UnmappedPage::map(unmapped_phys);
                    let message_offset = op.arg0 & 0xfff;
                    unsafe {
                        let ptr = (unmapped_for_dir.virtual_address() + message_offset).as_ptr_mut::<Message>();
                        core::ptr::write_volatile(ptr, message);
                    }
                    op.complete(sender.into());
                },
                None => return,
            }
            if !has_more {
                return;
            }
        }
        /*while !self.pending_ops.is_empty() {
            let (first_message, has_more) = messages.read(current_ticks);
            match first_message {
                Some(packet) => {
                    let (sender, message) = packet.open();
                    let (_, op) = self.pending_ops.pop().unwrap();
                    // arg0 is the address of the Message
                    // return value is the ID of the sender
                    let phys_frame_start = op.arg0 & 0xfffff000;
                    let unmapped_phys = PhysicalAddress::new(phys_frame_start);
                    let unmapped_for_dir = UnmappedPage::map(unmapped_phys);
                    let message_offset = op.arg0 & 0xfff;
                    unsafe {
                        let ptr = (unmapped_for_dir.virtual_address() + message_offset).as_ptr_mut::<Message>();
                        core::ptr::write_volatile(ptr, message);
                    }
                    op.complete(sender.into())
                },
                None => return,
            }
            if !has_more {
                return;
            }
        }*/
    }
}

impl IOProvider for MessageIOProvider {
    fn enqueue_op(&self, op: AsyncOp) -> (AsyncOpID, bool) {
        // convert the virtual address of the message pointer to a physical
        // address
        // TODO: if the message spans two physical pages, we're gonna have a problem!
        let message_size = core::mem::size_of::<Message>() as u32;
        if (op.arg0 & 0xfffff000) != ((op.arg0 + message_size) & 0xfffff000) {
            panic!("Messages can't bridge multiple pages (yet)");
        }
        let message_virt = VirtualAddress::new(op.arg0);
        let message_phys = match crate::task::paging::get_current_physical_address(message_virt) {
            Some(addr) => addr,
            None => panic!("Tried to reference unmapped address"),
        };
        let mut op_clone = op.clone();
        op_clone.arg0 = message_phys.as_u32();

        let id = self.pending_ops.push(op_clone);
        (id, true)
    }

    fn peek_op(&self) -> Option<(AsyncOpID, AsyncOp)> {
        self.pending_ops.peek()
    }

    fn remove_op(&self, id: AsyncOpID) -> Option<AsyncOp> {
        self.pending_ops.remove(id)
    }

    fn read(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<super::IOResult> {

        None
    }
}

