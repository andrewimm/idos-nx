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
    pub ctrl: bool,
    pub shift: bool,
}

impl KeyState {
    pub fn new() -> KeyState {
        Self {
            ctrl: false,
            shift: false,
        }
    }

    pub fn process_key_action(&mut self, action: KeyAction, buffer: &mut [u8]) -> Option<usize> {
        match action {
            KeyAction::Press(code) => {
                if code == KeyCode::Shift as u8 {
                    self.shift = true;
                    None
                } else if code == KeyCode::Control as u8 {
                    self.ctrl = true;
                    None
                } else {
                    let len = self.key_code_to_ascii(code, buffer);
                    if len > 0 {
                        Some(len)
                    } else {
                        None
                    }
                }
            }
            KeyAction::Release(code) => {
                if code == KeyCode::Shift as u8 {
                    self.shift = false;
                } else if code == KeyCode::Control as u8 {
                    self.ctrl = false;
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
                if self.ctrl {
                    if index < 0x60 {
                        // Control characters are in the range 0x00 to 0x1F
                        buffer[0] = (index & 0x1F) as u8; // Convert to control character
                        return 1;
                    } else {
                        // Non-control characters are ignored when Ctrl is pressed
                        return 0;
                    }
                }
                buffer[0] = if self.shift { shifted } else { normal };
                1
            }
        }
    }
}
