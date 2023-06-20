use crate::hardware::ps2::keyboard::KeyAction;
use crate::memory::address::VirtualAddress;

use super::console::Console;
use super::input::KeyState;

pub struct ConsoleManager {
    key_state: KeyState,

    // eventually make this an array of consoles
    console: Console,
}

impl ConsoleManager {
    pub fn new(text_buffer_base: VirtualAddress) -> Self {
        Self {
            key_state: KeyState::new(),

            console: Console::new(text_buffer_base),
        }
    }

    pub fn handle_action(&mut self, action: KeyAction) {
        let mut input_buffer: [u8; 4] = [0; 4];
        let result = self.key_state.process_key_action(action, &mut input_buffer);
        if let Some(len) = result {
            // send input buffer to current console
            self.console.send_input(&input_buffer[..len]);
        }
    }
}
