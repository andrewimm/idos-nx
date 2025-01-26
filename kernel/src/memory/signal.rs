use alloc::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};

use super::address::VirtualAddress;

/// A signal can be used to share the status of a procedure between multiple
/// Tasks. It contains an application-specific bitfield; a Task will know the
/// signal is complete when the value is non-zero.
pub struct Signal(Box<AtomicU32>);

impl Signal {
    pub fn new() -> Self {
        Self(Box::new(AtomicU32::new(0)))
    }

    pub fn get_address(&self) -> VirtualAddress {
        VirtualAddress::new(self.0.as_ptr() as u32)
    }

    /// Set the value of a Signal at the given address, and mark it as complete.
    pub fn complete(addr: VirtualAddress, flags: u32) -> u32 {
        let atomic = unsafe { AtomicU32::from_ptr(addr.as_ptr_mut::<u32>()) };
        atomic.swap(flags | 1, Ordering::SeqCst)
    }

    pub fn is_complete(&self) -> bool {
        self.0.load(Ordering::SeqCst) != 0
    }

    pub fn get_value(self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

impl Drop for Signal {
    fn drop(&mut self) {
        if !self.is_complete() {
            crate::kprintln!("WARN: An incomplete signal was dropped");
        }
    }
}

impl core::fmt::Debug for Signal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_complete() {
            let value = self.0.load(Ordering::SeqCst);
            f.write_fmt(format_args!("Signal ({})", value))
        } else {
            f.write_str("Signal (Pending)")
        }
    }
}
