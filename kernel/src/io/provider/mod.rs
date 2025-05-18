use core::sync::atomic::{AtomicU32, Ordering};

use alloc::collections::VecDeque;
use idos_api::io::{error::IOError, AsyncOp};
use spin::RwLock;

use crate::{
    memory::{
        address::{PhysicalAddress, VirtualAddress},
        virt::scratch::UnmappedPage,
    },
    sync::futex::futex_wake_inner,
    task::{
        id::TaskID,
        paging::get_current_physical_address,
        switching::{get_current_task, get_task},
    },
};

use super::{
    async_io::{AsyncOpID, ASYNC_OP_CLOSE, ASYNC_OP_OPEN, ASYNC_OP_READ, ASYNC_OP_WRITE},
    handle::Handle,
};

pub mod file;
pub mod irq;
pub mod message;
pub mod socket;
pub mod task;

pub type IOResult = Result<u32, IOError>;

#[allow(unused_variables)]
pub trait IOProvider {
    /// Queue operations must be implemented by each Provider.
    /// enqueue_op adds a new op to be handled. Many providers implement a
    /// single operation queue, but some may have multiple parallel queues if,
    /// say, reads should not block writes (as is the case with sockets).
    /// The optional handle to a wake set is passed in as well. The provider is
    /// responsible for making sure that the physical address of the op signal
    /// is removed from the set when the op is completed.
    /// The method returns the unique ID of the enqueued op, which can be used
    /// to reference, complete, or cancel the op.
    fn enqueue_op(&self, provider_index: u32, op: &AsyncOp, wake_set: Option<Handle>) -> AsyncOpID;

    fn get_active_op(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)>;

    fn take_active_op(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)>;

    /// Take the first enqueued operation and make it active, if there is one
    fn pop_queued_op(&self);

    fn transform_result(&self, op_code: u32, result: IOResult) -> u32 {
        let mapped_result = if op_code & 0xffff == ASYNC_OP_OPEN {
            // Opening a handle has some funky special behavior, since we
            // extract the driver lookup instance from the result, bind it
            // to this IO provider, and then overwrite the internal
            // details of that return value with a generic success state
            match result {
                Ok(instance) => {
                    self.bind_to(instance);
                    Ok(1)
                }
                Err(e) => Err(e),
            }
        } else {
            result
        };
        match mapped_result {
            Ok(inner) => inner & 0x7fffffff,
            Err(inner) => Into::<u32>::into(inner) | 0x80000000,
        }
    }

    /// Finish an op that could not complete immediately. If the active op
    /// matches the ID for the response, complete the UnmappedAsyncOp and wake
    /// any blocked Tasks, clearing the active op.
    /// Then in a loop, pop a queued op and make it active. Then run the
    /// active op and if an immediate response is available, complete it
    /// before continuing the loop.
    fn async_complete(
        &self,
        task_id: TaskID,
        provider_index: u32,
        id: AsyncOpID,
        result: IOResult,
    ) {
        match self.get_active_op() {
            Some((active_id, _)) => {
                if active_id != id {
                    return;
                }
            }
            None => return,
        }
        let active_op = match self.take_active_op() {
            Some((_, op)) => op,
            None => return,
        };
        active_op.complete(self.transform_result(active_op.op_code, result));

        loop {
            self.pop_queued_op();
            if let Some(res) = self.run_active_op(provider_index) {
                let active_op = match self.take_active_op() {
                    Some((_, op)) => op,
                    None => return,
                };
                active_op.complete(self.transform_result(active_op.op_code, res));
            } else {
                return;
            }
        }
    }

    /// Look up the active op, and run a specific io method based on its op code
    fn run_active_op(&self, provider_index: u32) -> Option<IOResult> {
        let (id, op) = self.get_active_op()?;
        match op.op_code & 0xffff {
            ASYNC_OP_OPEN => self.open(provider_index, id, op),
            ASYNC_OP_CLOSE => self.close(provider_index, id, op),
            ASYNC_OP_READ => self.read(provider_index, id, op),
            ASYNC_OP_WRITE => self.write(provider_index, id, op),
            _ => self.extended_op(provider_index, id, op),
        }
    }

    fn bind_to(&self, instance: u32) {
        // default behavior is a no-op
    }

