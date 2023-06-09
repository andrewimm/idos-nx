use alloc::{vec::Vec, boxed::Box, sync::Arc};

use crate::collections::RingBuffer;

const DEFAULT_SIZE: usize = 256;

pub struct Pipe<'buffer> {
    ring_buffer: Arc<RingBuffer<'buffer, u8>>,
    buffer_raw: *mut [u8],
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
        }
    }

    pub fn get_ring_buffer(&self) -> Arc<RingBuffer<'buffer, u8>> {
        self.ring_buffer.clone()
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

