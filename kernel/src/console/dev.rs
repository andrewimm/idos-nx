use alloc::boxed::Box;
use alloc::string::String;
use spin::RwLock;

use crate::collections::SlotList;
use crate::devices::SyncDriver;
use crate::files::error::IOError;
use crate::task::actions::yield_coop;

use super::buffers::ConsoleBuffers;

pub struct ConsoleDriver {
    open_handles: RwLock<SlotList<OpenHandle>>,
    index: usize,
}

impl ConsoleDriver {
    pub fn new(index: usize) -> Self {
        Self {
            open_handles: RwLock::new(SlotList::new()),
            index,
        }
    }
}

impl SyncDriver for ConsoleDriver {
    fn open(&self) -> Result<u32, IOError> {
        let handle = OpenHandle {};
        Ok(self.open_handles.write().insert(handle) as u32)
    }

    fn read(&self, index: u32, buffer: &mut [u8]) -> Result<u32, IOError> {
        let handle = self.open_handles.read().get(index as usize).ok_or(IOError::FileHandleInvalid)?;

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

    fn write(&self, index: u32, buffer: &[u8]) -> Result<u32, IOError> {
        let handle = self.open_handles.read().get(index as usize).ok_or(IOError::FileHandleInvalid)?;

        let mut bytes_written = 0;
        let output_buffer = loop {
            if let Some(buffers) = super::IO_BUFFERS.try_read() {
                break buffers.get(self.index).unwrap().output_buffer.clone();
            }
            yield_coop();
        };
        for i in 0..buffer.len() {
            if output_buffer.write(buffer[i]) {
                bytes_written += 1;
            } else {
                return Ok(bytes_written);
            }
        }
        Ok(bytes_written)
    }

    fn close(&self, index: u32) -> Result<(), IOError> {
        if self.open_handles.write().remove(index as usize).is_none() {
            return Err(IOError::FileHandleInvalid);
        }
        Ok(())
    }
}

#[derive(Copy, Clone)]
struct OpenHandle {
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

