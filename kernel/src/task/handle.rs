use alloc::collections::vec_deque::VecDeque;
use crate::collections::SlotList;
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
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

// Op Codes use the top bits to indicate the handle type they modify
pub const OPERATION_FLAG_FILE: u32 = 0x80000000;
pub const OPERATION_FLAG_TASK: u32 = 0x40000000;
pub const OPERATION_FLAG_INTERRUPT: u32 = 0x20000000;
pub const OPERATION_FLAG_MESSAGE: u32 = 0x10000000;

pub enum HandleType {
    /// A file or device
    File(OpenFile),

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
        }
    }
}

pub struct OpenHandle {
    handle_type: HandleType,
    queued_ops: VecDeque<HandleOp>,
}

pub struct OpenHandles {
    list: SlotList<OpenHandle>,
}

impl OpenHandles {
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
}

