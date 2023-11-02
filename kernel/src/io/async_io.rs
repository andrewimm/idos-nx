use core::sync::atomic::{AtomicU32, Ordering};

use alloc::{collections::BTreeMap, sync::Arc};
use spin::Mutex;

use crate::{memory::{address::{PhysicalAddress, VirtualAddress}, virt::scratch::UnmappedPage}, task::{id::TaskID, messaging::MessageQueue}};

use super::provider::{task::TaskIOProvider, IOProvider, message::MessageIOProvider, file::FileIOProvider};

pub enum IOType {
    ChildTask(TaskIOProvider),
    MessageQueue(MessageIOProvider),
    File(FileIOProvider),
}

impl IOType {
    pub fn add_op(&mut self, index: u32, op: AsyncOp) -> Result<(), ()> {
        match self {
            Self::ChildTask(io) => io.add_op(index, op),
            Self::MessageQueue(io) => io.add_op(index, op),
            Self::File(io) => io.add_op(index, op),
            _ => panic!("Not implemented"),
        }
    }
}

// Op Codes use the top 16 bits to indicate the handle type they modify
pub const OPERATION_FLAG_FILE: u32 = 0x80000000;
pub const OPERATION_FLAG_TASK: u32 = 0x40000000;
pub const OPERATION_FLAG_INTERRUPT: u32 = 0x20000000;
pub const OPERATION_FLAG_MESSAGE: u32 = 0x10000000;
pub const OPERATION_FLAG_SOCKET: u32 = 0x08000000;

pub const FILE_OP_OPEN: u32 = 1;
pub const FILE_OP_READ: u32 = 2;
pub const FILE_OP_WRITE: u32 = 3;
pub const FILE_OP_SEEK: u32 = 4;
pub const FILE_OP_STAT: u32 = 5;

pub const TASK_OP_WAIT: u32 = 1;

pub const SOCKET_OP_BIND: u32 = 1;
pub const SOCKET_OP_READ: u32 = 2;
pub const SOCKET_OP_WRITE: u32 = 3;

pub const MESSAGE_OP_READ: u32 = 2;

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

    pub fn add_io(&mut self, io_type: IOType) -> u32 {
        let entry = AsyncIOTableEntry {
            ref_count: AtomicU32::new(1),
            io_type: Arc::new(Mutex::new(io_type)),
        };
        let index = self.next.fetch_add(1, Ordering::SeqCst);
        self.inner.insert(index, entry);
        index
    }

    pub fn add_op(&mut self, index: u32, op: AsyncOp) -> Result<(), ()> {
        let entry = self.inner.get_mut(&index).ok_or(())?;
        entry.io_type.lock().add_op(index, op);
        Ok(())
    }

    pub fn get(&self, index: u32) -> Option<&AsyncIOTableEntry> {
        self.inner.get(&index)
    }

    /// convenience method to get first (and ideally, only) async io
    /// referencing a specific child task
    pub fn get_task_io(&mut self, id: TaskID) -> Option<Arc<Mutex<IOType>>> {
        for (_, entry) in self.inner.iter_mut() {
            let matched = match *entry.io_type.lock() {
                IOType::ChildTask(ref io) => io.matches_task(id),
                _ => false,
            };
            if matched {
                return Some(entry.io_type.clone());
            }
        }
        None
    }

    /// convenience method for handling incoming IPC messages
    pub fn handle_incoming_messages(&mut self, messages: &mut MessageQueue) {
        let current_ticks = 0;

        for (_, entry) in self.inner.iter_mut() {
            match *entry.io_type.lock() {
                IOType::MessageQueue(ref mut io) => io.check_message_queue(current_ticks, messages),
                _ => continue,
            }
        }
    }
}

pub struct AsyncIOTableEntry {
    pub ref_count: AtomicU32,
    pub io_type: Arc<Mutex<IOType>>,
}
