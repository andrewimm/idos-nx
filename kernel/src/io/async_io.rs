use core::sync::atomic::{AtomicU32, Ordering};

use alloc::{collections::BTreeMap, sync::Arc};
use idos_api::io::AsyncOp;

use crate::task::id::TaskID;

use super::{
    handle::Handle,
    provider::{
        file::FileIOProvider, irq::InterruptIOProvider, message::MessageIOProvider,
        socket::SocketIOProvider, task::TaskIOProvider, IOProvider,
    },
};

pub enum IOType {
    ChildTask(TaskIOProvider),
    MessageQueue(MessageIOProvider),
    File(FileIOProvider),
    Interrupt(InterruptIOProvider),
    Socket(SocketIOProvider),
}

impl IOType {
    pub fn inner(&self) -> &dyn IOProvider {
        match self {
            Self::ChildTask(io) => io,
            Self::MessageQueue(io) => io,
            Self::File(io) => io,
            Self::Interrupt(io) => io,
            Self::Socket(io) => io,
        }
    }

    pub fn op_request(&self, index: u32, op: &AsyncOp, wake_set: Option<Handle>) -> AsyncOpID {
        let provider = self.inner();
        provider.enqueue_op(index, op, wake_set)
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

pub const SOCKET_OP_BROADCAST: u32 = 6;
pub const SOCKET_OP_MULTICAST: u32 = 7;
pub const SOCKET_OP_ACCEPT: u32 = 8;

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

    pub fn get_message_io(&self) -> Option<(u32, Arc<IOType>)> {
        for (io_index, entry) in self.inner.iter() {
            let matched = match *entry.io_type {
                IOType::MessageQueue(_) => true,
                _ => false,
            };
            if matched {
                return Some((*io_index, entry.io_type.clone()));
            }
        }
        None
    }
}

pub struct AsyncIOTableEntry {
    pub ref_count: AtomicU32,
    pub io_type: Arc<IOType>,
}
