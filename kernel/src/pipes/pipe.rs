use core::sync::atomic::{AtomicU32, Ordering};

use alloc::{vec::Vec, boxed::Box, sync::Arc};

use crate::collections::RingBuffer;
use crate::task::id::TaskID;

const DEFAULT_SIZE: usize = 256;

pub struct Pipe<'buffer> {
    ring_buffer: Arc<RingBuffer<'buffer, u8>>,
    buffer_raw: *mut [u8],
    blocked_reader: Option<TaskID>,
    readers: AtomicU32,
    writers: AtomicU32,
}

impl<'buffer> Pipe<'buffer> {
    pub fn new() -> Self {
        Self::with_size(DEFAULT_SIZE)
    }

    pub fn with_size(size: usize) -> Self {
        let actual_size = size + 1;
        let mut heap_buffer = Vec::with_capacity(actual_size);
        for _ in 0..actual_size {
            heap_buffer.push(0);
        }
        let buffer_slice = heap_buffer.into_boxed_slice();
        let buffer_raw: *mut [u8] = Box::into_raw(buffer_slice);
        let rb = unsafe {
            RingBuffer::for_buffer(&*buffer_raw)
        };

        Self {
            ring_buffer: Arc::new(rb),
            buffer_raw,
            blocked_reader: None,
            readers: AtomicU32::new(1),
            writers: AtomicU32::new(1),
        }
    }

    pub fn get_ring_buffer(&self) -> Arc<RingBuffer<'buffer, u8>> {
        self.ring_buffer.clone()
    } 

    pub fn set_blocked_reader(&mut self, task: TaskID) {
        self.blocked_reader = Some(task);
    }

    pub fn clear_blocked_reader(&mut self) {
        self.blocked_reader = None;
    }

    pub fn get_blocked_reader(&self) -> Option<TaskID> {
        self.blocked_reader
    }

    pub fn remove_reader(&self) -> u32 {
        self.readers.fetch_sub(1, Ordering::SeqCst)
    }

    pub fn remove_writer(&self) -> u32 {
        self.writers.fetch_sub(1, Ordering::SeqCst)
    }
}

impl<'buffer> Drop for Pipe<'buffer> {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.buffer_raw);
        }
    }
}

unsafe impl Sync for Pipe<'_> {}
unsafe impl Send for Pipe<'_> {}

#[cfg(test)]
mod tests {
    use super::Pipe;

    #[test_case]
    fn pipes() {
        let pipe = Pipe::with_size(4);
        let rb = pipe.get_ring_buffer();
        assert_eq!(rb.read(), None);
        assert!(rb.write(1));
        assert!(rb.write(2));
        assert!(rb.write(3));
        assert!(rb.write(4));
        assert!(!rb.write(5));

        assert_eq!(rb.read(), Some(1));
        assert_eq!(rb.read(), Some(2));
        assert_eq!(rb.read(), Some(3));

        assert!(rb.write(5));
        assert!(rb.write(6));

        assert_eq!(rb.read(), Some(4));
        assert_eq!(rb.read(), Some(5));
        assert_eq!(rb.read(), Some(6));
        assert_eq!(rb.read(), None);
    }
}

