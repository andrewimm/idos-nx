use alloc::collections::vec_deque::VecDeque;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::collections::SlotList;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::virt::scratch::UnmappedPage;
use crate::net::socket::SocketHandle;
use super::files::OpenFile;
use super::id::TaskID;

#[derive(Copy, Clone)]
pub struct Handle(usize);

impl Handle {
    pub fn new(inner: usize) -> Self {
        Handle(inner)
    }
}

impl core::ops::Deref for Handle {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct HandleOp {
    pub op_code: u32,
    pub semaphore: PhysicalAddress,
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
}

impl HandleOp {
    pub fn new(code: u32, semaphore_addr: u32, arg0: u32, arg1: u32, arg2: u32) -> Self {
        let phys_addr = super::paging::get_current_physical_address(VirtualAddress::new(semaphore_addr)).unwrap();
        Self {
            op_code: code,
            semaphore: phys_addr,
            arg0,
            arg1,
            arg2,
        }
    }

    pub fn complete(&self, code: u32) {
        let phys_frame_start = self.semaphore.as_u32() & 0xfffff000;
        let semaphore_offset = self.semaphore.as_u32() - phys_frame_start;
        let unmapped_for_dir = UnmappedPage::map(PhysicalAddress::new(phys_frame_start));
        unsafe {
            let ptr = (unmapped_for_dir.virtual_address() + semaphore_offset).as_ptr::<AtomicU32>();
            (&*ptr).store(code, Ordering::SeqCst);
        }
    }
}

// Op Codes use the top bits to indicate the handle type they modify
pub const OPERATION_FLAG_FILE: u32 = 0x80000000;
pub const OPERATION_FLAG_TASK: u32 = 0x40000000;
pub const OPERATION_FLAG_INTERRUPT: u32 = 0x20000000;
pub const OPERATION_FLAG_MESSAGE: u32 = 0x10000000;
pub const OPERATION_FLAG_SOCKET: u32 = 0x08000000;

pub enum HandleType {
    /// A file or device
    File(Option<OpenFile>),

    /// A network socket
    Socket(SocketHandle),

    /// A task spawned from the current one
    Task(TaskID),

    /// Hardware interrupt
    HardwareInterrupt(u8),

    /// Soft interrupt
    SoftInterrupt(u8),

    /// The message queue
    MessageQueue,
}

impl HandleType {
    /// Determine whether this operation is applicable to this handle type
    pub fn can_apply_op(&self, op: HandleOp) -> bool {
        match self {
            Self::File(_) => op.op_code & OPERATION_FLAG_FILE != 0,
            Self::Task(_) => op.op_code & OPERATION_FLAG_TASK != 0,
            Self::HardwareInterrupt(_) => op.op_code & OPERATION_FLAG_INTERRUPT != 0,
            Self::SoftInterrupt(_) => op.op_code & OPERATION_FLAG_INTERRUPT != 0,
            Self::MessageQueue => op.op_code & OPERATION_FLAG_MESSAGE != 0,
            Self::Socket(_) => op.op_code & OPERATION_FLAG_SOCKET != 0,
        }
    }
}

pub struct OpenHandle {
    pub handle_type: HandleType,
    pub queued_ops: VecDeque<HandleOp>,
}

impl OpenHandle {
    pub fn current_op(&self) -> Option<&HandleOp> {
        self.queued_ops.get(0)
    }

    pub fn complete_current_op(&mut self, result: u32) {
        let current = match self.queued_ops.pop_front() {
            Some(op) => op,
            None => return,
        };
        current.complete(result);
    }
}

pub struct OpenHandles {
    list: SlotList<OpenHandle>,
}

impl OpenHandles {
    pub fn new() -> Self {
        Self {
            list: SlotList::new(),
        }
    }

    fn create_handle(&mut self, handle_type: HandleType) -> Handle {
        let open_handle = OpenHandle {
            handle_type,
            queued_ops: VecDeque::new(),
        };
        let index = self.list.insert(open_handle);
        Handle::new(index)
    }

    pub fn open_file(&mut self) -> Handle {
        panic!("not implemented");
    }

    pub fn open_socket(&mut self, socket: SocketHandle) -> Handle {
        self.create_handle(HandleType::Socket(socket))
    }

    pub fn create_task(&mut self, task: TaskID) -> Handle {
        self.create_handle(HandleType::Task(task))
    }

    pub fn hw_interrupt(&mut self, irq: u8) -> Handle {
        self.create_handle(HandleType::HardwareInterrupt(irq))
    }

    pub fn soft_interrupt(&mut self, irq: u8) -> Handle {
        self.create_handle(HandleType::SoftInterrupt(irq))
    }

    pub fn message_queue(&mut self) -> Handle {
        self.create_handle(HandleType::MessageQueue)
    }

    pub fn get_handle(&self, handle: Handle) -> Option<&OpenHandle> {
        self.list.get(*handle)
    }

    pub fn add_operation(&mut self, handle: Handle, op: HandleOp) -> Result<usize, ()> {
        let handle = self.list.get_mut(*handle).ok_or(())?;
        handle.queued_ops.push_back(op);
        Ok(handle.queued_ops.len())
    }

    pub fn iter(&self) -> impl Iterator<Item = &OpenHandle> {
        self.list.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut OpenHandle> {
        self.list.iter_mut()
    }
}

