use core::sync::atomic::{AtomicU32, Ordering};

pub mod error;

/// All async operations on handles are performed by passing an AsyncOp object
/// to the kernel. The fields are used to determine which action to take.
#[repr(C)]
pub struct AsyncOp {
    /// A field containing a type flag and an operation identifier
    pub op_code: u32,
    /// Atomic u32 that is used to indicate when the operation is complete
    pub signal: AtomicU32,
    pub return_value: AtomicU32,
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
}

impl AsyncOp {
    pub fn is_complete(&self) -> bool {
        self.signal.load(Ordering::SeqCst) != 0
    }

    pub fn signal_address(&self) -> u32 {
        self.signal.as_ptr() as u32
    }
}
