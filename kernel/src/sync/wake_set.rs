use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::memory::address::VirtualAddress;

use super::futex::{futex_wait, futex_wake};

pub struct WakeSet {
    wake_signal: AtomicU32,
    ready_queue: Mutex<VecDeque<u32>>,
}

impl WakeSet {
    pub fn new() -> Self {
        Self {
            wake_signal: AtomicU32::new(0),
            ready_queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn wake(&self, io_handle: u32) {
        self.ready_queue.lock().push_back(io_handle);
        let _ = self.wake_signal.fetch_add(1, Ordering::SeqCst);
        futex_wake(VirtualAddress::new(self.wake_signal.as_ptr() as u32), 1);
    }

    // Warning: Don't call this if you're holding the current task lock
    pub fn wait(&self, timeout: Option<u32>) -> u32 {
        // block while the signal is still zero
        futex_wait(
            VirtualAddress::new(self.wake_signal.as_ptr() as u32),
            0,
            timeout,
        );
        // TODO: make this critical section
        let prev = self.wake_signal.fetch_sub(1, Ordering::SeqCst);
        if prev == 0 {
            self.wake_signal.store(0, Ordering::SeqCst);
        }
        self.ready_queue.lock().pop_front().unwrap_or(0xffff_ffff)
    }

    // Warning: Don't call this if you're holding the current task lock
    pub fn wait_batch(&self, timeout: Option<u32>, buffer: &mut [u32]) -> usize {
        // block while the signal is still zero
        futex_wait(
            VirtualAddress::new(self.wake_signal.as_ptr() as u32),
            0,
            timeout,
        );
        let mut queue = self.ready_queue.lock();
        let count = queue.len().min(buffer.len());
        for i in 0..count {
            buffer[i] = queue.pop_front().unwrap();
        }
        // Reset the signal counter to match what we drained
        let prev = self.wake_signal.fetch_sub(count as u32, Ordering::SeqCst);
        if (prev as usize) < count {
            self.wake_signal.store(0, Ordering::SeqCst);
        }
        count
    }
}
