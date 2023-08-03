#![no_std]
#![no_main]

extern crate alloc;
extern crate idos_api;
extern crate idos_sdk;

use idos_api::syscall::exec::{read_message_blocking, send_message};
use idos_sdk::driver::{AsyncDriver, IOError};

#[no_mangle]
pub extern fn main() {
    let mut driver_impl = TestDriver::new();

    let mut z = alloc::vec::Vec::new();
    for i in 0..5 {
        z.push(i);
    }

    loop {
        let message_read = read_message_blocking(None);
        if let Some((sender, message)) = message_read {
            match driver_impl.handle_request(message) {
                Some(response) => send_message(sender, response, 0xffffffff),
                None => continue,
            }
        }
    }
}

pub struct TestDriver {
    next_handle: u32,
}

impl TestDriver {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
        }
    }
}

impl AsyncDriver for TestDriver {
    fn open(&mut self, path: &str) -> Result<u32, IOError> {
        let handle = self.next_handle;
        self.next_handle += 1;
        Ok(handle)
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> Result<u32, IOError> {
        let bytes = b"test";
        let len = buffer.len().min(bytes.len());
        buffer[..len].copy_from_slice(&bytes[..len]);
        Ok(len as u32)
    }

    fn write(&mut self, instance: u32, buffer: &[u8]) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn close(&mut self, handle: u32) -> Result<(), IOError> {
        Ok(())
    }
}