    fn open(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn read(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn write(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn close(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn extended_op(
        &self,
        provider_index: u32,
        id: AsyncOpID,
        op: UnmappedAsyncOp,
    ) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }
}

#[derive(Clone)]
pub struct UnmappedAsyncOp {
    pub op_code: u32,
    pub signal_address: PhysicalAddress,
    pub return_value_address: PhysicalAddress,
    pub args: [u32; 3],
    pub wake_set: Option<(TaskID, Handle)>,
}

impl UnmappedAsyncOp {
    pub fn from_op(op: &AsyncOp, wake_set: Option<(TaskID, Handle)>) -> Self {
        let signal_vaddr = VirtualAddress::new(op.signal.as_ptr() as u32);
        let return_value_vaddr = VirtualAddress::new(op.return_value.as_ptr() as u32);
        let signal_paddr =
            get_current_physical_address(signal_vaddr).expect("Tried to pass unmapped memory");
        let return_value_paddr = get_current_physical_address(return_value_vaddr)
            .expect("Tried to pass unmapped memory");
        UnmappedAsyncOp {
            op_code: op.op_code,
            signal_address: signal_paddr,
            return_value_address: return_value_paddr,
            args: [op.args[0], op.args[1], op.args[2]],
            wake_set,
        }
    }

    pub fn complete(&self, return_value: u32) {
        let mut frame_start = self.return_value_address.as_u32() & 0xfffff000;
        let return_value_offset = self.return_value_address.as_u32() & 0xfff;
        let mut unmapped_phys = PhysicalAddress::new(frame_start);
        let mut unmapped_page = UnmappedPage::map(unmapped_phys);
        unsafe {
            let ptr = (unmapped_page.virtual_address() + return_value_offset).as_ptr_mut::<u32>();
            AtomicU32::from_ptr(ptr).store(return_value, Ordering::SeqCst);
        }

        frame_start = self.signal_address.as_u32() & 0xfffff000;
        let signal_offset = self.signal_address.as_u32() & 0xfff;
        if unmapped_phys.as_u32() != frame_start {
            unmapped_phys = PhysicalAddress::new(frame_start);
            unmapped_page = UnmappedPage::map(unmapped_phys);
        }
        unsafe {
            let ptr = (unmapped_page.virtual_address() + signal_offset).as_ptr_mut::<u32>();
            AtomicU32::from_ptr(ptr).store(1, Ordering::SeqCst);
        }

        futex_wake_inner(unmapped_phys + signal_offset, 0xffffffff);

        if let Some((task_id, ws_handle)) = self.wake_set {
            let wake_set_found = get_task(task_id)
                .and_then(|task_lock| task_lock.read().wake_sets.get(ws_handle).cloned());
            if let Some(wake_set) = wake_set_found {
                wake_set.wake();
            }
        }
    }
}

/// Convenience struct to generate new Op IDs
pub struct OpIdGenerator(AtomicU32);

impl OpIdGenerator {
    pub fn new() -> Self {
        Self(AtomicU32::new(1))
    }

    pub fn next_id(&self) -> AsyncOpID {
        let next = self.0.fetch_add(1, Ordering::SeqCst);
        AsyncOpID::new(next)
    }
}

/// Stores a queue of pending Async Ops
pub struct AsyncOpQueue {
    queue: RwLock<VecDeque<(AsyncOpID, UnmappedAsyncOp)>>,
}

impl AsyncOpQueue {
    pub fn new() -> Self {
        Self {
            queue: RwLock::new(VecDeque::new()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.read().is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.read().len()
    }

    pub fn push(&self, id: AsyncOpID, op: UnmappedAsyncOp) {
        self.queue.write().push_back((id, op));
    }

    pub fn peek(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)> {
        self.queue.read().get(0).cloned()
    }

    pub fn pop(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)> {
        self.queue.write().pop_front()
    }

    pub fn find_by_id(&self, seek: AsyncOpID) -> Option<UnmappedAsyncOp> {
        for (id, op) in self.queue.read().iter() {
            if *id == seek {
                return Some(op.clone());
            }
        }
        None
    }

    pub fn remove(&self, seek: AsyncOpID) -> Option<UnmappedAsyncOp> {
        let index = self.queue.read().iter().position(|pair| pair.0 == seek)?;
        self.queue.write().remove(index).map(|pair| pair.1)
    }
}
