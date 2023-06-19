use alloc::vec::Vec;
use spin::RwLock;

use crate::collections::SlotList;
use crate::devices::SyncDriver;
use crate::files::error::IOError;
use crate::task::actions::lifecycle::wait_for_io;
use crate::task::id::TaskID;
use crate::task::switching::get_current_id;

pub static OPEN_KEYBOARD_HANDLES: RwLock<SlotList<OpenHandle>> = RwLock::new(SlotList::new());

pub struct KeyboardDriver {
}

impl KeyboardDriver {
    pub const fn new() -> Self {
        Self {
        }
    }

    pub fn begin_reading(&self, index: u32) -> Result<(), IOError> {
        let mut handles = OPEN_KEYBOARD_HANDLES.write();
        let handle = handles.get_mut(index as usize).ok_or(IOError::FileHandleInvalid)?;
        handle.is_reading = true;
        handle.unread.clear();
        Ok(())
    }

    pub fn end_reading(&self, index: u32) -> Result<(), IOError> {
        let mut handles = OPEN_KEYBOARD_HANDLES.write();
        let handle = handles.get_mut(index as usize).ok_or(IOError::FileHandleInvalid)?;
        handle.is_reading = false;
        Ok(())
    }

    pub fn get_unread_bytes(&self, index: u32, buffer: &mut [u8]) -> Result<usize, IOError> {
        let mut handles = OPEN_KEYBOARD_HANDLES.write();
        let handle = handles.get_mut(index as usize).ok_or(IOError::FileHandleInvalid)?;
        let to_write = handle.unread.len().min(buffer.len());
        for i in 0..to_write {
            buffer[i] = *handle.unread.get(i).unwrap();
        }
        Ok(to_write)
    }
}

impl SyncDriver for KeyboardDriver {
    fn open(&self) -> Result<u32, IOError> {
        let handle = OpenHandle {
            reader_id: get_current_id(),
            is_reading: false,
            unread: Vec::new(),
        };
        let index = OPEN_KEYBOARD_HANDLES.write().insert(handle);
        Ok(index as u32)
    }

    fn read(&self, index: u32, buffer: &mut [u8]) -> Result<u32, IOError> {
        self.begin_reading(index)?;

        let mut bytes_written = 0;
        while bytes_written < buffer.len() {
            wait_for_io(None);
            let written = self.get_unread_bytes(index, &mut buffer[bytes_written..])?;
            bytes_written += written;
        }
        
        self.end_reading(index)?;

        Ok(bytes_written as u32)
    }

    fn write(&self, index: u32, buffer: &[u8]) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn close(&self, index: u32) -> Result<(), IOError> {
        if OPEN_KEYBOARD_HANDLES.write().remove(index as usize).is_none() {
            Err(IOError::FileHandleInvalid)
        } else {
            Ok(())
        }
    }
}

pub struct OpenHandle {
    pub reader_id: TaskID,
    pub is_reading: bool,
    pub unread: Vec<u8>,
}
