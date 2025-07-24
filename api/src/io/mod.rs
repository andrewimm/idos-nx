use core::sync::atomic::{AtomicU32, Ordering};

pub mod error;
pub mod file;
pub mod handle;
pub mod message;
pub mod sync;

pub use handle::Handle;
pub use message::Message;

pub const ASYNC_OP_OPEN: u32 = 1;
pub const ASYNC_OP_READ: u32 = 2;
pub const ASYNC_OP_WRITE: u32 = 3;
pub const ASYNC_OP_CLOSE: u32 = 4;
pub const ASYNC_OP_TRANSFER: u32 = 5;

pub const FILE_OP_STAT: u32 = 0x10;

/// All async operations on handles are performed by passing an AsyncOp object
/// to the kernel. The fields are used to determine which action to take.
#[repr(C)]
pub struct AsyncOp {
    /// A field containing a type flag and an operation identifier
    pub op_code: u32,
    /// Atomic u32 that is used to indicate when the operation is complete
    pub signal: AtomicU32,
    pub return_value: AtomicU32,
    pub args: [u32; 3],
}

impl AsyncOp {
    pub fn new(op_code: u32, arg0: u32, arg1: u32, arg2: u32) -> Self {
        Self {
            op_code,
            signal: AtomicU32::new(0),
            return_value: AtomicU32::new(0),
            args: [arg0, arg1, arg2],
        }
    }

    pub fn is_complete(&self) -> bool {
        self.signal.load(Ordering::SeqCst) != 0
    }

    pub fn wait_for_completion(&self) {
        let current_signal = self.signal.load(Ordering::SeqCst);
        if current_signal != 0 {
            //futex_wait(self.signal_address(), current_signal);
        }
    }

    pub fn signal_address(&self) -> u32 {
        self.signal.as_ptr() as u32
    }
}

pub fn read_op(buffer: &mut [u8], offset: u32) -> AsyncOp {
    let buffer_ptr = buffer.as_ptr() as u32;
    let buffer_len = buffer.len() as u32;
    AsyncOp {
        op_code: ASYNC_OP_READ,
        signal: AtomicU32::new(0),
        return_value: AtomicU32::new(0),
        args: [buffer_ptr, buffer_len, offset],
    }
}

pub fn read_message_op(message: &mut Message) -> AsyncOp {
    let message_ptr = message as *mut Message as u32;
    let message_len = core::mem::size_of::<Message>() as u32;
    AsyncOp {
        op_code: ASYNC_OP_READ,
        signal: AtomicU32::new(0),
        return_value: AtomicU32::new(0),
        args: [message_ptr, message_len, 0],
    }
}
