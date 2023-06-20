use crate::hardware::ps2::{keyboard::KeyAction, keycodes::{KeyCode, US_LAYOUT}};

pub struct KeyState {
    pub shift: bool,
}

impl KeyState {
    pub fn new() -> KeyState {
        Self {
            shift: false,
        }
    }

    pub fn process_key_action(&mut self, action: KeyAction, buffer: &mut [u8]) -> Option<usize> {
        match action {
            KeyAction::Press(code) => {
                 match code {
                    KeyCode::Shift => {
                        self.shift = true;
                        None
                    },
                    _ => {
                        let len = self.key_code_to_ascii(code, buffer);
                        Some(len)
                    },
                 }
            },
            KeyAction::Release(code) => {
                match code {
                    KeyCode::Shift => self.shift = false,
                    _ => (),
                }
                None
            },
        }
    }

    pub fn key_code_to_ascii(&self, code: KeyCode, buffer: &mut [u8]) -> usize {
        match code {
            // handle non-printable keys here

            _ => {
                let index = code as usize;
                let (normal, shifted) = if index < 0x60 {
                    US_LAYOUT[index]
                } else {
                    (0, 0)
                };
                buffer[0] = if self.shift {
                    shifted
                } else {
                    normal
                };
                1
            },
        }
    }
}
