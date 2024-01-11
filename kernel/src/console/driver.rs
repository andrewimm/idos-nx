use alloc::boxed::Box;
use alloc::string::String;
use spin::RwLock;

use crate::collections::SlotList;
use crate::files::path::Path;
use crate::io::driver::comms::IOResult;
use crate::io::driver::kernel_driver::KernelDriver;
use crate::io::IOError;
use crate::io::filesystem::driver::AsyncIOCallback;
use crate::task::actions::yield_coop;

use super::buffers::ConsoleBuffers;
use super::wake_console_manager;

pub struct ConsoleDriver {
    open_instances: RwLock<SlotList<OpenInstance>>,
    index: usize,
}

impl ConsoleDriver {
    pub fn new(index: usize) -> Self {
        Self {
            open_instances: RwLock::new(SlotList::new()),
            index,
        }
    }

    fn read_impl(&self, instance: u32, buffer: &mut [u8]) -> IOResult {
        let instance = self.open_instances.read().get(instance as usize).ok_or(IOError::FileHandleInvalid)?;
        let mut bytes_written = 0;
        let input_buffer = loop {
            if let Some(buffers) = super::IO_BUFFERS.try_read() {
                break buffers.get(self.index).unwrap().input_buffer.clone();
            }
            yield_coop();
        };
        while bytes_written < buffer.len() {
            match input_buffer.read() {
                Some(ch) => {
                    buffer[bytes_written] = ch;
                    bytes_written += 1;
                },
                None => {
                    if bytes_written == 0 {
                        // TODO: this should sleep, and wake when something is written
                        yield_coop();
                    } else {
                        break;
                    }
                },
            }
        }
        Ok(bytes_written as u32)
    }

    fn write_impl(&self, instance: u32, buffer: &[u8]) -> IOResult {
        let instance = self.open_instances.read().get(instance as usize).ok_or(IOError::FileHandleInvalid)?;

        let mut bytes_written = 0;
        let output_buffer = loop {
            if let Some(buffers) = super::IO_BUFFERS.try_read() {
                break buffers.get(self.index).unwrap().output_buffer.clone();
            }
            yield_coop();
        };
        let mut i = 0;
        while i < buffer.len() {
            if output_buffer.write(buffer[i]) {
                bytes_written += 1;
                i += 1;
            } else {
                yield_coop();
            }
        }
        wake_console_manager();
        Ok(bytes_written)
    }
}

impl KernelDriver for ConsoleDriver {
    fn open(&self, _path: Option<Path>, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        let instance = OpenInstance {};
        Some(Ok(self.open_instances.write().insert(instance) as u32))
    }

    fn read(&self, instance: u32, buffer: &mut [u8], _io_callback: AsyncIOCallback) -> Option<IOResult> {
        Some(self.read_impl(instance, buffer))
    }

    fn write(&self, instance: u32, buffer: &[u8], _io_callback: AsyncIOCallback) -> Option<IOResult> {
        Some(self.write_impl(instance, buffer))
    }

    fn close(&self, instance: u32, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        if self.open_instances.write().remove(instance as usize).is_none() {
            return Some(Err(IOError::FileHandleInvalid));
        }
        Some(Ok(1))
    }
}

#[derive(Copy, Clone)]
struct OpenInstance {
}

pub fn create_new_console() -> (Box<ConsoleDriver>, String) {
    loop {
        if let Some(mut buffers) = super::IO_BUFFERS.try_write() {
            let index = buffers.len();
            buffers.push(ConsoleBuffers::new());
            let name = alloc::format!("CON{}", index + 1);
            let driver = Box::new(ConsoleDriver::new(index));
            
            return (driver, name);
        }
        yield_coop();
    }
}
