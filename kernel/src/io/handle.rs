use alloc::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::task::actions::yield_coop;
use crate::task::handle::{AsyncOp, Handle};
use crate::task::actions::handle::add_handle_op;

pub struct PendingHandleOp {
    signal: Box<AtomicU32>
}

impl PendingHandleOp {
    pub fn new(handle: Handle, op_code: u32, arg0: u32, arg1: u32, arg2: u32) -> Self {
        let signal = Box::new(AtomicU32::new(0));

        let signal_ptr = signal.as_mut_ptr();
        let op = AsyncOp::new(op_code, signal_ptr as u32, arg0, arg1, arg2);
        add_handle_op(handle, op);

        Self {
            signal,
        }
    }

    pub fn wait_for_completion(&self) -> u32 {
        loop {
            let res = self.signal.load(Ordering::SeqCst);
            if res != 0 {
                return res;
            }
            yield_coop();
        }
    }
}

