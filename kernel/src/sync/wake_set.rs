use core::sync::atomic::{AtomicU32, Ordering};

use crate::memory::address::VirtualAddress;

use super::futex::{futex_wait, futex_wake};

pub struct WakeSet {
    wake_signal: AtomicU32,
}

impl WakeSet {
    pub fn new() -> Self {
        Self {
            wake_signal: AtomicU32::new(0),
        }
    }

    pub fn wake(&self) {
        let _ = self.wake_signal.fetch_add(1, Ordering::SeqCst);
        futex_wake(VirtualAddress::new(self.wake_signal.as_ptr() as u32), 1);
    }

    // Warning: Don't call this if you're holding the current task lock
    pub fn wait(&self) {
        // block while the signal is still zero
        futex_wait(VirtualAddress::new(self.wake_signal.as_ptr() as u32), 0);
        // on wake, it should be assumed the value is > 0
        let _ = self.wake_signal.fetch_sub(1, Ordering::SeqCst);
    }
}
