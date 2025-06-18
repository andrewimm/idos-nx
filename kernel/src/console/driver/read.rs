//! Reading from the console doesn't block until the read buffer is full. It
//! returns when input is flushed. In raw mode, this happens every single time
//! input is received. In cooked mode, it happens when the user hits enter.

use alloc::collections::VecDeque;

use crate::{
    memory::{address::VirtualAddress, shared::release_buffer},
    task::actions::io::driver_io_complete,
};

pub struct PendingRead {
    pub request_id: u32,
    pub buffer_start: *mut u8,
    pub max_length: usize,
}

impl PendingRead {
    pub fn complete(self, flushed_input: &mut VecDeque<u8>) -> usize {
        let write_buffer =
            unsafe { core::slice::from_raw_parts_mut(self.buffer_start, self.max_length) };
        let to_read = self.max_length.min(flushed_input.len());
        let mut written = 0;
        for byte in flushed_input.drain(..to_read) {
            write_buffer[written] = byte;
            written += 1;
        }

        release_buffer(
            VirtualAddress::new(self.buffer_start as u32),
            self.max_length,
        );
        driver_io_complete(self.request_id, Ok(written as u32));

        written
    }
}
