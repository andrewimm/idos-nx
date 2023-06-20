use crate::hardware::ps2::keyboard::KeyAction;
use crate::memory::address::VirtualAddress;

use super::console::{Console, Color, ColorCode, TextCell};
use super::input::KeyState;

pub struct ConsoleManager {
    key_state: KeyState,
    text_buffer_base: VirtualAddress,

    // eventually make this an array of consoles
    pub console: Console,
}

impl ConsoleManager {
    pub fn new(text_buffer_base: VirtualAddress) -> Self {
        Self {
            key_state: KeyState::new(),
            text_buffer_base,

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

    pub fn render_top_bar(&self) {
        let width = 80;
        let top_slice = &mut self.console.get_text_buffer()[..width];
        let title = " IDOS-NX ".as_bytes();
        for i in 0..title.len() {
            top_slice[i].glyph = title[i];
            top_slice[i].color = ColorCode::new(Color::White, Color::Blue);
        }
        for i in title.len()..width {
            top_slice[i] = TextCell {
                glyph: 0xcd,
                color: ColorCode::new(Color::White, Color::Black),
            };
        }
    }

    pub fn clear_screen(&self) {
        let width = 80;
        let height = 25;

        for i in 0..(width * height) {
            unsafe {
                let ptr = self.text_buffer_base.as_ptr_mut::<TextCell>().add(i);
                *ptr = TextCell { glyph: 0x20, color: ColorCode::new(Color::LightGrey, Color::Black) };
            }
        }
    }
}
