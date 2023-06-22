use alloc::vec::Vec;

use crate::arch::port::Port;
use crate::hardware::ps2::keyboard::KeyAction;
use crate::memory::address::VirtualAddress;
use crate::task::actions::yield_coop;
use crate::time::system::Timestamp;

use super::console::{Console, Color, ColorCode, TextCell};
use super::input::KeyState;

pub struct ConsoleManager {
    key_state: KeyState,
    text_buffer_base: VirtualAddress,
    current_time: Timestamp,

    current_console: usize,
    consoles: Vec<Console>,
}

impl ConsoleManager {
    pub fn new(text_buffer_base: VirtualAddress) -> Self {
        let first_console = Console::new(text_buffer_base);
        let mut consoles = Vec::with_capacity(1);
        consoles.push(first_console);

        Self {
            key_state: KeyState::new(),
            text_buffer_base,
            current_time: crate::time::system::get_system_time().to_timestamp(),

            current_console: 0,
            consoles,
        }
    }

    pub fn handle_action(&mut self, action: KeyAction) {
        let mut input_buffer: [u8; 4] = [0; 4];
        let result = self.key_state.process_key_action(action, &mut input_buffer);
        if let Some(len) = result {
            // send input buffer to current console
            self.consoles.get_mut(self.current_console).unwrap().send_input(&input_buffer[..len]);
        }
    }

    pub fn process_buffers(&mut self) {
        for index in 0..self.consoles.len() {
            let console = self.consoles.get_mut(index).unwrap();
            let output_buffer = loop {
                if let Some(buffers) = super::IO_BUFFERS.try_read() {
                    break buffers.get(index).unwrap().output_buffer.clone();
                }
                yield_coop();
            };
            loop {
                match output_buffer.read() {
                    Some(value) => {
                        console.write_character(value);
                    },
                    None => break,
                }
            }
        }
    }

    pub fn update_cursor(&self) {
        let cursor_offset = self.consoles.get(self.current_console).unwrap().get_cursor_offset();
        let register = Port::new(0x3d4);
        let register_value = Port::new(0x3d5);

        register.write_u8(0x0f);
        register_value.write_u8(cursor_offset as u8);
        register.write_u8(0x0e);
        register_value.write_u8((cursor_offset >> 8) as u8);
    }

    pub fn render_top_bar(&self) {
        let width = 80;
        let top_slice = unsafe {
            core::slice::from_raw_parts_mut(
                self.text_buffer_base.as_ptr_mut::<TextCell>(),
                width,
            )
        };
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

        self.print_time();
    }

    pub fn update_clock(&mut self) {
        let current_time = crate::time::system::get_system_time().to_timestamp();
        if self.current_time.total_minutes() != current_time.total_minutes() {
            self.current_time = current_time;
            self.print_time();
        }
    }

    pub fn print_time(&self) {
        let width = 80;
        let mut clock_buffer: [u8; 7] = [0x20; 7];
        self.current_time.to_datetime().time.print_short_to_buffer(&mut clock_buffer[1..6]);
        let clock_color = ColorCode::new(Color::White, Color::Blue);
        let clock_start = width - clock_buffer.len();
        let top_slice = unsafe {
            core::slice::from_raw_parts_mut(
                self.text_buffer_base.as_ptr_mut::<TextCell>().add(clock_start),
                clock_buffer.len()
            )
        };
        for i in 0..clock_buffer.len() {
            top_slice[i] = TextCell {
                glyph: clock_buffer[i],
                color: clock_color,
            }
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
