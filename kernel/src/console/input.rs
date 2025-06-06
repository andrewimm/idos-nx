use crate::hardware::ps2::keycodes::{KeyCode, US_LAYOUT};

pub enum KeyAction {
    Press(u8),
    Release(u8),
}

impl KeyAction {
    pub fn from_raw(action_code: u8, key_code: u8) -> Option<Self> {
        match action_code {
            1 => Some(KeyAction::Press(key_code)),
            2 => Some(KeyAction::Release(key_code)),
            _ => None,
        }
    }
}

pub struct KeyState {
    pub shift: bool,
}

impl KeyState {
    pub fn new() -> KeyState {
        Self { shift: false }
    }

    pub fn process_key_action(&mut self, action: KeyAction, buffer: &mut [u8]) -> Option<usize> {
        match action {
            KeyAction::Press(code) => {
                if code == KeyCode::Shift as u8 {
                    self.shift = true;
                    None
                } else {
                    let len = self.key_code_to_ascii(code, buffer);
                    Some(len)
                }
            }
            KeyAction::Release(code) => {
                if code == KeyCode::Shift as u8 {
                    self.shift = false;
                }
                None
            }
        }
    }

    pub fn key_code_to_ascii(&self, code: u8, buffer: &mut [u8]) -> usize {
        match code {
            // handle non-printable keys here
            _ => {
                let index = code as usize;
                let (normal, shifted) = if index < 0x60 {
                    US_LAYOUT[index]
                } else {
                    (0, 0)
                };
                buffer[0] = if self.shift { shifted } else { normal };
                1
            }
        }
    }
}
