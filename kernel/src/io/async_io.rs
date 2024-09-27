use core::sync::atomic::{AtomicU32, Ordering};

use alloc::{collections::{BTreeMap, VecDeque}, sync::Arc};
use spin::RwLock;

use crate::{memory::{address::{PhysicalAddress, VirtualAddress}, virt::scratch::UnmappedPage}, task::{id::TaskID, messaging::MessageQueue}};

use super::provider::{task::TaskIOProvider, IOProvider, message::MessageIOProvider, file::FileIOProvider, irq::InterruptIOProvider};

pub enum IOType {
    ChildTask(TaskIOProvider),
    MessageQueue(MessageIOProvider),
    File(FileIOProvider),
    Interrupt(InterruptIOProvider),
}

impl IOType {
    pub fn op_request(&self, index: u32, op: AsyncOp) -> Result<AsyncOpID, ()> {
        let provider: &dyn IOProvider = match self {
            Self::ChildTask(io) => io,
            Self::MessageQueue(io) => io,
            Self::File(io) => io,
            Self::Interrupt(io) => io,
        };
        provider.op_request(index, op)
    }

    pub fn set_task(&self, task: TaskID) {
        match self {
            Self::File(io) => io.set_task(task),
            _ => (),
        }
    }
}

// Op Codes use the top 16 bits to indicate the handle type they modify
// Not in use right now, maybe valuable later?
pub const OPERATION_FLAG_FILE: u32 = 0x80000000;
pub const OPERATION_FLAG_TASK: u32 = 0x40000000;
pub const OPERATION_FLAG_INTERRUPT: u32 = 0x20000000;
pub const OPERATION_FLAG_MESSAGE: u32 = 0x10000000;
pub const OPERATION_FLAG_SOCKET: u32 = 0x08000000;

pub const ASYNC_OP_OPEN: u32 = 1;
pub const ASYNC_OP_READ: u32 = 2;
pub const ASYNC_OP_WRITE: u32 = 3;
pub const ASYNC_OP_CLOSE: u32 = 4;

pub const FILE_OP_STAT: u32 = 5;

/// All async operations on handles are performed by passing an AsyncOp object
/// to the kernel. The fields are used to determine which action to take.
#[derive(Clone)]
pub struct AsyncOp {
    /// A field containing a type flag and an operation identifier
    pub op_code: u32,
    /// Address of an atomic u32 that is used to indicate when the operation
    /// has completed.
    pub semaphore: PhysicalAddress,
    /// Address of a u32 that is used to write the return value
    pub return_value: PhysicalAddress,
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
}

impl AsyncOp {
    pub fn new(op_code: u32, semaphore_addr: VirtualAddress, return_value_addr: VirtualAddress, arg0: u32, arg1: u32, arg2: u32) -> Self {
        let semaphore = crate::task::paging::get_current_physical_address(semaphore_addr).unwrap();
        let return_value = crate::task::paging::get_current_physical_address(return_value_addr).unwrap();

        Self {
            op_code,
            semaphore,
            return_value,
            arg0,
            arg1,
            arg2,
        }
    }

    pub fn complete_with_result<E: Into<u32>>(&self, result: Result<u32, E>) {
        let value = match result {
            Ok(inner) => inner & 0x7fffffff,
            Err(inner) => Into::<u32>::into(inner) | 0x80000000,
        };
        self.complete(value);
    }

    pub fn complete(&self, return_value: u32) {
        // if this becomes configurable, make it an argument
        let semaphore_value = 1;

        let mut phys_frame_start = self.return_value.as_u32() & 0xfffff000;
        let mut unmapped_phys = PhysicalAddress::new(phys_frame_start);
        let mut unmapped_for_dir = UnmappedPage::map(unmapped_phys);
        let return_value_offset = self.return_value.as_u32() & 0xfff;
        unsafe {
            let ptr = (unmapped_for_dir.virtual_address() + return_value_offset).as_ptr_mut::<u32>();
            core::ptr::write_volatile(ptr, return_value);
        }

        phys_frame_start = self.semaphore.as_u32() & 0xfffff000;
        let semaphore_offset = self.semaphore.as_u32() & 0xfff;
        if unmapped_phys.as_u32() != phys_frame_start {
            unmapped_phys = PhysicalAddress::new(phys_frame_start);
            unmapped_for_dir = UnmappedPage::map(unmapped_phys);
        }
        unsafe {
            let ptr = (unmapped_for_dir.virtual_address() + semaphore_offset).as_ptr::<AtomicU32>();
            (&*ptr).store(semaphore_value, Ordering::SeqCst);
        }
    }
}

