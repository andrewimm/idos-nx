use alloc::{vec::Vec, sync::Arc};

use crate::collections::RingBuffer;

const SIZE: usize = 512;

pub struct ConsoleBuffers<'buf> {
    raw_buffer_start: *mut u8,
    raw_buffer_size: usize,

    pub input_buffer: Arc<RingBuffer<'buf, u8>>,
    pub output_buffer: Arc<RingBuffer<'buf, u8>>,
}

impl ConsoleBuffers<'_> {
    pub fn new() -> Self {
        let mut raw_buffer_vec = Vec::with_capacity(SIZE);
        for _ in 0..SIZE {
            raw_buffer_vec.push(0);
        }
        let (raw_buffer_start, raw_buffer_size, _) = raw_buffer_vec.into_raw_parts();

        let (input_buffer, output_buffer) = unsafe {
            let output_start = raw_buffer_start.add(SIZE / 2);
            let input_buffer = RingBuffer::for_buffer(
                core::slice::from_raw_parts(raw_buffer_start, SIZE / 2)
            );
            let output_buffer = RingBuffer::for_buffer(
                core::slice::from_raw_parts(output_start, SIZE / 2)
            );
            (input_buffer, output_buffer)
        };

        Self {
            raw_buffer_start,
            raw_buffer_size,

            input_buffer: Arc::new(input_buffer),
            output_buffer: Arc::new(output_buffer),
        }
    }
}

impl Drop for ConsoleBuffers<'_> {
    fn drop(&mut self) {
        let _ = unsafe {
            // make sure the heap memory gets freed
            Vec::from_raw_parts(self.raw_buffer_start, self.raw_buffer_size, self.raw_buffer_size)
        };
    }
}

unsafe impl Sync for ConsoleBuffers<'_> {}
unsafe impl Send for ConsoleBuffers<'_> {}
