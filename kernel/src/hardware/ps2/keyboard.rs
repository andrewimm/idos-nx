use alloc::vec::Vec;
use spin::RwLock;

use crate::collections::SlotList;
use crate::devices::SyncDriver;
use crate::io::IOError;
use crate::task::actions::lifecycle::wait_for_io;
use crate::task::id::TaskID;
use crate::task::switching::get_current_id;

use super::keycodes::{get_extended_keycode, get_keycode, KeyCode};

pub static OPEN_KEYBOARD_HANDLES: RwLock<SlotList<OpenHandle>> = RwLock::new(SlotList::new());

pub struct KeyboardDriver {}

impl KeyboardDriver {
    pub const fn new() -> Self {
        Self {}
    }

    pub fn begin_reading(&self, index: u32) -> Result<(), IOError> {
        let mut handles = OPEN_KEYBOARD_HANDLES.write();
        let handle = handles
            .get_mut(index as usize)
            .ok_or(IOError::FileHandleInvalid)?;
        handle.is_reading = true;
        handle.unread.clear();
        Ok(())
    }

    pub fn end_reading(&self, index: u32) -> Result<(), IOError> {
        let mut handles = OPEN_KEYBOARD_HANDLES.write();
        let handle = handles
            .get_mut(index as usize)
            .ok_or(IOError::FileHandleInvalid)?;
        handle.is_reading = false;
        Ok(())
    }

    pub fn get_unread_bytes(&self, index: u32, buffer: &mut [u8]) -> Result<usize, IOError> {
        let mut handles = OPEN_KEYBOARD_HANDLES.write();
        let handle = handles
            .get_mut(index as usize)
            .ok_or(IOError::FileHandleInvalid)?;
        let to_write = handle.unread.len().min(buffer.len());
        for i in 0..to_write {
            buffer[i] = *handle.unread.get(i).unwrap();
        }
        handle.unread.clear();
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

    fn write(&self, _index: u32, _buffer: &[u8]) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn close(&self, index: u32) -> Result<(), IOError> {
        if OPEN_KEYBOARD_HANDLES
            .write()
            .remove(index as usize)
            .is_none()
        {
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

/// State machine tracking raw keyboard scancodes and turning it into useful
/// input data
pub struct KeyboardState {
    receiving_extended_code: bool,
}

impl KeyboardState {
    pub const fn new() -> Self {
        Self {
            receiving_extended_code: false,
        }
    }

    /// Handle a raw stream of scancode bytes from a PS/2 keyboard, one at a
    /// time. Each byte can trigger at most one key action (such as a press or
    /// release), so the method returns an optional KeyAction if one has been
    /// generated.
    pub fn handle_scan_byte(&mut self, scan_code: u8) -> Option<KeyAction> {
        if scan_code == 0xe0 {
            self.receiving_extended_code = true;
            return None;
        }

        let key = scan_code & 0x7f;
        let pressed = scan_code & 0x80 == 0;

        let key_code = if self.receiving_extended_code {
            get_extended_keycode(key)
        } else {
            get_keycode(key)
        };
        self.receiving_extended_code = false;

        match key_code {
            KeyCode::None => None,
            _ => {
                if pressed {
                    Some(KeyAction::Press(key_code))
                } else {
                    Some(KeyAction::Release(key_code))
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum KeyAction {
    Press(KeyCode),
    Release(KeyCode),
}

impl KeyAction {
    pub fn to_raw(&self) -> [u8; 2] {
        match self {
            Self::Press(code) => [1, *code as u8],
            Self::Release(code) => [2, *code as u8],
        }
    }
}