/// When an op is added to an open IO instance, it is given a unique identifier
/// This can be used to cancel or complete the operation from an outside source
/// like an async driver.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AsyncOpID(u32);

impl AsyncOpID {
    pub fn new(inner: u32) -> Self {
        Self(inner)
    }
}

impl core::ops::Deref for AsyncOpID {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
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
    id_gen: OpIdGenerator,
    queue: RwLock<VecDeque<(AsyncOpID, AsyncOp)>>,
}

impl AsyncOpQueue {
    pub fn new() -> Self {
        Self {
            id_gen: OpIdGenerator::new(),
            queue: RwLock::new(VecDeque::new()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.read().is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.read().len()
    }

    pub fn push(&self, op: AsyncOp) -> AsyncOpID {
        let id = self.id_gen.next_id();
        self.queue.write().push_back((id, op));
        id
    }

    pub fn peek(&self) -> Option<(AsyncOpID, AsyncOp)> {
        self.queue.read().get(0).cloned()
    }

    pub fn pop(&self) -> Option<(AsyncOpID, AsyncOp)> {
        self.queue.write().pop_front()
    }

    pub fn find_by_id(&self, seek: AsyncOpID) -> Option<AsyncOp> {
        for (id, op) in self.queue.read().iter() {
            if *id == seek {
                return Some(op.clone());
            }
        }
        None
    }

    pub fn remove(&self, seek: AsyncOpID) -> Option<AsyncOp> {
        let index = self.queue.read().iter().position(|pair| pair.0 == seek)?;
        self.queue.write().remove(index).map(|pair| pair.1)
    }
}

/// An AsyncIOTable stores the data for all handles
/// Handles point to entries within the AsyncIOTable. Why the extra layer of
/// indirection? This way you can effectively `dup` a handle, having two
pub struct AsyncIOTable {
    next: AtomicU32,
    inner: BTreeMap<u32, AsyncIOTableEntry>,
}

impl AsyncIOTable {
    pub fn new() -> Self {
        Self {
            next: AtomicU32::new(1),
            inner: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, io_type: Arc<IOType>) -> u32 {
        let entry = AsyncIOTableEntry {
            ref_count: AtomicU32::new(1),
            io_type,
        };
        let index = self.next.fetch_add(1, Ordering::SeqCst);
        self.inner.insert(index, entry);
        index
    }

    pub fn add_io(&mut self, io_type: IOType) -> u32 {
        self.insert(Arc::new(io_type))
    }

    pub fn get(&self, index: u32) -> Option<&AsyncIOTableEntry> {
        self.inner.get(&index)
    }

    pub fn get_reference_count(&self, index: u32) -> Option<u32> {
        let count = self.inner.get(&index)?.ref_count.load(Ordering::SeqCst);
        Some(count)
    }

    pub fn add_reference(&self, index: u32) -> Option<u32> {
        let entry = self.inner.get(&index)?;
        let count = entry.ref_count.fetch_add(1, Ordering::SeqCst) + 1;
        Some(count)
    }

    /// If the last reference is removed, the table entry will be removed from
    /// the map as well, and returned
    pub fn remove_reference(&mut self, index: u32) -> Option<Arc<IOType>> {
        let entry = self.inner.get(&index)?;
        let count = entry.ref_count.fetch_sub(1, Ordering::SeqCst);
        if count > 1 {
            return None;
        }
        self.inner.remove(&index).map(|entry| entry.io_type)
    }

    /// convenience method to get first (and ideally, only) async io
    /// referencing a specific child task
    pub fn get_task_io(&self, id: TaskID) -> Option<(u32, Arc<IOType>)> {
        for (io_index, entry) in self.inner.iter() {
            let matched = match *entry.io_type {
                IOType::ChildTask(ref io) => io.matches_task(id),
                _ => false,
            };
            if matched {
                return Some((*io_index, entry.io_type.clone()));
            }
        }
        None
    }

    /// convenience method for handling incoming IPC messages
    /// We _explicitly_ don't support more than one Message Queue handle. Only
    /// the first one, numerically, will receive any messages from the queue.
    pub fn handle_incoming_messages(&mut self, messages: &mut MessageQueue) -> Option<u32> {
        // TODO: Fill this with the actual current ticks
        let current_ticks = 0;

        for (io_index, entry) in self.inner.iter_mut() {
            match *entry.io_type {
                IOType::MessageQueue(ref io) => {
                    io.check_message_queue(current_ticks, messages);
                    return Some(*io_index);
                },
                _ => continue,
            }
        }
        None
    }
}

pub struct AsyncIOTableEntry {
    pub ref_count: AtomicU32,
    pub io_type: Arc<IOType>,
}
